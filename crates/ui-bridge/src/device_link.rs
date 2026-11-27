// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host's network listener as the host process runs it: started with `--listen`, driven by what
//! the person types on the host, and answering a device with synthetic data.
//!
//! Nothing here reaches [`crate::host::Host`] or a bridge. A paired device is given a session that
//! does not exist, which says so, so the transport, the pairing and the grants can be used before the
//! decisions that let a device see a real session are built (LISTEN-6). Whether a request reads or
//! controls is the host's allowlist's answer for its method ([`allowlist::access_of`]). What the
//! person types on the host is the only way a device is paired or removed, and what a device sends is
//! never read as a command.

use crate::allowlist::{self, Access};
use crate::protocol::{ErrorCode, Failure, Request, Unreadable, response};
use bravebot_core::device::Grant;
use bravebot_core::scope::{Approvals, Confinement, Field, Reads, Scope, Writes};
use bravebot_listener::identity::{Identity, KeyHash};
use bravebot_listener::invitation::{self, Announcement, Invitations, Terms};
use bravebot_listener::limits::Limits;
use bravebot_listener::registry::{Device, Registry};
use bravebot_listener::server::{Listener, Prepared, Service};
use serde_json::{Value, json};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The one session a device is shown.
const SESSION: &str = "synthetic-1";

/// Answers a device from nothing but its own record and the request: a session that is not real, a
/// fixed answer to each control the host offers, which changes nothing, and a refusal of everything
/// else.
pub struct Synthetic;

fn grant_word(grant: Grant) -> &'static str {
    match grant {
        Grant::Read => "read",
        Grant::Control => "control",
    }
}

impl Service for Synthetic {
    /// The line is read here, with no lock held, and what is returned only builds the answer. A line
    /// that is not a request asks to control, so a `read` device is let through only with a request
    /// the allowlist says reads.
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a> {
        let request = match Request::parse(line) {
            Ok(request) => request,
            Err(Unreadable::Answerable { id, failure }) => {
                return Prepared::new(Access::Control, move |_| {
                    vec![response(id, Err(failure)).to_string()]
                });
            }
            Err(Unreadable::Unanswerable { .. }) => {
                return Prepared::new(Access::Control, |_| Vec::new());
            }
        };
        let access = allowlist::access_of(&request.method);
        Prepared::new(access, move |device| answer(device, &request))
    }
}

/// The answer to a request from `device`.
fn answer(device: &Device, request: &Request) -> Vec<String> {
    let session = request.params.get("session").and_then(Value::as_str);
    let outcome = match request.method.as_str() {
        "instance.info" => Ok(json!({
            "synthetic": true,
            "device": {"name": device.name, "grant": grant_word(device.grant)},
            "sessions": [{"handle": SESSION, "state": "open", "attached": false}],
        })),
        "session.attach" if session == Some(SESSION) => {
            Ok(json!({"version": 1, "sequence": 1, "omitted": 0}))
        }
        "session.attach" => Err(no_such_session()),
        method if allowlist::access_of(method) == Access::Control => controlled(request, session),
        _ => Err(synthetic_only()),
    };
    vec![response(request.id, outcome).to_string()]
}

/// A control the host offers, with the parameters it offers it with, answered without doing anything:
/// `session.new` names the synthetic session, whatever directory it names, and every other control of
/// the synthetic session is answered as done. A control of another session has no session, and a
/// method the host does not offer is refused.
fn controlled(request: &Request, session: Option<&str>) -> Result<Value, Failure> {
    allowlist::check(request).map_err(|_| synthetic_only())?;
    match request.method.as_str() {
        "session.new" => Ok(json!({"session": SESSION, "synthetic": true})),
        _ if session == Some(SESSION) => Ok(json!({"synthetic": true})),
        _ => Err(no_such_session()),
    }
}

fn no_such_session() -> Failure {
    Failure::new(
        ErrorCode::NoSuchSession,
        "this connection serves one synthetic session",
    )
}

fn synthetic_only() -> Failure {
    Failure::new(
        ErrorCode::NotAllowed,
        "this connection serves synthetic data only",
    )
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
    ceiling_of: CeilingOf,
}

/// The scope a new session in a workspace would hold now, which a device paired on it is approved
/// for, or `None` where it cannot be read. The host gives it from its own bridge.
pub type CeilingOf = Arc<dyn Fn(&Path) -> Option<Scope> + Send + Sync>;

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
    /// What the host has to say about a pairing as it goes is given to `say`. `ceiling_of` gives
    /// the limits a device paired on a workspace is approved for.
    pub fn start(
        address: SocketAddr,
        workspaces: &[PathBuf],
        draws_codes: bool,
        say: Say,
        ceiling_of: CeilingOf,
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
            ceiling_of,
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
            "revoke" => Some(vec![Said::Line(self.revoke(&rest))]),
            _ => None,
        }
    }

    /// The workspace a pairing covers: the one the person named, or the only one the host has when
    /// they named none.
    fn workspace_for(&self, named: Option<&str>) -> Result<PathBuf, String> {
        let canonical = named.map(|named| Path::new(named).canonicalize().ok());
        match (canonical, self.workspaces.as_slice()) {
            (None, [only]) => Ok(only.clone()),
            (None, _) => Err(format!(
                "name a workspace: {}",
                workspace_list(&self.workspaces)
            )),
            (Some(Some(path)), workspaces) if workspaces.contains(&path) => Ok(path),
            (Some(_), workspaces) => Err(format!(
                "{} is not one of the host's workspaces: {}",
                named.unwrap_or_default(),
                workspace_list(workspaces)
            )),
        }
    }

    /// `pair NAME read|control [WORKSPACE]`: open the window and show the invitation.
    fn pair(&self, arguments: &[&str]) -> Vec<Said> {
        const USAGE: &str = "type `pair NAME read|control [WORKSPACE]`, where NAME is 1 to 32 of a-z, 0-9 and -, and WORKSPACE is one of the host's --workspace directories, which can be left out where the host has one";
        let line = |text: String| vec![Said::Line(text)];
        let (name, grant, workspace) = match arguments {
            [name, grant] => (name, grant, None),
            [name, grant, workspace] => (name, grant, Some(*workspace)),
            _ => return line(USAGE.to_string()),
        };
        let grant = match *grant {
            "read" => Grant::Read,
            "control" => Grant::Control,
            _ => return line(USAGE.to_string()),
        };
        let workspace = match self.workspace_for(workspace) {
            Ok(workspace) => workspace,
            Err(refusal) => return line(format!("{refusal}; {USAGE}")),
        };
        let Some(ceiling) = (self.ceiling_of)(&workspace) else {
            return line(format!(
                "not opened: the limits of a session on {} cannot be read now; type the pair again",
                workspace.display()
            ));
        };
        let shown = limits(&ceiling);
        let opened = match self.invitations.open(Terms {
            name: (*name).to_string(),
            grant,
            workspace: workspace.clone(),
            ceiling,
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
        said.push(Said::Line(format!(
            "typing the code also approves these limits for {name}, and agrees {name} may show what a session there shows you, with what came from somewhere nobody vouched for shown as a placeholder; a session that goes past the limits stops reaching {name} until you approve it again:"
        )));
        said.extend(
            shown
                .into_iter()
                .map(|limit| Said::Line(format!("  {limit}"))),
        );
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

    /// `revoke NAME`: remove a device and end its connection.
    fn revoke(&self, arguments: &[&str]) -> String {
        let [name] = arguments else {
            return "type `revoke NAME`, with the name of a device".to_string();
        };
        match self.registry.revoke(name) {
            Ok(_) => format!("revoked {name}; its connection is ended"),
            Err(error) => format!("{error}: {name}"),
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
            .flat_map(|(key, device)| {
                let about = format!(
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
                );
                std::iter::once(about).chain(
                    limits(&device.ceiling)
                        .into_iter()
                        .map(|limit| format!("  {limit}")),
                )
            })
            .collect()
    }
}

/// The limits a ceiling sets, one line each, in plain words. A list with nothing in it is named in
/// one line at the end.
fn limits(scope: &Scope) -> Vec<String> {
    // Names every field with no `..`, so a field added to `Scope` stops the build here until it is
    // shown.
    let Scope {
        project,
        directories,
        trusted,
        distrusted,
        undecided,
        programs,
        exposed,
        mcp_servers,
        references,
        skill_directories,
        checkouts,
        language_servers,
        standing_answers,
        allow,
        ask,
        deny,
        confinement,
        approvals,
        reads_stay_in_workspace,
        auto_vetting,
        confined_lines,
    } = scope;
    let mut lines = vec![
        format!("{}: {}", Field::Project.name(), drawn(project)),
        format!(
            "{}: {}",
            Field::Confinement.name(),
            confinement_words(*confinement)
        ),
        format!(
            "{}: {}",
            Field::Approvals.name(),
            approvals_word(*approvals)
        ),
        format!(
            "{}: {}",
            Field::ReadsStayInWorkspace.name(),
            yes_or_no(*reads_stay_in_workspace)
        ),
        format!(
            "{}: {}",
            Field::AutoVetting.name(),
            on_or_off(*auto_vetting)
        ),
        format!(
            "{}: {}",
            Field::ConfinedLines.name(),
            yes_or_no(*confined_lines)
        ),
    ];
    let mut empty = Vec::new();
    for (field, keys) in [
        (Field::Directories, directories),
        (Field::Trusted, trusted),
        (Field::Distrusted, distrusted),
        (Field::Undecided, undecided),
        (Field::Programs, programs),
        (Field::Exposed, exposed),
        (Field::McpServers, mcp_servers),
        (Field::References, references),
        (Field::SkillDirectories, skill_directories),
        (Field::Checkouts, checkouts),
        (Field::LanguageServers, language_servers),
        (Field::StandingAnswers, standing_answers),
        (Field::Allow, allow),
        (Field::Ask, ask),
        (Field::Deny, deny),
    ] {
        if keys.is_empty() {
            empty.push(field.name());
        } else {
            // A line for each key, so a key cannot be drawn to look like two.
            for key in keys {
                lines.push(format!("{}: {}", field.name(), drawn(key)));
            }
        }
    }
    if !empty.is_empty() {
        lines.push(format!("none of {}", empty.join(", ")));
    }
    lines
}

/// A key of a ceiling as the host's terminal shows it: the characters of ASCII that print and a
/// space, except a backslash, and every other character written as `\u{..}`. A rule or a server's
/// name can come from a settings file in the checkout, which nobody typed on the host, so it must
/// not be able to move the cursor, rewrite the lines around it, or spell an escape it does not hold.
fn drawn(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for character in key.chars() {
        if (character.is_ascii_graphic() && character != '\\') || character == ' ' {
            out.push(character);
        } else {
            out.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        }
    }
    out
}

fn confinement_words(confinement: Confinement) -> String {
    let reads = match confinement.reads {
        Reads::Listed => "reads only what each program's profile lists",
        Reads::Machine => "reads the machine except the credential table",
        Reads::Unconfined => "reads everything",
    };
    let writes = match confinement.writes {
        Writes::Nothing => "writes nothing in the session's directories",
        Writes::Session => "writes the session's directories",
        Writes::Unconfined => "writes anywhere",
    };
    format!("{reads}, {writes}")
}

fn approvals_word(approvals: Approvals) -> &'static str {
    match approvals {
        Approvals::Plan => "plan, which writes nothing",
        Approvals::Ask => "ask, which asks before every effect",
        Approvals::AcceptEdits => "accept edits, which writes without asking",
        Approvals::Auto => "auto, which writes and runs confined programs on a classifier's word",
        Approvals::Bypass => "bypass, which asks nothing",
    }
}

fn yes_or_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn on_or_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn workspace_list(workspaces: &[PathBuf]) -> String {
    let named: Vec<String> = workspaces
        .iter()
        .map(|workspace| workspace.display().to_string())
        .collect();
    if named.is_empty() {
        "the host has none".to_string()
    } else {
        named.join(", ")
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
            ceiling: bravebot_listener::registry::narrow_ceiling("/work"),
        }
    }

    /// What the service answers `line` from a device with `grant`, in the two steps it is run in. The
    /// gate is not asked here, so a test of what a `read` device is answered asks only for a read.
    fn answered(grant: Grant, line: &str) -> Vec<String> {
        (Synthetic.prepare(line).answer)(&device(grant))
    }

    /// What a `control` device is answered, which is every device the gate lets ask anything.
    fn asked(method: &str, params: &Value) -> Value {
        let lines = answered(
            Grant::Control,
            &json!({"id": 7, "method": method, "params": params}).to_string(),
        );
        assert_eq!(lines.len(), 1);
        serde_json::from_str(&lines[0]).unwrap()
    }

    /// What a device is shown is a session that is not real, and it says so, whatever grant it has.
    #[test]
    fn a_device_is_shown_one_synthetic_session_and_told_it_is_not_real() {
        for grant in [Grant::Read, Grant::Control] {
            let lines = answered(grant, r#"{"id":1,"method":"instance.info","params":{}}"#);
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

    /// The reads the host offers a client it started, other than `instance.info` and
    /// `session.attach`, and every method it does not offer are refused for a `control` device, and
    /// none reaches a session: this service holds no way to one. A `read` device that asks one of
    /// them that needs control is ended by the gate before this answer is made.
    #[test]
    fn every_other_method_is_refused() {
        for method in [
            "agent.info",
            "changelog.read",
            "session.view.start",
            "session.view.resync",
            "turn.lookup",
            "session.list",
            "output.reply",
            "pair.begin",
            "anything-else",
        ] {
            let answer = asked(method, &json!({"session": SESSION}));
            assert_eq!(answer["error"]["code"], "not_allowed", "{method}");
            assert_eq!(answer["id"], 7);
        }
    }

    /// Each request is prepared with the access the host's allowlist gives its method, so a control
    /// is asked of the gate as a control. A line the host cannot read as a request is a control too,
    /// whatever method it names, so a line with a control's name and no number to answer under is not
    /// let through as a read.
    #[test]
    fn each_request_asks_the_gate_for_the_access_its_method_has() {
        for operation in allowlist::OFFERED {
            let line = json!({"id": 1, "method": operation.method, "params": {}}).to_string();
            assert_eq!(
                Synthetic.prepare(&line).operation,
                operation.access,
                "{}",
                operation.method
            );
        }
        for method in ["session.list", "pair.begin", "anything-else"] {
            let line = json!({"id": 1, "method": method, "params": {}}).to_string();
            assert_eq!(
                Synthetic.prepare(&line).operation,
                Access::Control,
                "{method}"
            );
        }
        for line in [
            "not json",
            r#"{"id":3}"#,
            r#"{"method":"turn.send","params":{"session":"synthetic-1"}}"#,
            r#"{"id":-1,"method":"session.close"}"#,
        ] {
            assert_eq!(Synthetic.prepare(line).operation, Access::Control, "{line}");
        }
    }

    /// A control the host offers is answered for the synthetic session as done, and changes nothing:
    /// a second `instance.info` shows the same one session. A control of another session has none,
    /// and one with a parameter the host does not offer is refused.
    #[test]
    fn a_control_is_answered_for_the_synthetic_session_and_changes_nothing() {
        let before = asked("instance.info", &json!({}));
        let opened = asked("session.new", &json!({"directory": "/anywhere"}));
        assert_eq!(opened["ok"], json!({"session": SESSION, "synthetic": true}));
        for (method, params) in [
            (
                "turn.send",
                json!({"session": SESSION, "prompt": "hi", "messageId": "m1"}),
            ),
            ("turn.cancel", json!({"session": SESSION, "target": "t1"})),
            (
                "confirm.reply",
                json!({"session": SESSION, "request": 1, "decision": "approve"}),
            ),
            (
                "run.reply",
                json!({"session": SESSION, "request": 1, "decision": "approve"}),
            ),
            (
                "fetch.reply",
                json!({"session": SESSION, "request": 1, "decision": "approve"}),
            ),
            (
                "ask.reply",
                json!({"session": SESSION, "request": 1, "answers": []}),
            ),
            ("trust.reply", json!({"session": SESSION, "trusted": true})),
            ("session.close", json!({"session": SESSION})),
        ] {
            let done = asked(method, &params);
            assert_eq!(done["ok"], json!({"synthetic": true}), "{method}");
            let mut elsewhere = params.clone();
            elsewhere["session"] = json!("another");
            assert_eq!(
                asked(method, &elsewhere)["error"]["code"],
                "no_such_session",
                "{method}"
            );
        }
        let with_files = asked(
            "turn.send",
            &json!({"session": SESSION, "prompt": "hi", "files": ["/etc/passwd"]}),
        );
        assert_eq!(with_files["error"]["code"], "not_allowed");
        assert_eq!(asked("instance.info", &json!({})), before);
    }

    /// For a `control` device, the only one the gate lets send a line that is not a request.
    #[test]
    fn a_line_that_cannot_be_answered_gets_no_answer_and_one_that_can_gets_a_failure() {
        assert!(answered(Grant::Control, "not json").is_empty());
        assert!(answered(Grant::Control, r#"{"method":"instance.info"}"#).is_empty());
        let lines = answered(Grant::Control, r#"{"id":3}"#);
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

    fn link_over(workspaces: &[PathBuf]) -> Link {
        Link::start(
            "127.0.0.1:0".parse().unwrap(),
            workspaces,
            false,
            Arc::new(|_| {}),
            Arc::new(|workspace: &Path| {
                Some(bravebot_listener::registry::narrow_ceiling(
                    &workspace.display().to_string(),
                ))
            }),
        )
        .unwrap()
    }

    fn said_lines(said: Vec<Said>) -> Vec<String> {
        said.into_iter()
            .filter_map(|said| match said {
                Said::Line(line) => Some(line),
                Said::Block(_) => None,
            })
            .collect()
    }

    /// A host with one workspace pairs a device on it without being told which, and a host with
    /// several asks, and names them.
    #[test]
    fn the_workspace_can_be_left_out_only_where_the_host_has_one() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_path = first.path().canonicalize().unwrap();

        let one = link_over(&[first.path().to_path_buf()]);
        let said = said_lines(one.command("pair kitchen-phone read").unwrap());
        assert!(
            said[0].starts_with(&format!(
                "pairing kitchen-phone with read on {}",
                first_path.display()
            )),
            "{said:?}"
        );
        one.stop();

        let two = link_over(&[first.path().to_path_buf(), second.path().to_path_buf()]);
        let said = said_lines(two.command("pair kitchen-phone read").unwrap());
        assert_eq!(said.len(), 1);
        assert!(said[0].starts_with("name a workspace: "), "{said:?}");
        assert!(said[0].contains(&first_path.display().to_string()));
        assert!(said[0].contains(&second.path().canonicalize().unwrap().display().to_string()));
        two.stop();
    }

    /// A directory that is not one of the host's is refused with the ones that are.
    #[test]
    fn a_workspace_the_host_was_not_given_is_refused_with_the_ones_it_has() {
        let given = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let link = link_over(&[given.path().to_path_buf()]);
        let said = said_lines(
            link.command(&format!(
                "pair kitchen-phone read {}",
                other.path().display()
            ))
            .unwrap(),
        );
        assert_eq!(said.len(), 1);
        assert!(
            said[0].contains("is not one of the host's workspaces: "),
            "{said:?}"
        );
        assert!(said[0].contains(&given.path().canonicalize().unwrap().display().to_string()));
        link.stop();
    }

    fn link_with(workspace: &Path, ceiling_of: CeilingOf) -> Link {
        Link::start(
            "127.0.0.1:0".parse().unwrap(),
            &[workspace.to_path_buf()],
            false,
            Arc::new(|_| {}),
            ceiling_of,
        )
        .unwrap()
    }

    /// A ceiling with something in every list, so each one is shown.
    fn full_ceiling() -> Scope {
        let mut ceiling = bravebot_listener::registry::narrow_ceiling("\"/work\"");
        for (at, list) in [
            &mut ceiling.directories,
            &mut ceiling.trusted,
            &mut ceiling.distrusted,
            &mut ceiling.undecided,
            &mut ceiling.programs,
            &mut ceiling.exposed,
            &mut ceiling.mcp_servers,
            &mut ceiling.references,
            &mut ceiling.skill_directories,
            &mut ceiling.checkouts,
            &mut ceiling.language_servers,
            &mut ceiling.standing_answers,
            &mut ceiling.allow,
            &mut ceiling.ask,
            &mut ceiling.deny,
        ]
        .into_iter()
        .enumerate()
        {
            list.push(format!("key-{at}"));
        }
        ceiling
    }

    /// Every field of the ceiling is shown with what it holds, each under the name a suspension
    /// gives it, so the person approves limits they have seen. A field added to `Scope` stops the
    /// build in `limits` until it is shown, and this fails where one is shown under no name.
    #[test]
    fn every_field_of_a_ceiling_is_shown_under_its_name() {
        let shown = limits(&full_ceiling()).join("\n");
        for field in Field::ALL {
            assert!(
                shown.contains(field.name()),
                "{} is not shown",
                field.name()
            );
        }
        for at in 0..15 {
            assert!(
                shown.contains(&format!("key-{at}")),
                "key-{at} is not shown"
            );
        }
        assert!(!shown.contains("none of"), "{shown}");
        let narrow = limits(&bravebot_listener::registry::narrow_ceiling("\"/work\"")).join("\n");
        assert!(
            narrow.contains("none of the open directories, the trusted paths"),
            "{narrow}"
        );
        assert!(
            narrow.contains("the permission mode: plan, which writes nothing"),
            "{narrow}"
        );
    }

    /// A key that holds a control character, as a rule a checkout's settings file wrote can, is
    /// drawn with it written out, so it cannot move the cursor or start a line of its own. A
    /// backslash is written out too, so a key cannot spell an escape it does not hold, and a key
    /// that holds a comma is one line, so it cannot look like two keys.
    #[test]
    fn a_key_in_a_ceiling_is_drawn_without_anything_that_could_rewrite_the_line() {
        let mut ceiling = bravebot_listener::registry::narrow_ceiling("\"/work\"");
        ceiling.allow = vec!["run(x)\u{1b}[2J\nyou approved bypass".into()];
        let shown = limits(&ceiling);
        assert!(
            shown
                .iter()
                .all(|line| !line.contains('\u{1b}') && !line.contains('\n'))
        );
        assert!(
            shown
                .iter()
                .any(|line| line.contains("run(x)\\u{1b}[2J\\u{a}you approved bypass"))
        );
        assert!(
            shown.iter().any(|line| line.contains("\"/work\"")),
            "{shown:?}"
        );

        ceiling.allow = vec!["x, Read(/notes/**)".into(), "a literal \\u{a}".into()];
        let shown = limits(&ceiling);
        let allowed: Vec<&String> = shown
            .iter()
            .filter(|line| line.starts_with("the allow rules: "))
            .collect();
        assert_eq!(allowed.len(), 2, "{shown:?}");
        assert!(
            allowed
                .iter()
                .any(|line| line.ends_with(": x, Read(/notes/**)"))
        );
        assert!(
            allowed
                .iter()
                .any(|line| line.ends_with(": a literal \\u{5c}u{a}")),
            "{shown:?}"
        );
    }

    /// `pair` shows the limits a new session in the workspace would have, says that typing the code
    /// approves them, and a device paired from that window holds them, which `devices` shows.
    #[test]
    fn pairing_shows_the_limits_the_code_approves_and_devices_lists_them() {
        let workspace = tempfile::tempdir().unwrap();
        let link = link_with(workspace.path(), Arc::new(|_: &Path| Some(full_ceiling())));
        let said = said_lines(link.command("pair kitchen-phone read").unwrap());
        let text = said.join("\n");
        assert!(
            text.contains("typing the code also approves these limits for kitchen-phone"),
            "{text}"
        );
        assert!(
            text.contains("agrees kitchen-phone may show what a session there shows you"),
            "{text}"
        );
        assert!(text.contains("  the programs vouched for: key-4"), "{text}");
        let before_link = text.split("or type this link").next().unwrap();
        assert!(
            before_link.contains("the sandbox mode:"),
            "the limits come after the link: {text}"
        );

        link.registry.admit([7; 32], device(Grant::Read)).unwrap();
        let listed = link.devices().join("\n");
        assert!(
            listed.contains("kitchen-phone read /work not connected"),
            "{listed}"
        );
        assert!(
            listed.contains("  the project directory: /work"),
            "{listed}"
        );
    }

    /// Where the limits of a session in the workspace cannot be read, no window opens, since a
    /// device would otherwise be paired with limits nobody was shown.
    #[test]
    fn no_window_opens_where_the_limits_cannot_be_read() {
        let workspace = tempfile::tempdir().unwrap();
        let link = link_with(workspace.path(), Arc::new(|_: &Path| None));
        let said = said_lines(link.command("pair kitchen-phone read").unwrap());
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with("not opened: the limits of a session on"),
            "{said:?}"
        );
        assert!(!link.invitations.is_open());
    }
}
