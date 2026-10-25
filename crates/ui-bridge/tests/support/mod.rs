//! What the tests of this crate that drive a program over lines of JSON share: the messages it
//! writes, kept for the test to wait on and as written, and the local host with the client it
//! starts.
//!
//! The client the host starts is the including test binary run again, which copies the channel the
//! host made for it to a socket the test listens on, so the test can speak to the host as its client.
//! That socket belongs to the test and the host never sees it. Each including file has a test named
//! [`RELAY_TEST`] that calls [`relay`].
#![allow(
    dead_code,
    reason = "each test crate that includes this file uses some of it"
)]

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// How long a test waits for what it expects before it fails.
pub const PATIENCE: Duration = Duration::from_secs(120);

/// The variable that names the socket the relay connects to.
const RELAY: &str = "BRAVEBOT_UI_BRIDGE_TEST_RELAY";

/// The name of the test the host starts as its client.
pub const RELAY_TEST: &str = "relay_entry";

/// The client the host starts. Does nothing unless it was started as one, so running every ignored
/// test of a binary starts nothing.
pub fn relay() {
    let Ok(path) = std::env::var(RELAY) else {
        return;
    };
    let socket = UnixStream::connect(path).expect("the test is listening");
    let mut up = socket.try_clone().unwrap();
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin(), &mut up);
        let _ = up.shutdown(std::net::Shutdown::Write);
    });
    let mut down = socket;
    let mut out = std::io::stdout();
    let mut buffer = [0u8; 8192];
    while let Ok(read) = down.read(&mut buffer) {
        if read == 0 || out.write_all(&buffer[..read]).is_err() || out.flush().is_err() {
            break;
        }
    }
    std::process::exit(0);
}

/// The lines a program writes, as they are written: parsed for the test to wait on, and kept as
/// written for the test to hold.
pub struct Inbox {
    pub said: mpsc::Receiver<Value>,
    /// What arrived while the test waited for something else.
    pub held: VecDeque<Value>,
    written: Arc<Mutex<Vec<String>>>,
}

impl Inbox {
    pub fn read(source: impl Read + Send + 'static) -> Self {
        let (sender, said) = mpsc::channel();
        let written = Arc::new(Mutex::new(Vec::new()));
        std::thread::spawn({
            let written = Arc::clone(&written);
            move || {
                for line in BufReader::new(source).lines().map_while(Result::ok) {
                    written.lock().unwrap().push(line.clone());
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && sender.send(value).is_err()
                    {
                        break;
                    }
                }
            }
        });
        Self {
            said,
            held: VecDeque::new(),
            written,
        }
    }

    /// The next message `wanted` accepts within `limit`, whether it arrived before or after the
    /// call, keeping the others for a later wait.
    pub fn within(&mut self, limit: Duration, wanted: impl Fn(&Value) -> bool) -> Option<Value> {
        if let Some(at) = self.held.iter().position(&wanted) {
            return self.held.remove(at);
        }
        let deadline = Instant::now() + limit;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return Some(message),
                Ok(message) => self.held.push_back(message),
                Err(_) => break,
            }
        }
        None
    }

    /// As [`Inbox::within`], for [`PATIENCE`], failing where nothing `wanted` accepts arrived. `who`
    /// is the program that was waited on.
    pub fn until(&mut self, who: &str, wanted: impl Fn(&Value) -> bool) -> Value {
        self.within(PATIENCE, wanted).unwrap_or_else(|| {
            panic!(
                "{who} never said what the test was waiting for: {:?}",
                self.held
            )
        })
    }

    /// Every line written so far, as written.
    pub fn written(&self) -> Vec<String> {
        self.written.lock().unwrap().clone()
    }
}

/// How to start the local host.
pub struct Start<'a> {
    pub home: &'a Path,
    pub sessions_root: &'a Path,
    pub workspace: &'a Path,
    /// Where the test listens for the client the host starts. The path has to fit in the address of a
    /// socket.
    pub relay: &'a Path,
    /// The settings file the host is started on, where there is one.
    pub settings: Option<&'a Path>,
    /// The directory the host is started in, where it is not the test's.
    pub current_dir: Option<&'a Path>,
    /// What the host is told of its model service and of anything else it reads from the environment,
    /// on top of a home, a locale and the keys of the gateway, which are the same for every test.
    pub environment: &'a [(&'a str, &'a OsStr)],
    pub stdout: Stdio,
    pub stderr: Stdio,
}

/// A local host, ended and waited for when it is dropped.
pub struct Host {
    pub child: Child,
    pub commands: Option<ChildStdin>,
    relay: UnixListener,
    relay_path: PathBuf,
}

impl Host {
    pub fn launch(start: Start<'_>) -> Self {
        let _ = std::fs::remove_file(start.relay);
        let relay = UnixListener::bind(start.relay).expect("a relay socket");
        relay.set_nonblocking(true).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-host"));
        if let Some(directory) = start.current_dir {
            command.current_dir(directory);
        }
        command
            .env_clear()
            .env("HOME", start.home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env(RELAY, start.relay);
        for (name, value) in start.environment {
            command.env(name, value);
        }
        command
            .arg("--sessions-root")
            .arg(start.sessions_root)
            .arg("--workspace")
            .arg(start.workspace);
        if let Some(settings) = start.settings {
            command.arg("--settings").arg(settings);
        }
        let mut child = command
            .arg("--")
            // The program started is this test binary, the one that is already running.
            // nosemgrep: rust.lang.security.current-exe.current-exe
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", RELAY_TEST, "--nocapture"])
            .stdin(Stdio::piped())
            .stdout(start.stdout)
            .stderr(start.stderr)
            .spawn()
            .expect("the host runs");
        let commands = child.stdin.take();
        Self {
            child,
            commands,
            relay,
            relay_path: start.relay.to_path_buf(),
        }
    }

    /// The client the host started, once it has connected to the relay.
    pub fn client(&self) -> Client {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self.relay.accept() {
                Ok((stream, _)) => return Client::over(stream),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("the host never started its client: {error}"),
            }
        }
    }

    pub fn command(&mut self, line: &str) {
        let commands = self.commands.as_mut().expect("the host's input is open");
        writeln!(commands, "{line}").unwrap();
        commands.flush().unwrap();
    }

    pub fn end_input(&mut self) {
        self.commands = None;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.relay_path);
    }
}

/// A client of the local host: lines of JSON over the channel the host made for it.
pub struct Client {
    pub stream: UnixStream,
    pub inbox: Inbox,
    next: u64,
}

impl Client {
    pub fn over(stream: UnixStream) -> Self {
        stream.set_nonblocking(false).unwrap();
        let inbox = Inbox::read(stream.try_clone().unwrap());
        Self {
            stream,
            inbox,
            next: 0,
        }
    }

    pub fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let line = json!({"id": self.next, "method": method, "params": params});
        writeln!(self.stream, "{line}").unwrap();
        self.next
    }

    /// A request, and the whole of the answer to it.
    pub fn answered(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.until(|message| message["id"] == id)
    }

    /// A request, and what the host answered it with, which is a failure of the test where the host
    /// answered with an error.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    pub fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        self.inbox.until("the host", wanted)
    }

    /// The next message `wanted` accepts within `limit`, keeping the others for a later wait.
    pub fn within(&mut self, limit: Duration, wanted: impl Fn(&Value) -> bool) -> Option<Value> {
        self.inbox.within(limit, wanted)
    }

    pub fn event(&mut self, name: &str) -> Value {
        self.until(|message| message["event"] == name)
    }

    /// A session opened in the project and trusted, ready for a turn.
    pub fn session_in(&mut self, project: &Path) -> String {
        let opened = self.call("session.new", json!({"directory": project}));
        let session = opened["session"].as_str().expect("a handle").to_string();
        // A yes an earlier session here gave is kept, and settles the next one without asking.
        if opened["remembered"].is_null() {
            self.call("trust.reply", json!({"session": session, "trusted": true}));
        }
        session
    }

    pub fn leave(self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    pub fn the_session(&mut self, session: &str) -> Value {
        let info = self.call("instance.info", json!({}));
        info["sessions"]
            .as_array()
            .and_then(|all| all.iter().find(|one| one["session"] == session))
            .cloned()
            .unwrap_or_else(|| panic!("the host does not list {session}: {info}"))
    }
}
