//! Finding a program bravebot starts on its own account, without finding one a confined stage wrote.
//!
//! A program bravebot starts itself (`aws`, a clipboard tool, `osascript`, `systemd-inhibit`) is
//! not confined, and found by name through an inherited `PATH` it is the first executable on it. An
//! activated virtualenv puts a directory a session may write at the front, so the name could
//! resolve to a file the session produced.
//!
//! Holding those directories back from the session's write grants would break `pip install` in an
//! activated environment. So the lookup leaves them out instead: it searches only absolute `PATH`
//! entries that lie outside every directory a session may write, and a program found only in one
//! of the others is not installed, as far as bravebot is concerned.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Directories a confined stage may write, for the life of the process.
///
/// Only ever added to. Closing a directory does not remove a file already written into it, and
/// the lookup runs far from the code that opens one.
static WRITABLE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Directories a stage may write only where the account itself can, for the life of the process.
///
/// A stage given every path the account can write ([SANDBOX-28]) holds `/usr` as a row though the
/// account owns none of it. Recording the row as a writable directory would leave `/usr/bin`
/// out of every search, and a clipboard tool with it. What a stage can plant is what the account
/// can write, so these are compared with that.
///
/// [SANDBOX-28]: ../../../docs/specs/sandboxing.md
static WRITABLE_WHERE_THE_ACCOUNT_CAN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Record a directory a confined stage may write.
///
/// Both the path as given and its resolved form are kept, because a `PATH` entry may reach the
/// same directory by either: `/var` and `/private/var` are one place.
pub fn keep_out(directory: &Path) {
    let mut writable = WRITABLE.lock().unwrap_or_else(|e| e.into_inner());
    for form in forms(directory) {
        if !writable.contains(&form) {
            writable.push(form);
        }
    }
}

/// Record a path a stage may write wherever the account can, in both of its spellings.
pub fn keep_out_where_writable(directory: &Path) {
    let mut held = WRITABLE_WHERE_THE_ACCOUNT_CAN
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for form in forms(directory) {
        if !held.contains(&form) {
            held.push(form);
        }
    }
}

/// The program as an absolute path, or `None` when the only one `PATH` offers is in a directory
/// a stage may write. A name that is already an absolute path is returned as it is.
pub fn find(program: &OsStr) -> Option<PathBuf> {
    find_on(program, &std::env::var_os("PATH").unwrap_or_default())
}

/// [`find`] against a given `PATH`, leaving out every directory recorded with [`keep_out`] or
/// [`keep_out_where_writable`].
pub fn find_on(program: &OsStr, path: &OsStr) -> Option<PathBuf> {
    find_in_with(program, path, &registered(), &held_where_the_account_can())
}

/// Every path recorded with [`keep_out_where_writable`] so far.
///
/// Copied out, so the lock is not held while every entry on `PATH` is read.
fn held_where_the_account_can() -> Vec<PathBuf> {
    WRITABLE_WHERE_THE_ACCOUNT_CAN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Make the names a started program looks up search only the entries [`find`] searches.
///
/// [`find`] chooses the file that is started, but a started program goes on to look up names of its
/// own through the `PATH` it inherits: an `env` shebang, a wrapper script, the `vim` in an
/// `$EDITOR` of `env vim`. So the command is given the process's `PATH` with every entry [`find`]
/// would leave out removed, in the order it was written.
///
/// Where no entry remains the command gets no `PATH` variable at all. `execvp` reads an empty
/// `PATH` as the current directory, which is the directory a session writes in.
pub fn restrict_path(command: &mut Command) -> &mut Command {
    restrict_path_with(
        command,
        &std::env::var_os("PATH").unwrap_or_default(),
        &registered(),
        &held_where_the_account_can(),
    )
}

/// [`restrict_path`] against a given `PATH` and sets of writable directories.
fn restrict_path_with<'a>(
    command: &'a mut Command,
    path: &OsStr,
    writable: &[PathBuf],
    where_the_account_can: &[PathBuf],
) -> &'a mut Command {
    match searched_path(path, writable, where_the_account_can) {
        Some(searched) => command.env("PATH", searched),
        None => command.env_remove("PATH"),
    }
}

/// The entries of `path` that [`find_in_with`] searches, joined as a `PATH`, or `None` when it
/// searches none of them.
fn searched_path(
    path: &OsStr,
    writable: &[PathBuf],
    where_the_account_can: &[PathBuf],
) -> Option<OsString> {
    let out = kept_out(writable);
    let searched: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|entry| is_searched(entry, &out, where_the_account_can))
        .collect();
    if searched.is_empty() {
        return None;
    }
    std::env::join_paths(searched).ok()
}

/// Every directory recorded with [`keep_out`] so far.
pub fn registered() -> Vec<PathBuf> {
    WRITABLE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// [`find`] against a given `PATH` and set of writable directories.
///
/// The system temp directory is always among the writable ones: a stage is granted it. No row
/// recorded with [`keep_out_where_writable`] is left out, so a program bravebot starts is found with
/// [`find`] or [`find_on`] instead.
pub fn find_in(program: &OsStr, path: &OsStr, writable: &[PathBuf]) -> Option<PathBuf> {
    find_in_with(program, path, writable, &[])
}

/// [`find_in`] with the directories a stage may write only where the account can as well.
///
/// A directory, or a program in it, is left out when it lies within one of `where_the_account_can`
/// and the account can write it or the directory holding it.
pub fn find_in_with(
    program: &OsStr,
    path: &OsStr,
    writable: &[PathBuf],
    where_the_account_can: &[PathBuf],
) -> Option<PathBuf> {
    // An absolute path is the caller's own choice and `PATH` cannot redirect it. Any other name
    // with a separator is relative to wherever the process happens to be, which is no choice at all.
    if Path::new(program).is_absolute() {
        return Some(PathBuf::from(program));
    }
    if Path::new(program).components().count() != 1 {
        return None;
    }
    let out = kept_out(writable);

    for entry in std::env::split_paths(path) {
        if !is_searched(&entry, &out, where_the_account_can) {
            continue;
        }
        for name in names(program) {
            let candidate = entry.join(&name);
            let Ok(resolved) = candidate.canonicalize() else {
                continue;
            };
            // A link in a safe directory that points into a writable one is the writable file.
            if !usable(&resolved)
                || within_any(&resolved, &out)
                || planted(&resolved, where_the_account_can)
            {
                continue;
            }
            return Some(candidate);
        }
    }
    None
}

/// Every directory a stage may write, in both of its spellings, with the system temporary
/// directory among them: a stage is granted it.
fn kept_out(writable: &[PathBuf]) -> Vec<PathBuf> {
    // Nothing is created here: the path only names a directory to leave out.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temporary = std::env::temp_dir();
    let mut out: Vec<PathBuf> = writable.iter().flat_map(|d| forms(d)).collect();
    out.extend(forms(&temporary));
    out
}

/// Whether a `PATH` entry is one the lookup searches. [`restrict_path`] keeps the same entries for a
/// started program's own lookups, so the two cannot differ on which entries are left out. A `PATH`
/// names directories, so the check on the file a lookup resolves to has no counterpart there.
fn is_searched(entry: &Path, out: &[PathBuf], where_the_account_can: &[PathBuf]) -> bool {
    entry.is_absolute() && !within_any(entry, out) && !planted(entry, where_the_account_can)
}

/// Whether `candidate` lies within a row a stage holds where the account can write, and the account
/// can write it or the directory holding it.
fn planted(candidate: &Path, where_the_account_can: &[PathBuf]) -> bool {
    within_any(candidate, where_the_account_can)
        && (the_account_can_write(candidate)
            || candidate.parent().is_some_and(the_account_can_write))
}

/// Whether the account this process runs as can write `path`.
///
/// Asked of the platform and not read from the mode bits, which leave out ownership, access lists
/// and a read-only volume.
#[cfg(unix)]
#[allow(unsafe_code)]
fn the_account_can_write(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(name) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe { libc::access(name.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
fn the_account_can_write(_path: &Path) -> bool {
    true
}

/// The path, and its resolved form where that differs and can be had.
fn forms(directory: &Path) -> Vec<PathBuf> {
    let mut forms = vec![directory.to_path_buf()];
    if let Ok(resolved) = directory.canonicalize()
        && resolved != directory
    {
        forms.push(resolved);
    }
    forms
}

fn within_any(path: &Path, directories: &[PathBuf]) -> bool {
    // The entry itself may be a link into a writable directory, so it is compared as written and as
    // resolved.
    let resolved = path.canonicalize().ok();
    directories.iter().any(|directory| {
        within(path, directory)
            || resolved
                .as_deref()
                .is_some_and(|resolved| within(resolved, directory))
    })
}

fn within(path: &Path, directory: &Path) -> bool {
    let mut inside = path.components();
    for expected in directory.components() {
        match inside.next() {
            Some(found) if same_component(found, expected) => {}
            _ => return false,
        }
    }
    true
}

/// Whether two components name one place on a case-insensitive filesystem.
///
/// Lossy, which only ever errs towards excluding a directory.
#[cfg(any(windows, target_os = "macos"))]
fn same_component(a: Component<'_>, b: Component<'_>) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn same_component(a: Component<'_>, b: Component<'_>) -> bool {
    a == b
}

/// The file names the program may have in a directory.
///
/// A name with no extension is not a program on Windows, so it is tried only with each of
/// `PATHEXT`'s: a pip script called `aws` beside an `aws.exe` is not the one to start.
#[cfg(windows)]
fn names(program: &OsStr) -> Vec<OsString> {
    if Path::new(program).extension().is_some() {
        return vec![program.to_os_string()];
    }
    let extensions =
        std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    let mut names = Vec::new();
    for extension in extensions.to_string_lossy().split(';') {
        let extension = extension.trim();
        if !extension.is_empty() {
            let mut name = program.to_os_string();
            name.push(extension);
            names.push(name);
        }
    }
    names
}

#[cfg(not(windows))]
fn names(program: &OsStr) -> Vec<OsString> {
    vec![program.to_os_string()]
}

#[cfg(unix)]
fn usable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn usable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with nothing in it, so a file left by an earlier run is not found.
    fn scratch_dir(name: &str) -> PathBuf {
        let directory = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[cfg(unix)]
    fn executable(directory: &Path, name: impl AsRef<OsStr>) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(directory).unwrap();
        let file = directory.join(name.as_ref());
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        file
    }

    #[cfg(windows)]
    fn executable(directory: &Path, name: impl AsRef<OsStr>) -> PathBuf {
        std::fs::create_dir_all(directory).unwrap();
        let file = directory.join(name.as_ref());
        std::fs::write(&file, "").unwrap();
        file
    }

    fn path_of(entries: &[&Path]) -> OsString {
        std::env::join_paths(entries).unwrap()
    }

    #[test]
    fn a_program_in_a_directory_a_stage_may_write_is_not_found() {
        let session = scratch_dir("programs-session");
        executable(&session.join("venv").join("bin"), "aws");
        let path = path_of(&[&session.join("venv").join("bin")]);

        assert_eq!(
            find_in(OsStr::new("aws"), &path, std::slice::from_ref(&session)),
            None
        );
    }

    #[test]
    fn the_program_behind_a_writable_directory_is_the_one_found() {
        let session = scratch_dir("programs-behind");
        let planted = executable(&session.join("venv").join("bin"), "aws");
        let installed = executable(&scratch_dir("programs-behind-system"), "aws");
        let path = path_of(&[planted.parent().unwrap(), installed.parent().unwrap()]);

        assert_eq!(
            find_in(OsStr::new("aws"), &path, std::slice::from_ref(&session)),
            Some(installed)
        );
    }

    #[test]
    fn a_program_is_found_when_no_writable_directory_is_involved() {
        let system = scratch_dir("programs-plain");
        let installed = executable(&system, "aws");

        assert_eq!(
            find_in(OsStr::new("aws"), &path_of(&[&system]), &[]),
            Some(installed)
        );
    }

    #[test]
    fn a_relative_path_entry_is_never_searched() {
        let system = scratch_dir("programs-relative");
        executable(&system, "aws");
        // Tests run from the crate's directory, two below the workspace's `target`.
        let relative = Path::new("../../target/test-scratch/programs-relative");
        assert!(
            relative
                .join(if cfg!(windows) { "aws.exe" } else { "aws" })
                .exists(),
            "the relative entry reaches the program, so only the rule leaves it out"
        );

        assert_eq!(find_in(OsStr::new("aws"), &path_of(&[relative]), &[]), None);
    }

    #[test]
    fn an_empty_path_entry_is_never_searched() {
        // An empty entry means the current directory to a shell.
        let path = OsString::from(if cfg!(windows) { ";" } else { ":" });

        assert_eq!(find_in(OsStr::new("aws"), &path, &[]), None);
    }

    #[test]
    fn the_system_temp_directory_counts_as_writable() {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temporary = std::env::temp_dir().join("bravebot-programs-temp");
        executable(&temporary, "aws");
        let found = find_in(OsStr::new("aws"), &path_of(&[&temporary]), &[]);
        let _ = std::fs::remove_dir_all(&temporary);

        assert_eq!(found, None);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_a_safe_directory_to_a_writable_file_is_not_found() {
        let session = scratch_dir("programs-link-session");
        let planted = executable(&session, "aws");
        let safe = scratch_dir("programs-link-safe");
        std::os::unix::fs::symlink(&planted, safe.join("aws")).unwrap();

        assert_eq!(
            find_in(
                OsStr::new("aws"),
                &path_of(&[&safe]),
                std::slice::from_ref(&session)
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_a_writable_directory_to_a_safe_program_is_not_found() {
        let session = scratch_dir("programs-retarget-session");
        let installed = executable(&scratch_dir("programs-retarget-system"), "aws");
        std::os::unix::fs::symlink(&installed, session.join("aws")).unwrap();

        assert_eq!(
            find_in(
                OsStr::new("aws"),
                &path_of(&[&session]),
                std::slice::from_ref(&session)
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_entry_that_links_into_a_writable_directory_is_not_searched() {
        let session = scratch_dir("programs-entry-session");
        executable(&session.join("bin"), "aws");
        let elsewhere = scratch_dir("programs-entry-elsewhere");
        let entry = elsewhere.join("bin");
        std::os::unix::fs::symlink(session.join("bin"), &entry).unwrap();

        assert_eq!(
            find_in(
                OsStr::new("aws"),
                &path_of(&[&entry]),
                std::slice::from_ref(&session)
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_sibling_directory_sharing_a_name_prefix_is_not_writable() {
        let parent = scratch_dir("programs-prefix");
        let session = parent.join("work");
        std::fs::create_dir_all(&session).unwrap();
        let installed = executable(&parent.join("work-tools"), "aws");

        assert_eq!(
            find_in(
                OsStr::new("aws"),
                &path_of(&[installed.parent().unwrap()]),
                std::slice::from_ref(&session)
            ),
            Some(installed)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_the_executable_bit_is_not_a_program() {
        let system = scratch_dir("programs-not-executable");
        std::fs::write(system.join("aws"), "").unwrap();

        assert_eq!(find_in(OsStr::new("aws"), &path_of(&[&system]), &[]), None);
    }

    #[test]
    fn an_absolute_path_is_the_callers_own_choice() {
        let fixed = executable(&scratch_dir("programs-absolute"), "aws");

        assert_eq!(find_in(fixed.as_os_str(), OsStr::new(""), &[]), Some(fixed));
    }

    #[test]
    fn a_relative_name_with_a_separator_is_not_looked_up() {
        let system = scratch_dir("programs-separator");
        executable(&system, "aws");

        assert_eq!(
            find_in(OsStr::new("./aws"), &path_of(&[&system]), &[]),
            None
        );
    }

    // APFS refuses a name that is not UTF-8.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_non_utf8_directory_is_compared_by_its_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let parent = scratch_dir("programs-bytes");
        let writable = parent.join(OsStr::from_bytes(b"w\xff"));
        let lookalike = parent.join(OsStr::from_bytes(b"w\xfe"));
        executable(&writable, "aws");
        let installed = executable(&lookalike, "aws");

        assert_eq!(
            find_in(
                OsStr::new("aws"),
                &path_of(&[&writable, &lookalike]),
                std::slice::from_ref(&writable)
            ),
            Some(installed)
        );
    }

    #[test]
    fn a_registered_directory_is_kept_out_of_the_search() {
        let session = scratch_dir("programs-registered");
        executable(&session.join("bin"), "bravebot-registered-probe");
        keep_out(&session);
        let path = path_of(&[&session.join("bin")]);

        assert_eq!(
            find_in(
                OsStr::new("bravebot-registered-probe"),
                &path,
                &registered()
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_in_a_row_the_account_can_write_is_not_found() {
        let session = scratch_dir("programs-where-it-can");
        executable(&session.join("bin"), "aws");
        let path = path_of(&[&session.join("bin")]);

        assert_eq!(
            find_in_with(
                OsStr::new("aws"),
                &path,
                &[],
                std::slice::from_ref(&session)
            ),
            None
        );
    }

    /// SANDBOX-28: a program under a row a stage holds is still found where the account cannot
    /// write it, its directory, or the directory holding that. The regression it rejects is every
    /// program under such a row left out, which drops `/usr/bin` from the lookup.
    #[cfg(unix)]
    #[test]
    fn a_program_in_a_row_the_account_cannot_write_is_still_found() {
        use std::os::unix::fs::PermissionsExt;
        let set = |path: &Path, mode| {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        };
        let row = crate::testutil::scratch_dir("programs-where-it-cannot");
        let held = row.join("held");
        let bin = held.join("bin");
        // A run that stopped before restoring these would leave the scratch directory unremovable.
        set(&held, 0o755);
        set(&bin, 0o755);
        let row = scratch_dir("programs-where-it-cannot");
        let tool = executable(&bin, "tool");
        for path in [&tool, &bin, &held] {
            set(path, 0o555);
        }
        let read_only = [&tool, &bin, &held]
            .iter()
            .all(|path| !the_account_can_write(path));

        let found = find_in_with(
            OsStr::new("tool"),
            &path_of(&[&bin]),
            &[],
            std::slice::from_ref(&row),
        );
        set(&bin, 0o755);
        set(&held, 0o755);

        // Root writes a directory whatever its mode, so there is no directory to test with.
        if !read_only {
            return;
        }
        assert_eq!(found, Some(tool));
    }

    /// SANDBOX-30: a caller searching a `PATH` of its own gets the rows a stage given the whole
    /// machine may write left out too, so a stage that replaced a program in a Homebrew prefix the
    /// account owns has not chosen what bravebot starts. The program behind it is the one found,
    /// which tells this apart from a lookup that finds nothing.
    #[cfg(unix)]
    #[test]
    fn a_search_of_a_given_path_leaves_out_a_row_held_where_the_account_can_write() {
        let prefix = scratch_dir("programs-on-path-held");
        let planted = executable(&prefix.join("bin"), "bravebot-held-probe");
        let installed = executable(
            &scratch_dir("programs-on-path-behind"),
            "bravebot-held-probe",
        );
        keep_out_where_writable(&prefix);
        let path = path_of(&[planted.parent().unwrap(), installed.parent().unwrap()]);

        assert_eq!(
            find_on(OsStr::new("bravebot-held-probe"), &path),
            Some(installed)
        );
    }

    /// SANDBOX-28: a row a stage given the whole machine holds stays searched where the account
    /// cannot write it, through a caller's own `PATH` as through the process's. Counting such a row
    /// as written outright would leave `/usr/bin` out, and the `aws` installed there with it.
    #[cfg(unix)]
    #[test]
    fn a_search_of_a_given_path_still_finds_a_program_in_a_held_row_the_account_cannot_write() {
        use std::os::unix::fs::PermissionsExt;
        let set = |path: &Path, mode| {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        };
        let row = crate::testutil::scratch_dir("programs-on-path-cannot");
        let held = row.join("held");
        let bin = held.join("bin");
        // A run that stopped before restoring these would leave the scratch directory unremovable.
        set(&held, 0o755);
        set(&bin, 0o755);
        let row = scratch_dir("programs-on-path-cannot");
        let tool = executable(&bin, "bravebot-held-read-only-probe");
        for path in [&tool, &bin, &held] {
            set(path, 0o555);
        }
        let read_only = [&tool, &bin, &held]
            .iter()
            .all(|path| !the_account_can_write(path));
        keep_out_where_writable(&row);

        let found = find_on(
            OsStr::new("bravebot-held-read-only-probe"),
            &path_of(&[&bin]),
        );
        set(&bin, 0o755);
        set(&held, 0o755);

        // Root writes a directory whatever its mode, so there is no directory to test with.
        if !read_only {
            return;
        }
        assert_eq!(found, Some(tool));
    }

    /// SANDBOX-30: the `PATH` a started program looks its own names up in holds the entries the
    /// lookup searches, in the order written, and no entry the lookup leaves out. A probe program
    /// in every entry ties the two together: the lookup finds it in an entry exactly when that entry
    /// is kept, so neither side can change which entries it leaves out without the other.
    #[cfg(unix)]
    #[test]
    fn a_started_programs_path_holds_exactly_the_entries_the_lookup_searches() {
        let parent = scratch_dir("programs-restrict");
        let session = parent.join("work");
        let held = parent.join("held");
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let shared = std::env::temp_dir();
        let temporary = shared.join(format!("bravebot-programs-restrict-{}", std::process::id()));
        let elsewhere = scratch_dir("programs-restrict-elsewhere");
        let linked = elsewhere.join("bin");
        std::fs::create_dir_all(session.join("bin")).unwrap();
        std::os::unix::fs::symlink(session.join("bin"), &linked).unwrap();
        executable(
            &scratch_dir("programs-restrict-relative"),
            "bravebot-restrict-probe",
        );
        // Tests run from the crate's directory, two below the workspace's `target`.
        let relative = Path::new("../../target/test-scratch/programs-restrict-relative");
        assert!(
            relative.join("bravebot-restrict-probe").exists(),
            "the relative entry reaches the probe, so only the rule leaves it out"
        );

        // (the entry, whether the lookup searches it, why)
        let entries: Vec<(PathBuf, bool, &str)> = vec![
            (
                session.join(".venv/bin"),
                false,
                "a directory a stage may write",
            ),
            (
                parent.join("work-tools"),
                true,
                "a sibling that shares a name prefix",
            ),
            (relative.to_path_buf(), false, "a relative entry"),
            (PathBuf::new(), false, "an empty entry"),
            (temporary.clone(), false, "the system temporary directory"),
            (linked, false, "a link into a directory a stage may write"),
            (
                held.join("bin"),
                false,
                "a row held where the account can write",
            ),
            (parent.join("tools"), true, "a directory outside every one"),
        ];
        for (entry, _, _) in entries.iter().filter(|(entry, _, _)| entry.is_absolute()) {
            executable(entry, "bravebot-restrict-probe");
        }

        let all = std::env::join_paths(entries.iter().map(|(entry, _, _)| entry)).unwrap();
        let writable = std::slice::from_ref(&session);
        let held_row = std::slice::from_ref(&held);
        let kept: Vec<&PathBuf> = entries
            .iter()
            .filter(|(_, searched, _)| *searched)
            .map(|(entry, _, _)| entry)
            .collect();

        let path_kept = searched_path(&all, writable, held_row);
        let mut command = Command::new("true");
        restrict_path_with(&mut command, &all, writable, held_row);
        let set: Vec<_> = command
            .get_envs()
            .filter(|(key, _)| *key == OsStr::new("PATH"))
            .collect();
        let found: Vec<_> = entries
            .iter()
            .filter(|(entry, _, _)| !entry.as_os_str().is_empty())
            .map(|(entry, searched, why)| {
                let probe = find_in_with(
                    OsStr::new("bravebot-restrict-probe"),
                    &path_of(&[entry]),
                    writable,
                    held_row,
                );
                (probe.is_some() == *searched, *why)
            })
            .collect();
        let _ = std::fs::remove_dir_all(&temporary);

        let expected = std::env::join_paths(kept).unwrap();
        assert_eq!(path_kept.as_deref(), Some(expected.as_os_str()));
        assert_eq!(set, [(OsStr::new("PATH"), Some(expected.as_os_str()))]);
        for (agrees, why) in found {
            assert!(
                agrees,
                "the lookup and the started program's PATH differ on {why}"
            );
        }
    }

    /// SANDBOX-30: an entry that is not UTF-8 is compared and carried as the bytes it has. A `PATH`
    /// built from a lossy rendering would merge `w\xff`, which a stage may write, with `w\xfe`, which
    /// it may not, and hand the started program `w\u{FFFD}`, a directory that is neither. No file is
    /// made: the names are only compared, and APFS refuses them as file names in any case.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_started_programs_path_carries_an_entry_that_is_not_utf8_as_its_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let parent = scratch_dir("programs-restrict-bytes");
        let writable = parent.join(OsStr::from_bytes(b"w\xff"));
        let lookalike = parent.join(OsStr::from_bytes(b"w\xfe"));
        let path = path_of(&[&writable.join("bin"), &lookalike]);

        assert_eq!(
            searched_path(&path, std::slice::from_ref(&writable), &[]),
            Some(lookalike.into_os_string())
        );
    }

    /// SANDBOX-30: with no entry left the started program gets no `PATH`. An empty one is read as
    /// the current directory by `execvp`, which for a session is a directory it writes.
    #[test]
    fn a_started_program_with_no_searched_entry_is_given_no_path_at_all() {
        let session = scratch_dir("programs-restrict-none");
        let path = path_of(&[&session.join("venv").join("bin")]);

        let mut command = Command::new("true");
        restrict_path_with(&mut command, &path, std::slice::from_ref(&session), &[]);

        let set: Vec<_> = command
            .get_envs()
            .filter(|(key, _)| *key == OsStr::new("PATH"))
            .collect();
        assert_eq!(
            set,
            [(OsStr::new("PATH"), None)],
            "the PATH was left alone or set to a value instead of being removed"
        );
    }
}
