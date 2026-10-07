//! A one-shot run's session record, and continuing one (CLI-25).

use bravebot_agent::conversation::Conversation;
use bravebot_agent::turn::Outcome;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_session::sessions::{Continuation, Front, Handle, Record, Standing};
use std::collections::BTreeMap;
use std::path::Path;

/// Which earlier session a one-shot run was asked to carry on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resume {
    /// The session with this id, as `--resume <id>` names it.
    Id(String),
    /// The most recent session in this directory, as `--continue` does.
    Latest,
}

/// The record a run was asked to continue, or why nothing can be.
///
/// Refused rather than answered by starting a fresh conversation: a script that asked to carry on
/// and was given an empty context would answer the follow-up as though nothing had come before it.
/// A manifest run has no conversation to carry on from, and a session another process is running is
/// written by that process, so neither is read (SESSION-10, BG-9).
pub fn find(directory: &Path, asked: &Resume) -> Result<Record, String> {
    let id = match asked {
        Resume::Id(id) => id.clone(),
        Resume::Latest => bravebot_session::sessions::most_recent(directory)
            .map(|session| session.id)
            .ok_or_else(|| t!(cli_nothing_to_continue).to_string())?,
    };
    let record = bravebot_session::sessions::load(directory, &id)
        .ok_or_else(|| t!(cli_no_such_session, id = id.as_str()).to_string())?;
    if record.manifest.is_some() {
        return Err(bravebot_tui::resume::manifest_note().to_string());
    }
    if crate::background::is_running(&record.id) {
        return Err(crate::background::held_by_a_background_session(&record.id));
    }
    Ok(record)
}

/// What a finished turn leaves to be written down.
pub struct Finished<'a> {
    pub prompt: &'a str,
    pub conversation: &'a Conversation,
    pub outcome: &'a Outcome,
    pub trust: &'a TrustStore,
    /// The length of the recounted conversation before the turn, and where the prompt entered it.
    pub begins: usize,
    pub prompt_at: Option<usize>,
}

/// Write the turn down, as a new session or as one more turn of the one it continued.
///
/// The id of the record that was written, and `None` where nothing was: an incognito run writes
/// none, and a record that cannot be saved does not fail a run that has already answered, as with
/// every other writer under the state directory.
pub fn keep(root: &Path, previous: Option<Record>, turn: Finished<'_>) -> Option<String> {
    let snapshot = turn.conversation.snapshot();
    let ends = turn.conversation.recounted().len();
    match previous {
        Some(record) => {
            let mut handle =
                Handle::resuming(root, &record, Front::Terminal, bravebot_stamp::BUILD);
            let written = handle.save_continuation(
                record,
                Continuation {
                    conversation: &snapshot,
                    prompt: turn.prompt,
                    tokens: turn.outcome.tokens,
                    model: &turn.outcome.model,
                    trust: turn.trust,
                    begins: turn.begins,
                    ends,
                    prompt_at: turn.prompt_at,
                },
            );
            written.then(|| handle.id().to_string())
        }
        None => {
            let mut handle = Handle::begin(root, Front::Terminal, bravebot_stamp::BUILD);
            let history = [bravebot_session::sessions::StoredTurn::completed(
                1,
                turn.prompt,
                0,
                ends,
                turn.prompt_at,
            )];
            handle.save(
                turn.prompt,
                Standing {
                    conversation: &snapshot,
                    history: Some(&history),
                    turns: 1,
                    tokens: turn.outcome.tokens,
                    spend: &BTreeMap::from([(1, turn.outcome.tokens)]),
                    timing: &BTreeMap::new(),
                    model: Some(&turn.outcome.model),
                    todos: &BTreeMap::new(),
                    asides: &[],
                    trust: turn.trust,
                    programs: &bravebot_core::programs::TrustedPrograms::new(),
                    directories: &[],
                    manifest: None,
                    rewind: &[],
                    checkouts: &[],
                },
            );
            handle.resumable().map(str::to_string)
        }
    }
}
