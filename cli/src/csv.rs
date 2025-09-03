use {
    crate::{
        Args, col::Col,
    },
    lfs_core::*,
    std::{
        fmt::Display,
        io::Write,
    },
};

/// Utility to write in CSV
struct Csv<W: Write> {
    separator: char,
    w: W,
}

impl<W: Write> Csv<W> {
    pub fn new(separator: char, w: W) -> Self {
        Self { separator, w }
    }
    pub fn cell<D: Display>(&mut self, content: D) -> Result<(), std::io::Error> {
        let s = content.to_string();
        let needs_quotes = s.contains(self.separator) || s.contains('"') || s.contains('\n');
        if needs_quotes {
            write!(self.w, "\"")?;
            for c in s.chars() {
                if c == '"' {
                    write!(self.w, "\"\"")?;
                } else {
                    write!(self.w, "{}", c)?;
                }
            }
            write!(self.w, "\"")?;
        } else {
            write!(self.w, "{}", s)?;
        }
        write!(self.w, "{}", self.separator)
    }
    pub fn cell_opt<D: Display>(&mut self, content: Option<D>) -> Result<(), std::io::Error> {
        if let Some(c) = content {
            self.cell(c)
        } else {
            write!(self.w, "{}", self.separator)
        }
    }
    pub fn end_line(&mut self) -> Result<(), std::io::Error> {
        writeln!(self.w)
    }
}

pub fn print(mounts: &[&Mount], args: &Args) -> Result<(), std::io::Error> {
    let units = args.units;
    let inodes_mode = args.inodes;
    let mut csv = Csv::new(args.csv_separator, std::io::stdout());
    
    for col in args.cols.cols() {
        csv.cell(col.title(inodes_mode))?;
    }
    csv.end_line()?;
    
    for mount in mounts {
        for col in args.cols.cols() {
            match col {
                Col::Id => csv.cell(mount.info.id),
                Col::Dev => csv.cell(format!("{}:{}", mount.info.dev.major, mount.info.dev.minor)),
                Col::Filesystem => csv.cell(&mount.info.fs),
                Col::Label => csv.cell_opt(mount.fs_label.as_ref()),
                Col::Type => csv.cell(&mount.info.fs_type),
                Col::Remote => csv.cell(if mount.info.is_remote() { "yes" } else { "no" }),
                Col::Disk => csv.cell_opt(mount.disk.as_ref().map(|d| d.disk_type())),
                Col::Used => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| i.used()))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| units.fmt(s.used())))
                    }
                },
                Col::Use => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| i.use_share()))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| s.use_share()))
                    }
                },
                Col::UsePercent => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| format!("{:.0}%", 100.0 * i.use_share())))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| format!("{:.0}%", 100.0 * s.use_share())))
                    }
                },
                Col::Free => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| i.favail))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| units.fmt(s.available())))
                    }
                },
                Col::FreePercent => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| format!("{:.0}%", 100.0 * (1.0 - i.use_share()))))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| format!("{:.0}%", 100.0 * (1.0 - s.use_share()))))
                    }
                },
                Col::Size => {
                    if inodes_mode {
                        csv.cell_opt(mount.inodes().map(|i| i.files))
                    } else {
                        csv.cell_opt(mount.stats().map(|s| units.fmt(s.size())))
                    }
                },
                Col::InodesUsed => csv.cell_opt(mount.inodes().map(|i| i.used())),
                Col::InodesUse => csv.cell_opt(mount.inodes().map(|i| i.use_share())),
                Col::InodesUsePercent => csv.cell_opt(mount.inodes().map(|i| format!("{:.0}%", 100.0 * i.use_share()))),
                Col::InodesFree => csv.cell_opt(mount.inodes().map(|i| i.favail)),
                Col::InodesCount => csv.cell_opt(mount.inodes().map(|i| i.files)),
                Col::MountPoint => csv.cell(mount.info.mount_point.to_string_lossy()),
                Col::FsName => csv.cell(crate::col::extract_fsname(&mount)),
                Col::Uuid => csv.cell(mount.uuid.as_ref().map_or("", |v| v)),
                Col::PartUuid => csv.cell(mount.part_uuid.as_ref().map_or("", |v| v)),
                Col::StripeCount => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell_opt(lustre_info.stripe_count.map(|c| c.to_string()))
                    } else {
                        csv.cell("")
                    }
                },
                Col::StripeSize => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell_opt(lustre_info.stripe_size.map(|s| s.to_string()))
                    } else {
                        csv.cell("")
                    }
                },
                Col::LustreVersion => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell(lustre_info.lustre_version.as_deref().unwrap_or(""))
                    } else {
                        csv.cell("")
                    }
                },
                Col::PoolName => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell(lustre_info.pool_name.as_deref().unwrap_or(""))
                    } else {
                        csv.cell("")
                    }
                },
                Col::ComponentType => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell(lustre_info.component_type.as_deref().unwrap_or(""))
                    } else {
                        csv.cell("")
                    }
                },
                Col::ComponentIndex => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell_opt(lustre_info.component_index.map(|i| i.to_string()))
                    } else {
                        csv.cell("")
                    }
                },
                Col::MirrorCount => {
                    let mount_point_str = mount.info.mount_point.to_string_lossy();
                    if let Some(lustre_info) = crate::get_lustre_info(&mount_point_str) {
                        csv.cell_opt(lustre_info.mirror_count.map(|m| m.to_string()))
                    } else {
                        csv.cell("")
                    }
                },
            }?;
        }
        csv.end_line()?;
    }
    Ok(())
}

#[test]
fn test_csv() {
    use std::io::Cursor;
    let mut w = Cursor::new(Vec::new());
    let mut csv = Csv::new(';', &mut w);
    csv.cell("1;2;3").unwrap();
    csv.cell("\"").unwrap();
    csv.cell("").unwrap();
    csv.end_line().unwrap();
    csv.cell(3).unwrap();
    let s = String::from_utf8(w.into_inner()).unwrap();
    assert_eq!(
        s,
r#""1;2;3";"""";;
3;"#,
    );
}