// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The devices the person has paired, and the connection each has open.
//!
//! The list is in this process's memory and nowhere else (decision 13 in
//! `docs/design/mobile/decisions-and-tradeoffs.md`). Nothing about a device comes from a file a tool
//! could write, and a host that restarts has no devices. Everything here is decided from what the
//! person did on the host.
//!
//! A device has one connection. A new connection from a device ends the one it had.

use crate::identity::KeyHash;
use bravebot_core::device::{Grant, Operation, Standing, Verdict};
use std::collections::{HashMap, HashSet};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// A device as the person paired it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// What the person calls it, typed on the host: 1 to 32 of `a-z`, `0-9` and `-`.
    pub name: String,
    pub grant: Grant,
    /// The one workspace the grant covers.
    pub workspace: PathBuf,
}

/// Why a device was not added.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The name is not 1 to 32 of `a-z`, `0-9` and `-`.
    BadName,
    /// Another device has the name.
    NameTaken,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadName => "a device name is 1 to 32 of a-z, 0-9 and -",
            Self::NameTaken => "another device has that name",
        })
    }
}

/// No device has the name.
#[derive(Debug, PartialEq, Eq)]
pub struct NoSuchDevice;

impl std::fmt::Display for NoSuchDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no device has that name")
    }
}

/// Whether `name` is one a person may give a device or a client: 1 to 32 of `a-z`, `0-9` and `-`.
/// The host's `--client NAME` uses this function too.
pub fn is_a_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

struct Live {
    id: u64,
    stream: TcpStream,
}

#[derive(Default)]
struct Inner {
    devices: HashMap<KeyHash, Device>,
    /// Keys of devices the person removed, until the person pairs one of them again.
    revoked: HashSet<KeyHash>,
    live: HashMap<KeyHash, Live>,
    connections: u64,
}

/// The devices and their connections, shared by the listener's threads and the host's input.
#[derive(Default)]
pub struct Registry {
    inner: Mutex<Inner>,
}

/// One connection a device has open, which ends when the device's next connection replaces it.
#[derive(Debug, PartialEq, Eq)]
pub struct Connection(u64);

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while the lock was held leaves a list that is still whole: each change below is
        // one insert or remove.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the host holds about the device with this key.
    pub fn standing(&self, key: &KeyHash) -> Standing {
        Self::standing_in(&self.lock(), key)
    }

    fn standing_in(inner: &Inner, key: &KeyHash) -> Standing {
        match inner.devices.get(key) {
            Some(device) => Standing::Paired(device.grant),
            None if inner.revoked.contains(key) => Standing::Revoked,
            None => Standing::Unknown,
        }
    }

    /// Add a device the person has paired. A key already held is replaced.
    pub fn admit(&self, key: KeyHash, device: Device) -> Result<(), Refused> {
        if !is_a_name(&device.name) {
            return Err(Refused::BadName);
        }
        let mut inner = self.lock();
        if inner
            .devices
            .iter()
            .any(|(held, other)| *held != key && other.name == device.name)
        {
            return Err(Refused::NameTaken);
        }
        inner.revoked.remove(&key);
        inner.devices.insert(key, device);
        Ok(())
    }

    /// Remove the device named `name` and end its connection, in one hold of the lock, so a request
    /// of the device that is being answered finishes first and no request of it runs after.
    pub fn revoke(&self, name: &str) -> Result<Device, NoSuchDevice> {
        let mut inner = self.lock();
        let key = inner
            .devices
            .iter()
            .find_map(|(key, device)| (device.name == name).then_some(*key))
            .ok_or(NoSuchDevice)?;
        let device = inner.devices.remove(&key).ok_or(NoSuchDevice)?;
        inner.revoked.insert(key);
        if let Some(live) = inner.live.remove(&key) {
            // Ending a connection that already ended is not a failure.
            live.stream.shutdown(Shutdown::Both).ok();
        }
        Ok(device)
    }

    /// The devices held, by name.
    pub fn devices(&self) -> Vec<(KeyHash, Device)> {
        let mut devices: Vec<_> = self
            .lock()
            .devices
            .iter()
            .map(|(key, device)| (*key, device.clone()))
            .collect();
        devices.sort_by(|(_, first), (_, second)| first.name.cmp(&second.name));
        devices
    }

    /// Whether no device is held.
    pub fn is_empty(&self) -> bool {
        self.lock().devices.is_empty()
    }

    /// Whether the device with this key has a connection open.
    pub fn is_connected(&self, key: &KeyHash) -> bool {
        self.lock().live.contains_key(key)
    }

    /// Record `stream` as the connection of the device with this key, ending the one it had.
    /// `None` where the device is not paired.
    pub(crate) fn connect(&self, key: &KeyHash, stream: TcpStream) -> Option<Connection> {
        let mut inner = self.lock();
        if !matches!(Self::standing_in(&inner, key), Standing::Paired(_)) {
            return None;
        }
        inner.connections += 1;
        let id = inner.connections;
        if let Some(earlier) = inner.live.insert(*key, Live { id, stream }) {
            // Ending a connection that already ended is not a failure.
            earlier.stream.shutdown(Shutdown::Both).ok();
        }
        Some(Connection(id))
    }

    /// The connection has ended. A newer one of the same device is left as it is.
    pub(crate) fn disconnect(&self, key: &KeyHash, connection: &Connection) {
        let mut inner = self.lock();
        if inner
            .live
            .get(key)
            .is_some_and(|live| live.id == connection.0)
        {
            inner.live.remove(key);
        }
    }

    /// End every connection, as the host does when it stops.
    pub fn end_every_connection(&self) {
        let mut inner = self.lock();
        for (_, live) in inner.live.drain() {
            // Ending a connection that already ended is not a failure.
            live.stream.shutdown(Shutdown::Both).ok();
        }
    }

    /// Run `answer` for the device with this key if, at this moment, it may do `operation`.
    ///
    /// The check and the answer are made under one lock, so a request is either answered while the
    /// device is held with a grant that covers it or refused, and nothing the person does on the host
    /// falls between the two. `answer` is kept short because the host's input waits on the same lock.
    pub(crate) fn answer<T>(
        &self,
        key: &KeyHash,
        operation: Operation,
        answer: impl FnOnce(&Device) -> T,
    ) -> Result<T, Verdict> {
        let inner = self.lock();
        let verdict = Self::standing_in(&inner, key).authorizes(operation);
        match (verdict, inner.devices.get(key)) {
            (Verdict::Allowed, Some(device)) => Ok(answer(device)),
            // The standing was read from this same list, so this is not reached; if it were, the
            // request is refused.
            (Verdict::Allowed, None) => Err(Verdict::NotPaired),
            (refused, _) => Err(refused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn device(name: &str, grant: Grant) -> Device {
        Device {
            name: name.into(),
            grant,
            workspace: PathBuf::from("/work"),
        }
    }

    /// A name is the same words a client is named with, so nothing a person types as one needs
    /// escaping where the host prints it.
    #[test]
    fn a_name_is_one_to_thirty_two_of_lower_case_digits_and_hyphens() {
        for good in [
            "a",
            "kitchen-phone",
            "0",
            "a-name-that-is-thirty-two-chars0",
        ] {
            assert!(is_a_name(good), "{good}");
        }
        for bad in [
            "",
            "A",
            "a b",
            "a_b",
            "a.b",
            "a\u{1b}[31m",
            "tab\t",
            "é",
            "a-name-that-is-thirty-three-chars",
        ] {
            assert!(!is_a_name(bad), "{bad:?}");
        }
    }

    /// The registry refuses a name that is not one and a name another device has, and a device that
    /// is paired again under its own name replaces its record.
    #[test]
    fn a_device_is_added_under_a_name_nobody_else_has() {
        let registry = Registry::new();
        assert_eq!(
            registry.admit([1; 32], device("Bad Name", Grant::Read)),
            Err(Refused::BadName)
        );
        registry
            .admit([1; 32], device("phone", Grant::Read))
            .unwrap();
        assert_eq!(
            registry.admit([2; 32], device("phone", Grant::Control)),
            Err(Refused::NameTaken)
        );
        assert_eq!(registry.standing(&[2; 32]), Standing::Unknown);
        registry
            .admit([1; 32], device("phone", Grant::Control))
            .unwrap();
        assert_eq!(
            registry.standing(&[1; 32]),
            Standing::Paired(Grant::Control)
        );
        assert_eq!(registry.devices().len(), 1);
    }

    #[test]
    fn the_devices_are_listed_by_name() {
        let registry = Registry::new();
        registry
            .admit([1; 32], device("watch", Grant::Read))
            .unwrap();
        registry
            .admit([2; 32], device("phone", Grant::Read))
            .unwrap();
        let names: Vec<_> = registry
            .devices()
            .into_iter()
            .map(|(_, d)| d.name)
            .collect();
        assert_eq!(names, ["phone", "watch"]);
    }

    /// A request is answered only for a device the registry holds, and the answer is not computed
    /// for one it does not.
    #[test]
    fn a_request_is_answered_for_a_held_device_and_not_computed_for_any_other() {
        let registry = Registry::new();
        registry
            .admit([1; 32], device("phone", Grant::Read))
            .unwrap();
        let mut ran = false;
        let held = registry.answer(&[1; 32], Operation::Read, |device| {
            ran = true;
            device.name.clone()
        });
        assert_eq!(held, Ok("phone".to_string()));
        assert!(ran);
        let mut ran = false;
        let stranger = registry.answer(&[9; 32], Operation::Read, |_| {
            ran = true;
        });
        assert_eq!(stranger, Err(Verdict::NotPaired));
        assert!(!ran);
        let too_much = registry.answer(&[1; 32], Operation::Control, |_| {
            ran = true;
        });
        assert_eq!(too_much, Err(Verdict::GrantTooNarrow));
        assert!(!ran);
    }

    /// The check and the answer are one hold of the lock, so what the person does on the host falls
    /// before a request or after it and never between.
    #[test]
    fn a_change_on_the_host_waits_for_the_request_being_answered() {
        use std::sync::mpsc;
        use std::time::Duration;
        let registry = Arc::new(Registry::new());
        registry
            .admit([1; 32], device("phone", Grant::Control))
            .unwrap();
        let (started, hear_started) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let answering = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                registry
                    .answer(&[1; 32], Operation::Control, |_| {
                        started.send(()).unwrap();
                        // Bounded, so an implementation that never lets go fails the test.
                        released.recv_timeout(Duration::from_secs(5)).unwrap();
                    })
                    .unwrap();
            })
        };
        hear_started.recv_timeout(Duration::from_secs(5)).unwrap();
        let (changed, hear_changed) = mpsc::channel();
        let changing = {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                registry
                    .admit([1; 32], device("phone", Grant::Read))
                    .unwrap();
                changed.send(()).unwrap();
            })
        };
        // A thread blocked on the registry's lock gives no event to wait for. This is the time in which
        // an `admit` that did not wait for the lock would finish.
        assert!(
            hear_changed
                .recv_timeout(Duration::from_millis(300))
                .is_err(),
            "the change was made while the request was being answered"
        );
        release.send(()).unwrap();
        hear_changed.recv_timeout(Duration::from_secs(5)).unwrap();
        answering.join().unwrap();
        changing.join().unwrap();
        assert_eq!(registry.standing(&[1; 32]), Standing::Paired(Grant::Read));
    }

    /// A device that was revoked is held as revoked and not as unknown, is answered nothing, and
    /// pairing it again makes it a device again.
    #[test]
    fn a_revoked_device_is_not_answered_until_the_person_pairs_it_again() {
        let registry = Registry::new();
        registry
            .admit([1; 32], device("phone", Grant::Control))
            .unwrap();
        registry.revoke("phone").unwrap();
        assert_eq!(registry.standing(&[1; 32]), Standing::Revoked);
        assert_eq!(registry.standing(&[2; 32]), Standing::Unknown);
        let mut ran = false;
        let answered = registry.answer(&[1; 32], Operation::Read, |_| {
            ran = true;
        });
        assert_eq!(answered, Err(Verdict::Revoked));
        assert!(!ran);
        assert!(registry.devices().is_empty());
        registry
            .admit([1; 32], device("phone", Grant::Read))
            .unwrap();
        assert_eq!(registry.standing(&[1; 32]), Standing::Paired(Grant::Read));
    }
}
