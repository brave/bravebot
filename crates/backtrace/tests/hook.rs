//! The panic hook, in a process of its own because a hook is the process's and `begin` runs once.
//!
//! What a panic does is checked here and what a report holds is checked in `privacy.rs`, so each
//! of the two is a process that panics once and is read once.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_backtrace::{Process, begin};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static EARLIER_HOOK_RAN: AtomicBool = AtomicBool::new(false);

/// CRASH-2: a panic leaves one file in the state directory and sends nothing. The endpoint is a
/// socket this test holds, so a request made while the process was panicking would be a connection
/// it could see. The hook that was there before still runs, so the message the person sees is the
/// one they would have seen.
#[test]
fn a_panic_leaves_one_report_and_sends_nothing() {
    // Says what panicked as well, since a hook that said nothing would swallow the message of an
    // assertion in this test that failed.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{info}");
        EARLIER_HOOK_RAN.store(true, Ordering::SeqCst);
    }));

    let endpoint = TcpListener::bind("127.0.0.1:0").unwrap();
    endpoint.set_nonblocking(true).unwrap();
    let url = format!(
        "http://127.0.0.1:{}/brave/token/json",
        endpoint.local_addr().unwrap().port()
    );
    let state = tempfile::tempdir().unwrap();
    begin(Process::Rpc, state.path().to_path_buf(), url);

    let reports = state.path().join("crash-reports");
    assert!(
        !reports.exists() || std::fs::read_dir(&reports).unwrap().next().is_none(),
        "a report existed before any panic"
    );

    let outcome = std::panic::catch_unwind(|| {
        panic!("a message the source wrote");
    });
    assert!(outcome.is_err());

    let mut files: Vec<_> = std::fs::read_dir(&reports)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "expected one report in {reports:?}");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files.remove(0)).unwrap()).unwrap();
    let attributes = &report["attributes"];
    assert_eq!(attributes["process"], "rpc");
    assert_eq!(attributes["error.message"], "a message the source wrote");
    assert!(
        attributes["panic.file"]
            .as_str()
            .unwrap()
            .ends_with("tests/hook.rs"),
        "{attributes}"
    );
    assert!(attributes["panic.line"].as_u64().unwrap() > 0);
    // The default hook again, so that an assertion below that fails says why. Both hooks that were
    // installed have done what this test asks of them by now.
    drop(std::panic::take_hook());
    assert!(
        EARLIER_HOOK_RAN.load(Ordering::SeqCst),
        "the hook that was installed first did not run"
    );

    // Long enough for a thread that was going to send to have connected.
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        assert!(
            endpoint.accept().is_err(),
            "a request was made while or after the process panicked"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
