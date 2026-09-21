//! That a session written here is a session the terminal can read.
//!
//! The whole justification for depending on the agent's own `sessions` module rather than
//! inventing a store is that both front-ends write one format. That is a claim, and a
//! claim about two programs agreeing is worth a test rather than a sentence.
//!
//! No model is involved: this writes a record the way a finished turn does and reads it
//! back the way `bravebot --resume` does.

use bravebot_aichat::protocol::Message;
use bravebot_core::todo::{Row, Status};
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_session::sessions::{self, Handle, Standing};
use std::collections::BTreeMap;

/// A directory nothing else is using, inside the real session store.
///
/// Sessions are keyed by the working directory they ran in, so an unused temporary path
/// gets its own directory under `~/.bravebot/sessions` and cannot disturb a real one.
fn scratch(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("bravebot-bridge-interop-{name}"));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

fn clean_up(project: &std::path::Path) {
    if let Some(directory) = sessions::project_directory(project) {
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[test]
fn a_record_written_here_is_read_back_by_the_agents_own_reader() {
    let project = scratch("round-trip");
    clean_up(&project);

    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("what does this do?"));
    conversation.push(Message::assistant("it parses commas"));

    let mut trust = TrustStore::new(&project);
    trust.trust(".");

    let mut todos = BTreeMap::new();
    todos.insert(
        1,
        vec![Row { content: "read the parser".into(), marker: "[x]", status: Status::Done }],
    );

    let mut handle = Handle::begin(&project, bravebot_bridge::agent_build());
    handle.save(
        "what does this do?",
        Standing {
            history: None,
            rewind: &[],
            asides: &[],
            conversation: &conversation.snapshot(),
            turns: 1,
            tokens: 42,
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &todos,
            trust: &trust,
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
        },
    );
    let id = handle.id().to_string();

    // Read back exactly as the terminal's resume does.
    let listed = sessions::list(&project);
    assert_eq!(listed.len(), 1, "the session should be listed");
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].title, "what does this do?");

    let record = sessions::load(&project, &id).expect("the record should load");
    assert_eq!(record.turns, 1);
    assert_eq!(record.tokens, 42);
    assert_eq!(record.directory, project.display().to_string());
    assert!(record.trust.is_some(), "the map must survive, not be re-derived");
    assert_eq!(record.todo_rows()[&1][0].content, "read the parser");

    // And as the bridge's own cross-project discovery does, which is the part that is
    // ours rather than upstream's.
    let found = bravebot_bridge::store::list_all();
    assert!(
        found.iter().any(|entry| entry.summary.id == id && entry.project == project),
        "a session in a new project must be discovered without being told where to look"
    );

    clean_up(&project);
}

/// The bridge recounts a stored conversation the same way the terminal does.
#[test]
fn a_stored_conversation_recounts_to_what_a_person_said() {
    let project = scratch("recount");
    clean_up(&project);

    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("first question"));
    conversation.push(Message::assistant("first answer"));
    conversation.push(Message::user("second question"));
    conversation.push(Message::assistant("second answer"));

    let mut handle = Handle::begin(&project, bravebot_bridge::agent_build());
    handle.save(
        "first question",
        Standing {
            history: None,
            rewind: &[],
            asides: &[],
            conversation: &conversation.snapshot(),
            turns: 2,
            tokens: 0,
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            trust: &TrustStore::new(&project),
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
        },
    );
    let id = handle.id().to_string();

    let record = sessions::load(&project, &id).expect("loads");
    let restored = bravebot_agent::Conversation::restored(record.conversation.clone());
    let said = restored.recounted();

    let text: Vec<&str> = said
        .iter()
        .map(|entry| match entry {
            bravebot_agent::conversation::Said::User(t) => t.as_str(),
            bravebot_agent::conversation::Said::Assistant(t) => t.as_str(),
            bravebot_agent::conversation::Said::Tool(t) => t.as_str(),
        })
        .collect();

    assert!(text.contains(&"first question"));
    assert!(text.contains(&"second answer"));

    clean_up(&project);
}

/// Resuming a session and taking a turn continues that session, rather than forking it.
///
/// The bug this pins down was invisible from inside a window: the turn ran, the reply
/// arrived, and the transcript looked right, because the state in memory was correct all
/// along. Only the record was wrong. A resumed `State` left `handle` unset, so the first
/// save minted a fresh id and wrote a *second* record — leaving the one the user opened
/// frozen at the moment they opened it, and the session list a little longer every time.
///
/// It compounded rather than merely duplicating. A record that never advances is resumed
/// from the same point forever, so the note upstream adds about dead quarantine references
/// was re-injected on every open, where it sits immediately before whatever the user types
/// next and quietly absorbs anything anaphoric — "tell me more" being answered about the
/// resume notice rather than about the conversation.
#[test]
fn resuming_a_session_writes_back_to_it_rather_than_forking() {
    let project = scratch("resume-continues");
    clean_up(&project);

    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("remember the word haddock"));
    conversation.push(Message::assistant("haddock it is"));

    let trust = TrustStore::new(&project);
    let todos = BTreeMap::new();
    let timing = BTreeMap::from([(1, bravebot_agent::timing::Timing {
        wall_ms: 120, inference_ms: 80, tools_ms: 20, stalled_ms: 10,
    })]);

    let mut handle = Handle::begin(&project, bravebot_bridge::agent_build());
    handle.save(
        "remember the word haddock",
        Standing {
            history: None,
            rewind: &[bravebot_session::sessions::RewindPoint {
                snapshot: bravebot_session::sessions::TurnSnapshot {
                    conversation: bravebot_agent::Conversation::new().snapshot(),
                    turns: 0, tokens: 0, spend: BTreeMap::new(), timing: BTreeMap::new(),
                    cached: None, trust: TrustStore::new(&project),
                    programs: TrustedPrograms::new(), transcript_len: 0,
                    title: "Before haddock".into(), was_wrote: false,
                },
                backups: Vec::new(), prompt: "remember the word haddock".into(),
            }],
            asides: &[bravebot_session::sessions::Aside {
                question: "what is haddock?".into(),
                answer: Some("a fish".into()), kept: true,
            }],
            conversation: &conversation.snapshot(),
            turns: 1,
            tokens: 10,
            spend: &BTreeMap::new(),
            timing: &timing,
            model: None,
            todos: &todos,
            trust: &trust,
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
        },
    );
    let original = handle.id().to_string();

    // What `session.open` does with what it found on disk.
    let record = sessions::load(&project, &original).expect("the record should load");
    let mut state = bravebot_bridge::running::State::resumed(&project, &record, trust.clone());

    // What the worker does at the end of a turn: the same call, through the same handle.
    state.turns = 2;
    state.tokens = 25;
    let saved = state.handle.as_mut().expect("a resumed session already has its handle");
    saved.save(
        &state.first_prompt.clone().unwrap_or_default(),
        Standing {
            history: None,
            rewind: &state.rewind,
            asides: &state.asides,
            conversation: &state.conversation.snapshot(),
            turns: state.turns,
            tokens: state.tokens,
            spend: &BTreeMap::new(),
            timing: &state.timing,
            model: None,
            todos: &state.todos,
            trust: &state.trust,
            programs: &state.programs,
            directories: &state.directories,
            manifest: None,
        },
    );

    let listed = sessions::list(&project);
    assert_eq!(listed.len(), 1, "resuming must not leave a second session behind");
    assert_eq!(listed[0].id, original, "the continued session keeps its id");

    let reread = sessions::load(&project, &original).expect("the record should still load");
    assert_eq!(reread.turns, 2, "the turn landed in the session it was taken in");
    assert_eq!(reread.tokens, 25);
    let rewind = reread.rewind_points(&project);
    assert_eq!(rewind.len(), 1);
    assert_eq!(rewind[0].prompt, "remember the word haddock");
    assert_eq!(rewind[0].snapshot.title, "Before haddock");
    let recalled = sessions::recall(&project, &reread);
    assert_eq!(recalled.asides.len(), 1);
    assert_eq!(recalled.asides[0].question, "what is haddock?");
    assert_eq!(recalled.asides[0].answer.as_deref(), Some("a fish"));
    assert!(recalled.asides[0].kept);
    assert_eq!(reread.timing, timing, "resuming must preserve the timing from the terminal");
    assert_eq!(reread.title, "remember the word haddock", "the title survives the resume");

    clean_up(&project);
}

// ------------------------------------------------------------------------------- forking

/// A bridge whose events land in a vector, as `tests/dispatch.rs` does.
fn harness() -> (
    bravebot_bridge::bridge::Bridge,
    std::sync::Arc<std::sync::Mutex<Vec<bravebot_bridge::protocol::Event>>>,
) {
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&events);
    let bridge = bravebot_bridge::bridge::Bridge::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    (bridge, events)
}

fn call(
    bridge: &mut bravebot_bridge::bridge::Bridge,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let line = serde_json::json!({ "id": 1, "method": method, "params": params }).to_string();
    let request = bravebot_bridge::protocol::Request::parse(&line).expect("well formed");
    bridge.dispatch(&request).expect("the call should be served")
}

/// A session on disk with two prompts in it, and its id.
fn two_prompt_session(project: &std::path::Path, trust: Option<&TrustStore>) -> String {
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("remember the word haddock"));
    conversation.push(Message::assistant("haddock it is"));
    conversation.push(Message::user("now forget it"));
    conversation.push(Message::assistant("forgotten"));

    let empty = TrustStore::new(project);
    let mut handle = Handle::begin(project, bravebot_bridge::agent_build());
    handle.save(
        "remember the word haddock",
        Standing {
            history: None,
            rewind: &[],
            asides: &[bravebot_session::sessions::Aside {
                question: "what is haddock?".into(),
                answer: Some("a fish".into()), kept: true,
            }],
            conversation: &conversation.snapshot(),
            turns: 2,
            tokens: 30,
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            // A record written before trust maps were kept has none, and `save` writes what it
            // is given — so an empty map here is still a map, and `None` needs the record.
            trust: trust.unwrap_or(&empty),
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
        },
    );
    handle.id().to_string()
}

/// The whole promise of a fork: the session it came from is left exactly as it was.
#[test]
fn forking_a_stored_session_leaves_the_parent_record_untouched() {
    let project = scratch("fork-parent-untouched");
    clean_up(&project);
    let mut trust = TrustStore::new(&project);
    trust.trust(".");
    let parent = two_prompt_session(&project, Some(&trust));
    let before = sessions::load(&project, &parent).expect("the record should load");

    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    call(
        &mut bridge,
        "session.fork",
        serde_json::json!({
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "now forget it",
        }),
    );

    let after = sessions::load(&project, &parent).expect("the record should still load");
    assert_eq!(after.updated, before.updated, "forking is not a write to the parent");
    assert_eq!(after.turns, before.turns);
    assert_eq!(
        after.conversation.messages.len(),
        before.conversation.messages.len(),
        "the parent keeps the whole of its history",
    );

    clean_up(&project);
}

#[test]
fn a_fork_writes_nothing_until_it_has_something_to_say() {
    let project = scratch("fork-writes-nothing");
    clean_up(&project);
    let parent = two_prompt_session(&project, Some(&TrustStore::new(&project)));

    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    let forked = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "now forget it",
        }),
    );

    let child = forked["id"].as_str().expect("a durable id").to_string();
    assert_ne!(child, parent, "a fork is a session of its own");
    assert_eq!(sessions::list(&project).len(), 1, "only the parent has a record");
    assert!(
        sessions::load(&project, &child).is_none(),
        "the id is reserved, but nothing stands behind it until the first turn",
    );

    clean_up(&project);
}

#[test]
fn a_fork_recounts_to_everything_before_the_prompt_it_was_cut_at() {
    let project = scratch("fork-recount");
    clean_up(&project);
    let parent = two_prompt_session(&project, Some(&TrustStore::new(&project)));

    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    let forked = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "now forget it",
        }),
    );

    let said = forked["said"].as_array().expect("a transcript");
    let texts: Vec<&str> = said.iter().map(|line| line["text"].as_str().unwrap_or("")).collect();
    assert_eq!(texts, vec!["remember the word haddock", "haddock it is"]);
    assert_eq!(forked["prefill"], "now forget it", "the prompt is handed back to be edited");
    assert_eq!(forked["turns"], 1);
    assert_eq!(forked["parent"]["id"], parent.as_str());
    assert_eq!(forked["parent"]["title"], "remember the word haddock");

    // And the prompt has to be the one the ordinal names.
    let line = serde_json::json!({
        "id": 2,
        "method": "session.fork",
        "params": {
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "something else entirely",
        },
    })
    .to_string();
    let request = bravebot_bridge::protocol::Request::parse(&line).expect("well formed");
    assert!(
        bridge.dispatch(&request).is_err(),
        "an ordinal the front-end disagrees with is not a place to cut",
    );

    clean_up(&project);
}

#[test]
fn a_fork_inherits_the_trust_map_rather_than_asking_again() {
    let project = scratch("fork-trust-inherited");
    clean_up(&project);
    let mut trust = TrustStore::new(&project);
    trust.trust(".");
    let parent = two_prompt_session(&project, Some(&trust));

    let (mut bridge, events) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    let forked = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "now forget it",
        }),
    );

    assert_eq!(forked["trust"]["known"], true);
    let asked = events.lock().expect("not poisoned");
    assert!(
        !asked.iter().any(|event| event.session.as_deref() == forked["session"].as_str()
            && event.name == "trust.request"),
        "the person who answered for this directory is the person forking in it",
    );

    clean_up(&project);
}

/// A parent still holding the question hands the question down, rather than an answer nobody
/// gave. Nothing recorded is not the same as nothing trusted.
#[test]
fn a_fork_of_a_record_with_no_trust_map_asks() {
    let project = scratch("fork-trust-unknown");
    clean_up(&project);
    let parent = two_prompt_session(&project, Some(&TrustStore::new(&project)));

    // Strip the map the way a record written before maps existed has none.
    let path = sessions::project_directory(&project)
        .expect("a project directory")
        .join(format!("{parent}.json"));
    let text = std::fs::read_to_string(&path).expect("the record");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("json");
    value.as_object_mut().expect("an object").remove("trust");
    std::fs::write(&path, value.to_string()).expect("rewritten");

    let (mut bridge, events) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    let forked = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({
            "session": opened["session"].as_str().expect("a handle"),
            "prompt": 1,
            "text": "now forget it",
        }),
    );

    assert_eq!(forked["trust"]["known"], false);
    let asked = events.lock().expect("not poisoned");
    assert!(
        asked.iter().any(|event| event.session.as_deref() == forked["session"].as_str()
            && event.name == "trust.request"),
        "the fork must ask what its parent never answered",
    );

    clean_up(&project);
}

/// The mirror of `resuming_a_session_writes_back_to_it_rather_than_forking`, and it belongs
/// beside it: one says a resume must not fork, the other says a fork must not resume.
#[test]
fn a_fork_gets_an_id_of_its_own_rather_than_the_one_it_came_from() {
    let project = scratch("fork-new-id");
    clean_up(&project);
    let parent = two_prompt_session(&project, Some(&TrustStore::new(&project)));
    let record = sessions::load(&project, &parent).expect("the record should load");

    // The agent's ids carry the second they were minted in, so a fork taken inside the same
    // second as its parent's first save would be its parent. `Bridge::begin_unique` is what
    // handles that in the app; this test is about what a *saved* fork does to the store, so it
    // simply waits the second out. `two_forks_in_the_same_second_stay_two_sessions` covers the
    // other half.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let mut cut = record.conversation.clone();
    cut.messages.truncate(2);
    let mut state = bravebot_bridge::running::State::forked(
        Handle::begin(&project, bravebot_bridge::agent_build()),
        cut,
        TrustStore::new(&project),
        TrustedPrograms::new(),
        Vec::new(),
        1,
        BTreeMap::new(),
        Some("remember the word haddock".into()),
    );

    // What the worker does at the end of the fork's first turn.
    state.turns = 2;
    let saved = state.handle.as_mut().expect("a fork has its handle from the start");
    saved.save(
        &state.first_prompt.clone().unwrap_or_default(),
        Standing {
            history: None,
            rewind: &state.rewind,
            asides: &state.asides,
            conversation: &state.conversation.snapshot(),
            turns: state.turns,
            tokens: 5,
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &state.todos,
            trust: &state.trust,
            programs: &state.programs,
            directories: &state.directories,
            manifest: None,
        },
    );
    let child = saved.id().to_string();

    assert_ne!(child, parent, "the fork must not write back to the session it came from");
    assert_eq!(sessions::list(&project).len(), 2, "both sessions are there");

    let reread = sessions::load(&project, &parent).expect("the parent should still load");
    assert_eq!(reread.turns, 2, "the parent is as it was");
    assert_eq!(reread.conversation.messages.len(), 4);

    let written = sessions::load(&project, &child).expect("the fork should load");
    assert_eq!(
        written.title, "remember the word haddock",
        "a fork keeps the name of what it came from rather than being renamed by its new prompt",
    );

    clean_up(&project);
}

/// Two forks taken in the same second are still two sessions.
///
/// The one place the agent's `<second>-<pid>` ids can genuinely collide: both are minted by a
/// click rather than by a turn, so nothing slow sits between them.
#[test]
fn two_forks_in_the_same_second_stay_two_sessions() {
    let project = scratch("fork-same-second");
    clean_up(&project);
    let parent = two_prompt_session(&project, Some(&TrustStore::new(&project)));

    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &parent }),
    );
    let handle = opened["session"].as_str().expect("a handle").to_string();

    let first = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({ "session": &handle, "prompt": 1, "text": "now forget it" }),
    );
    let second = call(
        &mut bridge,
        "session.fork",
        serde_json::json!({ "session": &handle, "prompt": 1, "text": "now forget it" }),
    );

    assert_ne!(first["id"], second["id"], "one click must not overwrite the other");
    assert_ne!(first["id"].as_str(), Some(parent.as_str()));
    assert_ne!(second["id"].as_str(), Some(parent.as_str()));

    clean_up(&project);
}

/// A resumed session says how much compaction has already taken out of it.
///
/// This is the signal a front-end needs to know whether something it put at the top of a
/// session is still in the session. The `compacting` phase cannot answer it: that is emitted
/// before compaction is attempted, so it also fires when there was nothing worth compacting,
/// and then on every round of a conversation that is over budget and cannot get under it. The
/// archive is a count of what was actually taken, it only rises, and — the part that matters
/// here — it is written into the record, so a session resumed in a new process knows it
/// without having watched it happen.
#[test]
fn a_resumed_session_reports_what_compaction_took_out_of_it() {
    let project = scratch("archived-count");
    clean_up(&project);
    std::fs::create_dir_all(&project).expect("a project directory");

    let id = two_prompt_session(&project, None);
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.open",
        serde_json::json!({ "directory": project.display().to_string(), "id": &id }),
    );

    assert_eq!(
        opened["archived"], 0,
        "a conversation nothing has been taken out of has archived nothing"
    );

    clean_up(&project);
}
