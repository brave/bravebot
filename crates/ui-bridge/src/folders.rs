// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The folders a session has open beside its project, and opening and closing one mid-session, as
//! the terminal's `/add-dir` and `/add-dir close` do (TRUST-9).
//!
//! One routine opens a folder, so every way a desktop session comes to open one opens it on the
//! same terms: a folder the person picks, and a yes to a directory a settings file named.
//!
//! Every decision is made here. The desktop's main process calls `session.folders.add` only with
//! the folder its own native picker returned, and a renderer cannot call it at all; whatever path
//! arrives is still checked as if anybody had typed it.

use super::{Bridge, rewind_json, rules_json, save, turn_workspace};
use crate::protocol::{ErrorCode, Failure, Request};
use crate::running::State;
use bravebot_agent::Workspace;
use bravebot_agent::workspace::WorkspaceError;
use bravebot_config::Settings;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

impl Bridge {
    /// `session.folders.list`, `session.folders.add` and `session.folders.close`.
    pub(super) fn folders(&mut self, request: &Request) -> Result<Value, Failure> {
        let handle = request.string("session")?;
        self.reap(&handle);
        match request.method.as_str() {
            "session.folders.add" => self.add_folder(&handle, &request.string("path")?),
            "session.folders.close" => self.close_folder(&handle, &request.string("path")?),
            _ => self.list_folders(&handle),
        }
    }

    /// The folders open beside the project, in the order they were opened.
    ///
    /// Answered while a turn runs, from the list kept on the session, since the worker holds the
    /// state and dispatch must not wait for it. Nothing but an add or a close here changes the
    /// list, and both are refused while a turn runs, so the kept list is the one the turn has.
    fn list_folders(&mut self, handle: &str) -> Result<Value, Failure> {
        let open = self
            .open
            .get_mut(handle)
            .ok_or_else(Failure::no_such_session)?;
        if let Ok(state) = open.state.try_lock() {
            open.folders.clone_from(&state.directories);
        }
        Ok(json!({
            "folders": folders_json(&open.folders),
            "busy": open.running.is_some(),
        }))
    }

    /// Open `path` beside the project and vouch for it, for the rest of the session, as `/add-dir`
    /// does: reachable, and trusted in the session's map, which the record keeps.
    ///
    /// Refused unless the path is absolute and names an existing directory outside the project
    /// that is not open already, under the same refusals `/add-dir` meets in the workspace. Refused
    /// too while a turn runs, since the turn holds the state, and before the trust question is
    /// answered, since that answer replaces the map this would write the folder's rule into.
    fn add_folder(&mut self, handle: &str, path: &str) -> Result<Value, Failure> {
        let (settings, confined) = {
            let open = self.open.get(handle).ok_or_else(Failure::no_such_session)?;
            open.ready_for_work().map_err(|failure| {
                if failure.code == ErrorCode::TurnInFlight {
                    Failure::new(
                        ErrorCode::TurnInFlight,
                        "Wait for the current turn to finish before adding a folder.",
                    )
                } else {
                    failure
                }
            })?;
            (
                crate::settings::layers(Some(&open.project), self.settings.as_deref()),
                open.reads_stay_in_workspace,
            )
        };
        // Said here rather than left to the workspace, so the refusal names what was wrong with
        // the request and not a path resolved against wherever this process happens to stand.
        if !Path::new(path).is_absolute() {
            return Err(Failure::bad_request(format!(
                "{path} must be an absolute path"
            )));
        }
        let open = self
            .open
            .get_mut(handle)
            .ok_or_else(Failure::no_such_session)?;
        let project = open.project.clone();
        let mut guard = open
            .state
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "session state is poisoned"))?;
        let state = &mut *guard;
        let mut workspace = folders_workspace(&project, &settings, confined, state)?;
        let canonical = workspace
            .resolve_directory(path)
            .map_err(|error| Failure::bad_request(error.to_string()))?;
        if state.directories.contains(&canonical) {
            return Err(Failure::bad_request(format!(
                "{} is already open in this chat",
                canonical.display()
            )));
        }
        let added = open_in(&mut workspace, state, &canonical.display().to_string())
            .map_err(|error| Failure::bad_request(error.to_string()))?;
        // A point holds a trust map without this folder's rule, while the folder would stay open.
        state.rewind.close();
        save_if_recorded(&project, state);
        open.folders.clone_from(&state.directories);
        Ok(json!({
            "folder": added.display().to_string(),
            "folders": folders_json(&state.directories),
            // CHECKOUT-7: a folder that holds the project means no delegate gets a checkout while
            // it is open, which the terminal says as it opens one.
            "endsCheckouts": workspace.ends_checkouts(&added),
            "trust": { "rules": rules_json(&state.answers.trust) },
            "rewind": rewind_json(state, &project),
        }))
    }

    /// Close a folder this session opened beside the project, and withdraw the trust it was opened
    /// with, as `/add-dir close` does (TRUST-9). `path` is the name it is listed under, or any
    /// absolute name that resolves to it now.
    ///
    /// Refused while a turn runs. Not refused before the trust question is answered: closing only
    /// takes reach away.
    fn close_folder(&mut self, handle: &str, path: &str) -> Result<Value, Failure> {
        let (settings, confined) = {
            let open = self.open.get(handle).ok_or_else(Failure::no_such_session)?;
            if open.running.is_some() {
                return Err(Failure::new(
                    ErrorCode::TurnInFlight,
                    "Wait for the current turn to finish before closing a folder.",
                ));
            }
            (
                crate::settings::layers(Some(&open.project), self.settings.as_deref()),
                open.reads_stay_in_workspace,
            )
        };
        if !Path::new(path).is_absolute() {
            return Err(Failure::bad_request(format!(
                "{path} must be an absolute path"
            )));
        }
        let open = self
            .open
            .get_mut(handle)
            .ok_or_else(Failure::no_such_session)?;
        let project = open.project.clone();
        let mut guard = open
            .state
            .lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "session state is poisoned"))?;
        let state = &mut *guard;
        let mut workspace = folders_workspace(&project, &settings, confined, state)?;
        // Matched against the list itself first, so a folder that has since been deleted or moved,
        // which the workspace could not reopen, can still be closed under the name it is listed by.
        let listed = state
            .directories
            .iter()
            .position(|open| open == Path::new(path))
            .map(|at| state.directories[at].clone());
        // A folder listed under that name is closed by taking it off the list, which is what every
        // later turn opens: the workspace here only resolves another name to one listed, and is not
        // kept, so there is nothing in it to close.
        let closed = match listed {
            Some(closed) => closed,
            None => workspace
                .close_added_directory(path)
                .map_err(|error| Failure::bad_request(error.to_string()))?,
        };
        state.directories.retain(|open| *open != closed);
        let withdrawn = state
            .answers
            .trust
            .withdraw_trust(&bravebot_agent::workspace::key_of(&closed));
        // A point holds a trust map with this folder's rule, while the folder would stay closed.
        state.rewind.close();
        // Written now rather than at the end of the next turn: a chat that closed a folder and
        // was then closed would otherwise resume with it open and trusted.
        save_if_recorded(&project, state);
        open.folders.clone_from(&state.directories);
        Ok(json!({
            "folder": closed.display().to_string(),
            "withdrawn": withdrawn,
            "folders": folders_json(&state.directories),
            "trust": { "rules": rules_json(&state.answers.trust) },
            "rewind": rewind_json(state, &project),
        }))
    }
}

/// Open `typed`, a name as a settings file wrote it, in `workspace` and record it in `state`, as
/// [`open_in`] does. Returns the directory opened, canonical.
///
/// `typed` is read against the project, with a leading `~` the home directory, as the terminal
/// reads it. A name the workspace refuses (one inside the project already, a credential location,
/// one the settings keep reads out of, or one that does not exist) opens nothing and records
/// nothing.
pub(super) fn open_folder(
    workspace: &mut Workspace,
    state: &mut State,
    typed: &str,
) -> Result<PathBuf, WorkspaceError> {
    let named = bravebot_agent::opening::named_directory(
        workspace.root(),
        typed,
        bravebot_agent::home::profile().as_deref(),
    )?;
    open_in(workspace, state, &named)
}

/// Open `directory` in `workspace` on the terms `/add-dir` opens one (TRUST-9), and record it in
/// `state`: in its added directories, once, so every later turn, a rewind and a resume open it
/// again, and in its trust map as vouched for. Returns the directory opened, canonical.
fn open_in(
    workspace: &mut Workspace,
    state: &mut State,
    directory: &str,
) -> Result<PathBuf, WorkspaceError> {
    let added = workspace.add_directory(directory)?;
    if !state.directories.contains(&added) {
        state.directories.push(added.clone());
    }
    state
        .answers
        .trust
        .trust(&bravebot_agent::workspace::key_of(&added));
    Ok(added)
}

/// The workspace a turn would open now, with the folders the session already has open, so the
/// refusals of a folder that overlaps one are the ones a turn would meet.
///
/// `confined` is whether the session's file tools are held to the project (PERM-16), as its turns
/// are.
fn folders_workspace(
    project: &Path,
    settings: &Settings,
    confined: bool,
    state: &State,
) -> Result<Workspace, Failure> {
    let mut workspace = turn_workspace(project.to_path_buf(), settings, confined)
        .map_err(|error| Failure::new(ErrorCode::Internal, error.to_string()))?;
    workspace.open_scratch(state.scratch.directory());
    for directory in &state.directories {
        workspace
            .add_directory(&directory.display().to_string())
            .ok();
    }
    Ok(workspace)
}

/// Save the record where there is one, which a session that has had no turn yet does not have:
/// an opened-and-abandoned chat leaves nothing behind, whatever it opened.
fn save_if_recorded(project: &Path, state: &mut State) {
    if state.handle.is_some() {
        let turns = state.turns;
        save(
            project,
            state,
            turns,
            &bravebot_session::audit::Trail::default(),
        );
    }
}

pub(super) fn folders_json(folders: &[PathBuf]) -> Vec<String> {
    folders
        .iter()
        .map(|folder| folder.display().to_string())
        .collect()
}
