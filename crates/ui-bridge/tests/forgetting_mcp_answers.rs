//! A project's standing MCP answers on the Permissions channel: `permissions.list` carries
//! `mcpRemembered` for the session's project, and `permissions.revoke` with `kind: "mcp"` forgets
//! them, as `bravebot mcp forget` does in the terminal.
//!
//! Driven through the binary with a home of the test's own, because the records are read from the
//! state directory and a test in-process would be reading the developer's.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_config::mcp::{self as records, Projects, Standing};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// A project and another beside it, and a home of a test's own, under the build directory,
/// removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project")).unwrap();
        std::fs::create_dir_all(path.join("elsewhere")).unwrap();
        std::fs::create_dir_all(path.join("home/.bravebot")).unwrap();
        Self {
            path: path.canonicalize().unwrap(),
        }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    fn elsewhere(&self) -> PathBuf {
        self.path.join("elsewhere")
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    fn state(&self) -> PathBuf {
        self.home().join(".bravebot")
    }

    /// Record answer 2 at a server's question in each of `servers`, and at a call's question for
    /// each `(alias, tool, project)` of `tools`.
    fn answered(&self, servers: &[&Path], tools: &[(&str, &str, &Path)]) {
        let mut projects = Projects::default();
        for project in servers {
            assert!(projects.add(project));
        }
        std::fs::write(records::projects_file(&self.state()), projects.to_text()).unwrap();
        let mut standing = Standing::default();
        for (alias, tool, project) in tools {
            assert!(standing.add(alias, tool, project));
        }
        std::fs::write(records::tools_file(&self.state()), standing.to_text()).unwrap();
    }

    fn records(&self) -> (String, String) {
        (
            std::fs::read_to_string(records::projects_file(&self.state())).unwrap_or_default(),
            std::fs::read_to_string(records::tools_file(&self.state())).unwrap_or_default(),
        )
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    next: u64,
}

impl FrontEnd {
    fn start(home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
            .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let said = lines(child.stdout.take().unwrap());
        Self {
            child,
            said,
            next: 0,
        }
    }

    /// Send a request and return the whole reply, `ok` or `error`.
    fn reply(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if message["id"] == id => return message,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("{method} was never answered");
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let reply = self.reply(method, params);
        reply
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {reply}"))
    }

    fn session(&mut self, project: &Path) -> String {
        let made = self.call(
            "session.new",
            json!({"directory": project.display().to_string()}),
        );
        made["session"].as_str().unwrap().to_string()
    }

    fn listed(&mut self, session: &str) -> Value {
        self.call("permissions.list", json!({"session": session}))["mcpRemembered"].clone()
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

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

fn tools(names: &[&str]) -> Value {
    json!(
        names
            .iter()
            .map(|name| json!({"tool": name, "shownTool": name}))
            .collect::<Vec<_>>()
    )
}

/// The list names the project it is about, and says what is remembered there and nothing about any
/// other project: the servers alone, the tools alone, both, or nothing.
#[test]
fn the_list_says_what_is_remembered_for_the_sessions_project() {
    let scratch = Scratch::new("bridge-mcp-remembered-list");
    let project = scratch.project();
    let elsewhere = scratch.elsewhere();
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&project);

    let nothing = front.listed(&session);
    assert_eq!(
        nothing,
        json!({
            "project": project.display().to_string(),
            "shownProject": project.display().to_string(),
            "everyServer": false,
            "tools": [],
            "forgettable": true,
        })
    );

    scratch.answered(&[&project], &[("weather", "get_alerts", &elsewhere)]);
    let servers = front.listed(&session);
    assert_eq!(servers["everyServer"], true);
    assert_eq!(servers["tools"], json!([]));

    scratch.answered(
        &[&elsewhere],
        &[
            ("weather", "get_forecast", &project),
            ("weather", "get_alerts", &project),
        ],
    );
    let tools_alone = front.listed(&session);
    assert_eq!(tools_alone["everyServer"], false);
    assert_eq!(
        tools_alone["tools"],
        tools(&["weather:get_alerts", "weather:get_forecast"])
    );

    scratch.answered(
        &[&project, &elsewhere],
        &[
            ("weather", "get_forecast", &project),
            ("news", "headlines", &elsewhere),
        ],
    );
    let both = front.listed(&session);
    assert_eq!(both["everyServer"], true);
    assert_eq!(both["tools"], tools(&["weather:get_forecast"]));
    assert_eq!(both["forgettable"], true);
}

/// A record that is there and cannot be read is reported with the file it is, not listed as if
/// nothing were remembered, and forgetting is refused with both records left as they are.
#[test]
fn an_unreadable_record_is_reported_and_nothing_is_forgotten() {
    let scratch = Scratch::new("bridge-mcp-remembered-unreadable");
    let project = scratch.project();
    scratch.answered(&[&project], &[("weather", "get_forecast", &project)]);
    let too_large = " ".repeat(64 * 1024 + 1);
    let tools_file = records::tools_file(&scratch.state());
    std::fs::write(&tools_file, &too_large).unwrap();
    let before = scratch.records();
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&project);

    let listed = front.listed(&session);
    assert!(listed.get("everyServer").is_none(), "{listed}");
    assert!(listed.get("tools").is_none(), "{listed}");
    assert_eq!(listed["forgettable"], false);
    assert_eq!(
        listed["unreadable"]["path"],
        tools_file.display().to_string()
    );
    assert_eq!(
        listed["unreadable"]["shownPath"],
        tools_file.display().to_string()
    );
    assert!(
        listed["unreadable"]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "{listed}"
    );

    let refused = front.reply(
        "permissions.revoke",
        json!({"session": session, "kind": "mcp"}),
    );
    assert_eq!(refused["error"]["code"], "state_unusable", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&tools_file.display().to_string())),
        "{refused}"
    );
    assert_eq!(scratch.records(), before);
}

/// Forgetting drops this project's answers and no other project's, says what it dropped, and
/// returns the list with nothing remembered here. Forgetting again forgets nothing and says so.
#[test]
fn revoking_forgets_only_this_projects_answers_and_says_what_went() {
    let scratch = Scratch::new("bridge-mcp-remembered-forget");
    let project = scratch.project();
    let elsewhere = scratch.elsewhere();
    scratch.answered(
        &[&project, &elsewhere],
        &[
            ("weather", "get_forecast", &project),
            ("weather", "get_alerts", &elsewhere),
        ],
    );
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&project);

    let revoked = front.call(
        "permissions.revoke",
        json!({"session": session, "kind": "mcp"}),
    );
    assert_eq!(
        revoked["forgotten"],
        json!({"everyServer": true, "tools": tools(&["weather:get_forecast"])})
    );
    assert_eq!(revoked["mcpRemembered"]["everyServer"], false);
    assert_eq!(revoked["mcpRemembered"]["tools"], json!([]));
    assert!(revoked.get("paths").is_some() && revoked.get("commands").is_some());

    let state = scratch.state();
    assert!(!Projects::read(&state).contains(&project));
    assert!(Projects::read(&state).contains(&elsewhere));
    let standing = Standing::read(&state);
    assert!(!standing.covers("weather", "get_forecast", &project));
    assert!(standing.covers("weather", "get_alerts", &elsewhere));

    let again = front.call(
        "permissions.revoke",
        json!({"session": session, "kind": "mcp"}),
    );
    assert_eq!(
        again["forgotten"],
        json!({"everyServer": false, "tools": []})
    );

    // A list is not a revoke, and says nothing was forgotten by carrying no `forgotten`.
    let listed = front.call("permissions.list", json!({"session": session}));
    assert!(listed.get("forgotten").is_none(), "{listed}");
}
