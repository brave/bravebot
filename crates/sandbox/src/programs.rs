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

/// The name a program is known by, from the file it resolved to: the file's name with a Windows
/// executable extension (`.exe` or `.com`) removed and the rest folded to lower case, so
/// `cargo.exe` and `CARGO.EXE` are `cargo`. A name with no such extension is returned as it is,
/// since a file system that keeps case keeps it apart.
///
/// Every lookup that keys on a resolved file (a toolchain list, a credential scope, a program that
/// talks to a remote) goes through this, since the file `PATHEXT` finds on Windows keeps its
/// extension and a bare name never matches it. `None` for a path with no name or one that is not
/// text.
pub fn program_name(resolved: &Path) -> Option<String> {
    let name = resolved.file_name()?.to_str()?;
    let stem = name
        .len()
        .checked_sub(4)
        .and_then(|start| Some((name.get(..start)?, name.get(start..)?)))
        .filter(|(stem, extension)| {
            !stem.is_empty()
                && (extension.eq_ignore_ascii_case(".exe")
                    || extension.eq_ignore_ascii_case(".com"))
        })
        .map(|(stem, _)| stem.to_ascii_lowercase());
    Some(stem.unwrap_or_else(|| name.to_string()))
}

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

/// A program bravebot may start, and the only thing [`start`] takes.
///
/// The field is private, so a value comes from one of the constructors below and each of those
/// says who chose the program: [`Resolved::find`] and [`Resolved::find_on`] search only the
/// directories SANDBOX-30 allows, and the rest name a program that needs no search. A call that
/// starts a program from a bare path or string does not compile, so a new start picks one in the
/// diff, where a reviewer reads it.
///
/// Neither a path nor a string is accepted, and the field cannot be written to by a caller:
///
/// ```compile_fail
/// use bravebot_sandbox::programs::{Inherits, SearchPath};
/// let path = std::path::PathBuf::from("/usr/bin/git");
/// let _ = bravebot_sandbox::programs::start(&path, Inherits::All, SearchPath::Inherited);
/// ```
///
/// ```compile_fail
/// let _ = bravebot_sandbox::programs::Resolved(std::path::PathBuf::from("/usr/bin/git"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved(PathBuf);

impl Resolved {
    /// [`find`], as the type [`start`] takes.
    pub fn find(program: &OsStr) -> Option<Self> {
        find(program).map(Self)
    }

    /// [`find_on`], as the type [`start`] takes.
    pub fn find_on(program: &OsStr, path: &OsStr) -> Option<Self> {
        find_on(program, path).map(Self)
    }

    /// This program's own executable.
    pub fn own_executable() -> std::io::Result<Self> {
        // nosemgrep: rust.lang.security.current-exe.current-exe
        std::env::current_exe().map(Self)
    }

    /// A path the source of this program writes out, such as `/usr/bin/xcode-select`.
    pub fn written_in_source(program: &'static str) -> Self {
        Self(PathBuf::from(program))
    }

    /// A program a person named: their `$SHELL`, a hook's or an editor's path, the client after
    /// `--`, a credential authority they configured. A bare name given here is searched by the
    /// operating system, so a caller that takes one looks it up with [`Resolved::find`] first.
    pub fn named_by_person(program: impl Into<PathBuf>) -> Self {
        Self(program.into())
    }

    /// A path an earlier step returned and the caller kept: this program's own executable
    /// passed down to a supervisor, the program a `run` line was planned with, a language server the host resolved before the person was shown it, a
    /// stage's program.
    pub fn found_earlier(program: impl Into<PathBuf>) -> Self {
        Self(program.into())
    }

    /// The program the caller of a confinement backend asked to run inside it. The backend starts
    /// it already confined, so there is no unconfined lookup for it to skip.
    pub fn to_confine(program: impl Into<PathBuf>) -> Self {
        Self(program.into())
    }

    /// The path this will start.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// What of this process's environment a started program inherits.
///
/// There is no default: a start says which, so a new one cannot leave out the removal
/// [CRED-14](../../../docs/specs/credential-protection.md#CRED-14) asks of a program bravebot
/// starts on its own account. `bravebot-sandbox` depends on no crate, so the names a caller
/// withholds are its argument. The configuration owns which they are.
#[derive(Clone, Copy, Debug)]
pub enum Inherits<'a> {
    /// All of it. For a program that is run with the person's own environment, or holds nothing
    /// this agent authenticates with to hand on: a line typed at the `!` prompt, the editor
    /// Ctrl-G opens, a clipboard tool, the inhibitor, a system helper, and this program started
    /// again (the background host, `doctor` for a bug report). A program that runs what the
    /// planner chose or what a project carries is given [`Inherits::Without`]
    /// ([RUN-12](../../../docs/specs/tools/run.md#RUN-12)).
    All,
    /// All of it except these names, which are removed rather than emptied: a program that checks
    /// whether a variable is set would read an empty string as configured but blank.
    Without(&'a [String]),
    /// Only these names, each where this process holds it, and the `PATH` a [`SearchPath::Reduced`]
    /// start is given. Every other variable this process holds is removed by name, so the removals
    /// show in `Command::get_envs` for code that builds a confined program's environment from it.
    Only(&'a [&'a str]),
}

impl Inherits<'_> {
    /// Remove the names [`Inherits::Without`] holds from `command`, and do nothing for the others.
    ///
    /// [`start`] does this. A caller that then sets variables from input it does not control,
    /// such as the assignments a `run` line writes in front of a program, calls it again, so a
    /// withheld name stays withheld whatever the line assigned.
    pub fn withhold(&self, command: &mut Command) {
        if let Inherits::Without(names) = self {
            for name in *names {
                command.env_remove(name);
            }
        }
    }
}

/// The `PATH` a started program is given, which is where the names it starts itself are looked up.
///
/// [`find`] chooses the file that is started, and a started program goes on to look names up
/// through the `PATH` it inherits: an `env` shebang, a wrapper script, the `vim` in an `$EDITOR`
/// of `env vim`. Every start says which of these it gets
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchPath {
    /// This process's own `PATH`, unchanged. For a program that starts no other by name, or one
    /// meant to find what the project carries: a language server runs the project's interpreter.
    Inherited,
    /// Only the entries [`find`] searches, in the order they were written. Where none remains the
    /// program is given no `PATH` variable, since an empty one is read as the current directory.
    Reduced,
}

/// Whether `held`, the name of a variable this process holds, is `kept`. Windows reads a variable's
/// name without regard to case, so `Path` is `PATH` there.
fn is_variable(held: &OsStr, kept: &str) -> bool {
    if cfg!(windows) {
        held.eq_ignore_ascii_case(kept)
    } else {
        held == OsStr::new(kept)
    }
}

/// The one place a program is started.
///
/// `clippy.toml` refuses `Command::new` everywhere else, so every start reaches here with a
/// [`Resolved`] and a program named by a variable or a helper no longer slips past a check that
/// reads literals. It also reaches here with an [`Inherits`] and a [`SearchPath`], which have
/// no default, so what the program is handed shows in the diff that starts it.
///
/// The environment is settled before this returns: the `inherits` rule first, then the `path`. A
/// variable the caller sets on the command afterwards replaces what was removed here, so a caller
/// that sets variables from input it does not control calls [`Inherits::withhold`] again.
pub fn start(program: &Resolved, inherits: Inherits<'_>, path: SearchPath) -> Command {
    #[allow(
        clippy::disallowed_methods,
        reason = "the one start: its argument is a `Resolved`, which only the lookup and the \
                  named constructors build (SANDBOX-30)"
    )]
    let mut command = Command::new(&program.0);
    inherits.withhold(&mut command);
    if let Inherits::Only(names) = inherits {
        for (held, _) in std::env::vars_os() {
            if !names.iter().any(|kept| is_variable(&held, kept)) {
                command.env_remove(held);
            }
        }
    }
    if path == SearchPath::Reduced {
        restrict_path(&mut command);
    }
    command
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

/// Whether a file found in a `PATH` entry is one [`find_on`] would return: the entry is outside
/// every directory a stage may write, and so is the file once its links are followed.
///
/// For a caller that walks a `PATH` itself, as a Windows lookup that tries each of `PATHEXT` does.
pub fn outside_stage_writable(candidate: &Path) -> bool {
    outside_stage_writable_with(candidate, &registered(), &held_where_the_account_can())
}

/// [`outside_stage_writable`] against given sets of writable directories.
pub fn outside_stage_writable_with(
    candidate: &Path,
    writable: &[PathBuf],
    where_the_account_can: &[PathBuf],
) -> bool {
    let out = Rows::new(kept_out(writable));
    let held = Rows::new(where_the_account_can.to_vec());
    let Some(entry) = candidate.parent() else {
        return false;
    };
    let Ok(resolved) = candidate.canonicalize() else {
        return false;
    };
    is_searched(entry, &out, &held) && !within_any(&resolved, &out) && !planted(&resolved, &held)
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
///
/// Only [`start`] calls this, for [`SearchPath::Reduced`], so no start reduces the `PATH` a second
/// way or forgets to.
fn restrict_path(command: &mut Command) -> &mut Command {
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
    let out = Rows::new(kept_out(writable));
    let held = Rows::new(where_the_account_can.to_vec());
    let searched: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|entry| is_searched(entry, &out, &held))
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
    let out = Rows::new(kept_out(writable));
    let held = Rows::new(where_the_account_can.to_vec());

    for entry in std::env::split_paths(path) {
        if !is_searched(&entry, &out, &held) {
            continue;
        }
        for name in names(program) {
            let candidate = entry.join(&name);
            let Ok(resolved) = candidate.canonicalize() else {
                continue;
            };
            // A link in a safe directory that points into a writable one is the writable file.
            if !usable(&resolved) || within_any(&resolved, &out) || planted(&resolved, &held) {
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
fn is_searched(entry: &Path, out: &Rows, where_the_account_can: &Rows) -> bool {
    entry.is_absolute() && !within_any(entry, out) && !planted(entry, where_the_account_can)
}

/// Whether `candidate` lies within a row a stage holds where the account can write, and the account
/// can write it or the directory holding it.
fn planted(candidate: &Path, where_the_account_can: &Rows) -> bool {
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

/// Directories a lookup leaves out, with what each of them is when the lookup begins.
///
/// What a directory is is asked once for the lookup and not for every path compared with it. A
/// directory recorded after the lookup began is not in it, as it is not in the names.
struct Rows {
    directories: Vec<PathBuf>,
    identities: Vec<Identity>,
    identity: fn(&Path) -> Option<Identity>,
}

impl Rows {
    fn new(directories: Vec<PathBuf>) -> Self {
        Self::identified_by(directories, identity_of)
    }

    fn identified_by(directories: Vec<PathBuf>, identity: fn(&Path) -> Option<Identity>) -> Self {
        let identities = directories
            .iter()
            .filter_map(|directory| identity(directory))
            .collect();
        Self {
            directories,
            identities,
            identity,
        }
    }
}

/// Whether `path` lies within one of the rows, by the names and then by the directories they
/// reach.
///
/// A volume can open two spellings of a name as one directory: an ext4 or tmpfs directory with
/// `casefold`, vfat, a macOS firmlink, a bind mount. Resolving a path there returns the spelling
/// it was given, so a `PATH` entry written another way than the recorded directory has other names
/// and is the same directory. The names are compared first, which is all there is to compare for a
/// directory that no longer exists.
fn within_any(path: &Path, rows: &Rows) -> bool {
    if rows.directories.is_empty() {
        return false;
    }
    // The entry itself may be a link into a writable directory, so it is compared as written and as
    // resolved.
    let resolved = resolve_existing(path);
    let by_name = rows.directories.iter().any(|directory| {
        within(path, directory)
            || resolved
                .as_deref()
                .is_some_and(|resolved| within(resolved, directory))
    });
    if by_name {
        return true;
    }
    // Every directory above where the path leads. These are the directories a program in it is
    // inside of, however the path was spelled to reach them.
    !rows.identities.is_empty()
        && resolved
            .as_deref()
            .into_iter()
            .flat_map(Path::ancestors)
            .filter_map(rows.identity)
            .any(|reached| rows.identities.contains(&reached))
}

/// `path` with the links in the part of it that exists followed and the rest as written, or `None`
/// where no part of it resolves.
///
/// An entry that does not exist yet leads where its existing part does: a stage that may write the
/// directory the part leads into can make the rest.
fn resolve_existing(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    for ancestor in path.ancestors() {
        if let Ok(resolved) = ancestor.canonicalize() {
            return Some(
                missing
                    .iter()
                    .rev()
                    .fold(resolved, |resolved, name| resolved.join(name)),
            );
        }
        missing.push(ancestor.file_name()?);
    }
    None
}

/// The device and inode of a directory, which it has under every spelling.
type Identity = (u64, u64);

/// The identity of the directory `path` names now, or `None` where it names nothing or something
/// that is not a directory. Always `None` off Unix, where the file index is not in the standard
/// library.
#[cfg(unix)]
fn identity_of(path: &Path) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;
    let found = path.metadata().ok().filter(|found| found.is_dir())?;
    Some((found.dev(), found.ino()))
}

#[cfg(not(unix))]
fn identity_of(_path: &Path) -> Option<Identity> {
    None
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
    use super::{Inherits, Resolved, SearchPath, start};
    use std::ffi::OsStr;

    /// [`start`] with the environment and `PATH` left as they are.
    fn started(program: &Resolved) -> std::process::Command {
        start(program, Inherits::All, SearchPath::Inherited)
    }

    /// `start` runs the program its argument names. The regression it rejects: a start that
    /// ignores the `Resolved` it is given, which would make every caller's lookup decorative.
    #[test]
    fn start_is_given_the_program_the_resolved_names() {
        for resolved in [
            Resolved::written_in_source("/usr/bin/xcode-select"),
            Resolved::named_by_person("/opt/editor/bin/vim"),
            Resolved::found_earlier("/usr/bin/git"),
            Resolved::to_confine("/bin/cat"),
        ] {
            assert_eq!(
                started(&resolved).get_program(),
                resolved.as_path().as_os_str()
            );
        }
        let own = Resolved::own_executable().expect("a test has an executable");
        assert_eq!(started(&own).get_program(), own.as_path().as_os_str());
    }

    /// CRED-14: every name a start is told to withhold is removed, and no other variable is touched.
    /// The regressions it rejects: a start that ignores the names, one that removes only the first,
    /// and one that removes something of its own when it is given none.
    #[test]
    fn start_removes_the_names_it_is_given_and_no_others() {
        let program = Resolved::named_by_person("true");
        let removed = |inherits| {
            let mut removed: Vec<_> = start(&program, inherits, SearchPath::Inherited)
                .get_envs()
                .map(|(name, value)| (name.to_os_string(), value.is_none()))
                .collect();
            removed.sort();
            removed
        };
        let names = [
            "BRAVEBOT_PROBE_KEY".to_string(),
            "BRAVEBOT_PROBE_ID".to_string(),
        ];

        assert_eq!(
            removed(Inherits::Without(&names)),
            [
                (OsString::from("BRAVEBOT_PROBE_ID"), true),
                (OsString::from("BRAVEBOT_PROBE_KEY"), true),
            ]
        );
        assert_eq!(removed(Inherits::Without(&[])), []);
        assert_eq!(removed(Inherits::All), []);
    }

    /// A start told to hand a program only some names hands it those, where this process holds them,
    /// and nothing else. The regressions it rejects: a start that does not clear the environment,
    /// which leaves the rest of it in place, and one that clears it and passes nothing back.
    #[cfg(unix)]
    #[test]
    fn start_hands_a_program_only_the_names_it_is_given() {
        let names = ["PATH", "BRAVEBOT_PROBE_NOT_HELD"];

        assert_eq!(
            handed(Inherits::Only(&names), SearchPath::Inherited),
            ["PATH"]
        );
    }

    /// A held variable is a name a start was given only where the two are the same name. The
    /// regression it rejects: a comparison through `to_string_lossy`, which reads `NAME-\xff` as
    /// `NAME-\u{FFFD}` and so keeps a variable that was never asked for.
    #[cfg(unix)]
    #[test]
    fn a_variable_name_that_is_not_text_is_not_its_lossy_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let not_text = OsStr::from_bytes(b"NAME-\xff");
        let lookalike = "NAME-\u{FFFD}";

        assert!(!is_variable(not_text, lookalike));
        assert!(is_variable(OsStr::new(lookalike), lookalike));
    }

    /// A start given only some names reports the removal of every other variable the process holds
    /// through `get_envs`. The regression it rejects: clearing the environment with `env_clear`,
    /// which `get_envs` does not report, so code that builds a confined program's environment from
    /// this process's own and the command's `get_envs` hands the program all of it.
    #[test]
    fn a_start_given_only_some_names_reports_the_removal_of_the_rest_through_get_envs() {
        let kept = ["PATH"];
        let command = start(
            &Resolved::named_by_person("true"),
            Inherits::Only(&kept),
            SearchPath::Inherited,
        );

        let mut removed: Vec<OsString> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_os_string())
            .collect();
        let mut expected: Vec<OsString> = std::env::vars_os()
            .map(|(name, _)| name)
            .filter(|name| name != "PATH")
            .collect();
        removed.sort();
        expected.sort();
        assert_eq!(removed, expected);
    }

    /// A start given only some names still reduces the `PATH` it is asked to reduce, though `PATH`
    /// is not among them, and gives no other name. The regression it rejects: reducing the `PATH`
    /// before the environment is cleared, which leaves the program none.
    #[cfg(unix)]
    #[test]
    fn a_start_given_only_some_names_still_reduces_its_path() {
        let names = ["BRAVEBOT_PROBE_NOT_HELD"];

        assert_eq!(
            handed(Inherits::Only(&names), SearchPath::Reduced),
            ["PATH"]
        );
    }

    /// The names `env` printed for a program started with these rules, which is what it was handed.
    #[cfg(unix)]
    fn handed(inherits: Inherits<'_>, path: SearchPath) -> Vec<String> {
        let printed = start(&Resolved::written_in_source("/usr/bin/env"), inherits, path)
            .output()
            .expect("env runs");
        String::from_utf8_lossy(&printed.stdout)
            .lines()
            .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
            .collect()
    }

    /// A caller that sets a withheld name after `start` takes it off again, and the other rules
    /// take nothing off. The regressions it rejects: a `withhold` that does nothing, which lets an
    /// assignment bring a withheld name back, and one that removes names under a rule that names none.
    #[test]
    fn withhold_takes_a_name_set_after_start_off_again() {
        let names = ["BRAVEBOT_PROBE_KEY".to_string()];
        let set = |inherits: Inherits<'_>| {
            let mut command = start(
                &Resolved::named_by_person("true"),
                inherits,
                SearchPath::Inherited,
            );
            command.env("BRAVEBOT_PROBE_KEY", "the-lines-own");
            inherits.withhold(&mut command);
            command
                .get_envs()
                .find(|(name, _)| *name == OsStr::new("BRAVEBOT_PROBE_KEY"))
                .map(|(_, value)| value.map(OsStr::to_os_string))
        };

        assert_eq!(set(Inherits::Without(&names)), Some(None));
        assert_eq!(
            set(Inherits::All),
            Some(Some(OsString::from("the-lines-own")))
        );
        assert_eq!(
            set(Inherits::Only(&[])),
            Some(Some(OsString::from("the-lines-own")))
        );
    }

    /// Only a start asked for the reduced `PATH` is given one. What the reduction holds is pinned
    /// by the tests below. The regressions it rejects: a start that never reduces, which leaves an
    /// `env` shebang to find a file a stage wrote, and one that always does, which breaks a language
    /// server that is meant to find the project's interpreter.
    #[test]
    fn start_gives_a_reduced_path_only_where_it_is_asked_for() {
        let program = Resolved::named_by_person("true");
        let sets_path = |path| {
            start(&program, Inherits::All, path)
                .get_envs()
                .any(|(name, _)| name == OsStr::new("PATH"))
        };

        assert!(sets_path(SearchPath::Reduced));
        assert!(!sets_path(SearchPath::Inherited));
    }

    /// The lookup constructors apply the lookup: a name nothing on `PATH` holds is not a program,
    /// and an absolute path is taken as written. The regression it rejects: a constructor that
    /// wraps whatever it is given without searching.
    #[cfg(unix)]
    #[test]
    fn the_lookup_constructors_search_and_do_not_wrap_a_name() {
        assert_eq!(
            Resolved::find(OsStr::new("/bin/sh")).map(|found| found.as_path().to_path_buf()),
            Some(std::path::PathBuf::from("/bin/sh"))
        );
        assert_eq!(
            Resolved::find_on(OsStr::new("sh"), OsStr::new("")),
            None,
            "an empty PATH searches nowhere"
        );
        assert_eq!(
            Resolved::find(OsStr::new("no-such-program-bravebot-2740")),
            None
        );
    }

    /// A Windows executable extension is not part of the name a program is known by, in any case,
    /// and nothing else is removed. The regression it rejects: comparing the file's whole name, which
    /// never matches `cargo` on Windows, or stripping any extension, which makes `git.bak` `git`.
    #[test]
    fn a_windows_executable_extension_is_not_part_of_a_programs_name() {
        for (resolved, name) in [
            ("C:/Users/p/.cargo/bin/cargo.exe", Some("cargo")),
            ("/x/CARGO.EXE", Some("cargo")),
            ("/x/git.Com", Some("git")),
            ("/x/GIT", Some("GIT")),
            ("/x/git", Some("git")),
            ("/x/python3.12.exe", Some("python3.12")),
            ("/x/git.exe.bak", Some("git.exe.bak")),
            ("/x/git.bat", Some("git.bat")),
            ("/x/.exe", Some(".exe")),
            ("/x/\u{e9}.exe", Some("\u{e9}")),
            ("/", None),
        ] {
            assert_eq!(
                program_name(Path::new(resolved)).as_deref(),
                name,
                "{resolved}"
            );
        }
    }

    use super::*;

    /// A name that is not text has no program name, and is not read as its lossy lookalike. The
    /// regression it rejects: `to_string_lossy`, which makes `git\xff.exe` into `git\u{FFFD}` and
    /// so gives a file that is not `git` a name some lookup could match.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_text_has_no_program_name() {
        use std::os::unix::ffi::OsStrExt;
        let not_text = std::ffi::OsStr::from_bytes(b"/x/git\xff.exe");
        assert_eq!(program_name(Path::new(not_text)), None);
        assert_eq!(
            program_name(Path::new("/x/git\u{FFFD}.exe")).as_deref(),
            Some("git\u{FFFD}")
        );
    }

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

    /// A file in a stage-writable `PATH` entry, or a link from a safe one into it, is not outside
    /// the directories a stage may write. The regression it rejects: checking only the entry, which
    /// accepts a link in a safe directory that points at a planted file.
    #[cfg(unix)]
    #[test]
    fn a_file_in_or_linked_into_a_writable_directory_is_not_outside_it() {
        let session = scratch_dir("programs-outside");
        let planted = executable(&session.join("venv").join("bin"), "npx");
        let safe = scratch_dir("programs-outside-safe");
        let genuine = executable(&safe, "npx");
        let link = safe.join("linked-npx");
        std::os::unix::fs::symlink(&planted, &link).unwrap();
        let writable = std::slice::from_ref(&session);

        assert!(!outside_stage_writable_with(&planted, writable, &[]));
        assert!(!outside_stage_writable_with(&link, writable, &[]));
        assert!(outside_stage_writable_with(&genuine, writable, &[]));
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

    /// The other spellings of `directory` that the volume under it opens as the same directory:
    /// macOS's second absolute name for a directory on its data volume, and the last name in capitals
    /// where the volume opens names without regard to case. Empty on a volume that opens neither,
    /// such as ext4 without `casefold`, where there is nothing to test.
    #[cfg(unix)]
    fn other_spellings(directory: &Path) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Ok(below_root) = directory.strip_prefix("/") {
            candidates.push(Path::new("/System/Volumes/Data").join(below_root));
        }
        if let Some(name) = directory.file_name().and_then(OsStr::to_str) {
            candidates.push(directory.with_file_name(name.to_uppercase()));
        }
        let reached = identity_of(directory);
        candidates
            .into_iter()
            .filter(|spelling| {
                spelling != directory && reached.is_some() && identity_of(spelling) == reached
            })
            .collect()
    }

    /// A directory a stage may write with an `aws` in its `bin`, the other spellings of it the
    /// volume opens, and an `aws` installed in a directory nothing leaves out.
    #[cfg(unix)]
    fn planted_beside_an_installed(name: &str) -> (PathBuf, Vec<PathBuf>, PathBuf) {
        let session = scratch_dir(&format!("programs-{name}-session"));
        executable(&session.join("bin"), "aws");
        let installed = executable(&scratch_dir(&format!("programs-{name}-system")), "aws");
        let spellings = other_spellings(&session);
        if spellings.is_empty() {
            eprintln!("{name}: the volume holds every spelling of the directory apart, skipped");
        }
        (session, spellings, installed)
    }

    /// SANDBOX-30: a `PATH` entry the volume opens as a directory a stage may write is not
    /// searched, however it is spelled. Where the volume returns the spelling it was given, the
    /// names differ and the entry is the directory, so a stage's `aws` there would be the one
    /// started. The installed `aws` behind it is the one found, which tells this apart from a lookup
    /// that leaves out every entry.
    #[cfg(unix)]
    #[test]
    fn a_path_entry_the_volume_opens_as_a_directory_a_stage_may_write_is_not_searched() {
        let (session, spellings, installed) = planted_beside_an_installed("spelling-entry");

        for spelling in &spellings {
            let path = path_of(&[&spelling.join("bin"), installed.parent().unwrap()]);

            assert_eq!(
                find_in(OsStr::new("aws"), &path, std::slice::from_ref(&session)),
                Some(installed.clone()),
                "the entry spelled {spelling:?} was searched"
            );
        }
    }

    /// SANDBOX-30: the `PATH` a started program looks its own names up in leaves out the same
    /// entry, since the editor, a hook and the suite's unconfined programs are given it through
    /// [`restrict_path`] and an `env` shebang searches it.
    #[cfg(unix)]
    #[test]
    fn a_started_programs_path_leaves_out_an_entry_the_volume_opens_as_a_writable_directory() {
        let (session, spellings, installed) = planted_beside_an_installed("spelling-restrict");
        let safe = installed.parent().unwrap();

        for spelling in &spellings {
            let path = path_of(&[&spelling.join("bin"), safe]);

            assert_eq!(
                searched_path(&path, std::slice::from_ref(&session), &[]).as_deref(),
                Some(safe.as_os_str()),
                "the entry spelled {spelling:?} was kept"
            );
        }
    }

    /// SANDBOX-30: a row a stage holds where the account can write is judged the same way, so a
    /// stage given the whole machine has not chosen the `aws` in a directory the account owns by
    /// spelling its path another way.
    #[cfg(unix)]
    #[test]
    fn a_program_the_volume_opens_in_a_row_held_where_the_account_can_write_is_not_found() {
        let (session, spellings, installed) = planted_beside_an_installed("spelling-held");

        for spelling in &spellings {
            let path = path_of(&[&spelling.join("bin"), installed.parent().unwrap()]);

            assert_eq!(
                find_in_with(
                    OsStr::new("aws"),
                    &path,
                    &[],
                    std::slice::from_ref(&session)
                ),
                Some(installed.clone()),
                "the entry spelled {spelling:?} was searched"
            );
        }
    }

    /// SANDBOX-30: the `PATH` a started program is given leaves out an entry that links into another
    /// spelling of a writable directory, as it does one that links into the directory under its
    /// recorded name. Neither the entry nor a directory above it is the recorded directory, and the
    /// directory it resolves to is, so only the ancestors of the resolved path show it. Nothing
    /// resolves a file in the entry here, as a lookup does, so no later check covers it.
    #[cfg(unix)]
    #[test]
    fn a_started_programs_path_leaves_out_a_link_into_another_spelling_of_a_writable_directory() {
        let (session, spellings, installed) = planted_beside_an_installed("spelling-entry-link");
        let safe = installed.parent().unwrap();

        for (index, spelling) in spellings.iter().enumerate() {
            let entry = scratch_dir(&format!("programs-spelling-entry-link-{index}")).join("bin");
            std::os::unix::fs::symlink(spelling.join("bin"), &entry).unwrap();
            let path = path_of(&[&entry, safe]);

            assert_eq!(
                searched_path(&path, std::slice::from_ref(&session), &[]).as_deref(),
                Some(safe.as_os_str()),
                "the entry linked to {spelling:?} was kept"
            );
        }
    }

    /// SANDBOX-30: a file in a searched directory that links into another spelling of a writable
    /// directory is the writable file. The entry is not the directory, so only the check on the
    /// file the lookup resolves to can leave it out.
    #[cfg(unix)]
    #[test]
    fn a_link_in_a_safe_directory_to_a_program_under_another_spelling_is_not_found() {
        let (session, spellings, installed) = planted_beside_an_installed("spelling-file-link");

        for (index, spelling) in spellings.iter().enumerate() {
            let safe = scratch_dir(&format!("programs-spelling-file-link-{index}"));
            std::os::unix::fs::symlink(spelling.join("bin").join("aws"), safe.join("aws")).unwrap();
            let path = path_of(&[&safe, installed.parent().unwrap()]);

            assert_eq!(
                find_in(OsStr::new("aws"), &path, std::slice::from_ref(&session)),
                Some(installed.clone()),
                "the link to {spelling:?} was found"
            );
        }
    }

    /// SANDBOX-30: which spellings a volume opens as one directory is the volume's answer, so the
    /// comparison is also driven with a made one in which `/mnt/mirror/work` is a second name for
    /// `/vol/Project`, which no volume need hold files to show. A path is within a recorded
    /// directory when it, or a directory above it, is that directory, and not when it is above it,
    /// beside it or elsewhere.
    #[test]
    fn a_path_is_within_a_directory_a_volume_opens_under_another_name() {
        let volume = |path: &Path| -> Option<Identity> {
            match path.to_str()? {
                "/vol" => Some((1, 2)),
                "/vol/Project" | "/mnt/mirror/work" => Some((1, 10)),
                "/vol/Project/bin" | "/mnt/mirror/work/bin" => Some((1, 11)),
                "/vol/Project-tools" => Some((1, 12)),
                "/mnt/mirror" => Some((1, 20)),
                _ => None,
            }
        };
        let rows = Rows::identified_by(vec![PathBuf::from("/vol/Project")], volume);

        for (path, within, why) in [
            ("/vol/Project/bin", true, "the name it was recorded under"),
            (
                "/mnt/mirror/work",
                true,
                "another name for the directory itself",
            ),
            (
                "/mnt/mirror/work/bin",
                true,
                "another name for a directory below it",
            ),
            (
                "/mnt/mirror/work/bin/aws",
                true,
                "a file below the other name, which the volume does not list",
            ),
            (
                "/vol/Project-tools",
                false,
                "a sibling that shares a name prefix",
            ),
            ("/vol", false, "the directory above it"),
            ("/mnt/mirror", false, "the directory above the other name"),
            ("/vol/other/bin", false, "a directory nothing recorded"),
        ] {
            assert_eq!(within_any(Path::new(path), &rows), within, "{path}: {why}");
        }
    }

    /// SANDBOX-30: a recorded directory that no longer exists has no identity, and is still left
    /// out under the name it was recorded under, which is the only thing to compare it by.
    #[test]
    fn a_recorded_directory_that_does_not_exist_is_still_compared_by_name() {
        let rows = Rows::identified_by(vec![PathBuf::from("/vol/gone")], |_| None);

        assert!(within_any(Path::new("/vol/gone/bin"), &rows));
        assert!(!within_any(Path::new("/vol/gone-tools/bin"), &rows));
    }

    /// SANDBOX-30: an entry that does not exist yet, below a link into a directory a stage may
    /// write, is left out of the `PATH` a started program is given, as one that exists is. The stage
    /// can make the directory, and the entry is then one it wrote. The link leads into a
    /// subdirectory of the recorded one, so the recorded directory is not among the entry's own
    /// ancestors, and the entry has no resolved form until its existing part is resolved. No volume
    /// that opens two spellings is needed, so this runs wherever there are links.
    #[cfg(unix)]
    #[test]
    fn a_started_programs_path_leaves_out_an_entry_not_yet_made_below_a_link_into_a_writable_directory()
     {
        let session = scratch_dir("programs-below-link-session");
        let deeper = session.join("tools");
        std::fs::create_dir_all(&deeper).unwrap();
        let safe = scratch_dir("programs-below-link-safe");
        let link = scratch_dir("programs-below-link-elsewhere").join("link");
        std::os::unix::fs::symlink(&deeper, &link).unwrap();
        let entry = link.join("bin");
        assert!(
            !entry.exists(),
            "the entry exists, so it has a resolved form"
        );

        assert_eq!(
            searched_path(
                &path_of(&[&entry, &safe]),
                std::slice::from_ref(&session),
                &[]
            )
            .as_deref(),
            Some(safe.as_os_str())
        );
    }

    /// SANDBOX-30: a directory whose name is not text is judged by what it is, and the directory
    /// named by its lossy rendering is another one ([PATH-003]). Each entry is below a link to one
    /// of them and does not exist, so it is judged by the directory its link leads to. A comparison
    /// that keyed a directory by a lossy rendering of its name would leave out both.
    ///
    /// [PATH-003]: ../../../docs/best-practices/paths.md
    #[cfg(target_os = "linux")]
    #[test]
    fn a_directory_that_is_not_text_is_judged_by_what_it_is_and_not_by_its_lossy_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let parent = scratch_dir("programs-bytes-identity");
        let writable = parent.join(OsStr::from_bytes(b"w\xff"));
        let lookalike = parent.join("w\u{FFFD}");
        std::fs::create_dir_all(&writable).unwrap();
        std::fs::create_dir_all(&lookalike).unwrap();
        let (to_writable, to_lookalike) = (parent.join("to-writable"), parent.join("to-lookalike"));
        std::os::unix::fs::symlink(&writable, &to_writable).unwrap();
        std::os::unix::fs::symlink(&lookalike, &to_lookalike).unwrap();
        let kept = to_lookalike.join("bin");

        assert_eq!(
            searched_path(
                &path_of(&[&to_writable.join("bin"), &kept]),
                std::slice::from_ref(&writable),
                &[]
            ),
            Some(kept.into_os_string())
        );
    }
}
