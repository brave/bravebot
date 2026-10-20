//! Summarising a session's earlier conversation when a person asks, as the terminal's `/compact`.
//!
//! Not a turn: nothing is sent to the planner but the summariser's request, no tool runs and no
//! question is asked. It holds the session as a turn does, so a turn or a run is refused until it
//! ends, and `turn.cancel` stops it. It ends with `compact.done` or `compact.error`.

use crate::bridge::{merge, save};
use crate::emit::Emitter;
use crate::protocol::Event;
use crate::running::State;
use crate::turn::{BridgeReporter, BridgeSink};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_net::Egress;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// Everything a compaction needs, handed to its worker thread.
pub(crate) struct Shorten {
    pub emitter: Emitter,
    pub session: String,
    pub project: std::path::PathBuf,
    pub state: Arc<Mutex<State>>,
    pub config: Config,
    pub model: Option<String>,
    pub cancel: Cancel,
    pub finished: Arc<AtomicBool>,
}

/// Summarise what the session's conversation holds before its last exchanges, and say how it went.
///
/// A summary that came back is kept even when a stop arrived after it, as the terminal keeps one.
/// One that did not come back leaves the conversation as it was. A session with nothing old enough
/// to summarise is told so, and its record is not written.
pub(crate) fn shorten(shorten: Shorten) {
    let Shorten {
        emitter,
        session,
        project,
        state,
        config,
        model,
        cancel,
        finished,
    } = shorten;

    let Ok(mut state) = state.lock() else {
        emitter.finish(
            Event::new("compact.error", &session, failed("internal", false)),
            &finished,
        );
        return;
    };

    let turn = state.turns;
    let started = std::time::Instant::now();
    let mut reporter = BridgeReporter::new(emitter.clone(), &session);
    let mut sink = BridgeSink::new(emitter.clone(), &session, turn);
    let trust = state.answers.trust.clone();
    let done = bravebot_agent::turn::compact(
        &config,
        &Egress::new(),
        &mut state.conversation,
        model.as_deref(),
        &mut reporter,
        &mut sink,
        trust,
        None,
        &cancel,
    );

    let event = match done {
        Ok(Some(summary)) => {
            let tokens = summary.usage.total();
            let took = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            state.tokens += tokens;
            *state.spend.entry(turn).or_insert(0) += tokens;
            let timing = state.timing.entry(turn).or_default();
            timing.wall_ms += took;
            timing.inference_ms += took;
            // Every point describes the conversation from before the summary, so rewinding to one
            // would quietly undo it.
            state.rewind.close();
            let archived = save(&project, &mut state, turn, sink.trail());
            let mut data = done_fields(true);
            merge(
                &mut data,
                json!({
                    "summarised": summary.summarised,
                    "kept": summary.kept,
                    "model": summary.model,
                    "tokens": tokens,
                    // What the summary won back, as the terminal reports it: what was sent less
                    // what came back in its place.
                    "wonBack": summary.usage.prompt_tokens.saturating_sub(summary.usage.completion_tokens),
                    "archived": archived,
                    "rewind": [],
                }),
            );
            Event::new("compact.done", &session, data)
        }
        Ok(None) => Event::new("compact.done", &session, done_fields(false)),
        Err(error) => {
            let stopped = cancel.is_cancelled();
            let category = if stopped {
                "cancelled"
            } else {
                error.category().name()
            };
            Event::new("compact.error", &session, failed(category, stopped))
        }
    };

    drop(state);
    emitter.finish(event, &finished);
}

fn done_fields(compacted: bool) -> serde_json::Value {
    json!({ "compacted": compacted, "summarised": 0, "kept": 0, "tokens": 0, "wonBack": 0 })
}

fn failed(category: &str, stopped: bool) -> serde_json::Value {
    json!({ "category": category, "stopped": stopped })
}
