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
    
    // Read regular mounts
    let mut mounts = match lfs_core::read_mounts(&options) {
        Ok(mounts) => mounts,
        Err(e) => {
            eprintln!("Error reading mounts: {}", e);
            return;
        }
    };

    // Add Lustre mounts if available
    if let Some(path) = &args.path {
        // If specific path is requested, check if it's Lustre
        if lustre_core::is_lustre_path(path) {
            // For Lustre paths, we might want to show Lustre-specific information
            // But still include regular mounts for comparison
            let mut lustre_mounts = lustre_core::discover_lustre_mounts();
            mounts.append(&mut lustre_mounts);
        }
    } else {
        // Add all discovered Lustre mounts to the regular mount list
        let mut lustre_mounts = lustre_core::discover_lustre_mounts();
        mounts.append(&mut lustre_mounts);
    }

    if !args.all {
        mounts.retain(is_normal);
    }
    
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
    
    args.sort.sort(&mut mounts);
    let mounts = match args.filter.clone().unwrap_or_default().filter(&mounts) {
        Ok(mounts) => mounts,
        Err(e) => {
            eprintln!("Error in filter evaluation: {}", e);
            return;
        }
    };
    if args.csv {
        csv::print(&mounts, &args).expect("writing csv failed");
        return;
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json::output_value(&mounts, args.units)).unwrap()
        );
        return;
    }
    if mounts.is_empty() {
        println!("no mount to display - try\n    dysk -a");
        return;
    }
    table::print(&mounts, args.color(), &args);
    csi_reset();
}

/// output a Reset CSI sequence
fn csi_reset(){
    print!("\u{1b}[0m");
}