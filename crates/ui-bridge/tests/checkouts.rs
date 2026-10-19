//! A checkout a desktop turn's delegate was given outlives the turn that made it, is written into
//! the session's record, and is taken back when the session is opened again (CHECKOUT-16).

#[allow(dead_code)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Failure, Request};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
    settings: PathBuf,
    /// The write questions the turns so far put, as they were put.
    asked: Vec<Value>,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "checkout-test/test", "provider": {"checkout-test": {
                "options": {"baseURL": endpoint}, "models": {"test": {}}
            }}})
            .to_string(),
        )
        .unwrap();
        let mut window = Self::with_settings(settings);
        let made = window.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        (window, session)
    }

    fn with_settings(settings: PathBuf) -> Self {
        let (sent, events) = mpsc::channel();
        let bridge = Bridge::new(Box::new(move |event| {
            let _ = sent.send(event);
        }))
        .with_settings(Some(settings.clone()));
        Self {
            bridge,
            events,
            settings,
            asked: Vec::new(),
        }
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        self.bridge.dispatch(&Request::parse(&line).unwrap())
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.dispatch(method, params)
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Send `prompt`, approve every question, and return the turn's final event.
    fn turn(&mut self, session: &str, prompt: &str) -> Event {
        self.call("turn.send", json!({"session": session, "prompt": prompt}));
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
                "confirm.request" => {
                    self.asked.push(event.data.clone());
                    self.call(
                        "confirm.reply",
                        json!({"session": session, "request": event.data["request"], "decision": "approve"}),
                    );
                }
                "turn.done" | "turn.error" => return event,
                _ => {}
            }
        }
    }
}

/// A project that is a git repository with one commit, so a delegate can be given a checkout of it.
fn repository(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    let project = std::fs::canonicalize(project).unwrap();
    std::fs::write(project.join("README"), "committed\n").unwrap();
    let git = |arguments: &[&str]| {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(arguments)
            .current_dir(&project)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {arguments:?} failed");
    };
    git(&["init", "--quiet"]);
    git(&["add", "README"]);
    git(&["commit", "--quiet", "-m", "first"]);
    project
}

/// Serves the planner and the delegates it starts from one endpoint. A planner does not wait for the
/// worker it asks for, so the order the requests arrive in is not the order the replies were
/// written in; each request is told apart by the marker in its body instead.
///
/// The first rule whose marker is in the body and that has a reply left answers. A planner replays
/// the task it handed out, so a turn's own marker goes before the markers of its workers' tasks.
fn serve(rules: Vec<(&'static str, Vec<String>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let waiting = Arc::new(Mutex::new(
        rules
            .into_iter()
            .map(|(marker, replies)| (marker, VecDeque::from(replies)))
            .collect::<Vec<_>>(),
    ));
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let waiting = Arc::clone(&waiting);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                let reply = if body.contains("You are a prompt-injection classifier.") {
                    Some(endpoint::answer())
                } else {
                    waiting
                        .lock()
                        .unwrap()
                        .iter_mut()
                        .filter(|(marker, _)| body.contains(marker))
                        .find_map(|(_, replies)| replies.pop_front())
                };
                let mut stream = reader.into_inner();
                let response = match reply {
                    Some(reply) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    ),
                    None => "HTTP/1.1 418 Teapot\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .into(),
                };
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    address
}

/// What one turn takes from the endpoint: a planner that asks for a worker in a checkout, waits for
/// it and ends, and a worker that writes `file` there. `ask` is in the prompt the person types and
/// `task` in the worker's, and neither is in the other's.
fn a_worker_writing(
    ask: &'static str,
    task: &'static str,
    file: &str,
) -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            ask,
            vec![
                endpoint::tool(
                    "spawn_agent",
                    json!({"kind": "worker", "task": task, "isolation": "checkout"}),
                ),
                endpoint::answer(),
                endpoint::answer(),
            ],
        ),
        (
            task,
            vec![
                endpoint::tool(
                    "write_file",
                    json!({"path": file, "contents": format!("from the delegate: {file}")}),
                ),
                endpoint::answer(),
            ],
        ),
    ]
}

/// What the record at `project` lists as kept checkouts: each one's number and the names written in
/// it.
fn kept(project: &Path, id: &str) -> Vec<(String, Vec<String>)> {
    let record = bravebot_ui_bridge::store::load(project, id).expect("the session's record");
    record
        .checkouts
        .iter()
        .map(|one| (one.id.clone(), one.written.clone()))
        .collect()
}

/// Where the session's state directory puts checkout `id` of `project`.
fn standing(project: &Path, id: &str) -> PathBuf {
    bravebot_agent::home::directory()
        .expect("a state directory")
        .join("checkouts")
        .join(bravebot_agent::home::key_for(project))
        .join(id)
}

fn session_id(done: &Event) -> String {
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    done.data["id"]
        .as_str()
        .expect("the record's id")
        .to_string()
}

/// CHECKOUT-16: the checkout a worker wrote in is in the record the turn saved, with the name it
/// wrote and the commit it holds, and its files are where the record says.
#[test]
fn a_checkout_a_worker_wrote_in_is_in_the_record_the_turn_saved() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-kept-by-a-turn");
    let endpoint = serve(a_worker_writing(
        "ASK-FOR-OUT",
        "WORKER-WRITES-OUT",
        "out.txt",
    ));
    let (mut window, session) = Window::on(&project, &endpoint);

    let done = window.turn(&session, "ASK-FOR-OUT");

    let id = session_id(&done);
    assert_eq!(
        std::fs::read_to_string(standing(&project, "c1").join("out.txt")).unwrap(),
        "from the delegate: out.txt",
        "the worker's file is not in the checkout"
    );
    assert_eq!(
        kept(&project, &id),
        [("c1".to_string(), vec!["out.txt".to_string()])],
        "the record does not list the checkout the worker wrote in"
    );
    let record = bravebot_ui_bridge::store::load(&project, &id).unwrap();
    let one = &record.checkouts[0];
    assert!(one.worked_in);
    assert_eq!(one.commit.len(), 40, "the commit is a full object id");
    assert_eq!(
        std::fs::read_to_string(Path::new(&one.path).join("out.txt")).unwrap(),
        "from the delegate: out.txt"
    );
}

/// CHECKOUT-16: a second turn's worker is given a checkout of its own beside the first turn's. The
/// workspace a turn reads through is built per turn, so without the first one's number carried
/// forward the second would be asked to make `c1` where `c1` already stands.
#[test]
fn a_second_turns_worker_gets_the_next_checkout_and_the_first_stays_listed() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-kept-across-turns");
    let mut rules = a_worker_writing("ASK-FOR-ONE", "WORKER-WRITES-ONE", "one.txt");
    rules.extend(a_worker_writing(
        "ASK-FOR-TWO",
        "WORKER-WRITES-TWO",
        "two.txt",
    ));
    let endpoint = serve(rules);
    let (mut window, session) = Window::on(&project, &endpoint);

    let first = window.turn(&session, "ASK-FOR-ONE");
    let second = window.turn(&session, "ASK-FOR-TWO");

    let id = session_id(&first);
    assert_eq!(session_id(&second), id, "both turns are one session");
    assert_eq!(
        kept(&project, &id),
        [
            ("c1".to_string(), vec!["one.txt".to_string()]),
            ("c2".to_string(), vec!["two.txt".to_string()]),
        ]
    );
}

/// CHECKOUT-16: a session opened again takes its record's checkouts back, so the worker it starts
/// next is given the next number and the record goes on listing the first.
#[test]
fn a_reopened_session_keeps_its_checkout_and_numbers_the_next_after_it() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-kept-across-a-reopen");
    let mut rules = a_worker_writing("ASK-FOR-ONE", "WORKER-WRITES-ONE", "one.txt");
    rules.extend(a_worker_writing(
        "ASK-FOR-TWO",
        "WORKER-WRITES-TWO",
        "two.txt",
    ));
    let endpoint = serve(rules);
    let (mut window, session) = Window::on(&project, &endpoint);
    let first = window.turn(&session, "ASK-FOR-ONE");
    let id = session_id(&first);

    let mut reopened = Window::with_settings(window.settings.clone());
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));
    let session = opened["session"].as_str().unwrap().to_string();
    let second = reopened.turn(&session, "ASK-FOR-TWO");

    assert_eq!(
        session_id(&second),
        id,
        "the reopened session writes the same record"
    );
    assert_eq!(
        kept(&project, &id),
        [
            ("c1".to_string(), vec!["one.txt".to_string()]),
            ("c2".to_string(), vec!["two.txt".to_string()]),
        ]
    );
}

/// CHECKOUT-16: a record naming a checkout whose directory is gone opens, and the records the
/// session saves from then on go on naming it, so a session that can reach it finds it. The
/// worker the next turn starts is given a number of its own, and the record names both.
#[test]
fn a_reopened_session_keeps_a_checkout_it_cannot_place_in_the_record_and_numbers_past_it() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-gone-before-a-reopen");
    let mut rules = a_worker_writing("ASK-FOR-ONE", "WORKER-WRITES-ONE", "one.txt");
    rules.extend(a_worker_writing(
        "ASK-FOR-TWO",
        "WORKER-WRITES-TWO",
        "two.txt",
    ));
    let endpoint = serve(rules);
    let (mut window, session) = Window::on(&project, &endpoint);
    let first = window.turn(&session, "ASK-FOR-ONE");
    let id = session_id(&first);
    std::fs::remove_dir_all(standing(&project, "c1")).unwrap();

    let mut reopened = Window::with_settings(window.settings.clone());
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));
    let session = opened["session"].as_str().unwrap().to_string();
    let second = reopened.turn(&session, "ASK-FOR-TWO");

    assert_eq!(session_id(&second), id);
    let mut recorded = kept(&project, &id);
    recorded.sort();
    assert_eq!(
        recorded,
        [
            ("c1".to_string(), vec!["one.txt".to_string()]),
            ("c2".to_string(), vec!["two.txt".to_string()]),
        ],
        "the record lost the checkout that could not be placed, or the new worker was given its number"
    );
}

/// CHECKOUT-16: a fork made in the window starts with no checkouts, and the session it was cut from
/// goes on listing the one its worker made. Two records listing one directory would each be a
/// reason to keep it, and neither session would know the other still used it.
#[test]
fn a_fork_starts_with_no_checkout_and_the_session_it_was_cut_from_keeps_its_own() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-fork");
    let mut rules = a_worker_writing("ASK-FOR-ONE", "WORKER-WRITES-ONE", "one.txt");
    rules.push(("SAY-HELLO", vec![endpoint::answer()]));
    let endpoint = serve(rules);
    let (mut window, session) = Window::on(&project, &endpoint);
    let first = window.turn(&session, "ASK-FOR-ONE");
    let parent = session_id(&first);

    let forked = window.call(
        "session.fork",
        json!({"session": session, "prompt": 0, "text": "ASK-FOR-ONE"}),
    );
    let child = forked["session"].as_str().unwrap().to_string();
    let second = window.turn(&child, "SAY-HELLO");

    let fork = session_id(&second);
    assert_ne!(fork, parent, "the fork wrote the parent's record");
    assert_eq!(
        kept(&project, &fork),
        [],
        "the fork lists a checkout it was not given"
    );
    assert_eq!(
        kept(&project, &parent),
        [("c1".to_string(), vec!["one.txt".to_string()])],
        "forking took the checkout from the session it was cut from"
    );
}

/// CHECKOUT-14: the note that the working directory's file was written since the checkout was made
/// survives the end of the turn the checkout was made in. A checkout brought back by a later turn
/// is judged against every write the session made, so the question about `out.txt` says the planner
/// wrote it after the worker's checkout existed.
#[test]
fn a_later_turn_is_told_the_working_directory_was_written_since_the_checkout() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-written-since-across-turns");
    let mut rules = a_worker_writing("ASK-FOR-OUT", "WORKER-WRITES-OUT", "out.txt");
    rules[0].1.insert(
        2,
        endpoint::tool(
            "write_file",
            json!({"path": "out.txt", "contents": "written by the planner"}),
        ),
    );
    rules.push((
        "ASK-TO-APPLY",
        vec![
            endpoint::tool("apply_checkout", json!({"checkout": "c1"})),
            endpoint::answer(),
        ],
    ));
    let endpoint = serve(rules);
    let (mut window, session) = Window::on(&project, &endpoint);
    let first = window.turn(&session, "ASK-FOR-OUT");
    assert_eq!(first.name, "turn.done", "{:?}", first.data);
    window.asked.clear();

    let second = window.turn(&session, "ASK-TO-APPLY");

    assert_eq!(second.name, "turn.done", "{:?}", second.data);
    let applies: Vec<&Value> = window
        .asked
        .iter()
        .filter(|question| question["path"] == "out.txt")
        .collect();
    assert_eq!(applies.len(), 1, "the apply asked {:?}", window.asked);
    assert_eq!(
        applies[0]["writtenSinceCheckout"], true,
        "the question does not say the working directory's file was written since the checkout"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("out.txt")).unwrap(),
        "from the delegate: out.txt"
    );
}

/// CHECKOUT-16: between turns the session still holds the lock on a checkout, so another session's
/// opening sweep does not take it even when no record lists it.
#[cfg(unix)]
#[test]
fn a_checkout_stays_locked_between_turns() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = repository("checkout-locked-between-turns");
    let endpoint = serve(a_worker_writing(
        "ASK-FOR-OUT",
        "WORKER-WRITES-OUT",
        "out.txt",
    ));
    let (mut window, session) = Window::on(&project, &endpoint);
    let done = window.turn(&session, "ASK-FOR-OUT");
    assert_eq!(done.name, "turn.done", "{:?}", done.data);

    let sweeper = bravebot_agent::Workspace::new(&project).unwrap();
    let removed = sweeper.sweep_checkouts(&bravebot_agent::home::directory().unwrap(), &|_| false);

    assert!(removed.is_empty(), "a sweep took {removed:?}");
    assert!(standing(&project, "c1").join("out.txt").exists());
}
