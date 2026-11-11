use super::*;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

/// Long enough for a request that is coming to arrive, on a loaded machine.
const ARRIVES_WITHIN: Duration = Duration::from_secs(10);

/// Long enough that a request that was going to be sent would have been.
const QUIET_FOR: Duration = Duration::from_millis(400);

/// A herdr that records each request and answers it, holding its first answer until let go when
/// asked to, so a test can line reports up behind one being sent.
struct FakeHerdr {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    requests: Receiver<Value>,
    let_go: Option<Sender<()>>,
}

impl FakeHerdr {
    fn answering() -> FakeHerdr {
        FakeHerdr::start("herdr.sock", false)
    }

    /// A herdr listening at `name` in a directory of its own.
    fn answering_at(name: &str) -> FakeHerdr {
        FakeHerdr::start(name, false)
    }

    fn holding_its_first_answer() -> FakeHerdr {
        FakeHerdr::start("herdr.sock", true)
    }

    fn start(name: &str, hold_first: bool) -> FakeHerdr {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join(name);
        let listener = UnixListener::bind(&socket).unwrap();
        let (recorded, requests) = channel();
        let (let_go, held) = channel::<()>();
        std::thread::spawn(move || {
            let mut first = hold_first;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                let id = request["id"].clone();
                let _ = recorded.send(request);
                if std::mem::take(&mut first) {
                    let _ = held.recv();
                }
                let answer = json!({"id": id, "result": {}});
                let _ = writeln!(stream, "{answer}");
            }
        });
        FakeHerdr {
            _directory: directory,
            socket,
            requests,
            let_go: Some(let_go),
        }
    }

    fn reporter(&self) -> Reporter {
        Reporter::from_variables(inside_herdr(&self.socket))
    }

    fn next(&self) -> Value {
        self.requests
            .recv_timeout(ARRIVES_WITHIN)
            .expect("herdr was sent a request")
    }

    fn quiet(&self) -> bool {
        self.requests.recv_timeout(QUIET_FOR).is_err()
    }

    fn let_go(&mut self) {
        let _ = self.let_go.take().map(|let_go| let_go.send(()));
    }
}

fn inside_herdr(socket: &Path) -> impl Fn(&str) -> Option<OsString> + use<> {
    let socket = socket.as_os_str().to_owned();
    move |name| match name {
        "HERDR_ENV" => Some("1".into()),
        "HERDR_PANE_ID" => Some("w1:p2".into()),
        "HERDR_SOCKET_PATH" => Some(socket.clone()),
        _ => None,
    }
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// A session outside herdr, or in a pane herdr did not fully describe, must not connect to a socket
/// a stray variable names: that socket could be anything's.
#[test]
fn a_reporter_outside_herdr_is_inactive_and_connects_to_nothing() {
    let herdr = FakeHerdr::answering();
    let socket = herdr.socket.as_os_str().to_owned();
    let partial: [&[(&str, &str)]; 4] = [
        &[],
        &[("HERDR_PANE_ID", "w1:p2"), ("HERDR_SOCKET_PATH", "")],
        &[("HERDR_ENV", "1"), ("HERDR_SOCKET_PATH", "")],
        &[
            ("HERDR_ENV", "0"),
            ("HERDR_PANE_ID", "w1:p2"),
            ("HERDR_SOCKET_PATH", ""),
        ],
    ];
    for set in partial {
        let reporter = Reporter::from_variables(|name| {
            set.iter().find(|(set, _)| *set == name).map(|(_, value)| {
                if name == "HERDR_SOCKET_PATH" {
                    socket.clone()
                } else {
                    (*value).into()
                }
            })
        });
        assert!(!reporter.is_active(), "{set:?}");
        reporter.report(State::Working, None);
        reporter.release();
    }
    assert!(herdr.quiet());
}

/// A pane id or socket path that is not valid UTF-8 leaves the reporter inactive. The regression
/// it rejects is a lossy reading: the socket path would become its lookalike with U+FFFD, where
/// something else could be listening, and the pane id one herdr never gave.
#[test]
fn a_herdr_variable_that_is_not_utf8_leaves_the_reporter_inactive() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let lookalike = FakeHerdr::answering_at("herdr\u{FFFD}.sock");
    let mut socket = lookalike
        .socket
        .parent()
        .unwrap()
        .as_os_str()
        .as_bytes()
        .to_vec();
    socket.extend_from_slice(b"/herdr\xff.sock");
    let socket = OsString::from_vec(socket);
    assert_eq!(
        PathBuf::from(socket.to_string_lossy().into_owned()),
        lookalike.socket,
        "the lossy reading of the variable is where the fake herdr listens"
    );
    let reporter = Reporter::from_variables(|name| match name {
        "HERDR_SOCKET_PATH" => Some(socket.clone()),
        other => inside_herdr(&lookalike.socket)(other),
    });
    assert!(!reporter.is_active(), "a socket path that is not UTF-8");
    reporter.report(State::Working, None);
    assert!(lookalike.quiet(), "a socket path that is not UTF-8");

    let herdr = FakeHerdr::answering();
    let reporter = Reporter::from_variables(|name| match name {
        "HERDR_PANE_ID" => Some(OsStr::from_bytes(b"w1:p\xff").to_owned()),
        other => inside_herdr(&herdr.socket)(other),
    });
    assert!(!reporter.is_active(), "a pane id that is not UTF-8");
    reporter.report(State::Working, None);
    assert!(herdr.quiet(), "a pane id that is not UTF-8");
}

/// The reporter takes nothing else from the environment, so nothing else in it can reach herdr.
#[test]
fn a_reporter_asks_the_environment_for_the_three_herdr_variables_alone() {
    let herdr = FakeHerdr::answering();
    let asked = Mutex::new(BTreeSet::new());
    let inside = inside_herdr(&herdr.socket);
    let reporter = Reporter::from_variables(|name| {
        asked.lock().unwrap().insert(name.to_owned());
        inside(name)
    });
    assert!(reporter.is_active());
    let asked = asked.into_inner().unwrap();
    assert_eq!(
        asked,
        BTreeSet::from(["HERDR_ENV", "HERDR_PANE_ID", "HERDR_SOCKET_PATH"].map(String::from))
    );
}

/// herdr shows the session by what the report names: a report without the pane or under another
/// name would light up the wrong pane or none.
#[test]
fn a_state_report_names_bravebot_the_pane_and_the_state() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.report(State::Blocked, Some("May I run cargo test?".into()));
    let request = herdr.next();
    assert_eq!(request["method"], "pane.report_agent");
    let params = &request["params"];
    assert_eq!(params["pane_id"], "w1:p2");
    assert_eq!(params["source"], "bravebot");
    assert_eq!(params["agent"], "bravebot");
    assert_eq!(params["state"], "blocked");
    assert_eq!(params["message"], "May I run cargo test?");
    assert_eq!(request["id"], format!("bravebot-{}", params["seq"]));
}

/// Every key a report carries is one this crate chose; a caller's text goes only in the message.
#[test]
fn what_a_report_carries_is_a_closed_set_of_keys() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.resume_with(Resume::new("0193", argv(&["bravebot", "--resume", "0193"])).unwrap());
    reporter.report(State::Idle, Some("Ready".into()));
    let request = herdr.next();
    let keys: BTreeSet<&str> = request["params"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let allowed = BTreeSet::from([
        "pane_id",
        "source",
        "agent",
        "state",
        "message",
        "seq",
        "agent_session_id",
        "resume_argv",
    ]);
    assert!(keys.is_subset(&allowed), "{keys:?}");
    let top: BTreeSet<&str> = request
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(top, BTreeSet::from(["id", "method", "params"]));
}

/// herdr ignores a report numbered at or below the last it took from bravebot in that pane, so a
/// number that fell, or a session started again in the pane counting from one, would go unseen.
#[test]
fn sequence_numbers_rise_and_start_from_the_time() {
    let herdr = FakeHerdr::answering();
    let before = now_ms();
    let reporter = herdr.reporter();
    let mut seqs = Vec::new();
    for state in [State::Working, State::Idle, State::Working] {
        reporter.report(state, None);
        seqs.push(herdr.next()["params"]["seq"].as_u64().unwrap());
    }
    reporter.release();
    seqs.push(herdr.next()["params"]["seq"].as_u64().unwrap());
    assert!(seqs[0] >= before, "{seqs:?} from {before}");
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");
}

/// A session that changes state faster than herdr answers must not queue up behind it: what
/// herdr needs is where the session is now, and a queue only delays that.
#[test]
fn only_the_newest_waiting_report_is_sent() {
    let mut herdr = FakeHerdr::holding_its_first_answer();
    let reporter = herdr.reporter();
    reporter.report(State::Working, None);
    assert_eq!(herdr.next()["params"]["state"], "working");
    reporter.report(State::Blocked, None);
    reporter.report(State::Idle, None);
    herdr.let_go();
    assert_eq!(herdr.next()["params"]["state"], "idle");
    assert!(herdr.quiet());
}

/// A report sent again for every redraw would spend a connection for nothing herdr did not know.
#[test]
fn an_unchanged_report_is_not_sent_again() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.report(State::Working, Some("Reading".into()));
    herdr.next();
    reporter.report(State::Working, Some("Reading".into()));
    assert!(herdr.quiet());
    reporter.report(State::Working, Some("Writing".into()));
    assert_eq!(herdr.next()["params"]["message"], "Writing");
}

/// herdr takes a resume command only from the agent it knows holds the pane, and learns that from
/// a state report; a resume command sent alone first would be refused and lost.
#[test]
fn a_resume_command_goes_with_a_state_report_and_not_before_one() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.resume_with(Resume::new("0193", argv(&["bravebot", "--resume", "0193"])).unwrap());
    assert!(herdr.quiet());
    reporter.report(State::Idle, None);
    let params = herdr.next()["params"].clone();
    assert_eq!(params["state"], "idle");
    assert_eq!(params["agent_session_id"], "0193");
    assert_eq!(
        params["resume_argv"],
        json!(["bravebot", "--resume", "0193"])
    );
    reporter.resume_with(Resume::new("0194", argv(&["bravebot", "--resume", "0194"])).unwrap());
    let params = herdr.next()["params"].clone();
    assert_eq!(params["state"], "idle");
    assert_eq!(
        params["resume_argv"],
        json!(["bravebot", "--resume", "0194"])
    );
    reporter.resume_with(Resume::new("0194", argv(&["bravebot", "--resume", "0194"])).unwrap());
    assert!(herdr.quiet());
}

/// herdr refuses a whole report whose resume command it would not type into a shell, so one sent
/// anyway would cost the state it rode on.
#[test]
fn a_resume_command_herdr_would_refuse_is_refused_here() {
    let long_id = "a".repeat(MOST_SESSION_ID_BYTES + 1);
    let long_argument = "a".repeat(MOST_COMMAND_BYTES);
    let refused: Vec<(&str, Vec<String>, Refused)> = vec![
        ("", argv(&["bravebot"]), Refused::NoSessionId),
        (&long_id, argv(&["bravebot"]), Refused::SessionIdTooLong),
        ("01\n93", argv(&["bravebot"]), Refused::ControlInSessionId),
        ("0193", vec![], Refused::NoCommand),
        (
            "0193",
            vec!["a".to_owned(); MOST_ARGUMENTS + 1],
            Refused::TooManyArguments,
        ),
        (
            "0193",
            vec!["bravebot".to_owned(), long_argument],
            Refused::TooManyBytes,
        ),
        (
            "0193",
            argv(&["bravebot", "--resume", "01\t93"]),
            Refused::ControlCharacter,
        ),
        ("0193", argv(&["bravebot", "it's"]), Refused::Apostrophe),
        (
            "0193",
            argv(&["/usr/local/bin/bravebot"]),
            Refused::NotAPlainCommand,
        ),
        ("0193", argv(&["-bravebot"]), Refused::NotAPlainCommand),
        ("0193", argv(&[""]), Refused::NotAPlainCommand),
        ("0193", argv(&["brave bot"]), Refused::NotAPlainCommand),
    ];
    for (session_id, argv, why) in refused {
        assert_eq!(Resume::new(session_id, argv.clone()), Err(why), "{argv:?}");
    }
    let most = "a".repeat(MOST_COMMAND_BYTES - "bravebot".len());
    let longest_id = "a".repeat(MOST_SESSION_ID_BYTES);
    let taken: Vec<(&str, Vec<String>)> = vec![
        ("0193", argv(&["bravebot", "--resume", "0193"])),
        ("0193", argv(&["bravebot-dev.2_x"])),
        ("0193", vec!["bravebot".to_owned(), most]),
        ("0193", vec!["a".to_owned(); MOST_ARGUMENTS]),
        (&longest_id, argv(&["bravebot", "/path with spaces"])),
    ];
    for (session_id, argv) in taken {
        assert!(Resume::new(session_id, argv.clone()).is_ok(), "{argv:?}");
    }
}

/// A report that reached herdr after the session left would mark the pane as a session no longer
/// running in it.
#[test]
fn nothing_is_sent_after_release() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.report(State::Working, None);
    herdr.next();
    reporter.release();
    let request = herdr.next();
    assert_eq!(request["method"], "pane.release_agent");
    assert_eq!(
        request["params"],
        json!({
            "pane_id": "w1:p2",
            "source": "bravebot",
            "agent": "bravebot",
            "seq": request["params"]["seq"],
        })
    );
    reporter.report(State::Idle, None);
    reporter.resume_with(Resume::new("0193", argv(&["bravebot"])).unwrap());
    reporter.release();
    assert!(herdr.quiet());
}

/// A session that never told herdr it held the pane has nothing to release.
#[test]
fn a_session_that_never_reported_releases_nothing() {
    let herdr = FakeHerdr::answering();
    let reporter = herdr.reporter();
    reporter.release();
    assert!(herdr.quiet());
}

/// Reporting is off the session's threads and a release on the way out is bounded, so a herdr
/// that accepts and never answers holds up neither.
#[test]
fn a_herdr_that_never_answers_holds_up_neither_reports_nor_release() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("herdr.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    let reporter = Reporter::from_variables(inside_herdr(&socket));
    let started = Instant::now();
    for state in [State::Working, State::Blocked, State::Idle, State::Working] {
        reporter.report(state, None);
    }
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "{:?}",
        started.elapsed()
    );
    let started = Instant::now();
    reporter.release();
    let took = started.elapsed();
    assert!(
        took >= RELEASE_WITHIN / 2 && took < Duration::from_secs(2),
        "{took:?}"
    );
}

/// herdr may have gone, or never made the socket the variable names.
#[test]
fn a_herdr_socket_that_is_not_there_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let reporter = Reporter::from_variables(inside_herdr(&directory.path().join("gone.sock")));
    assert!(reporter.is_active());
    reporter.report(State::Working, None);
    let started = Instant::now();
    reporter.release();
    assert!(started.elapsed() < Duration::from_secs(1));
}
