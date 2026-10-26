//! Where the user's own files live.
//!
//! `~/.bravebot` holds what should outlive a session: prompt history, session records, and now
//! standing instructions and skills. One definition of where that is, so the interface and the
//! agent cannot drift apart about it.
//!
//! It goes under the directory the platform states the user's profile is in, unless the person
//! names the directory itself in [`STATE_VARIABLE`]. There is deliberately no fallback past the
//! variables either of those is stated in. A machine naming nothing yields `None` and every caller
//! does without, because inventing a directory would mean reading files from somewhere the user
//! never chose, and this is the one place whose contents are trusted for being the user's own.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The name of the directory inside the user's home.
const DIRECTORY: &str = bravebot_sandbox::base::STATE_DIRECTORY;

/// The variables the platform states the user's profile directory in, in the order they answer.
///
/// `HOME` on either platform: it is the one Unix sets, and a Windows shell environment that sets one
/// has been told where the profile is. `USERPROFILE` is the one stock Windows sets, and is read
/// there only, since on Unix it is not a name the platform states anything in.
#[cfg(windows)]
pub const PROFILE_VARIABLES: &[&str] = &["HOME", "USERPROFILE"];
#[cfg(not(windows))]
pub const PROFILE_VARIABLES: &[&str] = &["HOME"];

/// The variable that names the state directory itself, in place of `.bravebot` under the profile
/// directory.
///
/// A person sets it in their own environment, so it states a location the way `HOME` does, and it
/// is read from the environment alone: no settings file, which a checkout can supply, names the one
/// directory whose contents are trusted. It answers before the profile variables. A value that is
/// empty or is not an absolute path names nothing and is passed over, as an empty `HOME` is, since
/// a relative one would name a directory under wherever the session happened to start.
pub const STATE_VARIABLE: &str = "BRAVEBOT_HOME";

/// [`STATE_VARIABLE`] and then [`PROFILE_VARIABLES`]: every variable that can state where the state
/// directory is, in the order they answer. What `doctor` names when there is no directory.
#[cfg(windows)]
pub const STATE_VARIABLES: &[&str] = &[STATE_VARIABLE, "HOME", "USERPROFILE"];
#[cfg(not(windows))]
pub const STATE_VARIABLES: &[&str] = &[STATE_VARIABLE, "HOME"];

/// Whether [`STATE_VARIABLE`] holds something that names no directory and so is passed over.
///
/// An unset or empty variable is the ordinary case and is not reported. A value that is set and is
/// not an absolute path is somebody's attempt to move the directory that did nothing, which `doctor`
/// says, since the state directory then reads as the profile directory's and nothing else does.
pub fn passed_over() -> bool {
    holds_a_value_passed_over(std::env::var_os(STATE_VARIABLE).as_deref())
}

/// [`passed_over`] for a value handed in, so the rule needs no process-wide variable to test.
fn holds_a_value_passed_over(stated: Option<&std::ffi::OsStr>) -> bool {
    stated.is_some_and(|stated| !stated.is_empty() && !Path::new(stated).is_absolute())
}

/// The user's own directory, or `None` when nothing names where to keep it.
///
/// This is the reading answer. Anything about to write wants [`writable`] instead.
pub fn directory() -> Option<PathBuf> {
    resolved().map(|(_, directory)| directory)
}

/// The same answer with the variable that gave it, or `None` when none of them did.
///
/// `doctor` reports which one answered, because more than one can and the one in force is what
/// somebody has to change to put the directory elsewhere.
pub fn resolved() -> Option<(&'static str, PathBuf)> {
    resolve_stated(std::env::var_os(STATE_VARIABLE), from_the_environment())
}

/// The user's profile directory itself, or `None` when the platform names nowhere.
///
/// The directory [`directory`] sits inside unless [`STATE_VARIABLE`] puts it elsewhere, and the
/// only answer here that is not about what this program keeps, so [`STATE_VARIABLE`] never changes
/// it. It is what a leading `~` stands for: a home-relative path somebody writes names a file in
/// their home, and answering with the state directory would send every one of them into
/// `~/.bravebot` instead (CMDLINE-4).
pub fn profile() -> Option<PathBuf> {
    named_profile(from_the_environment()).map(|(_, profile)| profile)
}

/// The variables the platform states a profile directory in, with what each holds here.
///
/// Read directly rather than taking a dependency for two variables. Both absent in some daemon
/// and container environments, which is a case that has to be handled anyway.
fn from_the_environment() -> impl Iterator<Item = (&'static str, Option<OsString>)> {
    PROFILE_VARIABLES
        .iter()
        .map(|variable| (*variable, std::env::var_os(variable)))
}

/// The same answer from the values rather than from the variables, `stated` being what
/// [`STATE_VARIABLE`] holds.
///
/// The directory it names is the answer as it stands. Only a value that names nothing falls through
/// to the profile directory, so no location is invented where none was stated.
fn resolve_stated(
    stated: Option<OsString>,
    named: impl IntoIterator<Item = (&'static str, Option<OsString>)>,
) -> Option<(&'static str, PathBuf)> {
    stated
        .map(PathBuf::from)
        .filter(|stated| stated.is_absolute())
        .map(|stated| (STATE_VARIABLE, stated))
        .or_else(|| resolve(named))
}

/// The answer under the profile directory, from the values rather than from the variables.
///
/// Split from the read so the order they answer in is testable without a process-wide variable. A
/// test that set one would have to take a lock against every other test in the binary, restore what
/// was there, and step outside safe Rust to do it, all to check a rule that is a function of a
/// couple of strings.
fn resolve(
    named: impl IntoIterator<Item = (&'static str, Option<OsString>)>,
) -> Option<(&'static str, PathBuf)> {
    named_profile(named).map(|(variable, profile)| (variable, profile.join(DIRECTORY)))
}

/// Which variable names the profile directory, and what it names, before `DIRECTORY` is joined on.
///
/// An empty value names nothing, so it is passed over rather than joined onto: joining would put the
/// user's own files in `/.bravebot`, and stopping there would lose a profile directory the platform
/// does name to a variable some shell exported empty.
fn named_profile(
    named: impl IntoIterator<Item = (&'static str, Option<OsString>)>,
) -> Option<(&'static str, PathBuf)> {
    named
        .into_iter()
        .filter_map(|(variable, value)| Some((variable, value?)))
        .find(|(_, value)| !value.is_empty())
        .map(|(variable, value)| (variable, Path::new(&value).to_path_buf()))
}

/// The user's own directory when something may be written into it, or `None` when nothing may be.
///
/// `None` for two different reasons that callers should treat the same way: there is no home to
/// write to, or the session is [incognito] and is not going to write to it. Both mean "do not
/// persist this", and every caller already handled the first, which is what makes the second cost
/// a line rather than a redesign.
///
/// # Why a second function rather than a flag inside the first
///
/// Reads must keep working in an incognito session: the chosen model, the theme, the standing
/// instructions and the credentials all come out of this directory, and a session that could not
/// read them would not be private but crippled. Making [`directory`] itself answer `None` would
/// have taken those away too. Splitting the question in two puts the mode exactly where it belongs,
/// on the writes, and makes each call site say which it is doing.
///
/// [incognito]: bravebot_core::incognito
pub fn writable() -> Option<PathBuf> {
    if bravebot_core::incognito::engaged() {
        return None;
    }
    directory()
}

/// The directory the platform keeps this user's disposable files in, or `None` where it names none.
///
/// Where a copy goes that has to be somewhere only this user can reach and outlives no more than a
/// prompt. Not the system temporary directory, which every program a session confines may write
/// (SANDBOX-12), and not [`directory`], which an incognito session adds nothing to. macOS names
/// `~/Library/Caches`, Windows `%LOCALAPPDATA%`, and every other platform `$XDG_CACHE_HOME` where
/// it is an absolute path and `~/.cache` where it is not.
pub fn cache() -> Option<PathBuf> {
    platform_cache()
}

#[cfg(target_os = "macos")]
fn platform_cache() -> Option<PathBuf> {
    profile().map(|profile| profile.join("Library").join("Caches"))
}

#[cfg(windows)]
fn platform_cache() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|named| !named.is_empty())
        .map(PathBuf::from)
}

#[cfg(all(not(target_os = "macos"), not(windows)))]
fn platform_cache() -> Option<PathBuf> {
    // A relative value is one the XDG specification says to ignore, since it would name a
    // directory under wherever the session happened to start.
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|named| named.is_absolute())
        .or_else(|| profile().map(|profile| profile.join(".cache")))
}

/// The single path segment standing for a working directory.
///
/// Separators become dashes and anything that is not a plain path character goes the same way, so
/// the name is one segment on every platform and readable in a directory listing. It is not
/// reversible, which is why a record keyed this way has to hold the real path as well, and why two
/// directories whose names reduce to the same segment are answered by one file.
///
/// One definition rather than one per subsystem: the session store and the record of command lines
/// somebody asked to be remembered are both kept per working directory, and two spellings of the
/// same key would put a session under one name and the answers given in it under another.
pub fn key_for(project: &Path) -> String {
    let mangled: String = project
        .display()
        .to_string()
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' => c,
            _ => '-',
        })
        .collect();

    // A path of only separators would otherwise name the directory holding the keys itself.
    if mangled.trim_matches('-').is_empty() {
        return "root".to_string();
    }
    mangled
}

/// Whether `path` is a directory by its own name, which a link to one is not.
fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.is_dir())
}

/// The plain files directly inside `directory` that were last written before `cutoff`.
///
/// What a sweep of the state directory chooses from, so the rules it keeps are written once. A link
/// standing as `directory`, or as an entry of it, is stepped over rather than followed: a sweep is
/// bounded by a comparison of names, which says where a name sits and nothing about where it leads.
/// A file whose time cannot be read, or is not before `cutoff`, stays, and a time later than now,
/// from a clock that was wrong when the file was written, is not old. Empty where `directory`
/// cannot be read.
///
/// Read-only: the caller removes what it takes from the answer, and so decides what removing one
/// means.
pub fn files_unwritten_since(
    directory: &Path,
    cutoff: std::time::SystemTime,
) -> Vec<std::fs::DirEntry> {
    if !is_real_directory(directory) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| {
            entry
                .metadata()
                .and_then(|found| found.modified())
                .is_ok_and(|written| written < cutoff)
        })
        .collect()
}

/// The directories directly inside `directory`, by their own names: a link to one is not listed.
///
/// For the same reason [`files_unwritten_since`] steps over links. Empty where `directory` is not a
/// directory of its own or cannot be read.
pub fn directories_in(directory: &Path) -> Vec<PathBuf> {
    if !is_real_directory(directory) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect()
}

/// Create `path` and everything between it and the state directory, reachable only by this user.
///
/// Every writer under `~/.bravebot` goes through this rather than `create_dir_all`, because a
/// directory keeps the mode it was made with and the state directory is made by whichever
/// subsystem happens to write first. Creating one at `0700` while another creates it at the
/// umask makes the mode of a directory holding prompt history a matter of call order.
///
/// An existing directory is tightened rather than left as it was found, which is what makes this
/// fix a machine that has already run an older build.
pub fn create_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        if let Some(root) = directory() {
            tighten(&root, path);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

/// Narrow `path` and every directory above it as far as `root`, both included.
///
/// Stops at `root` rather than walking to `/`: the directories above the state directory are the
/// user's own home and are none of this program's business.
///
/// A link is stepped over rather than followed. `set_permissions` resolves one, and the bound above
/// is a comparison of paths, which says where a name sits and nothing about where it leads: a
/// linked `sessions` directory would otherwise have this narrowing a directory somewhere else
/// entirely, which is the thing the bound exists to prevent.
#[cfg(unix)]
fn tighten(root: &Path, path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if !path.starts_with(root) {
        return;
    }
    let mut level = Some(path);
    while let Some(here) = level {
        if !is_link(here) {
            let _ = std::fs::set_permissions(here, std::fs::Permissions::from_mode(0o700));
        }
        if here == root {
            return;
        }
        level = here.parent();
    }
}

/// Whether the name itself is a link, rather than what it may lead to.
#[cfg(unix)]
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_symlink())
}

/// Write `contents` to `path`, readable only by this user.
///
/// The mode is asked for as the file is created rather than set once it is written. Several
/// callers write a temporary file and rename it over the real one, so a mode applied afterwards
/// would leave the contents readable for the length of the write, and the rename would carry the
/// temporary file's mode onto the real name anyway.
///
/// A file that is already there is tightened too, since one written by an older build has whatever
/// the umask gave it.
pub fn write_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = open_private(std::fs::OpenOptions::new().write(true).truncate(true), path)?;
    file.write_all(contents)
}

/// Write `text` over `path` through a temporary file beside it, so an interrupted write leaves the
/// file as it was and a reader never sees half of it.
///
/// The temporary file is named for this process and this write, since two sessions writing at once
/// would otherwise write into one file and rename whatever the pair of them left there. It is
/// removed when the write or the rename fails.
pub fn replace(path: &Path, text: impl AsRef<[u8]>) -> std::io::Result<()> {
    static WRITES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.{write}.tmp", std::process::id()));
    let temporary = std::path::PathBuf::from(temporary);
    let written =
        write_file(&temporary, text.as_ref()).and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Write `contents` to `path` so that a reader sees the old file or the new one and never a part
/// of either.
///
/// The bytes go to a temporary file beside `path`, named for the process and for this call so two
/// writers in one process do not share one, and the rename puts them in place.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".{name}.{}.{call}.tmp", std::process::id()));
    let written = write_file(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// Write `contents` to a file that is not there yet, readable only by this user.
///
/// Refuses, with `AlreadyExists`, a name that is taken, a link included, so a writer that must
/// never replace a file asks for this and not for [`write_file`].
pub fn create_new_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut file = open_private(
        std::fs::OpenOptions::new().write(true).create_new(true),
        path,
    )?;
    file.write_all(contents)
}

/// Open `path` to add to the end of it, readable only by this user.
///
/// Appending rather than rewriting is worth a second opening for: the history file is added to
/// once per prompt, and rewriting it would make that cost grow with how much history there is.
pub fn append_to_file(path: &Path) -> std::io::Result<std::fs::File> {
    open_private(std::fs::OpenOptions::new().append(true), path)
}

/// Open a file reachable only by this user.
///
/// Almost always one under the state directory. The exception is an exported transcript, which
/// SESSION-17 gives the same mode as the record it was recounted from.
fn open_private(options: &mut std::fs::OpenOptions, path: &Path) -> std::io::Result<std::fs::File> {
    options.create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    Ok(file)
}

/// Which of the variables the platform states a profile directory in decides the answer.
///
/// A function of the values, so both orders are checked on every platform the resolver is compiled
/// for rather than only on the one whose variable happens to be set.
#[cfg(test)]
mod resolution {
    use super::*;

    /// A variable as the environment hands one over.
    fn named(variable: &'static str, value: &str) -> (&'static str, Option<OsString>) {
        (variable, Some(OsString::from(value)))
    }

    /// A directory a person could name in [`STATE_VARIABLE`], absolute where the test runs.
    #[cfg(windows)]
    const NAMED_DIRECTORY: &str = "C:\\srv\\ci\\state";
    #[cfg(not(windows))]
    const NAMED_DIRECTORY: &str = "/srv/ci/state";

    /// STATE-2: stock Windows sets no `HOME`, so a state directory there is the profile directory the
    /// platform does name or nothing at all. Nothing at all is a session that works once and forgets:
    /// no settings of the user's own, no session to resume, no history, and no skills.
    #[test]
    fn the_profile_directory_answers_where_no_home_is_named() {
        let profile = Path::new("C:\\Users\\someone");
        assert_eq!(
            resolve([("HOME", None), named("USERPROFILE", "C:\\Users\\someone")]),
            Some(("USERPROFILE", profile.join(DIRECTORY)))
        );
        assert_eq!(
            resolve([
                named("HOME", ""),
                named("USERPROFILE", "C:\\Users\\someone")
            ]),
            Some(("USERPROFILE", profile.join(DIRECTORY))),
            "a variable exported empty took away a profile directory the platform names"
        );
    }

    /// SANDBOX-12 refuses a stage the state directory by the name the sandbox crate holds. A second
    /// spelling here would leave the directory the files are written to unrefused.
    #[test]
    fn the_state_directory_is_the_one_the_sandbox_refuses() {
        let refused = bravebot_sandbox::base::run_base(
            bravebot_sandbox::base::Prelude::Linux,
            Path::new("/scratch"),
            Some(Path::new("/somebody")),
        )
        .unreadable;
        let (_, state) = resolve([named("HOME", "/somebody")]).expect("a home");

        assert!(refused.contains(&state), "{state:?} not in {refused:?}");
    }

    /// STATE-2: a shell environment that sets `HOME` has been told where the profile is, and every
    /// other tool run from it reads that. A state directory somewhere else would leave the settings
    /// and history of one session unreachable from the next, in whichever shell it was started from.
    #[test]
    fn a_named_home_answers_before_the_profile_directory() {
        assert_eq!(
            resolve([
                named("HOME", "/somebody"),
                named("USERPROFILE", "C:\\Users\\someone")
            ]),
            Some(("HOME", PathBuf::from("/somebody").join(DIRECTORY)))
        );
    }

    /// STATE-2: the directory a person names is the state directory, not a profile directory to put
    /// `.bravebot` under. Joining the name on would put a CI job's state in a hidden directory
    /// inside the one it was given, and the person who set the variable would not find it where
    /// they pointed.
    #[test]
    fn a_directory_named_in_the_environment_is_the_state_directory_itself() {
        assert_eq!(
            resolve_stated(
                Some(OsString::from(NAMED_DIRECTORY)),
                [named("HOME", "/somebody")]
            ),
            Some((STATE_VARIABLE, PathBuf::from(NAMED_DIRECTORY)))
        );
    }

    /// STATE-2: a person who set the variable has said where the state is, and a `HOME` that is also
    /// set is the profile directory, not the state. Reading them the other way round would leave the
    /// variable doing nothing on every machine that has a `HOME`, which is every machine this is
    /// set on.
    #[test]
    fn a_directory_named_in_the_environment_answers_before_the_profile_directory() {
        assert_eq!(
            resolve_stated(
                Some(OsString::from(NAMED_DIRECTORY)),
                [
                    named("HOME", "/somebody"),
                    named("USERPROFILE", "C:\\Users\\someone")
                ]
            ),
            Some((STATE_VARIABLE, PathBuf::from(NAMED_DIRECTORY)))
        );
    }

    /// STATE-2: a value that is empty, or that is not an absolute path, names no directory, so the
    /// profile directory answers as though it were unset. A relative one would name a directory under
    /// wherever the session started, which is a location nobody stated.
    #[test]
    fn a_value_that_names_no_directory_is_passed_over() {
        let under_home = Some(("HOME", PathBuf::from("/somebody").join(DIRECTORY)));
        for value in ["", "state", "./state", "../state", "~/state", "$HOME/state"] {
            assert_eq!(
                resolve_stated(Some(OsString::from(value)), [named("HOME", "/somebody")]),
                under_home,
                "{value:?} was taken as a directory"
            );
        }
        assert_eq!(
            resolve_stated(None, [named("HOME", "/somebody")]),
            under_home,
            "an unset variable changed the profile directory's answer"
        );
    }

    /// STATE-2: a directory a person names is the state directory byte for byte, and one whose name is
    /// not UTF-8 is not the directory its U+FFFD rendering spells. A resolver that went through text
    /// would answer with the second, or pass the first over as naming nothing and let `HOME` answer,
    /// and either way the history of a job given its own state would land in a directory it was not
    /// given.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_not_text_is_the_state_directory_and_not_its_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid = OsStr::from_bytes(b"/srv/state-\xff");
        let lookalike = OsStr::new("/srv/state-\u{FFFD}");
        let answer = |stated: &OsStr| {
            resolve_stated(Some(stated.to_os_string()), [named("HOME", "/somebody")])
        };

        assert_eq!(
            answer(invalid),
            Some((STATE_VARIABLE, PathBuf::from(invalid)))
        );
        assert_eq!(
            answer(lookalike),
            Some((STATE_VARIABLE, PathBuf::from(lookalike)))
        );
        assert_ne!(answer(invalid), answer(lookalike));
    }

    /// `doctor` blames a value that was passed over, so a directory the resolver takes is never
    /// blamed. One whose name is not UTF-8 is such a directory, and a report that read the value as
    /// text would call it not a directory and send its owner looking for a mistake they did not make.
    #[cfg(unix)]
    #[test]
    fn a_directory_that_is_not_text_is_not_reported_as_passed_over() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        for value in [
            OsStr::from_bytes(b"/srv/state-\xff"),
            OsStr::new("/srv/state-\u{FFFD}"),
        ] {
            assert!(
                !holds_a_value_passed_over(Some(value)),
                "{value:?} was reported as naming no directory"
            );
        }
    }

    /// STATE-2: passing a value over leaves the machine as it was, and a machine that names nothing
    /// else has no state directory. The refusal of a guessed location is the clause's reason for
    /// existing, and a working-directory or temporary fallback behind a bad value would be the case
    /// it rules out.
    #[test]
    fn a_value_that_names_no_directory_leaves_no_state_directory_to_guess() {
        for value in ["", "state", "./state", "~/state"] {
            assert_eq!(
                resolve_stated(Some(OsString::from(value)), [("HOME", None)]),
                None,
                "{value:?} produced a location nothing stated"
            );
        }
    }

    /// `doctor` says when the variable was set and named nothing, and says nothing for the ordinary
    /// cases: unset, empty, or a directory. A report that warned of an empty value would fire in every
    /// environment that exports the variable blank, and one that missed a relative value leaves the
    /// person who typed one with a state directory they did not choose and no word of why.
    #[test]
    fn a_value_that_names_no_directory_is_reported_as_passed_over() {
        let held = |value: &str| holds_a_value_passed_over(Some(std::ffi::OsStr::new(value)));
        for value in ["state", "./state", "../state", "~/state", "$HOME/state"] {
            assert!(held(value), "{value:?} was not reported");
        }
        for value in ["", NAMED_DIRECTORY] {
            assert!(!held(value), "{value:?} was reported");
        }
        assert!(
            !holds_a_value_passed_over(None),
            "an unset variable was reported"
        );
    }

    /// `doctor` names the variables that were looked at when there is no directory, so the list is
    /// the one the resolver reads, in the order it reads them: a variable the resolver consults and
    /// the report omits sends somebody to set the wrong one.
    #[test]
    fn the_variables_a_report_names_are_the_ones_read() {
        let mut read = vec![STATE_VARIABLE];
        read.extend(PROFILE_VARIABLES);
        assert_eq!(STATE_VARIABLES, read.as_slice());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A scratch directory that removes itself.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self {
                path: path.canonicalize().expect("canonical scratch"),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777
    }

    fn loosen(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("loosen");
    }

    /// The prompt history is every path, branch name and pasted fragment somebody has typed. On a
    /// shared machine the umask decides who else can read it, and the umask is not this program's
    /// to rely on.
    #[test]
    fn a_directory_is_created_reachable_only_by_its_owner() {
        let scratch = Scratch::new("home-create");
        let made = scratch.path.join("state");
        create_directory(&made).expect("created");

        assert_eq!(mode_of(&made), 0o700);
    }

    /// A machine that has already run an older build has the directory at whatever the umask gave
    /// it, and creating it again would leave it there: `create_dir_all` succeeds without touching
    /// the mode of something that already exists.
    #[test]
    fn a_directory_left_open_by_an_older_build_is_narrowed() {
        let scratch = Scratch::new("home-tighten");
        let state = scratch.path.join("state");
        let inside = state.join("sessions");
        std::fs::create_dir_all(&inside).expect("create");
        loosen(&state, 0o755);
        loosen(&inside, 0o755);

        tighten(&state, &inside);

        assert_eq!(mode_of(&inside), 0o700);
        assert_eq!(
            mode_of(&state),
            0o700,
            "the directory above it was left open"
        );
    }

    /// Whose home this is, and what else is in it, is the user's business. Walking past the state
    /// directory would have this program narrowing directories it was never asked about.
    #[test]
    fn narrowing_stops_at_the_state_directory() {
        let scratch = Scratch::new("home-stops");
        let state = scratch.path.join("state");
        std::fs::create_dir_all(&state).expect("create");
        loosen(&scratch.path, 0o755);

        tighten(&state, &state);

        assert_eq!(mode_of(&state), 0o700);
        assert_eq!(
            mode_of(&scratch.path),
            0o755,
            "a directory above the state directory"
        );
    }

    /// The bound is a comparison of paths, so it says where a name sits and nothing about where it
    /// leads. Somebody who keeps their sessions on a synced volume and links the directory into
    /// place would otherwise have this program setting the mode of a directory outside the one it
    /// was given.
    #[test]
    fn narrowing_does_not_follow_a_link_out_of_the_state_directory() {
        let scratch = Scratch::new("home-link");
        let state = scratch.path.join("state");
        let elsewhere = scratch.path.join("elsewhere");
        std::fs::create_dir_all(&state).expect("create");
        std::fs::create_dir_all(&elsewhere).expect("create");
        let linked = state.join("sessions");
        std::os::unix::fs::symlink(&elsewhere, &linked).expect("link");
        loosen(&elsewhere, 0o755);

        tighten(&state, &linked);

        assert_eq!(
            mode_of(&elsewhere),
            0o755,
            "a directory outside the state directory was narrowed through a link"
        );
        assert_eq!(mode_of(&state), 0o700, "the state directory itself");
    }

    #[test]
    fn a_file_is_written_readable_only_by_its_owner() {
        let scratch = Scratch::new("home-write");
        let file = scratch.path.join("history");
        write_file(&file, b"a prompt\n").expect("written");

        assert_eq!(mode_of(&file), 0o600);
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "a prompt\n");
    }

    #[test]
    fn a_replaced_file_is_whole_and_private_and_a_failed_one_leaves_nothing_beside_it() {
        let scratch = Scratch::new("home-replace");
        let file = scratch.path.join("model");
        replace(&file, "one\n").expect("written");
        replace(&file, "two\n").expect("replaced");

        assert_eq!(std::fs::read_to_string(&file).expect("read"), "two\n");
        assert_eq!(mode_of(&file), 0o600);

        // A directory in the way makes the rename fail, after the temporary file was written.
        let blocked = scratch.path.join("blocked");
        std::fs::create_dir_all(blocked.join("inside")).expect("blocker");
        assert!(replace(&blocked, "x").is_err());
        let mut names: Vec<_> = std::fs::read_dir(&scratch.path)
            .expect("listed")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("utf8")
            })
            .collect();
        names.sort();
        assert_eq!(names, ["blocked", "model"]);
    }

    /// The file an older build left is the one holding the history worth protecting, so writing it
    /// again has to narrow it rather than keep the mode it was found with.
    #[test]
    fn a_file_left_readable_by_an_older_build_is_narrowed() {
        let scratch = Scratch::new("home-rewrite");
        let file = scratch.path.join("history");
        std::fs::write(&file, "old").expect("write");
        loosen(&file, 0o644);

        write_file(&file, b"new").expect("written");

        assert_eq!(mode_of(&file), 0o600);
    }

    #[test]
    fn an_appended_file_is_readable_only_by_its_owner() {
        let scratch = Scratch::new("home-append");
        let file = scratch.path.join("history");
        let mut opened = append_to_file(&file).expect("opened");
        opened.write_all(b"one\n").expect("written");

        assert_eq!(mode_of(&file), 0o600);
    }
    /// STATE-4: a sweep chooses from plain files by their own names and times. A file written inside
    /// the period, a directory as old as the file and a link all stay out of the answer, so a caller
    /// that removes what it is given cannot remove any of them.
    #[test]
    fn only_old_plain_files_are_named_as_unwritten() {
        let scratch = Scratch::new("home-unwritten");
        let old = scratch.path.join("old");
        let new = scratch.path.join("new");
        std::fs::write(&old, "old").expect("write");
        std::fs::write(&new, "new").expect("write");
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(40 * 86_400);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&old)
            .expect("open")
            .set_modified(then)
            .expect("age the file");
        let directory = scratch.path.join("directory");
        std::fs::create_dir(&directory).expect("a directory");
        std::fs::File::open(&directory)
            .expect("open the directory")
            .set_modified(then)
            .expect("age the directory");
        std::os::unix::fs::symlink(&old, scratch.path.join("linked")).expect("link");

        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
        let named: Vec<_> = files_unwritten_since(&scratch.path, cutoff)
            .into_iter()
            .map(|entry| entry.path())
            .collect();

        assert_eq!(named, [old]);
    }

    /// STATE-1: a link standing as the directory itself leads outside what a sweep was given.
    #[test]
    fn a_link_standing_as_the_directory_names_nothing() {
        let scratch = Scratch::new("home-linked-directory");
        let elsewhere = scratch.path.join("elsewhere");
        std::fs::create_dir_all(elsewhere.join("inside")).expect("create");
        let old = elsewhere.join("old");
        std::fs::write(&old, "old").expect("write");
        let linked = scratch.path.join("linked");
        std::os::unix::fs::symlink(&elsewhere, &linked).expect("link");

        let cutoff = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(files_unwritten_since(&linked, cutoff).is_empty());
        assert!(directories_in(&linked).is_empty());
        assert_eq!(directories_in(&elsewhere), [elsewhere.join("inside")]);
    }

    /// STATE-1: a link standing as an entry is not a directory of its own.
    #[test]
    fn a_link_standing_as_an_entry_is_not_listed_as_a_directory() {
        let scratch = Scratch::new("home-linked-entry");
        let elsewhere = scratch.path.join("elsewhere");
        let held = scratch.path.join("held");
        std::fs::create_dir_all(&elsewhere).expect("create");
        std::fs::create_dir_all(&held).expect("create");
        std::os::unix::fs::symlink(&elsewhere, held.join("linked")).expect("link");

        assert!(directories_in(&held).is_empty());
    }

    #[test]
    fn two_writers_never_leave_a_torn_file() {
        let scratch = Scratch::new("home-atomic");
        let path = scratch.path.join("file");
        let payloads = [vec![b'a'; 10_000], vec![b'b'; 10_000]];
        write_atomically(&path, &payloads[0]).expect("first write");

        std::thread::scope(|scope| {
            for payload in &payloads {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..200 {
                        write_atomically(path, payload).expect("write");
                    }
                });
            }
            for _ in 0..400 {
                let read = std::fs::read(&path).expect("read");
                assert!(payloads.contains(&read), "torn: {} bytes", read.len());
            }
        });
    }
}
