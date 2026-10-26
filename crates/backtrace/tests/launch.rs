//! A whole launch: what an earlier one left is sent in the background, and this one's own panic
//! leaves a report for the next.
//!
//! In a process of its own because `begin` runs once per process.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_backtrace::{Process, begin};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wait, bounded, for `condition`, so a launch that never sends fails the test instead of hanging
/// it.
fn eventually(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("gave up waiting for {what}");
}

fn report_names(state: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(state.join("crash-reports")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

/// Every body a loopback server was sent, each answered 200, until it is told to stop.
struct Collector {
    url: String,
    bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Collector {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://127.0.0.1:{}/brave/token/json",
            listener.local_addr().unwrap().port()
        );
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (kept, counted, halt) = (
            Arc::clone(&bodies),
            Arc::clone(&connections),
            Arc::clone(&stop),
        );
        let handle = std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                };
                counted.fetch_add(1, Ordering::SeqCst);
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                if let Some(body) = read_body(&mut stream) {
                    kept.lock().unwrap().push(body);
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                );
            }
        });
        Self {
            url,
            bodies,
            connections,
            stop,
            handle: Some(handle),
        }
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn bodies(&self) -> Vec<Vec<u8>> {
        self.bodies.lock().unwrap().clone()
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_body(stream: &mut std::net::TcpStream) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
            let wanted: usize = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            if buffer.len() >= end + 4 + wanted {
                return Some(buffer[end + 4..end + 4 + wanted].to_vec());
            }
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// CRASH-6: a launch sends the report a crash left and deletes it, without the caller waiting, and
/// then its own panic leaves a report and makes no second request. The report sent is the earlier
/// file's bytes, and the one left is a different file for the next launch.
#[test]
fn a_launch_sends_what_an_earlier_one_left_and_leaves_its_own_for_the_next() {
    let collector = Collector::start();
    let state = tempfile::tempdir().unwrap();
    let dir = state.path().join("crash-reports");
    std::fs::create_dir_all(&dir).unwrap();
    // What a crash an hour ago left. The bytes are not a report on purpose: it is sent as it is.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let earlier = dir.join(format!("{:016}-4242.json", now - 3_600_000));
    std::fs::write(&earlier, b"left by an earlier launch").unwrap();

    begin(
        Process::Cli,
        state.path().to_path_buf(),
        collector.url.clone(),
    );

    eventually("the earlier report to be sent and deleted", || {
        !earlier.exists() && collector.connections() == 1
    });
    assert_eq!(collector.bodies(), [b"left by an earlier launch".to_vec()]);

    // This launch's own crash leaves a file, and with the one sent already gone it is the only one.
    let outcome = std::panic::catch_unwind(|| {
        panic!("a message the source wrote");
    });
    assert!(outcome.is_err());
    let names = report_names(state.path());
    assert_eq!(names.len(), 1, "{names:?}");

    // Nothing is sent again until a launch begins, so the count stays where it was.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(collector.connections(), 1);
}
