// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host's network listener as the host process runs it: started with `--listen`, driven by what
//! the person types on the host, and answering a device with synthetic data.
//!
//! Nothing here reaches [`crate::host::Host`] or a bridge. A paired device is given a session that
//! does not exist, which says so, so the transport and the pairing can be used before the decisions
//! that let a device see a real session are built (LISTEN-6). What the person types on the host is
//! the only way a device is paired or removed, and what a device sends is never read as a command.

use crate::protocol::{ErrorCode, Failure, Request, Unreadable, response};
use bravebot_core::device::Grant;
use bravebot_listener::identity::{Identity, KeyHash};
use bravebot_listener::invitation::{self, Announcement, Invitations, Terms};
use bravebot_listener::limits::Limits;
use bravebot_listener::registry::{Device, Registry};
use bravebot_listener::server::{Listener, Service};
use serde_json::{Value, json};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The one session a device is shown.
const SESSION: &str = "synthetic-1";

/// Answers a device from nothing but its own record: a session that is not real, and a refusal of
/// everything else.
pub struct Synthetic;

fn grant_word(grant: Grant) -> &'static str {
    match grant {
        Grant::Read => "read",
        Grant::Control => "control",
    }
}

impl Service for Synthetic {
    fn answer(&self, device: &Device, line: &str) -> Vec<String> {
        let request = match Request::parse(line) {
            Ok(request) => request,
            Err(Unreadable::Answerable { id, failure }) => {
                return vec![response(id, Err(failure)).to_string()];
            }
            Err(Unreadable::Unanswerable { .. }) => return Vec::new(),
        };
        let outcome = match request.method.as_str() {
            "instance.info" => Ok(json!({
                "synthetic": true,
                "device": {"name": device.name, "grant": grant_word(device.grant)},
                "sessions": [{"handle": SESSION, "state": "open", "attached": false}],
            })),
            "session.attach"
                if request.params.get("session").and_then(Value::as_str) == Some(SESSION) =>
            {
                Ok(json!({"version": 1, "sequence": 1, "omitted": 0}))
            }
            "session.attach" => Err(Failure::new(
                ErrorCode::NoSuchSession,
                "this connection serves one synthetic session",
            )),
            _ => Err(Failure::new(
                ErrorCode::NotAllowed,
                "this connection serves synthetic data only",
            )),
        };
        vec![response(request.id, outcome).to_string()]
    }
}

/// Where the host's words about a pairing go.
pub type Say = Arc<dyn Fn(&str) + Send + Sync>;

/// Something the host says in answer to a command.
#[derive(Debug, PartialEq, Eq)]
pub enum Said {
    /// A line, which the host puts its own name before.
    Line(String),
    /// Lines drawn to be seen together, such as a QR code, which are written as they are.
    Block(String),
}

/// A running listener with what the person's commands act on.
pub struct Link {
    listener: Listener,
    registry: Arc<Registry>,
    invitations: Arc<Invitations>,
    workspaces: Vec<PathBuf>,
    draws_codes: bool,
}

/// Why the listener did not start.
#[derive(Debug)]
pub enum StartError {
    /// The address names every interface, which is not one address.
    Unspecified,
    /// The system would not make a key.
    Key,
    /// The address could not be bound.
    Bind(io::Error),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unspecified => f.write_str("--listen names one address and not every interface"),
            Self::Key => f.write_str("the system could not make a key for the listener"),
            Self::Bind(error) => write!(f, "cannot listen: {error}"),
        }
    }
}

impl Link {
    /// Listen on `address`, which names one interface. `workspaces` are the directories a device
    /// may be given a grant on. `draws_codes` is whether the person's terminal can show a QR code.
    /// What the host has to say about a pairing as it goes is given to `say`.
    pub fn start(
        address: SocketAddr,
        workspaces: &[PathBuf],
        draws_codes: bool,
        say: Say,
    ) -> Result<Self, StartError> {
        if address.ip().is_unspecified() {
            return Err(StartError::Unspecified);
        }
        let identity = Identity::generate("bravebot-host").map_err(|_| StartError::Key)?;
        let registry = Arc::new(Registry::new());
        let announce = Arc::new(move |announcement: &Announcement| say(&announced(announcement)));
        let invitations = Arc::new(Invitations::system(Arc::clone(&registry), announce));
        let listener = Listener::start(
            address,
            &identity,
            Arc::clone(&registry),
            Arc::clone(&invitations),
            Arc::new(Synthetic),
            Limits::default(),
        )
        .map_err(StartError::Bind)?;
        Ok(Self {
            listener,
            registry,
            invitations,
            workspaces: workspaces
                .iter()
                .filter_map(|workspace| workspace.canonicalize().ok())
                .collect(),
            draws_codes,
        })
    }

    /// The address devices connect to.
    pub fn address(&self) -> SocketAddr {
        self.listener.address()
    }

    /// The hash of the host's key, which an invitation tells a phone to pin.
    pub fn key_hash(&self) -> KeyHash {
        self.listener.key_hash()
    }

    /// What the host says when it starts listening.
    pub fn listening(&self) -> String {
        format!(
            "listening on {}, host key {}",
            self.address(),
            invitation::spoken(&self.key_hash())
        )
    }

    /// Stop accepting, close the window and end every connection.
    pub fn stop(&self) {
        self.listener.stop();
    }

    /// What the person typed, if it is a command about devices, and what to say in answer. `None`
    /// where the line is not one.
    pub fn command(&self, line: &str) -> Option<Vec<Said>> {
        let mut words = line.split_whitespace();
        let rest: Vec<&str> = words.clone().skip(1).collect();
        match words.next()? {
            "pair" => Some(self.pair(&rest)),
            "confirm" => Some(vec![Said::Line(self.confirm(&rest))]),
            "devices" => Some(self.devices().into_iter().map(Said::Line).collect()),
            _ => None,
        }
    }

    fn workspace_named(&self, named: &str) -> Option<PathBuf> {
        Path::new(named)
            .canonicalize()
            .ok()
            .filter(|path| self.workspaces.contains(path))
    }

    /// `pair NAME read|control WORKSPACE`: open the window and show the invitation.
    fn pair(&self, arguments: &[&str]) -> Vec<Said> {
        const USAGE: &str = "type `pair NAME read|control WORKSPACE`, where NAME is 1 to 32 of a-z, 0-9 and -, and WORKSPACE is one of the host's --workspace directories";
        let line = |text: String| vec![Said::Line(text)];
        let [name, grant, workspace] = arguments else {
            return line(USAGE.to_string());
        };
        let grant = match *grant {
            "read" => Grant::Read,
            "control" => Grant::Control,
            _ => return line(USAGE.to_string()),
        };
        let Some(workspace) = self.workspace_named(workspace) else {
            return line(format!(
                "{workspace} is not one of the host's workspaces; {USAGE}"
            ));
        };
        let opened = match self.invitations.open(Terms {
            name: (*name).to_string(),
            grant,
            workspace: workspace.clone(),
        }) {
            Ok(opened) => opened,
            Err(error) => return line(format!("not opened: {error}")),
        };
        let link = invitation::link(&self.key_hash(), &[self.address()], &opened.secret);
        let mut said = line(format!(
            "pairing {name} with {} on {}: scan this on the phone within {} seconds, then type the code the phone shows as `confirm CODE`",
            grant_word(grant),
            workspace.display(),
            opened.lifetime.as_secs()
        ));
        if self.draws_codes
            && let Ok(code) = bravebot_qr::Code::encode(link.as_bytes())
        {
            said.push(Said::Block(bravebot_qr::render(&code)));
        }
        said.push(Said::Line(format!(
            "or type this link on the phone: {link}"
        )));
        said
    }

    /// `confirm CODE`: the code the phone shows.
    fn confirm(&self, arguments: &[&str]) -> String {
        let [code] = arguments else {
            return "type `confirm CODE`, with the code the phone shows".to_string();
        };
        match self.invitations.confirm(code) {
            Ok(name) => format!("paired {name}"),
            Err(error) => error.to_string(),
        }
    }

    /// `devices`: each device paired, with its grant and whether it is connected.
    fn devices(&self) -> Vec<String> {
        let devices = self.registry.devices();
        if devices.is_empty() {
            return vec!["no device is paired".to_string()];
        }
        devices
            .iter()
            .map(|(key, device)| {
                format!(
                    "{} {} {} {} key {}",
                    device.name,
                    grant_word(device.grant),
                    device.workspace.display(),
                    if self.registry.is_connected(key) {
                        "connected"
                    } else {
                        "not connected"
                    },
                    invitation::spoken(key)
                )
            })
            .collect()
    }
}

/// What the host says of something that happened in a pairing. A phone's own words are drawn so
/// that they cannot change the lines around them.
fn announced(announcement: &Announcement) -> String {
    match announcement {
        Announcement::Asking { name, label } => {
            let who = label.as_deref().map_or_else(
                || "a phone".to_string(),
                |label| format!("a phone calling itself \"{}\"", invitation::escaped(label)),
            );
            format!("{who} is asking to pair as {name}; type the code it shows as `confirm CODE`")
        }
        Announcement::Paired { name } => format!("{name} is paired"),
        Announcement::Ended { name } => format!("the pairing of {name} ended without a device"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(grant: Grant) -> Device {
        Device {
            name: "kitchen-phone".into(),
            grant,
            workspace: PathBuf::from("/work"),
        }
    }

    fn asked(method: &str, params: &Value) -> Value {
        let lines = Synthetic.answer(
            &device(Grant::Read),
            &json!({"id": 7, "method": method, "params": params}).to_string(),
        );
        assert_eq!(lines.len(), 1);
        serde_json::from_str(&lines[0]).unwrap()
    }

    /// What a device is shown is a session that is not real, and it says so, whatever grant it has.
    #[test]
    fn a_device_is_shown_one_synthetic_session_and_told_it_is_not_real() {
        for grant in [Grant::Read, Grant::Control] {
            let lines = Synthetic.answer(
                &device(grant),
                r#"{"id":1,"method":"instance.info","params":{}}"#,
            );
            let info: Value = serde_json::from_str(&lines[0]).unwrap();
            assert_eq!(info["id"], 1);
            assert_eq!(info["ok"]["synthetic"], true);
            assert_eq!(info["ok"]["device"]["name"], "kitchen-phone");
            assert_eq!(info["ok"]["device"]["grant"], grant_word(grant));
            assert_eq!(info["ok"]["sessions"][0]["handle"], SESSION);
        }
        let attached = asked("session.attach", &json!({"session": SESSION}));
        assert_eq!(attached["ok"]["sequence"], 1);
        let missing = asked("session.attach", &json!({"session": "another"}));
        assert_eq!(missing["error"]["code"], "no_such_session");
    }

    /// Every other method is refused, including the ones the host offers a client it started, and
    /// none reaches a session: this service holds no way to one.
    #[test]
    fn every_other_method_is_refused() {
        for method in [
            "session.new",
            "turn.send",
            "turn.cancel",
            "session.close",
            "trust.reply",
            "confirm.reply",
            "agent.info",
            "pair.begin",
            "anything-else",
        ] {
            let answer = asked(method, &json!({}));
            assert_eq!(answer["error"]["code"], "not_allowed", "{method}");
            assert_eq!(answer["id"], 7);
        }
    }

    #[test]
    fn a_line_that_cannot_be_answered_gets_no_answer_and_one_that_can_gets_a_failure() {
        assert!(
            Synthetic
                .answer(&device(Grant::Read), "not json")
                .is_empty()
        );
        assert!(
            Synthetic
                .answer(&device(Grant::Read), r#"{"method":"instance.info"}"#)
                .is_empty()
        );
        let lines = Synthetic.answer(&device(Grant::Read), r#"{"id":3}"#);
        let failure: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(failure["error"]["code"], "bad_request");
    }

    /// A phone's own words are drawn so they cannot rewrite the host's line, and the host says what
    /// the person has to type without saying the code.
    #[test]
    fn what_the_host_says_of_a_phone_is_drawn_escaped() {
        let said = announced(&Announcement::Asking {
            name: "one".into(),
            label: Some("\u{1b}[2J\npaired kitchen-phone".into()),
        });
        assert!(!said.contains('\u{1b}'));
        assert!(!said.contains('\n'));
        assert!(said.contains("\\u{1b}[2J\\u{a}paired kitchen-phone"));
        assert!(said.ends_with("as `confirm CODE`"));
        assert!(
            announced(&Announcement::Asking {
                name: "one".into(),
                label: None
            })
            .starts_with("a phone is asking")
        );
    }
}
