//! A launch that ends within the first seconds sends nothing.
//!
//! In a process of its own because `begin` runs once per process.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_backtrace::{Process, begin};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// CRASH-6: a report is waiting and the launch begins, and for the first second no connection is
/// made and the report stays where it is. A command such as `--version` ends well inside that, so
/// it starts no request that it would cut off partway. The report is sent once the launch has run
/// longer, which `launch.rs` shows.
#[test]
fn a_launch_that_ends_within_the_first_seconds_makes_no_request() {
    let service = TcpListener::bind("127.0.0.1:0").unwrap();
    service.set_nonblocking(true).unwrap();
    let url = format!(
        "http://127.0.0.1:{}/brave/token/json",
        service.local_addr().unwrap().port()
    );
    let state = tempfile::tempdir().unwrap();
    let dir = state.path().join("crash-reports");
    std::fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let waiting = dir.join(format!("{:016}-4242.json", now - 3_600_000));
    std::fs::write(&waiting, b"left by an earlier launch").unwrap();

    begin(Process::Cli, state.path().to_path_buf(), url);

    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        assert!(
            service.accept().is_err(),
            "a request was made in the first second of a launch"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(waiting.exists(), "the waiting report was sent or removed");
}
