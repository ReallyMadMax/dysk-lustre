//! Lustre integration for dysk
//! 
//! This module provides Lustre filesystem discovery that integrates
//! directly with dysk's existing Mount structure.

use lfs_core::{Mount, MountInfo, Stats, Inodes, DeviceId};
use std::{
    ffi::{CString, CStr},
    os::raw::{c_char},
    path::PathBuf,
};

pub use crate::lustre_bindings::*;

/// Discover Lustre filesystems and convert them to dysk's Mount format
pub fn discover_lustre_mounts() -> Vec<Mount> {
    if !test_lustre_availability() {
        return Vec::new();
    }

    let mut lustre_mounts = Vec::new();
    let mut index = 0;
    let mut mntdir = vec![0u8; 4096];
    let mut fsname = vec![0u8; 256];
    let mut path = vec![0u8; 4096];

    unsafe {
        while llapi_search_mounts(
            path.as_ptr() as *const c_char,
            index,
            mntdir.as_mut_ptr() as *mut c_char,
            fsname.as_mut_ptr() as *mut c_char,
        ) == 0 {
            let mntdir_str = CStr::from_ptr(mntdir.as_ptr() as *const c_char)
                .to_string_lossy()
                .to_string();
            let fsname_str = CStr::from_ptr(fsname.as_ptr() as *const c_char)
                .to_string_lossy()
                .to_string();

            if !mntdir_str.is_empty() {
                if let Ok(mut fs_mounts) = collect_lustre_mounts(&mntdir_str, &fsname_str) {
                    lustre_mounts.append(&mut fs_mounts);
                }
            }

            index += 1;
            mntdir.fill(0);
            fsname.fill(0);
            path.fill(0);
        }
    }

    lustre_mounts
}

/// Check if a specific path is on a Lustre filesystem
pub fn is_lustre_path(path: &std::path::Path) -> bool {
    if !test_lustre_availability() {
        return false;
    }

    let path_str = match path.to_str() {
        Some(s) => s,
        None => return false,
    };

    let mut fsname = vec![0u8; 256];
    let path_c = match CString::new(path_str) {
        Ok(c) => c,
        Err(_) => return false,
    };

    unsafe {
        let rc = llapi_get_fsname(
            path_c.as_ptr(),
            fsname.as_mut_ptr() as *mut c_char,
            fsname.len(),
        );
        rc == 0
    }
}

fn collect_lustre_mounts(mntdir: &str, fsname: &str) -> Result<Vec<Mount>, Box<dyn std::error::Error>> {
    let mntdir_c = CString::new(mntdir)?;
    let mut mounts = Vec::new();

    unsafe {
        let fd = libc::open(mntdir_c.as_ptr(), libc::O_RDONLY);
        if fd < 0 {
            return Err(format!("Cannot open '{}': {}", mntdir, std::io::Error::last_os_error()).into());
        }

        // Create individual MDT entries
        let mut mdt_index = 0;
        while mdt_index < LOV_ALL_STRIPES {
            let mut stat_buf = obd_statfs::default();
            let mut uuid_buf = obd_uuid::default();

            let rc = llapi_obd_fstatfs(fd, LL_STATFS_LMV, mdt_index, &mut stat_buf, &mut uuid_buf);
            if rc == -38 { // ENODEV - no more MDTs
                break;
            }
            if rc == -11 { // EAGAIN - continue to next
                mdt_index += 1;
                continue;
            }

            if rc == 0 || rc == -61 { // Success or ENODATA (inactive)
                if let Ok(mdt_mount) = create_lustre_component_mount(
                    mntdir, 
                    fsname, 
                    &stat_buf, 
                    &uuid_buf, 
                    "MDT", 
                    mdt_index,
                    rc
                ) {
                    mounts.push(mdt_mount);
                }
            }
            mdt_index += 1;
        }

        // Create individual OST entries
        let mut ost_index = 0;
        while ost_index < LOV_ALL_STRIPES {
            let mut stat_buf = obd_statfs::default();
            let mut uuid_buf = obd_uuid::default();

            let rc = llapi_obd_fstatfs(fd, LL_STATFS_LOV, ost_index, &mut stat_buf, &mut uuid_buf);
            if rc == -38 { // ENODEV - no more OSTs
                break;
            }
            if rc == -11 { // EAGAIN - continue to next
                ost_index += 1;
                continue;
            }

            if rc == 0 || rc == -61 { // Success or ENODATA (inactive)
                if let Ok(ost_mount) = create_lustre_component_mount(
                    mntdir, 
                    fsname, 
                    &stat_buf, 
                    &uuid_buf, 
                    "OST", 
                    ost_index,
                    rc
                ) {
                    mounts.push(ost_mount);
                }
            }
            ost_index += 1;
        }

        // Create aggregated client mount entry (filesystem summary)
        if let Ok(client_mount) = create_lustre_client_mount(mntdir, fsname) {
            mounts.push(client_mount);
        }
        
        libc::close(fd);
    }

    Ok(mounts)
}

/// Create a Mount entry for an individual Lustre component (MDT or OST)
unsafe fn create_lustre_component_mount(
    mntdir: &str, 
    fsname: &str, 
    stat_buf: &obd_statfs,
    uuid_buf: &obd_uuid,
    component_type: &str,
    index: u32,
    rc: i32
) -> Result<Mount, Box<dyn std::error::Error>> {
    
    let uuid_str = uuid_to_string(uuid_buf);
    let component_name = if uuid_str.is_empty() {
        format!("{}:{:04x}", component_type, index)
    } else {
        uuid_str.clone()
    };

    // Create a unique mount point path for this component
    let component_mount_point = PathBuf::from(format!("{}[{}:{}]", mntdir, component_type, index));
    
    let mount_info = MountInfo {
        id: 0, // We'll need to generate appropriate IDs
        parent: 0,
        dev: DeviceId { 
            major: if component_type == "MDT" { 1 } else { 2 }, // Differentiate MDT vs OST
            minor: index 
        },
        fs: component_name,
        fs_type: "lustre".to_string(),
        mount_point: component_mount_point,
        bound: false,
        root: Default::default(),
    };

    let stats = if rc == 0 { // Success
        Ok(Stats {
            bsize: stat_buf.os_bsize as u64,
            blocks: stat_buf.os_blocks,
            bfree: stat_buf.os_bfree,
            bavail: stat_buf.os_bavail,
            inodes: if component_type == "MDT" && stat_buf.os_files > 0 {
                // Only MDTs typically have meaningful inode information
                Some(Inodes {
                    files: stat_buf.os_files,
                    ffree: stat_buf.os_ffree,
                    favail: stat_buf.os_ffree,
                })
            } else if component_type == "OST" && stat_buf.os_files > 0 {
                // OSTs may have file objects but different semantics
                Some(Inodes {
                    files: stat_buf.os_files,
                    ffree: stat_buf.os_ffree,
                    favail: stat_buf.os_ffree,
                })
            } else {
                None
            },
        })
    } else {
            Err(lfs_core::StatsError::Unreachable)
        };

    let mount = Mount {
        info: mount_info,
        stats,
        disk: None, // Lustre components don't map to traditional disks
        fs_label: Some(format!("{}-{}", fsname, component_type)),
        uuid: if uuid_str.is_empty() { None } else { Some(uuid_str) },
        part_uuid: None,
    };

    Ok(mount)
}

unsafe fn create_lustre_client_mount(mntdir: &str, fsname: &str) -> Result<Mount, Box<dyn std::error::Error>> {
    let mntdir_c = CString::new(mntdir)?;
    let fd = libc::open(mntdir_c.as_ptr(), libc::O_RDONLY);
    if fd < 0 {
        return Err("Cannot open mount directory".into());
    }

    // Aggregate stats from all OSTs for space information
    let mut total_blocks = 0u64;
    let mut total_bfree = 0u64;
    let mut total_bavail = 0u64;
    let mut total_files = 0u64;
    let mut total_ffree = 0u64;
    let mut bsize = 4096u32; // Default block size
    let mut ost_count = 0;

    // Get OST stats for space
    let mut index = 0;
    while index < LOV_ALL_STRIPES {
        let mut stat_buf = obd_statfs::default();
        let mut uuid_buf = obd_uuid::default();

        let rc = llapi_obd_fstatfs(fd, LL_STATFS_LOV, index, &mut stat_buf, &mut uuid_buf);
        if rc == -38 { break; } // ENODEV
        if rc == -11 { // EAGAIN
            index += 1;
            continue;
        }
        if rc == 0 {
            total_blocks += stat_buf.os_blocks;
            total_bfree += stat_buf.os_bfree;
            total_bavail += stat_buf.os_bavail;
            if bsize == 4096 { // Use first valid bsize
                bsize = stat_buf.os_bsize;
            }
            ost_count += 1;
        }
        index += 1;
    }

    // Get MDT stats for inode information
    index = 0;
    while index < LOV_ALL_STRIPES {
        let mut stat_buf = obd_statfs::default();
        let mut uuid_buf = obd_uuid::default();

        let rc = llapi_obd_fstatfs(fd, LL_STATFS_LMV, index, &mut stat_buf, &mut uuid_buf);
        if rc == -38 { break; } // ENODEV
        if rc == -11 { // EAGAIN
            index += 1;
            continue;
        }
        if rc == 0 {
            total_files += stat_buf.os_files;
            total_ffree += stat_buf.os_ffree;
        }
        index += 1;
    }

    libc::close(fd);

    // Create Mount structure compatible with dysk
    let mount_info = MountInfo {
        id: 0, // We'll need to generate appropriate IDs
        parent: 0,
        dev: DeviceId { major: 0, minor: 0 }, // Lustre doesn't have traditional device IDs
        fs: format!("{}@lustre", fsname), // Make it clear this is the aggregated view
        fs_type: "lustre".to_string(),
        mount_point: PathBuf::from(mntdir),
        bound: false,
        root: Default::default(),
    };

    let stats = if total_blocks > 0 && ost_count > 0 {
        Ok(Stats {
            bsize: bsize as u64,
            blocks: total_blocks,
            bfree: total_bfree,
            bavail: total_bavail,
            inodes: if total_files > 0 {
                Some(Inodes {
                    files: total_files,
                    ffree: total_ffree,
                    favail: total_ffree,
                })
            } else {
                None
            },
        })
    } else {
        Err(lfs_core::StatsError::Unreachable)
    };

    let mount = Mount {
        info: mount_info,
        stats,
        disk: None,
        fs_label: Some(format!("Lustre-{}", fsname)),
        uuid: None,
        part_uuid: None,
    };

    Ok(mount)
}

/// Check if Lustre is available on the system
pub fn lustre_availability() -> bool {
    // Check if lfs command is available
    std::process::Command::new("lfs")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lustre_availability() {
        let _available = lustre_availability();
    }

    #[test]
    fn test_discover_mounts() {
        let _mounts = discover_lustre_mounts();
    }
}