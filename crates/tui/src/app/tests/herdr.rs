//! HERDR-12: the run tells herdr it has left the pane when it leaves for good, and not when it
//! only hands the terminal over for a while.

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

/// A herdr that records the method of each request it is sent, and answers it or never does.
struct FakeHerdr {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    methods: Receiver<String>,
}

impl FakeHerdr {
    fn start(answers: bool) -> FakeHerdr {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (recorded, methods) = channel();
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
                let method = request["method"].as_str().unwrap_or_default().to_string();
                let _ = recorded.send(method);
            }
        });
        FakeHerdr {
            _directory: directory,
            socket,
            methods,
        }
    }

    fn reporter(&self) -> Reporter {
        Reporter::from_variables(inside_herdr(&self.socket))
    }

    fn next(&self) -> String {
        self.methods
            .recv_timeout(ARRIVES_WITHIN)
            .expect("herdr was sent a request")
    }

    fn nothing_more(&self) {
        if let Ok(method) = self.methods.recv_timeout(NOTHING_ARRIVES_WITHIN) {
            panic!("herdr was sent {method}");
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
