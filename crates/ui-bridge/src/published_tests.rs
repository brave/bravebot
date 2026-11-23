//! The exchange a desktop turn publishes for a `/btw` asked beside it
//! ([CMD-8](../../../docs/specs/commands.md#CMD-8)), kept on the session outside its state and
//! dropped when the next turn begins.

use super::test_profile as profile;
use super::*;
use crate::turn::Published;
use bravebot_agent::aside::Fork;
use bravebot_agent::report::Reporter;
use bravebot_core::file_authority::FileAuthority;

/// A turn's reporter builds the fork and keeps the latest one in the slot it was given. One given
/// no slot asks for no fork, so a manifest run does not build one at every round.
#[test]
fn a_turns_reporter_keeps_the_exchange_it_publishes() {
    let published = Published::default();
    let mut reporter = BridgeReporter::new(Emitter::new(Box::new(|_| {})), "s1")
        .publishing(published.exchange_slot());
    assert!(reporter.wants_fork());
    assert!(published.exchange().is_none());

    reporter.exchange_forked(Fork::of(&bravebot_agent::Conversation::new()));
    assert!(published.exchange().is_some());

    let unpublished = BridgeReporter::new(Emitter::new(Box::new(|_| {})), "s1");
    assert!(!unpublished.wants_fork());
}

/// `turn.send` empties what the last turn published before it says the turn has started, and the
/// new turn's worker fills both slots again: the exchange when it builds its first request and the
/// file authority it runs under. The regression it rejects is a `/btw` asked early in a turn being
/// answered over the exchange of the turn before.
#[test]
fn starting_a_turn_drops_what_the_last_one_published() {
    if !profile::in_isolated_profile() {
        return;
    }
    // A service that answers every request with a 500, sent once, so the turn builds one request
    // and ends.
    let service = std::net::TcpListener::bind("127.0.0.1:0").expect("a port on this machine");
    let port = service.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::Write;
        for mut connection in service.incoming().flatten() {
            let _ = connection.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let directory = tempfile::tempdir().expect("a project directory");
    let settings = directory.path().join("settings.json");
    std::fs::write(
        &settings,
        format!(
            r#"{{"provider":{{"local":{{"options":{{"baseURL":"http://127.0.0.1:{port}/v1","maxRetries":0}},"models":{{"m":{{}}}}}}}},"model":"local/m"}}"#
        ),
    )
    .unwrap();
    let project = directory.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let mut trust = TrustStore::new(&project);
    trust.trust(".");

    // What the slots held when the window was told the turn had started, which is before the
    // turn's worker is spawned.
    let watched: Arc<Mutex<Option<Published>>> = Arc::default();
    let seen: Arc<Mutex<Vec<(bool, bool)>>> = Arc::default();
    let (watching, noting) = (Arc::clone(&watched), Arc::clone(&seen));
    let mut bridge = Bridge::new(Box::new(move |event: Event| {
        if event.name == "turn.started"
            && let Some(published) = watching.lock().unwrap().as_ref()
        {
            noting
                .lock()
                .unwrap()
                .push((published.exchange().is_some(), published.files().is_some()));
        }
    }))
    .with_settings(Some(settings));
    let handle = bridge.mint(Open::opened(
        project.clone(),
        State::fresh(trust.clone()),
        true,
        None,
        false,
    ));
    let published = bridge.open.get(&handle).unwrap().published.clone();
    *published.exchange_slot().lock().unwrap() =
        Some(Fork::of(&bravebot_agent::Conversation::new()));
    published.keep_files(FileAuthority::new(trust));
    *watched.lock().unwrap() = Some(published.clone());

    let line = json!({"id": 1, "method": "turn.send", "params":
        crate::message_ids::with_fresh(json!({"session": handle, "prompt": "say hello"}))})
    .to_string();
    bridge
        .dispatch(&Request::parse(&line).unwrap())
        .unwrap_or_else(|failure| panic!("turn.send: {}", failure.message));
    assert_eq!(*seen.lock().unwrap(), [(false, false)]);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while published.exchange().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        published.exchange().is_some(),
        "the turn published its exchange"
    );
    assert!(
        published.files().is_some(),
        "the turn kept its file authority"
    );

    // The turn ends before its project directory is removed.
    let state = Arc::clone(&bridge.open.get(&handle).unwrap().state);
    while state.try_lock().is_err() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
