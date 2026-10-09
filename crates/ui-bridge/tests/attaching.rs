//! `turn.send` carries a pasted picture as `images` and a dropped picture or PDF as `attachments`
//! (PASTE-2, PASTE-3, DROP-10).

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

/// The eight bytes a PNG starts with, then a little more so the payload is not only a signature.
const PICTURE: &[u8] = b"\x89PNG\r\n\x1a\nnot-really-pixels";
const PICTURE_BASE64: &str = "iVBORw0KGgpub3QtcmVhbGx5LXBpeGVscw==";

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
    settings: PathBuf,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "attach-test/test", "provider": {"attach-test": {
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

    /// Send a turn carrying `extra` beside the prompt and return its final event.
    fn turn(&mut self, session: &str, prompt: &str, extra: Value) -> Event {
        let mut params = json!({"session": session, "prompt": prompt});
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        self.call("turn.send", params);
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            assert!(std::time::Instant::now() < until, "the turn never ended");
            let Ok(event) = self
                .events
                .recv_timeout(std::time::Duration::from_millis(10))
            else {
                continue;
            };
            if matches!(event.name, "turn.done" | "turn.error") {
                return event;
            }
        }
    }

    /// How many events named `name` have been raised and not yet read.
    fn raised(&self, name: &str) -> usize {
        self.events.try_iter().filter(|e| e.name == name).count()
    }
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

/// A directory outside the project, as a drop usually comes from.
fn elsewhere(name: &str) -> PathBuf {
    let elsewhere = bravebot_agent::home::profile()
        .unwrap()
        .join("elsewhere")
        .join(name);
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::canonicalize(elsewhere).unwrap()
}

/// The user message holding `prompt` in a request the model was sent.
fn user_message(body: &str, prompt: &str) -> Value {
    let request: Value = serde_json::from_str(body).unwrap();
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|message| {
            message["role"] == "user"
                && message["content"]
                    .as_array()
                    .is_some_and(|parts| parts.iter().any(|part| part["text"] == prompt))
        })
        .unwrap_or_else(|| panic!("no user message carries {prompt:?} with parts: {body}"))
        .clone()
}

/// The `data:` URL of every picture part in a message, in order.
fn pictures(message: &Value) -> Vec<String> {
    message["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part["type"] == "image_url")
        .map(|part| part["image_url"]["url"].as_str().unwrap().to_string())
        .collect()
}

fn data_url(media: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:{media};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// PASTE-2, PASTE-4: a pasted picture reaches the model inline, in the message holding the prompt.
#[test]
fn a_pasted_picture_reaches_the_model_with_the_prompt() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("paste");
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let done = window.turn(
        &session,
        "what is in this screenshot?",
        json!({"images": [{"media": "image/png", "data": PICTURE_BASE64}]}),
    );
    server.join().unwrap();

    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let sent = requests.recv().unwrap();
    let message = user_message(&sent, "what is in this screenshot?");
    assert_eq!(pictures(&message), [data_url("image/png", PICTURE)]);
}

/// DROP-10: a dropped picture is carried as bytes, and a dropped PDF is typed from its extension.
/// Dropped files go before pasted pictures in the message, as the terminal sends them.
#[test]
fn a_dropped_picture_and_pdf_reach_the_model_before_a_pasted_picture() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("drop");
    let outside = elsewhere("drop");
    let shot = outside.join("Shot.PNG");
    let paper = outside.join("paper.pdf");
    std::fs::write(&shot, b"dropped picture").unwrap();
    std::fs::write(&paper, b"%PDF-1.4 dropped").unwrap();
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let done = window.turn(
        &session,
        "compare these",
        json!({
            "images": [{"media": "image/gif", "data": PICTURE_BASE64}],
            "attachments": [shot, paper],
        }),
    );
    server.join().unwrap();

    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let message = user_message(&requests.recv().unwrap(), "compare these");
    assert_eq!(
        pictures(&message),
        [
            data_url("image/png", b"dropped picture"),
            data_url("application/pdf", b"%PDF-1.4 dropped"),
            data_url("image/gif", PICTURE),
        ]
    );
}

/// PASTE-9: a pasted picture is written into the session record, so a turn sent after the session
/// is opened again still carries it in the conversation.
#[test]
fn a_pasted_picture_is_still_in_the_conversation_after_the_session_is_reopened() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("reopen");
    let (config, requests, server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);
    let done = window.turn(
        &session,
        "remember this picture",
        json!({"images": [{"media": "image/webp", "data": PICTURE_BASE64}]}),
    );
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let id = done.data["id"]
        .as_str()
        .expect("the record's id")
        .to_string();

    let mut reopened = Window::with_settings(window.settings.clone());
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));
    let session = opened["session"].as_str().unwrap().to_string();
    // The record carries the trust map its person answered with, so the reopened session asks no
    // question and has none to answer.
    let done = reopened.turn(&session, "what was it?", json!({}));
    server.join().unwrap();

    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let _first = requests.recv().unwrap();
    let message = user_message(&requests.recv().unwrap(), "remember this picture");
    assert_eq!(pictures(&message), [data_url("image/webp", PICTURE)]);
}

/// PASTE-3, PASTE-6, DROP-10: what this side can tell is wrong refuses the send, before a turn
/// starts or a model is asked.
#[test]
fn a_picture_the_bridge_cannot_carry_refuses_the_send() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("refused");
    let outside = elsewhere("refused");
    let notes = outside.join("notes.txt");
    let missing = outside.join("gone.png");
    let folder = outside.join("folder.png");
    let heavy = outside.join("heavy.pdf");
    std::fs::write(&notes, "text").unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        &heavy,
        vec![0u8; bravebot_agent::workspace::MAX_ATTACHMENT_BYTES + 1],
    )
    .unwrap();
    let inside = project.join("inside.png");
    std::fs::write(&inside, b"picture").unwrap();
    // So the relative entry below names a file that is there, and is refused for being relative.
    // This test runs in a process of its own.
    std::env::set_current_dir(&project).unwrap();
    let over_cap = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(vec![
            0u8;
            bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES
                + 1
        ])
    };
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let refused = [
        (
            "an unknown type",
            json!({"images": [{"media": "image/svg+xml", "data": PICTURE_BASE64}]}),
        ),
        (
            "a type spelled differently from the table's",
            json!({"images": [{"media": "IMAGE/PNG", "data": PICTURE_BASE64}]}),
        ),
        (
            "data that is not base64",
            json!({"images": [{"media": "image/png", "data": "not base64!"}]}),
        ),
        (
            "an empty picture",
            json!({"images": [{"media": "image/png", "data": ""}]}),
        ),
        (
            "a picture over the cap",
            json!({"images": [{"media": "image/png", "data": over_cap}]}),
        ),
        (
            "a picture with no data",
            json!({"images": [{"media": "image/png"}]}),
        ),
        (
            "a picture that is a string",
            json!({"images": ["image/png"]}),
        ),
        (
            "an `images` that is not a list",
            json!({"images": {"media": "image/png", "data": PICTURE_BASE64}}),
        ),
        (
            "a relative path to a file that is there",
            json!({"attachments": ["inside.png"]}),
        ),
        ("a path naming nothing", json!({"attachments": [missing]})),
        ("a directory", json!({"attachments": [folder]})),
        ("a text file", json!({"attachments": [notes]})),
        ("a file over the cap", json!({"attachments": [heavy]})),
        ("an entry that is not a string", json!({"attachments": [7]})),
        (
            "an `attachments` that is not a list",
            json!({"attachments": inside}),
        ),
    ];
    // The note a window shows for a dropped file that grew past the cap after it was staged.
    let grown = window
        .dispatch(
            "turn.send",
            json!({"session": session, "prompt": "look", "attachments": [heavy]}),
        )
        .unwrap_err();
    assert_eq!(
        grown.message,
        "heavy.pdf is 8.0 MB, and an attachment carries at most 8.0 MB"
    );
    for (case, extra) in refused {
        let mut params = json!({"session": session, "prompt": "look"});
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        let sent = window.dispatch("turn.send", params);
        assert_eq!(
            sent.map_err(|failure| failure.code),
            Err(ErrorCode::BadRequest),
            "{case} was sent"
        );
    }
    assert_eq!(
        window.raised("turn.started"),
        0,
        "a refused send started a turn"
    );

    // The same session takes a turn afterwards, so the refusals were the parameters and not it.
    let done = window.turn(
        &session,
        "look",
        json!({"images": null, "attachments": [inside]}),
    );
    server.join().unwrap();
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let message = user_message(&requests.recv().unwrap(), "look");
    assert_eq!(pictures(&message), [data_url("image/png", b"picture")]);
}

/// The window stages a drop by what the bridge says each file is, so a dropped file gets the
/// marker the terminal would give it, and a picture or PDF over the agent's cap is left out with
/// the note the bridge built (DROP-11). Text has no cap. Names only: none of these paths exists.
#[test]
fn a_dropped_file_is_classified_by_the_terminals_rules() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("classify");
    let mut window = Window::with_settings(project.join("no-settings.json"));
    let cap = bravebot_agent::workspace::MAX_ATTACHMENT_BYTES as u64;
    assert_eq!(cap, 8 * 1024 * 1024);
    let file = |path: &str, bytes: u64| json!({"path": path, "bytes": bytes});
    let answer = window.call(
        "drops.classify",
        json!({"files": [
            file("/a/SHOT.PNG", cap),
            file("/a/scan.pdf", 1),
            file("/a/notes.md", cap * 4),
            file("/a/Makefile", 0),
            file("/a/app.dmg", cap * 4),
            file("/a/.png", 1),
            file("/a/big.jpg", cap + 1),
            file("/a/huge.pdf", cap + cap / 8),
        ]}),
    );
    assert_eq!(
        answer,
        json!({"files": [
            {"kind": "image", "noun": "Image"},
            {"kind": "pdf", "noun": "PDF"},
            {"kind": "text", "noun": "File"},
            {"kind": "text", "noun": "File"},
            null,
            null,
            {"note": "big.jpg is 8.0 MB, and an attachment carries at most 8.0 MB"},
            {"note": "huge.pdf is 9.0 MB, and an attachment carries at most 8.0 MB"},
        ]})
    );
    for params in [
        json!({}),
        json!({"files": "/a/shot.png"}),
        json!({"files": ["/a/shot.png"]}),
        json!({"files": [{"path": "/a/shot.png"}]}),
        json!({"files": [{"path": "/a/shot.png", "bytes": -1}]}),
        json!({"paths": ["/a/shot.png"]}),
    ] {
        let refused = window.dispatch("drops.classify", params).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
    }
}

/// The window asks the bridge about each pasted picture's size, so the cap, the type it names a
/// picture by, the marker's noun and the note for one too large are the bridge's, and the window
/// holds none of its own (PASTE-11). The note is the terminal's.
#[test]
fn a_window_asks_the_bridge_whether_a_paste_fits_and_is_told_the_terminals_note() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("paste-check");
    let mut window = Window::with_settings(project.join("no-settings.json"));
    let cap = bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES as u64;
    assert_eq!(cap, 10 * 1024 * 1024);
    let fits = window.call("pastes.check", json!({"bytes": cap}));
    assert_eq!(fits["ok"], true);
    let media = fits["media"].as_str().unwrap();
    assert!(
        bravebot_agent::turn::PASTED_IMAGE_MEDIA.contains(&media),
        "{media} is not a type turn.send takes"
    );
    assert_eq!(fits["noun"], "Image");
    assert_eq!(
        window.call("pastes.check", json!({"bytes": cap * 2})),
        json!({"ok": false, "note": "that picture is 20.0 MB, and a paste carries at most 10.0 MB"})
    );
    for params in [json!({}), json!({"bytes": "big"}), json!({"bytes": -1})] {
        let refused = window.dispatch("pastes.check", params).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
    }
}
