//! The search for programs leaves out the checkout holding a directory a session was opened in,
//! and not only the directory (SANDBOX-30).
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_sandbox::programs::{
    find_in, find_on, keep_out_with_checkout, registered, with_checkout,
};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// An empty directory under this repository's `target/test-scratch`, which is outside the system
/// temporary directory: the lookup leaves that directory out whatever a test records,
/// so a fixture there could not show a directory being left out for the reason under test.
fn fresh(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("checkout-in-search-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory.canonicalize().unwrap()
}

#[cfg(unix)]
fn executable(directory: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(directory).unwrap();
    let file = directory.join(name);
    std::fs::write(&file, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

#[cfg(windows)]
fn executable(directory: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(directory).unwrap();
    let file = directory.join(name);
    std::fs::write(&file, "").unwrap();
    file
}

fn path_of(entries: &[&Path]) -> OsString {
    std::env::join_paths(entries).unwrap()
}

/// The directories the first `count` entries of `with_checkout` name. The fixtures sit under this
/// repository, whose own root follows them in the list.
fn first(directory: &Path, count: usize) -> Vec<PathBuf> {
    with_checkout(directory).into_iter().take(count).collect()
}

/// The directory a command ran in and the checkout holding it are both left out of the lookup.
/// Rejects a lookup that leaves out only the directory itself, which finds a repository's `bin`
/// when the command runs in a subdirectory.
#[test]
fn the_checkout_holding_the_directory_is_left_out_with_it() {
    let root = fresh("holding");
    let here = root.join("crates").join("cli");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();

    assert_eq!(first(&here, 2), [here.clone(), root]);
}

/// A directory under no checkout is left out by itself. Rejects a lookup that leaves out the
/// checkout alone, which finds the `bin` of a project that has no `.git`.
#[test]
fn a_directory_under_no_checkout_is_left_out_by_itself() {
    let here = Path::new("/bravebot-no-such-directory/project");

    assert_eq!(with_checkout(here), [here.to_path_buf()]);
}

/// A submodule has a `.git` file and sits in an outer repository whose `.git` is a directory. The
/// submodule is the checkout holding the directory, so it is left out with the directory and the
/// outer repository is not. This pins the choice of the nearest `.git` (R8): rejects a walk that
/// takes every ancestor with a `.git`, which also leaves out the `bin` of a repository that merely
/// contains the checkout, and one that reads only a `.git` directory, which skips the submodule's
/// own root and finds the outer repository instead.
#[test]
fn the_nearest_checkout_is_the_one_left_out_with_the_directory() {
    let outer = fresh("submodule");
    let inner = outer.join("vendor").join("lib");
    let here = inner.join("src");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(outer.join(".git")).unwrap();
    std::fs::write(inner.join(".git"), "gitdir: ../../.git/modules/lib\n").unwrap();

    assert_eq!(first(&here, 2), [here.clone(), inner.clone()]);
    assert!(!with_checkout(&here).contains(&outer));
    let path = path_of(&[&outer.join("bin")]);
    executable(&outer.join("bin"), "xclip");
    assert!(
        find_in(OsStr::new("xclip"), &path, &with_checkout(&here)).is_some(),
        "the outer repository's bin is searched"
    );
}

/// A directory that has a `.git` is not listed twice: the walk that finds it does not add it again.
/// Rejects a walk that starts at the directory itself and records it a second time.
#[test]
fn the_checkout_root_is_listed_once() {
    let root = fresh("root");
    std::fs::create_dir_all(root.join(".git")).unwrap();

    let held = with_checkout(&root);

    assert_eq!(held.iter().filter(|held| **held == root).count(), 1);
}

/// A directory that holds its own `.git` is the checkout holding it, so nothing above it is added,
/// even when a parent holds a `.git` too. Rejects a walk that skips the directory's own `.git` and
/// takes the next ancestor, which leaves out the `bin` of a repository that merely contains the
/// checkout.
#[test]
fn a_directory_that_holds_its_own_git_leaves_out_only_itself() {
    let parent = fresh("own-git-parent");
    let here = parent.join("work");
    std::fs::create_dir_all(here.join(".git")).unwrap();
    std::fs::create_dir_all(parent.join(".git")).unwrap();
    let installed = executable(&parent.join("bin"), "xclip");
    let path = path_of(&[&parent.join("bin")]);

    assert_eq!(with_checkout(&here), std::slice::from_ref(&here));
    assert_eq!(
        find_in(OsStr::new("xclip"), &path, &with_checkout(&here)),
        Some(installed),
        "the parent's bin is searched"
    );
}

/// The reproduction of the issue. A session opened in a subdirectory left out that subdirectory
/// alone, so `xclip` planted in the `bin` of the checkout above it, first on `PATH`, was found.
/// With the set the function returns the program is not found. Rejects a set that holds the
/// subdirectory and no checkout.
#[test]
fn a_program_in_the_bin_of_the_checkout_above_the_directory_is_not_found() {
    let repo = fresh("reproduction");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let planted = executable(&repo.join("bin"), "xclip");
    let path = path_of(&[&repo.join("bin")]);

    assert_eq!(
        find_in(OsStr::new("xclip"), &path, std::slice::from_ref(&here)),
        Some(planted),
        "the directory alone does not leave out the bin of the checkout holding it"
    );
    assert_eq!(find_in(OsStr::new("xclip"), &path, &first(&here, 2)), None);
}

/// A bin outside every checkout is still searched when the directory is opened in a checkout.
/// Rejects a set that leaves out more than the directory and the checkouts holding it.
#[test]
fn a_program_in_a_bin_outside_the_checkout_is_still_found() {
    let repo = fresh("control-repo");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    executable(&repo.join("bin"), "xclip");
    let elsewhere = fresh("control-elsewhere");
    let installed = executable(&elsewhere.join("bin"), "xclip");
    let path = path_of(&[&repo.join("bin"), &elsewhere.join("bin")]);

    assert_eq!(
        find_in(OsStr::new("xclip"), &path, &first(&here, 2)),
        Some(installed)
    );
}

/// A session at the root of the checkout leaves its `bin` out too, so opening it one directory
/// deeper changes nothing about the answer.
#[test]
fn the_checkout_root_and_a_directory_below_it_leave_out_the_same_bin() {
    let repo = fresh("root-and-below");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    executable(&repo.join("bin"), "xclip");
    let path = path_of(&[&repo.join("bin")]);

    // The root is left out by itself; the directory below it with the checkout holding it.
    for (opened, count) in [(&repo, 1), (&here, 2)] {
        assert_eq!(
            find_in(OsStr::new("xclip"), &path, &first(opened, count)),
            None
        );
    }
}

/// The issue's reproduction through the lookup every start uses ([`find_on`], which reads what was
/// recorded) and a fixture outside the system temporary directory, which the lookup always leaves
/// out. With nothing recorded for the directory a command ran in, a program in the `bin` of the
/// checkout above it is found, which is the defect. After the command's own record is made, it is
/// not. Rejects a record that holds the directory and not the checkout (the program is still
/// found), and a lookup that leaves out more than the recorded set (the control program elsewhere
/// is no longer found).
#[test]
fn a_command_that_recorded_its_directory_does_not_find_the_checkouts_bin() {
    let repo = fresh("find-on-reproduction");
    let here = repo.join("crates").join("cli");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let planted = executable(&repo.join("bin"), "bravebot-planted-program");
    let elsewhere = fresh("find-on-reproduction-elsewhere");
    let installed = executable(&elsewhere.join("bin"), "bravebot-planted-program");
    let program = OsStr::new("bravebot-planted-program");
    let path = path_of(&[&repo.join("bin"), &elsewhere.join("bin")]);

    assert_eq!(
        find_on(program, &path),
        Some(planted),
        "nothing recorded: the checkout's bin is searched first"
    );

    keep_out_with_checkout(&here);

    assert_eq!(
        find_on(program, &path),
        Some(installed),
        "recorded: the checkout's bin is skipped and the one after it is found"
    );
}

/// A value of a variable that names a program or a file is judged against the same record as a
/// lookup, so one in the `bin` of the checkout holding a recorded directory is withheld
/// (SANDBOX-30). The controls are a file in a checkout nothing recorded and one outside every
/// checkout, which show the judgment is not refusing everything. Rejects a judgment made against a
/// set of its own, and one that holds the recorded directory without its checkout.
#[test]
fn a_value_naming_a_file_in_the_bin_of_the_checkout_holding_a_recorded_directory_is_withheld() {
    use bravebot_sandbox::programs::{Names, may_be_run, outside_stage_writable};

    let repo = fresh("may-be-run-recorded");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let planted = executable(&repo.join("bin"), "browser");
    let unrecorded = fresh("may-be-run-unrecorded");
    std::fs::create_dir_all(unrecorded.join(".git")).unwrap();
    let in_unrecorded = executable(&unrecorded.join("bin"), "browser");
    let installed = executable(&fresh("may-be-run-installed").join("bin"), "browser");

    keep_out_with_checkout(&here);

    for names in [Names::Program, Names::File] {
        assert!(
            !may_be_run(names, &planted, &outside_stage_writable),
            "{names:?} accepted a file in the bin of the checkout holding the recorded directory"
        );
        assert!(
            may_be_run(names, &in_unrecorded, &outside_stage_writable),
            "{names:?} refused a file in a checkout nothing was recorded for"
        );
        assert!(
            may_be_run(names, &installed, &outside_stage_writable),
            "{names:?} refused a file outside every checkout"
        );
    }
    let list = std::env::join_paths([&installed, &planted]).unwrap();
    assert!(
        !may_be_run(Names::Commands, Path::new(&list), &outside_stage_writable),
        "a list with an entry in the checkout's bin was accepted"
    );
}

/// Recording a directory records the checkout holding it, which is what the lookups of a running
/// session read. Rejects a function that returns the set and records only the directory.
#[test]
fn keeping_a_directory_out_keeps_the_checkout_holding_it_out() {
    let repo = fresh("recorded");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    keep_out_with_checkout(&here);

    let recorded = registered();
    assert!(recorded.contains(&here), "the directory itself");
    assert!(recorded.contains(&repo), "the checkout holding it");
}

/// Mercurial and Jujutsu checkouts are not recognised. A checkout is the nearest ancestor
/// holding a `.git`, so a directory under a `.hg` or `.jj` repository is left out by itself. This
/// pins that, and no support is added: the `.hg` repository above is not left out, so a `bin` in
/// it is searched. Rejects a walk that also treats `.hg` or `.jj` as a checkout marker, which
/// would change what is left out without a spec clause for it.
#[test]
fn a_mercurial_or_jujutsu_checkout_is_not_recognised() {
    for marker in [".hg", ".jj"] {
        let repo = fresh(&format!("not-recognised{marker}"));
        let here = repo.join("sub");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(repo.join(marker)).unwrap();
        let installed = executable(&repo.join("bin"), "xclip");
        let path = path_of(&[&repo.join("bin")]);

        assert_eq!(first(&here, 1), std::slice::from_ref(&here), "{marker}");
        assert!(!with_checkout(&here).contains(&repo), "{marker}");
        assert_eq!(
            find_in(OsStr::new("xclip"), &path, &first(&here, 1)),
            Some(installed),
            "the bin of a {marker} repository above the directory is searched"
        );
    }
}
