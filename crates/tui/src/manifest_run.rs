//! Live manifest progress, completion and session persistence.

use crate::remote_confirm::ToMain;
use crate::state::Session;
use bravebot_agent::{Outcome, Spent, TurnError};
use bravebot_core::cancel::Cancel;
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_session::audit::{Stamped, Trail};
use bravebot_session::sessions::{self, Front, Handle, Standing};
use std::path::Path;
use std::thread::JoinHandle;

/// What a manifest worker ends with: the outcome, the audit trail and the file decisions it holds.
pub type Worked = (Result<Outcome, TurnError>, Trail, TrustStore);

/// Join the worker and leave the session's map holding the decisions it ended with.
///
/// The worker's map is the one it was lent, so it carries attachment grants and every file
/// decision the run made, whether the run finished, failed or was stopped. A worker that panicked
/// returns nothing, and the map the session held before the run stays.
pub fn join(
    worker: JoinHandle<Worked>,
    trust: &mut TrustStore,
) -> (Result<Outcome, TurnError>, Trail) {
    match worker.join() {
        Ok((outcome, sink, worker_trust)) => {
            *trust = worker_trust;
            (outcome, sink)
        }
        Err(_) => (
            Err(TurnError::Precommit(
                t!(manifest_ended_unexpectedly).to_string(),
            )),
            Trail::new(),
        ),
    }
}

/// Cumulative progress belongs to one run and is replaced by its final outcome on success.
#[derive(Default)]
pub struct Run {
    retained: Option<Spent>,
}

impl Run {
    /// Apply progress and return questions for the terminal to answer.
    pub fn progress(&mut self, session: &mut Session, message: ToMain) -> Option<ToMain> {
        match message {
            ToMain::Spent(spent) => self.retained = Some(spent),
            ToMain::PromptRecorded(_) => {}
            ToMain::RequestBuilt(view) => session.set_last_request(*view),
            ToMain::Written(written) => session.set_written(written),
            ToMain::Phase(phase) => session.set_phase(phase),
            ToMain::Narration(text) => session.narrate(text),
            ToMain::Notice(text) => session.note_once(text),
            ToMain::Streaming(text) => session.streaming(&text),
            ToMain::Composing(call) => session.composing(call),
            ToMain::Started(activity) => session.start_activity(activity),
            ToMain::Finished(activity) => session.finish_activity(activity),
            ToMain::CheckStarted(checking) => session.checking(checking),
            ToMain::CheckFinished => session.checked(),
            ToMain::HookStarted(moment, program) => session.hook_running(moment, program),
            ToMain::HookFinished => session.hook_over(),
            ToMain::Quarantined(shown) => session.show(shown),
            ToMain::Returned(returned) => session.returned(returned),
            ToMain::Landed(landing) => session.landed(landing),
            question => return Some(question),
        }
        None
    }

    /// Charge the continuing session once and record runs the person did not stop.
    pub fn complete(
        &self,
        session: &mut Session,
        project: &Path,
        task: &str,
        outcome: &Result<Outcome, TurnError>,
        cancel: &Cancel,
    ) -> Option<String> {
        let usage = sessions::manifest_usage(outcome, self.retained);
        session.end_run(usage.map_or(0, |s| s.tokens), usage.map(|s| s.timing));
        if outcome.is_err() && cancel.is_cancelled() {
            None
        } else {
            sessions::record_manifest_run(
                project,
                task,
                outcome,
                self.retained,
                Front::Terminal,
                bravebot_stamp::BUILD,
            )
        }
    }
}

/// Persist the continuing conversation only after it has a turn to resume.
pub fn save_session(stored: &mut Handle, standing: Standing<'_>, events: &[Stamped]) {
    if standing.turns > 0 {
        let title = stored.title().to_string();
        let turns = standing.turns;
        stored.save(&title, standing);
        stored.append_audit(turns, events);
    }
}
