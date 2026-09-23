//! The workspace a turn started here runs on, and what the settings get to say about it.
//!
//! The bridge reads the settings files itself and hands what they say to the workspace, so the
//! wiring is the whole of SEARCH-9 for this front end: nothing in the workspace reads a settings
//! file, and a cap that is not passed on is a cap nobody in the graphical front end ever gets.

use bravebot_agent::workspace::{MAX_SEARCH_FILES, Workspace};
use bravebot_config::Settings;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::value::Labelled;
use bravebot_ui_bridge::bridge::turn_workspace;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A project directory of its own, removed when the test ends.
///
/// A fresh one per test rather than a fixed path: several of these suites run at once, and a
/// name two of them share is one deleting the other's files halfway through a walk.
fn project(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("bravebot-bridge-search-{name}-"))
        .tempdir()
        .expect("a project directory")
}

/// Settings as the bridge reads them for a session: the project's own files, and no home.
///
/// Home is left out so the machine running the test cannot contribute a cap of its own.
fn settings_of(project: &Path) -> Settings {
    Settings::layered(None, Some(project), None)
}

fn routing() -> Routing {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "search the project");
    routing
}

/// Whether a search of the whole project walked past a file it never looked at.
fn search_was_partial(workspace: &Workspace) -> bool {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let found = workspace
        .grep(
            &mut policy,
            std::slice::from_ref(&Labelled::trusted("needle".to_string())),
            &Labelled::trusted(".".to_string()),
            None,
            true,
            1,
        )
        .expect("grep succeeds");
    let proof = policy.authorise_content_release("test", "matches");
    found.declassify(&proof).unvisited
}

/// The number in a settings file is what bounds the walk, not the built-in cap.
///
/// This is what the clause is for: on a tree past the built-in cap every search comes back
/// partial, and a partial search reads like a complete one. A front end that builds its
/// workspace without passing the caps on leaves the person who wrote the number no way to say
/// so, and nothing about the answer says the number was ignored.
#[test]
fn a_turn_searches_under_the_file_cap_the_project_settings_name() {
    let held = project("file-cap");
    let root = held.path();
    std::fs::create_dir_all(root.join(".bravebot")).expect("a settings directory");
    std::fs::write(
        root.join(".bravebot/settings.json"),
        r#"{"search": {"maxFiles": 4}}"#,
    )
    .expect("a settings file");
    // Past the cap the file names and far below the built-in one, so a walk that ignored the
    // file reaches every one of them and reports nothing left over.
    for n in 0..12 {
        std::fs::write(root.join(format!("f{n:05}.txt")), "filler").expect("a file");
    }

    let settings = settings_of(root);
    let workspace = turn_workspace(root.to_path_buf(), &settings).expect("a workspace");

    assert!(
        search_was_partial(&workspace),
        "the settings capped the walk at four files of thirteen and the search claimed to have read them all"
    );
    assert!(
        !search_was_partial(&Workspace::new(root).expect("a workspace")),
        "the tree must be small enough that only the configured cap can truncate a search of it"
    );
}

/// A file naming only the time cap gets that one, and leaves the file cap where it was.
///
/// Raising a cap is invisible in a search that finishes, so the caps a turn's workspace holds
/// are what says whether a number reached it. The two are independent under the clause, so a
/// wiring that read one key and dropped the other has to fail here.
#[test]
fn a_turn_searches_under_the_time_cap_the_project_settings_name() {
    let held = project("time-cap");
    let root = held.path();
    std::fs::create_dir_all(root.join(".bravebot")).expect("a settings directory");
    std::fs::write(
        root.join(".bravebot/settings.json"),
        r#"{"search": {"maxSeconds": 45}}"#,
    )
    .expect("a settings file");

    let settings = settings_of(root);
    let workspace = turn_workspace(root.to_path_buf(), &settings).expect("a workspace");

    assert_eq!(
        workspace.search_caps(),
        (MAX_SEARCH_FILES, Duration::from_secs(45)),
        "the file named the seconds and nothing else, so only that cap moves"
    );
}

/// Settings that named no cap leave a turn's searches on the built-in numbers.
///
/// Absence has to arrive as absence: a front end that turned a key nobody wrote into a zero
/// would cap a search at nothing and answer every pattern with nothing found, which is the
/// reading of zero the clause rules out.
#[test]
fn caps_nobody_named_leave_a_turn_on_the_built_in_ones() {
    let held = project("no-caps");
    let root = held.path();

    let settings = settings_of(root);
    let configured = turn_workspace(root.to_path_buf(), &settings).expect("a workspace");
    let built_in = Workspace::new(root).expect("a workspace");

    assert_eq!(configured.search_caps(), built_in.search_caps());
    assert_eq!(configured.search_caps().0, MAX_SEARCH_FILES);
}

/// The project a turn is given is the workspace the caps are put on.
///
/// A helper that built the workspace somewhere else, such as the process's current directory,
/// would pass every assertion above and search the wrong tree.
#[test]
fn a_turn_runs_on_the_project_it_was_given() {
    let held = project("root");
    let root: PathBuf = held.path().to_path_buf();

    let settings = settings_of(&root);
    let workspace = turn_workspace(root.clone(), &settings).expect("a workspace");

    assert_eq!(
        workspace.root(),
        root.canonicalize().expect("a real directory")
    );
}

/// The front end's own binary, driven the way the desktop application drives it: a project with
/// a settings file, a turn, and a search the planner asks for.
///
/// Everything above builds the workspace through the helper the turn uses. This runs the turn,
/// so the line that calls the helper is covered too: a front end that went back to building the
/// workspace without the caps passes every test above and fails this one.
mod through_a_turn {
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdout, Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    /// How long any one answer from the binary is waited for.
    ///
    /// Bounded rather than blocking: a front end that never answers has to fail this test rather
    /// than hang the suite behind it.
    const PATIENCE: Duration = Duration::from_secs(120);

    /// A project and a home of a test's own, removed when the test ends.
    ///
    /// Under the build directory rather than the system temporary one, which is shared between
    /// users and where a name this predictable is somebody else's to create first.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("project")).expect("a project directory");
            std::fs::create_dir_all(path.join("home")).expect("a home directory");
            Self {
                path: path.canonicalize().expect("a real scratch directory"),
            }
        }

        fn project(&self) -> PathBuf {
            self.path.join("project")
        }

        /// Fill the project with files a search walks, and hide the needle in the last of them.
        ///
        /// The needle sorts last so that a walk which stopped at a cap is a search that missed
        /// it, which is the harm the clause is about: nothing found reads like nothing there.
        fn filled(self) -> Self {
            for n in 0..12 {
                std::fs::write(self.project().join(format!("f{n:05}.txt")), "filler\n")
                    .expect("a file");
            }
            std::fs::write(self.project().join("zz-needle.txt"), "the needle is here\n")
                .expect("a file");
            self
        }

        /// Write the project's settings file, as a person configuring this project does.
        fn with_project_settings(self, json: &str) -> Self {
            let directory = self.project().join(".bravebot");
            std::fs::create_dir_all(&directory).expect("a settings directory");
            std::fs::write(directory.join("settings.json"), json).expect("a settings file");
            self
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// A model service that asks for one search and then says it is done.
    ///
    /// The planner is what decides to search, so a turn that searches needs one. This answers the
    /// roster with a single model, the first request with a call to `search`, and the request
    /// after that, which carries the result, with a word to end the turn on.
    fn stub_service() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || answer(stream));
            }
        });
        endpoint
    }

    fn answer(mut stream: std::net::TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
        let mut start = String::new();
        if reader.read_line(&mut start).unwrap_or(0) == 0 {
            return;
        }
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                break;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; length];
        let _ = reader.read_exact(&mut body);

        let (kind, payload) = if start.starts_with("GET") {
            (
                "application/json",
                json!([{"key": "stub-model", "display_name": "Stub",
                        "capabilities": ["tools"],
                        "options": {"access": "basic_and_premium",
                                    "long_conversation_warning_character_limit": 400_000}}])
                .to_string(),
            )
        } else {
            // Whether the search has already run, read off the conversation the request carries
            // rather than off a substring of it: the planner is told about every tool it has, so
            // the name of one appears in the very first request.
            let asked: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let answered = asked["messages"]
                .as_array()
                .is_some_and(|messages| messages.iter().any(|m| m["role"] == "tool"));
            let delta = if answered {
                json!({"role": "assistant", "content": "done"})
            } else {
                json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "t1",
                    "type": "function",
                    "function": {"name": "search",
                                 "arguments": r#"{"pattern":"needle","directory":"."}"#}}]})
            };
            let finish = if answered { "stop" } else { "tool_calls" };
            let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
                "model": "stub-model",
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
            (
                "text/event-stream",
                format!("data: {chunk}\n\ndata: [DONE]\n\n"),
            )
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }

    /// The binary, in an environment the test wrote rather than the one it inherited.
    ///
    /// Nothing is inherited: a developer's own exports name a real service, and a turn that
    /// reached one would answer out of a model rather than out of this test.
    fn front_end(home: &Path, endpoint: &str) -> Child {
        Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
            .env("BRAVE_AI_CHAT_DEFAULT_MODEL", "stub-model")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs")
    }

    /// Every line the front end writes, as it writes them.
    fn lines(stdout: ChildStdout) -> mpsc::Receiver<Value> {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && sender.send(value).is_err()
                {
                    break;
                }
            }
        });
        receiver
    }

    /// Wait for the answer to a request, or for a named event, whichever the test asked for.
    fn until(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    fn send(child: &mut Child, id: u64, method: &str, params: Value) {
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let stdin = child.stdin.as_mut().expect("the front end reads requests");
        writeln!(stdin, "{line}").expect("the request is written");
        stdin.flush().expect("the request is sent");
    }

    /// What one turn's search reported, having been asked for by the planner.
    ///
    /// The note the front end draws beside a finished tool, which says how many files the search
    /// walked. That is the product's own report rather than a rendering of the test's.
    fn note_of_a_search(project: &Path, home: &Path) -> String {
        let endpoint = stub_service();
        let mut child = front_end(home, &endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));

        send(
            &mut child,
            1,
            "session.new",
            json!({"directory": project.display().to_string()}),
        );
        let opened = until(&said, |message| message["id"] == 1);
        let session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        send(
            &mut child,
            2,
            "trust.reply",
            json!({"session": session, "trusted": true}),
        );
        until(&said, |message| message["id"] == 2);
        send(
            &mut child,
            3,
            "turn.send",
            json!({"session": session, "prompt": "find the needle"}),
        );
        let finished = until(&said, |message| {
            message["event"] == "tool.finished" || message["event"] == "turn.error"
        });
        let _ = child.kill();
        assert_eq!(
            finished["event"], "tool.finished",
            "the turn failed instead of searching: {finished}"
        );
        finished["data"]["note"]
            .as_str()
            .expect("a finished tool carries a note")
            .to_string()
    }

    /// A turn in the desktop front end searches under the cap the project's settings name.
    ///
    /// The cap is lowered here because a raised one is invisible in any tree a test can build,
    /// and the direction does not change what is being checked: whether the number in the file
    /// reaches the search. A front end that builds its workspace without passing the caps on
    /// walks the whole tree, which is what the second half of this asserts is otherwise possible.
    #[test]
    fn a_turns_search_runs_under_the_cap_the_settings_name() {
        let capped = Scratch::new("bridge-search-capped")
            .filled()
            .with_project_settings(r#"{"search": {"maxFiles": 4}}"#);
        // A cap of zero is absence under the clause, so this tree is searched to the end. It
        // carries a settings file of its own so that the two trees hold the same number of
        // files, which is what makes the two counts below comparable.
        let uncapped = Scratch::new("bridge-search-uncapped")
            .filled()
            .with_project_settings(r#"{"search": {"maxFiles": 0}}"#);

        let under_the_cap = note_of_a_search(&capped.project(), &capped.path.join("home"));
        let under_the_built_in = note_of_a_search(&uncapped.project(), &uncapped.path.join("home"));

        assert_eq!(
            under_the_built_in, "1 match in 14 files",
            "the same tree with no cap in force must be searched to the end, or the assertion \
             below says nothing about the settings"
        );
        assert_eq!(
            under_the_cap, "0 matches in 5 files",
            "the project asked for four files and the search walked the whole tree"
        );
    }
}
