//! Tests for running a command line the user typed, against a real shell in a real directory.
//!
//! The interesting cases are the ones that separate shell mode from [`bravebot_agent::exec`]: a glob
//! expands, a variable expands, a redirection writes. Those are refused for the planner and are the
//! whole point here, so each is pinned rather than assumed.

use bravebot_agent::conversation::Conversation;
use bravebot_agent::shell::{self, ShellError};
use bravebot_aichat::protocol::Role;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::{Event, NullSink, RecordingSink};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

/// A scratch directory that removes itself, so tests do not leave state behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bravebot-shell-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run(line: &str, at: &std::path::Path) -> Result<shell::Ran, ShellError> {
    shell::run(line, at, &Cancel::new())
}

#[test]
fn a_command_returns_what_it_printed() {
    let scratch = Scratch::new("printed");
    let ran = run("echo hello", &scratch.path).expect("echo runs");
    assert_eq!(ran.stdout.trim(), "hello");
    assert!(ran.succeeded());
}

/// The reason shell mode exists rather than reusing the argv path: a user typing `*.txt` means the
/// files, and a mode that handed the literal string to a program would be useless to them.
#[test]
fn a_glob_is_expanded_by_the_shell() {
    let scratch = Scratch::new("glob");
    std::fs::write(scratch.path.join("one.txt"), "").expect("write");
    std::fs::write(scratch.path.join("two.txt"), "").expect("write");
    std::fs::write(scratch.path.join("three.md"), "").expect("write");

    let ran = run("echo *.txt", &scratch.path).expect("echo runs");

    assert_eq!(ran.stdout.trim(), "one.txt two.txt");
}

#[test]
fn a_variable_is_expanded_by_the_shell() {
    let scratch = Scratch::new("variable");
    let ran = run("echo \"[$USER]\"", &scratch.path).expect("echo runs");
    // Asserted as non-empty rather than against a name, since the test runner's user varies.
    assert_ne!(ran.stdout.trim(), "[]");
    assert!(ran.stdout.starts_with('['));
}

/// Redirection is the clearest case of something the planner may never have and the user must:
/// writing a file is an effect, and here the person chose it.
#[test]
fn a_redirection_writes_the_file() {
    let scratch = Scratch::new("redirect");
    let ran = run("echo written > out.txt", &scratch.path).expect("echo runs");

    assert!(ran.succeeded());
    let contents = std::fs::read_to_string(scratch.path.join("out.txt")).expect("the file exists");
    assert_eq!(contents.trim(), "written");
}

#[test]
fn stages_are_piped_together() {
    let scratch = Scratch::new("pipe");
    let ran = run("printf 'a\\nb\\nc\\n' | wc -l", &scratch.path).expect("the pipeline runs");
    assert_eq!(ran.stdout.trim(), "3");
}

#[test]
fn commands_joined_with_and_both_run() {
    let scratch = Scratch::new("andand");
    let ran = run("echo first && echo second", &scratch.path).expect("both run");
    let lines: Vec<&str> = ran.stdout.lines().collect();
    assert_eq!(lines, vec!["first", "second"]);
}

#[test]
fn a_command_runs_in_the_directory_it_was_given() {
    let scratch = Scratch::new("cwd");
    let ran = run("pwd", &scratch.path).expect("pwd runs");
    // Compared canonically, since the temporary directory is reached through a symlink on macOS.
    let printed = std::fs::canonicalize(ran.stdout.trim()).expect("the printed path exists");
    let expected = std::fs::canonicalize(&scratch.path).expect("the scratch path exists");
    assert_eq!(printed, expected);
}

/// A failure has to come back with what the command said about it. Dropping stderr would leave the
/// user looking at an exit code with no explanation, which is the least useful thing to show.
#[test]
fn a_failing_command_reports_its_code_and_its_message() {
    let scratch = Scratch::new("failing");
    let ran = run("echo trouble >&2; exit 3", &scratch.path).expect("the shell runs");

    assert!(!ran.succeeded());
    assert_eq!(ran.code, Some(3));
    assert_eq!(ran.stderr.trim(), "trouble");
}

/// The user is not typing at these commands: the terminal belongs to the TUI. A command that reads
/// stdin must come back empty rather than wait for keys nobody is sending.
#[test]
fn a_command_that_reads_stdin_is_given_nothing_rather_than_the_terminal() {
    let scratch = Scratch::new("stdin");
    let ran = run("cat", &scratch.path).expect("cat runs");
    assert!(ran.stdout.is_empty());
    assert!(ran.succeeded());
}

#[test]
fn cancelling_stops_a_running_command() {
    let scratch = Scratch::new("cancel");
    let cancel = Cancel::new();
    let stopper = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        stopper.cancel();
    });

    let outcome = shell::run("sleep 30", &scratch.path, &cancel);

    assert!(
        matches!(outcome, Err(ShellError::Cancelled)),
        "expected a cancellation, got {outcome:?}"
    );
}

/// Output larger than a pipe buffer must not deadlock against a drain nobody is running.
#[test]
fn a_large_result_does_not_deadlock() {
    let scratch = Scratch::new("large");
    let ran = run("yes long-enough-line | head -n 20000", &scratch.path).expect("it runs");
    assert_eq!(ran.stdout.lines().count(), 20000);
}

/// An empty line is a keystroke, not a command. Handing `-c ''` to a shell succeeds and produces
/// nothing, which would put a pointless empty result in the transcript.
#[test]
fn an_empty_line_is_not_run() {
    let scratch = Scratch::new("empty");
    let outcome = run("   ", &scratch.path);
    assert!(
        matches!(outcome, Err(ShellError::Io(_))),
        "expected a refusal, got {outcome:?}"
    );
}

/// `$SHELL` is what makes the line behave the way it does in the user's own terminal. A system with
/// it unset or blank still has to get a POSIX shell rather than an empty program name, and one that
/// names a shell gets that shell.
#[test]
fn the_shell_falls_back_to_a_posix_one_when_the_variable_is_unset() {
    assert_eq!(shell::shell_named(None), "/bin/sh");
    assert_eq!(shell::shell_named(Some(String::new())), "/bin/sh");
    assert_eq!(shell::shell_named(Some("  ".to_string())), "/bin/sh");
    assert_eq!(
        shell::shell_named(Some("/usr/bin/fish".to_string())),
        "/usr/bin/fish"
    );
}

/// The whole point of the feature: a user who runs something and then says "fix that" is relying on
/// the planner having seen it. The output reaches the context, which for any other command's output
/// would be a violation and here is the user's own doing.
#[test]
fn what_a_command_printed_reaches_the_planners_context() {
    let scratch = Scratch::new("context");
    let mut conversation = Conversation::new();
    let mut sink = NullSink;

    let ran = run("echo the-output", &scratch.path).expect("it runs");
    let recorded = shell::record("echo the-output", &ran, &mut conversation, &mut sink)
        .expect("it is recorded");

    assert!(recorded.succeeded);
    let said = conversation
        .messages()
        .iter()
        .map(|m| m.message.content.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("the-output"),
        "the planner was not told what it printed: {said}"
    );
    assert!(
        said.contains("echo the-output"),
        "the planner was not told what was run: {said}"
    );
}

/// A private command is shown without being recorded: the gate still releases the bytes the screen
/// draws, and the trail still says what labelled them, while `show` has no conversation to push into.
#[test]
fn a_private_command_is_released_for_the_screen_and_the_trail() {
    let scratch = Scratch::new("private");
    let mut sink = RecordingSink::new();

    let ran = run("echo for-my-eyes", &scratch.path).expect("it runs");
    let shown = shell::show("echo for-my-eyes", &ran, &mut sink).expect("it is shown");

    assert!(shown.text.contains("for-my-eyes"), "nothing was drawn");
    assert!(shown.succeeded);
    assert!(
        sink.events().iter().any(|event| matches!(
            event,
            Event::GatePassed { gate: "provenance", detail }
                if detail.contains("echo for-my-eyes") && detail.contains("the user typed")
        )),
        "the decision left no trace: {:?}",
        sink.events()
    );
}

/// Said as the user's own message rather than a tool result, because that is what happened: the
/// planner did not call anything, and describing it as a call would credit it with an action it
/// could not have taken.
#[test]
fn the_command_is_recorded_as_something_the_user_did() {
    let scratch = Scratch::new("role");
    let mut conversation = Conversation::new();
    let mut sink = NullSink;

    let ran = run("echo hi", &scratch.path).expect("it runs");
    shell::record("echo hi", &ran, &mut conversation, &mut sink).expect("it is recorded");

    let roles: Vec<Role> = conversation
        .messages()
        .iter()
        .map(|m| m.message.role)
        .collect();
    assert_eq!(roles, vec![Role::User]);
    assert_eq!(
        conversation.messages()[0].composed,
        Some(bravebot_agent::conversation::Composed::Written)
    );
}

/// SHELL-2: nothing asks. The only things a command line is handed are the line, the directory it
/// runs in and a cancel flag, and for a command shown as it prints, where to send what it prints.
/// The only thing the recording step is handed besides the result is a sink for the trail. None of
/// them takes a confirmer or any other way to put a question to the person, so adding an approval
/// prompt to any of them changes the signature this binds and stops compiling.
#[test]
fn a_command_line_is_run_and_recorded_without_any_way_to_ask() {
    let run: fn(&str, &std::path::Path, &Cancel) -> Result<shell::Ran, ShellError> = shell::run;
    let _run_live: fn(
        &str,
        &std::path::Path,
        &Cancel,
        &mpsc::Sender<String>,
    ) -> Result<shell::Ran, ShellError> = shell::run_live;
    let record: fn(
        &str,
        &shell::Ran,
        &mut Conversation,
        &mut NullSink,
    ) -> Result<shell::Recorded, ShellError> = shell::record::<NullSink>;

    let scratch = Scratch::new("asks-nothing");
    let mut conversation = Conversation::new();
    let ran = run("echo unasked", &scratch.path, &Cancel::new()).expect("it runs unasked");
    let recorded =
        record("echo unasked", &ran, &mut conversation, &mut NullSink).expect("it is recorded");
    assert!(recorded.succeeded);
}

/// Labelling a command's output trusted is the most consequential decision in the feature, so it
/// has to be visible in the trail rather than taken quietly.
#[test]
fn trusting_the_output_is_recorded_in_the_trail() {
    let scratch = Scratch::new("trail");
    let mut conversation = Conversation::new();
    let mut sink = RecordingSink::new();

    let ran = run("echo audited", &scratch.path).expect("it runs");
    shell::record("echo audited", &ran, &mut conversation, &mut sink).expect("it is recorded");

    assert!(
        sink.events().iter().any(|event| matches!(
            event,
            Event::GatePassed { gate: "provenance", detail }
                if detail.contains("echo audited") && detail.contains("the user typed")
        )),
        "the decision left no trace: {:?}",
        sink.events()
    );
}

/// A command that failed is still something the planner should know about: someone who runs a build
/// that breaks and says "fix it" means the errors it printed.
#[test]
fn a_failing_commands_output_still_reaches_the_planner() {
    let scratch = Scratch::new("failed");
    let mut conversation = Conversation::new();
    let mut sink = NullSink;

    let ran = run("echo the-error >&2; exit 1", &scratch.path).expect("the shell runs");
    let recorded = shell::record(
        "echo the-error >&2; exit 1",
        &ran,
        &mut conversation,
        &mut sink,
    )
    .expect("it is recorded");

    assert!(!recorded.succeeded);
    let said = conversation.messages()[0].message.content.text();
    assert!(said.contains("the-error"), "stderr was dropped: {said}");
    assert!(
        said.contains("exited 1"),
        "the planner was not told it failed: {said}"
    );
}

/// A command that was stopped records nothing, because there is no result to speak of and a
/// half-finished one described as output would be a claim about what it did. Running and recording
/// are separate calls, so a run that failed simply never reaches the second.
#[test]
fn a_cancelled_command_records_nothing() {
    let scratch = Scratch::new("cancelled-record");
    let conversation = Conversation::new();
    let cancel = Cancel::new();
    let stopper = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        stopper.cancel();
    });

    let outcome = shell::run("sleep 30", &scratch.path, &cancel);

    assert!(matches!(outcome, Err(ShellError::Cancelled)));
    assert!(
        conversation.messages().is_empty(),
        "a stopped command was described to the planner anyway"
    );
}

/// What the person sees when the command has ended and what the planner is told come from the same
/// gated value. Returning the ungated copy would leave the check deciding nothing, since the bytes
/// would reach the screen whatever it said. What is drawn while the command runs is
/// [`what_a_command_prints_is_sent_before_the_command_ends`]'s, and is for the screen alone.
#[test]
fn what_is_shown_is_what_the_gate_released() {
    let scratch = Scratch::new("gated");
    let mut conversation = Conversation::new();
    let mut sink = NullSink;

    let ran = run("echo shown-and-said", &scratch.path).expect("it runs");
    let recorded =
        shell::record("echo shown-and-said", &ran, &mut conversation, &mut sink).expect("recorded");

    assert!(recorded.text.contains("shown-and-said"));
    assert!(
        conversation.messages()[0]
            .message
            .content
            .text()
            .contains(&recorded.text)
    );
}

/// A loop that waits for the `release` file, and gives up after twenty seconds so a test that
/// failed before it released the command does not leave a shell waiting for good.
const UNTIL_RELEASED: &str =
    "n=0; while [ ! -e release ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done";

/// `script` as a line for whatever `$SHELL` is: the line runs a POSIX shell on it, since a loop and
/// arithmetic are not spelled the same in every shell a person may have set. The script holds no
/// single quote.
fn in_a_posix_shell(script: &str) -> String {
    format!("sh -c '{script}'")
}

/// How long a test waits for a piece the command has already printed. A broken implementation fails
/// at this bound where the command is still waiting on the test, instead of hanging.
const PIECE: Duration = Duration::from_secs(10);

/// A scratch directory of this test run's own, so two runs of the suite on one host, such as the
/// parallel slots of a build machine, do not remove each other's `release` file.
fn own_scratch(name: &str) -> Scratch {
    Scratch::new(&format!("{name}-{}", std::process::id()))
}

/// Run `line` as [`shell::run_live`] on a thread of its own, with the pieces it sends.
fn run_live_on_a_thread(
    line: &str,
    at: &std::path::Path,
    cancel: Cancel,
) -> (
    std::thread::JoinHandle<Result<shell::Ran, ShellError>>,
    mpsc::Receiver<String>,
) {
    let (live, pieces) = mpsc::channel();
    let (line, at) = (line.to_string(), at.to_path_buf());
    let worker = std::thread::spawn(move || shell::run_live(&line, &at, &cancel, &live));
    (worker, pieces)
}

/// Let a command that is waiting for the `release` file go on.
fn release(at: &std::path::Path) {
    std::fs::write(at.join("release"), "").expect("release the command");
}

/// SHELL-7: what a command has printed reaches the screen while the command is still running. The
/// command here cannot end until the test has been sent its first line, so an implementation that
/// collects everything and sends it after the exit never sends anything for the test to receive.
#[test]
fn what_a_command_prints_is_sent_before_the_command_ends() {
    let scratch = own_scratch("live-before-end");
    let (worker, pieces) = run_live_on_a_thread(
        &in_a_posix_shell(&format!("echo early; {UNTIL_RELEASED}; echo late")),
        &scratch.path,
        Cancel::new(),
    );

    let first = pieces
        .recv_timeout(PIECE)
        .expect("nothing was sent while the command was still running");
    assert_eq!(first, "early\n");

    release(&scratch.path);
    let ran = worker.join().expect("the worker ends").expect("it runs");
    assert_eq!(ran.stdout, "early\nlate\n");
    assert_eq!(pieces.recv_timeout(PIECE).expect("the rest"), "late\n");
}

/// What a command writes to standard error is as much a part of what a person watching it is waiting
/// for as standard output: a build says what is wrong there.
#[test]
fn standard_error_is_sent_as_it_is_printed() {
    let scratch = own_scratch("live-stderr");
    let (worker, pieces) = run_live_on_a_thread(
        &in_a_posix_shell(&format!("echo trouble >&2; {UNTIL_RELEASED}")),
        &scratch.path,
        Cancel::new(),
    );

    let first = pieces
        .recv_timeout(PIECE)
        .expect("standard error was held back until the command ended");
    assert_eq!(first, "trouble\n");

    release(&scratch.path);
    let ran = worker.join().expect("the worker ends").expect("it runs");
    assert_eq!(ran.stderr, "trouble\n");
}

/// A read can stop inside a character. The piece that ends there must not draw as the replacement
/// character what the next piece makes whole, or the screen shows damage the output never had.
#[test]
fn a_character_a_read_stops_inside_is_sent_whole() {
    let scratch = own_scratch("live-split-character");
    // The first two of the three bytes of the euro sign, then the third once the test has seen "a".
    let (worker, pieces) = run_live_on_a_thread(
        &in_a_posix_shell(&format!(
            "printf \"a\\342\\202\"; {UNTIL_RELEASED}; printf \"\\254\""
        )),
        &scratch.path,
        Cancel::new(),
    );

    let first = pieces
        .recv_timeout(PIECE)
        .expect("the text before the split");
    assert_eq!(first, "a", "the unfinished character was sent in pieces");

    release(&scratch.path);
    let ran = worker.join().expect("the worker ends").expect("it runs");
    assert_eq!(ran.stdout, "a\u{20ac}");
    assert_eq!(pieces.recv_timeout(PIECE).expect("the rest"), "\u{20ac}");
}

/// The pieces put together read as what the command printed, bytes that are not UTF-8 included, so
/// the screen while it runs and the result when it ends do not disagree about them.
#[test]
fn the_pieces_put_together_are_what_the_command_printed() {
    let scratch = own_scratch("live-lossy");
    // A byte that starts nothing valid, and the start of a character the output ends in the middle of.
    let (worker, pieces) = run_live_on_a_thread(
        "printf 'one\\377two\\n'; printf 'end\\342\\202'",
        &scratch.path,
        Cancel::new(),
    );

    let ran = worker.join().expect("the worker ends").expect("it runs");
    let sent: String = pieces.try_iter().collect();

    assert_eq!(ran.stdout, "one\u{fffd}two\nend\u{fffd}");
    assert_eq!(sent, ran.stdout);
}

/// Nobody reading the pieces is no reason for a command to stop or for its result to be less: the
/// screen can be gone while the command is not.
#[test]
fn a_command_runs_on_when_nothing_is_reading_what_it_prints() {
    let scratch = own_scratch("live-nobody-reading");
    let (live, pieces) = mpsc::channel();
    drop(pieces);

    let ran = shell::run_live(
        &in_a_posix_shell("echo out; echo err >&2"),
        &scratch.path,
        &Cancel::new(),
        &live,
    )
    .expect("it runs");

    assert_eq!(ran.stdout, "out\n");
    assert_eq!(ran.stderr, "err\n");
}

/// What a stopped command printed was sent while it ran, and stopping it still ends in the
/// cancellation that [`a_cancelled_command_records_nothing`] pins, which no result follows.
#[test]
fn a_stopped_command_has_sent_what_it_printed_and_is_still_cancelled() {
    let scratch = own_scratch("live-stopped");
    let cancel = Cancel::new();
    let (worker, pieces) = run_live_on_a_thread(
        &in_a_posix_shell("echo before-the-stop; exec sleep 30"),
        &scratch.path,
        cancel.clone(),
    );

    let first = pieces.recv_timeout(PIECE).expect("what it printed");
    assert_eq!(first, "before-the-stop\n");
    cancel.cancel();

    let outcome = worker.join().expect("the worker ends");
    assert!(
        matches!(outcome, Err(ShellError::Cancelled)),
        "expected a cancellation, got {outcome:?}"
    );
}
