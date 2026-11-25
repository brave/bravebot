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
use crate::server::{Listener, Service};
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
    fn answer(&self, _device: &Device, line: &str) -> Vec<String> {
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

/// A request of the device that the host has read but not yet run when the person revokes it does
/// not run. The revocation waits for the request being answered, which finishes, and the next one
/// is refused.
#[test]
fn a_request_not_yet_run_when_the_device_is_revoked_does_not_run() {
    let rig = rig();
    let (phone, other) = (Phone::new().unwrap(), Phone::new().unwrap());
    pair(&rig, &phone, "kitchen-phone");
    pair(&rig, &other, "watch");
    let mut connection = phone
        .connect(rig.listener.address(), rig.listener.key_hash())
        .unwrap();
    connection.send("hold").unwrap();
    rig.entered.recv_timeout(Duration::from_secs(5)).unwrap();
    // The answer to the first request is being made. The person revokes the device, which waits for
    // it, and the device has a second request waiting on the connection.
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
    connection.send("after").unwrap();
    rig.release.send(()).unwrap();
    hear_revoked.recv_timeout(Duration::from_secs(5)).unwrap();
    revoking.join().unwrap();
    // The answer to the first request may reach the phone before the connection ends or not, and the
    // second is never answered.
    let mut answers = Vec::new();
    while let Some(line) = connection.receive().unwrap() {
        answers.push(line);
    }
    assert!(!answers.contains(&"heard:after".to_string()), "{answers:?}");
    assert_eq!(rig.service.heard.lock().unwrap().as_slice(), ["hold"]);
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
