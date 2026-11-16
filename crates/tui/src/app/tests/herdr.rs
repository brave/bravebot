//! HERDR-8, HERDR-9 and HERDR-12 through the interface's own calls: the run tells herdr it holds
//! the pane once its settings are read, hands over how to reopen the session on each pass, and
//! tells herdr it has left the pane when it leaves for good, and not when it only hands the
//! terminal over for a while.

use super::*;
use crate::herdr::script;
use crate::title::{State, Title};
use bravebot_herdr::Reporter;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const ARRIVES_WITHIN: Duration = Duration::from_secs(10);

/// Long enough for a request that was going to be sent to have arrived.
const NOTHING_ARRIVES_WITHIN: Duration = Duration::from_millis(300);

/// A herdr that records each request it is sent, and answers it or never does.
struct FakeHerdr {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    requests: Receiver<Value>,
}

impl FakeHerdr {
    fn start(answers: bool) -> FakeHerdr {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (recorded, requests) = channel();
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                if answers {
                    let _ = writeln!(stream, r#"{{"id":{},"result":{{}}}}"#, request["id"]);
                } else {
                    kept.push(stream);
                }
                let _ = recorded.send(request);
            }
        });
        FakeHerdr {
            _directory: directory,
            socket,
            requests,
        }
    }

    fn reporter(&self) -> Reporter {
        Reporter::from_variables(inside_herdr(&self.socket))
    }

    fn next_request(&self) -> Value {
        self.requests
            .recv_timeout(ARRIVES_WITHIN)
            .expect("herdr was sent a request")
    }

    fn next(&self) -> String {
        self.next_request()["method"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn nothing_more(&self) {
        if let Ok(request) = self.requests.recv_timeout(NOTHING_ARRIVES_WITHIN) {
            panic!("herdr was sent {}", request["method"]);
        }
    }
}

fn inside_herdr(socket: &Path) -> impl Fn(&str) -> Option<OsString> + use<> {
    let socket = socket.as_os_str().to_owned();
    move |name| match name {
        "HERDR_ENV" => Some("1".into()),
        "HERDR_PANE_ID" => Some("w1:p1".into()),
        "HERDR_SOCKET_PATH" => Some(socket.clone()),
        _ => None,
    }
}

/// A run that has reported, as every run in a herdr pane has by its first frame.
fn reported(herdr: &FakeHerdr) {
    Title::new().mark(&mut Vec::new(), State::Working).unwrap();
    assert_eq!(herdr.next(), "pane.report_agent");
}

/// HERDR-12. Rejects: leaving the interface without telling herdr, which leaves the pane marked as
/// bravebot, with its last state and resume command, until herdr notices the shell prompt; and
/// telling it only when the terminal could be handed back, so a run whose way out failed is left
/// behind in herdr as well as on the screen.
#[test]
fn leaving_for_good_tells_herdr_the_run_has_left_its_pane_however_the_way_out_went() {
    for handed_back in [Ok(()), Err(io::Error::other("gone"))] {
        let herdr = FakeHerdr::start(true);
        let _held = script::hold(herdr.reporter());
        reported(&herdr);
        let failed = handed_back.is_err();
        let left = leave(&mut Vec::new(), |_: &mut Vec<u8>| handed_back);
        assert_eq!(left.is_err(), failed, "the way out's own result comes back");
        assert_eq!(herdr.next(), "pane.release_agent", "failed: {failed}");
    }
}

/// HERDR-8, through `adopt_settings`, which `begin_session` calls once the settings are read and
/// before the first question it can ask. Rejects: a session that first reaches herdr with its first
/// frame, after the startup questions, so herdr shows the pane as a plain shell while they wait.
#[test]
fn adopting_the_settings_tells_herdr_the_session_holds_its_pane_idle() {
    let herdr = FakeHerdr::start(true);
    let _held = script::hold(herdr.reporter());
    adopt_settings(
        &mut Session::new("none"),
        &bravebot_config::Settings::default(),
        None,
    );
    let request = herdr.next_request();
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "idle");
}

/// HERDR-9, through `name_the_id`, which the event loop calls on every pass. Rejects: a loop that
/// never hands herdr the session's id, so herdr has nothing to reopen after a restart; and one that
/// hands it over before the record `--resume` reads is on disk.
#[test]
fn each_pass_of_the_loop_hands_herdr_the_id_once_the_record_is_written() {
    use crate::herdr::{Modes, resume_script};
    use bravebot_herdr::State as Shown;
    use bravebot_session::sessions;
    use std::rc::Rc;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-herdr-resume");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let herdr = FakeHerdr::start(true);
    let reporter = Rc::new(herdr.reporter());
    let _held = resume_script::hold(Rc::clone(&reporter), Modes::default());
    reporter.report(Shown::Idle, None);
    assert_eq!(herdr.next(), "pane.report_agent");
    let mut session = Session::new("none");
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);

    name_the_id(&mut session, &stored);
    herdr.nothing_more();

    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    name_the_id(&mut session, &stored);
    let request = herdr.next_request();
    assert_eq!(request["params"]["agent_session_id"], stored.id());
    assert_eq!(
        request["params"]["resume_argv"],
        serde_json::json!(["bravebot", "--resume", stored.id()])
    );
}

/// PANEL-9, through `open_the_record`, which sending a first prompt calls before the turn runs.
/// Rejects: a first turn that leaves the session with no record until it ends, so the panel has no
/// id to show for as long as the turn runs; a second write for a record already on disk; and a
/// record written before any prompt is sent.
#[test]
fn sending_the_first_prompt_gives_the_panel_an_id_before_the_turn_ends() {
    use bravebot_session::sessions;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-first-prompt-record");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    session.toggle_panel();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    assert_eq!(
        stored.resumable(),
        None,
        "the session had an id before any prompt was sent"
    );

    let conversation = Conversation::new();
    let trust = TrustStore::new(&root);
    let programs = TrustedPrograms::new();
    let open = |session: &mut Session, stored: &mut sessions::Handle| {
        open_the_record(
            session,
            &conversation,
            stored,
            &trust,
            &programs,
            &workspace,
            "fix the gate",
        )
    };
    assert!(open(&mut session, &mut stored), "no record was written");
    assert_eq!(stored.title(), "fix the gate");
    assert_eq!(stored.resumable(), Some(stored.id()));
    // The turn's loop draws until the turn ends and names neither, so a panel with an id and a
    // name still to be given would show neither for as long as the turn runs.
    assert!(
        !name_the_id(&mut session, &stored),
        "the panel was not given the id"
    );
    assert!(
        !session.identify(
            stored.title(),
            crate::status::abbreviate(workspace.root()),
            stored.branch()
        ),
        "the panel was not given the name"
    );
    assert!(
        !open(&mut session, &mut stored),
        "a record already on disk was written again"
    );
}

/// HERDR-12, through the end of `run`. Rejects: a run that ends by handing the terminal back the
/// way an editor or a stop does, which is what it did before it knew about herdr, and so never
/// tells herdr it has gone.
#[test]
fn the_end_of_a_run_tells_herdr_the_run_has_left_its_pane() {
    use ratatui::layout::Rect;
    use ratatui::{TerminalOptions, Viewport};
    let herdr = FakeHerdr::start(true);
    let _held = script::hold(herdr.reporter());
    reported(&herdr);
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(Vec::new()),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 20, 5)),
        },
    )
    .unwrap();
    // `hand_back` is given the two things `hand_back_terminal` reads from the terminal the test binary
    // runs in: whether it has enhanced keys, and how to leave raw mode. Either stops a process that
    // is not in that terminal's foreground group, and a stopped test never finishes.
    end_the_run(&mut terminal, |backend| {
        hand_back(backend, false, || Ok(()))
    })
    .unwrap();
    assert_eq!(herdr.next(), "pane.release_agent");
}

/// HERDR-12. Rejects: releasing on the way the terminal is handed to an editor, a viewer or the
/// shell for a stop, which every one of them comes back from. herdr would show the pane as a plain
/// terminal while the person is in their editor, and refuse the run's next report as one from an
/// agent that left.
#[test]
fn handing_the_terminal_over_for_a_while_leaves_the_run_in_its_pane() {
    let herdr = FakeHerdr::start(true);
    let _held = script::hold(herdr.reporter());
    reported(&herdr);
    hand_back(&mut Vec::new(), false, || Ok(())).unwrap();
    herdr.nothing_more();
}

/// HERDR-12, with HERDR-7. Rejects: a run that keeps reporting after it has left, as the frames
/// drawn on the way out would: the pane would be marked as a session no longer in it.
#[test]
fn nothing_the_title_marks_after_leaving_reaches_herdr() {
    let herdr = FakeHerdr::start(true);
    let _held = script::hold(herdr.reporter());
    reported(&herdr);
    leave(&mut Vec::new(), |_: &mut Vec<u8>| Ok(())).unwrap();
    assert_eq!(herdr.next(), "pane.release_agent");
    Title::new().mark(&mut Vec::new(), State::Waiting).unwrap();
    herdr.nothing_more();
}

/// HERDR-12. Rejects: a way out that waits on a herdr that never answers for longer than the
/// quarter of a second a release may take, which the person sees as bravebot not quitting.
#[test]
fn a_herdr_that_never_answers_holds_the_way_out_up_only_briefly() {
    let herdr = FakeHerdr::start(false);
    let _held = script::hold(herdr.reporter());
    reported(&herdr);
    let began = Instant::now();
    leave(&mut Vec::new(), |_: &mut Vec<u8>| Ok(())).unwrap();
    let took = began.elapsed();
    assert_eq!(herdr.next(), "pane.release_agent");
    assert!(took < Duration::from_secs(1), "the way out took {took:?}");
}
