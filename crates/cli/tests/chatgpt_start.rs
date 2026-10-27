//! BACKEND-39 through the built binary, for a person who has signed in with a ChatGPT account and
//! configured nothing else, against the stand-in of `crates/agent/test-support/chatgpt_stand_in.rs`:
//! what a start does about the model it will request.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on ([CHATGPT-2]). Brave's own hosts
//! are named, so that nothing serves the model Brave's endpoint answers for and the start is the one
//! BACKEND-39 refuses, and a proxy that refuses is named for every host but this machine, so no
//! request leaves for them.
//!
//! These tests run on Linux and macOS, as `chatgpt_account.rs` does, since the home directory the
//! binary reads is set by `HOME` and a terminal is made with `script`.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_run::{Scratch, Shown, environment, sign_in_with};
use chatgpt_stand_in::{
    ACCESS_TWO, Asked, MODEL, NOT_ELIGIBLE, REFRESH_ONE, REFRESH_TWO, Refresh, Serving, StandIn,
    USAGE_LIMIT, now,
};

/// A home that is signed in and holds nothing else: no gateway, no Bedrock block, no Leo batch, no
/// settings and no remembered model.
fn signed_in_home(name: &str) -> Scratch {
    let home = Scratch::new(name);
    home.store_sign_in(ACCESS_TWO, REFRESH_TWO, now() + 3600);
    home
}

/// The command that runs the built binary with `arguments` in a terminal of a size, which the
/// interface that draws measures once at its start. `script` is the terminal, and the shell it runs
/// sets the size.
fn in_a_sized_terminal(arguments: &[&str]) -> Command {
    let quoted: Vec<String> = arguments
        .iter()
        .map(|argument| format!("'{}'", argument.replace('\'', r"'\''")))
        .collect();
    let line = format!(
        "/bin/stty cols 120 rows 40; exec '{}' {}",
        env!("CARGO_BIN_EXE_bravebot"),
        quoted.join(" ")
    );
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", "/bin/sh", "-c", &line]);
    #[cfg(target_os = "linux")]
    command.args(["-qec", &line, "/dev/null"]);
    command
}

/// The environment of a run with Brave's own hosts, which only the sign-in can replace, and no way
/// out to them.
fn at_braves_hosts(command: &mut Command, home: &Scratch, stand_in: &StandIn) {
    environment(command, home, stand_in);
    command
        .env("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com")
        .env(
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
            "https://ai-chat-premium.bsg.brave.com",
        )
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "127.0.0.1");
}

/// The requests the stand-in took as a turn, in order.
fn turns(stand_in: &StandIn) -> Vec<Asked> {
    stand_in
        .asked()
        .into_iter()
        .filter(|asked| asked.path == "/v1/responses")
        .collect()
}

/// What a run showed, and the status it ended with, or `None` where it was still running when it
/// was stopped.
struct Ran {
    shown: String,
    code: Option<i32>,
}

/// A run of the built binary in a terminal, which a person types lines at.
struct Terminal {
    child: std::process::Child,
    shown: Shown,
    typed: Option<std::process::ChildStdin>,
}

impl Terminal {
    fn start(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("script runs the built binary in a terminal");
        let shown = Shown::read(child.stdout.take().expect("the terminal"));
        let typed = child.stdin.take();
        Self {
            child,
            shown,
            typed,
        }
    }

    /// Whether the terminal has shown `words` before `bound` passes.
    fn shows(&self, words: &str, bound: Duration) -> bool {
        let stop = Instant::now() + bound;
        while Instant::now() < stop {
            if self.shown.text().contains(words) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Type `line` and a return.
    fn type_line(&mut self, line: &str) {
        let typed = self.typed.as_mut().expect("the input is still open");
        writeln!(typed, "{line}").expect("a line typed");
    }

    /// The end of the input, which ends a session in lines.
    fn end_input(&mut self) {
        self.typed = None;
    }

    /// What the run showed once it ended, or when `bound` passed and it was stopped.
    fn finish(mut self, bound: Duration) -> Ran {
        let stop = Instant::now() + bound;
        let code = loop {
            if let Some(status) = self.child.try_wait().expect("the terminal") {
                break status.code();
            }
            if Instant::now() >= stop {
                break None;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ran {
            shown: self.shown.finished(),
            code,
        }
    }
}

/// Run the built binary with `arguments` in `home`, in a terminal, until the stand-in has taken a
/// turn, the run has ended or a minute has passed.
fn started(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Ran {
    let mut command = in_a_sized_terminal(arguments);
    at_braves_hosts(&mut command, home, stand_in);
    let terminal = Terminal::start(command);
    let stop = Instant::now() + Duration::from_secs(60);
    while turns(stand_in).is_empty() && Instant::now() < stop {
        if terminal.shows("BB1003", Duration::from_millis(50)) {
            break;
        }
    }
    terminal.finish(Duration::from_millis(200))
}

fn the_state_file(home: &Scratch, name: &str) -> Option<String> {
    std::fs::read_to_string(Path::new(&home.path).join(".bravebot").join(name)).ok()
}

/// BACKEND-39, CLI-31: a session opened with `-i` and a `--model` is asked about the model named,
/// which the account serves, and not about the model in force without the flag, which is Brave's
/// own. The regression it rejects is the start asking about the model in force alone: it ends at the
/// refusal for no service although the model the person named is served, and the person has no way
/// to give the model a name that works, since a session never opens to pick one in. The turn that
/// `-i` carries reaches the account, under the sign-in's own token.
#[test]
fn a_session_on_a_prompt_starts_on_the_model_the_command_line_names_where_the_account_serves_it() {
    let home = signed_in_home("cli-chatgpt-start-named");
    let stand_in = StandIn::new(vec![]);

    let ran = started(
        &home,
        &stand_in,
        &[
            "-i",
            "hello",
            "--model",
            MODEL,
            "--dangerously-skip-permissions",
        ],
    );

    assert!(
        !ran.shown.contains("BB1003") && !ran.shown.contains("no model service"),
        "the start was refused for the model in force: {}",
        ran.shown
    );
    let asked = turns(&stand_in);
    assert_eq!(asked.len(), 1, "{asked:?}\n{}", ran.shown);
    assert_eq!(asked[0].bearer.as_deref(), Some(ACCESS_TWO), "{asked:?}");
    assert!(
        the_state_file(&home, "model").is_none(),
        "a model named for one start was remembered"
    );
}

/// BACKEND-39: the model `-i` names is the one asked about, and a recorded pick the account serves
/// does not stand in for it. The flag outranks the record, so a start that asked about the record
/// would open on Brave's model, which nothing serves, and send the first turn there. The regression
/// it rejects is a start that drops the check on the named model once the model in force answers.
#[test]
fn a_session_on_a_prompt_naming_a_model_nothing_serves_is_refused_whatever_the_record_serves() {
    let home = signed_in_home("cli-chatgpt-start-named-unserved");
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    let stand_in = StandIn::new(vec![]);

    let ran = started(
        &home,
        &stand_in,
        &[
            "-i",
            "hello",
            "--model",
            "automatic",
            "--dangerously-skip-permissions",
        ],
    );

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(
        ran.shown.contains("no model service is configured yet"),
        "the start was not refused for the model named: {}",
        ran.shown
    );
    assert!(
        turns(&stand_in).is_empty(),
        "a turn was sent on a model nothing serves"
    );
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some(format!("{MODEL}\n").as_str()),
        "the record was changed"
    );
}

/// The words the question about the account's models ends in.
const THE_QUESTION: &str = "Which one will bravebot use?";

/// What the account lists, in the order the service gives them. The picker lists them by name, so
/// the first row a person is shown is `gpt-a`, which the service gave second.
const THE_ACCOUNTS_MODELS: &[&str] = &["gpt-b", "gpt-a", "gpt-c"];

/// A stand-in whose listing answers with `names` each time it is asked, which is once for the
/// question and once more for the session that opens on the pick.
fn offering(names: &'static [&'static str]) -> StandIn {
    let stand_in = StandIn::new(vec![]);
    for _ in 0..8 {
        stand_in.state().listing.push_back(Serving::Lists(names));
    }
    stand_in
}

/// A session in lines, started in `home` from a terminal. Nothing is asked of its person beyond what
/// a start asks, so its first prompt is the end of its input.
fn plain_session(home: &Scratch, stand_in: &StandIn, flags: &[&str]) -> Terminal {
    let mut arguments = vec!["--plain", "--dangerously-skip-permissions"];
    arguments.extend_from_slice(flags);
    let mut command = in_a_sized_terminal(&arguments);
    at_braves_hosts(&mut command, home, stand_in);
    Terminal::start(command)
}

/// How a session in lines names the model it opens on.
fn opening_on(model: &str) -> String {
    format!("in lines, {model}.")
}

const THE_REFUSAL: &str = "no model service is configured yet";

/// BACKEND-39: a start that nothing serves, with a ChatGPT sign-in stored and the account listing
/// models, shows the rows by number, takes a number, records the pick as `/model` records one, and
/// opens on it. The next start has the pick in force and asks nothing.
///
/// The service lists `gpt-b` first and the picker lists the rows by name, so a number is the row it
/// numbers in the order shown. The regressions it rejects are a pick recorded for the row the
/// service listed at that place, a pick recorded anywhere but the file `/model` writes, a session
/// that opens on Brave's model after the pick, and a next start that asks again.
#[test]
fn a_start_with_only_a_sign_in_asks_which_model_and_the_pick_is_what_the_session_and_the_next_start_open_on()
 {
    let home = signed_in_home("cli-chatgpt-start-asks");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let mut terminal = plain_session(&home, &stand_in, &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("2");
    let opened = terminal.shows(&opening_on("chatgpt/gpt-b"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert!(
        opened,
        "the session did not open on the pick: {}",
        ran.shown
    );
    assert_eq!(ran.code, Some(0), "{}", ran.shown);
    let row = |number: usize, name: &str| format!("  {number}. Named {name} (chatgpt/{name})");
    let rows = [row(1, "gpt-a"), row(2, "gpt-b"), row(3, "gpt-c")];
    let positions: Vec<usize> = rows
        .iter()
        .map(|row| {
            ran.shown
                .find(row.as_str())
                .unwrap_or_else(|| panic!("{row} was not shown: {}", ran.shown))
        })
        .collect();
    assert!(
        positions.is_sorted(),
        "the rows came in another order: {}",
        ran.shown
    );
    assert!(
        positions[2] < ran.shown.find(THE_QUESTION).expect("the question"),
        "a row was shown after the question: {}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(
        ran.shown.contains("Using Named gpt-b. It is remembered"),
        "the pick was not said to be remembered: {}",
        ran.shown
    );
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("chatgpt/gpt-b\n"),
        "the pick was not recorded as /model records one"
    );

    // The next start has a model in force, so it asks nothing and opens on the pick.
    let mut terminal = plain_session(&home, &stand_in, &[]);
    let opened = terminal.shows(&opening_on("chatgpt/gpt-b"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(
        opened,
        "the next start did not open on the pick: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains(THE_QUESTION),
        "the next start asked: {}",
        ran.shown
    );
    assert_eq!(ran.code, Some(0), "{}", ran.shown);
}

/// BACKEND-39: the interface that draws asks the same question at the same moment, and goes on to
/// draw its screen once a row is picked. The regression it rejects is a start that asks only in the
/// session in lines, which is the one of the two the person does not meet first.
#[test]
fn the_interface_that_draws_asks_the_same_question_and_draws_once_a_row_is_picked() {
    let home = signed_in_home("cli-chatgpt-start-asks-screen");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    at_braves_hosts(&mut command, &home, &stand_in);

    let mut terminal = Terminal::start(command);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    assert!(
        !terminal.shown.text().contains("\u{1b}[?1049h"),
        "the screen was drawn before the question was answered"
    );
    terminal.type_line("1");
    let drawn = terminal.shows("\u{1b}[?1049h", Duration::from_secs(30));
    let ran = terminal.finish(Duration::from_millis(200));

    assert!(drawn, "no screen was drawn after the pick: {}", ran.shown);
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("chatgpt/gpt-a\n")
    );
}

/// BACKEND-39: an answer that is not the number of a row, an empty answer and the end of the input
/// are each a decline. The start is refused as it is where nothing could be asked, and nothing is
/// recorded. The regressions it rejects are a number past the last row clamped to it, zero read as
/// the last row, a sign or a second word read as a number, an empty answer taken as the first row,
/// and a decline that records the first row.
#[test]
fn an_answer_that_is_not_the_number_of_a_row_declines_and_records_nothing() {
    for answer in [
        Some(""),
        Some("0"),
        Some("4"),
        Some("-1"),
        Some("+1"),
        Some("1 2"),
        Some("gpt-a"),
        None,
    ] {
        let home = signed_in_home("cli-chatgpt-start-declined");
        let stand_in = offering(THE_ACCOUNTS_MODELS);

        let mut terminal = plain_session(&home, &stand_in, &[]);
        assert!(
            terminal.shows(THE_QUESTION, Duration::from_secs(30)),
            "{answer:?}: nothing was asked: {}",
            terminal.shown.text()
        );
        match answer {
            Some(typed) => terminal.type_line(typed),
            None => terminal.end_input(),
        }
        let ran = terminal.finish(Duration::from_secs(30));

        assert_eq!(ran.code, Some(3), "{answer:?}: {}", ran.shown);
        assert!(
            ran.shown.contains(THE_REFUSAL),
            "{answer:?} was not a decline: {}",
            ran.shown
        );
        assert!(
            the_state_file(&home, "model").is_none(),
            "{answer:?} recorded a model"
        );
    }
}

/// BACKEND-39: nothing is asked where the account offers nothing to choose from: a listing the
/// service refused, a listing that could not be had and the compiled names that stand in for one in
/// a picker, and a listing that was read and is empty. The start is refused, and the refusal says why in
/// the sentence the sign-in command says it in, above the line that says a sign-in is stored, which is
/// true of each: the file is as it was. The regressions it rejects are a question over the compiled
/// names, which would choose for a person a model the account may refuse, and a refusal that drops
/// the reason, which leaves a person who was never asked wondering why.
#[test]
fn nothing_is_asked_where_the_account_lists_no_model_and_the_refusal_says_why() {
    let nothing: &'static [&'static str] = &[];
    let refused_with = |status, code| {
        let stand_in = StandIn::new(vec![]);
        stand_in
            .state()
            .listing
            .push_back(Serving::RefusesWithCode(status, code));
        stand_in
    };
    for (what, stand_in, said) in [
        (
            "a listing that could not be had",
            StandIn::new(vec![]),
            "could not be listed",
        ),
        (
            "a listing that is empty",
            offering(nothing),
            "reaches no model",
        ),
        (
            "a listing refused for the usage limit",
            refused_with(429, USAGE_LIMIT),
            "usage limit",
        ),
        (
            "a listing refused for the account's standing",
            refused_with(403, NOT_ELIGIBLE),
            "does not allow",
        ),
    ] {
        let home = signed_in_home("cli-chatgpt-start-no-rows");

        let terminal = plain_session(&home, &stand_in, &[]);
        let ran = terminal.finish(Duration::from_secs(30));

        assert_eq!(ran.code, Some(3), "{what}: {}", ran.shown);
        assert!(ran.shown.contains(THE_REFUSAL), "{what}: {}", ran.shown);
        assert!(!ran.shown.contains(THE_QUESTION), "{what}: {}", ran.shown);
        let at = |words: &str| {
            ran.shown
                .find(words)
                .unwrap_or_else(|| panic!("{what}: {words:?} was not said: {}", ran.shown))
        };
        assert!(
            at(said) < at("A ChatGPT sign-in is stored"),
            "{what}: the reason does not come first: {}",
            ran.shown
        );
        assert!(the_state_file(&home, "model").is_none(), "{what}");
    }
}

/// BACKEND-39: a sign-in that has ended is found by asking for the listing, which removes the file. The
/// refusal that follows is built from the sign-in as it is then: it says the sign-in has ended, and the
/// routes, and does not say a sign-in is stored. The access token is past its expiry and the refresh
/// token is refused as no longer valid. The regression it rejects is the refusal built from the sign-in
/// as it was before the listing, which names a file that was removed a moment earlier.
#[test]
fn a_sign_in_that_ended_while_the_listing_was_asked_for_is_not_said_to_be_stored() {
    let home = Scratch::new("cli-chatgpt-start-ended");
    home.store_sign_in(ACCESS_TWO, REFRESH_ONE, 1);
    let stand_in = StandIn::new(vec![]);
    stand_in.state().refreshing.push_back(Refresh::Ended);

    let terminal = plain_session(&home, &stand_in, &[]);
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains("has ended"), "{}", ran.shown);
    assert!(
        ran.shown.contains("bravebot auth login chatgpt") && ran.shown.contains("amazon-bedrock"),
        "the routes were not said: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("A ChatGPT sign-in is stored"),
        "a sign-in that was removed was said to be stored: {}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(!home.account().exists(), "the ended sign-in was left");
}

/// BACKEND-39: `bravebot --bg` asks in the terminal that typed it, as a start does, and a decline ends
/// it with the refusal before any background process is started. A decline is the one answer that
/// starts nothing, so the test needs no process left running. The regression it rejects is a start in
/// the background that does not ask, which sends its prompt to Brave's endpoint with nothing configured.
#[test]
fn a_background_start_asks_in_the_terminal_and_a_decline_ends_it() {
    let home = signed_in_home("cli-chatgpt-start-background");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = in_a_sized_terminal(&["--bg", "hello"]);
    at_braves_hosts(&mut command, &home, &stand_in);

    let mut terminal = Terminal::start(command);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "a background start did not ask: {}",
        terminal.shown.text()
    );
    terminal.type_line("");
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());
}

/// BACKEND-39: where stdin, stdout and stderr are not all terminals the start asks nothing and reads
/// nothing, and does not ask the account either. A line piped in is not an answer. The regressions it
/// rejects are a question put to a pipe, which a script would answer without knowing, and a listing
/// fetched with the sign-in for a start that cannot use it.
#[test]
fn a_start_that_is_not_in_a_terminal_asks_nothing_reads_nothing_and_does_not_ask_the_account() {
    let home = signed_in_home("cli-chatgpt-start-no-terminal");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    at_braves_hosts(&mut command, &home, &stand_in);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the built binary runs");
    writeln!(child.stdin.take().expect("its stdin"), "2").expect("a line piped in");
    let output = child.wait_with_output().expect("the run ends");

    let said = chatgpt_run::said(&output);
    assert_eq!(output.status.code(), Some(3), "{said}");
    assert!(said.contains(THE_REFUSAL), "{said}");
    assert!(!said.contains(THE_QUESTION), "{said}");
    assert!(the_state_file(&home, "model").is_none(), "a pipe answered");
    assert!(
        stand_in.asked().is_empty(),
        "the account was asked for a start that cannot use the answer: {:?}",
        stand_in.asked()
    );
}

/// BACKEND-39, INCOG-8: an incognito session asks nothing and records nothing, and does not ask the
/// account. The regression it rejects is a question in the one mode that leaves nothing on the
/// machine, which would write the pick.
#[test]
fn an_incognito_start_asks_nothing_and_records_nothing() {
    let home = signed_in_home("cli-chatgpt-start-incognito");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let terminal = plain_session(&home, &stand_in, &["--incognito"]);
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());
    assert!(
        stand_in.asked().is_empty(),
        "the account was asked in an incognito start: {:?}",
        stand_in.asked()
    );
}

/// BACKEND-39: a start whose model in force is served asks nothing, and so does one where a gateway
/// block is configured, which asks about the gateway's models and not the account's, and whose
/// declined question ends at the refusal that names the `model` key. The regressions it rejects are a question at every start of a person who has signed
/// in, and a question where a service other than the account is configured.
#[test]
fn nothing_is_asked_where_a_model_is_served_or_a_gateway_is_configured() {
    let home = signed_in_home("cli-chatgpt-start-served");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        format!(r#"{{"model": "{MODEL}"}}"#),
    )
    .expect("a settings file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut terminal = plain_session(&home, &stand_in, &[]);
    let opened = terminal.shows(&opening_on(MODEL), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(opened, "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());

    let home = signed_in_home("cli-chatgpt-start-gateway");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"provider": {"openrouter": {
            "env": ["OPENROUTER_API_KEY"],
            "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
            "models": {"z-ai/glm-4.6": {}}
        }}}"#,
    )
    .expect("a settings file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut terminal = plain_session(&home, &stand_in, &[]);
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains("`model` key"), "{}", ran.shown);
    assert!(
        ran.shown.contains("openrouter/z-ai/glm-4.6") && !ran.shown.contains("the account offers"),
        "the question was not the gateway's: {}",
        ran.shown
    );
    assert!(
        stand_in.asked().is_empty(),
        "the account was asked beside a configured gateway: {:?}",
        stand_in.asked()
    );
}

/// BACKEND-39: a model named on the command line is not replaced by a question. The flag outranks a
/// recorded pick, so a pick made at a start that named a model nothing serves would change nothing,
/// and the person named the model. The regression it rejects is a question asked over a `--model`.
#[test]
fn a_model_named_on_the_command_line_is_not_asked_about() {
    let home = signed_in_home("cli-chatgpt-start-named-not-asked");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let terminal = {
        let mut command = in_a_sized_terminal(&[
            "-i",
            "hello",
            "--model",
            "automatic",
            "--dangerously-skip-permissions",
        ]);
        at_braves_hosts(&mut command, &home, &stand_in);
        Terminal::start(command)
    };
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());
}

/// The words of `line`, a double-quoted run of them kept as one word with its quotes taken off.
fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            other => word.push(other),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The commands `text` names, each as the arguments that follow `bravebot`, with a placeholder in
/// angle brackets replaced by something that names a thing: a prompt, a task, and the account's model.
fn commands_named_in(text: &str) -> Vec<Vec<String>> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| span.starts_with("bravebot "))
        .map(|span| {
            words_of(span)
                .into_iter()
                .skip(1)
                .map(|word| {
                    word.replace("<prompt>", "hello")
                        .replace("<task>", "hello")
                        .replace("<model>", "gpt-placeholder")
                })
                .collect()
        })
        .collect()
}

/// BACKEND-39: every command the refusal names works as typed. The refusal of a start that nothing
/// serves, where a sign-in is stored and nobody can be asked, names two commands, one that opens a
/// session and one that runs a task, and each of them is run here as the refusal writes it, in the
/// terminal or without one as it needs. `/model` is named only as what a session lists once it is
/// open. The regression it rejects is advice for `bravebot --model chatgpt/<model>`, which is refused
/// for want of a task, and advice that sends a person to `/model` before a session can open.
#[test]
fn every_command_the_refusal_names_works_as_typed() {
    let home = signed_in_home("cli-chatgpt-start-advice");
    let stand_in = StandIn::new(vec![]);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    at_braves_hosts(&mut command, &home, &stand_in);
    let refusal = command
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    let said = chatgpt_run::said(&refusal);
    assert_eq!(refusal.status.code(), Some(3), "{said}");
    assert!(said.contains(THE_REFUSAL), "{said}");

    let commands = commands_named_in(&said);
    assert_eq!(
        commands,
        vec![
            vec!["-i", "hello", "--model", MODEL],
            vec!["hello", "--model", MODEL]
        ],
        "the refusal does not name a command that opens a session and one that runs a task: {said}"
    );
    assert!(
        !said.contains("bravebot --model"),
        "the refusal names a form that needs a task: {said}"
    );
    let at = |words: &str| {
        said.find(words)
            .unwrap_or_else(|| panic!("{words:?}: {said}"))
    };
    assert!(
        at("/model") > at("session is open"),
        "/model is named before a session can be open: {said}"
    );

    for arguments in &commands {
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let turns_before = turns(&stand_in).len();
        match arguments[0] {
            "-i" => {
                let mut command = in_a_sized_terminal(&arguments);
                at_braves_hosts(&mut command, &home, &stand_in);
                let terminal = Terminal::start(command);
                let drawn = terminal.shows("\u{1b}[?1049h", Duration::from_secs(30));
                let ran = terminal.finish(Duration::from_millis(200));
                assert!(
                    drawn && !ran.shown.contains("BB100"),
                    "{arguments:?} did not open a session as typed: {}",
                    ran.shown
                );
            }
            _ => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
                at_braves_hosts(&mut command, &home, &stand_in);
                let output = command
                    .args(&arguments)
                    .stdin(Stdio::null())
                    .output()
                    .expect("the built binary runs");
                assert!(
                    output.status.success(),
                    "{arguments:?} did not run as typed: {}",
                    chatgpt_run::said(&output)
                );
                assert_eq!(
                    turns(&stand_in).len(),
                    turns_before + 1,
                    "{arguments:?} sent no turn to the account: {}",
                    chatgpt_run::said(&output)
                );
            }
        }
    }
}

/// BACKEND-39: the refusal says that `bravebot` started on its own asks which model to use, and it is
/// also what a one-shot run, a start that names a model and `doctor` end in, none of which asks. The
/// sentence is about the start that does, and the starts that show it are each run here with the
/// account listing models: each ends in the refusal, with the sentence, and puts no question and asks
/// the account nothing. The start the sentence is about asks. The regression it rejects is the
/// sentence worded for every refusal as if it were a bare start's, which is false for the three.
#[test]
fn the_refusal_says_which_start_asks_and_the_starts_that_show_it_without_asking_say_so() {
    let sentence = "Started on its own";
    for (what, arguments) in [
        ("a one-shot run", vec!["-p", "hello"]),
        (
            "a start that names a model",
            vec!["-i", "hello", "--model", "automatic"],
        ),
        ("doctor", vec!["doctor"]),
    ] {
        let home = signed_in_home("cli-chatgpt-start-refusal-scope");
        let stand_in = offering(THE_ACCOUNTS_MODELS);
        let mut command = in_a_sized_terminal(&arguments);
        at_braves_hosts(&mut command, &home, &stand_in);

        let ran = Terminal::start(command).finish(Duration::from_secs(30));

        assert_eq!(ran.code, Some(3), "{what}: {}", ran.shown);
        assert!(ran.shown.contains(THE_REFUSAL), "{what}: {}", ran.shown);
        assert!(ran.shown.contains(sentence), "{what}: {}", ran.shown);
        assert!(!ran.shown.contains(THE_QUESTION), "{what}: {}", ran.shown);
        assert!(
            stand_in.asked().is_empty(),
            "{what} asked the account: {:?}",
            stand_in.asked()
        );
    }

    let home = signed_in_home("cli-chatgpt-start-refusal-scope-bare");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = in_a_sized_terminal(&["--dangerously-skip-permissions"]);
    at_braves_hosts(&mut command, &home, &stand_in);
    let terminal = Terminal::start(command);
    let asked = terminal.shows(THE_QUESTION, Duration::from_secs(30));
    let ran = terminal.finish(Duration::from_millis(100));
    assert!(asked, "a start on its own did not ask: {}", ran.shown);
}

/// A home with no sign-in yet and `settings` in its settings file, and the sign-in made there with
/// Brave's own hosts named. Returns what the command showed.
fn signed_in_by_the_command(
    name: &str,
    settings: Option<&str>,
    stand_in: &StandIn,
) -> (Scratch, String) {
    let home = Scratch::new(name);
    if let Some(settings) = settings {
        std::fs::write(home.path.join(".bravebot/settings.json"), settings)
            .expect("a settings file");
    }
    let (terminal, _, signed_in) = sign_in_with(&home, stand_in, |command| {
        at_braves_hosts(command, &home, stand_in);
    });
    assert!(signed_in, "the sign-in failed: {terminal}");
    (home, terminal)
}

/// BACKEND-39, CHATGPT-1: the sign-in command promises the question only to a person the next start
/// asks. Where nothing serves the model in force it sends the person to run `bravebot`, and the start
/// then asks. Where a gateway is configured, or a model is in force that something serves, it names the
/// commands and promises nothing, and the start then asks nothing: the gateway's refusal names the
/// `model` key, and the served model opens. The regression it rejects is the promise made to every
/// person who signs in, which is false for those two.
#[test]
fn the_sign_in_command_promises_the_question_only_to_a_person_the_next_start_asks() {
    let promise = "Run bravebot to be asked";

    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let (home, terminal) = signed_in_by_the_command("cli-chatgpt-start-promise", None, &stand_in);
    assert!(terminal.contains(promise), "{terminal}");
    let start = plain_session(&home, &stand_in, &[]);
    let asked = start.shows(THE_QUESTION, Duration::from_secs(30));
    let ran = start.finish(Duration::from_millis(100));
    assert!(
        asked,
        "the start did not ask what the sign-in promised: {}",
        ran.shown
    );

    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let (home, terminal) = signed_in_by_the_command(
        "cli-chatgpt-start-no-promise-gateway",
        Some(
            r#"{"provider": {"openrouter": {
                "env": ["OPENROUTER_API_KEY"],
                "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
                "models": {"z-ai/glm-4.6": {}}
            }}}"#,
        ),
        &stand_in,
    );
    assert!(
        terminal.contains("Name one of the account's models") && !terminal.contains(promise),
        "{terminal}"
    );
    let mut start = plain_session(&home, &stand_in, &[]);
    start.end_input();
    let ran = start.finish(Duration::from_secs(30));
    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains("`model` key"), "{}", ran.shown);
    assert!(!ran.shown.contains("the account offers"), "{}", ran.shown);

    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let (home, terminal) = signed_in_by_the_command(
        "cli-chatgpt-start-no-promise-served",
        Some(&format!(r#"{{"model": "{MODEL}"}}"#)),
        &stand_in,
    );
    assert!(
        terminal.contains("Name one of the account's models") && !terminal.contains(promise),
        "{terminal}"
    );
    let mut start = plain_session(&home, &stand_in, &[]);
    let opened = start.shows(&opening_on(MODEL), Duration::from_secs(30));
    start.end_input();
    let ran = start.finish(Duration::from_secs(30));
    assert!(opened, "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
}

/// A name the store reads back as no choice, since a recorded name is read only up to 128 bytes: with
/// `chatgpt/` ahead of it, the key of this row is longer.
const A_NAME_TOO_LONG_TO_KEEP: &str = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

/// BACKEND-39: the start says a pick is remembered only where what it reads back is that name. A key
/// the store will not read back is written and read as no choice, so the start would go on to the
/// refusal; it says that the name could not be remembered, does not say it is using it, and ends as a
/// decline does. The regression it rejects is the start saying it is using a model and remembering it
/// and then refusing for want of a model.
#[test]
fn a_pick_the_start_cannot_read_back_is_not_said_to_be_remembered_and_ends_the_start() {
    let home = signed_in_home("cli-chatgpt-start-not-remembered");
    let stand_in = offering(&[A_NAME_TOO_LONG_TO_KEEP]);

    let mut terminal = plain_session(&home, &stand_in, &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("1");
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(
        ran.shown.contains("Could not remember Named"),
        "the start did not say the name could not be kept: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("Using ") && !ran.shown.contains("remembered for the next start"),
        "the start said it was using a model it could not keep: {}",
        ran.shown
    );
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
}

/// A home with a gateway block that names two models and no `model` key, so the model in force stays
/// Brave's own, and no sign-in.
fn gateway_home(name: &str) -> Scratch {
    let home = Scratch::new(name);
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"provider": {"openai": {
            "options": {"baseURL": "http://127.0.0.1:1/v1"},
            "models": {"gpt-z": {}, "gpt-4o-mini": {}}
        }}}"#,
    )
    .expect("write the settings");
    home
}

/// BACKEND-39: a start with a gateway configured and Brave's model in force lists the gateway's models
/// by number, records the pick as `/model` records one, opens on it, and the next start asks nothing.
///
/// The block names `gpt-z` first and the picker lists by name, so number 1 is `gpt-4o-mini`.
/// The regressions it rejects are a start that ends at the refusal naming the `model` key, a pick not
/// recorded as `/model` records it, and a next start that asks again.
#[test]
fn a_start_with_a_configured_gateway_asks_which_model_and_the_pick_is_what_the_next_start_opens_on()
{
    let home = gateway_home("cli-gateway-start-asks");
    let stand_in = StandIn::new(vec![]);

    let mut terminal = plain_session(&home, &stand_in, &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("1");
    let opened = terminal.shows(&opening_on("openai/gpt-4o-mini"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert!(
        opened,
        "the session did not open on the pick: {}",
        ran.shown
    );
    assert!(
        ran.shown.contains("  2. gpt-z (openai/gpt-z)"),
        "{}",
        ran.shown
    );
    assert!(
        ran.shown.contains("  1. gpt-4o-mini (openai/gpt-4o-mini)"),
        "{}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("openai/gpt-4o-mini\n")
    );

    let mut terminal = plain_session(&home, &stand_in, &[]);
    let opened = terminal.shows(&opening_on("openai/gpt-4o-mini"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(
        opened,
        "the next start did not open on the pick: {}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
}

/// BACKEND-39: an answer that is not a row's number declines, records nothing and ends at the
/// refusal that names the `model` key. The regression it rejects is a start that picks a model for
/// the person.
#[test]
fn a_declined_gateway_question_records_nothing_and_ends_at_the_refusal() {
    let home = gateway_home("cli-gateway-start-declined");
    let stand_in = StandIn::new(vec![]);

    let mut terminal = plain_session(&home, &stand_in, &[]);
    assert!(terminal.shows(THE_QUESTION, Duration::from_secs(30)));
    terminal.type_line("");
    let ran = terminal.finish(Duration::from_secs(30));

    assert_ne!(ran.code, Some(0), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert_eq!(the_state_file(&home, "model"), None);
}
