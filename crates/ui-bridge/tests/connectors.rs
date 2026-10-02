//! Connectors in the desktop front end: the MCP servers a person declares, approves and turns on
//! from the window (docs/specs/mcp-servers.md SERVERS-1, SERVERS-2, SERVERS-3, SERVERS-5).
//!
//! Driven through the binary with a home of the test's own, because the files written are the
//! person's own and a test in-process would be writing the developer's.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// A home of a test's own under the build directory, outside any repository's `.bravebot`, removed
/// when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    fn file(&self, name: &str) -> String {
        std::fs::read_to_string(self.home().join(".bravebot").join(name)).unwrap_or_default()
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
            .env("PATH", "/usr/bin:/bin")
            .env("BRAVEBOT_LOCALE", "en-US")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            said,
            next: 0,
        }
    }

    /// Send a request and return its whole answer, `ok` or `error`.
    fn answered(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let stdin = self.child.stdin.as_mut().expect("requests are read");
        writeln!(stdin, "{line}").expect("the request is written");
        stdin.flush().expect("the request is sent");
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if message["id"] == id => return message,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        panic!("{method} was never answered");
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    fn refused(&mut self, method: &str, params: Value) -> String {
        let answered = self.answered(method, params);
        answered["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{method} was not refused: {answered}"))
            .to_string()
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

fn remote(alias: &str, url: &str) -> Value {
    json!({"alias": alias, "transport": "http", "url": url})
}

fn with(mut params: Value, key: &str, value: Value) -> Value {
    params[key] = value;
    params
}

fn named<'a>(listed: &'a Value, alias: &str) -> Option<&'a Value> {
    listed["connectors"]
        .as_array()?
        .iter()
        .find(|connector| connector["alias"] == alias)
}

/// SERVERS-3: the form is not the approval. A preview writes nothing, and connecting takes back the
/// fingerprint of what was shown. Connected, the server is declared, approved and requested in the
/// home settings file; disconnected, only the request goes; removed, the declaration and approval go.
#[test]
fn a_connector_is_declared_approved_and_requested_only_as_it_was_shown() {
    let scratch = Scratch::new("bridge-connectors-remote");
    let mut front = FrontEnd::start(&scratch.home());

    let listed = front.call("connectors.list", json!({}));
    assert_eq!(listed["connectors"], json!([]));
    assert_eq!(listed["writable"], true);

    let form = remote("calc", "http://localhost:3333/mcp");
    let shown = front.call("connectors.preview", form.clone());
    assert_eq!(shown["transport"], "http");
    assert_eq!(shown["url"], "http://localhost:3333/mcp");
    assert_eq!(shown["exists"], false);
    let fingerprint = shown["fingerprint"]
        .as_str()
        .expect("a fingerprint")
        .to_string();
    assert!(
        scratch.file("mcp.json").is_empty(),
        "a preview wrote something"
    );

    // A form that changed after it was shown connects nothing.
    let moved = with(
        remote("calc", "http://localhost:4444/mcp"),
        "fingerprint",
        json!(fingerprint),
    );
    let said = front.refused("connectors.connect", moved);
    assert!(said.contains("other than what was shown"), "{said}");
    assert!(scratch.file("mcp.json").is_empty());

    let connected = front.call(
        "connectors.connect",
        with(form.clone(), "fingerprint", json!(fingerprint)),
    );
    let calc = named(&connected, "calc").expect("listed");
    assert_eq!(calc["connected"], true);
    assert_eq!(calc["approved"], true);
    assert_eq!(calc["requested"], true);
    assert!(scratch.file("mcp.json").contains("localhost:3333"));
    assert!(scratch.file("mcp-approved").contains(&fingerprint));
    let settings: Value = serde_json::from_str(&scratch.file("settings.json")).expect("json");
    assert_eq!(settings["mcp"]["request"], json!(["calc"]));

    let off = front.call("connectors.disconnect", json!({"alias": "calc"}));
    let calc = named(&off, "calc").expect("still declared");
    assert_eq!(calc["connected"], false);
    assert_eq!(calc["requested"], false);
    assert_eq!(calc["approved"], true, "turning it off keeps the approval");
    let settings: Value = serde_json::from_str(&scratch.file("settings.json")).expect("json");
    assert_eq!(settings["mcp"]["request"], json!([]));

    let gone = front.call("connectors.remove", json!({"alias": "calc"}));
    assert!(named(&gone, "calc").is_none());
    assert!(!scratch.file("mcp.json").contains("calc"));
    assert!(!scratch.file("mcp-approved").contains(&fingerprint));
}

/// SERVERS-3: removing a connector rewrites the approvals file, so an approvals file that cannot be
/// read refuses the removal and is left as it was, with the declaration still there.
#[test]
fn removing_a_connector_leaves_an_approvals_file_it_cannot_read() {
    let scratch = Scratch::new("bridge-connectors-unreadable-approvals");
    let mut front = FrontEnd::start(&scratch.home());
    for (alias, url) in [
        ("calc", "http://localhost:3333/mcp"),
        ("other", "http://localhost:4444/mcp"),
    ] {
        let form = remote(alias, url);
        let shown = front.call("connectors.preview", form.clone());
        front.call(
            "connectors.connect",
            with(form, "fingerprint", shown["fingerprint"].clone()),
        );
    }

    let approvals = scratch.home().join(".bravebot/mcp-approved");
    let mut text = scratch.file("mcp-approved");
    text.push_str(&"#".repeat(70 * 1024));
    std::fs::write(&approvals, &text).expect("an approvals file too large to read");

    let said = front.refused("connectors.remove", json!({"alias": "calc"}));
    assert!(said.contains("cannot be read"), "{said}");
    assert_eq!(
        std::fs::read_to_string(&approvals).expect("still there"),
        text,
        "the approvals file was written over"
    );
    assert!(scratch.file("mcp.json").contains("calc"));
}

/// Adding a connector cannot quietly change another of the same name. A settings page that means
/// to change one says so.
#[test]
fn a_connector_of_the_same_name_is_replaced_only_when_asked() {
    let scratch = Scratch::new("bridge-connectors-replace");
    let mut front = FrontEnd::start(&scratch.home());
    let first = remote("calc", "http://localhost:3333/mcp");
    let shown = front.call("connectors.preview", first.clone());
    front.call(
        "connectors.connect",
        with(first, "fingerprint", shown["fingerprint"].clone()),
    );

    let second = remote("calc", "http://localhost:4444/mcp");
    let shown = front.call("connectors.preview", second.clone());
    assert_eq!(shown["exists"], true);
    assert_eq!(shown["same"], false);
    let said = front.refused(
        "connectors.connect",
        with(second.clone(), "fingerprint", shown["fingerprint"].clone()),
    );
    assert!(said.contains("exists already"), "{said}");
    assert!(scratch.file("mcp.json").contains("3333"));

    let replaced = front.call(
        "connectors.connect",
        with(
            with(second, "fingerprint", shown["fingerprint"].clone()),
            "replace",
            json!(true),
        ),
    );
    assert_eq!(
        named(&replaced, "calc").unwrap()["url"],
        "http://localhost:4444/mcp"
    );
    assert_eq!(named(&replaced, "calc").unwrap()["connected"], true);
}

/// SERVERS-10: a stored value is written to the declarations and never sent back to the window,
/// a settings page keeps it without the person typing it again, and a program found through PATH
/// is given PATH.
#[test]
fn a_stored_value_is_kept_and_never_shown() {
    let scratch = Scratch::new("bridge-connectors-stored");
    let mut front = FrontEnd::start(&scratch.home());
    let form = json!({"alias": "github", "transport": "stdio",
        "command": ["github-mcp-server", "stdio", "--read-only"],
        "variables": [{"name": "GITHUB_PERSONAL_ACCESS_TOKEN", "value": "a-secret-token"}]});

    let shown = front.call("connectors.preview", form.clone());
    assert!(!shown.to_string().contains("a-secret-token"), "{shown}");
    assert_eq!(
        shown["variables"],
        json!([{"name": "GITHUB_PERSONAL_ACCESS_TOKEN", "stored": true},
               {"name": "PATH", "stored": false}])
    );
    let listed = front.call(
        "connectors.connect",
        with(form, "fingerprint", shown["fingerprint"].clone()),
    );
    assert!(!listed.to_string().contains("a-secret-token"), "{listed}");
    assert!(scratch.file("mcp.json").contains("a-secret-token"));

    // Another flag, and the token kept rather than typed again.
    let changed = json!({"alias": "github", "transport": "stdio",
        "command": ["github-mcp-server", "stdio", "--read-only", "--lockdown-mode"],
        "variables": [{"name": "GITHUB_PERSONAL_ACCESS_TOKEN", "keep": true}]});
    let shown = front.call("connectors.preview", changed.clone());
    assert_eq!(shown["exists"], true);
    assert!(!shown.to_string().contains("a-secret-token"), "{shown}");
    front.call(
        "connectors.connect",
        with(
            with(changed, "fingerprint", shown["fingerprint"].clone()),
            "replace",
            json!(true),
        ),
    );
    let declared = scratch.file("mcp.json");
    assert!(declared.contains("--lockdown-mode"), "{declared}");
    assert!(declared.contains("a-secret-token"), "{declared}");

    // Nothing to keep is refused rather than read as empty.
    let said = front.refused(
        "connectors.preview",
        json!({"alias": "other", "transport": "stdio", "command": ["x"],
            "variables": [{"name": "TOKEN", "keep": true}]}),
    );
    assert!(said.contains("no stored value"), "{said}");
}

/// A name or a form the declarations cannot hold is refused before anything is written.
#[test]
fn a_form_the_declarations_cannot_hold_is_refused() {
    let scratch = Scratch::new("bridge-connectors-refused");
    let mut front = FrontEnd::start(&scratch.home());
    for (form, says) in [
        (remote("a:b", "http://localhost/mcp"), "name"),
        (remote("calc", "ftp://localhost/mcp"), ""),
        (remote("calc", "http://user:pass@localhost/mcp"), ""),
        (
            json!({"alias": "calc", "transport": "http", "url": "http://localhost/mcp",
                "variables": [{"name": "TOKEN", "value": "x"}]}),
            "no variables",
        ),
        (
            json!({"alias": "calc", "transport": "stdio", "command": []}),
            "needs a program",
        ),
        (
            json!({"alias": "calc", "transport": "stdio", "command": ["x"],
                "variables": [{"name": "not a name"}]}),
            "variable",
        ),
        (
            json!({"alias": "calc", "transport": "stdio", "command": ["x"],
                "directory": "/no/such/directory"}),
            "not a directory",
        ),
    ] {
        let said = front.refused("connectors.preview", form.clone());
        assert!(said.contains(says), "{form}: {said}");
    }
    assert!(scratch.file("mcp.json").is_empty());
}

/// A connector turned off is turned on again from what the agent reports of it: its stored values
/// kept and its other variables named. That is the declaration it already has, so nothing new is
/// approved and its fingerprint is the one recorded.
#[test]
fn a_connector_turned_on_again_from_its_listing_is_the_same_declaration() {
    let scratch = Scratch::new("bridge-connectors-again");
    let mut front = FrontEnd::start(&scratch.home());
    let form = json!({"alias": "weather", "transport": "stdio",
        "command": ["weather-mcp", "--units", "metric"],
        "variables": [{"name": "WEATHER_KEY", "value": "k"}, {"name": "LANG"}]});
    let shown = front.call("connectors.preview", form.clone());
    let fingerprint = shown["fingerprint"].clone();
    front.call(
        "connectors.connect",
        with(form, "fingerprint", fingerprint.clone()),
    );
    let off = front.call("connectors.disconnect", json!({"alias": "weather"}));
    let listed = named(&off, "weather").expect("declared").clone();

    // What the window's formOf sends: stored values kept, others named, in the order listed.
    let variables: Vec<Value> = listed["variables"]
        .as_array()
        .expect("variables")
        .iter()
        .map(|variable| match variable["stored"].as_bool() {
            Some(true) => json!({"name": variable["name"], "keep": true}),
            _ => json!({"name": variable["name"]}),
        })
        .collect();
    let again = json!({"alias": "weather", "transport": "stdio",
        "command": listed["command"], "variables": variables});
    let shown = front.call("connectors.preview", again.clone());
    assert_eq!(shown["same"], true, "{shown}");
    assert_eq!(shown["fingerprint"], fingerprint);
    let on = front.call(
        "connectors.connect",
        with(
            with(again, "fingerprint", fingerprint),
            "replace",
            json!(true),
        ),
    );
    assert_eq!(named(&on, "weather").unwrap()["connected"], true);
}
