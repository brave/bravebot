//! What the session is doing, told to herdr when the run is inside a herdr pane
//! ([HERDR-8](../../../docs/specs/herdr.md#HERDR-8)).
//!
//! herdr is told the state the title tracks, from the same call, so the two cannot disagree; but it
//! is told whether or not a title is written, since a report carries no name. Outside herdr the
//! reporter is inactive and nothing here does anything.

use std::sync::OnceLock;

use bravebot_herdr::{Reporter, State as Shown};
use bravebot_i18n::t;

use crate::title::State;

/// What herdr is told for `state`. A question carries the fixed sentence herdr shows beside it,
/// which holds nothing the question is about.
fn shown(state: State) -> (Shown, Option<String>) {
    match state {
        State::Ready => (Shown::Idle, None),
        State::Working => (Shown::Working, None),
        State::Waiting => (Shown::Blocked, Some(t!(herdr_waiting).to_string())),
    }
}

/// The run's reporter, for the pane this process was started in. One per process, as the title is.
pub(crate) fn reporter() -> &'static Reporter {
    static REPORTER: OnceLock<Reporter> = OnceLock::new();
    REPORTER.get_or_init(ambient)
}

#[cfg(not(test))]
fn ambient() -> Reporter {
    Reporter::from_env()
}

/// A test run inside a herdr pane has that pane's variables, and must not report to it.
#[cfg(test)]
fn ambient() -> Reporter {
    Reporter::from_variables(|_| None)
}

/// Tell herdr the session is in `state`.
pub(crate) fn state(state: State) {
    #[cfg(test)]
    if script::told(state) {
        return;
    }
    tell(reporter(), state);
}

fn tell(reporter: &Reporter, state: State) {
    let (shown, message) = shown(state);
    reporter.report(shown, message);
}

/// A reporter a test holds in place of the run's, for one thread, so tests that drive the title
/// side by side each see only their own reports.
#[cfg(test)]
pub(crate) mod script {
    use super::{Reporter, State};
    use std::cell::RefCell;

    thread_local! {
        static REPORTER: RefCell<Option<Reporter>> = const { RefCell::new(None) };
    }

    /// The reporter, until it is dropped.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            REPORTER.with(|held| *held.borrow_mut() = None);
        }
    }

    /// Make `reporter` the one this thread tells.
    pub(crate) fn hold(reporter: Reporter) -> Held {
        REPORTER.with(|held| *held.borrow_mut() = Some(reporter));
        Held
    }

    /// Tell the held reporter, if one is held. Returns whether one was.
    pub(super) fn told(state: State) -> bool {
        REPORTER.with(|held| {
            held.borrow()
                .as_ref()
                .map(|reporter| super::tell(reporter, state))
                .is_some()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::title::Title;
    use serde_json::Value;
    use std::ffi::OsString;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{Receiver, channel};
    use std::time::Duration;

    const ARRIVES_WITHIN: Duration = Duration::from_secs(10);

    /// A herdr that records the parameters of each request it is sent and answers it.
    struct FakeHerdr {
        _directory: tempfile::TempDir,
        socket: PathBuf,
        requests: Receiver<Value>,
    }

    impl FakeHerdr {
        fn start() -> FakeHerdr {
            let directory = tempfile::tempdir().unwrap();
            let socket = directory.path().join("herdr.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let (recorded, requests) = channel();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        continue;
                    }
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let _ = writeln!(stream, r#"{{"id":{},"result":{{}}}}"#, request["id"]);
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

        /// The state and message of the next report.
        fn next(&self) -> (String, Option<String>) {
            let request = self
                .requests
                .recv_timeout(ARRIVES_WITHIN)
                .expect("herdr was sent a report");
            assert_eq!(request["method"], "pane.report_agent");
            let params = &request["params"];
            (
                params["state"].as_str().unwrap().to_string(),
                params["message"].as_str().map(str::to_string),
            )
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

    fn mark(title: &mut Title, state: State) -> Vec<u8> {
        let mut out = Vec::new();
        title.mark(&mut out, state).expect("mark");
        out
    }

    /// herdr marks a pane working, blocked or idle by what it is told, and waits and notifies on
    /// it, so each state the title tracks must reach it as the one herdr means by it: a question
    /// shown as working would never notify, and a turn shown as idle would end an `agent wait`.
    #[test]
    fn each_state_the_title_tracks_reaches_herdr_as_its_own() {
        let herdr = FakeHerdr::start();
        let _held = script::hold(herdr.reporter());
        let mut title = Title::new();
        for (state, expected) in [
            (State::Working, "working"),
            (State::Waiting, "blocked"),
            (State::Working, "working"),
            (State::Ready, "idle"),
        ] {
            mark(&mut title, state);
            assert_eq!(herdr.next().0, expected, "{state:?}");
        }
    }

    /// What herdr shows beside a blocked session is the fixed sentence, and an idle or working one
    /// carries none, so nothing a question is about leaves the session.
    #[test]
    fn only_a_question_carries_a_message_and_it_is_the_fixed_one() {
        let herdr = FakeHerdr::start();
        let _held = script::hold(herdr.reporter());
        let mut title = Title::new();
        mark(&mut title, State::Working);
        assert_eq!(herdr.next().1, None);
        mark(&mut title, State::Waiting);
        assert_eq!(herdr.next().1.as_deref(), Some(t!(herdr_waiting).as_ref()));
        mark(&mut title, State::Ready);
        assert_eq!(herdr.next().1, None);
    }

    /// `terminalTitle = false` and an incognito session turn the title off, and neither is a reason
    /// to hide the session from herdr: a report names no session.
    #[test]
    fn a_title_turned_off_still_tells_herdr() {
        let herdr = FakeHerdr::start();
        let _held = script::hold(herdr.reporter());
        let mut title = Title::new();
        title.adopt(Some(false), false);
        assert!(mark(&mut title, State::Working).is_empty());
        assert_eq!(herdr.next().0, "working");
        let mut incognito = Title::new();
        incognito.adopt(None, true);
        assert!(mark(&mut incognito, State::Waiting).is_empty());
        assert_eq!(herdr.next().0, "blocked");
    }
}
