//! What the tests of this crate that drive a program over lines of JSON share: the messages it
//! writes, kept for the test to wait on and as written, and the local host with the client it
//! starts.
//!
//! The client the host starts is the including test binary run again, which copies the channel the
//! host made for it to a socket the test listens on, so the test can speak to the host as its client.
//! That socket belongs to the test and the host never sees it. Each including file has a test named
//! [`RELAY_TEST`] that calls [`relay`]. The relay first writes its client's name and process id on
//! that socket, so a test with several clients connected can tell them apart.
#![allow(
    dead_code,
    reason = "each test crate that includes this file uses some of it"
)]

use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a test waits for what it expects before it fails.
pub const PATIENCE: Duration = Duration::from_secs(120);

/// The words that open the message naming the session's own directory (INSTR-9). It follows the
/// prompt in the conversation, so a stand-in model that reads "the last user message" as the prompt
/// has to step over it.
pub const SCRATCH_NOTE: &str = "The scratch directory of this session is";

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
    let name = std::env::args()
        .skip_while(|argument| argument != RELAY_TEST)
        .nth(1)
        .unwrap_or_default();
    let mut socket = UnixStream::connect(path).expect("the test is listening");
    writeln!(socket, "{name} {}", std::process::id()).expect("the test reads");
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
    /// Where what the host writes on its standard error is copied, besides the lines a test reads
    /// from [`Host::said`].
    pub stderr: Box<dyn Write + Send>,
    /// The clients the host is given, each a name and its answers, and each this test binary.
    pub clients: &'a [(&'a str, &'a str)],
}

/// One client that may give every answer the host offers.
pub const ONE_CLIENT: &[(&str, &str)] = &[("test", "confirm,run,fetch,ask,trust")];

/// Nowhere, for a host whose standard error no test keeps beyond [`Host::said`].
pub fn discarded() -> Box<dyn Write + Send> {
    Box::new(std::io::sink())
}

/// What ends the question the host puts about a workspace at startup, which the host writes with no
/// line break after it.
pub const STARTUP_QUESTION: &str = "trust this directory? [y/N] ";

/// What the host says about a workspace it puts no question about at startup.
pub const NOT_ASKED: [&str; 2] = ["start trusted, because", "cannot be kept"];

/// A local host, ended and waited for when it is dropped.
pub struct Host {
    pub child: Child,
    pub commands: Option<ChildStdin>,
    /// The lines the host writes on its standard error, as they arrive, and the question it puts at
    /// startup as a line of its own.
    pub said: mpsc::Receiver<String>,
    relay: UnixListener,
    relay_path: PathBuf,
    /// The thread that copies the host's standard error, and how many bytes it has copied.
    copying: Option<(JoinHandle<()>, Arc<AtomicUsize>)>,
    /// Clients that connected while the test waited for another.
    arrived: RefCell<VecDeque<Client>>,
}

impl Host {
    /// A host whose workspace has been asked about and answered no, so its sessions ask their client
    /// about trust; or one whose workspace was not asked about, where it says it was not.
    pub fn launch(start: Start<'_>) -> Self {
        let mut host = Self::launch_unanswered(start);
        host.decline_at_startup();
        host
    }

    /// Wait for the host to ask about its workspace or to say it does not, and answer no to the
    /// question.
    pub fn decline_at_startup(&mut self) {
        let said = self.says_any(&[STARTUP_QUESTION, NOT_ASKED[0], NOT_ASKED[1]]);
        if said.ends_with(STARTUP_QUESTION) {
            self.command("n");
        }
    }

    /// The next line the host says that holds one of `wanted`, failing where none arrives.
    pub fn says_any(&self, wanted: &[&str]) -> String {
        let deadline = Instant::now() + PATIENCE;
        let mut heard = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(line) if wanted.iter().any(|wanted| line.contains(wanted)) => return line,
                Ok(line) => heard.push(line),
                Err(_) => break,
            }
        }
        panic!("the host never said any of {wanted:?}; it said {heard:?}")
    }

    /// The next line the host says that holds `wanted`.
    pub fn says(&self, wanted: &str) -> String {
        self.says_any(&[wanted])
    }

    /// A host that is left at its startup question, if it puts one.
    pub fn launch_unanswered(start: Start<'_>) -> Self {
        Self::launch_in(start, &[])
    }

    /// As [`Host::launch_unanswered`], with `others` as further workspaces after the one `start`
    /// names.
    pub fn launch_in(start: Start<'_>, others: &[&Path]) -> Self {
        Self::launch_with(start, others, &[])
    }

    /// As [`Host::launch_in`], with `extra` as further arguments of the host, before its clients.
    pub fn launch_with(start: Start<'_>, others: &[&Path], extra: &[&OsStr]) -> Self {
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
        for other in others {
            command.arg("--workspace").arg(other);
        }
        if let Some(settings) = start.settings {
            command.arg("--settings").arg(settings);
        }
        command.args(extra);
        for (name, answers) in start.clients {
            command
                .args(["--client", name, "--answers", answers, "--program"])
                // The program started is this test binary, the one that is already running.
                // nosemgrep: rust.lang.security.current-exe.current-exe
                .arg(std::env::current_exe().unwrap());
            // The name after the test's is a second filter, which no test matches.
            for argument in ["--ignored", "--exact", RELAY_TEST, name, "--nocapture"] {
                command.args(["--arg", argument]);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(start.stdout)
            .stderr(Stdio::piped())
            .spawn()
            .expect("the host runs");
        let commands = child.stdin.take();
        let (said, copying) = lines_of(
            child.stderr.take().expect("the host's error output"),
            start.stderr,
        );
        Self {
            child,
            commands,
            said,
            relay,
            relay_path: start.relay.to_path_buf(),
            copying: Some(copying),
            arrived: RefCell::default(),
        }
    }

    /// The client the host started, once it has connected to the relay.
    pub fn client(&self) -> Client {
        self.client_within(PATIENCE)
            .expect("the host never started its client")
    }

    /// The client the host starts within `limit`, if it starts one.
    pub fn client_within(&self, limit: Duration) -> Option<Client> {
        if let Some(client) = self.arrived.borrow_mut().pop_front() {
            return Some(client);
        }
        self.next_within(limit)
    }

    /// The client named `name`, once it has connected, keeping any other that connects first.
    pub fn client_named(&self, name: &str) -> Client {
        let held = self
            .arrived
            .borrow()
            .iter()
            .position(|client| client.name == name);
        if let Some(at) = held {
            return self.arrived.borrow_mut().remove(at).unwrap();
        }
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.next_within(left) {
                Some(client) if client.name == name => return client,
                Some(client) => self.arrived.borrow_mut().push_back(client),
                None => break,
            }
        }
        panic!("the host never started its client {name}")
    }

    /// The next client to connect within `limit`.
    fn next_within(&self, limit: Duration) -> Option<Client> {
        let deadline = Instant::now() + limit;
        loop {
            match self.relay.accept() {
                Ok((stream, _)) => return Some(Client::named(stream)),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return None,
            }
        }
    }

    /// The client that `line`, a `launch`, starts. The host refuses a launch of a client until it
    /// has dealt with the loss of that client's last connection, so the line is typed again until
    /// a client connects. Once one is waiting to start, typing it again is refused, so one client
    /// starts.
    pub fn launched(&mut self, line: &str) -> Client {
        let deadline = Instant::now() + PATIENCE;
        loop {
            self.command(line);
            if let Some(client) = self.client_within(Duration::from_millis(200)) {
                return client;
            }
            assert!(Instant::now() < deadline, "`{line}` never started a client");
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

/// What `source` writes, a line at a time, copied to `copy` as it arrives. A question that ends
/// with [`STARTUP_QUESTION`] is a line of its own, since the host waits for the answer on the same
/// line. Read to the end whether or not the lines are wanted, so the host is never left writing to
/// a full pipe.
fn lines_of(
    mut source: impl Read + Send + 'static,
    mut copy: Box<dyn Write + Send>,
) -> (mpsc::Receiver<String>, (JoinHandle<()>, Arc<AtomicUsize>)) {
    let (said, lines) = mpsc::channel();
    let copied = Arc::new(AtomicUsize::new(0));
    let counting = Arc::clone(&copied);
    let copying = std::thread::spawn(move || {
        let mut pending = Vec::new();
        let mut buffer = [0u8; 8192];
        while let Ok(read) = source.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let _ = copy.write_all(&buffer[..read]);
            let _ = copy.flush();
            counting.fetch_add(read, Ordering::SeqCst);
            pending.extend_from_slice(&buffer[..read]);
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                let _ = said.send(String::from_utf8_lossy(&line[..end]).into_owned());
            }
            if pending.ends_with(STARTUP_QUESTION.as_bytes()) {
                let _ = said.send(String::from_utf8_lossy(&pending).into_owned());
                pending.clear();
            }
        }
    });
    (lines, (copying, copied))
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.relay_path);
        // A test that reads the copy after the host has gone reads all of it. Everything the host
        // wrote is in the pipe once it has exited, but a program it started can hold the pipe open
        // for longer, so the copy is done when its end is read or when nothing more arrives.
        if let Some((copying, copied)) = self.copying.take() {
            let deadline = Instant::now() + PATIENCE;
            let mut before = usize::MAX;
            while !copying.is_finished() && Instant::now() < deadline {
                let now = copied.load(Ordering::SeqCst);
                if now == before {
                    break;
                }
                before = now;
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// A client of the local host: lines of JSON over the channel the host made for it.
pub struct Client {
    pub stream: UnixStream,
    pub inbox: Inbox,
    /// The name the host's command line gave the client, and the relay's process id.
    pub name: String,
    pub pid: u32,
    next: u64,
}

impl Client {
    /// A client over a relay's socket, which says first which client it is.
    fn named(mut stream: UnixStream) -> Self {
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        // A byte at a time, so nothing after the first line is read here.
        let mut first = Vec::new();
        let mut byte = [0u8; 1];
        while stream.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {
            first.push(byte[0]);
        }
        stream.set_read_timeout(None).unwrap();
        let first = String::from_utf8_lossy(&first).into_owned();
        let (name, pid) = first.rsplit_once(' ').unwrap_or_default();
        let mut client = Self::over(stream);
        client.name = name.to_string();
        client.pid = pid.parse().unwrap_or_default();
        client
    }

    pub fn over(stream: UnixStream) -> Self {
        stream.set_nonblocking(false).unwrap();
        let inbox = Inbox::read(stream.try_clone().unwrap());
        Self {
            stream,
            inbox,
            name: String::new(),
            pid: 0,
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
