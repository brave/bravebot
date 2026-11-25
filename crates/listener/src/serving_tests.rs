// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A listener with a phone connecting to it over a real socket on loopback.

use crate::identity::Identity;
use crate::invitation::Invitations;
use crate::limits::Limits;
use crate::phone::Phone;
use crate::registry::{Device, Registry};
use crate::server::{Listener, Prepared, Service};
use bravebot_core::device::Grant;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Records each line and the device it came from, and answers with the line reversed in a fixed
/// frame, so a test sees both what the service was given and what the phone was sent.
#[derive(Default)]
struct Recorder {
    heard: Mutex<Vec<(String, String)>>,
}

impl Service for Recorder {
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        Box::new(move |device| {
            self.heard
                .lock()
                .unwrap()
                .push((device.name.clone(), line.to_string()));
            vec![format!("heard:{line}"), "second".to_string()]
        })
    }
}

impl Recorder {
    fn heard(&self) -> Vec<(String, String)> {
        self.heard.lock().unwrap().clone()
    }
}

struct Host {
    listener: Listener,
    registry: Arc<Registry>,
    service: Arc<Recorder>,
}

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// A listener with no device paired and no window open, which accepts nobody.
fn empty_host_with(limits: Limits) -> Host {
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let invitations = Arc::new(Invitations::system(Arc::clone(&registry), Arc::new(|_| {})));
    let service = Arc::new(Recorder::default());
    let listener = Listener::start(
        loopback(),
        &identity,
        Arc::clone(&registry),
        Arc::clone(&invitations),
        service.clone(),
        limits,
    )
    .unwrap();
    Host {
        listener,
        registry,
        service,
    }
}

/// A listener with one device paired, which is not one of the phones a test connects, so what a
/// test shows is what happens to a connection once the listener is accepting.
fn host_with(limits: Limits) -> Host {
    let host = empty_host_with(limits);
    pair(&host, &Phone::new().unwrap(), "placeholder", Grant::Read);
    host
}

fn host() -> Host {
    host_with(Limits::default())
}

fn pair(host: &Host, phone: &Phone, name: &str, grant: Grant) {
    host.registry
        .admit(
            phone.key_hash(),
            Device {
                name: name.into(),
                grant,
                workspace: PathBuf::from("/work"),
            },
        )
        .unwrap();
}

/// Wait until `done` holds, up to a few seconds, so a broken listener fails a test and does not
/// hang it.
fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(done(), "{what}");
}

/// Whether the host closed `stream` and sent nothing: the read ends or fails, and does not time out
/// and does not return a byte.
fn closed_with_nothing_sent(stream: &mut TcpStream) -> bool {
    let mut byte = [0u8; 1];
    match stream.read(&mut byte) {
        Ok(read) => read == 0,
        Err(error) => {
            error.kind() != std::io::ErrorKind::WouldBlock
                && error.kind() != std::io::ErrorKind::TimedOut
        }
    }
}

/// A paired phone is answered line by line over TLS 1.3 with a full handshake, and the service is
/// given the device the person paired.
#[test]
fn a_paired_phone_is_answered_and_the_service_is_given_its_record() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let mut connection = phone
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    assert_eq!(
        connection.protocol(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert!(connection.was_a_full_handshake());
    connection.send("one").unwrap();
    assert_eq!(connection.receive().unwrap().as_deref(), Some("heard:one"));
    assert_eq!(connection.receive().unwrap().as_deref(), Some("second"));
    connection.send("two").unwrap();
    assert_eq!(connection.receive().unwrap().as_deref(), Some("heard:two"));
    assert_eq!(
        host.service.heard(),
        [
            ("kitchen-phone".to_string(), "one".to_string()),
            ("kitchen-phone".to_string(), "two".to_string())
        ]
    );
}

/// A phone told to expect another key refuses the host before it sends anything, so a program that
/// took the address cannot be talked to as the host.
#[test]
fn a_phone_pinned_to_another_key_refuses_the_host() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let mut other = host.listener.key_hash();
    other[0] ^= 0x01;
    assert!(phone.connect(host.listener.address(), other).is_err());
    assert!(host.service.heard().is_empty());
}

/// A key nobody paired finishes the handshake, since that is how the host learns the key, and is
/// then closed with nothing sent. What it writes reaches no service.
#[test]
fn an_unknown_phone_is_closed_with_nothing_sent_and_reaches_no_service() {
    let host = host();
    let stranger = Phone::new().unwrap();
    let mut connection = stranger
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    // The write may succeed or fail: the host has closed the connection either way.
    connection.send("anything").ok();
    assert_eq!(connection.receive().unwrap(), None);
    assert!(host.service.heard().is_empty());
}

/// A phone that shows no certificate is not asked what it wants: the handshake does not complete.
#[test]
fn a_phone_that_shows_no_certificate_reaches_no_service() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    if let Ok(mut connection) =
        phone.connect_without_certificate(host.listener.address(), host.listener.key_hash())
    {
        connection.send("anything").ok();
        assert_eq!(connection.receive().unwrap(), None);
    }
    assert!(host.service.heard().is_empty());
}

/// Every connection is a full handshake, so a device that was paired is looked up again each time,
/// and a phone that kept what a resumed session needs is not resumed.
#[test]
fn a_second_connection_is_a_full_handshake_and_not_a_resumed_session() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    for _ in 0..3 {
        let mut connection = phone
            .connect(host.listener.address(), host.listener.key_hash())
            .unwrap();
        assert!(connection.was_a_full_handshake());
        connection.send("hello").unwrap();
        assert_eq!(
            connection.receive().unwrap().as_deref(),
            Some("heard:hello")
        );
    }
}

/// A device has one connection: the next one it makes ends the first.
#[test]
fn a_new_connection_of_a_device_ends_its_old_one() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let mut first = phone
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    first.send("a").unwrap();
    assert_eq!(first.receive().unwrap().as_deref(), Some("heard:a"));
    assert_eq!(first.receive().unwrap().as_deref(), Some("second"));
    let mut second = phone
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    second.send("b").unwrap();
    assert_eq!(second.receive().unwrap().as_deref(), Some("heard:b"));
    assert!(first.ended());
}

/// A line that reaches the limit without ending ends the connection and is not answered. A line of
/// exactly the limit is read.
#[test]
fn a_line_past_the_limit_ends_the_connection_and_one_at_it_is_read() {
    let host = host_with(Limits {
        line: 64,
        ..Limits::default()
    });
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let mut connection = phone
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    let at_limit = "x".repeat(64);
    connection.send(&at_limit).unwrap();
    assert_eq!(
        connection.receive().unwrap(),
        Some(format!("heard:{at_limit}"))
    );
    assert_eq!(connection.receive().unwrap().as_deref(), Some("second"));
    let past = "y".repeat(65);
    connection.send(&past).ok();
    assert!(connection.ended());
    assert_eq!(host.service.heard().len(), 1);
}

/// Bytes that are not text are not a request, and a blank line is skipped.
#[test]
fn a_line_that_is_not_text_ends_the_connection_and_a_blank_one_is_skipped() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let mut connection = phone
        .connect(host.listener.address(), host.listener.key_hash())
        .unwrap();
    connection.send("").unwrap();
    connection.send("after-blank").unwrap();
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some("heard:after-blank")
    );
    assert_eq!(connection.receive().unwrap().as_deref(), Some("second"));
    connection.send_bytes(&[0xff, 0xfe, b'\n']).unwrap();
    assert!(connection.ended());
    assert_eq!(host.service.heard().len(), 1);
}

/// An address already in use is reported by the call that starts the listener, so a host asked to
/// listen where it cannot says so.
#[test]
fn an_address_already_in_use_is_reported() {
    let taken = host();
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let error = Listener::start(
        taken.listener.address(),
        &identity,
        Arc::clone(&registry),
        Arc::new(Invitations::system(registry, Arc::new(|_| {}))),
        Arc::new(Recorder::default()),
        Limits::default(),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
}

/// Connections that never say anything fill the places a handshake may use, the ones past them are
/// closed as they arrive, and a phone connects again once they are gone.
#[test]
fn a_flood_of_silent_connections_is_closed_past_the_limit_and_does_not_keep_a_phone_out() {
    let host = host_with(Limits {
        connections: 8,
        handshakes: 2,
        handshake: Duration::from_secs(30),
        ..Limits::default()
    });
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let address = host.listener.address();
    let held: Vec<TcpStream> = (0..2)
        .map(|_| TcpStream::connect(address).unwrap())
        .collect();
    // The listener accepts in the order connections arrive, and takes a place for each as it
    // accepts, so these two have the places before any connection below is looked at.
    for _ in 0..5 {
        let mut past = TcpStream::connect(address).unwrap();
        past.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        // Closed by the host: end of file or a reset, and not a timeout.
        assert!(
            closed_with_nothing_sent(&mut past),
            "a connection past the limit was kept or sent a byte"
        );
    }
    drop(held);
    eventually(
        "a phone connects once the silent connections are gone",
        || {
            phone
                .connect(address, host.listener.key_hash())
                .and_then(|mut connection| {
                    connection.send("hello")?;
                    connection.receive()
                })
                .is_ok_and(|answer| answer.as_deref() == Some("heard:hello"))
        },
    );
}

/// Connections that are paired devices count against the limit of open connections too, and one
/// past it is closed as it arrives. It is let in once a place is free.
#[test]
fn a_connection_past_the_limit_of_open_connections_is_closed_as_it_arrives() {
    let host = host_with(Limits {
        connections: 2,
        ..Limits::default()
    });
    let address = host.listener.address();
    let key = host.listener.key_hash();
    let phones: Vec<Phone> = (0..3).map(|_| Phone::new().unwrap()).collect();
    for (number, phone) in phones.iter().enumerate() {
        pair(&host, phone, &format!("phone-{number}"), Grant::Read);
    }
    let mut first = phones[0].connect(address, key).unwrap();
    let mut second = phones[1].connect(address, key).unwrap();
    for connection in [&mut first, &mut second] {
        connection.send("hello").unwrap();
        assert_eq!(
            connection.receive().unwrap().as_deref(),
            Some("heard:hello")
        );
    }
    assert!(phones[2].connect(address, key).is_err());
    drop(first);
    eventually("a place is free once a connection is gone", || {
        phones[2].connect(address, key).is_ok()
    });
}

/// A connection that does not finish its handshake in time is closed.
#[test]
fn a_stalled_handshake_is_closed_when_its_time_is_up() {
    let host = host_with(Limits {
        handshake: Duration::from_millis(300),
        ..Limits::default()
    });
    let mut stalled = TcpStream::connect(host.listener.address()).unwrap();
    // Half of a record header, then nothing.
    stalled.write_all(&[0x16, 0x03]).unwrap();
    stalled
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let started = Instant::now();
    assert!(
        closed_with_nothing_sent(&mut stalled),
        "a stalled connection was kept or sent a byte"
    );
    assert!(started.elapsed() < Duration::from_secs(4));
}

/// The time a handshake is given is counted from the moment the connection is accepted, so a
/// connection that sends a byte now and then is closed when the time is up and does not hold a place
/// for as long as it keeps sending.
#[test]
fn a_handshake_that_trickles_bytes_is_closed_when_its_time_is_up() {
    let host = host_with(Limits {
        handshake: Duration::from_millis(400),
        ..Limits::default()
    });
    let mut slow = TcpStream::connect(host.listener.address()).unwrap();
    slow.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    // The start of a handshake record that says it holds 200 bytes, and then one byte at a time.
    slow.write_all(&[0x16, 0x03, 0x01, 0x00, 0xc8]).unwrap();
    let started = Instant::now();
    let mut closed = false;
    while !closed && started.elapsed() < Duration::from_secs(4) {
        closed = slow.write_all(&[0x01]).is_err();
        let mut byte = [0u8; 1];
        closed |= match slow.read(&mut byte) {
            Ok(read) => read == 0,
            Err(error) => !matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
        };
    }
    assert!(closed, "a connection that trickled bytes was never closed");
    assert!(started.elapsed() < Duration::from_secs(3));
}

/// Stopping the listener ends a connection that has not finished its handshake, and does not leave
/// it to run out its own time. The listener has one place for a handshake, and a second connection
/// is closed as it arrives only while the first holds it, which is how the test knows the first is
/// in its handshake before it stops the listener.
#[test]
fn stopping_ends_a_connection_still_in_its_handshake() {
    let host = host_with(Limits {
        handshake: Duration::from_secs(60),
        handshakes: 1,
        ..Limits::default()
    });
    let mut stalled = TcpStream::connect(host.listener.address()).unwrap();
    stalled.write_all(&[0x16, 0x03]).unwrap();
    stalled
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    eventually(
        "the stalled connection holds the place for a handshake",
        || {
            let mut probe = TcpStream::connect(host.listener.address()).unwrap();
            probe
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            closed_with_nothing_sent(&mut probe)
        },
    );
    host.listener.stop();
    assert!(
        closed_with_nothing_sent(&mut stalled),
        "a connection in its handshake outlived the listener"
    );
}

/// A paired device that stops reading while it is sent more than the connection holds has its
/// connection ended when a write makes no progress for the time the limit gives, and the thread that
/// was writing to it is let go.
#[test]
fn a_device_that_stops_reading_has_its_connection_ended_when_a_write_makes_no_progress() {
    struct Big;
    impl Service for Big {
        fn prepare<'a>(&'a self, _line: &'a str) -> Prepared<'a> {
            Box::new(|_| vec!["x".repeat(48 << 20)])
        }
    }
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let listener = Listener::start(
        loopback(),
        &identity,
        Arc::clone(&registry),
        Arc::new(Invitations::system(Arc::clone(&registry), Arc::new(|_| {}))),
        Arc::new(Big),
        Limits {
            write: Duration::from_millis(300),
            ..Limits::default()
        },
    )
    .unwrap();
    let phone = Phone::new().unwrap();
    registry
        .admit(
            phone.key_hash(),
            Device {
                name: "kitchen-phone".into(),
                grant: Grant::Read,
                workspace: PathBuf::from("/work"),
            },
        )
        .unwrap();
    let mut connection = phone
        .connect(listener.address(), listener.key_hash())
        .unwrap();
    eventually("the host holds the connection", || {
        registry.is_connected(&phone.key_hash())
    });
    connection.send("send it all").unwrap();
    // The phone does not read. The host gives up on the write, which ends the connection.
    eventually("the host gave up on a device that reads nothing", || {
        !registry.is_connected(&phone.key_hash())
    });
}

/// Stopping the listener ends the connections it holds and accepts no more.
#[test]
fn stopping_ends_every_connection_and_accepts_no_more() {
    let host = host();
    let phone = Phone::new().unwrap();
    pair(&host, &phone, "kitchen-phone", Grant::Read);
    let address = host.listener.address();
    let key = host.listener.key_hash();
    let mut connection = phone.connect(address, key).unwrap();
    connection.send("hello").unwrap();
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some("heard:hello")
    );
    assert_eq!(connection.receive().unwrap().as_deref(), Some("second"));
    host.listener.stop();
    assert!(connection.ended());
    assert!(phone.connect(address, key).is_err());
}
