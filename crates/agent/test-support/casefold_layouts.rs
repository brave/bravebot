//! Real directories with the casefold flag set on one level and not on the other.
//!
//! ext4 and f2fs fold case per directory, so a workspace and the directory above it can answer
//! differently for the same two spellings of a name. [`build`] makes each layout of the flag in
//! [`LAYOUTS`] and returns the workspace, or `None` where the host cannot set the flag: it needs
//! ext4 or f2fs with casefold enabled, an empty directory and the privilege `chattr +F` takes, and
//! no host is assumed to have that. The filesystem is asked about each layout before a test uses
//! it, so a fixture that did not produce the layout it names fails there and not in the test's
//! assertions.

use rustix::fs::{IFlags, Mode, OFlags};
use std::path::{Path, PathBuf};

/// `FS_CASEFOLD_FL`, which `IFlags` has no name for.
const FS_CASEFOLD_FL: u32 = 0x4000_0000;

/// Whether the directory above the workspace folds case, and whether the workspace does.
pub struct Layout {
    pub name: &'static str,
    pub parent_folds: bool,
    pub workspace_folds: bool,
}

/// Each way the flag can be set on a workspace and the directory above it.
pub const LAYOUTS: [Layout; 4] = [
    Layout {
        name: "ordinary-parent-casefold-workspace",
        parent_folds: false,
        workspace_folds: true,
    },
    Layout {
        name: "casefold-parent-casefold-workspace",
        parent_folds: true,
        workspace_folds: true,
    },
    Layout {
        name: "casefold-parent-ordinary-workspace",
        parent_folds: true,
        workspace_folds: false,
    },
    Layout {
        name: "ordinary-parent-ordinary-workspace",
        parent_folds: false,
        workspace_folds: false,
    },
];

/// Set or clear the casefold flag of the empty directory `directory`. `false` where the host
/// refuses.
fn set_flag(directory: &Path, on: bool) -> bool {
    let Ok(held) = rustix::fs::open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    ) else {
        return false;
    };
    let Ok(current) = rustix::fs::ioctl_getflags(&held) else {
        return false;
    };
    let wanted = match on {
        true => current.bits() | FS_CASEFOLD_FL,
        false => current.bits() & !FS_CASEFOLD_FL,
    };
    wanted == current.bits()
        || rustix::fs::ioctl_setflags(&held, IFlags::from_bits_retain(wanted)).is_ok()
}

/// The workspace `<base>/<layout name>/Parent/Workspace`, canonical, holding a file `.env`, with the
/// flag set as `layout` says; `None` where the host cannot set it.
///
/// The directory is made again each time, so a run does not depend on what an earlier one left.
pub fn build(base: &Path, layout: &Layout) -> Option<PathBuf> {
    let top = base.join(layout.name);
    let _ = std::fs::remove_dir_all(&top);
    let parent = top.join("Parent");
    std::fs::create_dir_all(&parent).unwrap();
    if !set_flag(&parent, layout.parent_folds) {
        let _ = std::fs::remove_dir_all(&top);
        return None;
    }
    let workspace = parent.join("Workspace");
    std::fs::create_dir(&workspace).unwrap();
    if !set_flag(&workspace, layout.workspace_folds) {
        let _ = std::fs::remove_dir_all(&top);
        return None;
    }
    let root = workspace.canonicalize().unwrap();
    std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();

    // The oracle is the filesystem, asked with names of the test's own: inside the workspace for the
    // names the rules are about, and inside its parent for the name the old probe asked about.
    assert_eq!(
        root.join(".ENV").exists(),
        layout.workspace_folds,
        "{}: the workspace did not fold as the layout says",
        layout.name
    );
    assert_eq!(
        root.parent().unwrap().join("wORKSPACE").exists(),
        layout.parent_folds,
        "{}: the parent did not fold as the layout says",
        layout.name
    );
    Some(root)
}
