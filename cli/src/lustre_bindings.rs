//! Lustre API bindings

use std::os::raw::{c_char, c_int, c_uint};

pub const LOV_ALL_STRIPES: u32 = 65535;
pub const LL_STATFS_LMV: u32 = 0x1;
pub const LL_STATFS_LOV: u32 = 0x2;
pub const LL_STATFS_NODELAY: u32 = 0x4;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct obd_statfs {
    pub os_type: u64,
    pub os_blocks: u64,
    pub os_bfree: u64,
    pub os_bavail: u64,
    pub os_files: u64,
    pub os_ffree: u64,
    pub os_fsid: [u8; 40],
    pub os_bsize: u32,
    pub os_namelen: u32,
    pub os_maxbytes: u64,
    pub os_state: u32,
    pub os_fprecreated: u32,
    pub os_granted: u32,
    pub os_spare3: u32,
    pub os_spare4: u32,
    pub os_spare5: u32,
    pub os_spare6: u32,
    pub os_spare7: u32,
    pub os_spare8: u32,
    pub os_spare9: u32,
}

impl Default for obd_statfs {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct obd_uuid {
    pub uuid: [c_char; 40],
}

impl Default for obd_uuid {
    fn default() -> Self {
        Self { uuid: [0; 40] }
    }
}

#[link(name = "lustreapi")]
unsafe extern "C" {
    pub fn llapi_search_mounts(
        pathname: *const c_char,
        index: c_int,
        mntdir: *mut c_char,
        fsname: *mut c_char,
    ) -> c_int;
    
    pub fn llapi_obd_fstatfs(
        fd: c_int,
        type_: c_uint,
        index: c_uint,
        stat_buf: *mut obd_statfs,
        uuid_buf: *mut obd_uuid,
    ) -> c_int;
    
    pub fn llapi_get_fsname(
        path: *const c_char,
        fsname: *mut c_char,
        fsname_len: usize,
    ) -> c_int;
}

/// Convert UUID to string safely
pub fn uuid_to_string(uuid: &obd_uuid) -> String {
    unsafe {
        let cstr = std::ffi::CStr::from_ptr(uuid.uuid.as_ptr());
        cstr.to_string_lossy().to_string()
    }
}

/// Test if Lustre is available and working
pub fn test_lustre_availability() -> bool {
    {
        std::process::Command::new("lfs")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}