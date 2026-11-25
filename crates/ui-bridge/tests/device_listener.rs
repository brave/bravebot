// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host as a process that listens for a phone: `--listen`, the pair and confirm commands the
//! person types on the host, and a phone that connects over TCP on loopback.

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

mod support;

use bravebot_listener::invitation::parse_link;
use bravebot_listener::phone::Phone;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Host, Start};

/// The client the host starts. Does nothing unless it was started as one.
#[test]
#[ignore = "run by the host as its client"]
fn relay_entry() {
    support::relay();
}

struct Scratch {
    directory: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        for name in ["project", "home/.bravebot", "root"] {
            std::fs::create_dir_all(directory.path().join(name)).unwrap();
        }
        std::fs::set_permissions(
            directory.path().join("root"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        Self { directory }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name).canonicalize().unwrap()
    }
}

/// Everything the host writes on its standard error, kept as written.
#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<u8>>>);

impl Write for Kept {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Kept {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// A host listening on loopback, its client connected and its startup question answered.
fn listening_host(scratch: &Scratch, kept: &Kept) -> Host {
    let mut host = Host::launch_with(
        Start {
            home: &scratch.path("home"),
            sessions_root: &scratch.path("root"),
            workspace: &scratch.path("project"),
            relay: &scratch.directory.path().join("relay.sock"),
            settings: None,
            current_dir: None,
            environment: &[],
            stdout: Stdio::null(),
            stderr: Box::new(kept.clone()),
            clients: support::ONE_CLIENT,
        },
        &[],
        &[OsStr::new("--listen"), OsStr::new("127.0.0.1:0")],
    );
    host.decline_at_startup();
    host
}

/// The address a host says it listens on, and the hash of its key.
fn address_of(said: &str) -> (std::net::SocketAddr, String) {
    let rest = said.split("listening on ").nth(1).unwrap();
    let (address, key) = rest.split_once(", host key ").unwrap();
    (address.parse().unwrap(), key.replace(' ', ""))
}

fn say(
    connection: &mut bravebot_listener::phone::Connection,
    method: &str,
    params: &Value,
) -> Value {
    connection
        .send(&json!({"id": 9, "method": method, "params": params}).to_string())
        .unwrap();
    serde_json::from_str(&connection.receive().unwrap().unwrap()).unwrap()
}

/// A phone pairs through what the person types on the host: the host names the address and its
/// key, shows the invitation, says a phone is asking and what to type, pairs it on the code, and
/// then answers it with the synthetic session. Nothing it says on its standard error holds the code.
#[test]
fn a_phone_pairs_through_the_hosts_input_and_is_shown_synthetic_data() {
    let scratch = Scratch::new();
    let kept = Kept::default();
    let mut host = listening_host(&scratch, &kept);
    let (address, key) = address_of(&host.says("listening on"));
    assert_eq!(address.ip().to_string(), "127.0.0.1");
    assert_eq!(key.len(), 64);

    host.command(&format!(
        "pair kitchen-phone control {}",
        scratch.path("project").display()
    ));
    let said = host.says("or type this link on the phone: ");
    let link_text = said
        .split("or type this link on the phone: ")
        .nth(1)
        .unwrap()
        .to_string();
    let link = parse_link(&link_text).unwrap();
    assert_eq!(link.addresses, [address]);
    assert_eq!(bravebot_listener::identity::hex(&link.host), key);

    let phone = Phone::new().unwrap();
    let mut connection = phone.connect(link.addresses[0], link.host).unwrap();
    let code = connection
        .begin_pairing(&link.secret, Some("Pixel \u{1b}[2J\nhost: paired"))
        .unwrap();
    let asking = host.says("is asking to pair as kitchen-phone");
    assert!(asking.contains("a phone calling itself \"Pixel \\u{1b}[2J\\u{a}host: paired\""));
    assert!(!asking.contains(&code));

    host.command(&format!("confirm {code}"));
    host.says("paired kitchen-phone");
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some(r#"{"id":1,"ok":{"paired":true}}"#)
    );
    let info = say(&mut connection, "instance.info", &json!({}));
    assert_eq!(info["ok"]["synthetic"], true);
    assert_eq!(info["ok"]["device"]["grant"], "control");
    let refused = say(
        &mut connection,
        "session.new",
        &json!({"directory": scratch.path("project")}),
    );
    assert_eq!(refused["error"]["code"], "not_allowed");

    host.command("devices");
    let listed = host.says("kitchen-phone control");
    assert!(
        listed.contains("connected") && !listed.contains("not connected"),
        "{listed}"
    );

    let everything = kept.text();
    assert!(
        !everything.contains(&code),
        "the host said the code the phone shows"
    );
    let secret = bravebot_listener::identity::hex(&link.secret);
    assert!(!everything.contains(&secret));
    // The link writes the secret in base64, so that is the form to look for: it is on the one line
    // that shows the link and on no other.
    let written = link_text.rsplit("&s=").next().unwrap();
    assert_eq!(written.len(), 43);
    let lines_with_it: Vec<&str> = everything
        .lines()
        .filter(|line| line.contains(written))
        .collect();
    assert_eq!(lines_with_it.len(), 1, "{lines_with_it:?}");
    assert!(lines_with_it[0].ends_with(&link_text), "{lines_with_it:?}");
    // The host's standard error is a pipe here, so it draws no QR code, and nothing a phone wrote
    // reached it as an escape.
    assert!(!everything.contains('\u{1b}'));
}

/// A host that was not told to listen has no pairing to do, and says how to get one.
#[test]
fn a_host_not_told_to_listen_says_how_to_pair() {
    let scratch = Scratch::new();
    let mut host = Host::launch(Start {
        home: &scratch.path("home"),
        sessions_root: &scratch.path("root"),
        workspace: &scratch.path("project"),
        relay: &scratch.directory.path().join("relay.sock"),
        settings: None,
        current_dir: None,
        environment: &[],
        stdout: Stdio::null(),
        stderr: support::discarded(),
        clients: support::ONE_CLIENT,
    });
    for line in ["pair one read /x", "confirm 123456", "devices"] {
        host.command(line);
        host.says("start the host with --listen ADDR:PORT to pair a device");
    }
}

/// An address that is every interface, one that is not an address, and a second address are refused
/// with status 2 before a directory is made.
#[test]
fn a_listen_that_does_not_name_one_address_is_refused_before_anything_is_made() {
    for arguments in [
        &["--listen", "0.0.0.0:7443"][..],
        &["--listen", "[::]:7443"],
        &["--listen", "localhost"],
        &["--listen", "127.0.0.1"],
        &["--listen", "127.0.0.1:1", "--listen", "127.0.0.1:2"],
        &["--listen"],
    ] {
        let scratch = Scratch::new();
        let root = scratch.directory.path().join("made-by-the-host");
        let status = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
            .env_clear()
            .args(["--sessions-root"])
            .arg(&root)
            .arg("--workspace")
            .arg(scratch.path("project"))
            .args(arguments)
            .args([
                "--client",
                "x",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(2), "{arguments:?}");
        assert!(!root.exists(), "{arguments:?} made a directory");
    }
}

/// An address that cannot be bound stops the host with status 1, says why, and starts no client.
#[test]
fn a_listen_that_cannot_be_bound_stops_the_host_before_its_client_starts() {
    let scratch = Scratch::new();
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let mut host = Host::launch_with(
        Start {
            home: &scratch.path("home"),
            sessions_root: &scratch.path("root"),
            workspace: &scratch.path("project"),
            relay: &scratch.directory.path().join("relay.sock"),
            settings: None,
            current_dir: None,
            environment: &[],
            stdout: Stdio::null(),
            stderr: support::discarded(),
            clients: support::ONE_CLIENT,
        },
        &[],
        &[OsStr::new("--listen"), OsStr::new(&address)],
    );
    host.decline_at_startup();
    host.says("cannot listen");
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = host.child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < until, "the host did not stop");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(1));
    // A client that is never started gives no event to wait for. This is the time in which a client
    // the host started before it exited would reach the relay, as `quit_ends_a_phones_connection_and_stops_the_host`
    // shows one does when the bind succeeds.
    assert!(host.client_within(Duration::from_millis(500)).is_none());
}

/// `quit` ends a phone's connection as it ends a client's, and the host stops.
#[test]
fn quit_ends_a_phones_connection_and_stops_the_host() {
    let scratch = Scratch::new();
    let kept = Kept::default();
    let mut host = listening_host(&scratch, &kept);
    let _client = host.client();
    let (address, _) = address_of(&host.says("listening on"));
    host.command(&format!(
        "pair one read {}",
        scratch.path("project").display()
    ));
    let link = parse_link(
        host.says("or type this link on the phone: ")
            .split("or type this link on the phone: ")
            .nth(1)
            .unwrap(),
    )
    .unwrap();
    let phone = Phone::new().unwrap();
    let mut connection = phone.connect(address, link.host).unwrap();
    let code = connection.begin_pairing(&link.secret, None).unwrap();
    host.says("is asking to pair");
    host.command(&format!("confirm {code}"));
    connection.receive().unwrap();
    host.command("quit");
    assert!(connection.ended());
    let until = Instant::now() + Duration::from_secs(10);
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < until, "the host did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `revoke NAME` on the host's input removes the device, ends its connection at once and says so, and
/// the host holds no device afterwards. A name no device has, and no name, are answered and change
/// nothing.
#[test]
fn revoke_on_the_hosts_input_ends_the_phones_connection_and_removes_the_device() {
    let scratch = Scratch::new();
    let kept = Kept::default();
    let mut host = listening_host(&scratch, &kept);
    let (address, _) = address_of(&host.says("listening on"));
    host.command(&format!(
        "pair one read {}",
        scratch.path("project").display()
    ));
    let link = parse_link(
        host.says("or type this link on the phone: ")
            .split("or type this link on the phone: ")
            .nth(1)
            .unwrap(),
    )
    .unwrap();
    let phone = Phone::new().unwrap();
    let mut connection = phone.connect(address, link.host).unwrap();
    let code = connection.begin_pairing(&link.secret, None).unwrap();
    host.says("is asking to pair");
    host.command(&format!("confirm {code}"));
    connection.receive().unwrap();
    let alive = say(&mut connection, "instance.info", &json!({}));
    assert_eq!(alive["ok"]["synthetic"], true);

    host.command("revoke nobody");
    host.says("no device has that name: nobody");
    host.command("revoke");
    host.says("type `revoke NAME`");
    host.command("revoke one");
    host.says("revoked one");
    assert!(connection.ended());
    host.command("devices");
    host.says("no device is paired");
}

/// The command-line phone pairs with the real host from the link it shows, and the host's `devices`
/// command lists it: the whole of pairing by hand, with a program in place of a phone.
#[test]
fn a_command_line_phone_pairs_with_the_real_host_and_is_answered() {
    let scratch = Scratch::new();
    let kept = Kept::default();
    let mut host = listening_host(&scratch, &kept);
    host.says("listening on");
    host.command(&format!(
        "pair by-hand control {}",
        scratch.path("project").display()
    ));
    let said = host.says("or type this link on the phone: ");
    let link = said
        .split("or type this link on the phone: ")
        .nth(1)
        .unwrap()
        .to_string();

    let written = Kept::default();
    let (finished, result) = std::sync::mpsc::channel();
    {
        let mut written = written.clone();
        std::thread::spawn(move || {
            let mut input = std::io::Cursor::new(
                "{\"id\":1,\"method\":\"instance.info\",\"params\":{}}\n\
                 {\"id\":2,\"method\":\"session.new\",\"params\":{}}\n",
            );
            let done = bravebot_listener::phone::pair_and_talk(&link, &mut input, &mut written);
            finished.send(done).ok();
        });
    }
    let until = Instant::now() + Duration::from_secs(10);
    let code = loop {
        let text = written.text();
        if let Some((_, after)) = text.split_once("code: ") {
            let digits: String = after.chars().take(6).collect();
            if digits.len() == 6 {
                break digits;
            }
        }
        assert!(Instant::now() < until, "the phone showed no code: {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    };
    host.says("is asking to pair as by-hand");
    host.command(&format!("confirm {code}"));
    host.says("paired by-hand");
    result
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    let text = written.text();
    assert!(text.contains("\"synthetic\":true"), "{text}");
    assert!(text.contains("\"code\":\"not_allowed\""), "{text}");
    host.command("devices");
    host.says("by-hand control");
}
