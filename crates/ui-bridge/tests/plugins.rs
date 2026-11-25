//! Adding and updating a plugin from the desktop window (docs/specs/plugins.md PLUGIN-3, PLUGIN-4,
//! PLUGIN-5, PLUGIN-9).
//!
//! Driven through the binary with a home of the test's own, because what is installed is the
//! person's own and a test in-process would be writing the developer's. Each question comes as a
//! `plugins.question` event and is answered with `plugins.reply`; each job ends with
//! `plugins.ended`.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "a request's params are built in the call that sends them"
)]
#![allow(
    clippy::let_underscore_must_use,
    reason = "a scratch directory or a child that is already gone needs no cleaning"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

    fn state(&self) -> PathBuf {
        self.home().join(".bravebot")
    }

    fn plugins(&self) -> PathBuf {
        self.state().join("plugins")
    }

    /// A plugin source named `name` at `version`, outside the state directory.
    fn source(&self, name: &str, version: &str) -> PathBuf {
        let root = self.path.join(format!("src-{name}"));
        std::fs::create_dir_all(root.join("skills").join("hello")).unwrap();
        std::fs::write(
            root.join("plugin.toml"),
            format!(
                "name = \"{name}\"\nversion = \"{version}\"\ndescription = \"d\"\nskills = \"skills\"\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("skills").join("hello").join("SKILL.md"),
            "---\nname: hello\ndescription: Says hello.\n---\n\nBody.\n",
        )
        .unwrap();
        root
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
    /// Events read while waiting for an answer, kept for [`FrontEnd::event`].
    events: Vec<Value>,
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
            events: Vec::new(),
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
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if message["id"] == id => return message,
                Ok(message) if message.get("event").is_some() => self.events.push(message),
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

    /// The data of the next `plugins.question` or `plugins.ended`, whichever comes first.
    fn next_event(&mut self) -> Value {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(at) = self
                .events
                .iter()
                .position(|event| event["event"].as_str().is_some_and(is_plugin_event))
            {
                return self.events.remove(at);
            }
            let left = deadline
                .checked_duration_since(Instant::now())
                .expect("a plugin event came");
            match self.said.recv_timeout(left) {
                Ok(message) if message.get("event").is_some() => self.events.push(message),
                Ok(_) => {}
                Err(_) => panic!("no plugin event came"),
            }
        }
    }

    fn question(&mut self, kind: &str) -> Value {
        let event = self.next_event();
        assert_eq!(event["event"], "plugins.question", "{event}");
        assert_eq!(event["data"]["kind"], kind, "{event}");
        event["data"].clone()
    }

    fn ended(&mut self) -> Value {
        let event = self.next_event();
        assert_eq!(event["event"], "plugins.ended", "{event}");
        event["data"].clone()
    }

    fn reply(&mut self, question: &Value, yes: bool) {
        self.call(
            "plugins.reply",
            json!({"job": question["job"], "question": question["question"], "yes": yes}),
        );
    }

    fn add(&mut self, source: &str) -> Value {
        self.call("plugins.add", json!({"source": source}))["job"].clone()
    }
}

fn is_plugin_event(name: &str) -> bool {
    name == "plugins.question" || name == "plugins.ended"
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

/// PLUGIN-4, PLUGIN-5: a local add puts one install question naming the plugin. A no writes
/// nothing, a yes installs the directory and its record, and the same source again is refused
/// pointing at `/plugin update`.
///
/// Rejects: an install without a question, a no that writes, a yes that installs nothing, and an
/// add of an installed source that replaces it.
#[test]
fn a_local_add_asks_once_and_only_a_yes_installs() {
    let scratch = Scratch::new("bridge-plugins-local");
    let source = scratch.source("demo", "2.3.4");
    let typed = source.display().to_string();
    let mut front = FrontEnd::start(&scratch.home());

    let job = front.add(&typed);
    let question = front.question("install");
    assert_eq!(question["job"], job);
    assert_eq!(question["listing"]["untrusted"]["name"], "demo");
    assert_eq!(question["listing"]["untrusted"]["version"], "2.3.4");
    front.reply(&question, false);
    let ended = front.ended();
    assert_eq!(ended["job"], job);
    assert_eq!(ended["outcome"], "declined", "{ended}");
    assert_eq!(ended["plugins"], json!([]));
    assert!(!scratch.plugins().exists(), "a no wrote something");

    front.add(&typed);
    let question = front.question("install");
    front.reply(&question, true);
    let ended = front.ended();
    assert_eq!(ended["outcome"], "installed", "{ended}");
    assert_eq!(ended["sentence"], format!("Installed demo from {typed}."));
    assert_eq!(ended["plugins"][0]["name"], "demo");
    assert_eq!(ended["plugins"][0]["enabled"], true);
    assert!(scratch.plugins().join("demo").is_dir());
    assert!(scratch.plugins().join("demo.record").is_file());

    // The shared install checks the source against the record when it writes, after the question.
    front.add(&typed);
    let question = front.question("install");
    front.reply(&question, true);
    let ended = front.ended();
    assert_eq!(ended["outcome"], "refused", "{ended}");
    let sentence = ended["sentence"].as_str().unwrap();
    assert!(sentence.contains("/plugin update demo"), "{sentence}");
}

/// PLUGIN-9: an update of an unchanged source asks nothing. Once the source changes, one update
/// question is put; a no keeps the record byte for byte and a yes takes the new version, whose tree
/// matches its record.
///
/// Rejects: a question put for an unchanged source, a no that replaces, and a yes that does not.
#[test]
fn an_update_asks_only_when_the_source_changed_and_only_a_yes_replaces() {
    let scratch = Scratch::new("bridge-plugins-update");
    let source = scratch.source("demo", "2.3.4");
    let mut front = FrontEnd::start(&scratch.home());
    front.add(&source.display().to_string());
    let question = front.question("install");
    front.reply(&question, true);
    assert_eq!(front.ended()["outcome"], "installed");

    front.call("plugins.update", json!({"name": "demo"}));
    let ended = front.ended();
    assert_eq!(ended["outcome"], "unchanged", "{ended}");
    assert!(ended["sentence"].as_str().unwrap().contains("up to date"));

    scratch.source("demo", "2.4.0");
    let record = scratch.plugins().join("demo.record");
    let kept = std::fs::read(&record).unwrap();
    front.call("plugins.update", json!({"name": "demo"}));
    let question = front.question("update");
    front.reply(&question, false);
    let ended = front.ended();
    assert_eq!(ended["outcome"], "declined", "{ended}");
    assert_eq!(
        std::fs::read(&record).unwrap(),
        kept,
        "a no changed the record"
    );

    front.call("plugins.update", json!({"name": "demo"}));
    let question = front.question("update");
    front.reply(&question, true);
    let ended = front.ended();
    assert_eq!(ended["outcome"], "updated", "{ended}");
    assert_eq!(ended["plugins"][0]["version"], "2.4.0");
    assert_eq!(
        bravebot_agent::plugins::tree_matches_record(&scratch.state(), "demo"),
        Ok(())
    );
}

/// Counts the connections made to a host on this machine.
struct Host {
    port: u16,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Host {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (counted, stopped) = (Arc::clone(&count), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                if listener.accept().is_ok() {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        Self { port, count, stop }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// PLUGIN-3: a git URL puts the host question first, naming the host, and a no makes no
/// connection to it and writes nothing.
///
/// Rejects: a fetch started before the host is asked about, and a no that fetches.
#[test]
fn a_git_url_asks_about_the_host_first_and_a_no_fetches_nothing() {
    let scratch = Scratch::new("bridge-plugins-git");
    let host = Host::start();
    let mut front = FrontEnd::start(&scratch.home());

    let url = format!("https://127.0.0.1:{}/x.git", host.port);
    front.add(&url);
    let question = front.question("host");
    assert_eq!(question["listing"]["host"], "127.0.0.1");
    assert_eq!(question["listing"]["url"], url);
    assert_eq!(
        host.count.load(Ordering::SeqCst),
        0,
        "fetched before asking"
    );
    front.reply(&question, false);
    let ended = front.ended();
    assert_eq!(ended["outcome"], "declined", "{ended}");
    // A connection made before `plugins.ended` is counted only when the accept loop next looks,
    // every 20 ms, and nothing signals that it has looked, so this waits several of its turns.
    // The connection below then shows that the counter counts.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(host.count.load(Ordering::SeqCst), 0, "a no fetched");
    assert!(!scratch.plugins().exists());

    let _probe = std::net::TcpStream::connect(("127.0.0.1", host.port)).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while host.count.load(Ordering::SeqCst) == 0 {
        assert!(
            std::time::Instant::now() < until,
            "the host never counted a connection"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// PLUGIN-3: an archive URL is not a source, so it is refused with no question put.
#[test]
fn an_archive_url_is_refused_without_a_question() {
    let scratch = Scratch::new("bridge-plugins-archive");
    let mut front = FrontEnd::start(&scratch.home());

    front.add("https://example.com/plugin.tar.gz");
    let ended = front.ended();
    assert_eq!(ended["outcome"], "refused", "{ended}");
    assert!(
        ended["sentence"]
            .as_str()
            .unwrap()
            .starts_with("Nothing was installed"),
        "{ended}"
    );
    assert!(!scratch.plugins().exists());
}

/// A cancel is a no, and a window that closes with a question open installs nothing: nobody is
/// left to answer it.
///
/// Rejects: a cancel that leaves the question waiting or installs, and a closed window whose open
/// question reads as yes.
#[test]
fn a_cancel_or_a_closed_window_installs_nothing() {
    let scratch = Scratch::new("bridge-plugins-gone");
    let source = scratch.source("demo", "2.3.4");
    let typed = source.display().to_string();
    let mut front = FrontEnd::start(&scratch.home());

    let job = front.add(&typed);
    front.question("install");
    front.call("plugins.cancel", json!({"job": job}));
    let ended = front.ended();
    assert_eq!(ended["outcome"], "declined", "{ended}");
    assert!(!scratch.plugins().exists());

    front.add(&typed);
    let question = front.question("install");
    drop(front.child.stdin.take());
    let deadline = Instant::now() + PATIENCE;
    while front.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the bridge did not end");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(question["question"], 1);
    assert!(!scratch.plugins().exists(), "a closed window installed");
}

/// A relative path has no workspace to be read from in the window, so it is refused at once.
#[test]
fn a_relative_path_is_refused() {
    let scratch = Scratch::new("bridge-plugins-relative");
    let mut front = FrontEnd::start(&scratch.home());
    let answered = front.answered("plugins.add", json!({"source": "./demo"}));
    let said = answered["error"]["message"].as_str().unwrap_or_default();
    assert!(said.contains("full path"), "{answered}");
}
