use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A task list row with `status`.
fn row(status: bravebot_core::todo::Status) -> bravebot_core::todo::Row {
    bravebot_core::todo::Row {
        content: "a task".into(),
        as_the_planner_had_it: "a task".into(),
        marker: "■",
        status,
    }
}

/// OPEN-1. A session that wrote and then ran a command that failed, with one task still open on
/// its newest list, has those loops open and no others.
#[test]
fn a_failed_command_after_a_write_and_an_open_todo_are_open_loops() {
    use bravebot_core::todo::Status;
    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let mut state = State::fresh(TrustStore::new(workspace.root()));
    state.work.count(true, false, false);
    state.work.count(false, true, true);
    // An older list with open rows is not counted: only the newest is the list now.
    state
        .todos
        .insert(1, vec![row(Status::Pending), row(Status::Active)]);
    state.todos.insert(
        2,
        vec![
            row(Status::Done),
            row(Status::Pending),
            row(Status::Cancelled),
        ],
    );

    let loops = crate::open_loops::of(&state, &workspace);
    assert_eq!(
        loops,
        bravebot_agent::open_loops::Loops {
            failed_command: true,
            open_todos: 1,
            ..Default::default()
        }
    );
}

/// OPEN-1. A session that has done nothing has nothing open.
#[test]
fn a_fresh_session_has_no_open_loops() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let state = State::fresh(TrustStore::new(workspace.root()));
    assert_eq!(
        crate::open_loops::of(&state, &workspace),
        bravebot_agent::open_loops::Loops::default()
    );
}

/// Whether the calling test runs here. `false` in the test's first process, which runs the test
/// again in a child process with a profile of its own and checks that it passed. A separate
/// process keeps profile changes out of parallel tests.
fn in_a_profile_of_its_own() -> bool {
    const CHILD: &str = "BRAVEBOT_OPEN_LOOPS_TEST";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let profile = tempfile::tempdir().unwrap();
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args([
        "--exact",
        std::thread::current().name().unwrap(),
        "--nocapture",
    ]);
    child.env(CHILD, "1");
    for name in bravebot_agent::home::PROFILE_VARIABLES {
        child.env(name, profile.path());
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

/// Whether the first request a turn sent tells the planner of open loops among the messages the
/// turn added: those after the last reply of an earlier turn. The conversation keeps a message
/// told on an earlier turn, so the whole request would still carry it.
fn told_of_open_loops(sent: &[String]) -> bool {
    let request: Value = serde_json::from_str(&sent[0]).unwrap();
    let messages = request["messages"].as_array().unwrap();
    let begins = messages
        .iter()
        .rposition(|message| message["role"] == "assistant")
        .map_or(0, |last| last + 1);
    messages[begins..]
        .iter()
        .any(|message| message.to_string().contains(OPEN))
}

/// How OPEN-4's message to the planner begins.
const OPEN: &str = "Open from earlier in this session";

/// OPEN-4. A desktop turn begins by telling the planner of the open loops where they changed since
/// it was last told, and says nothing of them on a turn after which nothing changed.
#[test]
fn a_desktop_turn_tells_the_planner_of_open_loops_once_per_changed_set() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));

    let sent = a_turn(&project, &state, 1, Some("a.txt"));
    assert!(!told_of_open_loops(&sent), "{}", sent[0]);
    // The write of turn 1 is a change no command has built.
    let sent = a_turn(&project, &state, 2, None);
    assert!(told_of_open_loops(&sent), "{}", sent[0]);
    assert!(sent[0].contains("changes not built since the last write"));
    let sent = a_turn(&project, &state, 3, None);
    assert!(!told_of_open_loops(&sent), "told again: {}", sent[0]);

    // A turn that ended with a command that failed.
    {
        let mut held = state.lock().unwrap();
        held.work.count(false, true, true);
        let loops = crate::open_loops::of(&held, &Workspace::new(&project).unwrap());
        held.open_loops.ended_with(loops);
    }
    let sent = a_turn(&project, &state, 4, None);
    assert!(told_of_open_loops(&sent), "{}", sent[0]);
    assert!(sent[0].contains("the last command failed"), "{}", sent[0]);
    let sent = a_turn(&project, &state, 5, None);
    assert!(!told_of_open_loops(&sent), "told again: {}", sent[0]);
}

/// OPEN-6. A session resumed from a record whose last command failed tells the planner so on its
/// first turn, and not on the turn after.
#[test]
fn a_resumed_session_tells_the_planner_on_its_first_turn_only() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    a_turn(&project, &state, 1, None);
    let id = {
        let held = state.lock().unwrap();
        held.handle.as_ref().unwrap().id().to_string()
    };
    drop(state);
    let mut record = bravebot_session::sessions::load(&project, &id).unwrap();
    record.work.count(false, true, true);

    let mut resumed = State::resumed(&project, &record, TrustStore::new(&project)).unwrap();
    let said = resumed.open_loops.for_the_planner();
    assert!(
        said.as_deref()
            .is_some_and(|said| said.contains("the last command failed")),
        "{said:?}"
    );
    assert_eq!(resumed.open_loops.for_the_planner(), None, "told twice");
    // Dropped, so the record is free to be opened again.
    drop(resumed);

    let resumed = Arc::new(Mutex::new(
        State::resumed(&project, &record, TrustStore::new(&project)).unwrap(),
    ));
    let sent = a_turn(&project, &resumed, 2, None);
    assert!(told_of_open_loops(&sent), "{}", sent[0]);
    let sent = a_turn(&project, &resumed, 3, None);
    assert!(!told_of_open_loops(&sent), "told again: {}", sent[0]);
}

/// OPEN-6. Each desktop turn adds its work and the files it wrote to the session's, and the record
/// the turn saves carries both, so a resume in either front end starts with what the session did.
#[test]
fn the_record_keeps_the_sessions_work_and_written_files() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    a_turn(&project, &state, 1, Some("a.txt"));
    a_turn(&project, &state, 2, Some("b.txt"));

    let id = {
        let held = state.lock().unwrap();
        held.handle.as_ref().unwrap().id().to_string()
    };
    // Dropped first, so the record is not held by the session that wrote it.
    drop(state);
    let record = bravebot_session::sessions::load(&project, &id).unwrap();
    assert_eq!(record.work.writes, 2, "{:?}", record.work);
    let written: Vec<PathBuf> = record
        .written
        .iter()
        .filter_map(bravebot_session::sessions::StoredPath::to_path)
        .collect();
    let names: Vec<_> = written
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.txt", "b.txt"], "{written:?}");

    let resumed = State::resumed(&project, &record, TrustStore::new(&project)).unwrap();
    assert_eq!(resumed.work, record.work);
    assert_eq!(resumed.written, written);
}

/// Run `git` in `root` with no configuration but its own. Unix only, like the one test that uses
/// it, since it points git's global configuration at `/dev/null`.
#[cfg(unix)]
fn git(root: &Path, arguments: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(arguments)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .expect("git ran");
    assert!(status.success(), "git {arguments:?}");
}

/// OPEN-1, OPEN-4, OPEN-6. A file a desktop turn wrote in a repository and nobody committed is
/// told to the planner on the next turn, and on the first turn of a session resumed from the
/// record.
#[cfg(unix)]
#[test]
fn a_written_file_not_committed_is_told_on_the_next_turn_and_on_resume() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    git(&project, &["init", "-q"]);
    std::fs::write(project.join("base.txt"), "base").unwrap();
    git(&project, &["add", "."]);
    git(&project, &["commit", "-qm", "base"]);
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));

    a_turn(&project, &state, 1, Some("a.txt"));
    let sent = a_turn(&project, &state, 2, None);
    assert!(told_of_open_loops(&sent), "{}", sent[0]);
    assert!(sent[0].contains("1 file not committed"), "{}", sent[0]);

    let id = {
        let held = state.lock().unwrap();
        held.handle.as_ref().unwrap().id().to_string()
    };
    drop(state);
    let record = bravebot_session::sessions::load(&project, &id).unwrap();
    let mut resumed = State::resumed(&project, &record, TrustStore::new(&project)).unwrap();
    let said = resumed.open_loops.for_the_planner();
    assert!(
        said.as_deref()
            .is_some_and(|said| said.contains("1 file not committed")),
        "{said:?}"
    );
}

/// OPEN-4 and GROUND-4. A turn that wrote a file and then failed still adds its work and the file
/// to the session's, so the next turn is told of the change no command built.
#[test]
fn a_turn_that_wrote_and_then_failed_is_told_on_the_next_turn() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));

    a_turn_that(&project, &state, 1, Some("a.txt"), true);
    {
        let held = state.lock().unwrap();
        assert_eq!(held.work.writes, 1, "{:?}", held.work);
        assert_eq!(held.written, [project.join("a.txt")]);
    }
    let sent = a_turn(&project, &state, 2, None);
    assert!(told_of_open_loops(&sent), "{}", sent[0]);
    assert!(
        sent[0].contains("changes not built since the last write"),
        "{}",
        sent[0]
    );
}

/// OPEN-6. The checkouts a record kept and never applied are open loops of the session that
/// resumes it once they are taken back, and the planner is told of them.
#[test]
fn a_kept_checkout_taken_back_on_resume_is_told_to_the_planner() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    a_turn(&project, &state, 1, None);
    let id = {
        let held = state.lock().unwrap();
        held.handle.as_ref().unwrap().id().to_string()
    };
    drop(state);

    // A checkout where a resume finds one: under the state directory, with git's record of it.
    let home = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let kept_at = home
        .canonicalize()
        .unwrap()
        .join("checkouts")
        .join(bravebot_agent::home::key_for(&project))
        .join("c1");
    std::fs::create_dir_all(&kept_at).unwrap();
    std::fs::create_dir_all(project.join(".git/worktrees/c1")).unwrap();
    let mut record = bravebot_session::sessions::load(&project, &id).unwrap();
    record
        .checkouts
        .push(bravebot_session::sessions::StoredCheckout {
            id: "c1".into(),
            path: kept_at.display().to_string(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            delegate: bravebot_core::delegate::DelegateId::nth(1).to_string(),
            worked_in: true,
            written: Vec::new(),
            referenced: 0,
            size: Some(bravebot_session::sessions::StoredSize {
                bytes: 1,
                whole: true,
            }),
            applied: false,
        });

    let mut resumed = State::resumed(&project, &record, TrustStore::new(&project)).unwrap();
    // Not placed yet, so nothing is open.
    assert_eq!(resumed.open_loops.for_the_planner(), None);
    let open = Open::opened(project.clone(), resumed, true, None, false);
    take_back_checkouts(
        &open,
        &record,
        &crate::settings::layers(Some(&project), None),
    );

    let said = open.state.lock().unwrap().open_loops.for_the_planner();
    assert!(
        said.as_deref()
            .is_some_and(|said| said.contains("1 kept checkout")),
        "{said:?}"
    );
}

/// OPEN-4. Undoing a turn works the open loops out again from what the session now holds, and the
/// planner is told of them on the next turn: the undone turn's open todo goes and the failed
/// command from before it stays.
#[test]
fn undoing_a_turn_tells_the_planner_of_the_loops_left_open() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let workspace = Workspace::new(&project).unwrap();
    let mut state = State::fresh(TrustStore::new(&project));
    state.work.count(false, true, true);
    state.turns = 1;
    open_rewind_point(&mut state, &workspace, "make a list");
    state.turns = 2;
    state
        .todos
        .insert(2, vec![row(bravebot_core::todo::Status::Pending)]);
    let loops = crate::open_loops::of(&state, &workspace);
    state.open_loops.ended_with(loops);
    assert!(state.open_loops.for_the_planner().is_some(), "setup");
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(project.clone(), state, true, None, false));
    let line = json!({"id": 1, "method": "session.rewind", "params": {
        "session": handle, "steps": 1
    }})
    .to_string();
    bridge.dispatch(&Request::parse(&line).unwrap()).unwrap();

    let said = bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .open_loops
        .for_the_planner();
    let said = said.expect("the loops left open were not told again");
    assert!(said.contains("the last command failed"), "{said}");
    assert!(
        !said.contains("todo"),
        "the undone turn's todo is told: {said}"
    );
}

/// OPEN-1. Undoing a session's only turn discards its record, and with it the work, the written
/// files and the open loops, so nothing is left open.
#[test]
fn undoing_the_only_turn_leaves_nothing_open() {
    if !in_a_profile_of_its_own() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let workspace = Workspace::new(&project).unwrap();
    let mut state = State::fresh(TrustStore::new(&project));
    open_rewind_point(&mut state, &workspace, "write it");
    state.turns = 1;
    state.work.count(true, false, false);
    state.written.push(project.join("a.txt"));
    let loops = crate::open_loops::of(&state, &workspace);
    state.open_loops.ended_with(loops);
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(project.clone(), state, true, None, false));
    let line = json!({"id": 1, "method": "session.rewind", "params": {
        "session": handle, "steps": 1
    }})
    .to_string();
    bridge.dispatch(&Request::parse(&line).unwrap()).unwrap();

    let mut held = bridge.open[&handle].state.lock().unwrap();
    assert_eq!(held.work, bravebot_agent::Work::default());
    assert!(held.written.is_empty(), "{:?}", held.written);
    // The loops the undone turn ended with are gone, so a session with nothing open has not
    // changed since.
    let loops = crate::open_loops::of(&held, &workspace);
    assert!(!held.open_loops.ended_with(loops), "{:?}", held.open_loops);
}

/// Run one desktop turn whose planner writes `file`, where there is one, and then ends. Gives back
/// the body of each request the turn sent.
fn a_turn(
    project: &Path,
    state: &Arc<Mutex<State>>,
    turn: usize,
    file: Option<&str>,
) -> Vec<String> {
    a_turn_that(project, state, turn, file, false)
}

/// [`a_turn`], where `fails` has the service refuse the request that follows the write, so the
/// turn ends in `turn.error`.
fn a_turn_that(
    project: &Path,
    state: &Arc<Mutex<State>>,
    turn: usize,
    file: Option<&str>,
    fails: bool,
) -> Vec<String> {
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = events.send(event);
        }
    }));
    let (endpoint, stop, server, bodies) = service(file, fails);
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .unwrap();
    let (_answers, receiver) = mpsc::channel();
    let workspace = Workspace::new(project).unwrap();
    let state = Arc::clone(state);
    let project = project.to_path_buf();
    let cancel = Cancel::new();
    let cancelling = cancel.clone();
    let worker = std::thread::spawn(move || {
        work(Work {
            emitter,
            session: "test".into(),
            project,
            state,
            reports: Default::default(),
            config,
            attribution: Default::default(),
            effort: None,
            variant: None,
            output_cap: None,
            download_cap: None,
            deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
            auto_vetting: false,
            front_end: bravebot_agent::FrontEnd::Desktop,
            sandbox: bravebot_sandbox::SandboxMode::default(),
            // Writes go through unasked, so the turn needs no answer from a window.
            permission_mode: LiveMode::new(PermissionMode::AcceptEdits),
            mcp_requested: Vec::new(),
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            addressing: None,
            addressing_standing: false,
            workspace,
            prompt: "write it".into(),
            composed: None,
            files: vec![],
            dropped: vec![],
            attachments: vec![],
            images: vec![],
            recall: false,
            turn,
            cancel,
            pending: Arc::new(Mutex::new(None)),
            answerers: Default::default(),
            answers: receiver,
            finished: Arc::new(AtomicBool::new(false)),
            interjections: Default::default(),
            actors: Default::default(),
            published: Default::default(),
        })
    });
    let result = received.recv_timeout(Duration::from_secs(20));
    if result.is_err() {
        // So the worker ends rather than waiting on a service that has stopped.
        cancelling.cancel();
    }
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    let event = result.expect("turn must report its end");
    worker.join().unwrap();
    let ends = if fails { "turn.error" } else { "turn.done" };
    assert_eq!(event.name, ends, "{:?}", event.data);
    bodies.lock().unwrap().clone()
}

/// The bodies of the requests a [`service`] received, in order.
type Bodies = Arc<Mutex<Vec<String>>>;

/// Respond locally: a request that ends with a tool result ends the turn, or is refused with a 400
/// where `fails`, and any other asks for `file` to be written, or ends the turn where there is no
/// file. Each request's body is kept. No real model or account is involved.
fn service(
    file: Option<&str>,
    fails: bool,
) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>, Bodies) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let arguments = file.map(|file| json!({"path": file, "contents": "one"}).to_string());
    let bodies = Bodies::default();
    let kept = Arc::clone(&bodies);
    let server = std::thread::spawn(move || {
        while !stopping.load(Ordering::Acquire) {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(error) => panic!("accept: {error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request: Value = serde_json::from_slice(&body).unwrap_or_default();
            // Only the planner's requests, which carry the conversation.
            if request["messages"].is_array() {
                kept.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
            }
            // The last message, since a later turn's request also carries the earlier turns'.
            let answered = request["messages"]
                .as_array()
                .and_then(|messages| messages.last())
                .is_some_and(|last| last["role"] == "tool");
            let (delta, finish) = if let (false, Some(arguments)) = (answered, &arguments) {
                (
                    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "w1",
                        "type": "function",
                        "function": {"name": "write_file", "arguments": arguments}}]}),
                    "tool_calls",
                )
            } else {
                (json!({"role": "assistant", "content": "done"}), "stop")
            };
            let chunk = json!({"model": "test",
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let status = if answered && fails {
                "400 Bad Request"
            } else {
                "200 OK"
            };
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, stop, server, bodies)
}
