//! What running the built binary does: the status a failure exits with, and the one command an
//! incognito session refuses.
//!
//! [CLI-6] and [INCOG-7] are the clauses, and both are properties of a process rather than of a
//! function. `main` returns an `ExitCode` that nothing in the same process can read back, and
//! asking for a session that leaves nothing behind is a one-way door for the life of a process,
//! so a test that engaged it would make every other test in its binary incognito too. Running the
//! binary answers both.
//!
//! [CLI-6]: ../../../docs/specs/cli.md
//! [INCOG-7]: ../../../docs/specs/incognito.md

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A directory handed to a run as its own, removed when the test that made it ends.
///
/// Under this crate's build directory rather than the system temporary one, which is shared
/// between users and where a name this predictable is somebody else's to create first.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }

    /// Where an imported subscription is kept under this home.
    fn credentials(&self) -> PathBuf {
        self.path.join(".bravebot").join("leo-premium.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Run the built binary, in an environment this test wrote rather than the one it inherited.
///
/// Nothing is inherited, because every configuration value is read from the environment first: a
/// developer's own exports would otherwise decide whether the configuration a run is handed here
/// is the broken one it was given. What goes back in is the home directory, pointed at a scratch
/// so no run reads or writes the real one, and the locale, so the words on stderr are the ones
/// asserted on. The variable is `bravebot_i18n::LOCALE`, named here as the string a person would
/// export, since a binary is being run rather than a crate called.
fn bravebot(home: &Path, environment: &[(&str, &str)], arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bravebot"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .envs(environment.iter().copied())
        .args(arguments)
        // Not a terminal, and carrying nothing: a run that reads a pipe reads the end of the
        // input rather than waiting on whatever started the tests.
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs")
}

/// What a run said, as the two streams it says it on.
fn said(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A configuration this build cannot use stops the run, and the status is what says so. A script
/// reads the reply off stdout and has nothing else to go on, so a failure explained there and
/// exited successfully from is one it cannot see at all.
#[test]
fn a_configuration_error_exits_non_zero() {
    let scratch = Scratch::new("cli-running-configuration");
    let output = bravebot(
        &scratch.path,
        // Complete but for the endpoint, which names no scheme. The other two are set rather
        // than left out so that the problem is this one whether or not the binary under test was
        // built somewhere with configuration to bake in.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid"),
        ],
        &["-p", "say something"],
    );

    let (stdout, stderr) = said(&output);
    assert!(
        !output.status.success(),
        "a configuration the run cannot use exited successfully: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "the reply stream carried the explanation instead: {stdout}"
    );
    assert!(
        // The value rather than the wording, so the run is known to have failed over the
        // endpoint it was handed and not over something else the environment lacks.
        stderr.contains("ai-chat.example.invalid"),
        "the run failed over something other than the endpoint it was given: {stderr}"
    );
}

/// An argument the program refuses ends the run the same way, whichever way it is refused: an
/// option the program does not have, a flag given none of what it needs, and an option a
/// subcommand parsing its own does not have.
///
/// The status and the complaint are what every one of them has in common. The first also prints
/// the usage, which goes to stdout, so the reply stream is asserted on where the failure is the
/// whole of what the run produced rather than here.
#[test]
fn a_refused_argument_exits_non_zero() {
    let scratch = Scratch::new("cli-running-argument");

    for (arguments, refused) in [
        (&["--not-an-option"][..], "--not-an-option"),
        (&["--fork"][..], "--fork"),
        (
            &["import-leo-creds", "--not-an-option"][..],
            "--not-an-option",
        ),
    ] {
        let output = bravebot(&scratch.path, &[], arguments);

        let (_, stderr) = said(&output);
        assert!(
            !output.status.success(),
            "{arguments:?} was refused and exited successfully: {stderr}"
        );
        assert!(
            stderr.contains(refused),
            "{arguments:?} failed without saying that {refused} is what it refused: {stderr}"
        );
    }
}

/// The third failure the clause names, and the one the status matters most for: a turn that could
/// not run produces no reply, so a script reading stdout sees an empty answer rather than an
/// error.
#[test]
fn a_turn_that_could_not_run_exits_non_zero() {
    let scratch = Scratch::new("cli-running-turn");
    let output = bravebot(
        &scratch.path,
        // A configuration with nothing wrong with it, naming a port nothing can be listening on:
        // binding port 1 takes privileges the machine running tests does not give away, so the
        // connection is refused at once rather than timing out or reaching a real service.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["-p", "say something"],
    );

    let (stdout, stderr) = said(&output);
    assert!(
        !output.status.success(),
        "a turn that never reached a backend exited successfully: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "the reply stream carried the explanation instead: {stdout}"
    );
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the run failed over something other than the backend it could not reach: {stderr}"
    );
}

/// An import is a write by definition, so an incognito session refuses it rather than doing it
/// and discarding the result: that would mint a batch on Brave's service that nothing could ever
/// spend. Refused before the device is registered, which is what the empty reply stream says:
/// the search for an order prints as it goes.
#[test]
fn an_import_is_refused_in_an_incognito_session() {
    let scratch = Scratch::new("cli-running-incognito-import");
    let output = bravebot(
        &scratch.path,
        &[],
        &["--incognito", "import-leo-creds", "stable"],
    );

    let (stdout, stderr) = said(&output);
    assert!(
        !output.status.success(),
        "an import in an incognito session exited successfully: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "the import started work before refusing: {stdout}"
    );
    assert!(
        stderr.contains("incognito"),
        "the run stopped without saying the mode is why: {stderr}"
    );
    assert!(
        !scratch.credentials().exists(),
        "an incognito session left credentials behind"
    );
}

/// Forgetting is permitted where importing is not: removing a stored secret leaves less behind
/// rather than more, which is the direction the mode points. A mode that refused every command
/// with a subscription in its name would take the private session away from anyone who wanted to
/// stop spending one.
#[test]
fn forgetting_an_import_is_allowed_in_an_incognito_session() {
    let scratch = Scratch::new("cli-running-incognito-forget");
    let stored = scratch.credentials();
    std::fs::create_dir_all(stored.parent().expect("the state directory"))
        .expect("create the state directory");
    std::fs::write(&stored, "{}").expect("write credentials to forget");

    let output = bravebot(
        &scratch.path,
        &[],
        &["--incognito", "import-leo-creds", "--forget"],
    );

    let (_, stderr) = said(&output);
    assert!(
        output.status.success(),
        "forgetting an import was refused: {stderr}"
    );
    assert!(
        !stored.exists(),
        "the credentials are still at {}",
        stored.display()
    );
}
