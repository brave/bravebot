// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The one window in which a device may pair, and the text of the invitation that opens it.
//!
//! The person types a pair command on the host, which opens a window: a secret made for it, the name
//! the device will have, its grant and its workspace. The host shows the secret to the phone in a QR
//! code. The phone proves it has the secret over its connection ([`crate::pairing`]) and shows a
//! confirmation code, and the person types that code on the host, which pairs the device. At most one
//! window is open. It ends when its device is paired, after two minutes, when another is opened, when
//! the person closes it, and at the first failed attempt: a proof that does not check, a second
//! device, or a code that does not match. A device that fails does not learn which.

use crate::identity::{KeyHash, hex};
use crate::pairing::{self, Exporter, Secret};
use crate::registry::{Device, Refused, Registry, is_a_name};
use bravebot_core::device::Grant;
use ring::rand::{SecureRandom, SystemRandom};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// How long a window stays open.
pub const LIFETIME: Duration = Duration::from_secs(120);

/// The time as both clocks of the machine give it. The monotonic one does not run while the machine
/// sleeps and the wall clock does, so a window is over when either says so: a laptop that slept with
/// a code on its screen does not wake to a window that is still open.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    pub monotonic: Instant,
    pub wall: SystemTime,
}

impl Moment {
    pub fn now() -> Self {
        Self {
            monotonic: Instant::now(),
            wall: SystemTime::now(),
        }
    }
}

/// Where the time comes from, which a test sets.
pub type Clock = Arc<dyn Fn() -> Moment + Send + Sync>;

/// Where the secret comes from, which a test sets. `None` where the system gave no random bytes.
pub type Random = Arc<dyn Fn() -> Option<Secret> + Send + Sync>;

/// What the person chose when they opened the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms {
    pub name: String,
    pub grant: Grant,
    pub workspace: PathBuf,
}

/// An open window, for the code that shows it.
#[derive(PartialEq, Eq)]
pub struct Opened {
    pub secret: Secret,
    pub lifetime: Duration,
}

impl std::fmt::Debug for Opened {
    /// Holds back the secret, so a log line or a failed assertion does not show it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opened {{ lifetime: {:?} }}", self.lifetime)
    }
}

/// Why a window was not opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The name or its use was refused.
    Name(Refused),
    /// The system gave no random bytes, so no secret was made.
    NoSecret,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(refused) => refused.fmt(f),
            Self::NoSecret => f.write_str("the system could not make a secret"),
        }
    }
}

/// Why a typed code did not pair the device.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfirmError {
    /// No device is waiting for a code: no window is open, or no phone has proved the secret.
    NothingToConfirm,
    /// The code was not the one the phone showed, and the window is closed.
    WrongCode,
    /// The device could not be added, and the window is closed.
    Refused(Refused),
}

impl std::fmt::Display for ConfirmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToConfirm => f.write_str("no phone is waiting for a code"),
            Self::WrongCode => {
                f.write_str("that is not the code the phone shows; the pairing is closed")
            }
            Self::Refused(refused) => write!(f, "{refused}; the pairing is closed"),
        }
    }
}

/// What a connection waiting for its code is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Paired,
    Refused,
}

/// Something the person at the host is told as a pairing goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Announcement {
    /// A phone proved the secret. `label` is what the phone calls itself, which a phone chooses.
    Asking { name: String, label: Option<String> },
    /// The device is paired.
    Paired { name: String },
    /// The window ended without pairing a device.
    Ended { name: String },
}

/// Where announcements go.
pub type Announce = Arc<dyn Fn(&Announcement) + Send + Sync>;

#[derive(Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for a phone to prove the secret.
    Open,
    /// A phone proved it, and the person has not typed its code.
    Proven { device: KeyHash, code: String },
    /// Over.
    Done(Outcome),
}

struct Window {
    secret: Secret,
    terms: Terms,
    ends: Instant,
    ends_wall: SystemTime,
    phase: Phase,
    /// Which window this is, so a connection waiting on one is not woken by another.
    generation: u64,
}

#[derive(Default)]
struct State {
    window: Option<Window>,
    generations: u64,
}

/// A connection that has proved the secret and waits for the person to type the code.
#[derive(Debug, PartialEq, Eq)]
pub struct Ticket(u64);

/// The window and what the person is told.
pub struct Invitations {
    state: Mutex<State>,
    changed: Condvar,
    registry: Arc<Registry>,
    clock: Clock,
    random: Random,
    announce: Announce,
}

impl Invitations {
    pub fn new(registry: Arc<Registry>, clock: Clock, random: Random, announce: Announce) -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            registry,
            clock,
            random,
            announce,
        }
    }

    /// The system's clock and random bytes.
    pub fn system(registry: Arc<Registry>, announce: Announce) -> Self {
        Self::new(
            registry,
            Arc::new(Moment::now),
            Arc::new(|| {
                let mut secret = [0u8; 32];
                SystemRandom::new().fill(&mut secret).ok().map(|()| secret)
            }),
            announce,
        )
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Each change below leaves the state whole, so one that was made by a thread that panicked
        // is still one.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> Moment {
        (self.clock)()
    }

    /// End the window if its time is up, so every use below sees only a window that is open.
    fn expire(&self, state: &mut State) -> Option<String> {
        let now = self.now();
        let over = state.window.as_ref().is_some_and(|window| {
            (window.ends <= now.monotonic || window.ends_wall <= now.wall)
                && !matches!(window.phase, Phase::Done(_))
        });
        if over {
            return self.end(state);
        }
        None
    }

    /// End the window as unsuccessful and say whose it was.
    fn end(&self, state: &mut State) -> Option<String> {
        let window = state.window.as_mut()?;
        if matches!(window.phase, Phase::Done(_)) {
            return None;
        }
        window.phase = Phase::Done(Outcome::Refused);
        self.changed.notify_all();
        Some(window.terms.name.clone())
    }

    fn say(&self, announcement: &Announcement) {
        (self.announce)(announcement);
    }

    /// Open a window for `terms`, ending any that is open.
    pub fn open(&self, terms: Terms) -> Result<Opened, OpenError> {
        if !is_a_name(&terms.name) {
            return Err(OpenError::Name(Refused::BadName));
        }
        if self
            .registry
            .devices()
            .iter()
            .any(|(_, device)| device.name == terms.name)
        {
            return Err(OpenError::Name(Refused::NameTaken));
        }
        let secret = (self.random)().ok_or(OpenError::NoSecret)?;
        let now = self.now();
        let ended = {
            let mut state = self.lock();
            let ended = self.end(&mut state);
            state.generations += 1;
            let generation = state.generations;
            state.window = Some(Window {
                secret,
                terms,
                ends: now.monotonic + LIFETIME,
                ends_wall: now.wall + LIFETIME,
                phase: Phase::Open,
                generation,
            });
            ended
        };
        if let Some(name) = ended {
            self.say(&Announcement::Ended { name });
        }
        Ok(Opened {
            secret,
            lifetime: LIFETIME,
        })
    }

    /// Whether a window is open: one a phone may still use, or one holding a phone's proof that the
    /// person has not answered.
    pub fn is_open(&self) -> bool {
        let mut state = self.lock();
        let ended = self.expire(&mut state);
        let open = state
            .window
            .as_ref()
            .is_some_and(|window| !matches!(window.phase, Phase::Done(_)));
        drop(state);
        if let Some(name) = ended {
            self.say(&Announcement::Ended { name });
        }
        open
    }

    /// A phone sent `proof` on a connection whose session value is `exporter`, with the key that
    /// hashes to `device`. It proves the secret, and the window then waits for the person's code, or
    /// it does not, and `None` says the window closed.
    pub fn prove(
        &self,
        proof: &[u8],
        exporter: &Exporter,
        device: &KeyHash,
        label: Option<String>,
    ) -> Option<Ticket> {
        let mut state = self.lock();
        let mut ended = self.expire(&mut state);
        let accepted = state.window.as_mut().and_then(|window| {
            if window.phase != Phase::Open
                || !pairing::proof_is_right(&window.secret, exporter, device, proof)
            {
                return None;
            }
            let code = pairing::confirmation_code(&window.secret, exporter, device);
            window.phase = Phase::Proven {
                device: *device,
                code,
            };
            Some((Ticket(window.generation), window.terms.name.clone()))
        });
        let Some((ticket, name)) = accepted else {
            ended = ended.or_else(|| self.end(&mut state));
            drop(state);
            if let Some(name) = ended {
                self.say(&Announcement::Ended { name });
            }
            return None;
        };
        drop(state);
        self.say(&Announcement::Asking { name, label });
        Some(ticket)
    }

    /// Wait until the person has answered for the connection with `ticket`, or the window's time is
    /// up.
    pub fn wait(&self, ticket: &Ticket) -> Outcome {
        let mut state = self.lock();
        loop {
            let ended = self.expire(&mut state);
            let outcome = match state.window.as_ref() {
                Some(window) if window.generation == ticket.0 => match window.phase {
                    Phase::Done(outcome) => Some(outcome),
                    _ => None,
                },
                // Another window has replaced this one.
                _ => Some(Outcome::Refused),
            };
            if let Some(name) = ended {
                // Said with the lock let go, so a place that hears it may ask the window a question.
                drop(state);
                self.say(&Announcement::Ended { name });
                state = self.lock();
            }
            if let Some(outcome) = outcome {
                return outcome;
            }
            let left = state
                .window
                .as_ref()
                .map_or(Duration::ZERO, |window| {
                    let now = self.now();
                    window.ends.saturating_duration_since(now.monotonic).min(
                        window
                            .ends_wall
                            .duration_since(now.wall)
                            .unwrap_or(Duration::ZERO),
                    )
                })
                .max(Duration::from_millis(1));
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// The person typed `typed` on the host.
    pub fn confirm(&self, typed: &str) -> Result<String, ConfirmError> {
        let mut state = self.lock();
        let ended = self.expire(&mut state);
        let result = (|| {
            let window = state
                .window
                .as_mut()
                .ok_or(ConfirmError::NothingToConfirm)?;
            let Phase::Proven { device, code } = &window.phase else {
                return Err(ConfirmError::NothingToConfirm);
            };
            let (device, matches) = (*device, pairing::codes_match(typed.trim(), code));
            if !matches {
                window.phase = Phase::Done(Outcome::Refused);
                return Err(ConfirmError::WrongCode);
            }
            let name = window.terms.name.clone();
            let added = self.registry.admit(
                device,
                Device {
                    name: name.clone(),
                    grant: window.terms.grant,
                    workspace: window.terms.workspace.clone(),
                },
            );
            match added {
                Ok(()) => {
                    window.phase = Phase::Done(Outcome::Paired);
                    Ok(name)
                }
                Err(refused) => {
                    window.phase = Phase::Done(Outcome::Refused);
                    Err(ConfirmError::Refused(refused))
                }
            }
        })();
        self.changed.notify_all();
        drop(state);
        if let Some(name) = ended {
            self.say(&Announcement::Ended { name });
        }
        if let Ok(name) = &result {
            self.say(&Announcement::Paired { name: name.clone() });
        }
        result
    }

    /// Close the window, as the host does when it stops listening.
    pub fn close(&self) {
        let ended = {
            let mut state = self.lock();
            self.end(&mut state)
        };
        if let Some(name) = ended {
            self.say(&Announcement::Ended { name });
        }
    }
}

/// The text a phone reads from the QR code: `bravebot://pair?v=1&h=<host key hash>&a=<address>&s=<secret>`
/// with the hash and secret in URL-safe base64 without padding, and one `a` for each address.
pub fn link(host: &KeyHash, addresses: &[SocketAddr], secret: &Secret) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut link = format!("bravebot://pair?v=1&h={}", URL_SAFE_NO_PAD.encode(host));
    for address in addresses {
        link.push_str(&format!("&a={address}"));
    }
    link.push_str(&format!("&s={}", URL_SAFE_NO_PAD.encode(secret)));
    link
}

/// What a phone finds in a link, or `None` where it is not one this host writes.
#[derive(PartialEq, Eq)]
pub struct Invitation {
    pub host: KeyHash,
    pub addresses: Vec<SocketAddr>,
    pub secret: Secret,
}

impl std::fmt::Debug for Invitation {
    /// Holds back the secret, as [`Opened`] does.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Invitation {{ host: {}, addresses: {:?} }}",
            hex(&self.host),
            self.addresses
        )
    }
}

/// Read a link made by [`link`]. A phone does this, and the host's tests do it to be the phone.
pub fn parse_link(text: &str) -> Option<Invitation> {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let query = text.strip_prefix("bravebot://pair?")?;
    let (mut version, mut host, mut secret) = (None, None, None);
    let mut addresses = Vec::new();
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        match name {
            "v" if version.is_none() => version = Some(value),
            "h" if host.is_none() => host = Some(value),
            "s" if secret.is_none() => secret = Some(value),
            "a" => addresses.push(value.parse().ok()?),
            _ => return None,
        }
    }
    if version? != "1" || addresses.is_empty() {
        return None;
    }
    let decode =
        |text: &str| -> Option<[u8; 32]> { URL_SAFE_NO_PAD.decode(text).ok()?.try_into().ok() };
    Some(Invitation {
        host: decode(host?)?,
        addresses,
        secret: decode(secret?)?,
    })
}

/// Text from a device, drawn so that it cannot change what a person reads around it. The characters
/// of ASCII that print, and a space, are kept, except a quote, which could end the quotes the host
/// puts the text in, and a backslash, which could begin an escape. Everything else is written as
/// `\u{..}`: a control character, a mark that reorders, hides or combines, and any letter that is
/// not ASCII. A phone is told apart by the code it shows and by the name the person typed, and not
/// by how its label reads.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        let kept = (character.is_ascii_graphic() && character != '"' && character != '\\')
            || character == ' ';
        if kept {
            out.push(character);
        } else {
            out.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        }
    }
    out
}

/// A hash for the person to read, in groups so a long one is not mistaken for another.
pub fn spoken(hash: &KeyHash) -> String {
    hex(hash)
        .as_bytes()
        .chunks(8)
        .filter_map(|group| std::str::from_utf8(group).ok())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::mpsc;

    struct Fixture {
        registry: Arc<Registry>,
        invitations: Arc<Invitations>,
        offset: Arc<Mutex<Duration>>,
        /// How far the wall clock is ahead of the monotonic one, as after the machine slept.
        slept: Arc<Mutex<Duration>>,
        said: Arc<Mutex<Vec<Announcement>>>,
    }

    impl Fixture {
        fn new() -> Self {
            let registry = Arc::new(Registry::new());
            let (start, wall_start) = (Instant::now(), SystemTime::now());
            let offset = Arc::new(Mutex::new(Duration::ZERO));
            let slept = Arc::new(Mutex::new(Duration::ZERO));
            let clock: Clock = {
                let (offset, slept) = (Arc::clone(&offset), Arc::clone(&slept));
                Arc::new(move || {
                    let passed = *offset.lock().unwrap();
                    Moment {
                        monotonic: start + passed,
                        wall: wall_start + passed + *slept.lock().unwrap(),
                    }
                })
            };
            // A different secret each time, and the first is [1; 32].
            let counter = Arc::new(AtomicU8::new(1));
            let random: Random =
                Arc::new(move || Some([counter.fetch_add(1, Ordering::SeqCst); 32]));
            let said = Arc::new(Mutex::new(Vec::new()));
            let announce: Announce = {
                let said = Arc::clone(&said);
                Arc::new(move |announcement| said.lock().unwrap().push(announcement.clone()))
            };
            Self {
                invitations: Arc::new(Invitations::new(
                    Arc::clone(&registry),
                    clock,
                    random,
                    announce,
                )),
                registry,
                offset,
                slept,
                said,
            }
        }

        fn later(&self, by: Duration) {
            *self.offset.lock().unwrap() += by;
        }

        fn terms(name: &str) -> Terms {
            Terms {
                name: name.into(),
                grant: Grant::Control,
                workspace: PathBuf::from("/work"),
            }
        }

        fn said(&self) -> Vec<Announcement> {
            self.said.lock().unwrap().clone()
        }
    }

    const EXPORTER: Exporter = [5; 32];
    const DEVICE: KeyHash = [6; 32];

    /// What a connection waiting on `ticket` is told, found on a thread so that a window that never
    /// answers fails the test and does not hang it.
    fn told(invitations: &Arc<Invitations>, ticket: Ticket) -> Outcome {
        let (sender, heard) = mpsc::channel();
        let invitations = Arc::clone(invitations);
        std::thread::spawn(move || {
            sender.send(invitations.wait(&ticket)).ok();
        });
        // A wait that never returns fails here, at the timeout.
        heard.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    fn proof_of(opened: &Opened, exporter: &Exporter, device: &KeyHash) -> [u8; 32] {
        pairing::proof(&opened.secret, exporter, device)
    }

    fn code_of(opened: &Opened, exporter: &Exporter, device: &KeyHash) -> String {
        pairing::confirmation_code(&opened.secret, exporter, device)
    }

    /// A phone that proves the secret and whose person types its code is paired with what the person
    /// chose when they opened the window, and everything the host says is about that.
    #[test]
    fn a_phone_that_proves_the_secret_and_whose_code_is_typed_is_paired() {
        let fixture = Fixture::new();
        let opened = fixture
            .invitations
            .open(Fixture::terms("kitchen-phone"))
            .unwrap();
        assert_eq!(opened.lifetime, Duration::from_secs(120));
        assert!(fixture.invitations.is_open());
        let ticket = fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                Some("a phone".into()),
            )
            .unwrap();
        // The phone has proved the secret, and is not paired until the person types its code.
        assert_eq!(
            fixture.registry.standing(&DEVICE),
            bravebot_core::device::Standing::Unknown
        );
        let name = fixture
            .invitations
            .confirm(&code_of(&opened, &EXPORTER, &DEVICE))
            .unwrap();
        assert_eq!(name, "kitchen-phone");
        assert_eq!(told(&fixture.invitations, ticket), Outcome::Paired);
        assert_eq!(
            fixture.registry.standing(&DEVICE),
            bravebot_core::device::Standing::Paired(Grant::Control)
        );
        let devices = fixture.registry.devices();
        assert_eq!(devices[0].1.workspace, PathBuf::from("/work"));
        assert_eq!(
            fixture.said(),
            [
                Announcement::Asking {
                    name: "kitchen-phone".into(),
                    label: Some("a phone".into())
                },
                Announcement::Paired {
                    name: "kitchen-phone".into()
                }
            ]
        );
        assert!(!fixture.invitations.is_open());
    }

    /// A window pairs one device: the next proof, even a right one, is refused.
    #[test]
    fn a_window_pairs_one_device() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        fixture
            .invitations
            .confirm(&code_of(&opened, &EXPORTER, &DEVICE))
            .unwrap();
        let other = [9; 32];
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&opened, &EXPORTER, &other),
                    &EXPORTER,
                    &other,
                    None
                )
                .is_none()
        );
        assert_eq!(fixture.registry.devices().len(), 1);
    }

    /// A proof that does not check closes the window, so the next one is refused even if it is right.
    /// A proof made for another connection, another key, or with a secret from another window does
    /// not check.
    #[test]
    fn a_proof_that_does_not_check_closes_the_window_whichever_input_was_wrong() {
        let other_exporter = [50; 32];
        let other_device = [60; 32];
        for wrong in 0..4 {
            let fixture = Fixture::new();
            let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
            let bad = match wrong {
                0 => proof_of(&opened, &other_exporter, &DEVICE),
                1 => proof_of(&opened, &EXPORTER, &other_device),
                2 => pairing::proof(&[99; 32], &EXPORTER, &DEVICE),
                _ => [0; 32],
            };
            assert!(
                fixture
                    .invitations
                    .prove(&bad, &EXPORTER, &DEVICE, None)
                    .is_none(),
                "case {wrong}"
            );
            assert!(!fixture.invitations.is_open(), "case {wrong}");
            let good = proof_of(&opened, &EXPORTER, &DEVICE);
            assert!(
                fixture
                    .invitations
                    .prove(&good, &EXPORTER, &DEVICE, None)
                    .is_none()
            );
            assert!(fixture.registry.devices().is_empty());
        }
    }

    /// A code that is not the phone's closes the window and pairs nothing, and the person is told
    /// nothing is waiting for a code afterwards.
    #[test]
    fn a_wrong_code_closes_the_window_and_pairs_nothing() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let ticket = fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        let right = code_of(&opened, &EXPORTER, &DEVICE);
        let wrong = if right == "000000" {
            "000001"
        } else {
            "000000"
        };
        assert_eq!(
            fixture.invitations.confirm(wrong),
            Err(ConfirmError::WrongCode)
        );
        assert_eq!(told(&fixture.invitations, ticket), Outcome::Refused);
        assert_eq!(
            fixture.invitations.confirm(&right),
            Err(ConfirmError::NothingToConfirm)
        );
        assert!(fixture.registry.devices().is_empty());
    }

    /// A second phone that proves the secret while the first waits for its code is refused, and the
    /// window closes, so the first is not paired either.
    #[test]
    fn a_second_device_closes_the_window_of_the_first() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let first = fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        let second = [70; 32];
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&opened, &EXPORTER, &second),
                    &EXPORTER,
                    &second,
                    None
                )
                .is_none()
        );
        assert_eq!(told(&fixture.invitations, first), Outcome::Refused);
        assert_eq!(
            fixture
                .invitations
                .confirm(&code_of(&opened, &EXPORTER, &DEVICE)),
            Err(ConfirmError::NothingToConfirm)
        );
    }

    /// A window lasts two minutes, and then neither a proof nor a code is taken from it.
    #[test]
    fn a_window_ends_when_its_time_is_up() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        fixture.later(Duration::from_secs(119));
        assert!(fixture.invitations.is_open());
        fixture.later(Duration::from_secs(2));
        assert!(!fixture.invitations.is_open());
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&opened, &EXPORTER, &DEVICE),
                    &EXPORTER,
                    &DEVICE,
                    None
                )
                .is_none()
        );
        assert_eq!(fixture.said(), [Announcement::Ended { name: "one".into() }]);
    }

    /// The monotonic clock stops while the machine sleeps and the wall clock does not, so a window
    /// that the wall clock says is over is over, and a proof is not taken from it.
    #[test]
    fn a_window_ends_when_the_wall_clock_says_so_though_the_other_clock_stopped() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        assert!(fixture.invitations.is_open());
        *fixture.slept.lock().unwrap() += Duration::from_secs(3600);
        assert!(!fixture.invitations.is_open());
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&opened, &EXPORTER, &DEVICE),
                    &EXPORTER,
                    &DEVICE,
                    None
                )
                .is_none()
        );
        assert!(fixture.registry.devices().is_empty());
    }

    /// The time is up while a phone waits for its code: the code is no longer taken.
    #[test]
    fn a_code_typed_after_the_time_is_up_pairs_nothing() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let ticket = fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        fixture.later(Duration::from_secs(121));
        assert_eq!(
            fixture
                .invitations
                .confirm(&code_of(&opened, &EXPORTER, &DEVICE)),
            Err(ConfirmError::NothingToConfirm)
        );
        assert_eq!(told(&fixture.invitations, ticket), Outcome::Refused);
        assert!(fixture.registry.devices().is_empty());
    }

    /// Opening another window ends the one open, and the secret of the first is not the second's.
    #[test]
    fn a_new_window_ends_the_old_one() {
        let fixture = Fixture::new();
        let first = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let ticket = fixture
            .invitations
            .prove(
                &proof_of(&first, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        let second = fixture.invitations.open(Fixture::terms("two")).unwrap();
        assert_ne!(first.secret, second.secret);
        assert_eq!(told(&fixture.invitations, ticket), Outcome::Refused);
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&first, &EXPORTER, &DEVICE),
                    &EXPORTER,
                    &DEVICE,
                    None
                )
                .is_none()
        );
    }

    #[test]
    fn closing_ends_the_window() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        fixture.invitations.close();
        assert!(!fixture.invitations.is_open());
        assert!(
            fixture
                .invitations
                .prove(
                    &proof_of(&opened, &EXPORTER, &DEVICE),
                    &EXPORTER,
                    &DEVICE,
                    None
                )
                .is_none()
        );
    }

    /// A window is refused for a name that is not a name or that a device has, and where the system
    /// gave no secret.
    #[test]
    fn a_window_is_not_opened_for_a_bad_or_taken_name_or_without_a_secret() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.invitations.open(Fixture::terms("Bad Name")),
            Err(OpenError::Name(Refused::BadName))
        );
        fixture
            .registry
            .admit(
                [1; 32],
                Device {
                    name: "taken".into(),
                    grant: Grant::Read,
                    workspace: PathBuf::new(),
                },
            )
            .unwrap();
        assert_eq!(
            fixture.invitations.open(Fixture::terms("taken")),
            Err(OpenError::Name(Refused::NameTaken))
        );
        let no_secret = Invitations::new(
            Arc::new(Registry::new()),
            Arc::new(Moment::now),
            Arc::new(|| None),
            Arc::new(|_| {}),
        );
        assert_eq!(
            no_secret.open(Fixture::terms("one")),
            Err(OpenError::NoSecret)
        );
        assert!(!no_secret.is_open());
    }

    /// A connection that waits for its code is told as soon as the person types it, on another
    /// thread.
    #[test]
    fn a_waiting_connection_is_told_when_the_code_is_typed() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let ticket = fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        let (told, hear) = mpsc::channel();
        let invitations = Arc::clone(&fixture.invitations);
        std::thread::spawn(move || told.send(invitations.wait(&ticket)).unwrap());
        // A thread blocked in `wait` gives no event to wait for. This is the time in which a `wait` that
        // returned early would say so, and the `confirm` below shows the watch can end.
        assert!(hear.recv_timeout(Duration::from_millis(200)).is_err());
        fixture
            .invitations
            .confirm(&code_of(&opened, &EXPORTER, &DEVICE))
            .unwrap();
        assert_eq!(
            hear.recv_timeout(Duration::from_secs(5)),
            Ok(Outcome::Paired)
        );
    }

    /// The secret is in the link and in nothing the host says or prints about a window.
    #[test]
    fn the_secret_is_in_no_debug_form_and_no_announcement() {
        let fixture = Fixture::new();
        let opened = fixture.invitations.open(Fixture::terms("one")).unwrap();
        let secret_text = hex(&opened.secret);
        assert!(!format!("{opened:?}").contains(&secret_text));
        fixture
            .invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        fixture
            .invitations
            .confirm(&code_of(&opened, &EXPORTER, &DEVICE))
            .unwrap();
        assert!(!format!("{:?}", fixture.said()).contains(&secret_text));
        let invitation = parse_link(&link(
            &[200; 32],
            &["127.0.0.1:7443".parse().unwrap()],
            &opened.secret,
        ))
        .unwrap();
        assert!(!format!("{invitation:?}").contains(&secret_text));
    }

    #[test]
    fn a_link_is_read_back_as_it_was_written() {
        let addresses: Vec<SocketAddr> = vec![
            "192.168.4.35:7443".parse().unwrap(),
            "[fd7a::1]:7443".parse().unwrap(),
        ];
        let written = link(&[11; 32], &addresses, &[22; 32]);
        assert!(written.starts_with("bravebot://pair?v=1&h="));
        assert_eq!(
            parse_link(&written),
            Some(Invitation {
                host: [11; 32],
                addresses,
                secret: [22; 32]
            })
        );
    }

    /// Anything that is not a link this host writes is not read, however close.
    #[test]
    fn text_that_is_not_a_link_is_not_read() {
        let good = link(&[11; 32], &["127.0.0.1:7443".parse().unwrap()], &[22; 32]);
        assert!(parse_link(&good).is_some());
        let host = good.split('&').nth(1).unwrap().to_string();
        let secret = good.rsplit('&').next().unwrap().to_string();
        for bad in [
            String::new(),
            "bravebot://open?v=1".into(),
            good.replace("v=1", "v=2"),
            good.replace("bravebot://", "https://"),
            good.replace("&a=127.0.0.1:7443", ""),
            good.replace("127.0.0.1:7443", "not-an-address"),
            good.replace(&host, "h=short"),
            good.replace(&secret, "s=!!!!"),
            format!("{good}&{host}"),
            format!("{good}&x=1"),
            format!("{good}&s"),
            good.replace("&s=", "&s=A"),
        ] {
            assert_eq!(parse_link(&bad), None, "{bad}");
        }
    }

    /// Text from a device is drawn so that it cannot rewrite the line it is in. The characters of
    /// ASCII that print, and a space, are kept, except a quote, which could end the quotes the host
    /// puts the text in, and a backslash, which could begin an escape. Everything else is written as
    /// its number: a control character, a mark that reorders, hides or combines, and any letter that
    /// is not ASCII.
    #[test]
    fn a_devices_text_is_drawn_without_anything_that_could_rewrite_the_line() {
        assert_eq!(escaped("Pixel 10 Pro"), "Pixel 10 Pro");
        assert_eq!(escaped("Chris's iPhone (2)"), "Chris's iPhone (2)");
        assert_eq!(escaped("a\nb"), "a\\u{a}b");
        assert_eq!(
            escaped("\u{1b}[31mred\u{1b}[0m"),
            "\\u{1b}[31mred\\u{1b}[0m"
        );
        assert_eq!(escaped("a\\b"), "a\\u{5c}b");
        assert_eq!(escaped("say \"hi\""), "say \\u{22}hi\\u{22}");
        assert_eq!(
            escaped("\u{e9}\u{65e5}\u{2019}"),
            "\\u{e9}\\u{65e5}\\u{2019}"
        );
        for hidden in [
            '\u{202e}',
            '\u{200b}',
            '\u{feff}',
            '\u{061c}',
            '\u{180e}',
            '\u{00ad}',
            '\u{2028}',
            '\u{2066}',
            '\u{e0041}',
            '\u{e007f}',
            '\u{fe0f}',
            '\u{0301}',
            '\u{0345}',
            '\u{3164}',
            '\u{115f}',
            '\u{ffa0}',
            '\u{e000}',
            '\u{7f}',
            '\u{9b}',
            '\u{d}',
            '\u{1f600}',
            '\u{fff9}',
        ] {
            let drawn = escaped(&format!("a{hidden}b"));
            assert_eq!(
                drawn,
                format!("a\\u{{{:x}}}b", u32::from(hidden)),
                "{hidden:?}"
            );
        }
    }

    /// What the host says is said after the lock is let go, so a place that hears it may ask the
    /// window a question. A host that said it under the lock would stop here, and the test would
    /// fail at its timeout.
    #[test]
    fn what_is_said_when_a_window_ends_is_said_outside_the_lock() {
        let registry = Arc::new(Registry::new());
        let slot: Arc<Mutex<Option<Arc<Invitations>>>> = Arc::new(Mutex::new(None));
        let offset = Arc::new(Mutex::new(Duration::ZERO));
        let (start, wall_start) = (Instant::now(), SystemTime::now());
        let clock: Clock = {
            let offset = Arc::clone(&offset);
            Arc::new(move || {
                let passed = *offset.lock().unwrap();
                Moment {
                    monotonic: start + passed,
                    wall: wall_start + passed,
                }
            })
        };
        let announce: Announce = {
            let slot = Arc::clone(&slot);
            Arc::new(move |_| {
                let invitations = slot.lock().unwrap().clone();
                if let Some(invitations) = invitations {
                    invitations.is_open();
                }
            })
        };
        let invitations = Arc::new(Invitations::new(
            Arc::clone(&registry),
            clock,
            Arc::new(|| Some([9; 32])),
            announce,
        ));
        *slot.lock().unwrap() = Some(Arc::clone(&invitations));
        let opened = invitations.open(Fixture::terms("one")).unwrap();
        let ticket = invitations
            .prove(
                &proof_of(&opened, &EXPORTER, &DEVICE),
                &EXPORTER,
                &DEVICE,
                None,
            )
            .unwrap();
        *offset.lock().unwrap() += Duration::from_secs(121);
        assert_eq!(told(&invitations, ticket), Outcome::Refused);
    }

    #[test]
    fn a_hash_is_spoken_in_groups_of_eight() {
        let spoken = spoken(&[0xab; 32]);
        assert_eq!(spoken.split(' ').count(), 8);
        assert!(spoken.split(' ').all(|group| group.len() == 8));
    }
}
