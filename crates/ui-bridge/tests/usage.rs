//! `session.usage`: what a desktop session has spent, in tokens and time, as the terminal's
//! `/cost` and `/status` report it.

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
    fn with_settings(project: &Path, endpoint: &str) -> Self {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "usage-test/test", "provider": {"usage-test": {
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
        Self { bridge, events }
    }

    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let mut window = Self::with_settings(project, endpoint);
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
            json!({"session": session, "prompt": prompt, "model": "usage-test/test"}),
        )
        .unwrap();
        let done = self.until(&["turn.done", "turn.error"]);
        assert_eq!(done.name, "turn.done", "{:?}", done.data);
        done
    }

    fn usage(&mut self, session: &str) -> Value {
        self.once_free("session.usage", json!({"session": session}))
            .unwrap()
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

/// An answer whose last chunk reports what the request cost, part of it read from the cache.
fn answer_costing(prompt: u64, completion: u64, cached: u64) -> String {
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"done\"}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":{prompt},\"completion_tokens\":{completion},\"prompt_tokens_details\":{{\"cached_tokens\":{cached}}}}}}}\n\ndata: [DONE]\n\n"
    )
}

fn by_turn_sum(usage: &Value) -> u64 {
    usage["byTurn"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["tokens"].as_u64().unwrap())
        .sum()
}

#[test]
fn a_session_reports_each_turn_and_a_total_the_turns_add_up_to() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("usage-two-turns");
    let (config, _requests, server) = endpoint::endpoint(
        vec![answer_costing(100, 5, 0), answer_costing(300, 7, 90)],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let before = window.usage(&session);
    assert_eq!(before["turns"], 0);
    assert_eq!(before["tokens"], 0);
    assert!(before.get("cached").is_none(), "{before:?}");

    window.turn(&session, "first");
    window.turn(&session, "second");
    server.join().unwrap();

    let usage = window.usage(&session);
    assert_eq!(usage["turns"], 2, "{usage:?}");
    let total = usage["tokens"].as_u64().unwrap();
    assert!(total > 0, "{usage:?}");
    assert_eq!(
        total,
        by_turn_sum(&usage) + usage["unattributed"].as_u64().unwrap(),
        "the breakdown does not add up to the total: {usage:?}"
    );
    let turns: Vec<u64> = usage["byTurn"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["turn"].as_u64().unwrap())
        .collect();
    assert_eq!(turns, [1, 2], "{usage:?}");
    assert!(usage["timing"]["wallMs"].is_u64(), "{usage:?}");
    assert_eq!(
        usage["cached"]["read"], 90,
        "the last turn's cache read is not reported: {usage:?}"
    );
}

#[test]
fn a_reopened_session_reports_what_it_spent_before() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("usage-reopened");
    let (config, _requests, server) = endpoint::endpoint(vec![answer_costing(120, 4, 50)], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    let done = window.turn(&session, "first");
    server.join().unwrap();
    let id = done.data["id"].as_str().unwrap().to_string();
    let spent = window.usage(&session);
    window.call("session.close", json!({"session": session}));
    assert!(
        window
            .dispatch("session.usage", json!({"session": session}))
            .is_err(),
        "a closed session still reported usage"
    );

    let mut reopened = Window::with_settings(&project, &config.endpoint);
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));
    let again = reopened.usage(opened["session"].as_str().unwrap());
    for field in ["turns", "tokens", "byTurn", "unattributed", "timing"] {
        assert_eq!(
            again[field], spent[field],
            "{field}: {again:?} against {spent:?}"
        );
    }
    assert!(
        again.get("cached").is_none(),
        "the record keeps no cache split, so a reopened session has none: {again:?}"
    );
}
