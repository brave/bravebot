//! Running one task as a manifest: plan everything, ask once, then walk the plan.
//!
//! The desktop side of MANIFEST-11 in `docs/specs/manifest.md`. A run is started from a session
//! and is not one of its turns:
//!
//! - The conversation is not sent to the planner and nothing is added to it.
//! - The run is saved as its own record, by the function the terminal and the command line use.
//! - The session's record is not written. Only its trust map changes, on a run that finished.
//!
//! The worker reports through the same events a turn uses for progress and questions. It ends
//! with `manifest.done` or `manifest.error`, and both carry what the run produced (MANIFEST-3).

use crate::bridge::{failure_fields, merge, rules_json};
use crate::emit::Emitter;
use crate::protocol::Event;
use crate::running::State;
use crate::turn::{BridgeConfirmer, BridgeReporter, BridgeSink};
use bravebot_agent::Workspace;
use bravebot_agent::manifest::Attempt;
use bravebot_agent::turn::{Task, TurnError};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_net::Egress;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

/// Everything a run needs, handed to its worker thread.
pub(crate) struct Walk {
    pub emitter: Emitter,
    pub session: String,
    pub project: std::path::PathBuf,
    pub state: Arc<Mutex<State>>,
    pub config: Config,
    pub attribution: bravebot_config::Attribution,
    pub output_cap: Option<usize>,
    pub deadlines: bravebot_agent::exec::Deadlines,
    pub model: Option<String>,
    pub workspace: Workspace,
    pub task: String,
    pub run: usize,
    pub cancel: Cancel,
    pub pending: crate::turn::Pending,
    pub answers: mpsc::Receiver<crate::turn::Reply>,
    pub finished: Arc<AtomicBool>,
}

/// What a run produced, as a front end reads it: the goal, the proposal, the plan and the steps.
///
/// All of it is released text. It came from a planner that was shown the task and nothing else,
/// or from the driver's own account of what it did.
fn attempt_json(attempt: &Attempt) -> Value {
    json!({
        "goal": attempt.shape,
        "proposed": attempt.proposed,
        "plan": attempt.plan,
        "steps": attempt.steps,
    })
}

/// Run one task as a manifest to its end, record it, and say what happened.
pub(crate) fn walk(walk: Walk) {
    let Walk {
        emitter,
        session,
        project,
        state,
        config,
        attribution,
        output_cap,
        deadlines,
        model,
        workspace,
        task: asked,
        run,
        cancel,
        pending,
        answers,
        finished,
    } = walk;

    // Held for the length of the run, as a turn holds it. The session refuses a turn and a
    // second run while one is in flight, so nothing else contends for it.
    let Ok(mut state) = state.lock() else {
        finished.store(true, Ordering::Release);
        return;
    };

    // No files and no conversation: the planner may be shown the task and nothing else
    // (MANIFEST-1). No auto-vetting either, since nothing is read before the plan is fixed.
    let task = Task::new(&asked)
        .with_home(bravebot_agent::home::directory())
        .with_cache(bravebot_agent::home::cache())
        .with_model(model)
        .with_attribution(attribution)
        .with_output_cap(output_cap)
        .with_deadlines(deadlines);

    let mut reporter = BridgeReporter::new(emitter.clone(), &session);
    let mut confirmer =
        BridgeConfirmer::new(emitter.clone(), &session, pending, answers, cancel.clone());
    let mut sink = BridgeSink::for_run(emitter.clone(), &session, run);
    let egress = Egress::new();

    let outcome = bravebot_agent::manifest::run(
        &config,
        &egress,
        &workspace,
        &task,
        &mut confirmer,
        &mut reporter,
        &mut sink,
        state.trust.clone(),
        &cancel,
    );

    // Read off the token and not off the error. A stop pressed at the plan question arrives as
    // a declined plan, and one pressed a step later arrives as a cancellation. Both are the
    // person stopping the run (MANIFEST-11).
    let stopped = outcome.is_err() && cancel.is_cancelled();

    // A rule a finished run recorded is about the same tree the next turn reads.
    if let Ok(done) = &outcome {
        state.trust = done.trust.clone();
    }

    // A run the person stopped is not written, as the terminal does not write one.
    let recorded = match stopped {
        true => None,
        false => bravebot_session::sessions::record_manifest_run(
            &project,
            &asked,
            &outcome,
            crate::FRONT,
            crate::agent_build(),
        ),
    };

    let event = match &outcome {
        Ok(done) => Event::new(
            "manifest.done",
            &session,
            json!({
                "run": run,
                // The released reply, as on `turn.done`. Never `reply`, which is labelled.
                "reply": done.reply_for_display(),
                "model": done.model,
                "steps": done.steps,
                "clean": done.clean,
                "tokens": done.tokens,
                "outputTokens": done.output_tokens,
                "notices": done.notices,
                "attempt": done.attempt.as_ref().map(attempt_json),
                "record": recorded,
                "trust": { "rules": rules_json(&state.trust) },
            }),
        ),
        Err(error) => {
            let chosen = task.model.as_deref().unwrap_or(&config.default_model);
            // The cause is what failed. The wrapper only says that a run is what it failed in.
            let (cause, attempt) = match error {
                TurnError::Manifest { attempt, cause } => (cause.as_ref(), Some(attempt.as_ref())),
                other => (other, None),
            };
            let mut data = failure_fields(cause, &config, chosen);
            // The driver's own sentence about why, where the driver wrote it. A failure of the
            // model service is reported by its category, since its text is the service's.
            let problem = match cause {
                TurnError::Precommit(_) | TurnError::Workspace(_) => Some(cause.to_string()),
                _ => None,
            };
            merge(
                &mut data,
                json!({
                    "run": run,
                    "stopped": stopped,
                    // Sent as a flag so a front end does not read the sentence below to find out.
                    "declined": !stopped && confirmer.declined_a_plan(),
                    "problem": problem,
                    "attempt": attempt.map(attempt_json),
                    "record": recorded,
                    "notices": reporter.notices(),
                }),
            );
            Event::new("manifest.error", &session, data)
        }
    };

    drop(state);
    finished.store(true, Ordering::Release);
    emitter.send(event);
}
