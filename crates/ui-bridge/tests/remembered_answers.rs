//! A question the person answered in a desktop session is not put to them again (ASK-8).
//!
//! Driven through the bridge against a scripted planner, because the clause is about a planner
//! that loops back across turns: the session that holds the memo, the turn that reads it and the
//! request the planner is sent next have to agree before the person is spared the question.
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Event, Failure, Request};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
    /// Every question the planner's series put to the window, in the order they were shown.
    shown: Vec<Value>,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "undo-test/test", "provider": {"undo-test": {
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
        let mut window = Self {
            bridge,
            events,
            shown: Vec::new(),
        };
        let session = window.open(project);
        (window, session)
    }

    /// Another session in the same window, on a project the person trusted. A project already
    /// trusted is not asked about again, and then there is no question to answer.
    fn open(&mut self, project: &Path) -> String {
        let made = self.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        let _ = self.dispatch("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        self.bridge.dispatch(&Request::parse(&line).unwrap())
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.dispatch(method, params)
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Send `prompt`, answer each series the planner puts with the next of `answers`, and return
    /// the turn's final event. A series with no answer left to give fails the test.
    fn turn(&mut self, session: &str, prompt: &str, answers: &[Value]) -> Event {
        let params = json!({"session": session, "prompt": prompt, "model": "undo-test/test"});
        self.once_finished(|window| window.dispatch("turn.send", params.clone()))
            .unwrap();
        let mut answers = answers.iter();
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            assert!(std::time::Instant::now() < until, "the turn never ended");
            let Ok(event) = self
                .events
                .recv_timeout(std::time::Duration::from_millis(10))
            else {
                continue;
            };
            match event.name {
                "ask.request" => {
                    let answer = answers
                        .next()
                        .unwrap_or_else(|| panic!("a series nobody had an answer for: {event:?}"));
                    self.shown.push(event.data.clone());
                    self.call(
                        "ask.reply",
                        json!({"session": session, "request": event.data["request"],
                            "answers": answer}),
                    );
                }
                "turn.done" | "turn.error" => return event,
                _ => {}
            }
        }
    }

    /// Retried while the worker that sent the last turn's final event lets the session go.
    fn once_finished(
        &mut self,
        mut request: impl FnMut(&mut Self) -> Result<Value, Failure>,
    ) -> Result<Value, Failure> {
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            match request(self) {
                Err(failure) if failure.code == ErrorCode::TurnInFlight => {
                    assert!(std::time::Instant::now() < until, "the turn never finished");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

/// A planner's `ask_user` call carrying one question per sentence, all under the tag `tag`.
fn asks(tag: &str, sentences: &[&str]) -> String {
    let questions: Vec<Value> = sentences
        .iter()
        .map(|sentence| {
            json!({"header": tag, "question": sentence,
                "options": [{"label": "HTTP"}, {"label": "Query"}]})
        })
        .collect();
    endpoint::tool("ask_user", json!({ "questions": questions }))
}

/// What the planner was told the person answered, as the last tool result of a request the
/// planner was sent says it.
fn told(request: &str) -> String {
    let body: Value = serde_json::from_str(request).unwrap();
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|message| message["role"] == "tool")
        .and_then(|message| message["content"].as_str())
        .unwrap_or_default()
        .to_string()
}

fn notices(done: &Event) -> Vec<String> {
    done.data["notices"]
        .as_array()
        .map(|notices| {
            notices
                .iter()
                .map(|notice| notice.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

const SECOND_OPTION: &str = r#"[{"chosen": [1]}]"#;

fn chosen_second() -> Vec<Value> {
    vec![serde_json::from_str(SECOND_OPTION).unwrap()]
}

#[test]
fn a_question_answered_in_one_turn_is_not_put_again_in_the_next() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("remembered-answer-next-turn");
    let (config, requests, server) = endpoint::endpoint(
        vec![
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let first = window.turn(&session, "pick a cache", &chosen_second());
    let second = window.turn(&session, "pick it again", &[]);
    server.join().unwrap();

    assert_eq!(first.name, "turn.done", "{:?}", first.data);
    assert_eq!(second.name, "turn.done", "{:?}", second.data);
    assert_eq!(
        window.shown.len(),
        1,
        "the second turn put the question again: {:?}",
        window.shown
    );
    let sent: Vec<String> = requests.try_iter().collect();
    assert_eq!(
        told(&sent[3]),
        told(&sent[1]),
        "the planner was told something else the second time"
    );
    assert!(told(&sent[3]).contains("The user chose: Query"));
    assert_eq!(
        notices(&second),
        vec!["answered already: Which layer?".to_string()],
        "the person was not told the answer was reused"
    );
    assert!(notices(&first).is_empty(), "{:?}", first.data);
}

#[test]
fn a_series_with_one_question_settled_puts_only_the_other() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("remembered-answer-partial-series");
    let (config, requests, server) = endpoint::endpoint(
        vec![
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
            asks("Cache", &["Which store?", "Which layer?"]),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    window.turn(&session, "pick a layer", &chosen_second());
    let second = window.turn(
        &session,
        "pick a store and a layer",
        &[json!([{"typed": "redis"}])],
    );
    server.join().unwrap();

    assert_eq!(second.name, "turn.done", "{:?}", second.data);
    let put = window.shown[1]["prompts"].as_array().unwrap();
    assert_eq!(put.len(), 1, "{:?}", window.shown[1]);
    assert_eq!(put[0]["question"], "Which store?");
    let sent: Vec<String> = requests.try_iter().collect();
    let said = told(&sent[3]);
    assert!(
        said.find("The user answered: redis") < said.find("The user chose: Query"),
        "the answers were not reported in the order the questions were asked: {said}"
    );
}

#[test]
fn a_question_that_differs_only_in_its_tag_is_put_again() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("remembered-answer-other-tag");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
            asks("Caching", &["Which layer?"]),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    window.turn(&session, "pick a cache", &chosen_second());
    window.turn(&session, "pick one more", &chosen_second());
    server.join().unwrap();

    assert_eq!(window.shown.len(), 2, "{:?}", window.shown);
}

#[test]
fn another_session_is_not_answered_from_the_first_ones_memory() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("remembered-answer-other-session");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.turn(&session, "pick a cache", &chosen_second());

    let other = window.open(&project);
    window.turn(&other, "pick a cache here", &chosen_second());
    server.join().unwrap();

    assert_eq!(
        window.shown.len(),
        2,
        "a session began with another's answers: {:?}",
        window.shown
    );
}

/// A turn that ends in an error still names what it spared the person. The notices of a failed
/// turn are read from the reporter, and the questions it did not put are held by the confirmer.
#[test]
fn a_turn_that_fails_after_a_remembered_answer_still_names_the_question_it_did_not_put() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("remembered-answer-failed-turn");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            asks("Cache", &["Which layer?"]),
            endpoint::answer(),
            asks("Cache", &["Which layer?"]),
            "fail".into(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    window.turn(&session, "pick a cache", &chosen_second());
    let failed = window.turn(&session, "pick it again", &[]);
    server.join().unwrap();

    assert_eq!(failed.name, "turn.error", "{:?}", failed.data);
    assert_eq!(
        notices(&failed),
        vec!["answered already: Which layer?".to_string()]
    );
    assert_eq!(window.shown.len(), 1, "{:?}", window.shown);
}
