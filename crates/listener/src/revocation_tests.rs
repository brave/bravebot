// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A device the person revokes on the host: its connection ends at once, it is not let back, and a
//! request of it that the host had not yet run does not run.

use crate::identity::Identity;
use crate::invitation::{Invitations, Terms};
use crate::limits::Limits;
use crate::phone::Phone;
use crate::registry::{Device, NoSuchDevice, Registry};
use crate::server::{Listener, Prepared, Service};
use bravebot_core::device::{Grant, Standing};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Records each line it answers, and holds the answer to the line `hold` until it is told to go.
struct Held {
    heard: Mutex<Vec<String>>,
    entered: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

impl Service for Held {
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        Box::new(move |_device| {
            self.heard.lock().unwrap().push(line.to_string());
            if line == "hold" {
                self.entered.lock().unwrap().send(()).unwrap();
                // Bounded, so a test that never releases fails and does not hang.
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
            vec![format!("heard:{line}")]
        })
    }
}

struct Rig {
    listener: Listener,
    registry: Arc<Registry>,
    invitations: Arc<Invitations>,
    service: Arc<Held>,
    entered: Receiver<()>,
    release: Sender<()>,
}

fn rig() -> Rig {
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let invitations = Arc::new(Invitations::system(Arc::clone(&registry), Arc::new(|_| {})));
    let (entered_sender, entered) = mpsc::channel();
    let (release, release_receiver) = mpsc::channel();
    let service = Arc::new(Held {
        heard: Mutex::new(Vec::new()),
        entered: Mutex::new(entered_sender),
        release: Mutex::new(release_receiver),
    });
    let listener = Listener::start(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        &identity,
        Arc::clone(&registry),
        Arc::clone(&invitations),
        service.clone(),
        Limits::default(),
    )
    .unwrap();
    Rig {
        listener,
        registry,
        invitations,
        service,
        entered,
        release,
    }
}

fn pair(rig: &Rig, phone: &Phone, name: &str) {
    rig.registry
        .admit(
            phone.key_hash(),
            Device {
                name: name.into(),
                grant: Grant::Control,
                workspace: PathBuf::from("/work"),
            },
        )
        .unwrap();
}

/// Revoking ends the device's connection at once, and the host holds it as revoked and not as a
/// device nobody paired.
#[test]
fn revoking_a_device_ends_its_connection_and_removes_it() {
    let rig = rig();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    pair(&rig, &phone, "kitchen-phone");
    pair(&rig, &other, "watch");
    let mut connection = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    connection.send("hello").unwrap();
    assert_eq!(
        connection.receive().unwrap().as_deref(),
        Some("heard:hello")
    );
    assert!(rig.registry.is_connected(&phone.key_hash()));
    let removed = rig.registry.revoke("kitchen-phone").unwrap();
    assert_eq!(removed.name, "kitchen-phone");
    assert!(connection.ended());
    assert_eq!(rig.registry.standing(&phone.key_hash()), Standing::Revoked);
    assert_eq!(
        rig.registry.standing(&other.key_hash()),
        Standing::Paired(Grant::Control)
    );
    assert!(!rig.registry.is_connected(&phone.key_hash()));
    let names: Vec<_> = rig
        .registry
        .devices()
        .into_iter()
        .map(|(_, d)| d.name)
        .collect();
    assert_eq!(names, ["watch"]);
}

/// A device that is revoked, or that the host never heard of, cannot be revoked.
#[test]
fn a_name_no_device_has_is_not_revoked() {
    let rig = rig();
    assert_eq!(rig.registry.revoke("nobody"), Err(NoSuchDevice));
    let phone = Phone::new().unwrap();
    pair(&rig, &phone, "one");
    rig.registry.revoke("one").unwrap();
    assert_eq!(rig.registry.revoke("one"), Err(NoSuchDevice));
}

/// A revoked device that connects again completes the handshake, since that is how the host learns
/// who it is, and is then closed with nothing sent. Its handshake is a full one: a connection of it
/// is not a session kept from before.
#[test]
fn a_revoked_device_is_refused_when_it_comes_back() {
    let rig = rig();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    pair(&rig, &phone, "kitchen-phone");
    pair(&rig, &other, "watch");
    let mut first = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    first.send("hello").unwrap();
    assert_eq!(first.receive().unwrap().as_deref(), Some("heard:hello"));
    rig.registry.revoke("kitchen-phone").unwrap();
    assert!(first.ended());
    let mut again = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    assert!(again.was_a_full_handshake());
    again.send("let me back in").ok();
    assert!(again.ended());
    assert_eq!(rig.service.heard.lock().unwrap().as_slice(), ["hello"]);
}

/// Revoking waits for the request being answered, which finishes, and then removes the device and
/// ends its connection: the person's command does not cut an answer in two.
#[test]
fn revoking_waits_for_the_request_being_answered_and_then_removes_the_device() {
    let rig = rig();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    pair(&rig, &phone, "kitchen-phone");
    pair(&rig, &other, "watch");
    let mut connection = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    connection.send("hold").unwrap();
    rig.entered.recv_timeout(Duration::from_secs(5)).unwrap();
    // The answer to the request is being made. The person revokes the device, which waits for it.
    let (revoked, hear_revoked) = mpsc::channel();
    let revoking = {
        let registry = Arc::clone(&rig.registry);
        std::thread::spawn(move || {
            registry.revoke("kitchen-phone").unwrap();
            revoked.send(()).unwrap();
        })
    };
    assert!(
        hear_revoked
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "the device was revoked while a request of it was being answered"
    );
    rig.release.send(()).unwrap();
    hear_revoked.recv_timeout(Duration::from_secs(5)).unwrap();
    revoking.join().unwrap();
    // The answer may reach the phone before the connection ends or not.
    while connection.receive().unwrap().is_some() {}
    assert_eq!(rig.service.heard.lock().unwrap().as_slice(), ["hold"]);
    assert_eq!(rig.registry.standing(&phone.key_hash()), Standing::Revoked);
}

/// A person can pair a device they revoked, with a new window, and its name is free again.
#[test]
fn a_revoked_device_can_be_paired_again_by_the_person() {
    let rig = rig();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    pair(&rig, &phone, "kitchen-phone");
    pair(&rig, &other, "watch");
    rig.registry.revoke("kitchen-phone").unwrap();
    let opened = rig
        .invitations
        .open(Terms {
            name: "kitchen-phone".into(),
            grant: Grant::Read,
            workspace: PathBuf::from("/work"),
        })
        .unwrap();
    let mut connection = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    let code = connection.begin_pairing(&opened.secret, None).unwrap();
    // The host has the phone's proof once it has read it, which is a moment after the phone sent it.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let mut confirmed = rig.invitations.confirm(&code);
    while confirmed.is_err() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
        confirmed = rig.invitations.confirm(&code);
    }
    let name = confirmed.unwrap();
    assert_eq!(name, "kitchen-phone");
    assert_eq!(
        rig.registry.standing(&phone.key_hash()),
        Standing::Paired(Grant::Read)
    );
    assert!(connection.receive().unwrap().is_some());
}

/// Reads each line it is given before the registry's lock is taken, and holds the reading of the
/// line `slow` until it is told to go.
struct Slow {
    reading: Mutex<Sender<()>>,
    go: Mutex<Receiver<()>>,
    answered: Mutex<Vec<String>>,
    decided: Mutex<Sender<()>>,
}

/// Says that what answers a line has been run or let go, which is when the host has decided about it.
struct Decided(Sender<()>);

impl Drop for Decided {
    fn drop(&mut self) {
        self.0.send(()).ok();
    }
}

impl Service for Slow {
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        if line == "slow" {
            self.reading.lock().unwrap().send(()).unwrap();
            // Bounded, so a test that never lets it go fails and does not hang.
            self.go
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
        let decided = Decided(self.decided.lock().unwrap().clone());
        Box::new(move |_| {
            let _decided = &decided;
            self.answered.lock().unwrap().push(line.to_string());
            vec![format!("heard:{line}")]
        })
    }
}

/// A line is read before the lock is taken, so a line that takes a long time to read does not make
/// the person wait to revoke the device, and a device revoked while its line is read is not
/// answered when the reading is done: the line was read and had not run when the device was revoked,
/// and it does not run.
#[test]
fn a_line_is_read_before_the_lock_so_revoking_does_not_wait_for_it() {
    let identity = Identity::generate("bravebot-host").unwrap();
    let registry = Arc::new(Registry::new());
    let (reading, hear_reading) = mpsc::channel();
    let (go_sender, go) = mpsc::channel();
    let (decided_sender, decided) = mpsc::channel();
    let service = Arc::new(Slow {
        reading: Mutex::new(reading),
        go: Mutex::new(go),
        answered: Mutex::new(Vec::new()),
        decided: Mutex::new(decided_sender),
    });
    let listener = Listener::start(
        "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        &identity,
        Arc::clone(&registry),
        Arc::new(Invitations::system(Arc::clone(&registry), Arc::new(|_| {}))),
        service.clone(),
        Limits::default(),
    )
    .unwrap();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    for (phone, name) in [(&phone, "kitchen-phone"), (&other, "watch")] {
        registry
            .admit(
                phone.key_hash(),
                Device {
                    name: name.into(),
                    grant: Grant::Control,
                    workspace: PathBuf::from("/work"),
                },
            )
            .unwrap();
    }
    let mut connection = phone
        .connect(listener.address(), listener.key_hash())
        .unwrap();
    connection.send("slow").unwrap();
    hear_reading.recv_timeout(Duration::from_secs(5)).unwrap();
    // The line is being read, with no lock held. Revoking does not wait for it.
    let (revoked, hear_revoked) = mpsc::channel();
    let revoking = {
        let registry = Arc::clone(&registry);
        std::thread::spawn(move || {
            registry.revoke("kitchen-phone").unwrap();
            revoked.send(()).unwrap();
        })
    };
    // Revoking that waited for the line being read would not be done in two seconds, and fails here.
    hear_revoked.recv_timeout(Duration::from_secs(2)).unwrap();
    revoking.join().unwrap();
    go_sender.send(()).unwrap();
    // The host has decided about the line once what answers it is run or let go, and it was let go.
    decided.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(connection.ended());
    assert!(service.answered.lock().unwrap().is_empty());
}
