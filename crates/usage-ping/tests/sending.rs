//! The ping against a local server: what it sends, and when it sends nothing.
//!
//! Each test takes a scratch state directory and points the ping at a listener of its own, so the
//! developer's own record is never read or written and nothing reaches the network.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bravebot_usage_ping::{Backend, Outcome, Surface, send};

/// One lock for the whole file, since the variables these set are process-wide.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// The variables that could change the answer, cleared for every test.
const CLEARED: [&str; 9] = [
    "BRAVEBOT_USAGE_PING",
    "DO_NOT_TRACK",
    "BRAVEBOT_INSTALLED_VIA",
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// A server that records the target of every request and answers each with `status`, after
/// `delay`.
struct Server {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(status: Arc<Mutex<u16>>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a listener");
        let url = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let target = text.split(' ').nth(1).unwrap_or_default().to_string();
                recorded.lock().expect("the record").push(target);
                std::thread::sleep(delay);
                let code = *status.lock().expect("the status");
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });
        Self { url, seen }
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("the record").clone()
    }
}

/// Run `body` with a scratch state directory, the notice shown where `notice` says so, and the
/// ping pointed at `server`.
fn with_home(server: &Server, notice: bool, body: impl FnOnce(&Path)) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("a scratch home");
    let state = home.path().join(".bravebot");
    std::fs::create_dir_all(&state).expect("a state directory");
    if notice {
        std::fs::write(state.join("usage-ping-notice"), "shown\n").expect("the notice marker");
    }
    // SAFETY: every test in this file holds ENV_LOCK while the variables are set and read.
    unsafe {
        for variable in CLEARED {
            std::env::remove_var(variable);
        }
        std::env::set_var("HOME", home.path());
        std::env::set_var("BRAVEBOT_HOME", &state);
        std::env::set_var("BRAVEBOT_USAGE_PING_URL", &server.url);
    }
    body(&state);
}

fn ok_server() -> Server {
    Server::start(Arc::new(Mutex::new(200)), Duration::ZERO)
}

fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

/// One ping carries exactly the fields the service requires, and a second start the same day sends
/// nothing.
#[test]
fn one_ping_a_day_with_exactly_the_required_fields() {
    let server = ok_server();
    with_home(&server, true, |state| {
        assert_eq!(send(Surface::Tui, Backend::Brave, true), Outcome::Sent);
        let seen = server.seen();
        assert_eq!(seen.len(), 1, "{seen:?}");
        let (path, query) = seen[0].split_once('?').expect("a query");
        assert_eq!(path, "/1/usage/bravebot");
        let pairs: Vec<(&str, &str)> = query
            .split('&')
            .map(|pair| pair.split_once('=').expect("a pair"))
            .collect();
        let woi = pairs
            .iter()
            .find(|(key, _)| *key == "woi")
            .map(|(_, value)| *value)
            .expect("woi");
        let wanted = [
            ("platform", platform()),
            ("version", env!("CARGO_PKG_VERSION")),
            ("install", "unknown"),
            ("surface", "tui"),
            ("backend", "brave"),
            ("premium", "true"),
            ("daily", "true"),
            ("weekly", "true"),
            ("monthly", "true"),
            ("first", "true"),
            ("woi", woi),
        ];
        assert_eq!(pairs, wanted);
        assert_eq!(woi.len(), 10, "{woi}");

        let record = std::fs::read_to_string(state.join("usage-ping-state")).expect("a record");
        assert!(record.contains(&format!("woi {woi}\n")), "{record}");
        assert!(record.contains("success "), "{record}");

        assert_eq!(
            send(Surface::Desktop, Backend::None, false),
            Outcome::AlreadyToday
        );
        assert_eq!(server.seen().len(), 1);
    });
}

/// A refused ping is not retried within the hour, and is once the hour has passed.
#[test]
fn a_refused_ping_is_retried_after_an_hour() {
    let status = Arc::new(Mutex::new(400));
    let server = Server::start(Arc::clone(&status), Duration::ZERO);
    with_home(&server, true, |state| {
        assert_eq!(send(Surface::Cli, Backend::Other, false), Outcome::Failed);
        assert_eq!(send(Surface::Cli, Backend::Other, false), Outcome::Waiting);
        assert_eq!(server.seen().len(), 1);

        let path = state.join("usage-ping-state");
        let record = std::fs::read_to_string(&path).expect("a record");
        assert!(!record.contains("success "), "{record}");
        let moved: String = record
            .lines()
            .map(|line| match line.strip_prefix("attempt ") {
                Some(at) => format!("attempt {}\n", at.parse::<u64>().expect("seconds") - 3601),
                None => format!("{line}\n"),
            })
            .collect();
        std::fs::write(&path, moved).expect("the record moved back");

        *status.lock().expect("the status") = 204;
        assert_eq!(send(Surface::Cli, Backend::Other, false), Outcome::Sent);
        let seen = server.seen();
        assert_eq!(seen.len(), 2);
        assert!(seen[1].contains("&first=true&"), "{}", seen[1]);
    });
}

/// Starts at the same moment send one ping between them.
#[test]
fn starts_at_once_send_once() {
    let server = Server::start(Arc::new(Mutex::new(200)), Duration::from_millis(300));
    with_home(&server, true, |_| {
        let threads: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| send(Surface::Tui, Backend::Brave, false)))
            .collect();
        let outcomes: Vec<Outcome> = threads
            .into_iter()
            .map(|thread| thread.join().expect("a thread"))
            .collect();
        assert_eq!(server.seen().len(), 1, "{outcomes:?}");
        assert_eq!(
            outcomes.iter().filter(|o| **o == Outcome::Sent).count(),
            1,
            "{outcomes:?}"
        );
        assert!(
            outcomes
                .iter()
                .all(|o| matches!(o, Outcome::Sent | Outcome::Busy | Outcome::AlreadyToday)),
            "{outcomes:?}"
        );
    });
}

/// Nothing is sent or written while the notice has not been shown.
#[test]
fn nothing_goes_before_the_notice() {
    let server = ok_server();
    with_home(&server, false, |state| {
        assert_eq!(
            send(Surface::Tui, Backend::Brave, false),
            Outcome::NoticeNotShown
        );
        assert!(server.seen().is_empty());
        assert!(!state.join("usage-ping-state").exists());
    });
}

/// Nothing is sent or written once the switch or `DO_NOT_TRACK` turned the ping off.
#[test]
fn nothing_goes_when_the_ping_is_off() {
    let server = ok_server();
    with_home(&server, true, |state| {
        bravebot_session::store::save_usage_ping(false);
        assert_eq!(send(Surface::Tui, Backend::Brave, false), Outcome::Off);
        bravebot_session::store::save_usage_ping(true);
        // SAFETY: ENV_LOCK is held by `with_home`.
        unsafe { std::env::set_var("DO_NOT_TRACK", "1") };
        assert_eq!(send(Surface::Tui, Backend::Brave, false), Outcome::Off);
        assert!(server.seen().is_empty());
        assert!(!state.join("usage-ping-state").exists());
    });
}

/// A development build with no destination named sends nothing and records nothing, so a test that
/// starts the program or a `cargo run` is never counted.
#[cfg(debug_assertions)]
#[test]
fn a_development_build_with_no_destination_named_sends_nothing() {
    let server = ok_server();
    with_home(&server, true, |state| {
        // SAFETY: under ENV_LOCK, which `with_home` holds.
        unsafe { std::env::remove_var("BRAVEBOT_USAGE_PING_URL") };
        assert_eq!(
            send(Surface::Cli, Backend::Brave, false),
            Outcome::Unsupported
        );
        assert!(
            !state.join("usage-ping-state").exists(),
            "a record was written"
        );
        assert_eq!(server.seen(), Vec::<String>::new());
    });
}

/// What `at_startup` started can be waited for, so a run about to exit lets the request finish.
#[test]
fn a_ping_started_at_startup_can_be_waited_for() {
    let server = ok_server();
    with_home(&server, true, |_| {
        let sending = bravebot_usage_ping::at_startup(Surface::Cli, Backend::Brave, false);
        assert_eq!(sending.settle(Duration::from_secs(10)), Some(Outcome::Sent));
        assert_eq!(server.seen().len(), 1);
    });
}
