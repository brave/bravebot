//! What running the built binary does: the status a failure exits with, the one command an
//! incognito session refuses, and what `doctor` says about the settings file a process found.
//!
//! [CLI-6] and [INCOG-7] are the clauses, and both are properties of a process rather than of a
//! function. `main` returns an `ExitCode` that nothing in the same process can read back, and
//! asking for a session that leaves nothing behind is a one-way door for the life of a process,
//! so a test that engaged it would make every other test in its binary incognito too. Running the
//! binary answers both. [PERM-11] is here for a different reason: the report it requires is made
//! out of two crates and printed by a third, and a process is what puts the three together.
//!
//! [CLI-6]: ../../../docs/specs/cli.md
//! [INCOG-7]: ../../../docs/specs/incognito.md
//! [PERM-11]: ../../../docs/specs/permissions.md

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

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

    /// Write the settings file this home's runs read, and return this scratch for chaining.
    ///
    /// What a test writes here is what cannot be stated in the environment: a `provider` block,
    /// because a gateway is a block rather than a variable, and a `permissions` block, because a
    /// rule is one too.
    fn with_settings(self, json: &str) -> Self {
        self.with_state("settings.json", json)
    }

    /// Declare hooks under this home, as writing the file by hand does, and return this scratch
    /// for chaining.
    ///
    /// Gated with the one test that calls it, which is Unix only: an ungated helper is dead code on
    /// Windows, where `-D warnings` makes that a failed build rather than a warning.
    #[cfg(unix)]
    fn with_hooks(self, json: &str) -> Self {
        self.with_state("hooks.json", json)
    }

    /// Record an effort level under this home, as choosing one in the interface does, and return
    /// this scratch for chaining.
    fn with_effort(self, level: &str) -> Self {
        self.with_state("effort", &format!("{level}\n"))
    }

    /// Write one file of the state a home keeps, creating the directory it lives in.
    fn with_state(self, name: &str, contents: &str) -> Self {
        let directory = self.path.join(".bravebot");
        std::fs::create_dir_all(&directory).expect("create the state directory");
        std::fs::write(directory.join(name), contents).expect("write the state file");
        self
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
    run(home, None, environment, arguments)
}

/// The same, started in a directory of the test's choosing.
///
/// The working directory is where a checkout's `.bravebot` is found, and [`bravebot`] leaves it
/// wherever the test runner was started, which is this crate's own directory. A test about what a
/// project layer does has to put one somewhere no other test is reading.
fn bravebot_started_in(
    home: &Path,
    cwd: &Path,
    environment: &[(&str, &str)],
    arguments: &[&str],
) -> Output {
    run(home, Some(cwd), environment, arguments)
}

fn run(
    home: &Path,
    cwd: Option<&Path>,
    environment: &[(&str, &str)],
    arguments: &[&str],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    command
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .envs(environment.iter().copied())
        .args(arguments)
        // Not a terminal, and carrying nothing: a run that reads a pipe reads the end of the
        // input rather than waiting on whatever started the tests.
        .stdin(Stdio::null());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().expect("the built binary runs")
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

/// Brave's own endpoint with nothing imported and nothing else configured is a machine with no
/// service configured to serve a turn, which is what a released binary arrives as. A run that went
/// ahead would be answered by whatever that endpoint gives it, and read as the agent being poor.
///
/// So the run stops before it asks anything, and says the three ways to configure a service. Each
/// is asserted by the words somebody has to type or write, not by the sentence around them: a
/// refusal that named the problem and no route out is the first-use experience this replaced.
#[test]
fn a_first_run_with_no_service_configured_says_how_to_configure_one() {
    let scratch = Scratch::new("cli-running-no-service");
    let output = bravebot(
        &scratch.path,
        // Brave's own hosts, which is what a released binary arrives pointed at. Nothing is
        // imported under this home, so no service is configured to answer.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
        ],
        &["-p", "say something"],
    );

    let (stdout, stderr) = said(&output);
    // The configuration status rather than only a failure: what is wrong is the configuration, and
    // a script that could not tell this from an unreachable backend would retry it forever.
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(
        stdout.is_empty(),
        "the reply stream carried the explanation instead: {stdout}"
    );
    for route in ["amazon-bedrock", "OpenRouter", "bravebot import-leo-creds"] {
        assert!(
            stderr.contains(route),
            "the run refused without saying that {route} is a way to configure one: {stderr}"
        );
    }
}

/// And a configured gateway is not refused: it is a service that can answer, so the run goes to
/// it. The status says which happened, since the gateway here is a port nothing is listening on:
/// a run that reached it and found nothing there is a run that was not stopped beforehand.
///
/// The other half of the rule, and the half worth pinning. A refusal that fired on a configured
/// backend would take the agent away from everybody who set one up, which is the failure mode a
/// gate before the first request has.
#[test]
fn a_configured_gateway_is_not_refused() {
    let scratch = Scratch::new("cli-running-gateway-configured").with_settings(
        // Port 1 takes privileges the machine running tests does not give away, so the connection
        // is refused at once rather than timing out or reaching a real service. The `model` key is
        // what sends this run to the gateway rather than to Brave's endpoint.
        r#"{
            "provider": {
                "openrouter": {
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            },
            "model": "openrouter/z-ai/glm-4.6"
        }"#,
    );

    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
            ("OPENROUTER_API_KEY", "a-token"),
        ],
        &["-p", "say something"],
    );

    let (_, stderr) = said(&output);
    assert_eq!(
        output.status.code(),
        Some(5),
        "a configured gateway was refused as no service at all: {stderr}"
    );
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the run went somewhere other than the gateway it was given: {stderr}"
    );
}

/// A service configured while the model in force is still Brave's own has no service for that
/// model, and is the case a settings block copied out of another tool lands in: those blocks name
/// their models and name no default, so the model stays the one this build baked in.
///
/// Told apart from having nothing configured, since what this person has to do is name one of
/// their own models. Read as "nothing is configured" they would be sent to write the block they
/// have already written, and the three routes are what says which of the two happened.
#[test]
fn a_service_configured_with_no_model_of_its_own_named_says_to_name_one() {
    let scratch = Scratch::new("cli-running-gateway-no-model").with_settings(
        // The block, without the `model` key that names one of its own models.
        r#"{
            "provider": {
                "openrouter": {
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }
        }"#,
    );

    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
            ("OPENROUTER_API_KEY", "a-token"),
        ],
        &["-p", "say something"],
    );

    let (_, stderr) = said(&output);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.contains("`model` key"),
        "the run refused without saying which key names a model: {stderr}"
    );
    assert!(
        !stderr.contains("bravebot import-leo-creds"),
        "somebody who has configured a service was sent to configure another: {stderr}"
    );
}

/// And the same block with a model of its own named is not refused at all: the run goes to the
/// gateway, which here is a port nothing is listening on, so the status says it got that far.
///
/// `--model` rather than the settings key, because the flag is read at a different point from the
/// file and a gate reading the wrong one refuses a run over a model it was never going to ask for.
#[test]
fn a_model_named_on_the_command_line_is_not_refused() {
    let scratch = Scratch::new("cli-running-gateway-flagged").with_settings(
        r#"{
            "provider": {
                "openrouter": {
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }
        }"#,
    );

    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
            ("OPENROUTER_API_KEY", "a-token"),
        ],
        &["-p", "say something", "--model", "openrouter/z-ai/glm-4.6"],
    );

    let (_, stderr) = said(&output);
    assert_eq!(
        output.status.code(),
        Some(5),
        "a model the command line named was refused as no service at all: {stderr}"
    );
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the run went somewhere other than the gateway it was given: {stderr}"
    );
}

/// A machine with nowhere to keep credentials has none imported, rather than a batch that could
/// not be read.
///
/// An absent profile directory is a state this program supports, and the store answers "there is
/// nowhere to keep them" with the same error it uses for a file that would not read. Reported as
/// the second, the refusal told somebody on a machine that has never held a subscription that
/// theirs could not be used, and sent them to import it again.
///
/// Run as a process because the answer comes from the environment the program starts in, and a
/// test that set a variable would set it for every other test sharing the binary.
#[test]
fn a_machine_with_no_profile_directory_has_nothing_imported() {
    let output = Command::new(env!("CARGO_BIN_EXE_bravebot"))
        .env_clear()
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com")
        .env(
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
            "https://ai-chat-premium.bsg.brave.com",
        )
        .args(["-p", "say something"])
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");

    let (_, stderr) = said(&output);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(
        !stderr.contains("subscription that is stored"),
        "a machine with no profile directory was told its stored subscription is unusable: {stderr}"
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

/// A command line refused before it named a command is a failure before the turn, so CLI-12 owes
/// it a result object, and stdout is where the object goes. The usage table is written there too,
/// so a run that asked for an object used to get prose in its place and nothing to parse: the
/// caller had to tell a usage table from a result, which is the surface the flag exists to remove.
///
/// Both dispatch refusals a command line can carry the flag into are here, since routing one of
/// them and not the other leaves a caller that has to know which mistake it made. The same
/// invocations without the flag are here for the other direction: the table is what a person
/// mistyping a flag needs, and suppressing it for everybody would answer this clause by breaking
/// CLI-5.
#[test]
fn a_refused_command_line_asking_for_a_result_object_gets_one_instead_of_the_usage() {
    let scratch = Scratch::new("cli-running-refused-json");

    for (arguments, refused) in [
        (
            &["--nonsense", "--json", "-p", "say something"][..],
            "--nonsense",
        ),
        (&["--plain", "--json"][..], "--plain"),
    ] {
        let output = bravebot(&scratch.path, &[], arguments);
        let (stdout, stderr) = said(&output);

        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?} did not exit as a refused argument: {stderr}"
        );
        assert_eq!(
            stdout.lines().count(),
            1,
            "{arguments:?} did not answer with one object on one line: {stdout}"
        );
        for field in [
            r#""schema":1"#,
            r#""ok":false"#,
            r#""status":2"#,
            r#""reason":"argument""#,
            r#""identifier":"BB1002""#,
        ] {
            assert!(
                stdout.contains(field),
                "{field} is missing from what {arguments:?} answered with: {stdout}"
            );
        }
        // The other half of the clause: the object took the reply's place, and the prose that used
        // to be there went nowhere rather than sharing the stream with it.
        assert!(
            !stdout.contains("Usage:"),
            "{arguments:?} put the usage table on stdout beside the object: {stdout}"
        );
        // Unchanged by the flag: the person still reads why it was refused, on stderr.
        assert!(
            stderr.contains("BB1002") && stderr.contains(refused),
            "{arguments:?} stopped saying what it refused: {stderr}"
        );
    }

    for arguments in [&["--nonsense"][..], &["--plain", "--also-nonsense"][..]] {
        let output = bravebot(&scratch.path, &[], arguments);
        let (stdout, stderr) = said(&output);

        assert!(
            stdout.contains("Usage:"),
            "{arguments:?} asked for no object and lost the usage table too: {stdout} / {stderr}"
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

/// A hook that went wrong is said even by a run whose turn then failed (HOOK-7), which is the one
/// case where the outcome that would have carried the sentence never arrives.
///
/// Nothing a hook prints is read, so this report is the only way somebody learns their formatter has
/// not run since they mistyped its path, and a turn failing is no reason for them not to hear it.
/// A property of the process: the sentence is made by the agent, kept by the reporter the run built,
/// and printed by the ending it reached, and only a run that has all three says whether they are
/// joined up.
///
/// Unix only, for the path the declaration names: a `{:?}` of a Windows path is a JSON string of a
/// different shape, and what the program is there is not this test's question.
#[cfg(unix)]
#[test]
fn a_run_whose_turn_failed_still_says_what_its_hooks_said() {
    let scratch = Scratch::new("cli-running-hook-notices");
    // Attached to the end of the turn, which is the moment a failed turn reaches last and the one a
    // caller is likeliest to leave out. The program does not exist, so firing it cannot start.
    let missing = scratch.path.join("no-such-formatter");
    let scratch = scratch.with_hooks(&format!(
        r#"{{"hooks": [{{"on": "turn-finished", "run": [{missing:?}]}}]}}"#
    ));

    let output = bravebot(
        &scratch.path,
        // The configuration `a_turn_that_could_not_run_exits_non_zero` fails with: nothing wrong
        // with it, naming a port nothing can be listening on, so the turn ends with no outcome at
        // all rather than with a reply to carry the sentence.
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
        "the turn this is about did not fail, so it says nothing about a failed one: {stderr}"
    );
    assert!(
        stderr.contains("127.0.0.1:1"),
        "the run failed over something other than the backend it could not reach: {stderr}"
    );
    assert!(
        stderr.contains("no-such-formatter"),
        "the turn failed and nobody was told the hook could not start: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "the reply stream carried what the hook said: {stdout}"
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

/// CLI-6 over the command that reports rather than runs. `doctor` used to end every failure it
/// found on `ExitCode::FAILURE` and say the configuration problem with nothing in front of it, so
/// a CI job running it to check a machine could not tell "the configuration is wrong, fail the
/// build" from the catch-all, and the failure it pasted into a bug report had nothing to search
/// for.
///
/// The same environment as [`a_configuration_error_exits_non_zero`], so what is pinned is that
/// the two commands classify one configuration the same way rather than each having a status of
/// its own for it.
#[test]
fn doctor_ends_on_the_configuration_status_and_says_its_identifier() {
    let scratch = Scratch::new("cli-running-doctor-configuration");
    let output = bravebot(
        &scratch.path,
        // Complete but for the endpoint, which names no scheme.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert_eq!(
        output.status.code(),
        Some(3),
        "the report did not end on the configuration status: {stdout}{stderr}"
    );
    // In front of the message rather than instead of it: the sentence is what says what to fix,
    // and the value is what says the report failed over the endpoint it was handed.
    assert!(
        stderr.contains("BB1003: configuration error:"),
        "the identifier is not in front of the message: {stderr}"
    );
    assert!(
        stderr.contains("ai-chat.example.invalid"),
        "the report failed over something other than the endpoint it was given: {stderr}"
    );
}

/// The same status for the other way a configuration can be unusable: one this build can parse
/// and that names nothing which will serve a turn, which CLI-7 calls a configuration error rather
/// than a finding. The report says it and goes on to the end, so the status is accumulated across
/// the run rather than returned by the line that found the problem, and it is the whole of what a
/// caller has to tell this machine from one the report passed.
#[test]
fn doctor_ends_on_the_configuration_status_where_nothing_will_serve_a_turn() {
    let scratch = Scratch::new("cli-running-doctor-no-service");
    let output = bravebot(
        &scratch.path,
        // Brave's own hosts, which is what a released binary arrives pointed at, with nothing
        // imported under this home to spend against them.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert_eq!(
        output.status.code(),
        Some(3),
        "a report over a configuration that will serve no turn did not end on row 3: \
         {stdout}{stderr}"
    );
    // And the report still says it, in the section CLI-7 puts it in: a status is what a caller
    // reads and the routes out are what the person in front of the screen reads, so the fix is
    // not the report losing one to gain the other.
    assert!(
        stdout.contains("bravebot import-leo-creds"),
        "the report stopped saying how to configure a service: {stdout}"
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

/// CRED-10: the figure sizing the one brief window reaches the person, under the account of what
/// would end the credential it sizes. The record holding a figure nothing prints is the same
/// position as the record holding no figure: somebody reading this after a session credential
/// appears somewhere public still has nothing to weigh the window against.
///
/// Under that line rather than anywhere in the report, and once rather than three times: the
/// figure belongs to the session credential, and a report that offered it for the signing key
/// would claim a window bounds a credential that has none.
///
/// Both AWS arrangements are held wherever an account is configured, so an account is all the
/// fixture needs; the AWS CLI is never run, since this is what the configuration holds rather
/// than what a profile resolves to.
#[test]
fn doctor_sizes_the_window_on_a_session_credential_and_on_nothing_else() {
    let scratch = Scratch::new("cli-running-brief-window");

    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
            ("BRAVEBOT_USE_BEDROCK", "1"),
            ("AWS_REGION", "us-west-2"),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", "an-opus-arn"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(output.status.success(), "doctor did not run: {stderr}");

    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    let sized: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("noticed "))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        sized.len(),
        1,
        "one credential is at Held briefly, and this report sized {}: {stdout}",
        sized.len()
    );

    let at = sized[0];
    assert!(
        lines[at].contains("15 minutes"),
        "the window was reported without the figure the record holds: {}",
        lines[at]
    );
    // The session credential is the one above it: `aws sso logout` is in that account and in no
    // other, so this says which credential the figure was printed under.
    assert!(
        lines[at - 1].starts_with("ends ") && lines[at - 1].contains("aws sso logout"),
        "the figure was not reported under the credential it sizes: {}",
        lines[at - 1]
    );
}

/// CRED-3: the walk a credential took reaches the person, one line per drop, under the account of
/// what would end that credential. A record nothing prints is the position the clause describes:
/// somebody reading the report is told where each credential stands and nothing about which gate
/// put it there, so a tier is an assertion again.
///
/// Counted per credential rather than over the report, because the session credential is the one
/// thing here that passes a gate: two drops where the other two have three, and a report that
/// printed three for it would say AWS enforces nothing it does enforce.
///
/// Both AWS arrangements are held wherever an account is configured, so an account is all the
/// fixture needs; the AWS CLI is never run, since this is what the configuration holds rather
/// than what a profile resolves to.
#[test]
fn doctor_accounts_for_every_drop_of_each_credentials_walk() {
    let scratch = Scratch::new("cli-running-gate-walk");

    let output = bravebot(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
            ("BRAVEBOT_USE_BEDROCK", "1"),
            ("AWS_REGION", "us-west-2"),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", "an-opus-arn"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(output.status.success(), "doctor did not run: {stderr}");

    // One block per credential: the account of what would end it, and everything printed under it
    // until the next one.
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("ends "))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        starts.len(),
        3,
        "this configuration holds the signing key and both AWS arrangements: {stdout}"
    );

    for (position, at) in starts.iter().enumerate() {
        let until = starts.get(position + 1).copied().unwrap_or(lines.len());
        let dropped: Vec<&str> = lines[*at..until]
            .iter()
            .filter(|line| line.starts_with("dropped "))
            .copied()
            .collect();

        // The session credential is the one whose account names `aws sso logout`, and the one
        // gate anything here passes is its third.
        let expected = match lines[*at].contains("aws sso logout") {
            true => 2,
            false => 3,
        };
        assert_eq!(
            dropped.len(),
            expected,
            "the walk reported under {} is not the walk the record holds: {dropped:?}",
            lines[*at]
        );
        for (gate, line) in dropped.iter().enumerate() {
            assert!(
                line.contains(&format!("gate {}", gate + 1)),
                "the drops are reported out of the order the gates are asked: {dropped:?}"
            );
        }
    }
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

/// PERM-11's first reporting site, from the file on disk to the words `doctor` prints: a deny rule
/// nested one array too deep, which is the ordinary way this key is mistyped.
///
/// Running the binary because the report is assembled out of two places that only meet here. The
/// settings layer cannot hand on an entry that is not a line, the rule parser names only what it
/// was handed, and `doctor` prints the one list the two of them make. A fix that stops short of
/// the command leaves somebody believing `.env` is denied, and that is what this sees and a test
/// of either half does not.
#[test]
fn doctor_names_a_permission_entry_that_is_not_a_rule() {
    let scratch = Scratch::new("cli-running-unreadable-rule")
        .with_settings(r#"{"permissions": {"deny": [["Read(./.env)"]]}}"#);

    let output = bravebot(
        &scratch.path,
        // A configuration with nothing else wrong with it, for the reason the layers test above
        // states: a build with no credentials baked in would otherwise stop at that instead.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(
        stdout.contains(r#"["Read(./.env)"]"#),
        "doctor did not name the entry it dropped: {stdout}{stderr}"
    );
    assert!(
        !output.status.success(),
        "a rule this build cannot act on was reported and the run still passed: {stdout}"
    );
}

/// PERM-14's report, from the process that reads the file: a checkout's `allow` entry is dropped,
/// and `doctor` names the rule and the file it was written in.
///
/// Running the binary rather than calling the crate, because the report crosses three of them: the
/// entry that drops the rule is `bravebot-config`'s, the words are `bravebot-i18n`'s, and the line
/// is printed by `bravebot-cli`. A rule dropped and reported nowhere reads to whoever wrote it as
/// one in force, which is the failure this rejects, and an in-process test of the config crate
/// cannot tell a missing line from a line nobody prints.
#[test]
fn doctor_names_an_allow_rule_a_checkout_wrote() {
    let scratch = Scratch::new("cli-running-checkout-allow");
    let cwd = scratch.path.join("checkout");
    let project = cwd.join(".bravebot");
    std::fs::create_dir_all(&project).expect("create the project directory");
    std::fs::write(
        project.join("settings.json"),
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"], "deny": ["Read(.env)"]}}"#,
    )
    .expect("write the project layer");

    let output = bravebot_started_in(
        &scratch.path,
        &cwd,
        // A configuration with nothing wrong with it, for the reason the named-settings test above
        // states: a run that stopped at the configuration would never reach the settings section.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(output.status.success(), "doctor did not run: {stderr}");
    assert!(
        stdout.contains("Bash(bash scripts/check.sh)"),
        "the dropped rule was not named: {stdout}"
    );
    assert!(
        stdout.contains(&project.join("settings.json").display().to_string()),
        "the file the dropped rule was written in was not named: {stdout}"
    );
    // The same file's `deny` rule is still in force, so the report is about the one list that
    // grants rather than about the file. One rule, which is that one.
    assert!(
        stdout.contains("1 rule"),
        "the project layer's deny rule stopped applying: {stdout}"
    );
}

/// PERM-14's exclusion, from the same process: a checkout's `allow` entry that is not a rule is
/// named for what is wrong with it, under PERM-11, and not as a grant that was withheld.
///
/// One file with both kinds of entry, because the failure is a report that cannot tell them apart:
/// a build that offers every dropped entry calls the typo a rule to grant, which sends whoever
/// wrote it to a question that will never make it decide anything, and a build that offers none
/// calls the rule a typo. Running the binary rather than calling the crate, because which of the
/// two lines a `doctor` run prints is the whole of what a person sees, and the split is made in one
/// crate and worded in another.
#[test]
fn doctor_names_a_checkouts_unreadable_allow_entry_rather_than_offering_it() {
    let scratch = Scratch::new("cli-running-checkout-allow-unreadable");
    let cwd = scratch.path.join("checkout");
    let project = cwd.join(".bravebot");
    std::fs::create_dir_all(&project).expect("create the project directory");
    std::fs::write(
        project.join("settings.json"),
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)", "Nonsense"]}}"#,
    )
    .expect("write the project layer");

    let output = bravebot_started_in(
        &scratch.path,
        &cwd,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["doctor"],
    );

    let (stdout, _) = said(&output);
    assert!(
        stdout.contains("'Nonsense' names no family of tools"),
        "the entry that is not a rule was not named for what is wrong with it: {stdout}"
    );
    assert!(
        !stdout.contains("the allow rule Nonsense"),
        "a line nothing can act on was reported as a grant that was withheld: {stdout}"
    );
    // And the readable entry in the same file still is one, so the split is on readability rather
    // than on the whole list having stopped being offered.
    assert!(
        stdout.contains("the allow rule Bash(bash scripts/check.sh)"),
        "the entry that is a rule was not offered as a grant: {stdout}"
    );
    // An unreadable rule is a fault `doctor` reports on, exactly as one in the home layer is.
    assert!(
        !output.status.success(),
        "a rule this build cannot act on was reported and the run still passed: {stdout}"
    );
}

/// PERM-15: the other answer for the same rule. One the person granted for this directory is in
/// force, and `doctor` says so and counts it, because a report calling it "not granted" one line
/// above a session that honours it would send somebody looking for a fault that is not there.
///
/// The record is seeded rather than written by a session, because the answer is the person's and the
/// question that collects it needs a terminal. What is under test here is the reading: that `doctor`
/// looks in the record keyed on the directory it ran in, and reports and counts what it finds.
#[test]
fn doctor_says_an_allow_rule_a_checkout_wrote_is_granted_where_it_was() {
    let scratch = Scratch::new("cli-running-granted-allow");
    let cwd = scratch.path.join("checkout");
    let project = cwd.join(".bravebot");
    std::fs::create_dir_all(&project).expect("create the project directory");
    let settings = project.join("settings.json");
    std::fs::write(
        &settings,
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)"], "deny": ["Read(.env)"]}}"#,
    )
    .expect("write the project layer");

    // The record a session would have written on a yes: in the person's own directory, keyed on the
    // workspace the answer was given about, holding the rule text and the file that proposed it.
    let workspace = cwd.canonicalize().expect("canonical checkout");
    let granted = scratch.path.join(".bravebot").join("granted");
    std::fs::create_dir_all(&granted).expect("create the record directory");
    let record = granted.join(format!(
        "{}.jsonl",
        bravebot_agent::home::key_for(&workspace)
    ));
    // Written out rather than encoded, since this crate's tests carry no JSON library. Both paths
    // are under the scratch directory, so neither holds a character JSON would need escaped, and a
    // path that did would produce a line the record skips rather than a wrong answer.
    for path in [&workspace, &settings] {
        let shown = path.display().to_string();
        assert!(
            !shown.contains(['"', '\\']),
            "the scratch path needs JSON escaping, so this test would seed an unreadable line: {shown}"
        );
    }
    std::fs::write(
        &record,
        format!(
            concat!(
                r#"{{"workspace":"{}","session":"an-earlier-session","#,
                r#""rule":"Bash(bash scripts/check.sh)","path":"{}"}}"#,
                "\n"
            ),
            workspace.display(),
            settings.display(),
        ),
    )
    .expect("seed the record");

    let output = bravebot_started_in(
        &scratch.path,
        &cwd,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["doctor"],
    );

    let (stdout, stderr) = said(&output);
    assert!(output.status.success(), "doctor did not run: {stderr}");
    assert!(
        stdout.contains("is granted for this directory"),
        "the granted rule was not reported as granted: {stdout}"
    );
    assert!(
        !stdout.contains("is not granted"),
        "a rule the person granted was reported as dropped: {stdout}"
    );
    // Two rules now: the `deny` entry the file could write on its own, and the `allow` entry the
    // person granted. A count that left the grant out would say a session here has one.
    assert!(
        stdout.contains("2 rules"),
        "the granted rule was not counted as a rule in force: {stdout}"
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

/// A session in lines reads what the person types, so its input has to be a terminal: the lines it
/// reads are prompts, which are the one trusted input there is, and a pipe carries bytes nothing
/// vouched for (CLI-3). A session that took its prompts from one would take instruction from
/// whatever fed it and answer its own approval questions out of the same bytes, so it is refused
/// before anything starts, and the refusal names the invocation that does read a pipe.
///
/// A property of the process rather than of a function: whether stdin is a terminal is a fact
/// about how the program was started, and nothing inside it can arrange to be started the other
/// way.
#[test]
fn a_session_in_lines_is_refused_where_its_input_is_not_a_terminal() {
    let scratch = Scratch::new("cli-running-plain-not-a-terminal");
    let output = bravebot(
        &scratch.path,
        // Complete and usable, so the refusal below is this one rather than the configuration's.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
        ],
        &["--plain"],
    );

    let (stdout, stderr) = said(&output);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a refused argument did not exit as one: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "the reply stream carried the explanation instead: {stdout}"
    );
    assert!(
        stderr.contains("-p"),
        "the refusal does not name what does read a pipe: {stderr}"
    );
    // Nothing was taken from the terminal on the way to refusing, which is the whole claim of the
    // mode: the alternate screen, mouse reporting and bracketed paste are each a `\x1b[?` away.
    assert!(
        !stderr.contains('\x1b') && !stdout.contains('\x1b'),
        "something was asked of the terminal: {stderr:?}"
    );
}

/// Run the built binary with a terminal for its input, and read back everything it wrote to one.
///
/// A session in lines refuses a pipe before it does anything else, so nothing it decides after
/// that is reachable from a run whose stdin is a file or a socket. `script` gives a process a
/// terminal of its own, which is the one way to reach those decisions without a dependency of this
/// tree's own to allocate a pty with.
///
/// Both streams come back as one, because the terminal they were written to is one device. The end
/// of the input is what the run reads at its first question, so a session that opens ends itself
/// rather than waiting for as long as the suite is allowed to run.
///
/// Linux, because the two `script` commands in the world take different arguments and report the
/// child's status differently, and the job that runs this suite is Linux.
#[cfg(target_os = "linux")]
fn in_a_terminal(home: &Path, environment: &[(&str, &str)], arguments: &[&str]) -> Output {
    in_a_terminal_run(home, None, environment, arguments)
}

/// The same, started in a directory of the test's choosing, for [`bravebot_started_in`]'s reason:
/// a checkout's `.bravebot` is found from the working directory, and a test about what one of
/// those layers does has to put it somewhere no other test is reading.
#[cfg(target_os = "linux")]
fn in_a_terminal_started_in(
    home: &Path,
    cwd: &Path,
    environment: &[(&str, &str)],
    arguments: &[&str],
) -> Output {
    in_a_terminal_run(home, Some(cwd), environment, arguments)
}

#[cfg(target_os = "linux")]
fn in_a_terminal_run(
    home: &Path,
    cwd: Option<&Path>,
    environment: &[(&str, &str)],
    arguments: &[&str],
) -> Output {
    let quoted = format!("'{}'", env!("CARGO_BIN_EXE_bravebot"));
    let command = std::iter::once(quoted)
        .chain(arguments.iter().map(|argument| argument.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    let mut terminal = Command::new("script");
    terminal
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .envs(environment.iter().copied())
        // `-q` leaves out the banner script would otherwise write into what is asserted on, `-e`
        // reports the status the binary exited with rather than script's own, and the transcript
        // file is not wanted: what is read here is what script copies to its own stdout.
        .args(["-qec", &command, "/dev/null"])
        .stdin(Stdio::null());
    if let Some(cwd) = cwd {
        terminal.current_dir(cwd);
    }
    terminal
        .output()
        .expect("script runs the built binary in a terminal")
}

/// A session in lines is a session, so it does not open on a machine with no service configured to
/// serve a turn: the three ways to configure one are said instead, and the status is the
/// configuration one.
///
/// The surface that is easiest to leave out, because it is the one that draws nothing and so the
/// one a person testing a refusal never sees. Left out, the fourth way of starting a session takes
/// prompts and sends them to an endpoint with no subscription to spend on them, and what comes
/// back reads as the agent being poor rather than as a configuration nobody has written yet.
#[cfg(target_os = "linux")]
#[test]
fn a_session_in_lines_with_no_service_configured_says_how_to_configure_one() {
    let scratch = Scratch::new("cli-running-plain-no-service");
    let output = in_a_terminal(
        &scratch.path,
        // Brave's own hosts with nothing imported under this home, which is what a released
        // binary arrives as.
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
        ],
        &["--plain"],
    );

    let (transcript, _) = said(&output);
    assert_eq!(output.status.code(), Some(3), "{transcript}");
    for route in ["amazon-bedrock", "OpenRouter", "bravebot import-leo-creds"] {
        assert!(
            transcript.contains(route),
            "the run refused without saying that {route} is a way to configure one: {transcript}"
        );
    }
    // The session did not open, which is the half of the clause a refusal printed after the
    // opening line would not satisfy: what is forbidden is starting the work, not staying quiet
    // about the configuration.
    assert!(
        !transcript.contains("in lines"),
        "the session opened before it refused: {transcript}"
    );
    assert!(
        !transcript.contains("trust this directory?"),
        "the startup question was put on a machine with nothing to answer a turn: {transcript}"
    );
}

/// And a session in lines on a machine that has configured a service opens: the refusal is about
/// what is configured, not about the way the session was started.
///
/// The half worth pinning, since a gate in front of a session takes the agent away from everybody
/// who set a service up. The session ends at once because the end of the input is the answer to
/// its first question, and that it got as far as asking is what says it was not refused.
#[cfg(target_os = "linux")]
#[test]
fn a_session_in_lines_with_a_configured_gateway_opens() {
    let scratch = Scratch::new("cli-running-plain-gateway").with_settings(
        // The `model` key is what puts this session on the gateway rather than on Brave's
        // endpoint. Nothing is ever asked of the gateway here: the session ends at the startup
        // question, before a prompt is read.
        r#"{
            "provider": {
                "openrouter": {
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            },
            "model": "openrouter/z-ai/glm-4.6"
        }"#,
    );

    let output = in_a_terminal(
        &scratch.path,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                "https://ai-chat-premium.bsg.brave.com",
            ),
            ("OPENROUTER_API_KEY", "a-token"),
        ],
        &["--plain"],
    );

    let (transcript, _) = said(&output);
    assert!(
        transcript.contains("trust this directory?"),
        "a configured gateway was refused as no service at all: {transcript}"
    );
    assert!(
        !transcript.contains("bravebot import-leo-creds"),
        "somebody who has configured a service was sent to configure another: {transcript}"
    );
    // The end of the input in place of an answer to the startup question starts no session and
    // is not a failure, so anything else here is a session that opened and then fell over.
    assert_eq!(output.status.code(), Some(0), "{transcript}");
    // Nothing was asked of the terminal on the way, which is the claim the mode exists for and
    // which only a session that opens can be held to: the alternate screen, mouse reporting and
    // bracketed paste are each a `\x1b[?` away.
    assert!(
        !transcript.contains('\x1b'),
        "something was asked of the terminal: {transcript:?}"
    );
}

/// PERM-14 on the third surface that reads the file: a session in lines names the `allow` entry a
/// checkout wrote and the file it was written in, and names an entry that is not a rule under
/// PERM-11 instead.
///
/// The surface a report is easiest to leave out of, because it is the one that draws nothing. This
/// session grants none of these entries, since it puts no question and so has nowhere an answer
/// could have come from (PERM-15), and it said nothing about them either, which leaves whoever
/// wrote one reading it as a rule in force while the prompt it was meant to answer keeps appearing.
///
/// Running the binary in a terminal because that is the only way to reach the decision: a session in
/// lines refuses a pipe before it reads a settings file, so nothing downstream of that refusal is
/// observable from an ordinary child process. Both kinds of entry from one file, for the reason the
/// `doctor` test above gives.
#[cfg(target_os = "linux")]
#[test]
fn a_session_in_lines_names_an_allow_rule_a_checkout_wrote() {
    let scratch = Scratch::new("cli-running-plain-checkout-allow").with_settings(
        // A configured gateway, for the reason the test above gives: a session in lines refuses
        // before it opens on a machine with no service to serve a turn, and nothing is ever asked
        // of this one. The session ends at the startup question, which comes after the report.
        r#"{
            "provider": {
                "openrouter": {
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            },
            "model": "openrouter/z-ai/glm-4.6"
        }"#,
    );
    let cwd = scratch.path.join("checkout");
    let project = cwd.join(".bravebot");
    std::fs::create_dir_all(&project).expect("create the project directory");
    std::fs::write(
        project.join("settings.json"),
        r#"{"permissions": {"allow": ["Bash(bash scripts/check.sh)", "Nonsense"]}}"#,
    )
    .expect("write the project layer");

    let output = in_a_terminal_started_in(
        &scratch.path,
        &cwd,
        &[
            ("SERVICES_KEY_AICHAT", "a-services-key"),
            ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
            ("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com"),
            ("OPENROUTER_API_KEY", "a-token"),
        ],
        &["--plain"],
    );

    let (transcript, _) = said(&output);
    assert!(
        transcript.contains("not granting the allow rule Bash(bash scripts/check.sh)"),
        "the dropped rule was not named: {transcript}"
    );
    assert!(
        transcript.contains(&project.join("settings.json").display().to_string()),
        "the file the dropped rule was written in was not named: {transcript}"
    );
    assert!(
        transcript.contains("'Nonsense' names no family of tools"),
        "the entry that is not a rule was not named for what is wrong with it: {transcript}"
    );
    assert!(
        !transcript.contains("the allow rule Nonsense"),
        "a line nothing can act on was reported as a grant that was withheld: {transcript}"
    );
    // Said before the startup question, which is the first thing a person is asked to answer: a
    // report printed after it would be one they read having already decided.
    let (report, question) = transcript
        .split_once("trust this directory?")
        .unwrap_or_else(|| panic!("the session never reached the startup question: {transcript}"));
    assert!(
        report.contains("not granting the allow rule"),
        "the report came after the first question: {question}"
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

/// A gateway that answers one roster and keeps what was asked of it.
///
/// Stood up rather than mocked because the subject is what a *process* puts on the wire: the level
/// a run sends is settled between reading the store and building the request, and nothing inside
/// the program can be asked what a request carried.
struct Gateway {
    port: u16,
    /// The body of each chat request, in the order they arrived. Rosters are not sent here: the
    /// first thing to come out is the first request a turn made.
    asked: mpsc::Receiver<String>,
}

/// Stand one up, offering a single model that takes `parameters` and nothing else.
///
/// Answers every chat request with a server error, which is the cheapest way to end the run: an
/// invalid-request status is what a service refusing the level itself answers with, and would have
/// the client drop the field on its own (BACKEND-22), so a test using one could not tell the two
/// apart.
fn a_gateway_listing(parameters: &str) -> Gateway {
    let listing = format!(
        r#"{{"data": [{{"id": "reasons-only", "context_length": 262144, "supported_parameters": {parameters}}}]}}"#
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    let (sender, asked) = mpsc::channel();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));

            let mut request = String::new();
            let _ = reader.read_line(&mut request);

            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);

            let answer = match request.starts_with("GET") {
                true => http(200, &listing),
                false => {
                    let _ = sender.send(String::from_utf8_lossy(&body).into_owned());
                    http(500, r#"{"error": {"message": "nothing here answers"}}"#)
                }
            };
            let _ = stream.write_all(answer.as_bytes());
            let _ = stream.flush();
        }
    });

    Gateway { port, asked }
}

/// One JSON response, framed.
fn http(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} \r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// The settings that send a run to `gateway` and name its one model, with no `models` key, so the
/// roster is the gateway's own answer rather than something the file stated.
fn settings_for(gateway: &Gateway) -> String {
    format!(
        r#"{{
            "provider": {{
                "openrouter": {{
                    "env": ["OPENROUTER_API_KEY"],
                    "options": {{"baseURL": "http://127.0.0.1:{}/api/v1"}}
                }}
            }},
            "model": "openrouter/reasons-only"
        }}"#,
        gateway.port
    )
}

/// The environment such a run needs: Brave's own endpoint is a port nothing listens on, so the
/// roster under test is the gateway's and no request leaves the machine.
const AT_A_GATEWAY: &[(&str, &str)] = &[
    ("SERVICES_KEY_AICHAT", "a-services-key"),
    ("BRAVE_SERVICES_KEY_ID", "a-key-id"),
    ("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1"),
    ("OPENROUTER_API_KEY", "a-token"),
];

/// A level recorded in the store does not reach a request to a model whose listing states which
/// parameters it takes and does not name the field (BACKEND-22).
///
/// A service that reads the field and one that discards it answer identically, so a level sent
/// where the roster says it is not read is a charge somebody chose, was billed for, and did not
/// get, with the interface reporting it as in force. The roster has already answered the question
/// here, so there is nothing to guess.
///
/// A property of the process: the run reads the level off disk, fetches the listing, and builds
/// the request, and only what went out on the wire says whether those were joined up.
#[test]
fn a_run_withholds_a_level_the_roster_says_the_model_does_not_read() {
    let gateway = a_gateway_listing(r#"["tools", "reasoning"]"#);
    let scratch = Scratch::new("cli-running-effort-withheld")
        .with_settings(&settings_for(&gateway))
        .with_effort("max");

    let output = bravebot(&scratch.path, AT_A_GATEWAY, &["-p", "say something"]);

    let (_, stderr) = said(&output);
    let asked = gateway
        .asked
        .recv_timeout(Duration::from_secs(60))
        .expect("the run reached the gateway");
    assert!(
        !asked.contains("reasoning_effort"),
        "a level went to a model the listing says takes no such parameter: {asked}"
    );
    // The level is still a level somebody chose, and it applies again the moment a model that
    // reads one is in force, so a run must not have spent it.
    assert_eq!(
        std::fs::read_to_string(scratch.path.join(".bravebot").join("effort"))
            .expect("the recorded level")
            .trim(),
        "max",
        "the run threw away the choice instead of withholding it"
    );
    assert!(
        stderr.contains("reads no effort level"),
        "the run withheld the level and said nothing about it: {stderr}"
    );
}

/// And the same run against a listing that names the field sends it. The withholding is the roster
/// answering the question, not a run deciding for itself: a rule that fired on every gateway would
/// take the level away from every model that reads one, which nothing would report either.
#[test]
fn a_run_sends_a_level_the_roster_says_the_model_reads() {
    let gateway = a_gateway_listing(r#"["tools", "reasoning", "reasoning_effort"]"#);
    let scratch = Scratch::new("cli-running-effort-sent")
        .with_settings(&settings_for(&gateway))
        .with_effort("max");

    let output = bravebot(&scratch.path, AT_A_GATEWAY, &["-p", "say something"]);

    let (_, stderr) = said(&output);
    let asked = gateway
        .asked
        .recv_timeout(Duration::from_secs(60))
        .expect("the run reached the gateway");
    assert!(
        asked.contains(r#""reasoning_effort":"max""#),
        "a level the listing names as read was withheld: {asked}"
    );
    assert!(
        !stderr.contains("reads no effort level"),
        "a model that reads a level was reported as reading none: {stderr}"
    );
}

/// A one-shot run takes one turn and exits, so nothing is left holding the line to send it again:
/// a tool for arranging a later look is not offered here (SCHED-6).
///
/// Offered it, the run answers a request to report a change by calling it, is told back that the
/// next look is arranged and needs nothing from the user, writes that into its reply, prints the
/// reply and exits. The person is told a watch exists and there is no watch and no next look.
///
/// A property of the process rather than of a task: which tools a surface offers is settled where
/// that surface builds its turn, and only what went out on the wire says what it settled on.
#[test]
fn a_one_shot_run_offers_no_way_to_arrange_a_later_look() {
    let gateway = a_gateway_listing(r#"["tools", "reasoning"]"#);
    let scratch = Scratch::new("cli-running-no-later-look").with_settings(&settings_for(&gateway));

    bravebot(
        &scratch.path,
        AT_A_GATEWAY,
        &["-p", "read a.txt, then tell me when it changes"],
    );

    let asked = gateway
        .asked
        .recv_timeout(Duration::from_secs(60))
        .expect("the run reached the gateway");
    assert!(
        !asked.contains("schedule_next"),
        "a run that exits after one turn was offered a way to arrange a later look: {asked}"
    );
    // The table itself was sent, so the absence above is this tool being withheld rather than a
    // request that carried no tools at all.
    assert!(
        asked.contains("read_file"),
        "the request carried no tool table: {asked}"
    );
}

/// The session in lines settles the level the same way, against the listing it fetches at startup.
///
/// Its own test because it builds its own task, per prompt, out of its own state: the one-shot
/// run's turn is assembled somewhere else entirely, and a fix to one says nothing about the other.
///
/// Linux only, because reaching this mode at all needs stdin to be a terminal (CLI-3) and
/// `script(1)` is what supplies one. The argument form here is util-linux's; the BSD program of
/// the same name takes another, and a run against the wrong one would fail for a reason that has
/// nothing to do with what is under test.
#[cfg(target_os = "linux")]
#[test]
fn a_session_in_lines_withholds_a_level_the_roster_says_the_model_does_not_read() {
    let gateway = a_gateway_listing(r#"["tools", "reasoning"]"#);
    let scratch = Scratch::new("cli-running-effort-withheld-in-lines")
        .with_settings(&settings_for(&gateway))
        .with_effort("max");

    let mut session = Command::new("/usr/bin/script")
        .env_clear()
        .env("HOME", &scratch.path)
        .env("BRAVEBOT_LOCALE", "en-US")
        .envs(AT_A_GATEWAY.iter().copied())
        // `-q` so the program's own lines are the whole of what comes back, `-e` so its status is,
        // and `/dev/null` for the transcript nothing here reads.
        .args([
            "-qec",
            &format!("{} --plain", env!("CARGO_BIN_EXE_bravebot")),
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("a terminal for a session in lines");

    // The startup trust question, then one prompt. Answered no: what is under test is what the
    // turn sends, and a session that trusted this directory would send the same request.
    session
        .stdin
        .take()
        .expect("the session's input")
        .write_all(b"n\nsay something\n")
        .expect("write the script");
    let output = session.wait_with_output().expect("the session ends");

    let (said_to_the_person, _) = said(&output);
    let asked = gateway
        .asked
        .recv_timeout(Duration::from_secs(60))
        .expect("the session reached the gateway");
    assert!(
        !asked.contains("reasoning_effort"),
        "a level went to a model the listing says takes no such parameter: {asked}"
    );
    assert!(
        said_to_the_person.contains("reads no effort level"),
        "the session withheld the level and said nothing about it: {said_to_the_person}"
    );
}
