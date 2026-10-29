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
//! The token is a credential. [`Secret`] keeps it out of a `Debug` render, the bytes the CLI
//! printed are cleared once read, and the only use made of it is the `authorization` header of a
//! request to the host built from the project and location.
//!
//! `gcloud auth application-default print-access-token` states no expiry, so no answer is kept: a
//! token is asked for with each request and a check before a turn is one more run of the program.

use bravebot_config::Secret;
use bravebot_i18n::t;
use std::ffi::OsStr;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// The program asked for a token, looked for on `PATH` as any command a person runs is.
const GCLOUD: &str = "gcloud";

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
/// ([CRED-14](../../../docs/specs/credential-protection.md#CRED-14)). The rest of the environment
/// stays, since `GOOGLE_APPLICATION_CREDENTIALS` and `CLOUDSDK_*` are what the CLI is run to read.
fn command_for(program: &Path) -> Command {
    bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::found_earlier(program),
        bravebot_sandbox::programs::Inherits::Without(&bravebot_config::scrub::own_credentials()),
        bravebot_sandbox::programs::SearchPath::Reduced,
    )
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
