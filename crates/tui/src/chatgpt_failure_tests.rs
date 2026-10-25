//! What the terminal client does with a ChatGPT turn that failed: the failure entry it adds to the
//! transcript, the reason its record keeps in place of the one a record composes, the screen it
//! draws, the transcript it exports, the note its picker says of the account's listing and the files
//! it writes, held to every value the run that failed handled
//! ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
//!
//! # Why the run is made in a child
//!
//! The failures are the real ones: a sign-in and four turns against the stand-in of
//! `crates/agent/test-support/chatgpt_stand_in.rs`, made by the account's own renewer and client.
//! The program reads the stand-in's address from its environment and its state directory from the
//! environment too, and the function that takes a turn's end into the session, `fold_outcome`, is
//! private to this crate, so only a unit test can call it. The unit tests of this crate share one
//! process, and this crate forbids `unsafe`, so the test that holds the outputs starts this binary
//! again to run one test with the environment set. That test makes the run and calls the real
//! functions with its failures, and writes what each produced to a file. The test that started it
//! holds every one of those files, every file the session wrote and what the child said on its
//! terminal to every value of the run, which only it knows: the stand-in is its own.

use super::*;
use bravebot_session::audit::Trail;
use bravebot_session::sessions::{self, Front, Handle};

#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
use chatgpt_stand_in::{
    ACCESS_TWO, MODEL, NOT_ELIGIBLE, REFRESH_ONE, REFRESH_TWO, Refresh, STAND_IN, Serving, StandIn,
    USAGE_LIMIT, files_under,
};

/// The variables that tell the child what to do. The child does nothing without them, so running
/// every ignored test of this crate starts nothing.
const OUT: &str = "BRAVEBOT_CHATGPT_TUI_TEST_OUT";
const WORK: &str = "BRAVEBOT_CHATGPT_TUI_TEST_WORK";
const URL_FILE: &str = "BRAVEBOT_CHATGPT_TUI_TEST_URL";

/// The screen the session draws, as the rows a terminal would show.
fn screen(session: &mut Session) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 50))
        .expect("a test terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| {
            laid = render::draw(frame, session);
        })
        .expect("a frame");
    session.note_layout(laid);
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(140)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A stream that keeps what is written to it, for a terminal that writes to memory.
#[derive(Clone, Default)]
struct Written(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Written {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The bytes the session's frame is written to a terminal as, which are the cells of the frame with
/// the sequences that move the cursor and set the colours between them.
fn bytes_written(session: &mut Session) -> String {
    let written = Written::default();
    // A fixed viewport, so that no terminal is asked for its size: there is none here.
    let mut terminal = ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(written.clone()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 140, 50)),
        },
    )
    .expect("a terminal that writes to memory");
    terminal
        .draw(|frame| {
            let _ = render::draw(frame, session);
        })
        .expect("a frame");
    let bytes = written.0.lock().expect("the bytes").clone();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A configuration whose model in force is a model of the account, with no gateway.
fn config() -> bravebot_config::Config {
    let mut config = bravebot_config::Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        // Nothing is asked of this: the turns run on the account.
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9".into()),
        _ => None,
    })
    .expect("config");
    config.default_model = MODEL.to_string();
    config
}

/// What a turn that failed left, each as the output it is.
struct Failed {
    error: bravebot_agent::TurnError,
    outputs: Vec<(&'static str, String)>,
    /// What the turn's reporter sent the thread that owns the terminal, in the order it sent it.
    sent: Vec<crate::remote_confirm::ToMain>,
}

/// One turn on the account in `workspace`, recorded in `trail`, which fails: the error, what the
/// turn reported to the main thread and what it holds of the run as text.
///
/// The turn reports through the terminal's own [`crate::remote_confirm::RemoteReporter`], as the one
/// a turn in the session runs with does, over a channel this end reads when the turn is over.
fn a_failing_turn(
    workspace: &Workspace,
    conversation: &mut Conversation,
    trail: &mut Trail,
) -> Failed {
    let (to_main, from_worker) = std::sync::mpsc::channel();
    let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
    let completed = turn::resume(
        &config(),
        &bravebot_net::Egress::new(),
        workspace,
        &turn::Task::new("a prompt that is not a secret"),
        conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut reporter,
        trail,
        bravebot_agent::workspace::trust_store(workspace.root()),
        TrustedPrograms::new(),
        None,
        &bravebot_core::cancel::Cancel::new(),
    );
    let error = completed
        .outcome
        .expect_err("the turn fails: the stand-in turns the request away");
    drop(reporter);
    let sent: Vec<crate::remote_confirm::ToMain> = from_worker.try_iter().collect();
    Failed {
        outputs: vec![
            ("the error message", error.to_string()),
            ("the error as debugged", format!("{error:?}")),
            ("how the turn ended", format!("{:?}", error.ending())),
            ("what the turn sent the main thread", format!("{sent:?}")),
            (
                "the conversation, which is the transcript and the session record",
                serde_json::to_string(&conversation.snapshot()).expect("a snapshot"),
            ),
        ],
        error,
        sent,
    }
}

/// What the loop that waits for a turn does with a message that needs no answer, which is the
/// function that loop calls. That loop needs a terminal and is not run here. The turn asks nothing,
/// so a message that asks is a failure of this test.
fn take_into(session: &mut Session, message: crate::remote_confirm::ToMain, config: &Config) {
    if let Some(question) = crate::from_worker::show_in_a_turn(
        session,
        message,
        config.context_budget,
        config.budget_is_guessed(),
    ) {
        panic!("a turn that asks nothing sent a question: {question:?}");
    }
}

/// Write one output of the run where the test that started this one reads it. Nothing is written
/// that is empty, so an output with nothing in it is a failure here and not a pass there.
fn hand_over(out: &std::path::Path, name: &str, text: &str) {
    assert!(!text.is_empty(), "{name} is empty, so nothing was held");
    let file: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    std::fs::write(out.join(format!("{file}.txt")), text).expect("an output is written");
}

/// Run by [`what_the_terminal_client_keeps_and_draws_of_a_failed_turn_holds_none_of_the_values_of_the_run`]
/// in a process of its own.
///
/// It signs in and takes four turns that fail, and gives each failure to `fold_outcome` as a finished
/// turn is given. It records the turn and writes the record and its trail as the session does. After
/// each turn it writes the transcript, the markdown export, the screen, the bytes the screen is written
/// to a terminal as, the turns the record keeps, the file `/export` writes and what the failed turn
/// holds to a file of its own, and after the second it opens the picker twice, for a listing that
/// cannot be read and for one the service refuses in a code that names the account. It also requires
/// that the record's reason is the failure the transcript shows, and that each of the four turns is a
/// different one.
#[test]
#[ignore = "run as a child process by the test that holds what it keeps to the values it handled"]
fn a_child_that_makes_the_run_and_folds_each_failure() {
    let (Some(out), Some(work), Some(url_file)) = (
        std::env::var_os(OUT),
        std::env::var_os(WORK),
        std::env::var_os(URL_FILE),
    ) else {
        return;
    };
    let (out, work) = (
        std::path::PathBuf::from(out),
        std::path::PathBuf::from(work),
    );
    let account = bravebot_skus::chatgpt::path().expect("the state directory names a file");
    let state = account.parent().expect("a state directory").to_path_buf();

    // The sign-in, as the command makes it: the listener, the address and the browser, which here
    // writes the address where the test that started this reads it and returns to the listener.
    let signed_in = bravebot_skus::chatgpt::sign_in(
        &bravebot_skus::chatgpt::Hosts::compiled_in(),
        bravebot_net::Transport::shared().agent_config_builder(),
        &bravebot_skus::chatgpt::Limits {
            callback: std::time::Duration::from_secs(30),
        },
        &bravebot_skus::chatgpt::Pages {
            done: "You can close this tab.",
            not_found: "Not found.",
        },
        |announcement| {
            std::fs::write(&url_file, announcement.url).expect("the address is written");
            chatgpt_stand_in::browse(announcement.url);
        },
    );
    signed_in.expect("the sign-in");

    let workspace = Workspace::new(&work).expect("workspace");
    let trust = bravebot_agent::workspace::trust_store(workspace.root());
    let programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let mut stored = Handle::begin(workspace.root(), Front::Terminal, bravebot_stamp::BUILD);
    let mut reasons = Vec::new();
    let config = config();
    for turn in 1..=4 {
        session.paste("a prompt that is not a secret");
        let prompt = session.submit().expect("a prompt");
        let start = conversation.recounted().len();

        let mut trail = Trail::new();
        let failed = a_failing_turn(&workspace, &mut conversation, &mut trail);
        let events = trail.events().to_vec();
        let (error, outputs) = (failed.error, failed.outputs);
        // What the turn's reporter sent, taken into the session as the loop that waits for the turn
        // takes it. The prompt is recorded by the turn's own message and the request it built is the
        // one the screen shows, so each is required to be among them.
        use crate::remote_confirm::ToMain;
        assert!(
            failed
                .sent
                .iter()
                .any(|message| matches!(message, ToMain::PromptRecorded(at) if *at == start)),
            "turn {turn}: the turn did not report the prompt as recorded at {start}: {:?}",
            failed.sent
        );
        assert!(
            failed
                .sent
                .iter()
                .any(|message| matches!(message, ToMain::RequestBuilt(_))),
            "turn {turn}: the turn did not report the request it built: {:?}",
            failed.sent
        );
        for message in failed.sent {
            take_into(&mut session, message, &config);
        }
        // The finished turn is taken into the session as the loop takes one.
        fold_outcome(
            &mut session,
            Err(error),
            trail,
            Carried {
                trust: TrustStore::new(workspace.root()),
                programs: TrustedPrograms::new(),
                asked: AskedAbout::new(),
                exposed: bravebot_core::credentials::Exposed::new(),
            },
            Occupied {
                budget: config.context_budget,
                guessed: config.budget_is_guessed(),
                last_request_tokens: 0,
            },
            Asked {
                name: MODEL.to_string(),
                comparable: true,
            },
            Line {
                text: "",
                addressed: None,
                offered_a_later_look: false,
            },
            &workspace,
        );
        session.record_turn(start, &conversation);
        with_standing(
            &session,
            &conversation,
            &trust,
            &programs,
            &workspace,
            |standing| stored.save(&prompt, standing),
        );
        stored.append_audit(session.turns, &events);

        // What the session holds of the turn that failed.
        let history = serde_json::to_string(session.turn_history()).expect("the turns");
        let transcript = session
            .transcript
            .iter()
            .map(|entry| format!("{:?}: {}", entry.speaker, entry.text))
            .collect::<Vec<_>>()
            .join("\n");
        let failure = session
            .transcript
            .iter()
            .rfind(|entry| entry.speaker == crate::state::Speaker::Failure)
            .unwrap_or_else(|| panic!("turn {turn} left no failure in the transcript"))
            .text
            .clone();
        let reason = match session
            .turn_history()
            .last()
            .and_then(|turn| turn.outcome.as_ref())
        {
            Some(sessions::StoredOutcome::Failed { reason }) => reason.clone(),
            other => panic!("turn {turn} was not recorded as failed: {other:?}"),
        };
        assert_eq!(
            reason, failure,
            "the record's reason is the failure the transcript shows"
        );
        reasons.push(reason);

        let bytes = bytes_written(&mut session);
        assert!(
            bytes.contains(failure.lines().next().unwrap_or_default()),
            "the bytes written to the terminal do not hold the failure the screen shows"
        );
        for (name, text) in [
            ("the transcript", transcript),
            (
                "the transcript exported",
                render::as_markdown(&session, "a title"),
            ),
            ("the screen", screen(&mut session)),
            ("the bytes written to the terminal", bytes),
            ("the turns the record keeps", history),
        ] {
            hand_over(&out, &format!("turn {turn}: {name}"), &text);
        }
        for (name, text) in outputs {
            hand_over(&out, &format!("turn {turn}: {name}"), &text);
        }
        // `/export` writes the transcript to a file in the working directory, and says where.
        let exported = format!("export-turn-{turn}.md");
        session_commands::export_session(&mut session, &stored, workspace.root(), Some(&exported));
        hand_over(
            &out,
            &format!("turn {turn}: the file /export wrote"),
            &std::fs::read_to_string(work.join(&exported))
                .unwrap_or_else(|why| panic!("/export wrote no {exported}: {why}")),
        );

        // The picker is opened while the sign-in is stored and the token is good, once when the
        // serving host cannot answer the listing and once when it refuses it in a code that names the
        // account. The note it says of each goes in the transcript, as the picker puts it there.
        if turn == 2 {
            for asked in ["first", "second"] {
                let answered = bravebot_agent::chatgpt::account_models(&config, &mut Trail::new());
                let problem = models::account_problem_said(&answered)
                    .unwrap_or_else(|| panic!("the picker said nothing of the {asked} listing"));
                session.note(problem.clone());
                hand_over(&out, &format!("the {asked} picker's note"), &problem);
                hand_over(
                    &out,
                    &format!("the transcript after the {asked} picker's note"),
                    &session
                        .transcript
                        .iter()
                        .map(|entry| entry.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                hand_over(
                    &out,
                    &format!("the screen after the {asked} picker's note"),
                    &screen(&mut session),
                );
            }
        }
    }
    for pair in [(0, 1), (1, 2), (2, 3), (0, 2), (0, 3), (1, 3)] {
        assert_ne!(
            reasons[pair.0],
            reasons[pair.1],
            "turns {} and {} failed alike: {reasons:?}",
            pair.0 + 1,
            pair.1 + 1
        );
    }
    // The record and the audit file are among what was held.
    let files = files_under(&state);
    assert!(
        files.iter().any(|(path, _)| path
            .display()
            .to_string()
            .ends_with(&format!("{}.json", stored.id()))),
        "no session record was written"
    );
    assert!(
        files
            .iter()
            .any(|(path, _)| path.display().to_string().ends_with(".audit.jsonl")),
        "no audit file was written"
    );
    assert!(!account.exists(), "the sign-in that ended was not removed");
}

/// A turn that fails is taken into the terminal client by `fold_outcome`, which adds the failure to
/// the transcript, and by `record_turn`, which keeps that entry's text as the reason of the turn in
/// the record. None of what the run handled is in the transcript, the markdown export of it, the
/// screen and the bytes written for it, the turns the record keeps, any file the session wrote, the
/// note the picker says of the account's listing or anything the failed turn holds, for the four
/// failures of a real run: a renewal while the token endpoint is busy, a refusal that is not of the
/// token, a refusal in a code that names the account, and a sign-in that has ended.
///
/// The child's own terminal is held as well, whatever the functions it called wrote to standard
/// output or standard error. Every one of the outputs is required to be there and to hold something,
/// so a run that wrote none of them fails and does not pass.
///
/// The regressions it rejects are a failure entry composed from more than the diagnosis, such as the
/// text of the error or a value the turn handled, a record that keeps what the transcript shows when
/// that is more than its reason, and a screen or an export that adds a value to what the transcript
/// holds.
#[test]
fn what_the_terminal_client_keeps_and_draws_of_a_failed_turn_holds_none_of_the_values_of_the_run() {
    let scratch = crate::testutil::scratch_dir("bravebot-tui-chatgpt-failure");
    let _ = std::fs::remove_dir_all(&scratch);
    let (out, state, work, home) = (
        scratch.join("out"),
        scratch.join("state"),
        scratch.join("work"),
        scratch.join("home"),
    );
    for directory in [&out, &state, &work, &home] {
        std::fs::create_dir_all(directory).expect("a directory");
    }
    let url_file = scratch.join("address.txt");

    // The stand-in: the first token lasts thirty seconds, so the first turn renews it. The serving
    // host turns the renewed token away and the replacement is asked for while the token endpoint is
    // busy; the next turn is refused for something that is not the token; the next is refused in a
    // code that names the account; the last turns the token away and the token endpoint says the
    // refresh token is no longer valid.
    let stand_in = StandIn::new(vec![]);
    {
        let mut state = stand_in.state();
        state.url_file = Some(url_file.clone());
        state.first_lifetime = 30;
        state.refreshing.extend([
            Refresh::Grants(ACCESS_TWO, REFRESH_TWO),
            Refresh::Busy,
            Refresh::Ended,
        ]);
        state.serving.extend([
            Serving::RefusesTheToken,
            Serving::Refuses(400),
            Serving::RefusesWithCode(429, USAGE_LIMIT),
            Serving::RefusesTheToken,
        ]);
        state.listing.extend([
            Serving::Refuses(404),
            Serving::RefusesWithCode(401, NOT_ELIGIBLE),
        ]);
    }

    let child = chatgpt_stand_in::in_a_process_of_its_own(
        "app::chatgpt_failure_tests::a_child_that_makes_the_run_and_folds_each_failure",
        &[
            (STAND_IN, std::ffi::OsStr::new(&stand_in.base)),
            (OUT, out.as_os_str()),
            (WORK, work.as_os_str()),
            (URL_FILE, url_file.as_os_str()),
            ("BRAVEBOT_HOME", state.as_os_str()),
            ("HOME", home.as_os_str()),
        ],
    );
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    assert!(
        child.status.success(),
        "the child failed: {}{}",
        said(&child.stdout),
        said(&child.stderr)
    );

    // The run is the one the stand-in was scripted for: every refresh was asked for and every turn
    // and listing answered, so each failure is the one it was meant to be.
    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO, REFRESH_TWO],
        "the refreshes the run made"
    );
    {
        let state = stand_in.state();
        assert!(
            state.serving.is_empty() && state.listing.is_empty() && state.refreshing.is_empty(),
            "the run did not use every scripted answer"
        );
    }

    let handled = stand_in.handled();
    handled.assert_distinct();
    assert!(
        handled.values().len() >= 19,
        "the run made few values: {:?}",
        handled.values()
    );
    let held: Vec<(String, String)> = files_under(&out)
        .into_iter()
        .map(|(path, bytes)| {
            (
                path.display().to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
            )
        })
        .collect();
    // Four turns, each with its eleven outputs, and the picker's three outputs twice.
    assert_eq!(
        held.len(),
        4 * 11 + 2 * 3,
        "{:?}",
        held.iter().map(|h| &h.0).collect::<Vec<_>>()
    );
    for (path, text) in &held {
        handled.assert_none_in(path, text);
    }
    // Every file the session wrote but the sign-in itself, which is where the tokens are kept, in the
    // state directory and in the working directory, where `/export` writes. A copy of the sign-in,
    // or a temporary file beside it, is not it.
    let mut files = 0;
    for (path, bytes) in files_under(&state).into_iter().chain(files_under(&work)) {
        if path
            .file_name()
            .is_some_and(|name| name == "chatgpt-account.json")
            || bytes.is_empty()
        {
            continue;
        }
        let path = path.display().to_string();
        files += 1;
        handled.assert_none_in(&path, &String::from_utf8_lossy(&bytes));
    }
    assert!(
        files >= 6,
        "the state and working directories hold {files} files, so the record, its trail and the four exports were not searched"
    );
    handled.assert_none_in("the child's standard output", &said(&child.stdout));
    // Nothing was said on standard error, which holds nothing of the run.
    assert_eq!(handled.found_in(&said(&child.stderr)), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&scratch);
}
