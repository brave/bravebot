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
use bravebot_core::device::{Grant, Standing};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

struct Echo;

impl Service for Echo {
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        Box::new(move |device| vec![format!("{}:{line}", device.name)])
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
