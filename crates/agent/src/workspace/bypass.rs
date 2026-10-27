//! What the file tools reach outside the workspace while the permission mode is bypass (MODE-4,
//! PATHREQ-7).
//!
//! The reach is not an opened directory. Nothing is added to the directories `/add-dir` keeps, no
//! trust rule is written, and nothing is carried by a resume: each tool call is handed the
//! decision again by whoever dispatches it, from the live mode, so the reach ends when the key
//! leaves bypass. The decision reads a path, the person's rules and facts about the path. It reads
//! no byte of a file and no program output.

use super::{Workspace, locked};
use bravebot_sandbox::rules::{Rules, judged_request};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The facts a call's reach outside the workspace is decided from, handed over by whoever
/// dispatches the call and held for that call alone.
#[derive(Debug, Clone)]
pub struct BypassReach {
    /// The tool the call is of, as the trail names it.
    pub tool: String,
    /// The field of that tool the path is named in.
    pub field: &'static str,
    /// The name of the permission mode that gave the reach.
    pub mode: &'static str,
    /// The user's profile directory, which the refusals about the home directory are judged
    /// against.
    pub profile: Option<PathBuf>,
    /// The person's `denyRead` and `denyWrite` lists, resolved.
    pub person: Rules,
    /// Directories no path of this call reaches, which a delegate in a checkout holds: the working
    /// directory and every other checkout (CHECKOUT-7).
    pub excluded: Vec<PathBuf>,
    /// Whether the call writes the path it names.
    pub write: bool,
}

/// A directory the file tools were first taken into by a bypass session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    /// The directory, as the path resolved where it leads.
    pub directory: PathBuf,
    /// The permission mode that gave the reach.
    pub mode: &'static str,
    /// The tool that named the path, and the field it named it in.
    pub tool: String,
    pub field: &'static str,
}

/// The reach the call in progress holds, copied and not shared when a workspace is cloned, so a
/// delegate's call cannot hand the lead session's call its reach or take it away.
#[derive(Debug, Default)]
pub(super) struct CallReach(Mutex<Option<BypassReach>>);

impl Clone for CallReach {
    fn clone(&self) -> Self {
        Self(Mutex::new(locked(&self.0).clone()))
    }
}

/// The directories bypass has taken the file tools into, in order, and the ones not yet reported.
#[derive(Debug, Default)]
pub(super) struct ReachedDirectories {
    all: Vec<Reached>,
    unreported: Vec<Reached>,
}

impl Workspace {
    /// Hold `reach` for the call about to be made, or none where the call has none. Set before every
    /// call and cleared after it, so a call that was not given the reach never finds one left by
    /// an earlier call.
    pub fn hold_bypass_reach(&self, reach: Option<BypassReach>) {
        *locked(&self.call_reach.0) = reach;
    }

    /// The directories bypass took the file tools into for the first time since the last call to
    /// this, for the caller to put in the trail.
    pub fn take_new_reaches(&self) -> Vec<Reached> {
        std::mem::take(&mut locked(&self.kept.reached).unreported)
    }

    /// Every directory bypass has taken the file tools into this session, oldest first.
    pub fn reached_directories(&self) -> Vec<Reached> {
        locked(&self.kept.reached).all.clone()
    }

    /// Whether the call in progress may name `resolved`, a path that is outside the workspace and
    /// every opened directory, because the permission mode reaches it.
    ///
    /// `resolved` is where the path leads, so a link is judged where it ends. A path that does not
    /// exist yet is judged by the nearest directory that does, since a write creates what it names:
    /// a new entry directly in a directory the refusals cover is refused with the directory.
    pub(super) fn bypass_reaches(&self, resolved: &Path, write: Option<bool>) -> bool {
        if self.reads_stay_inside {
            return false;
        }
        let Some(reach) = locked(&self.call_reach.0).clone() else {
            return false;
        };
        if reach.excluded.iter().any(|held| resolved.starts_with(held)) {
            return false;
        }
        // The path that was named, and not the directory it is judged by below, which has lost any
        // part a wildcard or a control character was in (PATHREQ-3).
        let spelled = resolved.to_string_lossy();
        if spelled.contains(['*', '?']) || spelled.chars().any(char::is_control) {
            return false;
        }
        let write = write.unwrap_or(reach.write);
        let mut judged = resolved;
        while !judged.exists() {
            match judged.parent() {
                Some(parent) => judged = parent,
                None => return false,
            }
        }
        if judged_request(judged, write, reach.profile.as_deref(), &reach.person).is_err() {
            return false;
        }
        let directory = if resolved.is_dir() {
            resolved
        } else {
            resolved.parent().unwrap_or(resolved)
        };
        let mut held = locked(&self.kept.reached);
        if !held
            .all
            .iter()
            .any(|seen| directory.starts_with(&seen.directory))
        {
            let first = Reached {
                directory: directory.to_path_buf(),
                mode: reach.mode,
                tool: reach.tool,
                field: reach.field,
            };
            held.all.push(first.clone());
            held.unreported.push(first);
        }
        true
    }

    /// The directories no call of a delegate in a checkout reaches: the working directory its
    /// parent had and every checkout the session made. Empty for a workspace that is not a
    /// checkout (CHECKOUT-7).
    pub fn checkout_exclusions(&self) -> Vec<PathBuf> {
        let Some(info) = self.checkout() else {
            return Vec::new();
        };
        let mut held = vec![info.source.root.clone()];
        held.extend(locked(&self.kept.checkouts).iter().cloned());
        held.extend(
            locked(&self.kept.session_checkouts)
                .iter()
                .map(|made| made.path.clone()),
        );
        held.retain(|dir| *dir != self.root);
        held.sort();
        held.dedup();
        held
    }
}
