//! Copying Claude Code's sessions over the bridge (SESSION-32).
//!
//! Driven through the binary, because where Claude Code keeps its sessions is read from the
//! process environment, which a test of its own process would share with every other test.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// The name Claude Code gave the session the test writes.
const SOURCE: &str = "4b0c5a6e-1111-2222-3333-444455556666";

/// A project and a home of a test's own, under the build directory, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project")).unwrap();
        std::fs::create_dir_all(path.join("home/.bravebot")).unwrap();
        Self {
            path: path.canonicalize().unwrap(),
        }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    /// One Claude Code transcript of the project, where Claude Code keeps it.
    fn claude_session(&self) {
        let project = self.project();
        let key: String = project
            .display()
            .to_string()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let directory = self.home().join(".claude/projects").join(key);
        std::fs::create_dir_all(&directory).unwrap();
        let cwd = project.display().to_string();
        let lines = [
            json!({"type": "user", "cwd": cwd, "timestamp": "2026-10-06T12:00:01.000Z",
                "message": {"role": "user", "content": "fix the parser"}}),
            json!({"type": "assistant", "cwd": cwd, "timestamp": "2026-10-06T12:00:02.000Z",
                "message": {"role": "assistant", "content": [{"type": "text", "text": "Done."}]}}),
        ];
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(directory.join(format!("{SOURCE}.jsonl")), body).unwrap();
    }

    /// The records the home keeps, by file name.
    fn records(&self) -> Vec<String> {
        let mut names = Vec::new();
        let Ok(stores) = std::fs::read_dir(self.home().join(".bravebot/sessions")) else {
            return names;
        };
        for store in stores.flatten() {
            for file in std::fs::read_dir(store.path()).unwrap().flatten() {
                names.push(file.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The binary, in an environment the test wrote rather than the one it inherited.
struct Bridge {
    child: Child,
    stdin: ChildStdin,
    said: mpsc::Receiver<Value>,
    asked: u64,
}

impl Bridge {
    fn start(home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, said) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && sender.send(value).is_err()
                {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            said,
            asked: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.asked += 1;
        let id = self.asked;
        let line = json!({"id": id, "method": method, "params": params});
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
        loop {
            let message = self
                .said
                .recv_timeout(PATIENCE)
                .expect("the bridge never answered");
            if message["id"] == id {
                return message;
            }
        }
    }

    fn import(&mut self, project: &Path, ids: Option<Value>) -> Value {
        let mut params = json!({"directory": project.display().to_string()});
        if let Some(ids) = ids {
            params["ids"] = ids;
        }
        self.call("session.import", params)
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A listing copies nothing, a copy writes one record, and a second copy finds the first.
///
/// The faults this rejects: a listing that writes, a copy that overwrites what is there, and a
/// copy that reports a session it never wrote.
#[test]
fn a_session_is_listed_then_copied_once() {
    let scratch = Scratch::new("bridge-import-copy");
    scratch.claude_session();
    let mut bridge = Bridge::start(&scratch.home());

    let listed = bridge.import(&scratch.project(), None);
    let found = &listed["ok"]["found"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{listed}");
    assert_eq!(found[0]["sourceId"], SOURCE, "{listed}");
    assert_eq!(found[0]["title"], "fix the parser", "{listed}");
    assert_eq!(found[0]["imported"], false, "{listed}");
    assert!(scratch.records().is_empty(), "a listing wrote a record");
    let id = found[0]["id"].as_str().unwrap().to_string();

    let copied = bridge.import(&scratch.project(), Some(json!([SOURCE])));
    assert_eq!(copied["ok"]["copied"], json!([id]), "{copied}");
    assert_eq!(scratch.records(), vec![format!("{id}.json")]);

    let again = bridge.import(&scratch.project(), Some(json!([SOURCE])));
    assert_eq!(again["ok"]["copied"], json!([]), "{again}");
    assert_eq!(again["ok"]["already"], json!([id]), "{again}");
    let relisted = bridge.import(&scratch.project(), None);
    assert_eq!(relisted["ok"]["found"][0]["imported"], true, "{relisted}");
}

/// A name that is not one listed, or only the beginning of one, copies nothing.
#[test]
fn only_a_listed_name_is_copied() {
    let scratch = Scratch::new("bridge-import-unknown");
    scratch.claude_session();
    let mut bridge = Bridge::start(&scratch.home());

    for ids in [
        json!([&SOURCE[..8]]),
        json!([SOURCE, "elsewhere"]),
        json!("x"),
    ] {
        let refused = bridge.import(&scratch.project(), Some(ids.clone()));
        assert_eq!(refused["error"]["code"], "bad_request", "{ids}: {refused}");
    }
    assert!(scratch.records().is_empty(), "{:?}", scratch.records());

    let nowhere = bridge.import(&scratch.path.join("missing"), None);
    assert_eq!(nowhere["error"]["code"], "not_a_directory", "{nowhere}");
}
