//! `bravebot acp` run as a process: an editor starts the built binary and speaks the Agent Client
//! Protocol to it (CLI-39, docs/specs/acp.md).
//!
//! The loop and every clause of ACP-1 to ACP-8 are the ones `crates/ui-bridge/tests/acp.rs` pins
//! in process. What these tests add is the entry point: the command is dispatched rather than taken
//! as a task, nothing the command line does before it reaches standard output, and an answer that
//! an editor gives or never gives has the same effect when the program is started this way.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A home and a project directory under this crate's build directory, emptied first.
fn scratch(name: &str) -> (PathBuf, PathBuf) {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("acp-subcommand-{name}"));
    let _ = std::fs::remove_dir_all(&home);
    let project = home.join("project");
    std::fs::create_dir_all(home.join(".bravebot")).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    (
        std::fs::canonicalize(&home).unwrap(),
        std::fs::canonicalize(&project).unwrap(),
    )
}

/// `bravebot` with `arguments`, in `project`, with `home` as the person's.
fn bravebot(home: &Path, project: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    command
        .args(arguments)
        .current_dir(project)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove(bravebot_agent::home::STATE_VARIABLE)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// A running `bravebot acp` and the lines it has written to standard output.
struct Editor {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    seen: Vec<Value>,
    cursor: usize,
    next: u64,
}

impl Editor {
    fn start(mut command: Command) -> Self {
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (sent, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sent.send(line).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            seen: Vec::new(),
            cursor: 0,
            next: 0,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let id = self.next;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    /// The next message matching `wanted`. Each line is checked to be one JSON value, since
    /// standard output holds nothing else (ACP-6).
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        let until = Instant::now() + endpoint::LIMIT * 2;
        loop {
            while self.cursor < self.seen.len() {
                self.cursor += 1;
                if wanted(&self.seen[self.cursor - 1]) {
                    return self.seen[self.cursor - 1].clone();
                }
            }
            let wait = until.saturating_duration_since(Instant::now());
            let line = self
                .lines
                .recv_timeout(wait.max(Duration::from_millis(1)))
                .unwrap_or_else(|_| panic!("never arrived: {:#?}", self.seen));
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| panic!("a line of standard output that is not JSON: {line:?}"));
            self.seen.push(message);
        }
    }

    fn response(&mut self, id: u64) -> Value {
        self.until(|message| message["id"] == json!(id) && message.get("method").is_none())
    }

    fn session(&mut self, project: &Path) -> String {
        let id = self.request("session/new", json!({"cwd": project, "mcpServers": []}));
        self.response(id)["result"]["sessionId"]
            .as_str()
            .expect("a session")
            .to_string()
    }

    fn prompt(&mut self, session: &str, words: &str) -> u64 {
        self.request(
            "session/prompt",
            json!({"sessionId": session, "prompt": [{"type": "text", "text": words}]}),
        )
    }

    /// Answer the next permission request with `option`, whether or not it was offered.
    fn answer_next(&mut self, option: &str) -> Value {
        let asked = self.until(|message| message["method"] == "session/request_permission");
        self.send(&json!({"jsonrpc": "2.0", "id": asked["id"],
            "result": {"outcome": {"outcome": "selected", "optionId": option}}}));
        asked
    }

    /// Close standard input and wait for the program to end, answering what it wrote to standard
    /// output after the last line this had read, and what it wrote to standard error.
    fn finish(mut self) -> (std::process::ExitStatus, Vec<String>, String) {
        drop(self.stdin.take());
        let until = Instant::now() + endpoint::LIMIT * 2;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < until, "the program did not end");
            std::thread::sleep(Duration::from_millis(20));
        };
        let rest = self.lines.iter().collect();
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut self.child.stderr.take().unwrap(), &mut stderr).unwrap();
        (status, rest, stderr)
    }
}

/// Write the settings file that makes `endpoint` the model, and return its path.
fn settings_for(home: &Path, endpoint: &str) -> PathBuf {
    let file = home.join("acp-settings.json");
    let settings = json!({"model": "acp-test/test", "provider": {"acp-test": {
        "options": {"baseURL": endpoint}, "models": {"test": {}}
    }}});
    std::fs::write(&file, settings.to_string()).unwrap();
    file
}

/// CLI-39: the command answers `initialize` on standard output with one line, and the answer is
/// the one `bravebot-acp` gives, with no model, credential or settings file in the home.
#[test]
fn acp_answers_initialize_with_one_json_line_and_nothing_else() {
    let (home, project) = scratch("initialize");
    let mut editor = Editor::start(bravebot(&home, &project, &["acp"]));
    let id = editor.request("initialize", json!({"protocolVersion": 1}));
    let answer = editor.response(id);
    assert_eq!(answer["jsonrpc"], "2.0", "{answer}");
    assert_eq!(answer["result"]["protocolVersion"], 1, "{answer}");
    let capabilities = &answer["result"]["agentCapabilities"];
    assert_eq!(capabilities["loadSession"], false, "{answer}");
    assert_eq!(answer["result"]["authMethods"], json!([]), "{answer}");
    assert_eq!(
        capabilities["promptCapabilities"]["image"], true,
        "{answer}"
    );
    let (status, rest, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
    assert_eq!(rest, Vec::<String>::new(), "more than one line on stdout");
}

/// CLI-39: an input that ends before a line was sent leaves standard output empty and the status 0,
/// so nothing the command line does on the way to the loop is written there.
#[test]
fn acp_with_nothing_to_read_writes_nothing_to_standard_output() {
    let (home, project) = scratch("eof");
    let editor = Editor::start(bravebot(&home, &project, &["acp"]));
    let (status, rest, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
    assert_eq!(rest, Vec::<String>::new());
}

/// CLI-39 and ACP-7: a flag that answers every question for the person is refused with the command,
/// before anything is read from the editor and with nothing on standard output.
#[test]
fn acp_refuses_the_flags_that_would_stop_the_editor_being_asked() {
    for flag in [
        "--dangerously-skip-permissions",
        "--dangerously-bypass-approvals-and-sandbox",
    ] {
        let (home, project) = scratch("bypass");
        let output = bravebot(&home, &project, &["acp", flag]).output().unwrap();
        assert!(!output.status.success(), "{flag} was taken");
        assert_eq!(output.stdout, b"", "{flag} wrote to standard output");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("acp does not take"),
            "{flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// CLI-39: a flag that shapes a session is refused beside the command, as it is beside the other
/// commands that start no session, and so is a word the command does not read.
#[test]
fn acp_refuses_flags_and_words_that_shape_nothing_it_starts() {
    for arguments in [
        &["acp", "--accept-edits"][..],
        &["acp", "--agent", "reviewer"][..],
        &["acp", "--tools", "read_file"][..],
        &["acp", "extra"][..],
    ] {
        let (home, project) = scratch("refused");
        let output = bravebot(&home, &project, arguments).output().unwrap();
        assert!(!output.status.success(), "{arguments:?} was taken");
        assert_eq!(output.stdout, b"", "{arguments:?} wrote to standard output");
    }
}

/// CLI-39: `--settings` names the file the sessions read, as it does for `bravebot-acp`, so the
/// model a prompt reaches is the one the file names, and a slash word in the prompt reaches it as
/// the text the editor sent (ACP-1).
#[test]
fn acp_reads_the_named_settings_and_sends_a_slash_word_to_the_model_as_text() {
    let (home, project) = scratch("settings");
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let settings = settings_for(&home, &config.endpoint);
    let mut editor = Editor::start(bravebot(
        &home,
        &project,
        &["acp", "--settings", settings.to_str().unwrap()],
    ));
    let session = editor.session(&project);
    let prompt = editor.prompt(&session, "/compact now");
    editor.answer_next("allow-once");
    let done = editor.response(prompt);
    assert_eq!(done["result"]["stopReason"], "end_turn", "{done}");
    let body = requests.recv_timeout(endpoint::LIMIT).unwrap();
    assert!(body.contains("/compact now"), "the model was sent {body}");
    server.join().unwrap();
    let (status, _, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
}

/// CLI-39 with ACP-3: an answer that selects an option the question never offered refuses it, for
/// the trust question and for the write after it, so the file is not written and the prompt ends.
#[test]
fn acp_treats_an_option_never_offered_as_a_refusal() {
    let (home, project) = scratch("unoffered");
    let (config, _requests, _server) = endpoint::endpoint(
        vec![
            endpoint::tool(
                "write_file",
                json!({"path": "output.txt", "contents": "written"}),
            ),
            endpoint::answer(),
        ],
        None,
    );
    let settings = settings_for(&home, &config.endpoint);
    let mut editor = Editor::start(bravebot(
        &home,
        &project,
        &["acp", "--settings", settings.to_str().unwrap()],
    ));
    let session = editor.session(&project);
    let prompt = editor.prompt(&session, "write it");
    editor.answer_next("allow-forever");
    editor.answer_next("allow-forever");
    let done = editor.response(prompt);
    assert_eq!(done["result"]["stopReason"], "end_turn", "{done}");
    let (status, _, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
    assert!(
        !project.join("output.txt").exists(),
        "an answer that selected nothing offered approved a write"
    );
}

/// CLI-39 with ACP-4: the end of the input closes the session the editor opened and the program
/// waits for its `session-ended` hook, which takes a second, so the hook has written when the
/// program is gone. A program that dropped its sessions at the end of its input would be gone
/// before the hook wrote.
#[test]
fn acp_waits_for_the_end_of_an_open_session_before_it_exits() {
    let (home, project) = scratch("hooks");
    let log = home.join("hooks.log");
    let hooks: Vec<Value> = ["session-started", "session-ended"]
        .iter()
        .map(|moment| {
            let wait = if *moment == "session-ended" {
                "sleep 1; "
            } else {
                ""
            };
            json!({"on": moment, "run": ["/bin/sh", "-c",
                format!("{wait}echo {moment} >> '{}'", log.display())]})
        })
        .collect();
    std::fs::write(
        bravebot_config::hooks::hooks_file(&home.join(".bravebot")),
        json!({ "hooks": hooks }).to_string(),
    )
    .unwrap();
    let (config, _requests, _server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let settings = settings_for(&home, &config.endpoint);
    let mut editor = Editor::start(bravebot(
        &home,
        &project,
        &["acp", "--settings", settings.to_str().unwrap()],
    ));
    let session = editor.session(&project);
    let prompt = editor.prompt(&session, "hello");
    editor.answer_next("allow-once");
    editor.response(prompt);
    let (status, _, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
    assert_eq!(
        std::fs::read_to_string(&log).unwrap_or_default(),
        "session-started\nsession-ended\n",
        "the program was gone before the end of the session had run"
    );
}

/// CLI-39: an input that ends while a write waits on the editor leaves the write unwritten, the
/// program ends with the status 0 and nothing but protocol messages is on standard output. The
/// write question is put after the trust question was rejected, so the file stays unwritten for a
/// rejected trust answer too. This test does not tell the refusal of the waiting question from the
/// cancellation of its turn: the end of the input cancels the turn, so nothing is written either
/// way, and the process ends before a model request that follows a refusal could be observed. The
/// refusal itself is pinned in process by
/// `bravebot_ui_bridge::acp::the_end_of_the_input_refuses_a_question_still_waiting` (ACP-4).
#[test]
fn acp_ending_its_input_with_a_question_waiting_writes_nothing_and_exits() {
    let (home, project) = scratch("ends");
    let (config, _requests, _server) = endpoint::endpoint(
        vec![
            endpoint::tool(
                "write_file",
                json!({"path": "output.txt", "contents": "written"}),
            ),
            endpoint::answer(),
        ],
        None,
    );
    let settings = settings_for(&home, &config.endpoint);
    let mut editor = Editor::start(bravebot(
        &home,
        &project,
        &["acp", "--settings", settings.to_str().unwrap()],
    ));
    let session = editor.session(&project);
    editor.prompt(&session, "write it");
    // Untrusted, so that the write is a question: in a trusted directory it is not asked about.
    editor.answer_next("reject-once");
    let write = editor.until(|message| message["method"] == "session/request_permission");
    assert_ne!(write["params"]["toolCall"]["kind"], "other", "{write}");
    let (status, rest, stderr) = editor.finish();
    assert!(status.success(), "{status}: {stderr}");
    for line in rest {
        serde_json::from_str::<Value>(&line)
            .unwrap_or_else(|_| panic!("not a protocol message: {line:?}"));
    }
    assert!(
        !project.join("output.txt").exists(),
        "a write was approved by an input that ended"
    );
    assert!(
        !stderr.contains("still working"),
        "a session was still working when the program ended: {stderr}"
    );
}
