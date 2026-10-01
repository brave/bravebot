//! The relay run as the processes it is: the native host started the way Brave starts it, the MCP
//! server started the way BraveBot starts it, and a fake extension on the host's stdin and stdout
//! speaking native messaging.

#![cfg(unix)]
#![forbid(unsafe_code)]

use bravebot_browser::framing;
use bravebot_browser::host::{SECRET_TIMEOUT, WAITING_LIMIT};
use bravebot_browser::relay::REPLY_TIMEOUT;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const PROGRAM: &str = env!("CARGO_BIN_EXE_bravebot-browser");

/// The extension the relay is installed for, and one it is not.
const OURS: &str = "abcdefghijklmnopabcdefghijklmnop";
const OTHER: &str = "ponmlkjihgfedcbaponmlkjihgfedcba";

/// Long enough for a host that is working, short enough that one that is not fails the test.
const WAIT: Duration = Duration::from_secs(10);

fn origin(id: &str) -> String {
    format!("chrome-extension://{id}/")
}

/// A directory for the relay, with `id` recorded as the one extension it serves.
fn installed_for(id: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("extension"), id).unwrap();
    directory
}

/// Waits until `condition` holds, or fails the test once [`WAIT`] has passed.
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// How `child` exited, or a failed test once [`WAIT`] has passed with it still running. A child
/// that should have exited and did not is the regression, so it fails the test rather than
/// hanging it.
fn exit_status(child: &mut Child, what: &str) -> ExitStatus {
    let mut status = None;
    wait_until(what, || {
        status = child.try_wait().unwrap();
        status.is_some()
    });
    status.unwrap()
}

/// The native host as Brave starts it, with a fake extension on its stdin and stdout.
struct Extension {
    host: Child,
    to_host: Option<ChildStdin>,
    from_host: Receiver<Value>,
}

impl Extension {
    /// Starts the host for the extension with origin `id`, and waits for its socket.
    fn connect(directory: &Path, id: &str) -> Self {
        let mut extension = Self::start(directory, id);
        let socket = directory.join("socket");
        wait_until("the host listens", || {
            if let Ok(Some(status)) = extension.host.try_wait() {
                panic!("the host exited with {status} before listening");
            }
            socket.exists()
        });
        extension
    }

    fn start(directory: &Path, id: &str) -> Self {
        let mut host = Command::new(PROGRAM)
            .arg(origin(id))
            .env("BRAVEBOT_BROWSER_DIR", directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let to_host = host.stdin.take();
        let mut stdout = host.stdout.take().unwrap();
        let (sender, from_host) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(message)) = framing::read_message(&mut stdout) {
                if sender
                    .send(serde_json::from_slice(&message).unwrap())
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            host,
            to_host,
            from_host,
        }
    }

    /// The next request the host sent the extension.
    fn request(&self) -> Value {
        self.from_host
            .recv_timeout(WAIT)
            .expect("the extension received a request")
    }

    fn reply(&mut self, message: Value) {
        let to_host = self.to_host.as_mut().unwrap();
        framing::write_message(to_host, &serde_json::to_vec(&message).unwrap()).unwrap();
    }

    /// A reply larger than the host may send the extension, which the extension may send the host.
    fn reply_large(&mut self, message: Value) {
        let bytes = serde_json::to_vec(&message).unwrap();
        let to_host = self.to_host.as_mut().unwrap();
        to_host
            .write_all(&u32::try_from(bytes.len()).unwrap().to_ne_bytes())
            .unwrap();
        to_host.write_all(&bytes).unwrap();
        to_host.flush().unwrap();
    }

    /// Closes the extension's port, as Brave does when the extension disconnects, and waits for the
    /// host to exit.
    fn disconnect(&mut self) {
        drop(self.to_host.take());
        exit_status(&mut self.host, "the host exits");
    }
}

impl Drop for Extension {
    fn drop(&mut self) {
        let _ = self.host.kill();
        let _ = self.host.wait();
    }
}

/// A peer on the host's socket, as the MCP server is one.
struct Peer {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Peer {
    fn connect(directory: &Path, secret: &str) -> Self {
        let mut writer = UnixStream::connect(directory.join("socket")).unwrap();
        writer.set_read_timeout(Some(WAIT)).unwrap();
        writeln!(writer, "{secret}").unwrap();
        let reader = BufReader::new(writer.try_clone().unwrap());
        Self { reader, writer }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.writer, "{message}").unwrap();
    }

    /// The next line from the host, or `None` where it closed the connection. A close with the
    /// peer's lines still unread arrives as a reset rather than an end, and is the same close.
    fn receive(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => None,
            Err(error) => panic!("reading from the host: {error}"),
        }
    }
}

fn secret(directory: &Path) -> String {
    std::fs::read_to_string(directory.join("secret")).unwrap()
}

/// A connection that sends nothing, as one made without the secret would.
fn idle(directory: &Path) -> UnixStream {
    UnixStream::connect(directory.join("socket")).unwrap()
}

/// Whether the host has closed `stream`, waiting at most `within` to find out.
fn closed_within(stream: &mut UnixStream, within: Duration) -> bool {
    stream.set_read_timeout(Some(within)).unwrap();
    let mut byte = [0u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => true,
        Ok(_) => panic!("the host wrote to a connection that sent nothing"),
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => true,
        Err(_) => false,
    }
}

/// The MCP server as BraveBot starts it, in `directory`, answering each line of `requests`.
///
/// Longer than the server's own reply timeout, so a call it gives up on is still a reply here, and
/// bounded, so a server that never exits fails the test.
fn mcp(directory: &Path, requests: &[Value]) -> Vec<Value> {
    let mut server = Command::new(PROGRAM)
        .arg("mcp")
        .current_dir(directory)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = server.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let mut stdout = server.stdout.take().unwrap();
    let output = std::thread::spawn(move || {
        let mut output = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut output).map(|_| output)
    });
    let deadline = Instant::now() + REPLY_TIMEOUT + WAIT;
    let status = loop {
        if let Some(status) = server.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = server.kill();
            let _ = server.wait();
            panic!("the MCP server did not exit");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    output
        .join()
        .unwrap()
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn call(id: u64, tool: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": tool, "arguments": arguments}})
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap()
}

/// The MCP server, started with no path at all, reaches the host through the socket in the
/// directory it runs in and gets the extension's answer. The two processes meet only there.
#[test]
fn a_tool_call_reaches_the_extension_through_the_socket_the_host_listens_on() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);

    let answering = std::thread::scope(|scope| {
        let replies = scope.spawn(|| mcp(directory.path(), &[call(7, "list_tabs", json!({}))]));
        let request = extension.request();
        assert_eq!(request["method"], "list_tabs");
        extension.reply(json!({"id": request["id"], "result": [{"id": 1, "title": "Brave"}]}));
        replies.join().unwrap()
    });

    assert_eq!(answering.len(), 1);
    assert_eq!(answering[0]["id"], 7);
    assert_eq!(answering[0]["result"]["isError"], false);
    let tabs: Value = serde_json::from_str(text(&answering[0])).unwrap();
    assert_eq!(tabs, json!([{"id": 1, "title": "Brave"}]));
}

/// A confined server cannot create a socket on macOS, so the MCP side never tries: with no host it
/// reports the extension missing and leaves the directory as it found it.
#[test]
fn the_mcp_server_never_creates_the_socket() {
    let directory = tempfile::tempdir().unwrap();
    let replies = mcp(directory.path(), &[call(1, "list_tabs", json!({}))]);
    assert_eq!(replies[0]["result"]["isError"], true);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

/// The socket and the secret are in the one directory, readable by this account alone, so the
/// declaration's `--dir` grants the MCP server both and nothing else grants them to anyone.
#[test]
fn the_socket_and_secret_are_in_a_directory_only_this_account_can_reach() {
    let directory = installed_for(OURS);
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let _extension = Extension::connect(directory.path(), OURS);

    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(directory.path()), 0o700);
    assert_eq!(mode(&directory.path().join("secret")), 0o600);
    assert!(directory.path().join("socket").exists());
}

/// A connection that never presents the secret is closed once the host has waited long enough for
/// it, so one left open costs the host nothing for long, and the host goes on serving.
#[test]
fn a_connection_that_does_not_present_the_secret_in_time_is_closed() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);

    let mut silent = idle(directory.path());
    let started = Instant::now();
    assert!(
        closed_within(&mut silent, SECRET_TIMEOUT + WAIT),
        "a connection that sent nothing was kept open"
    );
    assert!(started.elapsed() >= SECRET_TIMEOUT / 2);

    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));
    peer.send(&json!({"id": 1, "method": "list_tabs", "params": {}}));
    assert_eq!(extension.request()["method"], "list_tabs");
}

/// The 2 seconds run from the connection's accept, not from the last byte it sent: a connection
/// sending the host one byte at a time and never a line is closed as one sending nothing is.
#[test]
fn a_connection_trickling_bytes_without_the_secret_is_closed_in_time() {
    let directory = installed_for(OURS);
    let _extension = Extension::connect(directory.path(), OURS);

    let mut trickling = idle(directory.path());
    let mut writer = trickling.try_clone().unwrap();
    let started = Instant::now();
    let pacer = std::thread::spawn(move || {
        // Well inside the line's own length, so the length is not what closes it.
        while started.elapsed() < SECRET_TIMEOUT * 4 {
            if writer.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(SECRET_TIMEOUT / 4);
        }
    });
    assert!(
        closed_within(&mut trickling, SECRET_TIMEOUT * 4 + WAIT),
        "a connection trickling bytes was kept open"
    );
    assert!(
        started.elapsed() < SECRET_TIMEOUT + Duration::from_millis(1500),
        "a connection trickling bytes was closed only after {:?}",
        started.elapsed()
    );
    pacer.join().unwrap();
}

/// Connections that have not presented the secret are held to a number. One past it is closed as
/// soon as it is accepted, while the ones before it are still being waited for, so connections
/// nobody authenticates cannot use up the host's threads.
#[test]
fn connections_waiting_for_the_secret_are_held_to_a_number() {
    let directory = installed_for(OURS);
    let _extension = Extension::connect(directory.path(), OURS);

    let mut waiting: Vec<UnixStream> = (0..WAITING_LIMIT).map(|_| idle(directory.path())).collect();
    let mut one_too_many = idle(directory.path());
    assert!(
        closed_within(&mut one_too_many, SECRET_TIMEOUT / 2),
        "a connection past the limit was kept open"
    );
    assert!(
        !closed_within(&mut waiting[0], Duration::from_millis(50)),
        "a connection within the limit was closed before its time"
    );
}

/// Any process with egress can connect to the socket, so a peer without the secret is closed and
/// nothing it sent reaches the extension. The next request the extension sees is the good peer's.
#[test]
fn a_peer_without_the_secret_is_closed_and_nothing_it_sent_is_forwarded() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);

    let mut stranger = Peer::connect(directory.path(), "not the secret");
    // The host may already have closed, which is the behaviour under test, so a failed write is
    // not a failure here.
    let request = json!({"id": 1, "method": "search_history", "params": {"query": "bank"}});
    let _ = writeln!(stranger.writer, "{request}");
    assert_eq!(stranger.receive(), None);

    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));
    peer.send(&json!({"id": 1, "method": "list_tabs", "params": {}}));
    assert_eq!(extension.request()["method"], "list_tabs");
}

/// `allowed_origins` is the browser's check. The host checks the origin again, so a manifest
/// someone edited does not hand the relay to another extension, and it creates nothing first.
#[test]
fn the_host_refuses_an_extension_it_was_not_installed_for() {
    let entries = |path: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };

    let directory = installed_for(OURS);
    let mut other = Extension::start(directory.path(), OTHER);
    let status = exit_status(&mut other.host, "the host refuses the other extension");
    assert!(!status.success());
    assert_eq!(entries(directory.path()), ["extension"]);

    let never_installed = tempfile::tempdir().unwrap();
    let mut any = Extension::start(never_installed.path(), OURS);
    let status = exit_status(&mut any.host, "the host refuses with nothing installed");
    assert!(!status.success());
    assert!(entries(never_installed.path()).is_empty());
}

/// One host serves a directory at a time. A host that cannot take the lock is refused before it
/// makes a socket or a secret, even with no socket there yet, which is how a second host starting
/// alongside the first finds the directory. Once the lock is free a host starts.
#[test]
fn a_host_does_not_start_while_another_holds_the_lock() {
    use rustix::fs::{FlockOperation, flock};

    let directory = installed_for(OURS);
    let lock = std::fs::File::create(directory.path().join("lock")).unwrap();
    flock(&lock, FlockOperation::LockExclusive).unwrap();

    let mut second = Extension::start(directory.path(), OURS);
    let status = exit_status(&mut second.host, "the host is refused the lock");
    assert!(!status.success());
    assert!(!directory.path().join("socket").exists());
    assert!(!directory.path().join("secret").exists());

    drop(lock);
    let first = Extension::connect(directory.path(), OURS);
    let key = secret(directory.path());

    // A second host while the first serves leaves the first one's files as they were.
    let mut second = Extension::start(directory.path(), OURS);
    let status = exit_status(&mut second.host, "the second host is refused the lock");
    assert!(!status.success());
    assert_eq!(secret(directory.path()), key);
    let mut peer = Peer::connect(directory.path(), &key);
    peer.send(&json!({"id": 1, "method": "list_tabs", "params": {}}));
    assert_eq!(first.request()["method"], "list_tabs");
}

/// When the extension's port closes the host exits and takes its socket and secret with it, so a
/// server connecting later is told the extension is gone rather than reaching a stale socket.
#[test]
fn the_socket_and_secret_go_when_the_extension_disconnects() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);
    extension.disconnect();
    assert!(!directory.path().join("socket").exists());
    assert!(!directory.path().join("secret").exists());
}

/// With no host there is nothing to wait for, so the call fails at once with a reason a person can
/// act on, and as a tool error rather than an empty answer.
#[test]
fn a_call_with_no_extension_connected_fails_at_once_and_says_why() {
    let directory = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let replies = mcp(
        directory.path(),
        &[call(1, "read_page", json!({"url": "https://brave.com/"}))],
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(replies[0]["result"]["isError"], true);
    assert!(text(&replies[0]).contains("No Brave extension is connected"));
    assert!(text(&replies[0]).contains("Brave is not running"));
    assert!(text(&replies[0]).contains("extension is not installed"));
}

/// A host killed before cleanup leaves its secret and socket name behind. A call still fails at
/// once with the same useful reasons when nothing accepts on that socket.
#[test]
fn a_call_to_a_stale_socket_fails_at_once_and_says_why() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("secret"), "a secret").unwrap();
    let socket = directory.path().join("socket");
    drop(UnixListener::bind(&socket).unwrap());

    let started = Instant::now();
    let replies = mcp(directory.path(), &[call(1, "list_tabs", json!({}))]);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(replies[0]["result"]["isError"], true);
    assert!(text(&replies[0]).contains("Brave is not running"));
    assert!(text(&replies[0]).contains("extension is not installed"));
}

/// A call the extension never answers because it disconnected is a failure saying so, not a
/// result, and not a wait for the reply timeout.
#[test]
fn a_call_the_extension_disconnects_during_fails_and_says_so() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);

    let replies = std::thread::scope(|scope| {
        let replies = scope.spawn(|| mcp(directory.path(), &[call(1, "list_tabs", json!({}))]));
        extension.request();
        extension.disconnect();
        replies.join().unwrap()
    });

    assert_eq!(replies[0]["result"]["isError"], true);
    assert!(text(&replies[0]).contains("disconnected"));
}

/// Two sessions both number their requests from 1. Each gets its own reply back with its own id,
/// even when the extension answers them in the other order.
#[test]
fn each_reply_reaches_the_session_that_asked() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);
    let key = secret(directory.path());

    let mut first = Peer::connect(directory.path(), &key);
    first.send(&json!({"id": 1, "method": "list_tabs", "params": {"from": "first"}}));
    let to_first = extension.request();
    let mut second = Peer::connect(directory.path(), &key);
    second.send(&json!({"id": 1, "method": "list_tabs", "params": {"from": "second"}}));
    let to_second = extension.request();

    assert_eq!(to_first["params"]["from"], "first");
    assert_eq!(to_second["params"]["from"], "second");
    assert_ne!(to_first["id"], to_second["id"]);

    extension.reply(json!({"id": to_second["id"], "result": "for the second"}));
    extension.reply(json!({"id": to_first["id"], "result": "for the first"}));

    let first_reply = first.receive().unwrap();
    let second_reply = second.receive().unwrap();
    assert_eq!(first_reply, json!({"id": 1, "result": "for the first"}));
    assert_eq!(second_reply, json!({"id": 1, "result": "for the second"}));
}

/// Replies go out on one thread, in the order the extension answers them. A peer that stops reading
/// is dropped once it has not taken its reply for long enough, and the next reply reaches the
/// session that asked for it rather than waiting behind the stopped one for ever.
#[test]
fn a_peer_that_stops_reading_holds_up_no_other_session() {
    use bravebot_browser::host::REPLY_WRITE_TIMEOUT;

    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);
    let key = secret(directory.path());

    let mut stopped = Peer::connect(directory.path(), &key);
    stopped.send(&json!({"id": 1, "method": "read_page", "params": {}}));
    let to_stopped = extension.request();
    let mut reading = Peer::connect(directory.path(), &key);
    reading.send(&json!({"id": 1, "method": "list_tabs", "params": {}}));
    let to_reading = extension.request();

    let page = "x".repeat(4 * 1024 * 1024);
    let started = Instant::now();
    extension.reply_large(json!({"id": to_stopped["id"], "result": page}));
    extension.reply(json!({"id": to_reading["id"], "result": "for the reading one"}));

    reading
        .writer
        .set_read_timeout(Some(REPLY_WRITE_TIMEOUT + WAIT))
        .unwrap();
    assert_eq!(
        reading.receive(),
        Some(json!({"id": 1, "result": "for the reading one"}))
    );
    // Within one bound of the stopped reply, and not one per write it took to fill the socket.
    assert!(
        started.elapsed() < REPLY_WRITE_TIMEOUT + Duration::from_secs(3),
        "the reply waited {:?} behind the stopped peer",
        started.elapsed()
    );

    // A socket the host has shut down can refuse a new timeout, which is the answer already.
    let _ = stopped.writer.set_read_timeout(Some(WAIT));
    let mut taken = Vec::new();
    let ended = std::io::Read::read_to_end(&mut stopped.reader, &mut taken);
    let dropped = match &ended {
        Ok(_) => true,
        Err(error) => error.kind() == std::io::ErrorKind::ConnectionReset,
    };
    assert!(
        dropped && taken.len() < page.len(),
        "the stopped peer was not dropped: {ended:?} after {} bytes",
        taken.len()
    );
}

/// A peer can also hold its connection by sending lines it never reads the answers to: each one
/// that is not a request gets an error back, and once those fill the socket every one waits out its
/// time. The first that does not go closes the connection, so the peer holds the host for at most
/// one bound rather than one per line it sends.
#[test]
fn a_peer_that_sends_lines_and_does_not_read_is_closed() {
    use bravebot_browser::host::REPLY_WRITE_TIMEOUT;

    let directory = installed_for(OURS);
    let _extension = Extension::connect(directory.path(), OURS);
    let peer = Peer::connect(directory.path(), &secret(directory.path()));
    let mut writer = peer.writer.try_clone().unwrap();
    writer
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();

    let started = Instant::now();
    let (closed, until) = mpsc::channel();
    std::thread::spawn(move || {
        let limit = REPLY_WRITE_TIMEOUT * 4;
        while started.elapsed() < limit {
            match writer.write_all(b"not json\n") {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => {
                    let _ = closed.send(Some(started.elapsed()));
                    return;
                }
            }
        }
        let _ = closed.send(None);
    });

    let after = until
        .recv_timeout(REPLY_WRITE_TIMEOUT * 4 + WAIT)
        .expect("the writing thread finished");
    let Some(after) = after else {
        panic!("a peer that did not read its errors was kept open");
    };
    assert!(
        after < REPLY_WRITE_TIMEOUT + WAIT,
        "a peer that did not read its errors was closed only after {after:?}"
    );
    drop(peer);
}

/// A length over what the extension may send is a stream that has gone wrong, and nothing after it
/// can be read as a message. The host ends, and takes its socket and secret with it, rather than
/// reading on out of step.
#[test]
fn a_message_over_the_limit_from_the_extension_ends_the_host() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);
    let length = u32::try_from(framing::FROM_EXTENSION_LIMIT + 1).unwrap();
    let to_host = extension.to_host.as_mut().unwrap();
    to_host.write_all(&length.to_ne_bytes()).unwrap();
    to_host.flush().unwrap();

    let status = exit_status(&mut extension.host, "the host ends");
    assert!(!status.success());
    assert!(!directory.path().join("socket").exists());
    assert!(!directory.path().join("secret").exists());
}

/// Brave closes the port on a message over 1 MB, which would drop every session. The host refuses
/// such a request itself, and the next request the extension sees is the one after it.
#[test]
fn a_request_over_the_limit_is_refused_and_never_reaches_the_extension() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);
    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));

    let padding = "x".repeat(framing::TO_EXTENSION_LIMIT - 40);
    peer.send(&json!({"id": 1, "method": "read_page", "params": {"padding": padding}}));
    let refused = peer.receive().unwrap();
    assert_eq!(refused["id"], 1);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("limit")
    );

    peer.send(&json!({"id": 2, "method": "list_tabs", "params": {}}));
    assert_eq!(extension.request()["method"], "list_tabs");
}

/// A request far past the line limit is drained, refused under its own id and never sent. The next
/// request stays aligned and reaches the extension.
#[test]
fn a_request_far_over_the_limit_keeps_its_id_and_the_next_request() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);
    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));

    let padding = "x".repeat(framing::TO_EXTENSION_LIMIT * 2);
    peer.send(&json!({"method": "read_page", "params": {"padding": padding}, "id": 5}));
    let refused = peer.receive().unwrap();
    assert_eq!(refused["id"], 5);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("limit")
    );

    peer.send(&json!({"id": 6, "method": "list_tabs", "params": {}}));
    assert_eq!(extension.request()["method"], "list_tabs");
}

/// An id as long as a message may be is not held: the request is refused under no id, as JSON-RPC
/// answers a request whose id it could not read, whether its line fits and is read whole or is too
/// long and is drained. Neither is sent, and the next request on the connection is read as its own.
#[test]
fn a_request_whose_id_is_too_long_to_keep_is_refused_under_no_id() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);
    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));

    let id = "x".repeat(framing::TO_EXTENSION_LIMIT);
    let padding = "y".repeat(8192);
    let read_whole = json!({"id": id, "method": "list_tabs", "params": {}});
    let drained = json!({"id": id, "method": "list_tabs", "params": {"padding": padding}});
    for request in [read_whole, drained] {
        peer.send(&request);
        let refused = peer.receive().unwrap();
        assert_eq!(refused["id"], Value::Null);
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("limit")
        );
    }

    peer.send(&json!({"id": 7, "method": "list_tabs", "params": {"after": true}}));
    assert_eq!(extension.request()["params"], json!({"after": true}));
}

/// Whitespace around an id is not part of it, so a line pushed over the limit by spaces after a
/// short id is refused under that id, and the next request still reaches the extension.
#[test]
fn a_request_padded_after_its_id_is_refused_under_that_id() {
    let directory = installed_for(OURS);
    let extension = Extension::connect(directory.path(), OURS);
    let mut peer = Peer::connect(directory.path(), &secret(directory.path()));

    let padding = " ".repeat(framing::TO_EXTENSION_LIMIT + 8192);
    for (id, line) in [
        (
            json!(5),
            format!(r#"{{"id":5{padding},"method":"list_tabs"}}"#),
        ),
        (json!("a b"), format!(r#"{{"id":{padding}"a b"{padding}}}"#)),
    ] {
        writeln!(peer.writer, "{line}").unwrap();
        let refused = peer.receive().unwrap();
        assert_eq!(refused["id"], id);
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("limit")
        );
    }

    peer.send(&json!({"id": 7, "method": "list_tabs", "params": {"after": true}}));
    assert_eq!(extension.request()["params"], json!({"after": true}));
}
/// host's manifest as it was, and records the extension the host checks origins against.
#[test]
fn installing_writes_one_manifest_for_our_extension_alone() {
    let manifests = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let neighbour = manifests.path().join("com.example.other.json");
    std::fs::write(&neighbour, "{}").unwrap();

    let status = Command::new(PROGRAM)
        .args(["install", "--manifest-dir"])
        .arg(manifests.path())
        .arg(OURS)
        .env("BRAVEBOT_BROWSER_DIR", directory.path())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    let written = manifests.path().join("com.brave.bravebot.json");
    let manifest: Value = serde_json::from_slice(&std::fs::read(&written).unwrap()).unwrap();
    let program = PathBuf::from(PROGRAM).canonicalize().unwrap();
    assert_eq!(manifest["name"], "com.brave.bravebot");
    assert_eq!(manifest["type"], "stdio");
    assert_eq!(manifest["path"], json!(program));
    assert_eq!(manifest["allowed_origins"], json!([origin(OURS)]));
    assert_eq!(std::fs::read_to_string(&neighbour).unwrap(), "{}");
    assert_eq!(std::fs::read_dir(manifests.path()).unwrap().count(), 2);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("extension")).unwrap(),
        OURS
    );
}

/// Given no id, installing records the one the extension in this repository has, so a person
/// loading it unpacked has nothing to copy.
#[test]
fn installing_with_no_id_records_the_extension_in_this_repository() {
    let manifests = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let status = Command::new(PROGRAM)
        .args(["install", "--manifest-dir"])
        .arg(manifests.path())
        .env("BRAVEBOT_BROWSER_DIR", directory.path())
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let id = bravebot_browser::install::EXTENSION_ID;
    let written = manifests.path().join("com.brave.bravebot.json");
    let manifest: Value = serde_json::from_slice(&std::fs::read(&written).unwrap()).unwrap();
    assert_eq!(manifest["allowed_origins"], json!([origin(id)]));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("extension")).unwrap(),
        id
    );
}

/// An id that is not an extension's is refused before anything is written, so a mistyped one
/// cannot leave a manifest naming nobody.
#[test]
fn installing_refuses_what_is_not_an_extension_id() {
    let manifests = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let status = Command::new(PROGRAM)
        .args(["install", "--manifest-dir"])
        .arg(manifests.path())
        .arg("not-an-extension-id")
        .env("BRAVEBOT_BROWSER_DIR", directory.path())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(std::fs::read_dir(manifests.path()).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

/// A flag with its value missing is a mistake in the command line, not an extension id, so it is
/// answered with the usage rather than with a complaint about the id, and nothing is written.
#[test]
fn installing_with_a_flag_missing_its_value_prints_the_usage() {
    let directory = tempfile::tempdir().unwrap();
    let refused = Command::new(PROGRAM)
        .args(["install", "--manifest-dir"])
        .env("BRAVEBOT_BROWSER_DIR", directory.path())
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(said.contains("usage: bravebot-browser install"), "{said}");
    assert!(!said.contains("is not an extension id"), "{said}");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

/// JSON that is not a request object is answered with an error under no id, as JSON-RPC asks, so a
/// client is never left waiting on a line the server read. A notification is still answered with
/// nothing: only the two lines that are not requests get replies.
#[test]
fn a_line_that_is_not_a_request_is_answered_with_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let replies = mcp(
        directory.path(),
        &[
            json!([1, 2]),
            json!("list_tabs"),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        ],
    );
    assert_eq!(replies.len(), 2, "{replies:?}");
    for reply in &replies {
        assert_eq!(reply["id"], Value::Null, "{reply}");
        assert_eq!(reply["error"]["code"], -32600, "{reply}");
    }
}

/// A person vouches for the list once and BraveBot records its digest, so the list is the same
/// whether or not the extension is connected.
#[test]
fn the_tool_list_is_the_same_whether_or_not_the_extension_is_connected() {
    let list = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
    let directory = installed_for(OURS);
    let without = mcp(directory.path(), std::slice::from_ref(&list));
    let _extension = Extension::connect(directory.path(), OURS);
    let with = mcp(directory.path(), std::slice::from_ref(&list));

    assert_eq!(without, with);
    let names: Vec<&str> = with[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "get_platform_info",
            "list_tabs",
            "read_page",
            "search_history",
            "search_bookmarks"
        ]
    );
}

/// A tool's arguments reach the extension unchanged as that method's parameters.
#[test]
fn a_tools_arguments_are_the_extension_methods_parameters() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);
    let arguments = json!({"url": "https://brave.com/", "nested": {"kept": true}});

    let replies = std::thread::scope(|scope| {
        let call = call(1, "read_page", arguments.clone());
        let replies = scope.spawn(|| mcp(directory.path(), &[call]));
        let request = extension.request();
        assert_eq!(request["method"], "read_page");
        assert_eq!(request["params"], arguments);
        extension.reply(json!({"id": request["id"], "result": "page"}));
        replies.join().unwrap()
    });
    assert_eq!(replies[0]["result"]["isError"], false);
}

/// A tool the list does not have is refused by the server, and nothing is sent to the extension:
/// the first request the extension sees is the second call's.
#[test]
fn a_tool_that_is_not_on_the_list_is_refused_without_asking_the_extension() {
    let directory = installed_for(OURS);
    let mut extension = Extension::connect(directory.path(), OURS);

    let replies = std::thread::scope(|scope| {
        let replies = scope.spawn(|| {
            mcp(
                directory.path(),
                &[
                    call(1, "close_all_tabs", json!({})),
                    call(2, "list_tabs", json!({})),
                ],
            )
        });
        let request = extension.request();
        assert_eq!(request["method"], "list_tabs");
        extension.reply(json!({"id": request["id"], "result": []}));
        replies.join().unwrap()
    });

    assert_eq!(replies[0]["result"]["isError"], true);
    assert!(text(&replies[0]).contains("close_all_tabs"));
    assert_eq!(replies[1]["result"]["isError"], false);
}
