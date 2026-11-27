//! Where the token for a Google Vertex AI project that takes no API key comes from.
//!
//! The Google Cloud CLI is asked, rather than the Application Default Credentials file being read
//! here. That file holds a refresh token or a service account key, and which of them applies is
//! decided by `GOOGLE_APPLICATION_CREDENTIALS`, the CLI's own configuration and impersonation
//! settings, all of which `gcloud` already resolves.
//!
//! The subprocess is fixed argv: the program is `gcloud`, the subcommands are constants in this
//! file, and no value from a settings file, a model or the workspace reaches it. Which file `gcloud`
//! is can be workspace content, so the lookup leaves out the directories a stage may write
//! ([`bravebot_sandbox::programs::find_on`]).
//!
//! Three of the variables the CLI reads name code it then runs: `CLOUDSDK_PYTHON` is the interpreter
//! its launcher executes, the launcher sources `$CLOUDSDK_CONFIG/virtenv/bin/activate` when it
//! has no interpreter of its own, and `GOOGLE_APPLICATION_CREDENTIALS` names a file that, as an
//! `external_account` credential, holds a command the CLI runs. The login the CLI is asked for when
//! no token can be had opens a browser, which reads `BROWSER` and the `XDG_` directories to choose
//! the program it runs. [`CODE_SELECTORS`] lists them.
//!
//! The start passes [`bravebot_sandbox::programs::CodeSelectors::Checked`]. A variable in the list
//! whose value is a path a stage may write is taken off the environment the CLI is started with,
//! and so is each generic name every checked start removes.
//!
//! Variables the CLI's launcher and auth code read to choose code to run are on that list too, among
//! them `PYTHONPATH` and `CLOUDSDK_PYTHON_ARGS`. A value that is a list of paths is judged by every
//! entry, and one that holds options is taken off whatever it holds.
//!
//! The token is a credential. [`Secret`] keeps it out of a `Debug` render, the bytes the CLI
//! printed are cleared once read, and the only use made of it is the `authorization` header of a
//! request to the host built from the project and location.
//!
//! `gcloud auth application-default print-access-token` states no expiry, so no answer is kept: a
//! token is asked for with each request and a check before a turn is one more run of the program.

use bravebot_config::Secret;
use bravebot_i18n::t;
use bravebot_sandbox::programs::{Names, selectors};
use std::ffi::{OsStr, OsString};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// The program asked for a token, looked for on `PATH` as any command a person runs is.
const GCLOUD: &str = "gcloud";

/// The variables the CLI reads that name code it runs, and what each names. They are judged on top
/// of the generic names ([`bravebot_sandbox::programs::CODE_SELECTORS`]) every checked start
/// removes.
///
/// A value is kept only when it is outside every directory a stage may write
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). A variable that selects code and
/// is in neither list reaches the CLI as the person set it, so one added to the CLI's launcher
/// belongs in this list.
///
/// - `CLOUDSDK_PYTHON` is the interpreter the launcher executes, and the launcher sources
///   `$CLOUDSDK_CONFIG/virtenv/bin/activate` when it has no interpreter of its own.
/// - `CLOUDSDK_PYTHON_ARGS` is placed before `gcloud.py`, so `-c <code>` and `-m <module>` run. It
///   names no path, so it is removed whatever it holds.
/// - The launcher passes neither `-E` nor `-I`, so `PYTHONPATH` (`sitecustomize.py` and any module
///   it shadows), `PYTHONHOME`, `PYTHONUSERBASE` (`usercustomize.py` and `.pth` files) and
///   `PYTHONPYCACHEPREFIX` (an unchecked `.pyc`) choose code the interpreter runs.
/// - `GOOGLE_APPLICATION_CREDENTIALS` names a file that, for an `external_account` credential,
///   names a command to run and a file to read and send to a host the file names.
/// - The two `CLOUDSDK_CONTEXT_AWARE_*` files name a binary or a certificate provider command the
///   CLI runs when `context_aware/use_client_certificate` is true.
/// - `BROWSER` is what `gcloud auth login` opens a page with, and `OPENSSL_CONF` can load a
///   provider module into the OpenSSL Python loads. Both are shared with the other lists
///   ([`bravebot_sandbox::programs::selectors`]). `BROWSER` is judged as a list of commands.
/// - `gcloud auth application-default login` ([`login_command`]) opens the sign-in page with
///   Python's `webbrowser`, which with a display runs `xdg-open`, which reads the four `XDG_`
///   directories to choose the handler it runs. The names and kinds are those of the browser
///   opener (`crates/tui/src/opener.rs`) and the AWS start (`crates/bedrock/src/credentials.rs`).
///   `XDG_DATA_DIRS` is also in the generic list and is repeated here so this list reads as
///   complete.
/// - `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` are where `gio open`, which `xdg-open` runs
///   under a GNOME session, finds the session bus (`$XDG_RUNTIME_DIR/bus` when the address is not
///   set). A service a stage runs on a socket it can create answers as the bus. They are judged as
///   the opener judges them: the directory as one directory and the address as a
///   [`Names::BusAddress`]. Whether a service on such a socket can make `gio open` run code or
///   choose a handler was not shown ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)).
const CODE_SELECTORS: [(&str, Names); 18] = [
    ("CLOUDSDK_PYTHON", Names::Program),
    ("CLOUDSDK_CONFIG", Names::Directory),
    ("CLOUDSDK_PYTHON_ARGS", Names::Options),
    selectors::PYTHONPATH,
    selectors::PYTHONHOME,
    selectors::PYTHONUSERBASE,
    selectors::PYTHONPYCACHEPREFIX,
    ("GOOGLE_APPLICATION_CREDENTIALS", Names::File),
    (
        "CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH",
        Names::File,
    ),
    (
        "CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH",
        Names::File,
    ),
    selectors::BROWSER,
    selectors::OPENSSL_CONF,
    ("XDG_DATA_HOME", Names::Directory),
    ("XDG_DATA_DIRS", Names::Directories),
    ("XDG_CONFIG_HOME", Names::Directory),
    ("XDG_CONFIG_DIRS", Names::Directories),
    selectors::XDG_RUNTIME_DIR,
    selectors::DBUS_SESSION_BUS_ADDRESS,
];

/// Why no token could be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcloudError {
    /// No `gcloud` was found on the usable part of `PATH`.
    NotInstalled,
    /// The CLI ran and did not give a token, or could not be run.
    Refused { detail: String },
}

impl std::fmt::Display for GcloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str(t!(gcloud_not_installed)),
            Self::Refused { detail } => f.write_str(&t!(gcloud_refused, detail = detail.as_str())),
        }
    }
}

impl std::error::Error for GcloudError {}

fn installed() -> PathBuf {
    installed_in(&std::env::var_os("PATH").unwrap_or_default())
}

fn installed_in(path: &OsStr) -> PathBuf {
    bravebot_sandbox::programs::find_on(OsStr::new(GCLOUD), path).unwrap_or_default()
}

/// A command for `program` with this agent's own credentials taken off it
/// ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)) and each variable that
/// selects code, generic or in [`CODE_SELECTORS`], that names a path a stage may write. The rest of
/// the environment stays, since the other `CLOUDSDK_*` values are what the CLI is run to read.
///
/// `None` for a `program` with no path separator, which is the empty path [`installed`] gives where
/// no `gcloud` was found. Only a path [`installed`] returned is started (SANDBOX-30).
fn command_for(program: &Path, args: &[&str]) -> Option<Command> {
    command_for_with(
        program,
        args,
        std::env::vars_os(),
        &bravebot_sandbox::programs::outside_stage_writable,
    )
}

/// [`command_for`] against a given environment and a given judgment of which files are outside the
/// directories a stage may write.
fn command_for_with(
    program: &Path,
    args: &[&str],
    held: impl IntoIterator<Item = (OsString, OsString)>,
    outside_stage_writable: &impl Fn(&Path) -> bool,
) -> Option<Command> {
    let held: Vec<(OsString, OsString)> = held.into_iter().collect();
    let mut command = bravebot_sandbox::programs::start_against(
        &bravebot_sandbox::programs::Resolved::found_earlier(program)?,
        bravebot_sandbox::programs::Inherits::Without(&bravebot_config::scrub::own_credentials()),
        bravebot_sandbox::programs::SearchPath::Reduced,
        bravebot_sandbox::programs::CodeSelectors::Checked(&CODE_SELECTORS),
        held.iter().cloned(),
        outside_stage_writable,
    );
    // The launcher runs Python, which reads `usercustomize.py` from the user site, which a stage
    // may be able to write (SANDBOX-30).
    bravebot_sandbox::programs::python_user_site_off_when_writable(
        &mut command,
        &held,
        outside_stage_writable,
    );
    command.args(args);
    Some(command)
}

/// The three argument lists the CLI is started with. Each is started by [`command_for`], so the
/// credential removal, the code selectors and the user site are judged the same for all of them.
const TOKEN_ARGS: &[&str] = &["auth", "application-default", "print-access-token"];
const LOGIN_ARGS: &[&str] = &["auth", "application-default", "login"];
/// `--quiet` answers the CLI's own "do you want to continue" question with its default, which is
/// yes: the person was asked by `auth logout google` before this is started.
const REVOKE_ARGS: &[&str] = &["auth", "application-default", "revoke", "--quiet"];

fn token_command(program: &Path) -> Option<Command> {
    command_for(program, TOKEN_ARGS)
}

fn login_command(program: &Path) -> Option<Command> {
    command_for(program, LOGIN_ARGS)
}

fn revoke_command(program: &Path) -> Option<Command> {
    command_for(program, REVOKE_ARGS)
}

fn start_piped(command: &mut Command) -> std::io::Result<std::process::Child> {
    bravebot_sandbox::process::starting_a_program(|| {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    })
}

fn spawn_error(e: &std::io::Error) -> GcloudError {
    match e.kind() {
        std::io::ErrorKind::NotFound => GcloudError::NotInstalled,
        _ => GcloudError::Refused {
            detail: e.to_string(),
        },
    }
}

/// An access token from the CLI, or why there is none.
pub fn token() -> Result<Secret, GcloudError> {
    token_from(&installed())
}

fn token_from(program: &Path) -> Result<Secret, GcloudError> {
    if program.as_os_str().is_empty() {
        return Err(GcloudError::NotInstalled);
    }
    let Some(mut command) = token_command(program) else {
        return Err(GcloudError::NotInstalled);
    };
    let mut output = start_piped(&mut command)
        .and_then(std::process::Child::wait_with_output)
        .map_err(|e| spawn_error(&e))?;
    if !output.status.success() {
        // A CLI that printed a token and then failed printed it all the same.
        bravebot_config::scrub_bytes(&mut output.stdout);
        return Err(GcloudError::Refused {
            detail: first_line(&output.stderr),
        });
    }
    decode(&mut output.stdout)
}

/// The token in what the CLI printed, which is one line with no whitespace in it.
///
/// Anything else is refused without quoting it, since a reply this cannot read may still hold a
/// token. The bytes are cleared whatever the answer.
fn decode(bytes: &mut [u8]) -> Result<Secret, GcloudError> {
    let token = std::str::from_utf8(bytes)
        .ok()
        .map(str::trim)
        .filter(|text| !text.is_empty() && !text.contains(char::is_whitespace))
        .map(Secret::new);
    bravebot_config::scrub_bytes(bytes);
    token.ok_or(GcloudError::Refused {
        detail: t!(gcloud_no_token).to_string(),
    })
}

/// The first line of the CLI's stderr, bounded.
fn first_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let no_reason = t!(gcloud_no_reason).to_string();
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(&no_reason)
        .chars()
        .take(200)
        .collect()
}

/// Whether the CLI gives a token now, without opening a browser.
pub fn is_signed_in() -> bool {
    token().is_ok()
}

/// [`is_signed_in`] with the reason it is not.
pub fn session() -> Result<(), GcloudError> {
    token().map(drop)
}

/// Sign in with `gcloud auth application-default login` where no token is to be had, reporting each
/// line the CLI writes as it arrives, since the URL and code in them are the flow
/// ([BACKEND-9](../../../docs/specs/backends.md#BACKEND-9)). Quiet when a token is already given.
pub fn sign_in_if_needed(say: impl FnMut(String)) -> Result<(), GcloudError> {
    if is_signed_in() {
        return Ok(());
    }
    login_from(&installed(), say)
}

/// Whether a `gcloud` is on the usable part of `PATH`, so a caller can say it is missing before it
/// asks for anything.
pub fn is_installed() -> bool {
    !installed().as_os_str().is_empty()
}

/// What [`sign_in_checked`] found or did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signed {
    /// The CLI already gave a token, so no login was run and no browser was opened.
    Already,
    /// The login was run and the CLI gives a token now.
    Now,
}

/// [`sign_in_if_needed`] for a person who asked to sign in, which also asks for a token once the
/// login has ended, since a login can end well for an account that still gives none
/// ([BACKEND-65](../../../docs/specs/backends.md#BACKEND-65)).
///
/// The program, its arguments and its environment are those of the sign-in a turn makes. Each line
/// the CLI writes goes to `say` as it arrives, and nothing a person typed reaches the program.
pub fn sign_in_checked(say: impl FnMut(String)) -> Result<Signed, GcloudError> {
    sign_in_checked_from(&installed(), say)
}

fn sign_in_checked_from(program: &Path, say: impl FnMut(String)) -> Result<Signed, GcloudError> {
    if token_from(program).is_ok() {
        return Ok(Signed::Already);
    }
    login_from(program, say)?;
    token_from(program).map(|_| Signed::Now)
}

/// End the Application Default Credentials the CLI holds with
/// `gcloud auth application-default revoke`, for a person who said to at the terminal. Every program
/// that reads those credentials stops working until a sign-in is made again, which is why this is
/// asked for by a typed answer and never by a turn.
///
/// Nothing the CLI writes is returned or shown: only its exit status decides the outcome.
pub fn revoke() -> Result<(), GcloudError> {
    revoke_from(&installed())
}

fn revoke_from(program: &Path) -> Result<(), GcloudError> {
    if program.as_os_str().is_empty() {
        return Err(GcloudError::NotInstalled);
    }
    let Some(command) = revoke_command(program) else {
        return Err(GcloudError::NotInstalled);
    };
    run_showing(command, |_| {}, t!(gcloud_revoke_incomplete).to_string())
}

fn login_from(program: &Path, say: impl FnMut(String)) -> Result<(), GcloudError> {
    if program.as_os_str().is_empty() {
        return Err(GcloudError::NotInstalled);
    }
    let Some(command) = login_command(program) else {
        return Err(GcloudError::NotInstalled);
    };
    run_showing(command, say, t!(gcloud_login_incomplete).to_string())
}

/// Run `command`, giving each line it writes to `say` as it arrives, and succeed where it exits 0.
/// `incomplete` is the detail of the failure of any other end.
fn run_showing(
    mut command: Command,
    mut say: impl FnMut(String),
    incomplete: String,
) -> Result<(), GcloudError> {
    let mut child = start_piped(&mut command).map_err(|e| spawn_error(&e))?;
    let (lines, arriving) = mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(stream) = child.stdout.take() {
        readers.push(read_lines(stream, lines.clone()));
    }
    if let Some(stream) = child.stderr.take() {
        readers.push(read_lines(stream, lines.clone()));
    }
    drop(lines);
    for line in arriving {
        say(line);
    }
    for reader in readers {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a reader that panicked has only lost lines of display, and the exit status decides the outcome"
        )]
        let _ = reader.join();
    }
    let status = child.wait().map_err(|e| GcloudError::Refused {
        detail: e.to_string(),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(GcloudError::Refused { detail: incomplete })
    }
}

fn read_lines(
    stream: impl std::io::Read + Send + 'static,
    lines: mpsc::Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stream).lines() {
            let Ok(line) = line else { return };
            if lines.send(line).is_err() {
                return;
            }
        }
    })
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, reason = "test helpers unwrap")]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;
    use std::os::unix::fs::PermissionsExt;

    /// A `gcloud` that is `body`, in a directory of its own.
    fn fake_gcloud(name: &str, body: &str) -> PathBuf {
        let dir = scratch_dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gcloud");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn args_of(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_token_is_asked_for_with_the_application_default_credentials_command() {
        let command = token_command(Path::new("/usr/bin/gcloud")).expect("an absolute path");
        assert_eq!(
            args_of(&command),
            ["auth", "application-default", "print-access-token"]
        );
        assert_eq!(
            args_of(&login_command(Path::new("/usr/bin/gcloud")).expect("an absolute path")),
            ["auth", "application-default", "login"]
        );
    }

    #[test]
    fn a_token_the_cli_prints_is_returned_without_its_newline() {
        let program = fake_gcloud("gcloud-prints-a-token", "echo placeholder-a-token");
        assert_eq!(
            token_from(&program).unwrap().expose(),
            "placeholder-a-token"
        );
    }

    #[test]
    fn the_program_is_given_the_application_default_credentials_arguments() {
        let program = fake_gcloud(
            "gcloud-checks-its-arguments",
            r#"[ "$*" = "auth application-default print-access-token" ] && echo right || echo wrong"#,
        );
        assert_eq!(token_from(&program).unwrap().expose(), "right");
    }

    #[test]
    fn a_refusal_reports_the_first_line_of_stderr_and_never_a_token_it_printed() {
        let program = fake_gcloud(
            "gcloud-refuses",
            "echo placeholder-leaked; echo 'Reauthentication failed.' >&2; echo more >&2; exit 1",
        );
        let failure = token_from(&program).unwrap_err();
        assert_eq!(
            failure,
            GcloudError::Refused {
                detail: "Reauthentication failed.".to_string()
            }
        );
        assert!(!failure.to_string().contains("placeholder-leaked"));
    }

    #[test]
    fn a_reply_that_is_not_one_token_is_refused_and_not_quoted() {
        for printed in ["", "   ", "two words"] {
            let mut bytes = printed.as_bytes().to_vec();
            let failure = decode(&mut bytes).unwrap_err();
            assert!(!failure.to_string().contains("words"), "{printed:?}");
            assert!(bytes.iter().all(|byte| *byte == 0), "{printed:?}");
        }
    }

    /// SANDBOX-30. A bare `gcloud` is never handed to the operating system's search of `PATH`: the
    /// start is refused and reported as not installed, as the empty path is.
    #[test]
    fn a_bare_name_is_not_started_and_is_reported_as_not_installed() {
        assert!(command_for(Path::new("gcloud"), TOKEN_ARGS).is_none());
        assert!(command_for(Path::new(""), TOKEN_ARGS).is_none());
        assert_eq!(
            token_from(Path::new("gcloud")).unwrap_err(),
            GcloudError::NotInstalled
        );
        assert_eq!(
            login_from(Path::new("gcloud"), |_| {}).unwrap_err(),
            GcloudError::NotInstalled
        );
        assert!(revoke_command(Path::new("gcloud")).is_none());
        assert_eq!(
            revoke_from(Path::new("gcloud")).unwrap_err(),
            GcloudError::NotInstalled
        );
    }

    #[test]
    fn no_program_is_reported_as_not_installed() {
        assert_eq!(
            token_from(Path::new("")).unwrap_err(),
            GcloudError::NotInstalled
        );
        assert_eq!(
            login_from(Path::new(""), |_| {}).unwrap_err(),
            GcloudError::NotInstalled
        );
    }

    #[test]
    fn a_gcloud_a_stage_may_write_is_not_the_one_run() {
        let session = scratch_dir("gcloud-planted-session");
        let planted = session.join(".venv").join("bin");
        std::fs::create_dir_all(&planted).unwrap();
        let program = planted.join("gcloud");
        std::fs::write(&program, "#!/bin/sh\necho planted\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        bravebot_sandbox::programs::keep_out(&session);
        let path = std::env::join_paths([&planted]).unwrap();

        assert!(installed_in(&path).as_os_str().is_empty());
    }

    /// The regression it rejects: starting with `SearchPath::Inherited`, which leaves `PATH` alone,
    /// so the names `gcloud`'s own wrapper looks up (its shell, its Python) are searched through
    /// directories a stage may write ([PATH-005](../../../docs/best-practices/paths.md#PATH-005)).
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_started_gcloud_is_given_a_path_the_lookup_has_reduced() {
        let program = Path::new("/usr/bin/gcloud");
        for command in [
            token_command(program),
            login_command(program),
            revoke_command(program),
        ] {
            let command = command.expect("an absolute path");
            let path = command
                .get_envs()
                .find(|(name, _)| *name == OsStr::new("PATH"));
            assert!(path.is_some(), "PATH is left as inherited");
        }
    }

    /// The token, login and revoke starts of `gcloud` when the person's environment is `held` and
    /// `writable` is the one directory a stage may write.
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn starts_against(held: &[(OsString, OsString)], writable: &Path) -> Vec<Command> {
        let row = writable.to_path_buf();
        let rows = std::slice::from_ref(&row);
        [TOKEN_ARGS, LOGIN_ARGS, REVOKE_ARGS]
            .into_iter()
            .map(|args| {
                command_for_with(
                    Path::new("/usr/bin/gcloud"),
                    args,
                    held.iter().cloned(),
                    &|file: &Path| {
                        bravebot_sandbox::programs::outside_stage_writable_with(file, rows, &[])
                    },
                )
                .expect("an absolute path")
            })
            .collect()
    }

    /// The answer of each of the token, login and revoke starts to `asks`, which must be the same
    /// for all three.
    fn alike(commands: &[Command], what: &str, asks: impl Fn(&Command) -> bool) -> bool {
        let answers: Vec<bool> = commands.iter().map(asks).collect();
        assert!(
            answers.iter().all(|answer| *answer == answers[0]),
            "{what}: the token, login and revoke starts differ: {answers:?}"
        );
        answers[0]
    }

    /// Whether `name` is taken off the environment a `gcloud` is started with, when the person's
    /// environment holds `value` for it and `writable` is the directory a stage may write. The
    /// token, login and revoke starts have to agree.
    fn withheld(name: &str, value: &Path, writable: &Path) -> bool {
        let held = [(OsString::from(name), value.as_os_str().to_os_string())];
        alike(&starts_against(&held, writable), name, |command| {
            command
                .get_envs()
                .any(|(set, now)| set == OsStr::new(name) && now.is_none())
        })
    }

    /// Whether a `gcloud` start has `PYTHONNOUSERSITE=1`, when the person's `HOME` is `home` and
    /// `writable` is the one directory a stage may write. The token, login and revoke starts have
    /// to agree.
    fn turns_off_the_user_site(home: &Path, writable: &Path) -> bool {
        let held = [(OsString::from("HOME"), home.as_os_str().to_os_string())];
        alike(
            &starts_against(&held, writable),
            "the user site",
            |command| {
                command.get_envs().any(|(name, value)| {
                    name == "PYTHONNOUSERSITE" && value == Some(OsStr::new("1"))
                })
            },
        )
    }

    /// SANDBOX-30. The launcher runs Python, which imports `usercustomize` from the user site under
    /// the home. The regression it rejects: never setting `PYTHONNOUSERSITE`, so a
    /// `usercustomize.py` a stage wrote under a `HOME` it may write runs unconfined with the person's
    /// access to `~/.config/gcloud`, and setting it always, which also hides the packages a person
    /// installed with `pip install --user`.
    #[test]
    fn the_gcloud_start_turns_off_the_user_site_only_where_a_stage_may_write_it() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-user-site");

        assert!(turns_off_the_user_site(&session, &session));
        assert!(!turns_off_the_user_site(&safe, &session));
    }

    /// The regression it rejects: judging `HOME` alone, so a `HOME` outside every stage directory
    /// passes while its `.local`, where the user base is read from, is a link into one.
    #[test]
    fn the_gcloud_start_turns_off_the_user_site_when_dot_local_leads_into_a_stage_directory() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-user-site-link");
        let home = scratch_dir("gcloud-user-site-link-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(&session, home.join(".local")).unwrap();

        assert!(turns_off_the_user_site(&home, &session));
        assert!(!turns_off_the_user_site(&safe, &session));
    }

    /// A file or directory a stage can plant, under a row it holds, and a genuine one beside it that
    /// is outside every row.
    ///
    /// Each test passes a `name` no other test uses: the directories are at fixed paths, each call
    /// removes them first, and tests run in parallel.
    fn planted_and_genuine(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let session = scratch_dir(&format!("{name}-session"));
        let safe = scratch_dir(&format!("{name}-safe"));
        for dir in [&session, &safe] {
            let _ = std::fs::remove_dir_all(dir);
        }
        let planted_bin = session.join(".venv").join("bin");
        let genuine_bin = safe.join("venv").join("bin");
        for dir in [&planted_bin, &genuine_bin] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let planted = planted_bin.join("python");
        let genuine = genuine_bin.join("python");
        for file in [&planted, &genuine] {
            std::fs::write(file, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (session, safe, planted, genuine)
    }

    /// The regression it rejects: starting `gcloud` with `CLOUDSDK_PYTHON` as the person set it, so
    /// the launcher executes an interpreter a stage wrote, outside confinement.
    #[test]
    fn an_interpreter_a_stage_may_write_is_not_the_one_gcloud_is_told_to_run() {
        let (session, _safe, planted, genuine) = planted_and_genuine("gcloud-python");
        let linked = scratch_dir("gcloud-python-linked");
        let _ = std::fs::remove_dir_all(&linked);
        std::fs::create_dir_all(&linked).unwrap();
        let link = linked.join("python");
        std::os::unix::fs::symlink(&planted, &link).unwrap();

        assert!(withheld("CLOUDSDK_PYTHON", &planted, &session));
        assert!(withheld("CLOUDSDK_PYTHON", &link, &session));
        assert!(!withheld("CLOUDSDK_PYTHON", &genuine, &session));
    }

    /// A bare interpreter name is looked up on the reduced `PATH`, so it is left alone. A relative
    /// path, and a path that names nothing yet, are read from or made in a directory a stage writes.
    #[test]
    fn an_interpreter_with_no_directory_is_kept_and_a_relative_or_missing_one_is_not() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-python-spellings");

        assert!(!withheld("CLOUDSDK_PYTHON", Path::new("python3"), &session));
        assert!(withheld(
            "CLOUDSDK_PYTHON",
            Path::new("./python3"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_PYTHON",
            Path::new("venv/bin/python"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_PYTHON",
            &safe.join("venv").join("bin").join("not-yet-written"),
            &session
        ));
    }

    /// A configuration directory is never a bare name: a relative one is read from the directory the
    /// session writes in, and one that does not exist is made by whoever writes it first.
    #[test]
    fn a_relative_or_missing_config_directory_is_not_the_one_gcloud_is_told_to_read() {
        let (session, safe, _planted, genuine) = planted_and_genuine("gcloud-config-spellings");

        assert!(withheld("CLOUDSDK_CONFIG", Path::new("gcloud"), &session));
        assert!(withheld("CLOUDSDK_CONFIG", Path::new("./gcloud"), &session));
        assert!(withheld(
            "CLOUDSDK_CONFIG",
            Path::new("../gcloud"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONFIG",
            &safe.join("not-made-yet"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONFIG",
            &session.join("not-made-yet"),
            &session
        ));
        assert!(!withheld("CLOUDSDK_CONFIG", &genuine, &session));
    }

    /// A value that is not text is judged as the bytes it is. A loop that skipped a value it cannot
    /// convert would keep a planted one, and one that rendered it lossily would judge the genuine
    /// file whose name holds `U+FFFD` instead ([PATH-003](../../../docs/best-practices/paths.md#PATH-003)).
    #[test]
    fn a_value_that_is_not_text_is_withheld_and_not_judged_by_its_lossy_rendering() {
        use std::os::unix::ffi::OsStrExt;

        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-not-text");
        let lookalike = safe.join("python-\u{FFFD}");
        std::fs::write(&lookalike, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&lookalike, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bytes = OsStr::from_bytes(b"python-\xff");

        assert!(withheld("CLOUDSDK_PYTHON", &session.join(bytes), &session));
        assert!(withheld(
            "CLOUDSDK_PYTHON",
            &session.join("python-\u{FFFD}"),
            &session
        ));
        assert!(withheld("CLOUDSDK_PYTHON", &safe.join(bytes), &session));
        assert!(!withheld("CLOUDSDK_PYTHON", &lookalike, &session));
        assert!(withheld("CLOUDSDK_CONFIG", &session.join(bytes), &session));
    }

    /// The regression it rejects: starting `gcloud` with `CLOUDSDK_CONFIG` as the person set it, so
    /// the launcher sources `virtenv/bin/activate` from a directory a stage wrote.
    #[test]
    fn a_config_directory_a_stage_may_write_is_not_the_one_gcloud_is_told_to_read() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-config");
        let planted = session.join(".venv");
        let genuine = safe.join("venv");
        let linked = scratch_dir("gcloud-config-linked");
        let _ = std::fs::remove_file(&linked);
        std::fs::create_dir_all(linked.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&planted, &linked).unwrap();

        assert!(withheld("CLOUDSDK_CONFIG", &planted, &session));
        assert!(withheld("CLOUDSDK_CONFIG", &linked, &session));
        assert!(!withheld("CLOUDSDK_CONFIG", &genuine, &session));
    }

    /// A list of directories joined with the platform's separator, as `PYTHONPATH` holds it.
    fn joined(entries: &[&Path]) -> PathBuf {
        PathBuf::from(std::env::join_paths(entries).unwrap())
    }

    /// `gcloud auth application-default login` opens a browser with Python's `webbrowser`, which
    /// runs each entry of `BROWSER`, and `xdg-open` reads the `XDG_` directories to choose the
    /// handler it runs. The regression it rejects: the gcloud start left without those names, so a
    /// value naming a file a stage wrote is run by the login.
    #[test]
    fn a_browser_or_xdg_directory_a_stage_may_write_is_not_the_one_the_gcloud_login_is_told_to_use()
    {
        let (session, _safe, planted, genuine) = planted_and_genuine("gcloud-browser");
        let planted_dir = planted.parent().unwrap();
        let genuine_dir = genuine.parent().unwrap();
        let with_s = |p: &Path| PathBuf::from(format!("{} %s", p.display()));

        assert!(withheld("BROWSER", &planted, &session));
        assert!(withheld("BROWSER", &with_s(&planted), &session));
        assert!(withheld(
            "BROWSER",
            &joined(&[&genuine, &planted]),
            &session
        ));
        assert!(withheld(
            "BROWSER",
            Path::new("firefox --new-window %s"),
            &session
        ));
        assert!(!withheld("BROWSER", Path::new("firefox"), &session));
        assert!(!withheld("BROWSER", &genuine, &session));
        assert!(!withheld("BROWSER", &with_s(&genuine), &session));

        assert!(withheld("XDG_DATA_HOME", planted_dir, &session));
        assert!(!withheld("XDG_DATA_HOME", genuine_dir, &session));
        assert!(withheld("XDG_CONFIG_HOME", planted_dir, &session));
        assert!(!withheld("XDG_CONFIG_HOME", genuine_dir, &session));
        let mixed = joined(&[genuine_dir, planted_dir]);
        assert!(withheld("XDG_DATA_DIRS", Path::new(&mixed), &session));
        assert!(!withheld("XDG_DATA_DIRS", genuine_dir, &session));
        assert!(withheld("XDG_CONFIG_DIRS", Path::new(&mixed), &session));
        assert!(!withheld("XDG_CONFIG_DIRS", genuine_dir, &session));
        // A list whose every entry is outside the directories a stage may write is kept. The
        // regression it rejects: a list judged as one path, which names no directory.
        let genuine_list = joined(&[genuine_dir, genuine_dir.parent().unwrap()]);
        assert!(!withheld("XDG_DATA_DIRS", &genuine_list, &session));
        assert!(!withheld("XDG_CONFIG_DIRS", &genuine_list, &session));
    }

    /// A `BROWSER` entry that is not text is judged as the bytes it is: a lossy rendering would
    /// read `fire\u{FFFD}ox` as a bare program name and keep it.
    #[test]
    fn a_gcloud_browser_entry_that_is_not_text_is_removed() {
        use std::os::unix::ffi::OsStrExt;

        let (session, _safe, _planted, _genuine) = planted_and_genuine("gcloud-browser-not-text");

        assert!(withheld(
            "BROWSER",
            Path::new(OsStr::from_bytes(b"fire\xffox")),
            &session
        ));
        assert!(!withheld("BROWSER", Path::new("fire\u{FFFD}ox"), &session));
    }

    /// The variables the login is run to read are left as the person set them.
    #[test]
    fn the_gcloud_start_keeps_the_display_a_login_reads() {
        let (session, _safe, planted, _genuine) = planted_and_genuine("gcloud-display");

        for name in ["DISPLAY", "WAYLAND_DISPLAY", "XDG_SESSION_TYPE"] {
            assert!(!withheld(name, &planted, &session), "{name} was removed");
        }
    }

    /// SANDBOX-30, `gio open` under `gcloud auth application-default login`: the session bus it
    /// connects to, judged where the socket is on the disk. The regressions it rejects: the start
    /// left without the two names (a start that keeps both whatever they hold), a name
    /// removed whatever it holds, which takes the person's own bus from the login, a bus address
    /// compared as written so a link under an installed directory to a socket a stage may write is
    /// kept, and `DBUS_SESSION_BUS_ADDRESS` judged as a path, which no address is.
    #[test]
    fn a_runtime_directory_or_bus_address_that_leads_to_a_socket_a_stage_may_write_is_not_the_one_the_gcloud_login_uses()
     {
        let (session, safe, planted, genuine) = planted_and_genuine("gcloud-bus");
        let (planted_dir, genuine_dir) = (planted.parent().unwrap(), genuine.parent().unwrap());
        let planted_bus = planted_dir.join("bus");
        let genuine_bus = genuine_dir.join("bus");
        let linked_bus = safe.join("linked-bus");
        std::fs::write(&planted_bus, "").unwrap();
        std::fs::write(&genuine_bus, "").unwrap();
        std::os::unix::fs::symlink(&planted_bus, &linked_bus).unwrap();
        let address = |path: &Path| PathBuf::from(format!("unix:path={}", path.display()));

        assert!(withheld("XDG_RUNTIME_DIR", planted_dir, &session));
        assert!(withheld("XDG_RUNTIME_DIR", Path::new("run"), &session));
        assert!(!withheld("XDG_RUNTIME_DIR", genuine_dir, &session));

        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &address(&planted_bus),
            &session
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &address(&linked_bus),
            &session
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &address(&safe.join("not-yet")),
            &session
        ));
        let mixed = format!(
            "unix:path={};unix:path={}",
            genuine_bus.display(),
            planted_bus.display()
        );
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            Path::new(&mixed),
            &session
        ));
        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            Path::new("unix:abstract=/tmp/dbus-x"),
            &session
        ));
        assert!(!withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            &address(&genuine_bus),
            &session
        ));
        assert!(!withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            Path::new("unix:runtime=yes"),
            &session
        ));
    }

    /// A runtime directory or bus address that is not text is judged as the bytes it is. The
    /// regression it rejects: a bus address with a byte that is not UTF-8 after a safe path read
    /// lossily as that path and kept, and a directory name judged by its lossy rendering.
    #[test]
    fn a_gcloud_runtime_directory_or_bus_address_that_is_not_text_is_judged_by_its_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let (session, safe, _planted, genuine) = planted_and_genuine("gcloud-bus-not-text");
        let genuine_bus = genuine.parent().unwrap().join("bus");
        std::fs::write(&genuine_bus, "").unwrap();
        let mut bytes = format!("unix:path={}", genuine_bus.display()).into_bytes();
        bytes.push(0xff);
        let lookalike = safe.join("run-\u{FFFD}");
        std::fs::create_dir_all(&lookalike).unwrap();

        assert!(withheld(
            "DBUS_SESSION_BUS_ADDRESS",
            Path::new(OsStr::from_bytes(&bytes)),
            &session
        ));
        assert!(withheld(
            "XDG_RUNTIME_DIR",
            &safe.join(OsStr::from_bytes(b"run-\xff")),
            &session
        ));
        assert!(!withheld("XDG_RUNTIME_DIR", &lookalike, &session));
    }

    /// The regression it rejects: starting `gcloud` with `PYTHONPATH` as the person set it, so the
    /// launcher's version probe imports a `sitecustomize.py` a stage wrote. A list is judged by every
    /// entry: one bad entry among good ones, in either position, an empty entry, which Python reads
    /// as the working directory, and a relative one all remove the whole variable.
    #[test]
    fn a_python_path_with_an_entry_a_stage_may_write_is_not_the_one_gcloud_imports_from() {
        let (session, safe, planted, genuine) = planted_and_genuine("gcloud-python-path");
        let (planted_dir, genuine_dir) = (planted.parent().unwrap(), genuine.parent().unwrap());
        let linked = scratch_dir("gcloud-python-path-linked");
        let _ = std::fs::remove_dir_all(&linked);
        std::fs::create_dir_all(&linked).unwrap();
        let link = linked.join("bin");
        std::os::unix::fs::symlink(planted_dir, &link).unwrap();

        assert!(withheld("PYTHONPATH", &joined(&[planted_dir]), &session));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[genuine_dir, planted_dir]),
            &session
        ));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[planted_dir, genuine_dir]),
            &session
        ));
        assert!(withheld("PYTHONPATH", &joined(&[&link]), &session));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[genuine_dir, Path::new("")]),
            &session
        ));
        assert!(withheld("PYTHONPATH", Path::new(""), &session));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[genuine_dir, Path::new("lib")]),
            &session
        ));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[Path::new("./lib")]),
            &session
        ));
        assert!(withheld(
            "PYTHONPATH",
            &joined(&[genuine_dir, &safe.join("not-made-yet")]),
            &session
        ));
    }

    /// A list whose every entry is an existing directory outside every row still reaches the CLI,
    /// so a loop that removes `PYTHONPATH` whatever it holds fails.
    #[test]
    fn a_python_path_outside_every_directory_a_stage_may_write_still_reaches_gcloud() {
        let (session, safe, _planted, genuine) = planted_and_genuine("gcloud-python-path-kept");
        let genuine_dir = genuine.parent().unwrap();

        assert!(!withheld("PYTHONPATH", &joined(&[genuine_dir]), &session));
        assert!(!withheld(
            "PYTHONPATH",
            &joined(&[genuine_dir, &safe]),
            &session
        ));
    }

    /// The regression it rejects: starting `gcloud` with `PYTHONHOME`, `PYTHONUSERBASE` or
    /// `PYTHONPYCACHEPREFIX` as the person set it, so the interpreter loads its standard library, a
    /// `usercustomize.py` or a `.pth` file, or an unchecked `.pyc` from a directory a stage wrote.
    #[test]
    fn a_python_directory_a_stage_may_write_is_not_the_one_gcloud_is_told_to_load_from() {
        let (session, safe, planted, _genuine) = planted_and_genuine("gcloud-python-dirs");
        let planted_dir = planted.parent().unwrap();

        assert!(withheld("PYTHONHOME", planted_dir, &session));
        assert!(withheld("PYTHONHOME", Path::new("python"), &session));
        assert!(withheld("PYTHONHOME", &safe.join("not-made-yet"), &session));
        assert!(withheld("PYTHONUSERBASE", planted_dir, &session));
        assert!(withheld("PYTHONUSERBASE", Path::new(".local"), &session));
        assert!(withheld(
            "PYTHONUSERBASE",
            &safe.join("not-made-yet"),
            &session
        ));
        assert!(withheld("PYTHONPYCACHEPREFIX", planted_dir, &session));
        assert!(withheld(
            "PYTHONPYCACHEPREFIX",
            Path::new("__pycache__"),
            &session
        ));
        assert!(withheld(
            "PYTHONPYCACHEPREFIX",
            &safe.join("not-made-yet"),
            &session
        ));
    }

    /// A directory outside every row still reaches the CLI as each of the three, so a loop that
    /// removes them whatever they hold fails.
    #[test]
    fn a_python_directory_outside_every_directory_a_stage_may_write_still_reaches_gcloud() {
        let (session, safe, _planted, _genuine) = planted_and_genuine("gcloud-python-dirs-kept");

        assert!(!withheld("PYTHONHOME", &safe, &session));
        assert!(!withheld("PYTHONUSERBASE", &safe, &session));
        assert!(!withheld("PYTHONPYCACHEPREFIX", &safe, &session));
    }

    /// The regression it rejects: starting `gcloud` with a credential or client certificate
    /// configuration file as the person set it. An `external_account` file at
    /// `GOOGLE_APPLICATION_CREDENTIALS` names a command to run and a file to read and post to a host
    /// it names, and the two `CLOUDSDK_CONTEXT_AWARE_*` files name a binary or a certificate
    /// provider command. A link into a row, a directory and a path that does not exist are removed
    /// too, since each is a file a stage can make or swap.
    #[test]
    fn a_credential_or_certificate_file_a_stage_may_write_is_not_the_one_gcloud_is_told_to_read() {
        let (session, safe, planted, _genuine) = planted_and_genuine("gcloud-files");
        let linked = scratch_dir("gcloud-files-linked");
        let _ = std::fs::remove_dir_all(&linked);
        std::fs::create_dir_all(&linked).unwrap();
        let link = linked.join("file");
        std::os::unix::fs::symlink(&planted, &link).unwrap();
        let missing = safe.join("not-made-yet");
        let directory = planted.parent().unwrap();

        assert!(withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            &planted,
            &session
        ));
        assert!(withheld("GOOGLE_APPLICATION_CREDENTIALS", &link, &session));
        assert!(withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            Path::new("adc.json"),
            &session
        ));
        assert!(withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            &missing,
            &session
        ));
        assert!(withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            directory,
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH",
            &planted,
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH",
            &link,
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH",
            Path::new("cert.json"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH",
            &planted,
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH",
            &link,
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH",
            &missing,
            &session
        ));
    }

    /// A file outside every row still reaches the CLI, so the person's own credential file and the
    /// configuration the CLI is run to read are not lost, and a loop that removes these whatever
    /// they hold fails.
    #[test]
    fn a_credential_or_certificate_file_outside_every_directory_a_stage_may_write_still_reaches_gcloud()
     {
        let (session, _safe, _planted, genuine) = planted_and_genuine("gcloud-files-kept");

        assert!(!withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            &genuine,
            &session
        ));
        assert!(!withheld(
            "CLOUDSDK_CONTEXT_AWARE_CERTIFICATE_CONFIG_FILE_PATH",
            &genuine,
            &session
        ));
        assert!(!withheld(
            "CLOUDSDK_CONTEXT_AWARE_AUTO_DISCOVERY_FILE_PATH",
            &genuine,
            &session
        ));
    }

    /// The regression it rejects: starting `gcloud` with `OPENSSL_CONF` as the person set it, so the
    /// OpenSSL Python loads takes a provider module a stage wrote from the file's `module =` line.
    #[test]
    fn an_openssl_configuration_a_stage_may_write_is_not_the_one_gcloud_is_told_to_read() {
        let (session, safe, planted, _genuine) = planted_and_genuine("gcloud-openssl");

        assert!(withheld("OPENSSL_CONF", &planted, &session));
        assert!(withheld("OPENSSL_CONF", Path::new("openssl.cnf"), &session));
        assert!(withheld("OPENSSL_CONF", &safe.join("not-yet"), &session));
    }

    /// A configuration file outside every row still reaches the CLI.
    #[test]
    fn an_openssl_configuration_outside_every_directory_a_stage_may_write_still_reaches_gcloud() {
        let (session, _safe, _planted, genuine) = planted_and_genuine("gcloud-openssl-kept");

        assert!(!withheld("OPENSSL_CONF", &genuine, &session));
    }

    /// The regression it rejects: starting `gcloud` with `BROWSER` as the person set it, so
    /// `gcloud auth login` opens a page with a program a stage wrote.
    #[test]
    fn a_browser_a_stage_may_write_is_not_the_one_the_sign_in_opens() {
        let (session, _safe, planted, _genuine) = planted_and_genuine("gcloud-sign-in-browser");

        assert!(withheld("BROWSER", &planted, &session));
        assert!(withheld("BROWSER", Path::new("./opener"), &session));
        assert!(withheld("BROWSER", Path::new("bin/opener"), &session));
    }

    /// A bare program name, which the reduced `PATH` resolves, and an absolute path outside every
    /// row still reach the CLI.
    #[test]
    fn a_browser_outside_every_directory_a_stage_may_write_still_reaches_gcloud() {
        let (session, _safe, _planted, genuine) = planted_and_genuine("gcloud-browser-kept");

        assert!(!withheld("BROWSER", Path::new("xdg-open"), &session));
        assert!(!withheld("BROWSER", &genuine, &session));
    }

    /// The regression it rejects: starting `gcloud` with `CLOUDSDK_PYTHON_ARGS` as the person set it,
    /// so `-c <code>`, `-m <module>` or a script path is placed before `gcloud.py` and runs. The
    /// value holds options and no path, so it is removed whatever it holds, a path outside every row
    /// and an empty value included. A loop that applied a path check to it would keep a value that
    /// happens to be an existing path, and one that skipped an unreadable value would keep a
    /// planted one.
    #[test]
    fn python_options_are_not_passed_to_gcloud_whatever_they_hold() {
        let (session, _safe, planted, genuine) = planted_and_genuine("gcloud-python-args");

        assert!(withheld(
            "CLOUDSDK_PYTHON_ARGS",
            Path::new("-c print(1)"),
            &session
        ));
        assert!(withheld(
            "CLOUDSDK_PYTHON_ARGS",
            Path::new("-m planted"),
            &session
        ));
        assert!(withheld("CLOUDSDK_PYTHON_ARGS", &planted, &session));
        assert!(withheld("CLOUDSDK_PYTHON_ARGS", &genuine, &session));
        assert!(withheld("CLOUDSDK_PYTHON_ARGS", Path::new(""), &session));
    }

    /// A variable of the CLI's that selects no code is left as the person set it, whatever it names.
    #[test]
    fn a_cloudsdk_value_that_selects_no_code_is_left_as_set() {
        let (session, _safe, planted, _genuine) = planted_and_genuine("gcloud-other-values");

        assert!(!withheld("CLOUDSDK_CORE_PROJECT", &planted, &session));
        assert!(!withheld("GOOGLE_CLOUD_PROJECT", &planted, &session));
        assert!(!withheld("PYTHONUNBUFFERED", &planted, &session));
    }

    /// The regression it rejects: a gcloud start that passes `Kept`, or `Checked` with its own list
    /// replacing the generic table, so a generic name (`LD_PRELOAD`, a `GIT_CONFIG_` name) reaches
    /// the CLI as the person set it.
    #[test]
    fn a_started_gcloud_has_the_generic_code_selectors_removed_as_well() {
        let (session, safe, planted, genuine) = planted_and_genuine("gcloud-generic");

        assert!(withheld("LD_PRELOAD", &planted, &session));
        assert!(withheld("NODE_OPTIONS", &genuine, &session));
        assert!(withheld("GIT_CONFIG_GLOBAL", &safe, &session));
        assert!(!withheld("LD_PRELOAD", &genuine, &session));
    }

    /// The regression it rejects: starting without the credential removal, so the agent's own keys
    /// reach the CLI ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)), for the
    /// revoke as for the token and the login.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_started_gcloud_has_this_agents_credentials_removed() {
        let names = bravebot_config::scrub::own_credentials();
        for command in [
            token_command(Path::new("/usr/bin/gcloud")),
            login_command(Path::new("/usr/bin/gcloud")),
            revoke_command(Path::new("/usr/bin/gcloud")),
        ] {
            let command = command.expect("an absolute path");
            for name in &names {
                assert!(
                    command
                        .get_envs()
                        .any(|(set, value)| set == OsStr::new(name) && value.is_none()),
                    "{name} is not removed"
                );
            }
        }
    }

    #[test]
    fn the_sign_in_reports_each_line_the_cli_writes_and_its_failure() {
        let program = fake_gcloud(
            "gcloud-logs-in",
            "echo 'Go to the following link'; echo 'https://accounts.example.invalid/' >&2; exit 0",
        );
        let mut said = Vec::new();
        login_from(&program, |line| said.push(line)).unwrap();
        said.sort();
        assert_eq!(
            said,
            [
                "Go to the following link",
                "https://accounts.example.invalid/"
            ]
        );

        let failing = fake_gcloud("gcloud-login-fails", "echo 'no browser' >&2; exit 1");
        assert!(login_from(&failing, |_| {}).is_err());
    }

    /// A `gcloud` that gives a token once its login has run, as the real one does after a browser
    /// sign-in, and records each start in a log beside it.
    fn gcloud_that_signs_in(name: &str, signed_already: bool, login_gives_token: bool) -> PathBuf {
        let dir = scratch_dir(name);
        let marker = dir.join("signed");
        let log = dir.join("log");
        let after_login = if login_gives_token {
            format!(": > '{}'", marker.display())
        } else {
            "echo 'cancelled' >&2; exit 1".to_string()
        };
        let program = fake_gcloud(
            name,
            &format!(
                r#"echo "$*" >> '{log}'
case "$3" in
  print-access-token) [ -e '{marker}' ] && echo placeholder-a-token || exit 1 ;;
  login) echo 'Go to the following link'; {after_login} ;;
esac"#,
                log = log.display(),
                marker = marker.display(),
            ),
        );
        if signed_already {
            std::fs::write(&marker, "").unwrap();
        }
        program
    }

    fn starts_of(program: &Path) -> Vec<String> {
        std::fs::read_to_string(program.with_file_name("log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The regression it rejects: a revoke that is started with arguments other than the fixed ones
    /// (a credential file, or the older `auth revoke` that ends every account the CLI holds), one that
    /// starts the CLI when none is installed, and one that reports success for a CLI that failed.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn the_revoke_is_the_application_default_credentials_command_and_reports_only_its_exit_status()
    {
        assert_eq!(
            args_of(&revoke_command(Path::new("/usr/bin/gcloud")).expect("an absolute path")),
            ["auth", "application-default", "revoke", "--quiet"]
        );
        let revoking = fake_gcloud(
            "gcloud-revokes",
            r#"echo "$*" > "$(dirname "$0")/log"; echo placeholder-revoke-line"#,
        );
        revoke_from(&revoking).unwrap();
        assert_eq!(
            starts_of(&revoking),
            ["auth application-default revoke --quiet"]
        );
        let failing = fake_gcloud(
            "gcloud-revoke-fails",
            "echo placeholder-secret-line >&2; exit 1",
        );
        let failure = revoke_from(&failing).unwrap_err();
        assert!(
            !failure.to_string().contains("placeholder-secret-line"),
            "{failure}"
        );
        assert_eq!(
            revoke_from(Path::new("")).unwrap_err(),
            GcloudError::NotInstalled
        );
    }

    /// The regression it rejects: a sign-in a person asked for that opens the browser although the CLI
    /// gives a token already, which is what the sign-in a turn makes avoids, and one that reports
    /// success because the login ended well although the CLI still gives no token.
    #[test]
    fn a_sign_in_asked_for_opens_no_browser_for_a_token_already_given_and_checks_one_after_the_login()
     {
        let had = gcloud_that_signs_in("gcloud-checked-already", true, true);
        let mut said = Vec::new();
        assert_eq!(
            sign_in_checked_from(&had, |line| said.push(line)).unwrap(),
            Signed::Already
        );
        assert!(said.is_empty(), "{said:?}");
        assert_eq!(
            starts_of(&had),
            ["auth application-default print-access-token"],
            "the login was started for a token already given"
        );

        let none = gcloud_that_signs_in("gcloud-checked-now", false, true);
        let mut said = Vec::new();
        assert_eq!(
            sign_in_checked_from(&none, |line| said.push(line)).unwrap(),
            Signed::Now
        );
        assert_eq!(said, ["Go to the following link"]);
        assert_eq!(
            starts_of(&none),
            [
                "auth application-default print-access-token",
                "auth application-default login",
                "auth application-default print-access-token"
            ]
        );

        let cancelled = gcloud_that_signs_in("gcloud-checked-cancelled", false, false);
        assert!(sign_in_checked_from(&cancelled, |_| {}).is_err());

        // A login that ends well and leaves no token is a failure too.
        let hollow = fake_gcloud(
            "gcloud-checked-hollow",
            r#"case "$3" in print-access-token) exit 1 ;; login) exit 0 ;; esac"#,
        );
        assert!(matches!(
            sign_in_checked_from(&hollow, |_| {}),
            Err(GcloudError::Refused { .. })
        ));
        assert_eq!(
            sign_in_checked_from(Path::new(""), |_| {}).unwrap_err(),
            GcloudError::NotInstalled
        );
    }
}
