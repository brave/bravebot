//! NAME-9: the window names a file with `@` through the bridge, on the terminal's rules.
//! NAME-10: it offers no past session and sends a `@session:<id>` line as typed.
#![allow(
    clippy::unwrap_used,
    reason = "the profile helper unwraps, and a test that unwraps fails at the line that did"
)]

#[allow(
    dead_code,
    reason = "undo_endpoint.rs is shared by several test binaries, and this one does not use tool"
)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../test-support/message_ids.rs"]
mod message_ids;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request};
use serde_json::{Value, json};

use std::sync::{Arc, Mutex};

fn harness() -> (Bridge, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let bridge = Bridge::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    (bridge, events)
}

/// The answer, or the refusal's message.
fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, String> {
    let line = json!({ "id": 1, "method": method, "params": params }).to_string();
    let request = Request::parse(&line).expect("well formed");
    bridge
        .dispatch(&request)
        .map_err(|failure| format!("{:?}: {}", failure.code, failure.message))
}

/// A project beside a file outside it, with a session opened on the project and its trust
/// question answered.
struct Project {
    _held: tempfile::TempDir,
    root: std::path::PathBuf,
    session: String,
}

fn project(bridge: &mut Bridge) -> Project {
    let held = tempfile::tempdir().expect("scratch");
    let root = held.path().join("project");
    for directory in ["src", "node_modules", ".git", "docs"] {
        std::fs::create_dir_all(root.join(directory)).expect("create");
    }
    std::fs::write(root.join("README.md"), "read me\n").expect("write");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write");
    std::fs::write(
        root.join("picture.png"),
        [0x89, b'P', b'N', b'G', 0, 0, 0, 13],
    )
    .expect("write");
    std::fs::write(held.path().join("outside.txt"), "secret\n").expect("write");
    let opened = call(
        bridge,
        "session.new",
        json!({ "directory": root.display().to_string() }),
    )
    .expect("opens");
    let session = opened["session"].as_str().expect("a handle").to_string();
    call(
        bridge,
        "trust.reply",
        json!({ "session": &session, "trusted": false }),
    )
    .expect("answered");
    Project {
        _held: held,
        root,
        session,
    }
}

fn offered(answer: &Value) -> Vec<&str> {
    answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect()
}

/// NAME-4, NAME-5: the window's list is the terminal's, drawn from the session's project.
#[test]
fn the_window_is_offered_the_terminals_list_of_the_project() {
    let (mut bridge, _) = harness();
    let project = project(&mut bridge);
    let offer = |bridge: &mut Bridge, line: &str| {
        call(
            bridge,
            "mentions.offer",
            json!({ "session": &project.session, "line": line }),
        )
        .expect("answered")
    };

    let root = offer(&mut bridge, "what is in @");
    assert_eq!(root["typed"], json!(""));
    assert_eq!(
        offered(&root),
        vec!["docs/", "src/", "README.md", "picture.png"],
        "directories first, without version control or dependencies"
    );
    assert_eq!(root["entries"][0]["directory"], json!(true));

    let nested = offer(&mut bridge, "@src/m");
    assert_eq!(offered(&nested), vec!["src/main.rs"]);
    assert_eq!(
        nested["completes"],
        json!(true),
        "a half-typed name completes"
    );

    let finished = offer(&mut bridge, "Summarise @README.md");
    assert_eq!(
        finished["completes"],
        json!(false),
        "Enter sends a finished name"
    );

    assert!(
        offered(&offer(&mut bridge, "@../")).is_empty(),
        "climbing out"
    );
    assert_eq!(
        offer(&mut bridge, "@README.md ")["typed"],
        Value::Null,
        "a space finishes the name and closes the list"
    );
    assert_eq!(
        offer(&mut bridge, "an ordinary prompt")["typed"],
        Value::Null
    );
}

/// NAME-1, NAME-5, NAME-6: the files a prompt names are the ones the turn can read, and a name the
/// read would refuse is refused with a message naming it.
#[test]
fn a_prompt_names_only_text_files_inside_the_project() {
    let (mut bridge, _) = harness();
    let project = project(&mut bridge);
    let named = |bridge: &mut Bridge, prompt: &str| {
        call(
            bridge,
            "mentions.named",
            json!({ "session": &project.session, "prompt": prompt }),
        )
    };

    assert_eq!(
        named(
            &mut bridge,
            "compare @README.md with @src/main.rs and not @docs/ or me@example.com @"
        )
        .expect("named")["files"],
        json!(["README.md", "src/main.rs"])
    );

    let outside = project.root.parent().expect("a parent").join("outside.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, project.root.join("leads-out")).expect("link");
    let mut refused = vec![
        ("@../outside.txt", "is not a file in this project"),
        ("@missing.md", "is not a file in this project"),
        ("@src", "is not a file in this project"),
        ("@picture.png", "is not a text file"),
    ];
    let absolute = format!("@{}", outside.display());
    refused.push((&absolute, "is not a file in this project"));
    #[cfg(unix)]
    refused.push(("@leads-out", "is not a file in this project"));
    for (prompt, said) in refused {
        let refusal = named(&mut bridge, &format!("read {prompt}")).expect_err(prompt);
        assert!(
            refusal.starts_with("BadRequest") && refusal.contains(prompt) && refusal.contains(said),
            "{prompt}: {refusal}"
        );
    }
}

/// NAME-8, NAME-9: the window splices what the bridge writes for an entry, so a name holding a
/// no-break space or ending in a backslash arrives escaped and the prompt that results names the
/// file picked and not the one beside it. A name that ends in a no-break space is left out of the
/// list, since the window trims the prompt it sends.
#[cfg(unix)]
#[test]
fn an_entry_holding_a_no_break_space_is_written_so_the_prompt_names_it() {
    let (mut bridge, _) = harness();
    let project = project(&mut bridge);
    for name in ["a b.md", "a.md", "a.md\u{a0}", "a.md\\", "a\u{a0}b.md"] {
        std::fs::write(project.root.join(name), "text\n").expect("write");
    }

    let offer = call(
        &mut bridge,
        "mentions.offer",
        json!({ "session": &project.session, "line": "read @a" }),
    )
    .expect("answered");
    // The rows for the prefix come first, and names a fragment finds below the root follow them.
    let listed: Vec<&str> = offered(&offer)
        .into_iter()
        .filter(|path| path.starts_with('a'))
        .collect();
    assert_eq!(listed, vec!["a b.md", "a.md", "a.md\\", "a\u{a0}b.md"]);
    assert_eq!(offered(&offer)[..4], listed[..]);
    assert_eq!(offer["entries"][2]["written"], json!("a.md\\\\"));
    assert_eq!(offer["entries"][3]["written"], json!("a\\\u{a0}b.md"));
    for (row, file) in [(2, "a.md\\"), (3, "a\u{a0}b.md")] {
        let written = offer["entries"][row]["written"].as_str().expect("written");
        let named = call(
            &mut bridge,
            "mentions.named",
            json!({ "session": &project.session, "prompt": format!("read @{written} now") }),
        )
        .expect("named");
        assert_eq!(named["files"], json!([file]));
    }
}

/// NAME-9: `turn.send` reads the names out of the prompt itself and refuses one that cannot go,
/// before a turn starts, and a prompt the app composed names nothing.
#[test]
fn a_send_naming_a_file_that_cannot_go_is_refused_before_the_turn() {
    let (mut bridge, events) = harness();
    let project = project(&mut bridge);

    let refusal = call(
        &mut bridge,
        "turn.send",
        message_ids::with_fresh(
            json!({ "session": &project.session, "prompt": "read @../outside.txt" }),
        ),
    )
    .expect_err("an outside name");
    assert!(refusal.contains("@../outside.txt"), "{refusal}");
    assert!(
        !events
            .lock()
            .expect("not poisoned")
            .iter()
            .any(|event| event.name == "turn.started"),
        "a turn started for a refused send"
    );

    let composed = call(
        &mut bridge,
        "turn.send",
        message_ids::with_fresh(json!({
            "session": &project.session,
            "prompt": "bring your memory up to date, @../outside.txt",
            "composed": "consolidation",
        })),
    );
    assert!(
        !composed
            .as_ref()
            .is_err_and(|refusal| refusal.contains("@../outside.txt")),
        "a prompt the app composed was read for names: {composed:?}"
    );
}

/// NAME-12: a name inside the words of a folded paste is sent as text. The window sends the line as
/// its box showed it as `shown`, and the bridge reads the names there, so an `@` the person pasted
/// refuses nothing. A name `shown` gives that the prompt does not is not read either, so `shown`
/// cannot name a file the sent words never named.
///
/// The faults this rejects: the names read out of the prompt when `shown` is given, which refuses
/// the pasted name, and a name read out of `shown` alone, which `turn.send` then refuses.
#[test]
fn a_send_reads_names_in_the_shown_line_and_never_adds_one() {
    let (mut bridge, _events) = harness();
    let project = project(&mut bridge);

    let pasted = call(
        &mut bridge,
        "turn.send",
        message_ids::with_fresh(json!({
            "session": &project.session,
            "prompt": "look at this\nlog line @../outside.txt\nmore\nend",
            "shown": "look at this [Pasted text #1 +3 lines]",
        })),
    );
    assert!(
        !pasted
            .as_ref()
            .is_err_and(|refusal| refusal.contains("@../outside.txt")),
        "a name inside a folded paste was read: {pasted:?}"
    );

    let added = call(
        &mut bridge,
        "turn.send",
        message_ids::with_fresh(json!({
            "session": &project.session,
            "prompt": "look at this",
            "shown": "look at this @../outside.txt",
        })),
    );
    assert!(
        !added
            .as_ref()
            .is_err_and(|refusal| refusal.contains("@../outside.txt")),
        "a name only `shown` gives was read: {added:?}"
    );

    let typed = call(
        &mut bridge,
        "turn.send",
        message_ids::with_fresh(json!({
            "session": &project.session,
            "prompt": "read @../outside.txt and\nthe\npasted\nwords",
            "shown": "read @../outside.txt and [Pasted text #2 +4 lines]",
        })),
    )
    .expect_err("a name typed beside a folded paste is still read");
    assert!(typed.contains("@../outside.txt"), "{typed}");
}

/// REFER-6: the window is offered a reference's alias and the files behind it, and a prompt naming
/// one of them is accepted. The bridge builds the alias list from the settings itself, so a front
/// end that passed none would offer the window nothing and refuse the name as not in the project.
#[test]
fn the_window_names_a_file_under_a_reference_alias() {
    let held = tempfile::tempdir().expect("scratch");
    let library = held.path().join("library");
    std::fs::create_dir_all(library.join("src")).expect("create");
    std::fs::write(library.join("src/lexer.rs"), "fn lex() {}\n").expect("write");
    let settings = held.path().join("settings.json");
    std::fs::write(
        &settings,
        json!({ "references": { "parser": { "path": library.display().to_string() } } })
            .to_string(),
    )
    .expect("settings");

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut bridge = Bridge::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }))
    .with_settings(Some(settings));
    let project = project(&mut bridge);
    let offer = |bridge: &mut Bridge, line: &str| {
        call(
            bridge,
            "mentions.offer",
            json!({ "session": &project.session, "line": line }),
        )
        .expect("answered")
    };

    assert!(offered(&offer(&mut bridge, "@par")).contains(&"parser/"));
    assert_eq!(
        offered(&offer(&mut bridge, "@parser/src/")),
        vec!["parser/src/lexer.rs"]
    );
    let named = call(
        &mut bridge,
        "mentions.named",
        json!({ "session": &project.session, "prompt": "read @parser/src/lexer.rs" }),
    )
    .expect("a file under a reference is a named file");
    assert_eq!(named["files"].as_array().expect("files").len(), 1);
}

/// A project directory the profile owns, with a settings file beside it that points the model at
/// `endpoint`, and a session opened on it and its trust question answered.
struct Isolated {
    bridge: Bridge,
    events: Arc<Mutex<Vec<Event>>>,
    root: std::path::PathBuf,
    session: String,
}

fn isolated(name: &str, endpoint: &str) -> Isolated {
    let root = test_profile::project(name);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("README.md"), "read me\n").unwrap();
    let settings = root.with_file_name(format!("{name}-test-settings.json"));
    std::fs::write(
        &settings,
        json!({"model": "mention-test/test", "provider": {"mention-test": {
            "options": {"baseURL": endpoint}, "models": {"test": {}}
        }}})
        .to_string(),
    )
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let mut bridge = Bridge::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }))
    .with_settings(Some(settings));
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": root.display().to_string() }),
    )
    .expect("opens");
    let session = opened["session"].as_str().expect("a handle").to_string();
    call(
        &mut bridge,
        "trust.reply",
        json!({ "session": &session, "trusted": true }),
    )
    .expect("answered");
    Isolated {
        bridge,
        events,
        root,
        session,
    }
}

/// Store a record of an earlier session of `root`, titled `title`, holding one exchange. The id is
/// the one a record is stored under in the profile's session store.
fn an_earlier_session(root: &std::path::Path, id: &str, title: &str) {
    use bravebot_aichat::protocol::Message;
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("a question asked long ago"));
    conversation.push(Message::assistant("an answer given long ago"));
    let directory = bravebot_session::sessions::project_directory(root).expect("a store");
    std::fs::create_dir_all(&directory).unwrap();
    let record = json!({
        "id": id, "directory": root.display().to_string(), "title": title,
        "started": 1, "updated": 2, "front": "terminal",
        "conversation": conversation.snapshot(),
    });
    std::fs::write(directory.join(format!("{id}.json")), record.to_string()).unwrap();
    assert_eq!(
        bravebot_session::sessions::list(root)
            .iter()
            .filter(|row| row.id == id)
            .count(),
        1,
        "the stored session is not one the terminal's list would offer"
    );
}

/// NAME-10: the desktop message box offers no sessions. With an earlier session stored in the
/// project, `@` and a word of its title offer files and nothing of kind `session` or with a
/// `session:` path, and `@session:` offers nothing, where the terminal offers the session for each.
/// Rejects a bridge that adds the past-sessions rows to the window's list.
#[test]
fn the_window_is_offered_no_past_session() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut window = isolated("mentions-offers-no-session", "http://127.0.0.1:9");
    std::fs::write(window.root.join("parser-notes.md"), "notes\n").unwrap();
    an_earlier_session(&window.root, "20260101T000000-aaaa", "parser rewrite plan");

    for line in [
        "@",
        "read @pars",
        "read @rewrite",
        "@session:",
        "@session:2026",
    ] {
        let answer = call(
            &mut window.bridge,
            "mentions.offer",
            json!({ "session": &window.session, "line": line, "definitions": true }),
        )
        .expect("answered");
        for entry in answer["entries"].as_array().expect("entries") {
            assert_ne!(entry["kind"], json!("session"), "{line:?} offered {entry}");
            let path = entry["path"].as_str().expect("a path");
            assert!(
                !path.starts_with("session:") && !path.contains("20260101T000000-aaaa"),
                "{line:?} offered a session as {entry}"
            );
        }
    }
    // The files are still offered, so an empty list above is not an answer that offers nothing.
    let answer = call(
        &mut window.bridge,
        "mentions.offer",
        json!({ "session": &window.session, "line": "read @pars" }),
    )
    .expect("answered");
    assert_eq!(offered(&answer), vec!["parser-notes.md"]);
}

/// NAME-10: the desktop sends the line as typed. A line naming `@session:<id>` of a stored session
/// is neither refused nor read as a file (`mentions.named` names no file), and the model is sent the
/// line with nothing of the session added: no excerpt heading, none of that session's words.
/// Rejects a bridge that reads the line for sessions as the terminal does.
#[test]
fn the_window_sends_a_session_mention_as_typed_with_nothing_of_the_session() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let mut window = isolated("mentions-sends-session-as-typed", &config.endpoint);
    an_earlier_session(&window.root, "20260101T000000-aaaa", "parser rewrite plan");
    let line = "carry on from @session:20260101T000000-aaaa please";

    let named = call(
        &mut window.bridge,
        "mentions.named",
        json!({ "session": &window.session, "prompt": line }),
    )
    .expect("a session mention is not refused");
    assert_eq!(named["files"], json!([]), "a session was read as a file");

    call(
        &mut window.bridge,
        "turn.send",
        message_ids::with_fresh(json!({"session": &window.session, "prompt": line})),
    )
    .expect("a send naming a session is not refused");
    let until = std::time::Instant::now() + endpoint::LIMIT;
    loop {
        assert!(std::time::Instant::now() < until, "the turn never ended");
        let ended = window
            .events
            .lock()
            .expect("not poisoned")
            .iter()
            .any(|event| matches!(event.name, "turn.done" | "turn.error"));
        if ended {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    server.join().unwrap();
    let done = window
        .events
        .lock()
        .expect("not poisoned")
        .iter()
        .find(|event| matches!(event.name, "turn.done" | "turn.error"))
        .map(|event| (event.name, event.data.clone()))
        .expect("an event");
    assert_eq!(done.0, "turn.done", "{:?}", done.1);

    let sent = requests.recv().unwrap();
    let request: Value = serde_json::from_str(&sent).unwrap();
    let user: Vec<String> = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .map(|message| message["content"].to_string())
        .collect();
    let holds_line = user.iter().any(|content| content.contains(line));
    assert!(holds_line, "the line was not sent as typed: {user:?}");
    assert!(
        !sent.contains("Excerpt of an earlier session")
            && !sent.contains("a question asked long ago")
            && !sent.contains("an answer given long ago"),
        "the desktop added words of the session the line named: {sent}"
    );
}
