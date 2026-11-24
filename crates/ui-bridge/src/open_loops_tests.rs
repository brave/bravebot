use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// OPEN-6. Each desktop turn adds its work and the files it wrote to the session's, and the record
/// the turn saves carries both, so a resume in either front end starts with what the session did.
#[test]
fn the_record_keeps_the_sessions_work_and_written_files() {
    const CHILD: &str = "BRAVEBOT_OPEN_LOOPS_TEST";
    if std::env::var_os(CHILD).is_none() {
        let profile = tempfile::tempdir().unwrap();
        // A separate process keeps profile changes out of parallel tests.
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
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let project = Workspace::new(directory.path())
        .unwrap()
        .root()
        .to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    a_turn_that_writes(&project, &state, 1, "a.txt");
    a_turn_that_writes(&project, &state, 2, "b.txt");

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

/// Run one desktop turn whose planner writes `file` and then ends.
fn a_turn_that_writes(project: &Path, state: &Arc<Mutex<State>>, turn: usize, file: &str) {
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = events.send(event);
        }
    }));
    let (endpoint, stop, server) = service(file);
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
}

/// Respond locally: a request that ends with a tool result ends the turn, and any other asks for
/// `file` to be written. No real model or account is involved.
fn service(file: &str) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let arguments = json!({"path": file, "contents": "one"}).to_string();
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
            // The last message, since a later turn's request also carries the earlier turns'.
            let answered = request["messages"]
                .as_array()
                .and_then(|messages| messages.last())
                .is_some_and(|last| last["role"] == "tool");
            let (delta, finish) = if answered {
                (json!({"role": "assistant", "content": "done"}), "stop")
            } else {
                (
                    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "w1",
                        "type": "function",
                        "function": {"name": "write_file", "arguments": arguments}}]}),
                    "tool_calls",
                )
            };
            let chunk = json!({"model": "test",
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, stop, server)
}
