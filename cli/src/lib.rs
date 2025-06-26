pub mod args;
pub mod col;
pub mod col_expr;
pub mod cols;
pub mod csv;
pub mod filter;
pub mod help;
pub mod json;
pub mod list_cols;
pub mod normal;
pub mod order;
pub mod sorting;
pub mod table;
pub mod units;

// Lustre integration modules - always enabled
pub mod lustre_bindings;
pub mod lustre_core;

use lfs_core::Mount;
use {
    crate::{
        args::*,
        normal::*,
        units::Units,
    },
    clap::Parser,
    std::{
        fs,
        os::unix::fs::MetadataExt,
    },
};
use crate::cols::Cols;

#[allow(clippy::match_like_matches_macro)]
pub fn run() {
    let mut args = Args::parse();
    if args.version {
        println!("dysk {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.help {
        help::print(args.ascii);
        csi_reset();
        return;
    }
    if args.list_cols {
        list_cols::print(args.color(), args.ascii);
        csi_reset();
        return;
    }
    
    let mut options = lfs_core::ReadOptions::default();
    options.remote_stats(args.remote_stats.unwrap_or_else(||true));
    
    // Read regular mounts
    let mut mounts = match lfs_core::read_mounts(&options) {
        Ok(mounts) => mounts,
        Err(e) => {
            eprintln!("Error reading mounts: {}", e);
            return;
        }
    };

    // Filter out Lustre server component mounts (OST/MDT) since we'll replace them with API-based ones
    mounts.retain(|m| {
        if m.info.fs_type == "lustre" {
            // Keep client mounts, filter out server component mounts
            !is_lustre_server_component(&m.info.mount_point)
        } else {
            true
        }
    });

    // Add Lustre API-based mounts
    let mut lustre_mounts = lustre_core::discover_lustre_mounts();
    
    // Replace any remaining Lustre client mounts with our API-enhanced versions
    replace_lustre_client_mounts(&mut mounts, &lustre_mounts);
    
    // Add the individual component mounts
    let mut has_lustre_mounts = false;
    for lustre_mount in lustre_mounts {
        if is_lustre_component_mount(&lustre_mount) {
            mounts.push(lustre_mount);
            has_lustre_mounts = true;
        }
    }

    // Check if we have any Lustre mounts at all (including client mounts)
    if !has_lustre_mounts {
        has_lustre_mounts = mounts.iter().any(|m| m.info.fs_type == "lustre");
    }

    if !args.all {
        if has_lustre_mounts {
            // If Lustre is detected, show only Lustre-related mounts by default
            mounts.retain(|m| m.info.fs_type == "lustre");
            
            // Sort Lustre mounts alphabetically/numerically by filesystem name
            mounts.sort_by(|a, b| {
                // Extract the component type and number for proper ordering
                let a_name = &a.info.fs;
                let b_name = &b.info.fs;
                
                // Handle client mount (should come first)
                if a_name.contains("@lustre") && !b_name.contains("@lustre") {
                    return std::cmp::Ordering::Less;
                }
                if !a_name.contains("@lustre") && b_name.contains("@lustre") {
                    return std::cmp::Ordering::Greater;
                }
                
                // Handle MDT vs OST ordering (MDTs first, then OSTs)
                let a_is_mdt = a_name.contains("MDT");
                let b_is_mdt = b_name.contains("MDT");
                let a_is_ost = a_name.contains("OST");
                let b_is_ost = b_name.contains("OST");
                
                match (a_is_mdt, a_is_ost, b_is_mdt, b_is_ost) {
                    (true, _, false, true) => std::cmp::Ordering::Less,  // MDT before OST
                    (false, true, true, _) => std::cmp::Ordering::Greater, // OST after MDT
                    _ => {
                        // Same type or client, sort by name/number
                        a_name.cmp(b_name)
                    }
                }
            });
        } else {
            // No Lustre detected, use normal filtering and sorting
            mounts.retain(is_normal);
            args.sort.sort(&mut mounts);
        }
    } else {
        // Show all mounts, use regular sorting
        args.sort.sort(&mut mounts);
    }
    
    // Modify columns for Lustre-only display  
    let final_args = if has_lustre_mounts && !args.all && mounts.iter().all(|m| m.info.fs_type == "lustre") {
        // We're showing only Lustre mounts, use optimized column set
        if args.cols == Cols::default() {
            // Only modify if using default columns - remove redundant type and disk columns
            let mut modified_args = args.clone();
            modified_args.cols = "fs+used+use+free+size+mp".parse().unwrap();
            modified_args
        } else {
            args
        }
    } else {
        args
    };
    
    if let Some(path) = &args.path {
        let md = match fs::metadata(path) {
            Ok(md) => md,
            Err(e) => {
                eprintln!("Can't read {:?} : {}", path, e);
                return;
            }
        };
        let dev = lfs_core::DeviceId::from(md.dev());
        
        // For Lustre filesystems, we need special handling since they don't have traditional device IDs
        if lustre_core::is_lustre_path(path) {
            // Filter to only show Lustre mounts that match this path
            mounts.retain(|m| m.info.fs_type == "lustre" && path.starts_with(&m.info.mount_point));
        } else {
            mounts.retain(|m| m.info.dev == dev);
        }
    }
    
    final_args.sort.sort(&mut mounts);
    let mounts = match final_args.filter.clone().unwrap_or_default().filter(&mounts) {
        Ok(mounts) => mounts,
        Err(e) => {
            eprintln!("Error in filter evaluation: {}", e);
            return;
        }
    };
    if final_args.csv {
        csv::print(&mounts, &final_args).expect("writing csv failed");
        return;
    }
    if final_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json::output_value(&mounts, final_args.units)).unwrap()
        );
        return;
    }
    if mounts.is_empty() {
        println!("no mount to display - try\n    dysk -a");
        return;
    }
    table::print(&mounts, final_args.color(), &final_args);
    csi_reset();
}

/// Check if a mount point looks like a Lustre server component
fn is_lustre_server_component(mount_point: &std::path::Path) -> bool {
    let path_str = mount_point.to_string_lossy();
    // Common patterns for Lustre server mounts
    path_str.contains("-ost") || 
    path_str.contains("-mdt") || 
    path_str.contains("-mds") ||
    path_str.contains("ost") && (path_str.contains("lustre") || path_str.contains("scratch")) ||
    path_str.contains("mdt") && (path_str.contains("lustre") || path_str.contains("scratch"))
}

/// Check if this is one of our component mounts (has [MDT:] or [OST:] in the path)
fn is_lustre_component_mount(mount: &Mount) -> bool {
    let path_str = mount.info.mount_point.to_string_lossy();
    path_str.contains("[MDT:") || path_str.contains("[OST:")
}

/// Replace Lustre client mounts with API-enhanced versions that have better stats
fn replace_lustre_client_mounts(mounts: &mut Vec<Mount>, lustre_mounts: &[Mount]) {
    for lustre_mount in lustre_mounts {
        if !is_lustre_component_mount(lustre_mount) {
            // This is a client mount, find and replace the corresponding regular mount
            if let Some(pos) = mounts.iter().position(|m| {
                m.info.fs_type == "lustre" && 
                m.info.mount_point == lustre_mount.info.mount_point
            }) {
                mounts[pos] = lustre_mount.clone();
            } else {
                // No existing mount found, add this one
                mounts.push(lustre_mount.clone());
            }
        }
    }
}

/// output a Reset CSI sequence
fn csi_reset(){
    print!("\u{1b}[0m");
}