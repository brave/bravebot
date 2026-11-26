// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A phone pairing over a real socket on loopback: the invitation, the proof, the code the person
//! types on the host, and each way it can fail.

use crate::identity::Identity;
use crate::invitation::{Announce, Announcement, Clock, ConfirmError, Invitations, Moment, Terms};
use crate::limits::Limits;
use crate::pairing;
use crate::phone::{Connection, Phone};
use crate::registry::{Device, Registry};
use crate::server::{Listener, Prepared, Service};
use bravebot_core::device::{Grant, Operation, Standing};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

struct Echo;

impl Service for Echo {
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        Prepared::new(Operation::Read, move |device| {
            vec![format!("{}:{line}", device.name)]
        })
    }
}

struct Rig {
    listener: Listener,
    registry: Arc<Registry>,
    invitations: Arc<Invitations>,
    said: Arc<Mutex<Vec<Announcement>>>,
}

fn rig_with(limits: Limits) -> Rig {
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let said = Arc::new(Mutex::new(Vec::new()));
    let announce: Announce = {
        let said = Arc::clone(&said);
        Arc::new(move |announcement| said.lock().unwrap().push(announcement.clone()))
    };
    let invitations = Arc::new(Invitations::system(Arc::clone(&registry), announce));
    let listener = Listener::start(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        &identity,
        Arc::clone(&registry),
        Arc::clone(&invitations),
        Arc::new(Echo),
        limits,
    )
    .unwrap();
    Rig {
        listener,
        registry,
        invitations,
        said,
    }
}

fn rig() -> Rig {
    rig_with(Limits::default())
}

fn terms(name: &str, grant: Grant) -> Terms {
    Terms {
        name: name.into(),
        grant,
        workspace: PathBuf::from("/work"),
    }
}

impl Rig {
    fn connect(&self, phone: &Phone) -> Connection {
        phone
            .connect(self.listener.address(), self.listener.key_hash())
            .unwrap()
    }

    fn said(&self) -> Vec<Announcement> {
        self.said.lock().unwrap().clone()
    }

    /// Wait for the host to say something `wanted` accepts.
    fn hears(&self, wanted: impl Fn(&Announcement) -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if self.said().iter().any(&wanted) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(self.said().iter().any(&wanted), "{:?}", self.said());
    }
}

/// With no device paired and no window open nobody can be let in, so a connection is closed as it
/// arrives. Opening a window lets a phone connect, and closing it does not let one in again.
#[test]
fn a_listener_with_no_device_and_no_window_accepts_nobody() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    assert!(
        phone
            .connect(rig.listener.address(), rig.listener.key_hash())
            .is_err()
    );
    rig.invitations.open(terms("one", Grant::Read)).unwrap();
    assert!(
        phone
            .connect(rig.listener.address(), rig.listener.key_hash())
            .is_ok()
    );
    rig.invitations.close();
    assert!(
        phone
            .connect(rig.listener.address(), rig.listener.key_hash())
            .is_err()
    );
}

/// The whole pairing: a phone that has the secret proves it, shows a code, and is paired once the
/// person types that code on the host. It is answered on the connection it paired over, with the
/// grant and the workspace the person chose, and not before the code is typed.
#[test]
fn a_phone_pairs_with_the_secret_and_the_code_typed_on_the_host() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    let opened = rig
        .invitations
        .open(terms("kitchen-phone", Grant::Control))
        .unwrap();
    let mut connection = rig.connect(&phone);
    let code = connection
        .begin_pairing(&opened.secret, Some("Pixel 10 Pro"))
        .unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    // Proved the secret, and not paired: the host has said nothing to the phone.
    assert_eq!(rig.registry.standing(&phone.key_hash()), Standing::Unknown);
    assert!(
        connection.receive().is_err(),
        "the host answered before the code was typed"
    );
    assert_eq!(rig.invitations.confirm(&code).unwrap(), "kitchen-phone");
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some(r#"{"id":1,"ok":{"paired":true}}"#)
    );
    assert_eq!(
        rig.registry.standing(&phone.key_hash()),
        Standing::Paired(Grant::Control)
    );
    // The same connection is served as the device it is now.
    connection.send("hello").unwrap();
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some("kitchen-phone:hello")
    );
    assert!(rig.registry.is_connected(&phone.key_hash()));
    assert_eq!(
        rig.said(),
        [
            Announcement::Asking {
                name: "kitchen-phone".into(),
                label: Some("Pixel 10 Pro".into())
            },
            Announcement::Paired {
                name: "kitchen-phone".into()
            }
        ]
    );
}

/// What the host says while a phone waits for its code holds the name the person chose and what the
/// phone called itself, and neither the code nor the secret.
#[test]
fn the_host_does_not_say_the_code_or_the_secret() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&phone);
    let code = connection.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    let heard = format!("{:?}", rig.said());
    assert!(!heard.contains(&code), "{heard}");
    assert!(
        !heard.contains(&crate::identity::hex(&opened.secret)),
        "{heard}"
    );
}

/// A label that is too long is left out and the phone is still paired. A label that is text a
/// terminal would act on is carried as written, for the host to draw escaped.
#[test]
fn a_label_is_carried_as_written_and_left_out_where_it_is_too_long() {
    for (label, carried) in [
        (
            "\u{1b}[2J\u{202e}evil\nline".to_string(),
            Some("\u{1b}[2J\u{202e}evil\nline".to_string()),
        ),
        ("x".repeat(64), Some("x".repeat(64))),
        ("x".repeat(65), None),
    ] {
        let rig = rig();
        let phone = Phone::new().unwrap();
        let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
        let mut connection = rig.connect(&phone);
        let code = connection
            .begin_pairing(&opened.secret, Some(&label))
            .unwrap();
        rig.hears(|said| matches!(said, Announcement::Asking { .. }));
        assert_eq!(
            rig.said()[0],
            Announcement::Asking {
                name: "one".into(),
                label: carried
            }
        );
        rig.invitations.confirm(&code).unwrap();
        assert!(connection.receive().unwrap().is_some());
    }
}

/// A phone that does not have the secret is closed, and the window is closed with it, so the phone
/// that does have it is refused afterwards.
#[test]
fn a_phone_without_the_secret_is_closed_and_the_window_closes() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let stranger = Phone::new().unwrap();
    let mut guess = rig.connect(&stranger);
    guess.begin_pairing(&[0; 32], None).unwrap();
    assert!(guess.ended());
    assert!(!rig.invitations.is_open());
    let phone = Phone::new().unwrap();
    // The listener has a device to let in no more, and no window: it accepts nobody.
    assert!(
        phone
            .connect(rig.listener.address(), rig.listener.key_hash())
            .is_err()
    );
    assert!(rig.registry.is_empty());
    let _ = opened;
}

/// A proof made on one connection and sent on another is refused, though the secret and the key
/// are right: the value it was made from is the first connection's.
#[test]
fn a_proof_made_on_another_connection_is_refused() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let first = rig.connect(&phone);
    let exporter = first
        .exporter(pairing::EXPORTER_LABEL, Some(pairing::EXPORTER_CONTEXT))
        .unwrap();
    let proof = pairing::proof(&opened.secret, &exporter, &phone.key_hash());
    let mut second = rig.connect(&phone);
    second.send_proof(&proof, None).unwrap();
    assert!(second.ended());
    assert!(rig.registry.is_empty());
    assert!(!rig.invitations.is_open());
}

/// A second phone that proves the secret while the first waits for its code is refused, and the
/// first is not paired.
#[test]
fn a_second_phone_closes_the_pairing_of_the_first() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let (first, second) = (Phone::new().unwrap(), Phone::new().unwrap());
    let mut waiting = rig.connect(&first);
    let code = waiting.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    let mut other = rig.connect(&second);
    other.begin_pairing(&opened.secret, None).unwrap();
    assert!(other.ended());
    assert!(waiting.ended());
    assert_eq!(
        rig.invitations.confirm(&code),
        Err(ConfirmError::NothingToConfirm)
    );
    assert!(rig.registry.is_empty());
}

/// A code that is not the phone's pairs nothing and ends the phone's connection.
#[test]
fn a_wrong_code_pairs_nothing_and_ends_the_connection() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&phone);
    let code = connection.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(rig.invitations.confirm(wrong), Err(ConfirmError::WrongCode));
    assert!(connection.ended());
    assert!(rig.registry.is_empty());
}

/// A first line that is not a request to pair ends the connection and leaves the window open for a
/// phone that is, however it is wrong. A line that is too long is not read.
#[test]
fn a_first_line_that_is_not_a_pairing_request_ends_the_connection_and_leaves_the_window_open() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let lines = [
        "not json".to_string(),
        "[]".to_string(),
        r#"{"id":1,"method":"instance.info","params":{}}"#.to_string(),
        r#"{"id":1,"method":"pair.begin"}"#.to_string(),
        r#"{"id":1,"method":"pair.begin","params":{}}"#.to_string(),
        r#"{"id":1,"method":"pair.begin","params":{"proof":"not-hex"}}"#.to_string(),
        r#"{"id":1,"method":"pair.begin","params":{"proof":7}}"#.to_string(),
        format!("{{\"padding\":\"{}\"}}", "x".repeat(5000)),
    ];
    for line in lines {
        let stranger = Phone::new().unwrap();
        let mut connection = rig.connect(&stranger);
        connection.send(&line).ok();
        let shown: String = line.chars().take(60).collect();
        assert!(connection.ended(), "{shown}");
        assert!(rig.invitations.is_open(), "{shown}");
    }
    // The window is still open for a phone that has the secret.
    let phone = Phone::new().unwrap();
    let mut connection = rig.connect(&phone);
    let code = connection.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.invitations.confirm(&code).unwrap();
    assert!(connection.receive().unwrap().is_some());
}

/// A first line over the limit is refused even when it would be a request that proves the secret
/// if it were read, so the limit is what stops it and not the shape of what is in it.
#[test]
fn a_first_line_over_the_limit_is_refused_though_it_would_pair_the_phone() {
    let rig = rig();
    let phone = Phone::new().unwrap();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&phone);
    let exporter = connection
        .exporter(pairing::EXPORTER_LABEL, Some(pairing::EXPORTER_CONTEXT))
        .unwrap();
    let proof = pairing::proof(&opened.secret, &exporter, &phone.key_hash());
    let padded = serde_json::json!({
        "id": 1,
        "method": pairing::BEGIN,
        "params": {"proof": crate::identity::hex(&proof), "padding": "x".repeat(5000)},
    });
    connection.send(&padded.to_string()).ok();
    assert!(connection.ended());
    assert!(rig.invitations.is_open());
    assert!(
        !rig.said()
            .iter()
            .any(|said| matches!(said, Announcement::Asking { .. }))
    );
    // The same request within the limit pairs the phone, so the padding is all that was wrong.
    let mut within = rig.connect(&phone);
    let code = within.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.invitations.confirm(&code).unwrap();
}

/// A first line that does not arrive within the time of the handshake ends the connection, however
/// slowly bytes come.
#[test]
fn a_first_line_that_never_ends_is_closed_when_the_time_is_up() {
    let rig = rig_with(Limits {
        handshake: Duration::from_millis(600),
        ..Limits::default()
    });
    rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&Phone::new().unwrap());
    connection.send_bytes(b"{\"id\":1").unwrap();
    let started = Instant::now();
    assert!(connection.ended());
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(rig.invitations.is_open());
}

/// A first line that is sent a byte at a time is closed when the time of the handshake is up, though
/// its bytes keep coming and every one of them arrives sooner than the time allows: the host counts
/// the time of the line and not the time between its bytes.
#[test]
fn a_first_line_that_trickles_is_closed_when_the_time_is_up() {
    let rig = rig_with(Limits {
        handshake: Duration::from_millis(600),
        ..Limits::default()
    });
    rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&Phone::new().unwrap());
    let started = Instant::now();
    // A hundred bytes at 40 ms each is four seconds, and the host gives the line 600 ms.
    // The send ends when the host closes the connection, so how long it took is how long the host
    // kept the connection.
    connection
        .send_slowly(&"x".repeat(100), Duration::from_millis(40))
        .ok();
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "the host kept a connection that trickled a line for {:?}",
        started.elapsed()
    );
    assert!(connection.ended());
    assert!(rig.invitations.is_open());
}

/// Stopping the listener ends a phone that connected and has not yet sent its first line.
#[test]
fn stopping_ends_a_phone_that_has_not_sent_its_first_line() {
    let rig = rig();
    rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&Phone::new().unwrap());
    let started = Instant::now();
    rig.listener.stop();
    assert!(connection.ended());
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// A window that runs out is said to have ended by the host while it listens, and not only when
/// something next asks the window about itself: the code is still on the person's screen.
#[test]
fn a_window_that_runs_out_is_announced_while_the_listener_runs() {
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let said = Arc::new(Mutex::new(Vec::new()));
    let announce: Announce = {
        let said = Arc::clone(&said);
        Arc::new(move |announcement| said.lock().unwrap().push(announcement.clone()))
    };
    let (start, wall_start) = (Instant::now(), SystemTime::now());
    let passed = Arc::new(Mutex::new(Duration::ZERO));
    let clock: Clock = {
        let passed = Arc::clone(&passed);
        Arc::new(move || {
            let passed = *passed.lock().unwrap();
            Moment {
                monotonic: start + passed,
                wall: wall_start + passed,
            }
        })
    };
    let invitations = Arc::new(Invitations::new(
        Arc::clone(&registry),
        clock,
        Arc::new(|| Some([7; 32])),
        announce,
    ));
    let listener = Listener::start(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        &identity,
        Arc::clone(&registry),
        Arc::clone(&invitations),
        Arc::new(Echo),
        Limits::default(),
    )
    .unwrap();
    // A device is paired and no phone connects, so nothing reaches the listener but its own loop,
    // which is the only thing that asks the window about itself, and a listener that asked only
    // while no device was paired would say nothing here.
    registry
        .admit(
            [1; 32],
            Device {
                name: "watch".into(),
                grant: Grant::Read,
                workspace: PathBuf::from("/work"),
            },
        )
        .unwrap();
    invitations.open(terms("one", Grant::Read)).unwrap();
    *passed.lock().unwrap() += Duration::from_secs(121);
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until
        && !said
            .lock()
            .unwrap()
            .iter()
            .any(|said| matches!(said, Announcement::Ended { .. }))
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        said.lock()
            .unwrap()
            .iter()
            .any(|said| matches!(said, Announcement::Ended { .. })),
        "the window ran out and nothing said so"
    );
    drop(listener);
}

/// Stopping the listener closes a window that is open and releases a phone waiting for its code.
#[test]
fn stopping_closes_the_window_and_releases_a_phone_waiting_for_its_code() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut connection = rig.connect(&Phone::new().unwrap());
    connection.begin_pairing(&opened.secret, None).unwrap();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.listener.stop();
    assert!(connection.ended());
    assert!(!rig.invitations.is_open());
}

/// Output that a test reads while a command-line phone is still writing it.
#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Kept {
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

    /// The six digits after `code: `, once the phone has written them.
    fn code(&self) -> String {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let text = self.text();
            if let Some((_, after)) = text.split_once("code: ") {
                let digits: String = after.chars().take(6).collect();
                if digits.len() == 6 {
                    return digits;
                }
            }
            assert!(Instant::now() < until, "the phone showed no code: {text:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// A command-line phone run on a thread, with what it reads as its input and the text it wrote.
fn command_line_phone_reading(
    link: String,
    input: impl std::io::BufRead + Send + 'static,
) -> (Kept, std::sync::mpsc::Receiver<std::io::Result<()>>) {
    let kept = Kept::default();
    let (finished, result) = std::sync::mpsc::channel();
    let output = kept.clone();
    std::thread::spawn(move || {
        let mut output = output;
        let done = crate::phone::pair_and_talk(&link, input, &mut output);
        finished.send(done).ok();
    });
    (kept, result)
}

/// A command-line phone run on a thread, with the lines it will send and the text it wrote.
fn command_line_phone(
    link: String,
    lines: &'static str,
) -> (Kept, std::sync::mpsc::Receiver<std::io::Result<()>>) {
    command_line_phone_reading(link, std::io::Cursor::new(lines))
}

/// A command-line phone given more input at once than the sockets between it and the host hold writes
/// every answer, because it reads what the host sends between the lines it sends. One that sent the
/// whole input first would stop reading while the host stopped reading too.
#[test]
fn a_command_line_phone_given_a_large_input_writes_every_answer() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let link = crate::invitation::link(
        &rig.listener.key_hash(),
        &[rig.listener.address()],
        &opened.secret,
    );
    let lines = 300;
    let input: String = (0..lines)
        .map(|n| format!("{n:04}{}\n", "x".repeat(8 * 1024)))
        .collect();
    let (output, finished) = command_line_phone_reading(link, std::io::Cursor::new(input));
    let code = output.code();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.invitations.confirm(&code).unwrap();
    finished
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let text = output.text();
    assert_eq!(text.matches("host: one:").count(), lines, "{}", text.len());
}

/// A command-line phone whose input cannot be read, here a line that is not text, sends what came
/// before it, writes the host's answers to that, and stops with the error. It does not take the error
/// for the end of the input, and sends nothing after it.
#[test]
fn a_command_line_phone_stops_with_an_error_its_input_gives() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let link = crate::invitation::link(
        &rig.listener.key_hash(),
        &[rig.listener.address()],
        &opened.secret,
    );
    let (output, finished) = command_line_phone_reading(
        link,
        std::io::Cursor::new(b"hello\n\xff\xfe\nworld\n".to_vec()),
    );
    let code = output.code();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.invitations.confirm(&code).unwrap();
    let ended = finished.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        ended.map_err(|error| error.kind()),
        Err(std::io::ErrorKind::InvalidData)
    );
    let text = output.text();
    assert!(text.contains("host: one:hello"), "{text}");
    assert!(!text.contains("host: one:world"), "{text}");
}

/// Input a test types into a command-line phone a line at a time, which ends when the test drops its
/// sender, as a person's terminal does when they type nothing for a while and then go on.
struct Typed {
    lines: std::sync::mpsc::Receiver<String>,
    left: Vec<u8>,
}

impl std::io::Read for Typed {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.left.is_empty() {
            match self.lines.recv() {
                Ok(line) => self.left = format!("{line}\n").into_bytes(),
                Err(_) => return Ok(0),
            }
        }
        let taken = self.left.len().min(buffer.len());
        buffer[..taken].copy_from_slice(&self.left[..taken]);
        self.left.drain(..taken);
        Ok(taken)
    }
}

/// A command-line phone whose person types nothing for many times the silence the host allows keeps
/// its connection, because it answers each heartbeat, and is answered when the person goes on. The
/// heartbeats are not written for the person to read.
#[test]
fn a_command_line_phone_answers_heartbeats_while_its_person_types_nothing() {
    let limits = Limits {
        heartbeat: Duration::from_millis(100),
        silence: Duration::from_secs(1),
        ..Limits::default()
    };
    let rig = rig_with(limits);
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let link = crate::invitation::link(
        &rig.listener.key_hash(),
        &[rig.listener.address()],
        &opened.secret,
    );
    let (typing, lines) = std::sync::mpsc::channel();
    let (output, finished) = command_line_phone_reading(
        link,
        std::io::BufReader::new(Typed {
            lines,
            left: Vec::new(),
        }),
    );
    let code = output.code();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    rig.invitations.confirm(&code).unwrap();
    typing.send("hello".to_string()).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !output.text().contains("host: one:hello") {
        assert!(Instant::now() < until, "{}", output.text());
        std::thread::sleep(Duration::from_millis(10));
    }
    // The person typing nothing is what is under test, for three times the silence the host allows.
    std::thread::sleep(limits.silence * 3);
    // A phone that lost its connection has stopped reading its input, and the assertions below say so.
    typing.send("world".to_string()).ok();
    drop(typing);
    finished
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let text = output.text();
    assert!(text.contains("host: one:world"), "{text}");
    assert!(!text.contains("heartbeat"), "{text}");
    assert!(!text.contains("ended"), "{text}");
}

/// The command-line phone pairs from the link the host shows, writes the code for the person to
/// type, and once they have, sends each line it is given and writes what the host answers.
#[test]
fn a_command_line_phone_pairs_from_a_link_and_talks_to_the_host() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let link = crate::invitation::link(
        &rig.listener.key_hash(),
        &[rig.listener.address()],
        &opened.secret,
    );
    let (output, finished) = command_line_phone(link, "hello\n\nworld\n");
    let code = output.code();
    // The phone shows the code as soon as it has sent its proof, and the host may not have read it.
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    assert_eq!(rig.invitations.confirm(&code).unwrap(), "one");
    finished
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let text = output.text();
    assert!(text.contains("paired"), "{text}");
    assert!(text.contains("host: one:hello"), "{text}");
    assert!(text.contains("host: one:world"), "{text}");
    assert_eq!(text.matches("host: one:").count(), 2, "{text}");
}

/// A link that is not an invitation, and an invitation for another host, are refused with a reason
/// and without connecting or showing a code.
#[test]
fn a_command_line_phone_refuses_a_link_that_is_not_the_hosts() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let mut another_host = rig.listener.key_hash();
    another_host[0] ^= 1;
    for link in [
        "not a link".to_string(),
        "bravebot://pair?v=1".to_string(),
        crate::invitation::link(&another_host, &[rig.listener.address()], &opened.secret),
    ] {
        let (output, finished) = command_line_phone(link.clone(), "");
        let result = finished.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(result.is_err(), "{link}");
        assert!(
            !output.text().contains("code: "),
            "{link}: {}",
            output.text()
        );
    }
    assert!(rig.invitations.is_open());
}

/// A host that refuses the code ends the phone's pairing, and the phone says so and stops.
#[test]
fn a_command_line_phone_says_when_the_host_ends_the_pairing() {
    let rig = rig();
    let opened = rig.invitations.open(terms("one", Grant::Read)).unwrap();
    let link = crate::invitation::link(
        &rig.listener.key_hash(),
        &[rig.listener.address()],
        &opened.secret,
    );
    let (output, finished) = command_line_phone(link, "hello\n");
    let code = output.code();
    rig.hears(|said| matches!(said, Announcement::Asking { .. }));
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert!(rig.invitations.confirm(wrong).is_err());
    let result = finished.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(result.is_err());
    assert!(!output.text().contains("host: one:"));
}

/// The command-line phone takes the link as its one argument, and says how to run it otherwise.
#[test]
fn a_command_line_phone_takes_one_argument_that_is_text() {
    use std::ffi::OsString;
    let one = |text: &str| vec![OsString::from(text)].into_iter();
    assert_eq!(
        crate::phone::link_from_arguments(one("bravebot://pair?v=1")),
        Ok("bravebot://pair?v=1".to_string())
    );
    assert!(
        crate::phone::link_from_arguments(Vec::new().into_iter())
            .unwrap_err()
            .contains("usage")
    );
    let two = vec![OsString::from("a"), OsString::from("b")];
    assert!(
        crate::phone::link_from_arguments(two.into_iter())
            .unwrap_err()
            .contains("usage")
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let not_text = vec![OsString::from_vec(vec![0xff, 0xfe])];
        assert!(
            crate::phone::link_from_arguments(not_text.into_iter())
                .unwrap_err()
                .contains("text")
        );
    }
}
