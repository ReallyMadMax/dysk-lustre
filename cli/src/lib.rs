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

use lfs_core::Mount;
use rustreapi::LustrePath;
use {
    crate::{
        args::*,
        normal::*,
    },
    clap::Parser,
    std::{
        fs,
        os::unix::fs::MetadataExt,
    },
};
use crate::cols::Cols;

/// Helper function to parse Lustre component names
fn parse_lustre_component(name: &str) -> Option<(String, u32)> {
    // Handle names like "lustre-MDT0000_UUID" or "lustre-OST0001_UUID"
    if let Some(dash_pos) = name.find('-') {
        let after_dash = &name[dash_pos + 1..];
        if let Some(underscore_pos) = after_dash.find('_') {
            let component_part = &after_dash[..underscore_pos];

            // Extract type (MDT/OST) and number
            if component_part.len() >= 7 { // At least "MDT0000" or "OST0000"
                let (comp_type, num_str) = component_part.split_at(3);
                if let Ok(num) = num_str.parse::<u32>() {
                    return Some((comp_type.to_string(), num));
                }
            }
        }
    }
    None
}

#[allow(clippy::match_like_matches_macro)]
pub fn run() {
    let args = Args::parse();
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

    let mut mounts = match lfs_core::read_mounts(&options) {
        Ok(mounts) => mounts,
        Err(e) => {
            eprintln!("Error reading mounts: {}", e);
            return;
        }
    };

    mounts.retain(|m| {
        if m.info.fs_type == "lustre" {
            !is_lustre_server_component(&m.info.mount_point)
        } else {
            true
        }
    });

    let lustre_mounts = LustrePath::discover_lustre_mounts();

    replace_lustre_client_mounts(&mut mounts, &lustre_mounts);

    let mut has_lustre_mounts = false;
    for lustre_mount in lustre_mounts {
        if is_lustre_component_mount(&lustre_mount) {
            mounts.push(lustre_mount);
            has_lustre_mounts = true;
        }
    }

    if !has_lustre_mounts {
        has_lustre_mounts = mounts.iter().any(|m| m.info.fs_type == "lustre");
    }

    if !args.all {
        if has_lustre_mounts {
            mounts.retain(|m| m.info.fs_type == "lustre");

            mounts.sort_by(|a, b| {
                let a_name = &a.info.fs;
                let b_name = &b.info.fs;

                // Client mount (filesystem_summary) should come LAST
                let a_is_client = a_name == "filesystem_summary";
                let b_is_client = b_name == "filesystem_summary";

                match (a_is_client, b_is_client) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (true, true) => a_name.cmp(b_name),
                    (false, false) => {
                        let a_parts = parse_lustre_component(a_name);
                        let b_parts = parse_lustre_component(b_name);

                        match (a_parts, b_parts) {
                            (Some((a_type, a_idx)), Some((b_type, b_idx))) => {
                                // MDTs come before OSTs
                                match (a_type.as_str(), b_type.as_str()) {
                                    ("MDT", "OST") => std::cmp::Ordering::Less,
                                    ("OST", "MDT") => std::cmp::Ordering::Greater,
                                    _ => {
                                        a_idx.cmp(&b_idx)
                                    }
                                }
                            }
                            _ => a_name.cmp(b_name)
                        }
                    }
                }
            });
        } else {
            mounts.retain(is_normal);
            args.sort.sort(&mut mounts);
        }
    } else {
        args.sort.sort(&mut mounts);
    }

    let final_args = if has_lustre_mounts && !args.all && mounts.iter().all(|m| m.info.fs_type == "lustre") {
        if args.cols == Cols::default() {
            let mut modified_args = args.clone();
            modified_args.cols = "fs+used+use+free+size+mp".parse().unwrap();
            modified_args
        } else {
            let cols_str = format!("{:?}", args.cols);
            if cols_str.starts_with('+') || cols_str.starts_with('-') {
                let mut modified_args = args.clone();
                let lustre_base = "fs+used+use+free+size+mp";
                let user_cols = "";
                modified_args.cols = format!("{}+{}", lustre_base, user_cols).parse().unwrap();
                modified_args
            } else {
                args.clone()
            }
        }
    } else {
        args.clone()
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

        let is_lustre = {
            match LustrePath::parse(&path.to_string_lossy()) {
                Ok(_) => true,
                Err(_) => false,
            }
        };

        if is_lustre {
            mounts.retain(|m| m.info.fs_type == "lustre" && path.starts_with(&m.info.mount_point));
        } else {
            mounts.retain(|m| m.info.dev == dev);
        }
    }

    let is_lustre_only_view = has_lustre_mounts && !args.all &&
        mounts.iter().all(|m| m.info.fs_type == "lustre");

    if !is_lustre_only_view {
        final_args.sort.sort(&mut mounts);
    }

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