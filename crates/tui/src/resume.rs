//! Picking up a session again.
//!
//! The list to choose from is the Sessions screen (SCREEN-1): `--resume` and `--from-pr` open it over
//! a new session, `/resume` opens it over the one being run, and leaving it by Escape stays in the
//! session. What is here is what a session named outright is checked against before it is carried
//! on, and what is said when it cannot be.

use bravebot_i18n::t;
use bravebot_session::sessions;
use std::path::Path;

/// Why a manifest run cannot be picked up.
///
/// A session is turns over one conversation and a manifest run has none: the planner is never
/// shown a result, so there is nothing for a later turn to continue. The record is written all
/// the same, because what a run produced is worth reading whether or not it can be resumed.
pub fn manifest_note() -> &'static str {
    t!(resume_manifest_run)
}

/// Why `/resume` carried on from no record.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The id is not shaped like a session's name, or names no record of this directory.
    NoSuchSession,
    /// The id is the session being run.
    AlreadyHere,
    /// The record is a manifest run, which has no conversation to continue.
    Manifest,
    /// A running background session holds the record, so a second writer would fork it (BG-9).
    Held,
    /// Another process holds the record, which one process writes at a time (SESSION-38).
    HeldByAnother,
}

impl Refusal {
    /// What to say in the transcript. The id typed is never part of it, since it may hold an escape.
    pub fn note(&self) -> String {
        match self {
            Self::NoSuchSession => t!(session_resume_no_such).to_string(),
            Self::AlreadyHere => t!(session_resume_already_here).to_string(),
            Self::Manifest => manifest_note().to_string(),
            Self::Held => t!(session_resume_held_by_background).to_string(),
            Self::HeldByAnother => t!(session_resume_held_by_another_process).to_string(),
        }
    }
}

/// The record `/resume <id>` names, read as `--resume <id>` reads it.
///
/// The id is checked for the shape of a session's name before it reaches a path.
pub fn named(project: &Path, current: &str, typed: &str) -> Result<Box<sessions::Record>, Refusal> {
    if typed == current {
        return Err(Refusal::AlreadyHere);
    }
    sessions::is_a_session_name(typed)
        .then(|| sessions::load(project, typed))
        .flatten()
        .map(Box::new)
        .ok_or(Refusal::NoSuchSession)
        .and_then(|record| continuable(project, record))
}

/// The record, if it is one that can be carried on from: neither a manifest run nor one a running
/// background session or another process holds. The picker refuses the first on Enter; the others
/// are checked here because a record is picked up in this process and not another.
pub fn continuable(
    project: &Path,
    record: Box<sessions::Record>,
) -> Result<Box<sessions::Record>, Refusal> {
    continuable_unless(record, bravebot_session::jobs::is_running, |id| {
        sessions::is_open_elsewhere(project, id)
    })
}

/// [`continuable`], with the questions of whether a running background session holds an id and
/// whether another process does put by the caller, so that the refusals can be reached without
/// either.
pub fn continuable_unless(
    record: Box<sessions::Record>,
    held: impl Fn(&str) -> bool,
    open: impl Fn(&str) -> bool,
) -> Result<Box<sessions::Record>, Refusal> {
    if record.manifest.is_some() {
        return Err(Refusal::Manifest);
    }
    if held(&record.id) {
        return Err(Refusal::Held);
    }
    if open(&record.id) {
        return Err(Refusal::HeldByAnother);
    }
    Ok(record)
}
