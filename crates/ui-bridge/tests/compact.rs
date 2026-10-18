//! `session.compact`: a desktop session's earlier conversation is summarised when asked, as the
//! terminal's `/compact` summarises it.

#[allow(dead_code)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Event, Failure, Request};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
}

impl Window {
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "compact-test/test", "provider": {"compact-test": {
                "options": {"baseURL": endpoint}, "models": {"test": {}}
            }}})
            .to_string(),
        )
        .unwrap();
        let (sent, events) = mpsc::channel();
        let bridge = Bridge::new(Box::new(move |event| {
            let _ = sent.send(event);
        }))
        .with_settings(Some(settings));
        let mut window = Self { bridge, events };
        let made = window.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        (window, session)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        self.bridge.dispatch(&Request::parse(&line).unwrap())
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.dispatch(method, params)
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Retried while the worker that sent the last final event lets the session go.
    fn once_free(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let until = Instant::now() + endpoint::LIMIT;
        loop {
            match self.dispatch(method, params.clone()) {
                Err(failure) if failure.code == ErrorCode::TurnInFlight => {
                    assert!(Instant::now() < until, "the session never came free");
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }

    fn turn(&mut self, session: &str, prompt: &str) -> Event {
        self.once_free(
            "turn.send",
            json!({"session": session, "prompt": prompt, "model": "compact-test/test"}),
        )
        .unwrap();
        self.until(&["turn.done", "turn.error"])
    }

    fn compact(&mut self, session: &str) -> Event {
        self.once_free("session.compact", json!({"session": session}))
            .unwrap();
        self.until(&["compact.done", "compact.error"])
    }

    /// Every event up to the first named one, which is returned.
    fn until(&mut self, names: &[&str]) -> Event {
        let until = Instant::now() + endpoint::LIMIT;
        loop {
            assert!(Instant::now() < until, "none of {names:?} arrived");
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(10))
                && names.contains(&event.name)
            {
                return event;
            }
        }
    }
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

/// Enough exchanges that the earlier two outweigh the two a summary keeps word for word.
const TALK: [&str; 4] = [
    "tell me how the parser reads a quoted string with escapes",
    "and how does it recover after an unterminated one",
    "thanks",
    "ok",
];

/// The endpoint's replies: one answer for each exchange in [`TALK`], then `then`.
fn after_talking(then: Vec<String>) -> Vec<String> {
    let mut replies = vec![endpoint::answer(); TALK.len()];
    replies.extend(then);
    replies
}

impl Window {
    fn talk(&mut self, session: &str) {
        for prompt in TALK {
            let done = self.turn(session, prompt);
            assert_eq!(done.name, "turn.done", "{:?}", done.data);
        }
    }
}

#[test]
fn compacting_summarises_the_earlier_exchanges_and_the_next_turn_carries_the_summary() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("compact-summarises");
    let summary = "data: {\"choices\":[{\"delta\":{\"content\":\"We talked about the parser.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_string();
    let (config, requests, server) =
        endpoint::endpoint(after_talking(vec![summary, endpoint::answer()]), None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.talk(&session);

    let done = window.compact(&session);
    assert_eq!(done.name, "compact.done", "{:?}", done.data);
    assert_eq!(done.data["compacted"], true);
    assert!(
        done.data["summarised"].as_u64().unwrap() > 0,
        "{:?}",
        done.data
    );
    assert!(
        done.data["archived"].as_u64().unwrap() > 0,
        "{:?}",
        done.data
    );
    assert_eq!(
        done.data["rewind"],
        json!([]),
        "a point from before the summary is still offered"
    );

    let next = window.turn(&session, "afterwards");
    server.join().unwrap();
    assert_eq!(next.name, "turn.done", "{:?}", next.data);
    let sent: Vec<String> = requests.try_iter().collect();
    assert_eq!(sent.len(), TALK.len() + 2, "{sent:?}");
    let last = sent.last().unwrap();
    assert!(
        !last.contains(TALK[0]),
        "the summarised prompt was still sent word for word"
    );
    assert!(
        last.contains("We talked about the parser."),
        "the summary was not sent"
    );
    assert!(
        window
            .dispatch("session.rewind", json!({"session": session, "steps": 2}))
            .is_err(),
        "rewinding reached back past the summary"
    );
}

#[test]
fn a_session_with_nothing_old_enough_to_summarise_is_told_so_and_nothing_is_sent() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("compact-nothing");
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.turn(&session, "only");
    server.join().unwrap();

    let done = window.compact(&session);
    assert_eq!(done.name, "compact.done", "{:?}", done.data);
    assert_eq!(done.data["compacted"], false);
    assert_eq!(requests.try_iter().count(), 1, "a summary was asked for");
}

#[test]
fn a_summary_that_fails_says_why_and_leaves_the_session_free() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("compact-fails");
    let (config, _requests, server) =
        endpoint::endpoint(after_talking(vec!["fail".into(), endpoint::answer()]), None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.talk(&session);

    let failed = window.compact(&session);
    assert_eq!(failed.name, "compact.error", "{:?}", failed.data);
    assert_eq!(failed.data["stopped"], false);
    assert!(
        failed.data["category"]
            .as_str()
            .is_some_and(|c| !c.is_empty())
    );

    let next = window.turn(&session, "afterwards");
    server.join().unwrap();
    assert_eq!(next.name, "turn.done", "{:?}", next.data);
}

#[test]
fn a_summary_can_be_stopped_and_a_turn_is_refused_while_it_runs() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("compact-stopped");
    // The reply after the held one is never asked for; while the endpoint waits for it, it keeps
    // the summary's connection open.
    let (config, requests, _server) =
        endpoint::endpoint(after_talking(vec!["hold".into(), endpoint::answer()]), None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.talk(&session);

    window
        .once_free("session.compact", json!({"session": session}))
        .unwrap();
    let refused = window
        .dispatch(
            "turn.send",
            json!({"session": session, "prompt": "too soon"}),
        )
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::TurnInFlight);
    for _ in 0..=TALK.len() {
        requests.recv_timeout(endpoint::LIMIT).unwrap();
    }

    window.call("turn.cancel", json!({"session": session}));
    let stopped = window.until(&["compact.done", "compact.error"]);
    assert_eq!(stopped.name, "compact.error", "{:?}", stopped.data);
    assert_eq!(stopped.data["stopped"], true);
    assert_eq!(stopped.data["category"], "cancelled");
}
