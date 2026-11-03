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
//! Two of the variables the CLI reads name code it then runs: `CLOUDSDK_PYTHON` is the interpreter
//! its launcher executes, and the launcher sources `$CLOUDSDK_CONFIG/virtenv/bin/activate` when it
//! has no interpreter of its own. [`CODE_SELECTORS`] lists them, and one whose value is a path a
//! stage may write is taken off the environment the CLI is started with.
//!
//! The token is a credential. [`Secret`] keeps it out of a `Debug` render, the bytes the CLI
//! printed are cleared once read, and the only use made of it is the `authorization` header of a
//! request to the host built from the project and location.
//!
//! `gcloud auth application-default print-access-token` states no expiry, so no answer is kept: a
//! token is asked for with each request and a check before a turn is one more run of the program.

use bravebot_config::Secret;
use bravebot_i18n::t;
use std::ffi::{OsStr, OsString};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// The program asked for a token, looked for on `PATH` as any command a person runs is.
const GCLOUD: &str = "gcloud";

/// A variable the CLI reads to choose code to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Names {
    /// A program: a bare name is looked up on the reduced `PATH`, and anything with a separator is
    /// a file.
    Program,
    /// A directory the CLI may source a script from.
    Directory,
}

/// The variables the CLI reads that name code it runs, and what each names.
///
/// A value is kept only when it is outside every directory a stage may write
/// ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)). A variable that selects code and
/// is not listed here reaches the CLI as the person set it, so one added to the CLI's launcher
/// belongs in this list.
const CODE_SELECTORS: [(&str, Names); 2] = [
    ("CLOUDSDK_PYTHON", Names::Program),
    ("CLOUDSDK_CONFIG", Names::Directory),
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
/// ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)) and each of
/// [`CODE_SELECTORS`] that names a path a stage may write. The rest of the environment stays, since
/// `GOOGLE_APPLICATION_CREDENTIALS` and the other `CLOUDSDK_*` values are what the CLI is run to
/// read.
fn command_for(program: &Path) -> Command {
    command_for_with(
        program,
        |name| std::env::var_os(name),
        bravebot_sandbox::programs::outside_stage_writable,
    )
}

/// [`command_for`] against a given environment and a given judgment of which files are outside the
/// directories a stage may write.
fn command_for_with(
    program: &Path,
    held: impl Fn(&str) -> Option<OsString>,
    outside_stage_writable: impl Fn(&Path) -> bool,
) -> Command {
    let mut command = bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::found_earlier(program),
        bravebot_sandbox::programs::Inherits::Without(&bravebot_config::scrub::own_credentials()),
        bravebot_sandbox::programs::SearchPath::Reduced,
    );
    for (name, names) in CODE_SELECTORS {
        if let Some(value) = held(name)
            && !may_be_run(names, Path::new(&value), &outside_stage_writable)
        {
            command.env_remove(name);
        }
    }
    command
}

/// Whether the CLI may be left to run what `value` names.
///
/// A program given by a bare name is looked up on the `PATH` the CLI is started with, which leaves
/// out the directories a stage may write. Any other value must be an absolute path to something
/// that exists outside them, with its links followed. A relative path is read from the directory the
/// session writes in, and one that does not exist is made by whoever writes it first.
fn may_be_run(names: Names, value: &Path, outside_stage_writable: &impl Fn(&Path) -> bool) -> bool {
    if names == Names::Program && value.components().count() == 1 && !value.is_absolute() {
        return true;
    }
    value.is_absolute() && outside_stage_writable(value)
}

fn token_command(program: &Path) -> Command {
    let mut command = command_for(program);
    command.args(["auth", "application-default", "print-access-token"]);
    command
}

fn login_command(program: &Path) -> Command {
    let mut command = command_for(program);
    command.args(["auth", "application-default", "login"]);
    command
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
    let mut output = start_piped(&mut token_command(program))
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

fn login_from(program: &Path, mut say: impl FnMut(String)) -> Result<(), GcloudError> {
    if program.as_os_str().is_empty() {
        return Err(GcloudError::NotInstalled);
    }
    let mut child = start_piped(&mut login_command(program)).map_err(|e| spawn_error(&e))?;
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
        Err(GcloudError::Refused {
            detail: t!(gcloud_login_incomplete).to_string(),
        })
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
    fn the_token_is_asked_for_with_the_application_default_credentials_command() {
        let command = token_command(Path::new("/usr/bin/gcloud"));
        assert_eq!(
            args_of(&command),
            ["auth", "application-default", "print-access-token"]
        );
        assert_eq!(
            args_of(&login_command(Path::new("/usr/bin/gcloud"))),
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
    fn a_started_gcloud_is_given_a_path_the_lookup_has_reduced() {
        let program = Path::new("/usr/bin/gcloud");
        for command in [token_command(program), login_command(program)] {
            let path = command
                .get_envs()
                .find(|(name, _)| *name == OsStr::new("PATH"));
            assert!(path.is_some(), "PATH is left as inherited");
        }
    }

    /// Whether `name` is taken off the environment a `gcloud` is started with, when the person's
    /// environment holds `value` for it and `writable` is the directory a stage may write.
    fn withheld(name: &str, value: &Path, writable: &Path) -> bool {
        let row = writable.to_path_buf();
        let rows = std::slice::from_ref(&row);
        let command = command_for_with(
            Path::new("/usr/bin/gcloud"),
            |asked| (asked == name).then(|| value.as_os_str().to_os_string()),
            |file| bravebot_sandbox::programs::outside_stage_writable_with(file, rows, &[]),
        );
        command
            .get_envs()
            .any(|(set, now)| set == OsStr::new(name) && now.is_none())
    }

    /// A file or directory a stage can plant, under a row it holds, and a genuine one beside it that
    /// is outside every row.
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

    /// A variable of the CLI's that selects no code is left as the person set it, whatever it names.
    #[test]
    fn a_cloudsdk_value_that_selects_no_code_is_left_as_set() {
        let (session, _safe, planted, _genuine) = planted_and_genuine("gcloud-other-values");

        assert!(!withheld("CLOUDSDK_CORE_PROJECT", &planted, &session));
        assert!(!withheld(
            "GOOGLE_APPLICATION_CREDENTIALS",
            &planted,
            &session
        ));
    }

    /// The regression it rejects: starting without the credential removal, so the agent's own keys
    /// reach the CLI ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)).
    #[test]
    fn a_started_gcloud_has_this_agents_credentials_removed() {
        let names = bravebot_config::scrub::own_credentials();
        let command = command_for(Path::new("/usr/bin/gcloud"));
        for name in &names {
            assert!(
                command
                    .get_envs()
                    .any(|(set, value)| set == OsStr::new(name) && value.is_none()),
                "{name} is not removed"
            );
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
}
