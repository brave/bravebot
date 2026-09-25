use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A client may act on the final event immediately, before the emitter returns.
/// Success, failure and cancellation must all release the session first.
#[test]
fn final_events_find_the_session_ready_for_another_request() {
    const CHILD: &str = "BRAVEBOT_COMPLETION_TEST";
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
    let results = ["success", "failure", "cancel"].map(|ending| (ending, check_completion(ending)));
    assert!(
        results.iter().all(|(_, ready)| *ready == (true, true)),
        "(finished, unlocked) at final event: {results:?}"
    );
}

fn check_completion(ending: &str) -> (bool, bool) {
    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let project = workspace.root().to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    let finished = Arc::new(AtomicBool::new(false));
    let (events, received) = mpsc::channel();
    let held_state = state.clone();
    let held_finished = finished.clone();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            events
                .send((
                    event,
                    held_finished.load(Ordering::Acquire),
                    held_state.try_lock().is_ok(),
                ))
                .unwrap();
        }
    }));
    let (endpoint, stop, server) = service(ending == "failure");
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .unwrap();
    let cancel = Cancel::new();
    if ending == "cancel" {
        cancel.cancel();
    }
    let (_answers, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        work(Work {
            emitter,
            session: "test".into(),
            project,
            state,
            config,
            attribution: Default::default(),
            output_cap: None,
            auto_vetting: false,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            workspace,
            prompt: "hello".into(),
            composed: None,
            files: vec![],
            dropped: vec![],
            recall: false,
            turn: 1,
            cancel,
            pending: Arc::new(Mutex::new(None)),
            answers: receiver,
            finished,
        })
    });
    let result = received.recv_timeout(Duration::from_secs(10));
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    let (event, finished, unlocked) = result.expect("turn must report its end");
    worker.join().unwrap();
    assert_eq!(
        event.name,
        if ending == "success" {
            "turn.done"
        } else {
            "turn.error"
        }
    );
    if ending == "cancel" {
        assert_eq!(event.data["kind"], "cancelled");
    } else if ending == "failure" {
        assert_eq!(event.data["kind"], "chat");
        assert_eq!(event.data["status"], 400);
    }
    (finished, unlocked)
}

/// Respond locally; no real model or account is involved. All socket waits are bounded.
fn service(fail: bool) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
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
            reader.read_exact(&mut vec![0; length]).unwrap();
            let status = if fail { "400 Bad Request" } else { "200 OK" };
            let body = if fail { json!({"error":{"message":"test failure"}}) } else {
                json!({"model":"test", "choices":[{"index":0,"delta":{"role":"assistant","content":"hello"},"finish_reason":"stop"}]})
            }.to_string();
            let body = if fail {
                body
            } else {
                format!("data: {body}\n\ndata: [DONE]\n\n")
            };
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (endpoint, stop, server)
}
