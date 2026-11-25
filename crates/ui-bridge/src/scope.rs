// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The scope an open session holds now, as the sources of docs/design/mobile/architecture.md
//! ("Scope sources today") bound it, in the terms of [`bravebot_core::scope`].
//!
//! Nothing here decides who sees what. It reads the session and the settings in force and returns
//! the value a ceiling is compared with.

use super::Open;
use crate::rules::SettingsRules;
use crate::running::Mcp;
use bravebot_agent::permission_mode::PermissionMode;
use bravebot_agent::servers::Home;
use bravebot_config::{Managed, Settings};
use bravebot_core::label::Integrity;
use bravebot_core::scope::{Approvals, Confinement, Reads, Scope, Writes};
use bravebot_sandbox::SandboxMode;
use std::path::Path;

/// The scope `open` holds under `settings` and `managed`, or `None` while a turn holds the
/// session's state, which is where the trust map, the programs and the rules are.
///
/// `None` is not an empty scope. An empty one reads as narrower than any ceiling, so a caller that
/// took the absence of an answer for one would let a device keep what it could not check.
///
/// `home` is the state directory the plugins are read from.
///
/// `settings` is what the next turn reads. A settings edit changes the sandbox mode, the
/// references, the skill directories and, until the session has started its servers, the servers
/// asked for and the plugins carry. The permission rules, the trust map and the programs are the
/// session's own (PERM-12).
///
/// The caps, deadlines and effort the settings also set bound size, time and spend and are not in
/// the scope. A server is keyed by its alias and the command under it is not compared. A declared
/// server's command changes only by the person's edit on the host. A plugin's server changes with
/// an update the person accepts on the host, and the updated server does not start until a person
/// answers its first-use question again (PLUGIN-7, PLUGIN-9).
///
/// The files are read before the session's state is locked, so a turn waiting for the state does
/// not wait on the disk.
pub(super) fn scope_of(
    open: &Open,
    settings: &Settings,
    managed: &Managed,
    home: &Home,
) -> Option<Scope> {
    // The session writes and honours these under its resolved project, and under the project as
    // given where that cannot be resolved, so both spellings are read.
    let resolved = open
        .project
        .canonicalize()
        .unwrap_or_else(|_| open.project.clone());
    let standing_answers = home
        .directory
        .as_deref()
        .map(|directory| {
            let standing = bravebot_config::mcp::Standing::read(directory);
            let mut answers = standing.in_project(&resolved);
            answers.extend(standing.in_project(&open.project));
            answers
        })
        .unwrap_or_default();
    // What a first start reaches: what the settings request and, where they request any, the
    // servers installed plugins carry. Used only while the session has not started its servers.
    let first_start: Vec<String> = match bravebot_agent::servers::requested_by(settings).is_empty()
    {
        true => Vec::new(),
        false => bravebot_agent::servers::requested_for_a_first_start(settings, home)
            .into_iter()
            .map(|(_, alias)| alias)
            .collect(),
    };
    let state = open.state.try_lock().ok()?;
    let mut trusted = Vec::new();
    let mut distrusted = Vec::new();
    let mut undecided = Vec::new();
    for (path, decision) in state.answers.trust.keyed() {
        let list = match decision {
            Some(Integrity::Trusted) => &mut trusted,
            Some(Integrity::Untrusted) => &mut distrusted,
            None => &mut undecided,
        };
        list.push(path.to_string());
    }
    let programs = state
        .answers
        .programs
        .iter()
        .map(|command| format!("{command:?}"))
        .collect();
    let exposed = state
        .answers
        .exposed
        .agreed_reads()
        .map(|path| format!("read {path}"))
        .chain(
            state
                .answers
                .exposed
                .agreed_creations()
                .map(|path| format!("create {}", key(path))),
        )
        .collect();
    let rules = &state.answers.rules;
    let directories = state.directories.iter().map(|path| key(path)).collect();
    let checkouts = state
        .kept
        .as_ref()
        .map(|kept| kept.checkouts())
        .unwrap_or_default()
        .iter()
        .map(|checkout| key(&checkout.path))
        .collect();
    let language_servers = match &state.answers.servers {
        Some(servers) => servers.started_programs()?,
        None => Vec::new(),
    };
    // The servers a session can call are the ones it started, once, on its first turn or plan run.
    // Until then the first one starts `first_start`, so that is what the session will hold.
    let mcp_servers = match &state.mcp {
        Mcp::Started(Some(started)) => started.aliases(),
        Mcp::Started(None) => Vec::new(),
        Mcp::Unstarted => first_start,
    };
    let references = settings
        .references()
        .iter()
        .map(|reference| reference.path.clone())
        .collect();
    let skill_directories = settings.shared_skill_directories().to_vec();
    let mode = bravebot_config::sandbox::for_a_window_choosing(open.sandbox, settings, managed);
    Some(Scope {
        project: key(&open.project),
        directories: distinct(directories),
        trusted: distinct(trusted),
        distrusted: distinct(distrusted),
        undecided: distinct(undecided),
        programs: distinct(programs),
        exposed: distinct(exposed),
        mcp_servers: distinct(mcp_servers),
        references: distinct(references),
        skill_directories: distinct(skill_directories),
        checkouts: distinct(checkouts),
        language_servers: distinct(language_servers),
        standing_answers: distinct(standing_answers),
        allow: distinct(rule_keys(rules, "allow", &rules.allow)),
        ask: distinct(rule_keys(rules, "ask", &rules.ask)),
        deny: distinct(rule_keys(rules, "deny", &rules.deny)),
        confinement: confinement_of(mode),
        approvals: approvals_of(open.permission_mode.get()),
        reads_stay_in_workspace: open.reads_stay_in_workspace,
        auto_vetting: open.auto_vetting,
        confined_lines: state.answers.trust.allows_confined_lines(),
    })
}

/// The key of a path: the path as the operating system holds it, so two names that differ in bytes
/// that are not text are two keys.
pub(super) fn key(path: &Path) -> String {
    format!("{path:?}")
}

/// The keys of the rules in `list`, one for each file that wrote a rule. The rule in force is
/// spelled as the layer that wrote it anchored it, and the file is part of the key so that two
/// files that wrote one spelling are two grants. A rule no file wrote, such as a checkout's rule
/// the person granted, is keyed `granted`.
fn rule_keys(rules: &SettingsRules, list: &str, written: &[String]) -> Vec<String> {
    written
        .iter()
        .flat_map(|rule| {
            let files = rules.sources.files_for(list, rule);
            match files.is_empty() {
                true => vec![format!("{rule} @ granted")],
                false => files
                    .into_iter()
                    .map(|file| format!("{rule} @ {}", key(file)))
                    .collect(),
            }
        })
        .collect()
}

/// The keys of `keys`, each once, in order.
fn distinct(mut keys: Vec<String>) -> Vec<String> {
    keys.sort();
    keys.dedup();
    keys
}

/// What a program held to `mode` reads and writes on this platform.
pub(super) fn confinement_of(mode: SandboxMode) -> Confinement {
    confinement_on(mode, cfg!(windows))
}

/// What a program held to `mode` reads and writes, on Windows or elsewhere, ranked as
/// `SandboxMode::looser_than_on` ranks the modes (SANDBOX-22). On Windows no mode but `off` reads
/// the machine, so `read-only` reads what `strict` reads and is no looser than it. `workspace-write`
/// is given the machine's reads there only to keep the place the sandbox ranks it in, looser than
/// both, and the value does not say what it reads.
pub(super) fn confinement_on(mode: SandboxMode, windows: bool) -> Confinement {
    let (reads, writes) = match (mode, windows) {
        (SandboxMode::Strict, _) => (Reads::Listed, Writes::Session),
        (SandboxMode::ReadOnly, false) => (Reads::Machine, Writes::Nothing),
        (SandboxMode::ReadOnly, true) => (Reads::Listed, Writes::Nothing),
        (SandboxMode::WorkspaceWrite, _) => (Reads::Machine, Writes::Session),
        (SandboxMode::Off, _) => (Reads::Unconfined, Writes::Unconfined),
    };
    Confinement { reads, writes }
}

/// Where `mode` stands among the modes by how much a session does unasked.
pub(super) fn approvals_of(mode: PermissionMode) -> Approvals {
    match mode {
        PermissionMode::Plan => Approvals::Plan,
        PermissionMode::Ask => Approvals::Ask,
        PermissionMode::AcceptEdits => Approvals::AcceptEdits,
        PermissionMode::Auto => Approvals::Auto,
        PermissionMode::Bypass => Approvals::Bypass,
    }
}
