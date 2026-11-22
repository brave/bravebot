//! The variant of a model's options the desktop window picks for one session (BACKEND-66).
//!
//! Driven through the binary, because the names come from the provider blocks in the person's
//! settings, which the bridge reads for the session's project.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

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
        std::fs::create_dir_all(path.join("project")).expect("a project directory");
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    /// The person's own settings file.
    fn settings(&self, settings: &Value) {
        std::fs::write(
            self.home().join(".bravebot/settings.json"),
            settings.to_string(),
        )
        .expect("the settings are written");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The binary, in an environment the test wrote rather than the one it inherited. Nothing here
/// sends a turn, so the service endpoints name nothing that answers.
fn front_end(home: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
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

/// The answer to request `id`.
fn answer(said: &mpsc::Receiver<Value>, id: u64) -> Value {
    let deadline = std::time::Instant::now() + PATIENCE;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) if message["id"] == id => return message,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("the front end never answered request {id}");
}

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// BACKEND-66: the window reads what the model it has picked offers, without a disabled variant,
/// picks one, is refused a name the model does not offer with the pick left as it was, and drops
/// the pick. A session the bridge does not have open is refused.
#[test]
fn session_variant_lists_picks_refuses_and_drops() {
    let scratch = Scratch::new("variant-lists-picks-refuses-and-drops");
    scratch.settings(&json!({"provider": {"gw": {
        "options": {"baseURL": "https://gateway.example.invalid/v1"},
        "models": {"m": {"variants": {
            "terse": {"textVerbosity": "low"},
            "wide": {"textVerbosity": "high"},
            "off": {"disabled": true}
        }}}
    }}}));
    let mut child = front_end(&scratch.home());
    let said = lines(child.stdout.take().expect("the front end answers"));

    send(
        &mut child,
        1,
        "session.new",
        json!({"directory": scratch.project().display().to_string()}),
    );
    let opened = answer(&said, 1);
    let session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("no session was opened: {opened}"))
        .to_string();

    let mut id = 1;
    let mut ask = |params: Value| {
        id += 1;
        send(&mut child, id, "session.variant", params);
        answer(&said, id)
    };

    let read = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(
        read["ok"],
        json!({"model": "gw/m", "offered": ["terse", "wide"], "variant": null}),
        "{read}"
    );

    let picked = ask(json!({"session": session, "model": "gw/m", "variant": "terse"}));
    assert_eq!(picked["ok"]["variant"], "terse", "{picked}");

    let refused = ask(json!({"session": session, "model": "gw/m", "variant": "typo"}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("terse, wide"), "{refused}");
    let after = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(after["ok"]["variant"], "terse", "{after}");

    let dropped = ask(json!({"session": session, "model": "gw/m", "variant": null}));
    assert_eq!(dropped["ok"]["variant"], Value::Null, "{dropped}");
    let after = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(after["ok"]["variant"], Value::Null, "{after}");

    let unknown = ask(json!({"session": "no-such-session", "model": "gw/m"}));
    assert_eq!(unknown["error"]["code"], "no_such_session", "{unknown}");

    drop(ask);
    let _ = child.kill();
    let _ = child.wait();
}
