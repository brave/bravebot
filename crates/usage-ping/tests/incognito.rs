//! An incognito session sends no ping and writes no record.
//!
//! A file of its own, because incognito mode cannot be turned off again in a process.

use std::net::TcpListener;

use bravebot_usage_ping::{Backend, Outcome, Surface, send};

#[test]
fn an_incognito_session_sends_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a listener");
    listener.set_nonblocking(true).expect("a polling listener");
    let home = tempfile::tempdir().expect("a scratch home");
    let state = home.path().join(".bravebot");
    std::fs::create_dir_all(&state).expect("a state directory");
    std::fs::write(state.join("usage-ping-notice"), "shown\n").expect("the notice marker");
    let url = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("an address").port()
    );
    // SAFETY: the only test in this process, and it sets these before anything reads them.
    unsafe {
        std::env::remove_var("BRAVEBOT_USAGE_PING");
        std::env::remove_var("DO_NOT_TRACK");
        std::env::set_var("HOME", home.path());
        std::env::set_var("BRAVEBOT_HOME", &state);
        std::env::set_var("BRAVEBOT_USAGE_PING_URL", &url);
    }
    bravebot_core::incognito::engage();

    assert_eq!(send(Surface::Tui, Backend::Brave, false), Outcome::Off);
    assert!(listener.accept().is_err(), "a connection was made");
    assert!(!state.join("usage-ping-state").exists());
}
