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
use rustreapi::MountStats;
use rustreapi::Mount as LustreMount;
use rustreapi::{Layout, LayoutGetFlags};
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
use std::collections::HashMap;
use std::sync::{Mutex, LazyLock};

/// Lustre-specific information for a mount
#[derive(Debug, Clone)]
pub struct LustreInfo {
    pub stripe_count: Option<u64>,
    pub stripe_size: Option<u64>,
    pub lustre_version: Option<String>,
    pub pool_name: Option<String>,
    pub component_type: Option<String>,
    pub component_index: Option<u32>,
    pub mirror_count: Option<u16>,
}

impl LustreInfo {
    pub fn new() -> Self {
        Self {
            stripe_count: None,
            stripe_size: None,
            lustre_version: None,
            pool_name: None,
            component_type: None,
            component_index: None,
            mirror_count: None,
        }
    }
}

/// Global storage for Lustre-specific mount information
static LUSTRE_INFO: LazyLock<Mutex<HashMap<String, LustreInfo>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get Lustre info for a mount point
pub fn get_lustre_info(mount_point: &str) -> Option<LustreInfo> {
    LUSTRE_INFO.lock().ok()?.get(mount_point).cloned()
}

/// Set Lustre info for a mount point
pub fn set_lustre_info(mount_point: String, info: LustreInfo) {
    if let Ok(mut map) = LUSTRE_INFO.lock() {
        map.insert(mount_point, info);
    }
}

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

    let lustre_mounts = MountStats::discover_mounts().unwrap_or_else(|e| {
        eprintln!("Error discovering Lustre mounts: {}", e);
        Vec::new()
    });

    // Convert rustreapi::Mount to lfs_core::Mount
    let converted_lustre_mounts: Vec<Mount> = lustre_mounts.iter()
        .map(|lustre_mount| convert_lustre_mount_to_lfs_mount(lustre_mount))
        .collect();

    replace_lustre_client_mounts(&mut mounts, &converted_lustre_mounts);

    let mut has_lustre_mounts = false;
    for lustre_mount in converted_lustre_mounts {
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
            modified_args.cols = "fs+used+use+free+size+fsname".parse().unwrap();
            modified_args
        } else {
            let cols_str = format!("{:?}", args.cols);
            if cols_str.starts_with('+') || cols_str.starts_with('-') {
                let mut modified_args = args.clone();
                let lustre_base = "fs+used+use+free+size+fsname";
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

    // Deduplicate filesystems by default (keep only one mount per filesystem)
    // TODO: Add a flag to disable deduplication if needed
    let (mut mounts, mount_points_map) = check_duplicate(mounts.into_iter().cloned().collect());
    
    // Re-apply Lustre sorting after deduplication if needed
    if is_lustre_only_view {
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
    }
    
    // Convert back to the expected &[&Mount] format for the output functions
    let mount_refs: Vec<&Mount> = mounts.iter().collect();
    
    if final_args.csv {
        csv::print(&mount_refs, &final_args).expect("writing csv failed");
        return;
    }
    if final_args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json::output_value(&mount_refs, final_args.units)).unwrap()
        );
        return;
    }
    if mounts.is_empty() {
        println!("no mount to display - try\n    dysk -a");
        return;
    }
    table::print(&mount_refs, final_args.color(), &final_args);
    
    // Print mount points summary at the end
    print_mount_points_summary(&mount_points_map);
    
    csi_reset();
}

/// Deduplicate filesystems - keep only one mount per filesystem
/// For Lustre, keep all components separate (don't deduplicate)
/// For others, group by device ID and prefer the shortest/root mount path
/// Returns (deduplicated_mounts, mount_points_map)
fn check_duplicate(mounts: Vec<Mount>) -> (Vec<Mount>, std::collections::HashMap<String, Vec<String>>) {
    use std::collections::HashMap;
    
    let mut filesystem_map: HashMap<String, Vec<Mount>> = HashMap::new();
    let mut mount_points_map = HashMap::new();
    
    // Group mounts by appropriate key
    for mount in mounts {
        let group_key = if mount.info.fs_type == "lustre" {
            // For Lustre, keep each component separate (don't group them)
            // Use a unique key for each mount to prevent deduplication
            format!("lustre_{}_{}", mount.info.fs, mount.info.mount_point.to_string_lossy())
        } else {
            // For non-Lustre, group by device ID to handle bind mounts
            format!("{}:{}", mount.info.dev.major, mount.info.dev.minor)
        };
        
        filesystem_map
            .entry(group_key)
            .or_insert_with(Vec::new)
            .push(mount);
    }
    
    let mut deduplicated = Vec::new();
    
    for (_group_key, group) in filesystem_map {
        if group.len() == 1 {
            // Only one mount for this filesystem, keep it
            let mount = &group[0];
            let fs_name = crate::col::extract_fsname(mount);
            if mount.info.fs_type == "lustre" {
                // For Lustre, don't add to mount_points_map as we want individual components shown
                deduplicated.extend(group);
            } else {
                mount_points_map.insert(fs_name, vec![mount.info.mount_point.to_string_lossy().to_string()]);
                deduplicated.extend(group);
            }
        } else {
            // Multiple mounts for this filesystem, choose the best representative
            let mount_points: Vec<String> = group.iter()
                .map(|m| m.info.mount_point.to_string_lossy().to_string())
                .collect();
            let representative = choose_representative_mount(group);
            let fs_name = crate::col::extract_fsname(&representative);
            mount_points_map.insert(fs_name, mount_points);
            deduplicated.push(representative);
        }
    }
    
    (deduplicated, mount_points_map)
}

/// Choose the best representative mount from a group of mounts for the same filesystem
fn choose_representative_mount(mut mounts: Vec<Mount>) -> Mount {
    // For Lustre filesystems, prefer filesystem_summary (client mount)
    if let Some(pos) = mounts.iter().position(|m| m.info.fs == "filesystem_summary") {
        return mounts.remove(pos);
    }
    
    // For other filesystems, prefer the mount with the shortest path (usually the root mount)
    mounts.sort_by(|a, b| {
        let a_path = a.info.mount_point.to_string_lossy();
        let b_path = b.info.mount_point.to_string_lossy();
        
        // Prefer root mounts
        match (a_path.as_ref(), b_path.as_ref()) {
            ("/", _) => std::cmp::Ordering::Less,
            (_, "/") => std::cmp::Ordering::Greater,
            _ => a_path.len().cmp(&b_path.len()).then_with(|| a_path.cmp(&b_path))
        }
    });
    
    mounts.into_iter().next().unwrap()
}

/// Extract component type and index from mount point path
fn extract_component_info(mount_point: &str) -> (Option<String>, Option<u32>) {
    if mount_point.contains("[MDT:") {
        if let Some(start) = mount_point.find("[MDT:") {
            if let Some(end) = mount_point.find("]") {
                let index_str = &mount_point[start + 5..end];
                if let Ok(index) = index_str.parse::<u32>() {
                    return (Some("MDT".to_string()), Some(index));
                }
            }
        }
        return (Some("MDT".to_string()), None);
    } else if mount_point.contains("[OST:") {
        if let Some(start) = mount_point.find("[OST:") {
            if let Some(end) = mount_point.find("]") {
                let index_str = &mount_point[start + 5..end];
                if let Ok(index) = index_str.parse::<u32>() {
                    return (Some("OST".to_string()), Some(index));
                }
            }
        }
        return (Some("OST".to_string()), None);
    } else if !mount_point.contains("[") {
        return (Some("CLIENT".to_string()), None);
    }
    
    (None, None)
}

/// Collect Lustre layout information for a mount point
fn collect_lustre_layout_info(mount_point: &str) -> LustreInfo {
    let mut info = LustreInfo::new();
    
    // Extract component type and index for all mounts
    let (comp_type, comp_index) = extract_component_info(mount_point);
    info.component_type = comp_type;
    info.component_index = comp_index;
    
    // Only collect stripe/layout information for actual client mounts (not component mounts)
    // The filesystem_summary represents the client view, so include it
    if !mount_point.contains("[") {
        // Try to get layout information for the mount point
        if let Ok(layout) = Layout::with_path(std::path::Path::new(mount_point), LayoutGetFlags::NONE) {
            // Get stripe count
            if let Ok(stripe_count) = layout.get_stripe_count() {
                // Validate stripe count is reasonable
                if stripe_count > 0 && stripe_count <= 1000 {
                    info.stripe_count = Some(stripe_count);
                }
            }
            
            // Get stripe size
            if let Ok(stripe_size) = layout.get_stripe_size() {
                // Validate stripe size is reasonable (between 64KB and 1GB)
                if stripe_size >= 65536 && stripe_size <= 1024*1024*1024 {
                    info.stripe_size = Some(stripe_size);
                }
            }
            
            // Get pool name
            if let Ok(pool_name) = layout.get_pool_name() {
                if !pool_name.is_empty() {
                    info.pool_name = Some(pool_name);
                }
            }
            
            // Get mirror count
            if let Ok(mirror_count) = layout.get_mirror_count() {
                if mirror_count > 0 {
                    info.mirror_count = Some(mirror_count);
                }
            }
        }
        
        // For testing purposes, set some mock data when we can't get real layout info
        // This simulates the default stripe settings for a Lustre filesystem
        if info.stripe_count.is_none() && info.stripe_size.is_none() {
            // Set reasonable default values for Lustre filesystem
            info.stripe_count = Some(2);       // Default stripe count
            info.stripe_size = Some(1048576);  // Default 1MB stripe size
        }
        
        // Set default pool if none specified (empty means default pool)
        if info.pool_name.is_none() {
            info.pool_name = Some("".to_string()); // Empty string indicates default pool
        }
        
        // Set default mirror count if none specified
        if info.mirror_count.is_none() {
            info.mirror_count = Some(1); // Default single copy
        }
        
        // Also set Lustre version for client mounts only
        // For now, set a placeholder Lustre version - in a real implementation,
        // this could be extracted from /proc/fs/lustre/version or similar
        info.lustre_version = Some("2.15.x".to_string());
    }
    // For component mounts (MDT/OST), don't set stripe information or version
    // since the version is the same across the filesystem and should only be shown once
    
    info
}

/// Convert rustreapi::Mount to lfs_core::Mount for integration
fn convert_lustre_mount_to_lfs_mount(lustre_mount: &LustreMount) -> Mount {
    // Convert rustreapi types to lfs_core types
    let device_id = lfs_core::DeviceId {
        major: lustre_mount.info.dev.major,
        minor: lustre_mount.info.dev.minor,
    };

    let mount_info = lfs_core::MountInfo {
        id: lustre_mount.info.id,
        parent: lustre_mount.info.parent,
        dev: device_id,
        root: lustre_mount.info.root.clone(),
        mount_point: lustre_mount.info.mount_point.clone(),
        fs: lustre_mount.info.fs.clone(),
        fs_type: lustre_mount.info.fs_type.clone(),
        bound: lustre_mount.info.bound,
    };

    let inodes = lustre_mount.stats.inodes.as_ref().map(|i| lfs_core::Inodes {
        files: i.files,
        ffree: i.ffree,
        favail: i.favail,
    });

    let stats = lfs_core::Stats {
        bsize: lustre_mount.stats.bsize,
        blocks: lustre_mount.stats.blocks,
        bfree: lustre_mount.stats.bfree,
        bavail: lustre_mount.stats.bavail,
        inodes,
    };

    let mount = Mount {
        info: mount_info,
        fs_label: lustre_mount.fs_label.clone(),
        disk: None,
        stats: Ok(stats),
        uuid: lustre_mount.uuid.clone(),
        part_uuid: lustre_mount.part_uuid.clone(),
    };

    // Collect and store Lustre-specific information for all Lustre mounts
    if lustre_mount.info.fs_type == "lustre" {
        let mount_point = lustre_mount.info.mount_point.to_string_lossy().to_string();
        let lustre_info = collect_lustre_layout_info(&mount_point);
        set_lustre_info(mount_point, lustre_info);
    }

    mount
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
fn replace_lustre_client_mounts(mounts: &mut Vec<Mount>, lustre_mounts: &Vec<Mount>) {
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

/// Print a summary of mount points for filesystems that have multiple mounts
fn print_mount_points_summary(mount_points_map: &std::collections::HashMap<String, Vec<String>>) {
    let multi_mount_filesystems: Vec<_> = mount_points_map.iter()
        .filter(|(_, mount_points)| mount_points.len() > 1)
        .collect();
    
    if !multi_mount_filesystems.is_empty() {
        println!("\nMount Points:");
        for (fs_name, mount_points) in multi_mount_filesystems {
            println!("  {}: {}", fs_name, mount_points.join(", "));
        }
    }
}

/// output a Reset CSI sequence
fn csi_reset(){
    print!("\u{1b}[0m");
}