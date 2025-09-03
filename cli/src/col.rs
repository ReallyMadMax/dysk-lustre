use {
    crate::order::Order,
    lfs_core::Mount,
    std::{
        cmp::Ordering,
        fmt,
        str::FromStr,
    },
    termimad::minimad::Alignment,
};

/// Extract filesystem name for display
/// For Lustre: show mount points for components and client mount
/// For others: use filesystem type
pub fn extract_fsname(mount: &Mount) -> String {
    if mount.info.fs_type == "lustre" {
        // For all Lustre mounts, return the mount point
        mount.info.mount_point.to_string_lossy().to_string()
    } else {
        // For non-Lustre filesystems, use filesystem type
        mount.info.fs_type.clone()
    }
}

macro_rules! col_enum {
    (@just_variant $variant:ident $discarded:ident) => {
        Col::$variant
    };
    ($($variant:ident $name:literal $($alias:literal)* : $title:literal $inode_title:literal $($def:ident)*,)*) => {
        /// A column of the lfs table.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub enum Col {
            $($variant,)*
        }
        pub static ALL_COLS: &[Col] = &[
            $(Col::$variant,)*
        ];
        pub static DEFAULT_COLS: &[Col] = &[
            $(
                $(col_enum!(@just_variant $variant $def),)*
            )*
        ];
        impl FromStr for Col {
            type Err = ParseColError;
            fn from_str(s: &str) -> Result<Self, ParseColError> {
                match s {
                    $(
                        $name => Ok(Self::$variant),
                        $(
                            $alias => Ok(Self::$variant),
                        )*
                    )*
                    _ => Err(ParseColError::new(s)),
                }
            }
        }
        impl fmt::Display for Col {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(
                        Self::$variant => write!(f, "{}", self.title(false)),
                    )*
                }
            }
        }
        impl Col {
            pub fn name(self) -> &'static str {
                match self {
                    $(
                        Self::$variant => $name,
                    )*
                }
            }
            pub fn title(self, inodes_mode: bool) -> &'static str {
                match self {
                    $(
                        Self::$variant => if inodes_mode { $inode_title } else { $title },
                    )*
                }
            }
            pub fn aliases(self) -> &'static [&'static str] {
                match self {
                    $(
                        Self::$variant => &[$($alias,)*],
                    )*
                }
            }
            pub fn is_default(self) -> bool {
                DEFAULT_COLS.contains(&self)
            }
        }
    };
}

// definition of all columns and their names
// in the --cols definition
// syntax: Variant name [aliases]: byte_title inode_title [default]
col_enum!(
    Id "id": "id" "id",
    Dev "dev" "device" "device_id": "dev" "dev",
    Filesystem "fs" "filesystem": "filesystem" "filesystem" default,
    Label "label": "label" "label",
    Type "type": "type" "type",
    Remote "remote" "rem": "remote" "remote",
    Disk "disk" "dsk": "disk" "disk",
    Used "used": "bytes used" "inodes used" default,
    Use "use": "use %" "use %" default,
    UsePercent "use_percent": "bytes %" "inodes %",
    Free "free": "bytes free" "inodes free" default,
    FreePercent "free_percent": "bytes free %" "inodes free %",
    Size "size": "bytes total" "inodes total" default,
    InodesUsed "inodes_used" "iused": "used inodes" "used inodes",
    InodesUse "inodes" "ino" "inodes_use" "iuse": "inodes" "inodes",
    InodesUsePercent "inodes_use_percent" "iuse_percent": "inodes%" "inodes%",
    InodesFree "inodes_free" "ifree": "free inodes" "free inodes",
    InodesCount "inodes_total" "inodes_count" "itotal": "inodes total" "inodes total",
    MountPoint "mount" "mount_point" "mp": "mount point" "mount point" default,
    FsName "fsname" "fs_name": "filesystem name" "fsname",
    Uuid "uuid": "UUID" "UUID",
    PartUuid "partuuid" "part_uuid": "PARTUUID" "PARTUUID",
    StripeCount "stripe_count" "stripes": "stripe count" "stripe count",
    StripeSize "stripe_size": "stripe size" "stripe size", 
    LustreVersion "lustre_version" "lus_ver": "lustre version" "lustre version",
    PoolName "pool_name" "pool": "pool name" "pool name",
    ComponentType "component_type" "comp_type": "component type" "component type",
    ComponentIndex "component_index" "comp_idx": "component index" "component index",
    MirrorCount "mirror_count" "mirrors": "mirror count" "mirror count",
);

impl Col {
    pub fn header_align(self) -> Alignment {
        match self {
            Self::Label => Alignment::Left,
            Self::MountPoint => Alignment::Left,
            Self::FsName => Alignment::Left,
            _ => Alignment::Center,
        }
    }
    pub fn content_align(self) -> Alignment {
        match self {
            Self::Id => Alignment::Center,
            Self::Dev => Alignment::Center,
            Self::Filesystem => Alignment::Left,
            Self::Label => Alignment::Left,
            Self::Type => Alignment::Center,
            Self::Remote => Alignment::Center,
            Self::Disk => Alignment::Center,
            Self::Used => Alignment::Center,
            Self::Use => Alignment::Center,
            Self::UsePercent => Alignment::Center,
            Self::Free => Alignment::Center,
            Self::FreePercent => Alignment::Center,
            Self::Size => Alignment::Center,
            Self::InodesUsed => Alignment::Center,
            Self::InodesUse => Alignment::Center,
            Self::InodesUsePercent => Alignment::Center,
            Self::InodesFree => Alignment::Center,
            Self::InodesCount => Alignment::Center,
            Self::MountPoint => Alignment::Left,
            Self::FsName => Alignment::Left,
            Self::Uuid => Alignment::Left,
            Self::PartUuid => Alignment::Left,
            Self::StripeCount => Alignment::Center,
            Self::StripeSize => Alignment::Center,
            Self::LustreVersion => Alignment::Center,
            Self::PoolName => Alignment::Left,
            Self::ComponentType => Alignment::Center,
            Self::ComponentIndex => Alignment::Center,
            Self::MirrorCount => Alignment::Center,
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Id => "mount point id",
            Self::Dev => "device id",
            Self::Filesystem => "filesystem",
            Self::Label => "volume label",
            Self::Type => "filesystem type",
            Self::Remote => "whether it's a remote filesystem",
            Self::Disk => "storage type",
            Self::Used => "bytes used (or inodes used with -i)",
            Self::Use => "usage graphical view (bytes or inodes with -i)",
            Self::UsePercent => "percentage used (bytes or inodes with -i)",
            Self::Free => "free bytes (or free inodes with -i)",
            Self::FreePercent => "percentage free (bytes or inodes with -i)",
            Self::Size => "total size (bytes or inodes with -i)",
            Self::InodesUsed => "number of inodes used",
            Self::InodesUse => "graphical view of inodes usage",
            Self::InodesUsePercent => "percentage of inodes used",
            Self::InodesFree => "number of free inodes",
            Self::InodesCount => "total count of inodes",
            Self::MountPoint => "mount point",
            Self::FsName => "filesystem name",
            Self::Uuid => "filesystem UUID",
            Self::PartUuid => "partition UUID",
            Self::StripeCount => "number of OSTs file data is striped across",
            Self::StripeSize => "size of each stripe in bytes",
            Self::LustreVersion => "version of Lustre filesystem",
            Self::PoolName => "OST pool name for workload isolation",
            Self::ComponentType => "type of Lustre component (MDT/OST/CLIENT)",
            Self::ComponentIndex => "index number of the component",
            Self::MirrorCount => "number of file mirrors for data replication",
        }
    }
    pub fn comparator(self) -> impl for<'a, 'b> FnMut(&'a Mount, &'b Mount) -> Ordering {
        match self {
            Self::Id => |a: &Mount, b: &Mount| a.info.id.cmp(&b.info.id),
            Self::Dev => |a: &Mount, b: &Mount| a.info.dev.cmp(&b.info.dev),
            Self::Filesystem =>  |a: &Mount, b: &Mount| a.info.fs.cmp(&b.info.fs),
            Self::Label =>  |a: &Mount, b: &Mount| match (&a.fs_label, &b.fs_label) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            Self::Type =>  |a: &Mount, b: &Mount| a.info.fs_type.cmp(&b.info.fs_type),
            Self::Remote =>  |a: &Mount, b: &Mount| a.info.is_remote().cmp(&b.info.is_remote()),
            Self::Disk =>  |a: &Mount, b: &Mount| match (&a.disk, &b.disk) {
                (Some(a), Some(b)) => a.disk_type().to_lowercase().cmp(&b.disk_type().to_lowercase()),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::Used =>  |a: &Mount, b: &Mount| match (&a.stats(), &b.stats()) {
                (Some(a), Some(b)) => a.used().cmp(&b.used()),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::Use | Self::UsePercent =>  |a: &Mount, b: &Mount| match (&a.stats(), &b.stats()) {
                // SAFETY: use_share() doesn't return NaN
                (Some(a), Some(b)) => a.use_share().partial_cmp(&b.use_share()).unwrap(),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::Free =>  |a: &Mount, b: &Mount| match (&a.stats(), &b.stats()) {
                (Some(a), Some(b)) => a.available().cmp(&b.available()),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::FreePercent =>  |a: &Mount, b: &Mount| match (&a.stats(), &b.stats()) {
                (Some(a), Some(b)) => b.use_share().partial_cmp(&a.use_share()).unwrap(),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::Size =>  |a: &Mount, b: &Mount| match (&a.stats(), &b.stats()) {
                (Some(a), Some(b)) => a.size().cmp(&b.size()),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::InodesUsed =>  |a: &Mount, b: &Mount| match (&a.inodes(), &b.inodes()) {
                (Some(a), Some(b)) => a.used().cmp(&b.used()),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::InodesUsePercent | Self::InodesUse  =>  |a: &Mount, b: &Mount| match (&a.inodes(), &b.inodes()) {
                // SAFETY: use_share() doesn't return NaN
                (Some(a), Some(b)) => a.use_share().partial_cmp(&b.use_share()).unwrap(),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::InodesFree =>  |a: &Mount, b: &Mount| match (&a.inodes(), &b.inodes()) {
                (Some(a), Some(b)) => a.favail.cmp(&b.favail),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::InodesCount =>  |a: &Mount, b: &Mount| match (&a.inodes(), &b.inodes()) {
                (Some(a), Some(b)) => a.files.cmp(&b.files),
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
                (None, None) => Ordering::Equal,
            },
            Self::MountPoint =>  |a: &Mount, b: &Mount| a.info.mount_point.cmp(&b.info.mount_point),
            Self::FsName => |a: &Mount, b: &Mount| extract_fsname(a).cmp(&extract_fsname(b)),
            Self::Uuid => |a: &Mount, b: &Mount| match (&a.uuid, &b.uuid) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            Self::PartUuid => |a: &Mount, b: &Mount| match (&a.part_uuid, &b.part_uuid) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            Self::StripeCount => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.stripe_count).unwrap_or(0);
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.stripe_count).unwrap_or(0);
                a_info.cmp(&b_info)
            },
            Self::StripeSize => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.stripe_size).unwrap_or(0);
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.stripe_size).unwrap_or(0);
                a_info.cmp(&b_info)
            },
            Self::LustreVersion => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.lustre_version).unwrap_or_else(|| "".to_string());
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.lustre_version).unwrap_or_else(|| "".to_string());
                a_info.cmp(&b_info)
            },
            Self::PoolName => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.pool_name).unwrap_or_else(|| "".to_string());
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.pool_name).unwrap_or_else(|| "".to_string());
                a_info.cmp(&b_info)
            },
            Self::ComponentType => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.component_type).unwrap_or_else(|| "".to_string());
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.component_type).unwrap_or_else(|| "".to_string());
                a_info.cmp(&b_info)
            },
            Self::ComponentIndex => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.component_index).unwrap_or(u32::MAX);
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.component_index).unwrap_or(u32::MAX);
                a_info.cmp(&b_info)
            },
            Self::MirrorCount => |a: &Mount, b: &Mount| {
                let a_point = a.info.mount_point.to_string_lossy();
                let b_point = b.info.mount_point.to_string_lossy();
                let a_info = crate::get_lustre_info(&a_point).and_then(|i| i.mirror_count).unwrap_or(0);
                let b_info = crate::get_lustre_info(&b_point).and_then(|i| i.mirror_count).unwrap_or(0);
                a_info.cmp(&b_info)
            },
        }
    }
    pub fn default_sort_order(self) -> Order {
        match self {
            Self::Id => Order::Asc,
            Self::Dev => Order::Asc,
            Self::Filesystem => Order::Asc,
            Self::Label => Order::Asc,
            Self::Type => Order::Asc,
            Self::Remote => Order::Desc,
            Self::Disk => Order::Asc,
            Self::Used => Order::Asc,
            Self::Use => Order::Desc,
            Self::UsePercent => Order::Asc,
            Self::Free => Order::Asc,
            Self::FreePercent => Order::Desc,
            Self::Size => Order::Desc,
            Self::InodesUsed => Order::Asc,
            Self::InodesUse => Order::Asc,
            Self::InodesUsePercent => Order::Asc,
            Self::InodesFree => Order::Asc,
            Self::InodesCount => Order::Asc,
            Self::MountPoint => Order::Asc,
            Self::FsName => Order::Asc,
            Self::Uuid => Order::Asc,
            Self::PartUuid => Order::Asc,
            Self::StripeCount => Order::Desc,
            Self::StripeSize => Order::Desc,
            Self::LustreVersion => Order::Asc,
            Self::PoolName => Order::Asc,
            Self::ComponentType => Order::Asc,
            Self::ComponentIndex => Order::Asc,
            Self::MirrorCount => Order::Desc,
        }
    }
    pub fn default_sort_col() -> Self {
        Self::Size
    }
}


#[derive(Debug)]
pub struct ParseColError {
    /// the string which couldn't be parsed
    pub raw: String,
}
impl ParseColError {
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self { raw: s.into() }
    }
}
impl fmt::Display for ParseColError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} can't be parsed as a column; use 'dysk --list-cols' to see all column names",
            self.raw,
        )
    }
}
impl std::error::Error for ParseColError {}