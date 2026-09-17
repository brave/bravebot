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
        (&["--settings"][..], "--settings"),
        // A run told to configure itself from a file that is not there is refused rather than run
        // under whatever the directory carried. The path is what it says, since a mistyped one is
        // the mistake and the flag's own name would not point at it.
        (
            &["--settings", "/no/such/settings.json", "doctor"][..],
            "/no/such/settings.json",
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

/// The whole of the reported defect: a configuration that cannot be used, an argument the program
/// does not have, and a backend nothing is listening on all exited 1, so a caller could not tell
/// "fix the config" from "try again in a minute" without reading English.
///
/// The statuses are asserted as the numbers a script writes, not as "not zero": the number is what
/// this program promised, and a test that only checked for a failure would pass again the day they
/// all collapsed back into one.
#[test]
fn each_kind_of_failure_has_a_status_of_its_own() {
    let scratch = Scratch::new("cli-running-status-per-failure");
    let usable = [
        ("SERVICES_KEY_AICHAT", "a-services-key"),
        ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
        ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
    ];

    let argument = bravebot(&scratch.path, &usable, &["--not-an-option"]);
    assert_eq!(argument.status.code(), Some(2), "{:?}", said(&argument));

    let configuration = bravebot(
        &scratch.path,
        // Complete but for the endpoint, which names no scheme.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid"),
        ],
        &["-p", "say something"],
    );
    assert_eq!(
        configuration.status.code(),
        Some(3),
        "{:?}",
        said(&configuration)
    );

    // Port 1 takes privileges the machine running tests does not give away, so the connection is
    // refused at once rather than timing out or reaching a real service.
    let unreachable = bravebot(&scratch.path, &usable, &["-p", "say something"]);
    assert_eq!(
        unreachable.status.code(),
        Some(5),
        "{:?}",
        said(&unreachable)
    );
}

/// A message is in the reader's own language, so a bug report carries a sentence nobody receiving
/// it can search for. The identifier is the same failure said in a form that does not change.
#[test]
fn a_failure_says_a_stable_identifier_whatever_language_it_explains_itself_in() {
    let scratch = Scratch::new("cli-running-identifier");
    // Complete but for the endpoint, which names no scheme.
    let broken = [
        ("SERVICES_KEY_AICHAT", "a-services-key"),
        ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
        ("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid"),
    ];
    let mut in_french = broken.to_vec();
    // Applied after the locale the helper sets, so this is the one in force.
    in_french.push(("BRAVEBOT_LOCALE", "fr"));

    let (_, english) = said(&bravebot(&scratch.path, &broken, &["-p", "say something"]));
    let (_, french) = said(&bravebot(
        &scratch.path,
        &in_french,
        &["-p", "say something"],
    ));

    assert!(
        english.contains("configuration error"),
        "the run failed over something else: {english}"
    );
    assert!(
        french.contains("erreur de configuration"),
        "the message was not in the reader's language: {french}"
    );
    assert!(
        english.contains("BB1003") && french.contains("BB1003"),
        "the identifier changed with the language: {english} / {french}"
    );
}

/// The point of the flag: one object on stdout, in the reply's place, holding what a caller would
/// otherwise have had to read out of English on stderr. A failure before the turn is a result too,
/// since a caller that had to tell an empty stdout from a result has the prose surface back.
#[test]
fn a_run_asked_for_a_result_object_puts_one_on_stdout() {
    let scratch = Scratch::new("cli-running-json");
    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid"),
        ],
        &["--json", "-p", "say something"],
    );

    let (stdout, stderr) = said(&output);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "the result is not one object on one line: {stdout}"
    );
    for field in [
        r#""ok":false"#,
        r#""status":3"#,
        r#""reason":"configuration""#,
        r#""identifier":"BB1003""#,
    ] {
        assert!(stdout.contains(field), "{field} is missing from {stdout}");
    }
    // The prose is still on stderr, where a person reads it.
    assert!(
        stderr.contains("ai-chat.example.invalid"),
        "the explanation went nowhere: {stderr}"
    );
}

/// The flag reaches the layer a process reads, which is the half of it no in-process test can
/// answer: `Settings::load` is called from the interface, from a one-shot run and from the list a
/// subprocess is built with, and what carries the named file to all three is process-wide state the
/// entry point sets. `doctor` reports the layers that were read, so running the binary says whether
/// the file the command line named was one of them and whether it is the file that won a name.
///
/// The file is written outside the home this run is given and outside the directory it starts in,
/// so neither of the layers that are found could have supplied it.
#[test]
fn a_settings_file_named_on_the_command_line_is_read_above_the_ones_found() {
    let scratch = Scratch::new("cli-running-named-settings");
    let mine = scratch.path.join(".bravebot");
    std::fs::create_dir_all(&mine).expect("create the state directory");
    std::fs::write(
        mine.join("settings.json"),
        r#"{"env": {"AWS_PROFILE": "personal", "AWS_REGION": "us-west-2"}}"#,
    )
    .expect("write the home layer");
    let named = scratch.path.join("ci.json");
    std::fs::write(&named, r#"{"env": {"AWS_PROFILE": "the-ci-account"}}"#)
        .expect("write the named layer");

    let output = bravebot(
        &scratch.path,
        // A configuration with nothing wrong with it, so `doctor` reports the layers rather than
        // stopping at the configuration. Set rather than left out because CI builds with no
        // credentials baked in, where a run that read them off the build would find none.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["--settings", &named.display().to_string(), "doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(output.status.success(), "doctor did not run: {stderr}");
    assert!(
        stdout.contains(&named.display().to_string()),
        "the file the command line named was not read: {stdout}"
    );
    // The name it won, against the file that won it. Values are never reported, so this is the
    // whole of what says the named file outranked the one in the home directory.
    assert!(
        stdout.contains(&format!("AWS_PROFILE from {}", named.display())),
        "the named file did not outrank the layer that was found: {stdout}"
    );
    assert!(
        stdout.contains("AWS_PROFILE, AWS_REGION"),
        "a fourth layer replaced the one below it instead of overriding a name: {stdout}"
    );
}

/// A failure before the turn is a result object too, and this one happens before the arguments have
/// been parsed as an invocation. Whether one was asked for is therefore read off the command line as
/// typed: the flag takes the token after it as its path, whatever that token is, so a caller who
/// forgot the path would have had the refusal on stderr and an empty stdout.
#[test]
fn a_refused_settings_file_still_answers_with_a_result_object() {
    let scratch = Scratch::new("cli-running-named-settings-json");
    let output = bravebot(
        &scratch.path,
        &[],
        &["--settings", "--json", "-p", "say something"],
    );

    let (stdout, stderr) = said(&output);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "the result is not one object on one line: {stdout}"
    );
    for field in [r#""ok":false"#, r#""status":2"#, r#""reason":"argument""#] {
        assert!(stdout.contains(field), "{field} is missing from {stdout}");
    }
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
