//! Editing the prompt in the user's own editor.
//!
//! A prompt worth thinking about outgrows the box it is typed in. The caret reaches any word of it
//! now, but the box is still ten rows at most and has nothing to search, fold or reflow with.
//! Ctrl-G hands the line to whatever the user edits text with, waits for them to finish, and takes
//! back what they saved.
//!
//! Nothing labelled passes through here. The line is the user's own words on their way from the
//! keyboard to their editor and back, and nothing read out of the workspace joins them.
//!
//! The file the editor is handed is made in a directory of its own under the account's cache
//! directory (`bravebot/editor/<random>/`), and not in the system temporary directory. A stage of
//! `run` is the same account and may write the temporary directory (SANDBOX-12), so a file there
//! can be replaced before the editor opens it and after it saves, and what is read back goes into
//! the prompt box or a write body. No row of a stage's profile names the cache directory as a
//! whole. [INPUT-14](../../../docs/specs/terminal-input.md#INPUT-14) states the rule and what it
//! leaves.
//!
//! `$VISUAL` and `$EDITOR` hold a command line by convention, and one is split into argv here
//! rather than handed to a shell. That is the rule the rest of the system runs under, and it
//! costs nothing to keep: a shell between the user's configuration and what starts would be a
//! second interpreter with its own opinions about quoting.

use bravebot_i18n::t;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(not(unix))]
use std::sync::atomic::{AtomicU64, Ordering};

/// What to open when nothing is configured, in the order they are tried.
///
/// A guess, and kept to editors that open in the terminal the key was pressed in, so finishing
/// puts the user back where they were. The full editors come first: someone with `vim` or `emacs`
/// installed almost certainly reaches for it, and `nano` is the last resort rather than the first.
///
/// Only reached when the user has said nothing. A configured editor that will not start is an
/// answer, not a reason to run something they did not choose.
///
/// The same list everywhere the binary ships. A list of its own for Windows puts `notepad` in
/// front of the `vim` that Git for Windows installs, and `notepad` is neither a full editor nor
/// one that opens in the terminal the key was pressed in.
const FALLBACKS: &[&str] = &["vim", "vi", "emacs", "nano"];

/// Editors that return before the file has been edited, and the flag that makes them wait.
///
/// A window opens, the process exits at once, the line comes back exactly as it went, and
/// nothing anywhere says why. That is the most confusing outcome available: neither a failure
/// nor an edit. The flag is only added where the command is a bare program, since a user who
/// wrote arguments of their own has already said how they want it run.
const WAITING: &[(&str, &str)] = &[
    ("code", "--wait"),
    ("codium", "--wait"),
    ("cursor", "--wait"),
    ("windsurf", "--wait"),
    ("zed", "--wait"),
    ("subl", "--wait"),
];

/// Why a line did not come back from an editor.
#[derive(Debug)]
pub enum Failure {
    /// Nothing is configured, and nothing on the fallback list is installed.
    NoEditor,
    /// An editor was found, and it did not edit the line.
    Editor(String),
    /// The file the editor was to open could not be written, or could not be read back.
    Scratch(io::Error),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::NoEditor => write!(f, "{}", t!(editor_none_configured)),
            Failure::Editor(what) => write!(f, "{what}"),
            Failure::Scratch(error) => {
                write!(f, "{}", t!(editor_scratch_unusable, problem = error))
            }
        }
    }
}

/// Put `line` in front of the user in their editor, and return what they saved.
///
/// Quitting without saving returns `line` unchanged, because the file already holds it: not
/// saving loses the edits rather than the prompt.
pub fn edit(line: &str) -> Result<String, Failure> {
    round_trip(place().as_deref(), line, open_in_an_editor)
}

/// Put the transcript in front of the user in their editor, and read nothing back.
///
/// The one difference from [`edit`], and the whole of it. A prompt is something they are still
/// writing, so what they save comes back; a transcript is the record of what happened, and a
/// record that can be edited into the session is not a record. The file is theirs to do anything
/// with, and this process never looks at it again.
pub fn show(text: &str) -> Result<(), Failure> {
    show_through(place().as_deref(), text, open_in_an_editor)
}

/// Put the file at `path` in front of the user in their editor, at `line` and `column` where they
/// are given and the editor takes a place, and wait for it.
///
/// Nothing is copied and nothing is read back. The editor is handed the file itself, which is the
/// user's own, and what it does to it is theirs to do. The program is the one a prompt is edited
/// in, looked up and started the same way (SANDBOX-30): through [`run`], which [`start`] shares, so
/// `an_editor_found_only_in_a_directory_a_stage_may_write_is_not_started` and
/// `an_editor_that_runs_another_program_by_name_never_runs_one_a_stage_may_have_written` show that no
/// file a stage wrote is the program that runs here either.
///
/// The file is one the workspace holds, which a checkout may have written, and an editor acts on
/// what is in a file it opens (a modeline, a project's configuration). So it is started without this
/// agent's own credentials, as the browser is
/// ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)); the editor a prompt is edited in is
/// started with all of the environment, since nothing but the person's own line goes to it. The
/// command is built by [`file_command`], and
/// `the_editor_a_click_opens_is_started_without_this_agents_own_credentials` reads it.
pub fn open_file(path: &Path, line: Option<u32>, column: Option<u32>) -> Result<(), Failure> {
    let (program, arguments) = which_editor(configured())?;
    let (arguments, file) = at_a_place(&program, arguments, path, line, column);
    run(
        &program,
        file_command(&program, &arguments, file.as_os_str()),
        |editor, code| match code {
            Some(code) => t!(editor_file_exited_badly, editor = editor, code = code),
            None => t!(editor_file_was_stopped, editor = editor),
        },
    )
}

/// The file half of [`show`], with opening it left to the caller so it can be tested.
fn show_through(
    place: Option<&Path>,
    text: &str,
    open: impl FnOnce(&Path) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let file = scratch(place, "transcript", "md")?;
    if let Err(failure) = write_scratch(&file.path, text) {
        file.forget();
        return Err(failure);
    }
    let opened = open(&file.path);
    // Before the result is examined, so a transcript does not outlive the look at it down any
    // path. Whatever the person wanted to keep, they kept from inside their own editor.
    file.forget();
    opened
}

/// The round trip through a file, with opening it left to the caller.
///
/// Separated so the file half can be tested without an editor, which is the half with the
/// property worth pinning: what comes back when nothing was written.
fn round_trip(
    place: Option<&Path>,
    line: &str,
    open: impl FnOnce(&Path) -> Result<(), Failure>,
) -> Result<String, Failure> {
    let mut text = through_a_file(scratch(place, "prompt", "md")?, line, open)?;
    tidy(&mut text);
    Ok(text)
}

/// Put the body of a proposed write in front of the user in their editor, and return what they
/// saved.
///
/// The text comes back exactly as the editor wrote it. A prompt is fitted to the box it returns
/// to, and a file is not: a final newline is part of it, and so are its line endings, which the
/// write prompt shows the effect of on the screen where the text is approved. `extension` names
/// the scratch file so the editor chooses the syntax of the file the body is for.
pub fn edit_body(body: &str, extension: &str) -> Result<String, Failure> {
    body_round_trip(place().as_deref(), body, extension, open_in_an_editor)
}

/// The file half of [`edit_body`], with opening it left to the caller so it can be tested.
fn body_round_trip(
    place: Option<&Path>,
    body: &str,
    extension: &str,
    open: impl FnOnce(&Path) -> Result<(), Failure>,
) -> Result<String, Failure> {
    through_a_file(scratch(place, "write", extension)?, body, open)
}

/// What to call the scratch file of a body bound for `path`, so the editor takes it for the kind of
/// file it is.
///
/// The extension of the last component, where it is short and made of letters and digits, and
/// `txt` otherwise. Nothing else of the name is used: the path is the planner's, and this is a
/// file name.
pub fn scratch_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= MOST_EXTENSION_BYTES
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .unwrap_or("txt")
        .to_string()
}

/// The longest extension a scratch file takes from a path.
const MOST_EXTENSION_BYTES: usize = 12;

/// Write `text` to `file`, run `open` on it and read back what is there, removing the file and its
/// directory on every path out.
fn through_a_file(
    file: ScratchFile,
    text: &str,
    open: impl FnOnce(&Path) -> Result<(), Failure>,
) -> Result<String, Failure> {
    if let Err(failure) = write_scratch(&file.path, text) {
        // A write that failed partway leaves a file behind, and one holding a prompt is not
        // something to leave in a directory.
        file.forget();
        return Err(failure);
    }

    let opened = open(&file.path);
    let saved = std::fs::read_to_string(&file.path);
    // Before either result is examined, so the text does not outlive the edit down any path.
    file.forget();

    opened?;
    saved.map_err(Failure::Scratch)
}

/// Where the file the editor is handed is made: under the account's cache directory.
///
/// Not the system temporary directory, which every stage of `run` may write (SANDBOX-12), and so
/// may replace what the editor is shown and what it saved. Not `~/.bravebot` either, which is the
/// user's configuration surface, is read as trusted, and is not written in an incognito session.
fn place() -> Option<PathBuf> {
    bravebot_agent::home::cache()
}

/// A file made for the editor to open, and the directory made for it where there is one.
struct ScratchFile {
    /// The file.
    path: PathBuf,
    /// The directory made for this one file, which goes with it. `None` where the file is in a
    /// directory that is not this file's to remove.
    directory: Option<PathBuf>,
}

impl ScratchFile {
    /// Take the file and its directory away. A failure is not reported: this runs as an edit ends,
    /// where there is nobody to tell, and the directory is one only this account can enter.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "nobody is left to tell when an edit has ended, and a file already gone is the aim"
    )]
    fn forget(&self) {
        let _ = std::fs::remove_file(&self.path);
        if let Some(directory) = &self.directory {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

/// The file and its directory go when the value does, so an unwind out of the editor's own call (a
/// panic) removes them as a return does. A process that is killed, or that aborts on a panic as the
/// release build does, runs no destructor, and leaves them.
impl Drop for ScratchFile {
    fn drop(&mut self) {
        self.forget();
    }
}

/// Where the line is put for the editor to open: `bravebot/editor/<32 random hexadecimal
/// digits>/<kind>.<extension>` under `place`.
///
/// Each directory above the one made for this file is refused where it is a link or belongs to
/// another account, and is set to mode 0700, so a program of another account cannot replace what is
/// inside. The directory made for this file is created and never adopted: a name already there is an
/// error and not a directory to use. The name is read from the operating system's random source,
/// since one made from the process id, the time or a count is one a program that can write `place`
/// works out and creates first. Where there is no `place` the platform has no directory to give this
/// account, and nothing else is used in its place: the system temporary directory is never the
/// fallback.
#[cfg(unix)]
fn scratch(place: Option<&Path>, kind: &str, extension: &str) -> Result<ScratchFile, Failure> {
    use std::os::unix::fs::DirBuilderExt;
    let place = place.ok_or_else(|| {
        Failure::Scratch(io::Error::new(
            io::ErrorKind::NotFound,
            "this account has no cache directory to make the file in",
        ))
    })?;
    let made = |directory: PathBuf| -> io::Result<PathBuf> {
        bravebot_sandbox::private::owner_only_directory(&directory)?;
        Ok(directory)
    };
    let directory = (|| -> io::Result<PathBuf> {
        let editor = made(made(place.join("bravebot"))?.join("editor"))?;
        let directory = editor.join(bravebot_sandbox::private::random_name()?);
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        Ok(directory)
    })()
    .map_err(Failure::Scratch)?;
    Ok(ScratchFile {
        path: directory.join(format!("{kind}.{extension}")),
        directory: Some(directory),
    })
}

/// Windows has no mode to set on a directory, and gives each account a temporary directory a stage
/// is not granted where the session has a scratch directory, so the file stays there.
#[cfg(not(unix))]
fn scratch(_place: Option<&Path>, kind: &str, extension: &str) -> Result<ScratchFile, Failure> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let nth = SCRATCH_NAMES.fetch_add(1, Ordering::Relaxed);
    // The name carries the pid, a stamp and a count, and `write_scratch` below creates it
    // with `create_new`, the reason a name already taken is refused rather than reused.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let path = std::env::temp_dir().join(format!(
        "bravebot-{kind}-{}-{stamp}-{nth}.{extension}",
        std::process::id()
    ));
    Ok(ScratchFile {
        path,
        directory: None,
    })
}

/// What tells two scratch names from one process apart, where the name is made from the process.
///
/// The stamp is in nanoseconds and the clock behind it is not: it holds a value for thousands of
/// reads, so two names taken in the same moment are routinely the same name. The pid separates
/// processes and this separates the calls within one.
#[cfg(not(unix))]
static SCRATCH_NAMES: AtomicU64 = AtomicU64::new(0);

/// Write the line where the editor will find it.
///
/// Created rather than opened, so an existing name is refused and not written through. Readable by
/// nobody else of the account's machine: the mode is 0600, and the directory above it is 0700.
fn write_scratch(path: &Path, line: &str) -> Result<(), Failure> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(Failure::Scratch)?;
    file.write_all(line.as_bytes()).map_err(Failure::Scratch)?;
    file.flush().map_err(Failure::Scratch)
}

/// Make what an editor saved fit the box it is going back into.
fn tidy(text: &mut String) {
    if text.contains('\r') {
        *text = text.replace("\r\n", "\n").replace('\r', "\n");
    }
    // Editors end a file with a newline, and the box would draw it as an empty line below the
    // prompt. One only: a blank line the user left deliberately is theirs to have left.
    if text.ends_with('\n') {
        text.pop();
    }
}

/// Run an editor on `path` and wait for it.
fn open_in_an_editor(path: &Path) -> Result<(), Failure> {
    let (program, arguments) = which_editor(configured())?;
    start(&program, &arguments, path)
}

/// The editor to run, given whatever the user configured.
fn which_editor(command: Option<String>) -> Result<(PathBuf, Vec<String>), Failure> {
    let Some(command) = command else {
        return FALLBACKS
            .iter()
            .find_map(|fallback| command_line(fallback))
            .ok_or(Failure::NoEditor);
    };

    command_line(&command)
        .ok_or_else(|| Failure::Editor(t!(editor_named_but_missing, command = command)))
}

/// The editor the user configured, if they configured one.
fn configured() -> Option<String> {
    chosen(std::env::var_os("VISUAL"), std::env::var_os("EDITOR"))
}

/// Which of the two variables answers, given what they hold.
///
/// `$VISUAL` first, which is the convention: `$EDITOR` is the one that has to work on a teletype,
/// and `$VISUAL` is what the user wants when the terminal can do more than print. An empty value
/// is not an answer, since exporting a variable to nothing is how a profile takes one back.
fn chosen(visual: Option<OsString>, editor: Option<OsString>) -> Option<String> {
    [visual, editor]
        .into_iter()
        .flatten()
        .map(|value| value.to_string_lossy().into_owned())
        .find(|value| !value.trim().is_empty())
}

/// Split a configured editor into the program to run and the arguments to run it with.
///
/// `None` when there is no such program, which is the fallback list's turn or, for a configured
/// editor, the whole answer.
///
/// Whitespace usually separates a command from its flags, but it is also just a character in a
/// path, and on macOS most of `/Applications` has one. So the lookup decides and the shape of the
/// string never does: a name that resolves whole is a path and keeps its spaces, and only one
/// that does not is read as a command line. That leaves an editor whose path has a space in it
/// and takes arguments unreachable, which needs quoting rules to fix and has never been asked for.
fn command_line(command: &str) -> Option<(PathBuf, Vec<String>)> {
    // Only consulted for a name with a separator in it, and a relative one at that. The current
    // directory is what such a name means to whoever exported it.
    let working = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    split(command, |name| lookup(name, &working))
}

/// Split a command into a program and its arguments, with finding the program left to the caller.
///
/// Separated so the splitting and the flag can be tested against an installation laid out like a
/// real one, without a machine that has that editor on its `$PATH`.
fn split(command: &str, find: impl Fn(&str) -> Option<PathBuf>) -> Option<(PathBuf, Vec<String>)> {
    let (program, mut arguments) = match find(command) {
        Some(program) => (program, Vec::new()),
        None => {
            let mut words = command.split_whitespace();
            let program = find(words.next()?)?;
            (program, words.map(str::to_string).collect())
        }
    };

    if arguments.is_empty() {
        arguments.extend(waiting_flag(&program));
    }
    Some((program, arguments))
}

/// Find the editor `command` names, keeping the name it was found under.
///
/// A bare name is looked up as every program bravebot starts unconfined is (SANDBOX-30): through
/// [`bravebot_sandbox::programs::find`], which skips every `$PATH` entry a confined stage may write.
/// An `$EDITOR` of `vim` therefore never starts a `vim` a stage left in `.venv/bin` or
/// `node_modules/.bin`, however early that directory sits on the `$PATH`.
///
/// The lookup returns the entry joined to the name, not the canonicalised file, and that is what
/// runs. An editor is started rather than approved, and here the name is part of what was asked for:
/// MacVim installs `vim`, `vi` and `gvim` as links to one shim that reads its own `argv[0]` and
/// opens a detached window for the `m*` and `g*` spellings. Resolved through the link it becomes
/// `mvim`, so asking for `vim` got a GUI window and a prompt that came straight back. Started the way
/// the shell would start it, under the name on the `$PATH`, it stays in the terminal.
///
/// A name with a separator in it is a path the user wrote, absolute or relative to `working`, and the
/// `$PATH` plays no part in it.
fn lookup(command: &str, working: &Path) -> Option<PathBuf> {
    if !names_a_path(command) {
        return bravebot_sandbox::programs::find(OsStr::new(command));
    }
    let resolved = bravebot_agent::programs::resolve(command, working)?;
    Some(started_as(command, resolved, working))
}

/// Whether `command` is spelled as a path rather than a name to look for.
fn names_a_path(command: &str) -> bool {
    command.contains('/') || (cfg!(windows) && command.contains('\\'))
}

/// Which path starts a `command` that names a path, given the file the lookup settled on.
///
/// It stays under the name written, and only while that is still the same program. A link pointing
/// somewhere else, or a name that no longer resolves, is not something to run on a guess.
fn started_as(command: &str, resolved: PathBuf, working: &Path) -> PathBuf {
    let named = if Path::new(command).is_absolute() {
        PathBuf::from(command)
    } else {
        working.join(command)
    };

    if same_file(&named, &resolved) {
        named
    } else {
        resolved
    }
}

/// Whether two paths reach one file, links and all.
fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// How an editor is told which line of a file to open at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// `+LINE` before the file, as vim and the editors that follow it take it.
    Plus,
    /// `+LINE,COLUMN` before the file.
    PlusComma,
    /// `+LINE:COLUMN` before the file.
    PlusColon,
    /// `--goto FILE:LINE:COLUMN`, as Visual Studio Code and what is built on it take it.
    Goto,
    /// `FILE:LINE:COLUMN`, as Sublime Text, Zed and Helix take it.
    Suffix,
}

/// The editors that take a place, by the name of the program, and how each takes it.
///
/// Only those whose way is known. An editor that is not here is handed the file alone, which opens
/// it at the top: a guess at another editor's flag is a flag it may read as something else.
const PLACES: &[(&str, Place)] = &[
    ("vim", Place::Plus),
    ("vi", Place::Plus),
    ("nvim", Place::Plus),
    ("view", Place::Plus),
    ("gvim", Place::Plus),
    ("mvim", Place::Plus),
    ("joe", Place::Plus),
    ("mg", Place::Plus),
    ("nano", Place::PlusComma),
    ("ne", Place::PlusComma),
    ("emacs", Place::PlusColon),
    ("emacsclient", Place::PlusColon),
    ("micro", Place::PlusColon),
    ("kak", Place::PlusColon),
    ("code", Place::Goto),
    ("codium", Place::Goto),
    ("cursor", Place::Goto),
    ("windsurf", Place::Goto),
    ("subl", Place::Suffix),
    ("zed", Place::Suffix),
    ("hx", Place::Suffix),
];

/// The arguments that open `path` at `line` and `column` in `program`: the ones it already has, with
/// whatever tells it the place, and the argument that names the file.
///
/// A line and a column are numbers, so nothing here is text for an editor to read as an option. The
/// file is handed over as it is, an absolute path, which no editor reads as one either.
fn at_a_place(
    program: &Path,
    mut arguments: Vec<String>,
    path: &Path,
    line: Option<u32>,
    column: Option<u32>,
) -> (Vec<String>, OsString) {
    let named = program
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned());
    let place = named
        .as_deref()
        .and_then(|stem| PLACES.iter().find(|(name, _)| *name == stem))
        .map(|(_, place)| *place);
    let (Some(place), Some(line)) = (place, line) else {
        return (arguments, path.as_os_str().to_os_string());
    };
    let with_column = |separator: &str| match column {
        Some(column) => format!("{line}{separator}{column}"),
        None => line.to_string(),
    };
    let suffixed = || {
        let mut file = path.as_os_str().to_os_string();
        file.push(format!(":{}", with_column(":")));
        file
    };
    match place {
        Place::Plus => {
            arguments.push(format!("+{line}"));
            (arguments, path.as_os_str().to_os_string())
        }
        Place::PlusComma => {
            arguments.push(format!("+{}", with_column(",")));
            (arguments, path.as_os_str().to_os_string())
        }
        Place::PlusColon => {
            arguments.push(format!("+{}", with_column(":")));
            (arguments, path.as_os_str().to_os_string())
        }
        Place::Goto => {
            arguments.push("--goto".to_string());
            (arguments, suffixed())
        }
        Place::Suffix => (arguments, suffixed()),
    }
}

/// The flag that makes `program` wait, if it is one of the ones that needs telling.
fn waiting_flag(program: &Path) -> Option<String> {
    let stem = program.file_stem()?.to_string_lossy().into_owned();
    WAITING
        .iter()
        .find(|(name, _)| *name == stem)
        .map(|(_, flag)| (*flag).to_string())
}

/// Start the editor and wait for it to finish.
///
/// A non-zero exit is taken at its word and the line is left alone. `vi` spells "discard this"
/// as `:cq`, which exits 1, and honouring that is the difference between an editor that can be
/// abandoned and one that cannot.
///
/// The editor's own lookups of other programs by name (an `env` shebang, a wrapper script, an
/// `$EDITOR` of `env vim`) search only the `PATH` entries the lookup that found it searches
/// (SANDBOX-30).
fn start(program: &Path, arguments: &[String], path: &Path) -> Result<(), Failure> {
    run(
        program,
        command_for(
            program,
            arguments,
            &[path.as_os_str()],
            bravebot_sandbox::programs::Inherits::All,
        ),
        |editor, code| match code {
            Some(code) => t!(editor_exited_badly, editor = editor, code = code),
            None => t!(editor_was_stopped, editor = editor),
        },
    )
}

/// The command that opens `file` for a click: the editor's own arguments, then the file, started
/// without this agent's own credentials.
fn file_command(
    program: &Path,
    arguments: &[String],
    file: &OsStr,
) -> Option<std::process::Command> {
    let withheld = bravebot_config::scrub::own_credentials();
    command_for(
        program,
        arguments,
        &[file],
        bravebot_sandbox::programs::Inherits::Without(&withheld),
    )
}

/// The command that starts `program`, given the arguments after the editor's own as they are.
///
/// `None` for a `program` with no path separator. [`lookup`] returns a path for every editor, so
/// the editor a person named is never started by the operating system's search of `PATH`.
fn command_for(
    program: &Path,
    arguments: &[String],
    trailing: &[&OsStr],
    inherits: bravebot_sandbox::programs::Inherits<'_>,
) -> Option<std::process::Command> {
    let mut command = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::named_by_person(program)?,
        inherits,
        bravebot_sandbox::programs::SearchPath::Reduced,
        // The editor is the person's own choice (SANDBOX-30).
        bravebot_sandbox::programs::CodeSelectors::Kept,
    );
    command.args(arguments).args(trailing);
    Some(command)
}

/// Run `command` and wait for it, saying what a run that did not end well says as the caller gives
/// it, who knows what the run was for. No `command` is an editor that would not start.
fn run(
    program: &Path,
    command: Option<std::process::Command>,
    exited_badly: impl FnOnce(String, Option<i32>) -> String,
) -> Result<(), Failure> {
    let name = program
        .file_name()
        .unwrap_or(program.as_os_str())
        .to_string_lossy()
        .into_owned();
    let Some(mut command) = command else {
        return Err(Failure::Editor(t!(
            editor_would_not_start,
            editor = name,
            problem = io::Error::from(io::ErrorKind::NotFound)
        )));
    };

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(Failure::Editor(exited_badly(name, status.code()))),
        Err(error) => Err(Failure::Editor(t!(
            editor_would_not_start,
            editor = name,
            problem = error
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What an editor is handed to open `/work/a.rs` at line 12, column 5 (or with some of that
    /// left out), as the arguments it is started with and the file argument last.
    fn handed(
        program: &str,
        arguments: &[&str],
        line: Option<u32>,
        column: Option<u32>,
    ) -> Vec<String> {
        let (mut all, file) = at_a_place(
            Path::new(program),
            arguments
                .iter()
                .map(|argument| argument.to_string())
                .collect(),
            Path::new("/work/a.rs"),
            line,
            column,
        );
        all.push(file.to_string_lossy().into_owned());
        all
    }

    /// Each editor that takes a place is told it in its own words, and any other is handed the file
    /// alone, since a flag guessed for it is one it may read as something else. A column with no line
    /// says nothing, and a line alone is a line.
    #[test]
    fn each_editor_is_told_the_line_in_its_own_words() {
        let at = |program: &str| handed(program, &[], Some(12), Some(5));
        assert_eq!(at("/usr/bin/vim"), ["+12", "/work/a.rs"]);
        assert_eq!(at("nvim"), ["+12", "/work/a.rs"]);
        assert_eq!(at("vi"), ["+12", "/work/a.rs"]);
        assert_eq!(at("nano"), ["+12,5", "/work/a.rs"]);
        assert_eq!(at("emacs"), ["+12:5", "/work/a.rs"]);
        assert_eq!(at("micro"), ["+12:5", "/work/a.rs"]);
        assert_eq!(at("kak"), ["+12:5", "/work/a.rs"]);
        assert_eq!(at("code"), ["--goto", "/work/a.rs:12:5"]);
        assert_eq!(at("cursor"), ["--goto", "/work/a.rs:12:5"]);
        assert_eq!(at("subl"), ["/work/a.rs:12:5"]);
        assert_eq!(at("hx"), ["/work/a.rs:12:5"]);
        for unknown in ["ed", "bbedit", "my-editor", "sed-vim", "vimm"] {
            assert_eq!(at(unknown), ["/work/a.rs"], "{unknown}");
        }

        let line_only = |program: &str| handed(program, &[], Some(12), None);
        assert_eq!(line_only("nano"), ["+12", "/work/a.rs"]);
        assert_eq!(line_only("emacs"), ["+12", "/work/a.rs"]);
        assert_eq!(line_only("code"), ["--goto", "/work/a.rs:12"]);
        assert_eq!(line_only("subl"), ["/work/a.rs:12"]);
        for program in ["vim", "nano", "code", "subl", "emacs"] {
            assert_eq!(
                handed(program, &[], None, Some(5)),
                ["/work/a.rs"],
                "{program}: a column with no line"
            );
            assert_eq!(
                handed(program, &[], None, None),
                ["/work/a.rs"],
                "{program}"
            );
        }
    }

    /// What a person wrote after the program is kept, in front of what places the file, so a
    /// configured `vim -u NONE` and the `--wait` a window editor needs are not lost to the line.
    #[test]
    fn the_arguments_an_editor_already_has_come_first() {
        assert_eq!(
            handed("vim", &["-u", "NONE"], Some(3), None),
            ["-u", "NONE", "+3", "/work/a.rs"]
        );
        assert_eq!(
            handed("code", &["--wait"], Some(3), Some(1)),
            ["--wait", "--goto", "/work/a.rs:3:1"]
        );
    }

    /// A directory standing for the account's cache directory, removed with the test. Nothing runs
    /// in it, so its place under the system temporary directory costs the test nothing.
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_place() -> tempfile::TempDir {
        tempfile::tempdir().expect("a directory to stand for the cache directory")
    }

    /// INPUT-14 (#4609): where the file is made is under the directory the caller names, in a
    /// directory made for this one file, mode 0700, with the directories above it narrowed to 0700
    /// where an earlier run left them open, and a name that is not one a stage could work out.
    ///
    /// The regression it rejects is a directory made with `create_dir_all`, which keeps the mode an
    /// existing directory has, so the directories that hold every edit's file stay open to another
    /// account.
    #[cfg(unix)]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    #[test]
    fn the_file_is_made_in_a_directory_of_its_own_that_only_this_account_enters() {
        use std::os::unix::fs::PermissionsExt;
        let place = a_place();
        let editor = place.path().join("bravebot").join("editor");
        std::fs::create_dir_all(&editor).expect("create the parents");
        for open in [place.path().join("bravebot"), editor.clone()] {
            std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).expect("chmod");
        }

        let file = scratch(Some(place.path()), "prompt", "md").expect("a file of its own");

        let mode_of = |path: &Path| {
            std::fs::metadata(path)
                .expect("exists")
                .permissions()
                .mode()
                & 0o777
        };
        let made = file.directory.clone().expect("a directory of its own");
        assert_eq!(made.parent(), Some(editor.as_path()));
        assert_eq!(file.path.parent(), Some(made.as_path()));
        for directory in [place.path().join("bravebot"), editor, made] {
            assert_eq!(
                mode_of(&directory),
                0o700,
                "{} was left readable by another account",
                directory.display()
            );
        }
        file.forget();
    }

    /// INPUT-14 (#4609): where `bravebot` or `bravebot/editor` is a link, nothing is made and nothing
    /// is made where the link leads. A link there is not one a stage can make (SANDBOX-12 keeps it out
    /// of the cache rows), but an earlier version or a person can, and a directory made through it is
    /// in a place this process does not know the owner or the mode of.
    ///
    /// The regression it rejects is a directory made with `create_dir_all`, which follows the link.
    #[cfg(unix)]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    #[test]
    fn a_link_where_a_directory_above_the_file_should_be_is_refused_and_leads_nowhere_new() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        for linked in ["bravebot", "bravebot/editor"] {
            let place = a_place();
            let elsewhere = place.path().join("elsewhere");
            std::fs::create_dir_all(&elsewhere).expect("create where the link leads");
            std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
            std::fs::create_dir_all(place.path().join("bravebot")).expect("create the parent");
            let link = place.path().join(linked);
            if linked == "bravebot" {
                std::fs::remove_dir(&link).expect("take the directory out of the way");
            }
            symlink(&elsewhere, &link).expect("link");

            let refused = scratch(Some(place.path()), "prompt", "md");

            assert!(
                refused.is_err(),
                "{linked} was a link and a file was made under it"
            );
            assert!(
                std::fs::read_dir(&elsewhere)
                    .expect("read where the link leads")
                    .next()
                    .is_none(),
                "{linked}: something was made where the link leads"
            );
            let mode = std::fs::metadata(&elsewhere)
                .expect("exists")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o755,
                "{linked}: the mode of the linked directory changed"
            );
        }
    }

    /// INPUT-14 (#4609): with no directory to make the file in, the edit is refused and the
    /// temporary directory is not the fallback. A session in which the account names no cache
    /// directory has none, and the file holds the person's prompt.
    ///
    /// The regression it rejects is a fallback to the system temporary directory, which every stage
    /// may write, so what the editor shows and what comes back could be a stage's.
    #[cfg(unix)]
    #[expect(
        clippy::panic,
        reason = "the editor standing in for a real one fails the test by panicking if it is started"
    )]
    #[test]
    fn with_no_directory_to_make_it_in_nothing_is_made_and_the_temporary_directory_is_not_used() {
        let outcome = round_trip(None, "something private", |_| {
            panic!("the editor was started with no file")
        });
        assert!(
            matches!(outcome, Err(Failure::Scratch(_))),
            "no directory, and the edit was not refused: {outcome:?}"
        );
        let shown = show_through(None, "what happened", |_| {
            panic!("the editor was started with no file")
        });
        assert!(matches!(shown, Err(Failure::Scratch(_))), "{shown:?}");
        let body = body_round_trip(None, "the body", "rs", |_| {
            panic!("the editor was started with no file")
        });
        assert!(matches!(body, Err(Failure::Scratch(_))), "{body:?}");
    }

    /// INPUT-14 (#4609): the name of the directory made for a file is 32 lower-case hexadecimal digits
    /// that no program able to write the cache directory can work out, so none can create the name
    /// first or find the file by listing a name it can predict.
    ///
    /// The regression it rejects is a name built from the process id, a clock reading or a counter,
    /// padded to the same length: each is a value a stage computes. Read as two 64-bit halves, such a
    /// name has a half that is zero, constant or a few steps from the same half of the name before
    /// it, and a random name has none of those but with a probability of about 2^-28. A test of the
    /// format alone passes a counter padded to 32 digits.
    #[cfg(unix)]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    #[expect(
        clippy::string_slice,
        reason = "the name is checked to be ASCII hexadecimal"
    )]
    #[test]
    fn the_name_of_the_directory_made_for_a_file_is_not_one_a_stage_could_work_out() {
        let place = a_place();
        let held: Vec<ScratchFile> = (0..8)
            .map(|_| scratch(Some(place.path()), "prompt", "md").expect("a file of its own"))
            .collect();
        let halves: Vec<(u64, u64)> = held
            .iter()
            .map(|file| {
                let name = file
                    .directory
                    .as_deref()
                    .and_then(Path::file_name)
                    .and_then(OsStr::to_str)
                    .expect("a utf-8 name");
                assert_eq!(name.len(), 32, "{name}");
                assert!(
                    name.bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                    "{name} is not lower-case hexadecimal"
                );
                (
                    u64::from_str_radix(&name[..16], 16).expect("hexadecimal"),
                    u64::from_str_radix(&name[16..], 16).expect("hexadecimal"),
                )
            })
            .collect();
        for pair in halves.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            assert!(
                first.0.abs_diff(second.0) > 1 << 32,
                "two names in a row share their first half or step it: {first:x?} {second:x?}"
            );
            assert!(
                first.1.abs_diff(second.1) > 1 << 32,
                "two names in a row share their second half or step it: {first:x?} {second:x?}"
            );
        }
        for file in held {
            file.forget();
        }
    }

    /// A name already taken is refused rather than reused, so two names that are the same name
    /// cost whichever prompt asked second its editor.
    ///
    /// Taken at once, because that is the only way to reach it: a name made from the clock repeats
    /// within one reading, and nothing else in a name made from the process varies within it.
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    #[test]
    fn scratch_names_taken_at_once_still_differ() {
        use std::collections::HashSet;
        use std::sync::{Arc, Barrier};

        const AT_ONCE: usize = 8;
        const ROUNDS: usize = 25;

        let place = a_place();
        let mut taken: HashSet<PathBuf> = HashSet::new();
        for _ in 0..ROUNDS {
            let gate = Arc::new(Barrier::new(AT_ONCE));
            let together: Vec<_> = (0..AT_ONCE)
                .map(|_| {
                    let gate = Arc::clone(&gate);
                    let place = place.path().to_path_buf();
                    std::thread::spawn(move || {
                        gate.wait();
                        scratch(Some(&place), "prompt", "md").expect("a file of its own")
                    })
                })
                .collect();

            for opened in together {
                let file = opened.join().expect("the thread finished");
                assert!(
                    taken.insert(file.path.clone()),
                    "two scratch names taken at once were the same name: {}",
                    file.path.display()
                );
                file.forget();
            }
        }
    }

    /// The convention both variables are part of: `$EDITOR` is the one that must work anywhere,
    /// and `$VISUAL` is what to use when the terminal can do more than print a line at a time.
    #[test]
    fn visual_answers_before_editor() {
        assert_eq!(
            chosen(Some("vim".into()), Some("ed".into())),
            Some("vim".to_string())
        );
        assert_eq!(chosen(None, Some("ed".into())), Some("ed".to_string()));
        assert_eq!(chosen(None, None), None);
    }

    /// Exporting a variable to nothing is how a shell profile takes one back. Reading that as a
    /// configured editor would try to run the empty string and report that it was not found,
    /// when what the user has is no editor configured at all.
    #[test]
    fn an_empty_variable_is_not_a_configured_editor() {
        assert_eq!(
            chosen(Some("".into()), Some("vi".into())),
            Some("vi".into())
        );
        assert_eq!(chosen(Some("   ".into()), None), None);
    }

    /// A scratch directory, removed with the test.
    ///
    /// Every test that uses one looks up a program on the PATH by mode bits or reaches it through
    /// a symlink, so all of them are Unix-only and so is this.
    #[cfg(unix)]
    struct Scratch {
        path: PathBuf,
    }

    #[cfg(unix)]
    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(&format!("bravebot-editor-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create");
            Self { path }
        }

        fn program(&self, name: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.path.join(name);
            std::fs::write(&path, "#!/bin/sh\n").expect("write");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            path
        }
    }

    #[cfg(unix)]
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// MacVim installs `vim`, `vi` and `gvim` as links to one shim that reads its own `argv[0]` and
    /// opens a detached GUI window for the `m*` and `g*` spellings. Resolved through the link, a
    /// request for `vim` became `mvim`: a window opened, the process forked, and the prompt came
    /// straight back unedited. The name is part of what was asked for, so it survives the lookup.
    #[cfg(unix)]
    #[test]
    fn a_program_reached_through_a_link_keeps_the_name_it_was_asked_for() {
        let scratch = Scratch::new("named");
        scratch.program("mvim");
        let asked = scratch.path.join("vim");
        std::os::unix::fs::symlink("mvim", &asked).expect("link");

        let found = lookup(asked.to_str().unwrap(), &scratch.path).expect("the editor is found");

        assert_eq!(
            found.file_name().unwrap(),
            "vim",
            "the editor was started under the name the link points at, not the one asked for"
        );
    }

    /// The real path from a configured editor to what runs, which is where the fix has to be wired
    /// in. An absolute path needs no `$PATH`, so the whole of `command_line` can be checked here: a
    /// link to the shim is started as the link, not as what it points at.
    #[cfg(unix)]
    #[test]
    fn a_configured_link_to_a_gui_shim_runs_as_the_link() {
        let scratch = Scratch::new("configured");
        scratch.program("mvim");
        let asked = scratch.path.join("vim");
        std::os::unix::fs::symlink("mvim", &asked).expect("link");

        let (program, arguments) =
            command_line(asked.to_str().unwrap()).expect("the editor is found");

        assert_eq!(
            program, asked,
            "the configured editor ran under the shim's own name, which forks a GUI window"
        );
        assert!(arguments.is_empty());
    }

    /// The whole choice, against an installation laid out the way MacVim's is: `vim` and `vi` on
    /// the `$PATH` are links into a bundle holding one shim, which reads its own `argv[0]` and
    /// forks a GUI window for the `m*` spellings. What runs has to be the name that was asked for,
    /// and no waiting flag belongs on a terminal editor.
    #[cfg(unix)]
    #[test]
    fn a_terminal_editor_behind_a_gui_shim_is_started_under_its_own_name() {
        use bravebot_sandbox::programs::find_in;
        let scratch = Scratch::new("macvim");
        let bundle = scratch.path.join("MacVim.app/Contents/bin");
        std::fs::create_dir_all(&bundle).expect("create");
        let shim = bundle.join("mvim");
        std::fs::write(&shim, "#!/bin/sh\n").expect("write");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        std::os::unix::fs::symlink("mvim", bundle.join("vim")).expect("link");
        for name in ["vim", "vi"] {
            std::os::unix::fs::symlink(bundle.join("vim"), scratch.path.join(name)).expect("link");
        }
        let path = std::env::join_paths([&scratch.path]).unwrap();

        for name in ["vim", "vi"] {
            let (program, arguments) =
                split(name, |command| find_in(OsStr::new(command), &path, &[]))
                    .expect("the editor is found");

            assert_eq!(
                program,
                scratch.path.join(name),
                "{name} was started under the shim's own name, which forks a GUI window"
            );
            assert!(
                arguments.is_empty(),
                "a terminal editor was given {arguments:?}"
            );
        }
    }

    /// The name only survives while it is the same program. A link repointed at something else is
    /// not an editor to start on a guess, and neither is one that no longer resolves: the lookup
    /// settled on a file, and that file is what runs.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_no_longer_the_same_program_falls_back_to_the_resolved_path() {
        let scratch = Scratch::new("elsewhere");
        let emacs = scratch.program("emacs").canonicalize().unwrap();
        let asked = scratch.path.join("editor");

        scratch.program("vim");
        std::os::unix::fs::symlink("vim", &asked).expect("link");
        assert_eq!(
            started_as(asked.to_str().unwrap(), emacs.clone(), &scratch.path),
            emacs,
            "a link repointed at another program was started under its name"
        );

        std::fs::remove_file(&asked).expect("unlink");
        std::os::unix::fs::symlink("gone", &asked).expect("dangling link");
        assert_eq!(
            started_as(asked.to_str().unwrap(), emacs.clone(), &scratch.path),
            emacs,
            "a link that no longer resolves was started under its name"
        );

        let relative = started_as("editor", emacs.clone(), &scratch.path);
        assert_eq!(
            relative, emacs,
            "a relative name that no longer resolves was started under its name"
        );
    }

    /// CRED-14: the editor a click opens is started without the variables this agent's own
    /// credentials arrive in, and with the editor's arguments then the file, while the editor a
    /// prompt is edited in withholds none. A start that inherited everything would hand those
    /// credentials to whatever a workspace file makes the editor run.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_editor_a_click_opens_is_started_without_this_agents_own_credentials() {
        fn withheld_from(command: &std::process::Command) -> Vec<String> {
            let mut names: Vec<String> = command
                .get_envs()
                .filter(|(_, value)| value.is_none())
                .map(|(name, _)| name.to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
        let program = Path::new("/usr/bin/vim");
        let arguments = vec!["+3".to_string()];

        let click =
            file_command(program, &arguments, OsStr::new("/work/a.rs")).expect("an absolute path");
        let mut expected = bravebot_config::scrub::own_credentials();
        expected.sort();
        assert!(
            !expected.is_empty(),
            "no credential is withheld to be found"
        );
        assert_eq!(withheld_from(&click), expected);
        assert_eq!(
            click.get_args().collect::<Vec<_>>(),
            ["+3", "/work/a.rs"],
            "the file is not the last argument, after the editor's own"
        );

        let prompt = command_for(
            program,
            &arguments,
            &[OsStr::new("/work/prompt.md")],
            bravebot_sandbox::programs::Inherits::All,
        )
        .expect("an absolute path");
        assert!(
            withheld_from(&prompt).is_empty(),
            "the editor a prompt is edited in is not given all of the environment"
        );
    }

    /// SANDBOX-30: the editor is started unconfined, so a file a confined stage wrote under its
    /// name must never be the one that runs. The test binary runs again with `PATH` holding a
    /// directory the session keeps out ahead of one it does not, each holding a `vim`, and asks
    /// for the editor through `command_line`. A lookup that searched `$PATH` itself picks the
    /// planted `vim` first and fails the child; one that gives up when the first entry is refused
    /// finds nothing and fails it too.
    #[cfg(unix)]
    #[test]
    fn an_editor_found_only_in_a_directory_a_stage_may_write_is_not_started() {
        const TEST: &str =
            "editor::tests::an_editor_found_only_in_a_directory_a_stage_may_write_is_not_started";
        const SESSION: &str = "BRAVEBOT_TEST_EDITOR_KEPT_OUT_SESSION";
        const SYSTEM: &str = "BRAVEBOT_TEST_EDITOR_KEPT_OUT_SYSTEM";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            let (program, _) = command_line("vim").expect("the installed editor is found");

            assert_eq!(
                program,
                PathBuf::from(std::env::var_os(SYSTEM).unwrap()).join("vim"),
                "the editor started was not the installed one"
            );
            return;
        }

        let session = crate::testutil::scratch_dir("editor-kept-out-session");
        let system = crate::testutil::scratch_dir("editor-kept-out-system");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        for directory in [session.join(".venv/bin"), system.clone()] {
            std::fs::create_dir_all(&directory).unwrap();
            let editor = directory.join("vim");
            std::fs::write(&editor, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([session.join(".venv/bin"), system.clone()]).unwrap();

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
            .env(SESSION, &session)
            .env(SYSTEM, &system)
            .env("PATH", &path)
            .output()
            .unwrap();

        let ran = session.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    /// With nothing but the planted directory on the `$PATH` there is no editor at all, which is
    /// the answer a configured name must get rather than the planted file.
    #[cfg(unix)]
    #[test]
    fn an_editor_named_only_by_a_planted_file_is_not_found() {
        const TEST: &str = "editor::tests::an_editor_named_only_by_a_planted_file_is_not_found";
        const SESSION: &str = "BRAVEBOT_TEST_EDITOR_PLANTED_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            assert_eq!(command_line("bravebot-planted-editor --wait"), None);
            return;
        }

        let session = crate::testutil::scratch_dir("editor-planted-session");
        let _ = std::fs::remove_dir_all(&session);
        let bin = session.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        let editor = bin.join("bravebot-planted-editor");
        std::fs::write(&editor, "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
            .env(SESSION, &session)
            .env("PATH", &bin)
            .output()
            .unwrap();

        let ran = session.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    /// What an editor did when [`start`] ran it in a child process of the test binary, with programs
    /// of the same name planted in a directory the session keeps out.
    #[cfg(unix)]
    struct Started {
        /// The editor ran to its end.
        edited: bool,
        /// A program planted in the directory the session keeps out ran.
        planted: bool,
        /// What the child printed, for a failure.
        report: String,
        succeeded: bool,
    }

    #[cfg(unix)]
    const STARTED_SESSION: &str = "BRAVEBOT_TEST_EDITOR_STARTED_SESSION";
    #[cfg(unix)]
    const STARTED_IN_SESSION: &str = "BRAVEBOT_TEST_EDITOR_STARTED_IN_SESSION";
    #[cfg(unix)]
    const STARTED_COMMAND: &str = "BRAVEBOT_TEST_EDITOR_STARTED_COMMAND";

    /// The child half of [`run_started_editor`]: reports whether it ran, so a test returns at once
    /// when it is the child.
    #[cfg(unix)]
    fn started_child() -> bool {
        let Some(session) = std::env::var_os(STARTED_SESSION) else {
            return false;
        };
        let session = PathBuf::from(session);
        bravebot_sandbox::programs::keep_out(&session);
        if std::env::var_os(STARTED_IN_SESSION).is_some() {
            std::env::set_current_dir(&session).unwrap();
        }
        let file = session.join("prompt.md");
        std::fs::write(&file, "").unwrap();
        match std::env::var(STARTED_COMMAND) {
            Ok(command) => {
                let (program, arguments) = command_line(&command).expect("the editor is found");
                start(&program, &arguments, &file)
            }
            Err(_) => start(&session.join("editor"), &[], &file),
        }
        .expect("the editor starts and exits cleanly");
        true
    }

    /// Run `start` in a child of this test binary whose `PATH` is `entries` (given the session
    /// directory and a directory of installed tools). The editor is a script with an `env` shebang,
    /// or the configured `command` where one is given. A `sh` and a `bravebot-env-tool` that record
    /// that they ran are planted at `<session>/.venv/bin`, the `sh` also at `<session>/sh`, and the
    /// installed `bravebot-env-tool` is in the tools directory. The session is kept out of the
    /// search, and the child runs from it when `in_session`.
    #[cfg(unix)]
    fn run_started_editor(
        test: &str,
        name: &str,
        in_session: bool,
        command: Option<&str>,
        entries: impl FnOnce(&Path, &Path) -> Vec<PathBuf>,
    ) -> Started {
        use std::os::unix::fs::PermissionsExt;
        let session = crate::testutil::scratch_dir(name);
        let tools = crate::testutil::scratch_dir(&format!("{name}-tools"));
        for directory in [&session, &tools] {
            let _ = std::fs::remove_dir_all(directory);
        }
        std::fs::create_dir_all(session.join(".venv/bin")).unwrap();
        std::fs::create_dir_all(&tools).unwrap();
        let script = |path: PathBuf, body: String| {
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let planted = |runs: &str| {
            format!(
                "#!/bin/sh\n: > '{}'\nexec {runs} \"$@\"\n",
                session.join("planted-ran").display()
            )
        };
        script(session.join(".venv/bin/sh"), planted("/bin/sh"));
        script(session.join("sh"), planted("/bin/sh"));
        script(
            session.join(".venv/bin/bravebot-env-tool"),
            planted(&format!("'{}'", tools.join("bravebot-env-tool").display())),
        );
        script(
            tools.join("bravebot-env-tool"),
            "#!/bin/sh\n: > \"$1.edited\"\n".to_string(),
        );
        script(
            session.join("editor"),
            "#!/usr/bin/env sh\n: > \"$1.edited\"\n".to_string(),
        );
        let path = std::env::join_paths(entries(&session, &tools)).unwrap();

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env(STARTED_SESSION, &session)
            .env("PATH", &path);
        if in_session {
            child.env(STARTED_IN_SESSION, "1");
        }
        if let Some(command) = command {
            child.env(STARTED_COMMAND, command);
        }
        let child = child.output().unwrap();

        let outcome = Started {
            edited: session.join("prompt.md.edited").exists(),
            planted: session.join("planted-ran").exists(),
            report: String::from_utf8_lossy(&child.stderr).into_owned(),
            succeeded: child.status.success(),
        };
        for directory in [&session, &tools] {
            let _ = std::fs::remove_dir_all(directory);
        }
        outcome
    }

    /// SANDBOX-30: an editor that is a script with an `env` shebang, or a launcher that runs another
    /// program by name, looks that program up on the `PATH` it was started with. A `sh` a confined
    /// stage wrote into a directory the session keeps out, first on that `PATH`, must not be the one
    /// that runs: the editor is started unconfined, with the person's access to `~/.ssh` and
    /// `~/.aws`. The system `sh` behind it is the one found, and the editor still runs to its end.
    #[cfg(unix)]
    #[test]
    fn an_editor_that_runs_another_program_by_name_never_runs_one_a_stage_may_have_written() {
        const TEST: &str = "editor::tests::an_editor_that_runs_another_program_by_name_never_runs_one_a_stage_may_have_written";
        if started_child() {
            return;
        }

        let outcome = run_started_editor(TEST, "editor-env-shebang", false, None, |session, _| {
            vec![
                session.join(".venv/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        });

        assert!(outcome.succeeded, "{}", outcome.report);
        assert!(
            outcome.edited,
            "the editor did not run to its end: {}",
            outcome.report
        );
        assert!(
            !outcome.planted,
            "the editor looked up `sh` in a directory a stage may write"
        );
    }

    /// The same for an editor configured as a command line that names another program for the
    /// first to run, `env` and a tool here. Only the first word is looked up by the editor's own
    /// search, so the rest reaches `env` as an argument and is found through the `PATH` `env` was
    /// started with. The tool installed behind the planted one is the one that runs.
    #[cfg(unix)]
    #[test]
    fn an_editor_configured_as_env_and_a_name_never_runs_one_a_stage_may_have_written() {
        const TEST: &str = "editor::tests::an_editor_configured_as_env_and_a_name_never_runs_one_a_stage_may_have_written";
        if started_child() {
            return;
        }

        let outcome = run_started_editor(
            TEST,
            "editor-env-command",
            false,
            Some("env bravebot-env-tool"),
            |session, tools| {
                vec![
                    session.join(".venv/bin"),
                    tools.to_path_buf(),
                    PathBuf::from("/usr/bin"),
                    PathBuf::from("/bin"),
                ]
            },
        );

        assert!(outcome.succeeded, "{}", outcome.report);
        assert!(
            outcome.edited,
            "the installed tool did not run: {}",
            outcome.report
        );
        assert!(
            !outcome.planted,
            "`env` looked its argument up in a directory a stage may write"
        );
    }

    /// With every entry of the `PATH` a directory the session keeps out, nothing is left to search.
    /// The editor must then be started with no `PATH` at all: an empty one is read by `execvp` as
    /// the current directory, which for a session is the directory it writes in. Here the editor
    /// runs from the session, holding a `sh` of its own, and the system one is the one found.
    #[cfg(unix)]
    #[test]
    fn an_editor_with_no_searched_path_entry_left_does_not_search_the_current_directory() {
        const TEST: &str = "editor::tests::an_editor_with_no_searched_path_entry_left_does_not_search_the_current_directory";
        if started_child() {
            return;
        }

        let outcome = run_started_editor(TEST, "editor-env-empty", true, None, |session, _| {
            vec![session.join(".venv/bin")]
        });

        assert!(outcome.succeeded, "{}", outcome.report);
        assert!(
            outcome.edited,
            "the editor did not run to its end: {}",
            outcome.report
        );
        assert!(
            !outcome.planted,
            "the editor looked up `sh` in the directory it was started in"
        );
    }

    /// The four the clause names, wherever the binary ships. A platform that has a list of its
    /// own tries none of them, however many of them are installed there, and this is the only
    /// statement a test can make about a constant the compiler chooses.
    #[test]
    fn the_fallback_list_is_the_same_on_every_platform() {
        assert_eq!(FALLBACKS, ["vim", "vi", "emacs", "nano"]);
    }

    /// Someone with `vim` or `emacs` installed chose to install it, and opening `nano` at them
    /// instead is a guess overriding an answer they already gave. `nano` stays on the list for the
    /// person who has neither, and stays last.
    #[test]
    fn a_full_editor_is_preferred_to_the_last_resort() {
        let order = |name: &str| FALLBACKS.iter().position(|entry| *entry == name);

        assert!(order("vim") < order("nano"));
        assert!(order("vi") < order("nano"));
        assert!(order("emacs") < order("nano"));
        assert_eq!(order("nano"), Some(FALLBACKS.len() - 1));
    }

    /// A name the user exported is an answer. Trying the list after it would open an editor they
    /// did not choose and leave them thinking their configuration had worked.
    #[test]
    fn a_configured_editor_that_will_not_start_ends_the_search() {
        let outcome = which_editor(Some("definitely-not-an-editor-on-this-machine".to_string()));

        assert!(
            matches!(outcome, Err(Failure::Editor(_))),
            "the fallback list was tried behind a configured editor"
        );
    }

    /// The one difference between showing a transcript and editing a prompt, and the whole of it.
    /// A record that can be edited into the session is not a record, so whatever the editor saved
    /// stays in the editor.
    #[test]
    fn a_transcript_opened_in_the_editor_is_never_read_back() {
        let place = a_place();
        let mut opened = None;
        show_through(Some(place.path()), "what happened", |path| {
            assert_eq!(
                std::fs::read_to_string(path).expect("the transcript is there"),
                "what happened"
            );
            std::fs::write(path, "what did not happen").unwrap();
            opened = Some(path.to_path_buf());
            Ok(())
        })
        .expect("the editor opens");

        // Nothing came back to compare, which is the property: the call returns no text at all,
        // so there is no path by which an edited record could reach the session.
        assert!(opened.is_some(), "the editor was never handed a file");
    }

    /// A closure standing in for the editor runs before anything `show_through` does afterwards, so
    /// a read of the file added after it returns is invisible to
    /// `a_transcript_opened_in_the_editor_is_never_read_back`. This test makes the file unreadable
    /// from inside that closure: the editor leaves a directory where the transcript was, which
    /// cannot be read as text by anyone, whatever account the test runs as. Showing the transcript
    /// still succeeds, so nothing reads the file after the editor exits and a failure to read it
    /// cannot become the call's result.
    ///
    /// Rejects: a `show` that reads the file back under any helper name, such as the read of
    /// `through_a_file` left in place when the transcript is shown, which a source scan for the
    /// names of the standard calls does not see once the read is behind another function.
    #[cfg(unix)]
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn showing_a_transcript_succeeds_when_the_file_cannot_be_read_after_the_editor() {
        let place = a_place();
        let mut replaced = None;
        let shown = show_through(Some(place.path()), "what happened", |path| {
            std::fs::remove_file(path).expect("the transcript is there");
            std::fs::create_dir(path).expect("a directory takes the file's place");
            replaced = Some(path.to_path_buf());
            Ok(())
        });
        let replaced = replaced.expect("the editor was never handed a file");
        // The directory made for the file goes with it, and the directory the editor left is in it.
        assert!(!replaced.exists(), "the directory the editor left was kept");
        assert!(
            shown.is_ok(),
            "a transcript the editor left unreadable made showing it fail: {shown:?}"
        );
    }

    /// A closure standing in for the editor runs before anything `show_through` does afterwards, so
    /// a read of the file added after it returns is invisible to
    /// `a_transcript_opened_in_the_editor_is_never_read_back`. The three functions between the key
    /// and the file are scanned instead: none of them reads a file or goes through the round trip
    /// that does, which is what would let an edited transcript reach the session.
    #[test]
    fn showing_a_transcript_reads_no_file_back() {
        let editor = include_str!("editor.rs");
        let terminal = include_str!("app/terminal.rs");
        // The function each one hands the transcript to, so the chain scanned is the chain that runs.
        let chain = [
            (editor, "pub fn show(text: &str)", "show_through("),
            (editor, "fn show_through(", "write_scratch("),
            (
                terminal,
                "pub(super) fn show_transcript(",
                "crate::editor::show(",
            ),
        ];
        let reads = [
            "read_to_string",
            "read_to_end",
            "read_exact",
            "read_line",
            "fs::read(",
            ".read(",
            "File::open",
            "OpenOptions",
            "BufReader",
            "read_dir",
            "metadata",
            "round_trip",
            "through_a_file",
            "take_edited",
        ];
        for (source, signature, callee) in chain {
            let from = source.find(signature).unwrap_or_else(|| {
                panic!("`{signature}` is not in the source it was looked up in")
            });
            let rest = &source[from..];
            let body = &rest[..rest.find("\n}\n").expect("the function ends")];
            assert!(
                body.contains(callee),
                "`{signature}` no longer reaches `{callee}`, so the scan below follows a path that does not run"
            );
            for read in reads {
                assert!(
                    !body.contains(read),
                    "`{signature}` contains `{read}`, a way to read the transcript file back"
                );
            }
        }
    }

    /// A transcript holds content nobody vouched for. It goes where scratch files go, and not
    /// into the workspace, where the next turn would find it as a file somebody had written. That
    /// is the directory made for it under the cache directory, which is the one place named, and not
    /// the system temporary directory a stage may write.
    #[cfg(unix)]
    #[test]
    fn a_transcript_opened_in_the_editor_is_written_outside_the_workspace() {
        let place = a_place();
        let mut written = None;
        show_through(Some(place.path()), "what happened", |path| {
            written = Some(path.to_path_buf());
            Ok(())
        })
        .expect("the editor opens");

        let written = written.expect("the editor was handed a file");
        assert!(
            written.starts_with(place.path().join("bravebot").join("editor")),
            "the transcript was written to {}",
            written.display()
        );
        assert_ne!(
            written.parent(),
            std::env::current_dir().ok().as_deref(),
            "the transcript was written into the working directory"
        );
    }

    /// What is left under the directory the files are made in, once an edit is over: the directories
    /// above the one made for each file stay, and each file's own goes with it.
    #[cfg(unix)]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn left_under(place: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(place.join("bravebot").join("editor"))
            .expect("the directory the files are made in")
            .flatten()
            .map(|entry| entry.path())
            .collect()
    }

    /// It does not outlive the look at it, down either path, and neither does the directory made for
    /// it. A transcript left behind is the session's contents sitting on disk after the editor is
    /// closed.
    ///
    /// The regression it rejects is a removal of the file that leaves its directory, which passes a
    /// check of the file alone.
    #[cfg(unix)]
    #[test]
    fn the_file_goes_when_the_editor_exits() {
        let place = a_place();
        let mut left = None;
        show_through(Some(place.path()), "what happened", |path| {
            left = Some(path.to_path_buf());
            Ok(())
        })
        .expect("the editor opens");
        assert!(
            !left.expect("a file").exists(),
            "the transcript was left behind"
        );
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());

        let mut left = None;
        let failed = show_through(Some(place.path()), "what happened", |path| {
            left = Some(path.to_path_buf());
            Err(Failure::Editor("no".to_string()))
        });
        assert!(failed.is_err());
        assert!(
            !left.expect("a file").exists(),
            "an editor that failed left the transcript behind"
        );
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());
    }

    /// The file and its directory go when the editor's own call panics, which unwinds out of
    /// [`round_trip`] without reaching the explicit removal after the call.
    ///
    /// The regression it rejects is a `ScratchFile` with no `Drop`, which removes the file only on
    /// the paths that return.
    #[cfg(unix)]
    #[expect(clippy::panic, reason = "the editor standing in for a real one panics")]
    #[test]
    fn a_panic_while_the_editor_has_the_file_leaves_nothing_behind() {
        let place = a_place();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            round_trip(Some(place.path()), "something private", |_| {
                panic!("the editor panicked")
            })
        }));
        assert!(outcome.is_err(), "the editor did not panic");
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());
    }

    /// [`round_trip`] in a place of its own, for a test that looks only at what comes back.
    fn round(
        line: &str,
        open: impl FnOnce(&Path) -> Result<(), Failure>,
    ) -> Result<String, Failure> {
        let place = a_place();
        round_trip(Some(place.path()), line, open)
    }

    /// The whole point of the round trip: what was saved is what comes back.
    #[test]
    fn what_the_editor_saved_becomes_the_line() {
        let line = round("first thoughts", |path| {
            std::fs::write(path, "second thoughts").unwrap();
            Ok(())
        })
        .expect("the round trip completes");

        assert_eq!(line, "second thoughts");
    }

    /// Quitting without saving must not blank the prompt. The file already holds the line, so
    /// what comes back is what went in, and not saving costs the edits rather than the prompt.
    #[test]
    fn quitting_without_saving_leaves_the_line_as_it_was() {
        let line = round("first thoughts", |_| Ok(())).expect("the round trip completes");

        assert_eq!(line, "first thoughts");
    }

    /// The editor opens on what has been typed so far, not on an empty file: pressing the key
    /// halfway through a prompt continues it rather than starting again.
    #[test]
    fn the_editor_opens_on_what_was_already_typed() {
        round("half a thought", |path| {
            assert_eq!(std::fs::read_to_string(path).unwrap(), "half a thought");
            Ok(())
        })
        .expect("the round trip completes");
    }

    /// A prompt is the user's own words and has no business staying on disk after the edit, whether
    /// the edit worked or not, and the directory made for it goes too.
    #[cfg(unix)]
    #[test]
    fn the_file_does_not_outlive_the_edit() {
        let place = a_place();
        let mut opened = PathBuf::new();
        round_trip(Some(place.path()), "something private", |path| {
            opened = path.to_path_buf();
            Ok(())
        })
        .expect("the round trip completes");
        assert!(!opened.exists(), "the file was left behind after an edit");
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());

        let mut failed = PathBuf::new();
        let outcome = round_trip(Some(place.path()), "something private", |path| {
            failed = path.to_path_buf();
            Err(Failure::Editor("no".to_string()))
        });
        assert!(outcome.is_err());
        assert!(!failed.exists(), "the file was left behind after a failure");
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());
    }

    /// The file holds the user's own words, so no group or other bit may be set on it while the
    /// editor has it open. A stricter umask only narrows it.
    #[cfg(unix)]
    #[test]
    fn the_file_the_editor_opens_is_readable_by_nobody_else() {
        use std::os::unix::fs::PermissionsExt;
        let mode_of = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let place = a_place();

        let mut prompt = 0;
        round_trip(Some(place.path()), "something private", |path| {
            prompt = mode_of(path);
            Ok(())
        })
        .expect("the round trip completes");
        assert_eq!(prompt & 0o077, 0, "the prompt file had mode {prompt:o}");

        let mut transcript = 0;
        show_through(Some(place.path()), "what happened", |path| {
            transcript = mode_of(path);
            Ok(())
        })
        .expect("the editor opens");
        assert_eq!(
            transcript & 0o077,
            0,
            "the transcript file had mode {transcript:o}"
        );
    }

    /// An editor that failed says nothing about what the user wanted, so the line stays as it
    /// was rather than being replaced by whatever happened to be in the file.
    #[test]
    fn an_editor_that_failed_does_not_produce_a_line() {
        let outcome = round("first thoughts", |path| {
            std::fs::write(path, "half an edit").unwrap();
            Err(Failure::Editor("stopped".to_string()))
        });

        assert!(matches!(outcome, Err(Failure::Editor(_))));
    }

    /// Editors end a file with a newline. Kept, it would draw an empty line under the prompt on
    /// every trip through the editor, and they would accumulate.
    #[test]
    fn the_newline_an_editor_leaves_at_the_end_is_dropped() {
        let mut text = "a prompt\n".to_string();
        tidy(&mut text);
        assert_eq!(text, "a prompt");
    }

    /// One newline, not every one: a blank line at the end of a paragraph was typed on purpose.
    #[test]
    fn only_the_last_newline_goes() {
        let mut text = "a prompt\n\n".to_string();
        tidy(&mut text);
        assert_eq!(text, "a prompt\n");
    }

    /// A file saved by an editor on Windows, or edited over one, comes back with the line endings
    /// the box draws, which are the ones a paste is normalised to.
    #[test]
    fn line_endings_come_back_the_way_a_paste_does() {
        let mut text = "one\r\ntwo\rthree\n".to_string();
        tidy(&mut text);
        assert_eq!(text, "one\ntwo\nthree");
    }

    /// WRITE-7: the body of a file comes back as the editor wrote it. A prompt is fitted to a box,
    /// and a file is not: the newline an editor ends it with and its line endings are part of what
    /// the person approves, and `tidy` here would strip the one and rewrite the other.
    #[test]
    fn a_body_comes_back_exactly_as_the_editor_wrote_it() {
        let written = "fn main() {\r\n    run();\r\n}\r\n\r\n";
        let place = a_place();
        let saved = body_round_trip(Some(place.path()), "fn main() {}\n", "rs", |path| {
            std::fs::write(path, written).unwrap();
            Ok(())
        })
        .expect("the round trip completes");
        assert_eq!(saved, written);

        let untouched = body_round_trip(Some(place.path()), "no final newline", "txt", |_| Ok(()))
            .expect("the round trip completes");
        assert_eq!(untouched, "no final newline");
    }

    /// WRITE-7: a body is as private as a prompt: the file is removed whether the editor worked or
    /// not, and carries no group or other bit while it is open. The extension is the path's, so the
    /// editor treats the text as the kind of file it is for.
    #[cfg(unix)]
    #[test]
    fn the_file_a_body_is_edited_in_is_private_named_for_its_kind_and_removed() {
        use std::os::unix::fs::PermissionsExt;
        let place = a_place();
        let mut opened = PathBuf::new();
        let mut mode = 0;
        body_round_trip(Some(place.path()), "the body", "rs", |path| {
            opened = path.to_path_buf();
            mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            Ok(())
        })
        .expect("the round trip completes");
        assert_eq!(opened.extension().and_then(OsStr::to_str), Some("rs"));
        assert_eq!(mode & 0o077, 0, "the body's file had mode {mode:o}");
        assert!(!opened.exists(), "the file was left behind after an edit");
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());

        let mut failed = PathBuf::new();
        let outcome = body_round_trip(Some(place.path()), "the body", "rs", |path| {
            failed = path.to_path_buf();
            Err(Failure::Editor("no".to_string()))
        });
        assert!(outcome.is_err());
        assert!(!failed.exists(), "the file was left behind after a failure");
        assert_eq!(left_under(place.path()), Vec::<PathBuf>::new());
    }

    /// WRITE-7: the scratch file takes the extension of the last component of the path, where that
    /// is a few letters or digits, and `txt` for everything else. The path is the planner's, so
    /// nothing else of it reaches a file name.
    #[test]
    fn a_scratch_file_takes_a_plain_extension_from_the_path_and_nothing_else() {
        for (path, extension) in [
            ("src/main.rs", "rs"),
            ("notes.md", "md"),
            ("archive.tar.gz", "gz"),
            ("Makefile", "txt"),
            (".gitignore", "txt"),
            ("dir.d/file", "txt"),
            ("ref:1", "txt"),
            ("odd.r s", "txt"),
            ("odd.a/b", "txt"),
            ("odd.\u{fffd}", "txt"),
            ("long.abcdefghijklmnopqrstuvwxyz", "txt"),
            ("empty.", "txt"),
        ] {
            assert_eq!(scratch_extension(path), extension, "{path}");
        }
    }

    /// A bare GUI editor exits the moment its window opens, and the line would come back
    /// unchanged with nothing to say why.
    #[test]
    fn a_gui_editor_is_told_to_wait() {
        assert_eq!(
            waiting_flag(Path::new("/usr/local/bin/code")),
            Some("--wait".to_string())
        );
    }

    /// The flag belongs to the program, not to the name it was reached by, so a resolved path
    /// and an extension are both still that program.
    #[test]
    fn the_flag_follows_the_program_through_a_path() {
        assert_eq!(
            waiting_flag(Path::new("code.cmd")),
            Some("--wait".to_string())
        );
    }

    /// Someone who wrote arguments for their editor chose how it runs, and a flag added behind
    /// theirs can contradict them or repeat what they already wrote.
    #[test]
    fn a_gui_editor_is_told_to_wait_only_where_the_user_wrote_no_arguments_of_their_own() {
        let find = |name: &str| (name == "code").then(|| PathBuf::from("/usr/local/bin/code"));

        let (_, bare) = split("code", find).expect("the editor is found");
        assert_eq!(bare, ["--wait"]);

        for written in ["code --new-window", "code --wait"] {
            let (program, arguments) = split(written, find).expect("the editor is found");
            assert_eq!(program, PathBuf::from("/usr/local/bin/code"));
            let own: Vec<String> = written
                .split_whitespace()
                .skip(1)
                .map(String::from)
                .collect();
            assert_eq!(arguments, own, "the user's arguments were added to");
        }
    }

    /// A terminal editor already waits, and an argument it did not ask for is one it may refuse
    /// to start over.
    #[test]
    fn a_terminal_editor_is_given_no_extra_flag() {
        assert_eq!(waiting_flag(Path::new("/usr/bin/vim")), None);
        assert_eq!(waiting_flag(Path::new("nano")), None);
    }
}
