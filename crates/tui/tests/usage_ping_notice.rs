//! The one-time notice about the usage ping, through the calls the startup screen, the runs without
//! one and the desktop bridge make.
//!
//! In a process of its own because the home directory and the variables that turn the ping off are
//! the process's.

use bravebot_config::env_var;
use bravebot_session::store;

/// Point HOME at a scratch directory with neither variable that turns the ping off set.
///
/// Every test here takes the lock, since HOME and the variables are process-wide.
fn with_home(name: &str, body: impl FnOnce(&std::path::Path)) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("bravebot-notice-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");
    // SAFETY: single-threaded within the lock. The variables are this test binary's alone.
    unsafe {
        std::env::set_var("HOME", &dir);
        std::env::remove_var("BRAVEBOT_HOME");
        std::env::remove_var(env_var::USAGE_PING);
        std::env::remove_var(env_var::DO_NOT_TRACK);
    }
    body(&dir.join(".bravebot"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// With the ping on, the notice is due once: the first take returns the line and records it, and
/// every take after it, in any front end, returns nothing. The line names what is sent and every
/// way of turning it off, since a person told only one way may be using a front end without it.
#[test]
fn the_notice_is_said_once_and_names_what_is_sent_and_how_to_stop_it() {
    with_home("once", |state| {
        assert!(!store::usage_ping_notice_shown(), "a new home has seen it");
        assert!(store::usage_ping_notice_due());

        let line = store::take_usage_ping_notice().expect("the first start said nothing");
        for words in [
            "Brave",
            "version",
            "operating system",
            "installed",
            "premium",
            "/config",
            "usagePing",
            "BRAVEBOT_USAGE_PING=0",
        ] {
            assert!(line.contains(words), "{words:?} missing from {line}");
        }

        assert_eq!(store::take_usage_ping_notice(), None, "said twice");
        assert!(store::usage_ping_notice_shown());
        assert!(!store::usage_ping_notice_due());
        assert!(
            state.join("usage-ping-notice").exists(),
            "nothing was left to stop the next start saying it"
        );
    });
}

/// With the ping off the notice is not due and nothing is recorded, so it is said before the first
/// ping once the person turns it on. Recording it while off would let that first ping go with
/// nobody told.
#[test]
fn the_notice_waits_while_the_ping_is_off_and_is_due_once_it_is_turned_on() {
    with_home("off", |state| {
        store::save_usage_ping(false);
        assert!(!store::usage_ping_notice_due());
        assert_eq!(store::take_usage_ping_notice(), None);
        assert!(!state.join("usage-ping-notice").exists());

        // SAFETY: under the lock `with_home` holds.
        unsafe { std::env::set_var(env_var::DO_NOT_TRACK, "1") };
        store::save_usage_ping(true);
        assert_eq!(
            store::take_usage_ping_notice(),
            None,
            "said with DO_NOT_TRACK set"
        );
        unsafe { std::env::remove_var(env_var::DO_NOT_TRACK) };

        assert!(
            store::take_usage_ping_notice().is_some(),
            "not said once turned on"
        );
    });
}

/// The desktop records the notice through the mark alone, and a start in the terminal then says
/// nothing, since the marker is one for every front end.
#[test]
fn a_notice_marked_shown_by_another_front_end_is_not_said_again() {
    with_home("marked", |_| {
        store::mark_usage_ping_notice_shown();
        assert!(store::usage_ping_notice_shown());
        assert_eq!(store::take_usage_ping_notice(), None);
    });
}
