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

/// Run one desktop turn whose planner writes `file`, where there is one, and then ends. Gives back
/// the body of each request the turn sent.
fn a_turn(
    project: &Path,
    state: &Arc<Mutex<State>>,
    turn: usize,
    file: Option<&str>,
) -> Vec<String> {
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = events.send(event);
        }
    }));
    let (endpoint, stop, server, bodies) = service(file);
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
            cancel: Cancel::new(),
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
    worker.join().unwrap();
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    let event = result.expect("turn must report its end");
    assert_eq!(event.name, "turn.done", "{:?}", event.data);
    bodies.lock().unwrap().clone()
}

/// The bodies of the requests a [`service`] received, in order.
type Bodies = Arc<Mutex<Vec<String>>>;

/// Respond locally: a request that ends with a tool result ends the turn, and any other asks for
/// `file` to be written, or ends the turn where there is no file. Each request's body is kept. No
/// real model or account is involved.
fn service(file: Option<&str>) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>, Bodies) {
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
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, stop, server, bodies)
}
