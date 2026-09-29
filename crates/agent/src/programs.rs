//! Working out which binary a program name means.
//!
//! `bravebot_core::programs::TrustedPrograms` is keyed by resolved path rather than by the name a
//! planner typed, and the reason is the one `bravebot_core::pure` states: a name is not a program.
//! `$PATH` decides what `grep` means, and on the machine this was developed against it means
//! `ugrep`, a different implementation with a far larger option surface. An approval recorded
//! against the string would follow the name onto whatever it later pointed at.
//!
//! So resolution happens once, before the person is asked, and it settles two paths. One is the
//! file the name reaches. The other is the path it was found by, made absolute, which is what
//! starts, because some programs read the path they were started by
//! (`bravebot_core::command::Step::started_as`). The person is shown both, the list records both,
//! and both are fixed before the question, so a `$PATH` changed after the approval changes nothing
//! that runs.
//!
//! This crate does the looking up because `bravebot-core` performs no I/O.

use std::path::{Path, PathBuf};

/// Which file a program name means, and the path that reaches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The path the name was found by, made absolute with no link in it followed.
    pub started_as: PathBuf,
    /// The file `started_as` leads to, canonical.
    pub resolved: PathBuf,
}

/// Work out which file `program` names, or `None` if nothing usable was found.
pub fn resolve(program: &str, working: &Path) -> Option<PathBuf> {
    find(program, working).map(|found| found.resolved)
}

/// Work out which file `program` names and the path it is found by, or `None` if nothing usable
/// was found.
///
/// A name with no separator in it is looked up in `$PATH`. A name with one is taken as a path,
/// relative to `working` rather than to this process's own directory: the stage is going to run in
/// `working`, so that is the directory `./script.sh` means to whoever wrote it.
///
/// Both paths are absolute, so neither the lookup nor the directory is asked again at the start.
pub fn find(program: &str, working: &Path) -> Option<Found> {
    if program.is_empty() {
        return None;
    }

    if has_separator(program) {
        let candidate = if Path::new(program).is_absolute() {
            PathBuf::from(program)
        } else {
            working.join(program)
        };
        return usable(&candidate);
    }

    for directory in std::env::split_paths(&std::env::var_os("PATH")?) {
        // An empty entry in $PATH means the current directory, which is a long-standing footgun:
        // it would let a file in the workspace shadow a system program. Skipped rather than
        // honoured, since nothing here needs it and the surprise is all downside.
        if directory.as_os_str().is_empty() {
            continue;
        }
        for name in candidates(program) {
            if let Some(found) = usable(&directory.join(&name)) {
                return Some(found);
            }
        }
    }
    None
}

/// Whether the name is a path rather than something to look up.
pub(crate) fn has_separator(program: &str) -> bool {
    program.contains('/') || (cfg!(windows) && program.contains('\\'))
}

/// The filenames to try for one program name.
///
/// One on unix. On Windows a bare name may mean any of the extensions in `%PATHEXT%`, and the name
/// as given is tried first so an extension already written out is not doubled.
///
/// Public because anything that has to find the same file under the name it was asked for has to
/// try the same spellings: a second list would answer differently for the name that matters.
pub fn candidates(program: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![program.to_string()];
    }
    let mut names = vec![program.to_string()];
    if let Some(pathext) = std::env::var_os("PATHEXT") {
        for extension in pathext.to_string_lossy().split(';') {
            let extension = extension.trim();
            if !extension.is_empty() {
                names.push(format!("{program}{extension}"));
            }
        }
    }
    names
}

/// `candidate` and the file it leads to, if that is a file this user could execute.
///
/// The file is canonicalised, so a path reached through `..` or a symlink is shown as the file it
/// actually is.
fn usable(candidate: &Path) -> Option<Found> {
    let resolved = candidate.canonicalize().ok()?;
    if !resolved.is_file() {
        return None;
    }
    if !executable(&resolved) {
        return None;
    }
    Some(Found {
        started_as: started_as(candidate, &resolved)?,
        resolved,
    })
}

/// The path `candidate` starts by, made absolute. `std::path::absolute` follows no link and on
/// unix keeps a `..`, which after a link means what the link's target makes it mean.
#[cfg(unix)]
fn started_as(candidate: &Path, _resolved: &Path) -> Option<PathBuf> {
    std::path::absolute(candidate).ok()
}

/// The file itself. On Windows `canonicalize` gives a `\\?\` path in the case the file has on
/// disk, so no path a name is found by would equal it, and a virtual environment's `python.exe` is
/// a copy in the environment rather than a link.
#[cfg(not(unix))]
fn started_as(_candidate: &Path, resolved: &Path) -> Option<PathBuf> {
    Some(resolved.to_path_buf())
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Windows has no executable bit; being a file with a known extension is as far as it goes.
#[cfg(not(unix))]
fn executable(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ordinary case: a bare name is found on `$PATH`, as an absolute path.
    #[test]
    fn a_program_on_the_path_resolves_to_an_absolute_file() {
        let found = resolve("sh", Path::new("/")).expect("sh is on the path");
        assert!(found.is_absolute());
        assert!(found.is_file());
    }

    #[test]
    fn a_program_that_is_not_installed_resolves_to_nothing() {
        assert!(resolve("bravebot-no-such-program-anywhere", Path::new("/")).is_none());
    }

    #[test]
    fn an_empty_name_resolves_to_nothing() {
        assert!(resolve("", Path::new("/")).is_none());
    }

    /// A name with a separator is a path, and it means what it means from the directory the stage
    /// will run in rather than from wherever this process happens to be.
    #[test]
    fn a_relative_path_resolves_against_the_working_directory() {
        let scratch = crate::testutil::scratch_dir("bravebot-programs-relative");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let script = scratch.join("tool.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let found = resolve("./tool.sh", &scratch).expect("resolves in the working directory");
        assert_eq!(found, script.canonicalize().unwrap());
        assert!(
            resolve("./tool.sh", Path::new("/")).is_none(),
            "a relative path resolved against the wrong directory"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A file that cannot be executed is not a program, so it is not something to record an
    /// approval against.
    #[cfg(unix)]
    #[test]
    fn a_file_without_the_executable_bit_is_not_a_program() {
        let scratch = crate::testutil::scratch_dir("bravebot-programs-notexec");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let plain = scratch.join("notes.txt");
        std::fs::write(&plain, "not a program").unwrap();

        assert!(resolve("./notes.txt", &scratch).is_none());
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Two spellings of one binary are shown as the file they both reach, and each is started by
    /// the path it was written as.
    #[test]
    fn a_path_through_a_traversal_resolves_to_the_same_file() {
        let direct = resolve("sh", Path::new("/")).expect("sh is on the path");
        let parent = direct.parent().expect("sh has a directory");
        let roundabout = parent
            .join("..")
            .join(parent.file_name().expect("the directory has a name"))
            .join("sh");
        let through = find(roundabout.to_string_lossy().as_ref(), Path::new("/"))
            .expect("the same file by a longer road");
        assert_eq!(through.resolved, direct);
        #[cfg(unix)]
        assert_eq!(through.started_as, roundabout);
        #[cfg(not(unix))]
        assert_eq!(through.started_as, through.resolved);
    }

    /// A link is started by its own path and resolves to the file it leads to.
    #[cfg(unix)]
    #[test]
    fn a_link_is_started_by_its_own_path() {
        let scratch = crate::testutil::scratch_dir("bravebot-programs-link");
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let script = scratch.join("tool.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::os::unix::fs::symlink(&script, scratch.join("link")).unwrap();

        let found = find("./link", &scratch).expect("the link leads to a program");
        assert_eq!(found.resolved, script.canonicalize().unwrap());
        assert_eq!(
            found.started_as,
            std::path::absolute(scratch.join("./link")).unwrap()
        );
        assert_eq!(found.started_as.file_name(), Some("link".as_ref()));
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
