//! Where the user's own files live.
//!
//! `~/.bravebot` holds what should outlive a session: prompt history, session records, and now
//! standing instructions and skills. One definition of where that is, so the interface and the
//! agent cannot drift apart about it.
//!
//! It goes under the directory the platform states the user's profile is in, and there is
//! deliberately no fallback past the variables the platform states that in. A platform naming
//! nothing yields `None` and every caller does without, because inventing a directory would mean
//! reading files from somewhere the user never chose, and this is the one place whose contents are
//! trusted for being the user's own.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The name of the directory inside the user's home.
const DIRECTORY: &str = ".bravebot";

/// The variables the platform states the user's profile directory in, in the order they answer.
///
/// `HOME` on either platform: it is the one Unix sets, and a Windows shell environment that sets one
/// has been told where the profile is. `USERPROFILE` is the one stock Windows sets, and is read
/// there only, since on Unix it is not a name the platform states anything in.
#[cfg(windows)]
pub const PROFILE_VARIABLES: &[&str] = &["HOME", "USERPROFILE"];
#[cfg(not(windows))]
pub const PROFILE_VARIABLES: &[&str] = &["HOME"];

/// The user's own directory, or `None` when the platform names nowhere to keep it.
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
    resolve(from_the_environment())
}

/// The user's profile directory itself, or `None` when the platform names nowhere.
///
/// The directory [`directory`] sits inside, and the only answer here that is not about what this
/// program keeps. It is what a leading `~` stands for: a home-relative path somebody writes names
/// a file in their home, and answering with the state directory would send every one of them into
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

/// The same answer from the values rather than from the variables.
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
}
