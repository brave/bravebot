//! NAME-9: the window names a file with `@` through the bridge, on the terminal's rules.

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

/// NAME-9: `turn.send` reads the names out of the prompt itself and refuses one that cannot go,
/// before a turn starts, and a prompt the app composed names nothing.
#[test]
fn a_send_naming_a_file_that_cannot_go_is_refused_before_the_turn() {
    let (mut bridge, events) = harness();
    let project = project(&mut bridge);

    let refusal = call(
        &mut bridge,
        "turn.send",
        json!({ "session": &project.session, "prompt": "read @../outside.txt" }),
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
        json!({
            "session": &project.session,
            "prompt": "bring your memory up to date, @../outside.txt",
            "composed": "consolidation",
        }),
    );
    assert!(
        !composed
            .as_ref()
            .is_err_and(|refusal| refusal.contains("@../outside.txt")),
        "a prompt the app composed was read for names: {composed:?}"
    );
}
