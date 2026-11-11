//! What the session is doing, told to herdr when the run is inside a herdr pane
//! ([HERDR-8](../../../docs/specs/herdr.md#HERDR-8)).
//!
//! herdr is told the state the title tracks, from the same call, so the two cannot disagree; but it
//! is told whether or not a title is written, since a report carries no name. Outside herdr the
//! reporter is inactive and nothing here does anything.
//!
//! The command that reopens the session is told from the same reporter, below the states
//! ([HERDR-9](../../../docs/specs/herdr.md#HERDR-9)).

use std::sync::OnceLock;

use bravebot_herdr::{Reporter, Resume, State as Shown};
use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;
use bravebot_sandbox::network::Network;

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

// The command that reopens the session (HERDR-9, HERDR-10, HERDR-11).

/// What `--safe` or `--locked` left the run loading. `--locked` includes `--safe`, so it is the
/// stricter and the one kept when both are given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Strictness {
    #[default]
    Usual,
    Safe,
    Locked,
}

/// The process-wide modes the resume command is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Modes {
    pub(crate) incognito: bool,
    pub(crate) strictness: Strictness,
    /// The `--sandbox` mode to carry, which is narrower than the run would get without it.
    pub(crate) sandbox: Option<SandboxMode>,
    /// Whether `--run-network closed` is carried, closing what the run would get open without it.
    pub(crate) network_closed: bool,
    /// The paths `--sandbox-deny-read` and `--sandbox-deny-write` named, as typed.
    pub(crate) deny_read: Vec<String>,
    pub(crate) deny_write: Vec<String>,
}

impl Modes {
    /// Read once: the layers and flags it is built from are settled before the interface starts.
    fn of_this_process() -> &'static Modes {
        static MODES: OnceLock<Modes> = OnceLock::new();
        MODES.get_or_init(|| {
            let settings = bravebot_config::Settings::load();
            let managed = bravebot_config::Managed::load();
            let lists = bravebot_config::sandbox_filesystem();
            Modes {
                incognito: bravebot_core::incognito::engaged(),
                strictness: if bravebot_config::locked() {
                    Strictness::Locked
                } else if bravebot_core::safe::engaged() {
                    Strictness::Safe
                } else {
                    Strictness::Usual
                },
                sandbox: narrower_sandbox(
                    &bravebot_config::sandbox::in_force(),
                    bravebot_config::sandbox::resolve(None, &settings, &managed)
                        .ok()
                        .map(|choice| choice.mode),
                ),
                network_closed: closed_by_the_flag(
                    &bravebot_config::settled_run_network().cloned().unwrap_or(
                        bravebot_config::RunNetwork {
                            network: Network::Open,
                            decided: bravebot_config::Decided::Default,
                        },
                    ),
                    bravebot_config::resolve_run_network(None, &settings, &managed).network,
                ),
                deny_read: typed(&lists.deny_read),
                deny_write: typed(&lists.deny_write),
            }
        })
    }
}

/// The paths of `entries` the command line named, which a settings file or the managed file
/// will name again on their own.
pub(crate) fn typed(entries: &[bravebot_sandbox::rules::Entry]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| entry.by.is_none() && !entry.pinned)
        .map(|entry| entry.path.clone())
        .collect()
}

/// The mode `--sandbox` gave the run, where the run would be looser without it and it is looser
/// in nothing. The modes are not a line, so a flag that is looser in reads or writes than what
/// the settings give is not carried, nor one where the settings would be refused without it.
pub(crate) fn narrower_sandbox(
    used: &bravebot_config::sandbox::Choice,
    without: Option<SandboxMode>,
) -> Option<SandboxMode> {
    let without = without?;
    (used.source == bravebot_config::sandbox::Source::Flag
        && without.is_looser_than(used.mode)
        && !used.mode.is_looser_than(without))
    .then_some(used.mode)
}

/// Whether `--run-network` closed a network the run would have had open without it. The network
/// has two settings, so `closed` is the only one that can narrow.
pub(crate) fn closed_by_the_flag(used: &bravebot_config::RunNetwork, without: Network) -> bool {
    used.decided == bravebot_config::Decided::Flag
        && used.network == Network::Closed
        && without == Network::Open
}

/// The command that reopens session `id` under `modes`, or `None` where none may be sent.
///
/// Only the modes that narrow a session and that its record does not keep are carried: a command
/// herdr types with nobody asking must not reopen a session under looser rules than it ran under,
/// nor under looser ones than anybody typed for it.
pub(crate) fn command(id: &str, modes: &Modes) -> Option<Vec<String>> {
    if modes.incognito || id.starts_with('-') {
        return None;
    }
    let mut argv = vec![
        "bravebot".to_string(),
        "--resume".to_string(),
        id.to_string(),
    ];
    match modes.strictness {
        Strictness::Usual => {}
        Strictness::Safe => argv.push("--safe".to_string()),
        Strictness::Locked => argv.push("--locked".to_string()),
    }
    if let Some(mode) = modes.sandbox {
        argv.extend(["--sandbox".to_string(), mode.name().to_string()]);
    }
    if modes.network_closed {
        argv.extend([
            "--run-network".to_string(),
            Network::Closed.name().to_string(),
        ]);
    }
    for path in &modes.deny_read {
        argv.extend(["--sandbox-deny-read".to_string(), path.clone()]);
    }
    for path in &modes.deny_write {
        argv.extend(["--sandbox-deny-write".to_string(), path.clone()]);
    }
    Some(argv)
}

/// Tell herdr how to reopen the session whose record is `id`, or nothing where it has none yet.
/// Called on every pass of the event loop, so a session that changes is told on the next one.
pub(crate) fn resume(id: Option<&str>) {
    let Some(id) = id else { return };
    #[cfg(test)]
    if resume_script::told(id) {
        return;
    }
    if reporter().is_active() {
        offer(reporter(), id, Modes::of_this_process());
    }
}

fn offer(reporter: &Reporter, id: &str, modes: &Modes) {
    if let Some(argv) = command(id, modes)
        && let Ok(resume) = Resume::new(id, argv)
    {
        reporter.resume_with(resume);
    }
}

/// A reporter and modes a test holds in place of the run's for [`resume`], for one thread.
#[cfg(test)]
pub(crate) mod resume_script {
    use super::{Modes, Reporter};
    use std::cell::RefCell;
    use std::rc::Rc;

    thread_local! {
        static HELD: RefCell<Option<(Rc<Reporter>, Modes)>> = const { RefCell::new(None) };
    }

    /// The reporter and modes, until it is dropped.
    pub(crate) struct Held;

    impl Drop for Held {
        fn drop(&mut self) {
            HELD.with(|held| *held.borrow_mut() = None);
        }
    }

    /// Make `reporter` the one this thread tells its resume command, built under `modes`.
    pub(crate) fn hold(reporter: Rc<Reporter>, modes: Modes) -> Held {
        HELD.with(|held| *held.borrow_mut() = Some((reporter, modes)));
        Held
    }

    /// Offer the held reporter `id`, if one is held. Returns whether one was.
    pub(super) fn told(id: &str) -> bool {
        HELD.with(|held| {
            held.borrow()
                .as_ref()
                .map(|(reporter, modes)| super::offer(reporter, id, modes))
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
    use std::rc::Rc;
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
            let params = self.params();
            (
                params["state"].as_str().unwrap().to_string(),
                params["message"].as_str().map(str::to_string),
            )
        }

        /// The parameters of the next report.
        fn params(&self) -> Value {
            let request = self
                .requests
                .recv_timeout(ARRIVES_WITHIN)
                .expect("herdr was sent a report");
            assert_eq!(request["method"], "pane.report_agent");
            request["params"].clone()
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

    // The command that reopens the session (HERDR-9, HERDR-10, HERDR-11).

    const ID: &str = "0193f1d2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    /// A herdr this thread's resume commands go to, under `modes`, that has been told one state
    /// already, since a resume command rides on a state report.
    fn reporting(modes: Modes) -> (FakeHerdr, Rc<Reporter>, resume_script::Held) {
        let herdr = FakeHerdr::start();
        let reporter = Rc::new(herdr.reporter());
        let held = resume_script::hold(Rc::clone(&reporter), modes);
        reporter.report(Shown::Idle, None);
        assert_eq!(herdr.next().0, "idle");
        (herdr, reporter, held)
    }

    fn strings(argv: &[&str]) -> Vec<String> {
        argv.iter().map(|arg| arg.to_string()).collect()
    }

    /// HERDR-9. A plain session is reopened by its id and nothing else, under the plain program
    /// name herdr requires. The regressions it rejects are a command naming a path to the binary,
    /// one that resumes by anything but the id, and one carrying a flag the session was not given.
    #[test]
    fn a_plain_session_is_reopened_by_bravebot_resume_and_its_id() {
        let (herdr, _reporter, _held) = reporting(Modes::default());
        resume(Some(ID));
        let params = herdr.params();
        assert_eq!(params["state"], "idle");
        assert_eq!(params["agent_session_id"], ID);
        assert_eq!(
            params["resume_argv"],
            serde_json::json!(["bravebot", "--resume", ID])
        );
    }

    /// HERDR-10. A session run under `--safe` or `--locked` is reopened under it, because its
    /// record does not keep it and a restart nobody typed must not loosen the session. `--locked`
    /// already includes `--safe`, so it is the one carried. The regression it rejects is a
    /// command built from the id alone, which would reopen the session loading what it refused.
    #[test]
    fn a_session_run_safe_or_locked_is_reopened_so() {
        let safe = Modes {
            strictness: Strictness::Safe,
            ..Modes::default()
        };
        let locked = Modes {
            strictness: Strictness::Locked,
            ..Modes::default()
        };
        assert_eq!(
            command(ID, &safe),
            Some(strings(&["bravebot", "--resume", ID, "--safe"]))
        );
        assert_eq!(
            command(ID, &locked),
            Some(strings(&["bravebot", "--resume", ID, "--locked"]))
        );
        let (herdr, _reporter, _held) = reporting(locked);
        resume(Some(ID));
        assert_eq!(
            herdr.params()["resume_argv"],
            serde_json::json!(["bravebot", "--resume", ID, "--locked"])
        );
    }

    /// HERDR-9. An id that would read as a flag is not sent, since `--resume` would take it as
    /// one; nor is a session with no record yet, which `--resume` could not find.
    #[test]
    fn an_id_read_as_a_flag_and_a_session_not_yet_written_send_nothing() {
        assert_eq!(command("-x", &Modes::default()), None);
        assert_eq!(command("--safe", &Modes::default()), None);
        let (herdr, reporter, _held) = reporting(Modes::default());
        resume(Some("--locked"));
        resume(None);
        reporter.report(Shown::Working, None);
        let params = herdr.params();
        assert_eq!(params["state"], "working");
        assert!(params.get("resume_argv").is_none(), "{params}");
    }

    /// HERDR-11. An incognito session sends no resume command, even when it resumed a record: the
    /// command would reopen it as an ordinary session that writes, and its id would be kept in
    /// herdr's own state. The regression it rejects is the command sent whatever the mode.
    #[test]
    fn an_incognito_session_sends_no_resume_command() {
        let incognito = Modes {
            incognito: true,
            ..Modes::default()
        };
        assert_eq!(command(ID, &incognito), None);
        let (herdr, reporter, _held) = reporting(incognito);
        resume(Some(ID));
        reporter.report(Shown::Working, None);
        let params = herdr.params();
        assert_eq!(params["state"], "working");
        assert!(params.get("agent_session_id").is_none(), "{params}");
        assert!(params.get("resume_argv").is_none(), "{params}");
    }

    /// HERDR-9. A session that becomes another, as `/branch` and `/clear` make it, tells herdr the
    /// new id once its record is written, so a restart reopens the session in front of the person
    /// and not the one before it. The regression it rejects is a command told once, for the first
    /// id the process knew.
    #[test]
    fn a_session_that_becomes_another_sends_the_new_id() {
        const NEXT: &str = "0193f1d2-7a3b-7c4d-8e5f-000000000002";
        let (herdr, _reporter, _held) = reporting(Modes::default());
        resume(Some(ID));
        assert_eq!(herdr.params()["agent_session_id"], ID);
        resume(Some(NEXT));
        let params = herdr.params();
        assert_eq!(params["agent_session_id"], NEXT);
        assert_eq!(
            params["resume_argv"],
            serde_json::json!(["bravebot", "--resume", NEXT])
        );
    }

    /// HERDR-10. A `--sandbox` mode is carried where the run would be looser without it and it is
    /// looser in nothing. The regressions it rejects are a narrowing mode dropped, which reopens
    /// the session under the looser one its settings give, and a loosening one carried, which
    /// reopens it under a mode nobody typed for that run; and a comparison by position, which
    /// would take `strict` as narrower than `read-only` when it writes what `read-only` does not.
    #[test]
    fn a_narrowing_sandbox_is_carried_and_a_loosening_one_is_not() {
        use bravebot_config::sandbox::{Choice, Source};
        let flag = |mode| Choice {
            mode,
            source: Source::Flag,
        };
        assert_eq!(
            narrower_sandbox(
                &flag(SandboxMode::ReadOnly),
                Some(SandboxMode::WorkspaceWrite)
            ),
            Some(SandboxMode::ReadOnly)
        );
        assert_eq!(
            narrower_sandbox(&flag(SandboxMode::WorkspaceWrite), Some(SandboxMode::Off)),
            Some(SandboxMode::WorkspaceWrite)
        );
        for (used, without) in [
            (SandboxMode::WorkspaceWrite, SandboxMode::ReadOnly),
            (SandboxMode::Off, SandboxMode::WorkspaceWrite),
            (SandboxMode::ReadOnly, SandboxMode::ReadOnly),
        ] {
            assert_eq!(
                narrower_sandbox(&flag(used), Some(without)),
                None,
                "{used:?}"
            );
        }
        if !cfg!(windows) {
            assert_eq!(
                narrower_sandbox(&flag(SandboxMode::Strict), Some(SandboxMode::ReadOnly)),
                None
            );
        }
        let from_a_file = Choice {
            mode: SandboxMode::ReadOnly,
            source: Source::File(PathBuf::from("settings.json")),
        };
        assert_eq!(
            narrower_sandbox(&from_a_file, Some(SandboxMode::WorkspaceWrite)),
            None
        );
        let modes = Modes {
            sandbox: Some(SandboxMode::ReadOnly),
            ..Modes::default()
        };
        assert_eq!(
            command(ID, &modes),
            Some(strings(&[
                "bravebot",
                "--resume",
                ID,
                "--sandbox",
                "read-only"
            ]))
        );
    }

    /// HERDR-10. `--run-network closed` is carried where the run would have the network open
    /// without it, and `open` never is. The regressions it rejects are a closed network dropped,
    /// which reopens the session with its programs reaching the network, and an open one carried.
    #[test]
    fn a_closed_network_is_carried_and_an_open_one_is_not() {
        use bravebot_config::{Decided, RunNetwork};
        let by = |network, decided| RunNetwork { network, decided };
        assert!(closed_by_the_flag(
            &by(Network::Closed, Decided::Flag),
            Network::Open
        ));
        assert!(!closed_by_the_flag(
            &by(Network::Open, Decided::Flag),
            Network::Closed
        ));
        assert!(!closed_by_the_flag(
            &by(Network::Open, Decided::Flag),
            Network::Open
        ));
        assert!(!closed_by_the_flag(
            &by(Network::Closed, Decided::Flag),
            Network::Closed
        ));
        assert!(!closed_by_the_flag(
            &by(Network::Closed, Decided::Managed(None)),
            Network::Open
        ));
        let modes = Modes {
            network_closed: true,
            ..Modes::default()
        };
        assert_eq!(
            command(ID, &modes),
            Some(strings(&[
                "bravebot",
                "--resume",
                ID,
                "--run-network",
                "closed"
            ]))
        );
    }

    /// HERDR-10. A path the command line denied is carried, since it only takes reach away, and
    /// one a settings file or the managed file wrote is not, since that file says it again. The
    /// regression it rejects is a denial dropped, which reopens the session reaching the path.
    #[test]
    fn a_path_the_command_line_denied_is_carried() {
        use bravebot_sandbox::rules::Entry;
        let entry = |path: &str, by: Option<&str>, pinned| Entry {
            path: path.to_string(),
            by: by.map(PathBuf::from),
            pinned,
        };
        assert_eq!(
            typed(&[
                entry("secrets", None, false),
                entry("from-settings", Some("settings.json"), false),
                entry("pinned", Some("managed.json"), true),
            ]),
            strings(&["secrets"])
        );
        let modes = Modes {
            deny_read: strings(&["secrets"]),
            deny_write: strings(&["~/notes"]),
            ..Modes::default()
        };
        assert_eq!(
            command(ID, &modes),
            Some(strings(&[
                "bravebot",
                "--resume",
                ID,
                "--sandbox-deny-read",
                "secrets",
                "--sandbox-deny-write",
                "~/notes",
            ]))
        );
    }
}
