use {
    crate::{
        Args, col::Col,
    },
    lfs_core::*,
    termimad::{
        crossterm::style::Color::*,
        minimad::{self, OwningTemplateExpander, TableBuilder},
        CompoundStyle, MadSkin, ProgressBar,
    },
};

// those colors are chosen to be "redish" for used, "greenish" for available
// and, most importantly, to work on both white and black backgrounds. If you
// find a better combination, please show me.
static USED_COLOR: u8 = 209;
static AVAI_COLOR: u8 = 65;
static SIZE_COLOR: u8 = 172;

static BAR_WIDTH: usize = 5;
static INODES_BAR_WIDTH: usize = 5;

pub fn print(mounts: &[&Mount], color: bool, args: &Args) {
    if args.cols.is_empty() {
        return;
    }
    let units = args.units;
    let inodes_mode = args.inodes;  // Add this line
    let mut expander = OwningTemplateExpander::new();
    expander.set_default("");

    // Check if this is a Lustre-only display
    let is_lustre_display = mounts.iter().all(|m| m.info.fs_type == "lustre") && mounts.len() > 1;
    let mut added_separator = false;

    for mount in mounts {
        // Add empty row separator before client mount (filesystem summary) in Lustre display
        if is_lustre_display && !added_separator && mount.info.fs == "filesystem_summary" {
            expander.sub("rows"); // Add empty row
            added_separator = true;
        }

        let sub = expander
            .sub("rows")
            .set("id", mount.info.id)
            .set("dev-major", mount.info.dev.major)
            .set("dev-minor", mount.info.dev.minor)
            .set("filesystem", &mount.info.fs)
            .set("disk", mount.disk.as_ref().map_or("", |d| d.disk_type()))
            .set("type", &mount.info.fs_type)
            .set("mount-point", mount.info.mount_point.to_string_lossy())
            .set_option("uuid", mount.uuid.as_ref())
            .set_option("part_uuid", mount.part_uuid.as_ref());
        if let Some(label) = &mount.fs_label {
            sub.set("label", label);
        }
        if mount.info.is_remote() {
            sub.set("remote", "x");
        }
        if let Some(stats) = mount.stats() {
            if inodes_mode {
                // Show inode data instead of byte data
                if let Some(inodes) = &stats.inodes {
                    let iuse_share = inodes.use_share();
                    let ifree_share = 1.0 - iuse_share;
                    sub
                        .set("size", inodes.files)
                        .set("used", inodes.used())
                        .set("use-percents", format!("{:.0}%", 100.0 * iuse_share))
                        .set_md("bar", progress_bar_md(iuse_share, BAR_WIDTH, args.ascii))
                        .set("free", inodes.favail)
                        .set("free-percents", format!("{:.0}%", 100.0 * ifree_share));
                } else {
                    sub.set("use-error", "no inodes data");
                }
            } else {
                // Show byte data (default)
                let use_share = stats.use_share();
                let free_share = 1.0 - use_share;
                sub
                    .set("size", units.fmt(stats.size()))
                    .set("used", units.fmt(stats.used()))
                    .set("use-percents", format!("{:>3.0}%", 100.0 * use_share))
                    .set_md("bar", progress_bar_md(use_share, BAR_WIDTH, args.ascii))
                    .set("free", units.fmt(stats.available()))
                    .set("free-percents", format!("{:.0}%", 100.0 * free_share));
            }
            
            // Always set the dedicated inode columns regardless of mode
            if let Some(inodes) = &stats.inodes {
                let iuse_share = inodes.use_share();
                sub
                    .set("inodes", inodes.files)
                    .set("iused", inodes.used())
                    .set("iuse-percents", format!("{:>3.0}%", 100.0 * iuse_share))
                    .set_md("ibar", progress_bar_md(iuse_share, INODES_BAR_WIDTH, args.ascii))
                    .set("ifree", inodes.favail);
            }
        } else if mount.is_unreachable() {
            sub.set("use-error", "unreachable");
        }
    }
    let mut skin = if color {
        make_colored_skin()
    } else {
        MadSkin::no_style()
    };
    if args.ascii {
        skin.limit_to_ascii();
    }

    let mut tbl = TableBuilder::default();
    for col in args.cols.cols() {
        tbl.col(
            minimad::Col::new(
                col.title(inodes_mode),
                match col {
                    Col::Id => "${id}",
                    Col::Dev => "${dev-major}:${dev-minor}",
                    Col::Filesystem => "${filesystem}",
                    Col::Label => "${label}",
                    Col::Disk => "${disk}",
                    Col::Type => "${type}",
                    Col::Remote => "${remote}",
                    Col::Used => "~~${used}~~",
                    Col::Use => "~~${use-percents}~~ ${bar}~~${use-error}~~",
                    Col::UsePercent => "~~${use-percents}~~",
                    Col::Free => "*${free}*",
                    Col::FreePercent => "*${free-percents}*",
                    Col::Size => "**${size}**",
                    Col::InodesFree => "*${ifree}*",
                    Col::InodesUsed => "~~${iused}~~",
                    Col::InodesUse => "~~${iuse-percents}~~ ${ibar}",
                    Col::InodesUsePercent => "~~${iuse-percents}~~",
                    Col::InodesCount => "**${inodes}**",
                    Col::MountPoint => "${mount-point}",
                    Col::Uuid => "${uuid}",
                    Col::PartUuid => "${part_uuid}",
                }
            )
            .align_content(col.content_align())
            .align_header(col.header_align())
        );
    }

    skin.print_owning_expander_md(&expander, &tbl);
}

fn make_colored_skin() -> MadSkin {
    MadSkin {
        bold: CompoundStyle::with_fg(AnsiValue(SIZE_COLOR)), // size
        inline_code: CompoundStyle::with_fgbg(AnsiValue(USED_COLOR), AnsiValue(AVAI_COLOR)), // use bar
        strikeout: CompoundStyle::with_fg(AnsiValue(USED_COLOR)), // use%
        italic: CompoundStyle::with_fg(AnsiValue(AVAI_COLOR)), // available
        ..Default::default()
    }
}

fn progress_bar_md(
    share: f64,
    bar_width: usize,
    ascii: bool,
) -> String {
    if ascii {
        let count = (share * bar_width as f64).round() as usize;
        let bar: String = "■".repeat(count);
        let no_bar: String = "-".repeat(bar_width-count);
        format!("~~{}~~*{}*", bar, no_bar)
    } else {
        let pb = ProgressBar::new(share as f32, bar_width);
        format!("`{:<width$}`", pb, width = bar_width)
    }
}