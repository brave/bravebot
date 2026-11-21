//! What a session's `/cost`, `/context`, `/request` and `/status` reports are answered from while a turn
//! holds its state (CMD-8).
//!
//! A turn holds the session's [`State`] locked from its start to its end, so a report read from
//! there would wait for the turn. The turn's [`crate::turn::BridgeReporter`] writes into a
//! [`Held`] shared with the session instead, and the bridge brings it up to date with the state
//! when a turn ends and whenever the state is replaced.

use crate::running::State;
use bravebot_agent::breakdown::{ContextReading, Spend, share};
use bravebot_agent::confirm::printable_spelled;
use bravebot_agent::request_view::{RequestView, Section};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

/// How full the session's context stands, as `/context` reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Occupancy {
    /// No request has been measured.
    #[default]
    NotMeasured,
    /// The last request came to `used` tokens, against a budget of `budget`. A budget of zero
    /// means none was known when the count was taken.
    Measured { used: u64, budget: u64 },
    /// Compaction shortened the conversation and no request has been measured since.
    Compacted,
}

impl Occupancy {
    /// The reading a conversation gives: its last measured count, and whether compaction has
    /// taken anything from it. A count of zero after a compaction is a conversation that was
    /// shortened and not measured since, because compaction sets the count to zero.
    pub(crate) fn of(measured: u64, archived: usize, budget: u64) -> Self {
        if measured > 0 {
            Self::Measured {
                used: measured,
                budget,
            }
        } else if archived > 0 {
            Self::Compacted
        } else {
            Self::NotMeasured
        }
    }
}

/// What a session's reports are answered from.
#[derive(Clone, Debug, Default)]
pub struct Held {
    /// The last request the session's own turn built. A delegate's is never kept: it is a
    /// different conversation with a different prompt.
    pub last_request: Option<RequestView>,
    pub occupancy: Occupancy,
    /// Turns taken, counting the one running.
    pub turns: usize,
    /// Tokens the turns that have ended were charged.
    pub total: u64,
    /// What each ended turn was charged, by turn number.
    pub spend: BTreeMap<usize, u64>,
    /// What the running turn has spent so far. Zero between turns.
    pub running: u64,
    /// The budget the running turn compacts at, for a count it measures.
    budget: u64,
    /// The session's id, once it has one, and the title its record is written under, as they
    /// stood when the state was last read, for a `/status` asked while a turn holds it.
    id: Option<String>,
    title: Option<String>,
}

/// A session's [`Held`], shared between the session and the turn that writes into it.
pub type Shared = Arc<Mutex<Held>>;

/// Take the lock, reading the store as it stands if a turn panicked holding it: each update
/// replaces whole fields, so none is left half written.
pub fn hold(shared: &Mutex<Held>) -> MutexGuard<'_, Held> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Held {
    /// The store for a session whose state is `state`, as it is opened, forked, rewound or made
    /// afresh: its spend and its last count, measured against `budget`, and no request, since
    /// none has been built in this process for the conversation it holds.
    pub fn of(state: &State, budget: u64) -> Self {
        Self {
            last_request: None,
            occupancy: Occupancy::of(
                state.conversation.last_request_tokens(),
                state.conversation.snapshot().archive.len(),
                budget,
            ),
            turns: state.turns,
            total: state.tokens,
            spend: state.spend.clone(),
            running: 0,
            budget,
            id: state
                .handle
                .as_ref()
                .map(|handle| handle.id().to_owned())
                .or_else(|| state.named.clone()),
            title: state
                .handle
                .as_ref()
                .map(|handle| handle.title().to_owned())
                .filter(|title| !title.is_empty())
                .or_else(|| state.first_prompt.clone()),
        }
    }

    /// The session was put back to an earlier turn and now holds `state`. The budget the last turn
    /// read is kept, since the settings it came from did not change.
    pub fn rewound(&mut self, state: &State) {
        *self = Self::of(state, self.budget);
    }

    /// Turn `turn` has begun and compacts at `budget`.
    pub fn turn_started(&mut self, turn: usize, budget: u64) {
        self.turns = turn;
        self.running = 0;
        self.budget = budget;
    }

    /// The turn has built the request it is about to send.
    pub fn request_built(&mut self, view: RequestView) {
        self.last_request = Some(view);
    }

    /// What the running turn has spent so far, and what its last completed request came to.
    pub fn spent(&mut self, spent: &bravebot_agent::Spent) {
        self.running = spent.tokens;
        if spent.context_tokens > 0 {
            self.occupancy = Occupancy::Measured {
                used: spent.context_tokens,
                budget: self.budget,
            };
        }
    }

    /// The turn has ended and been charged to `state`, whose conversation compaction has taken
    /// `archived` messages from in all.
    pub fn turn_ended(&mut self, state: &State, archived: usize) {
        self.turns = state.turns;
        self.total = state.tokens;
        self.spend = state.spend.clone();
        self.running = 0;
        self.occupancy = Occupancy::of(
            state.conversation.last_request_tokens(),
            archived,
            self.budget,
        );
        let named = Self::of(state, self.budget);
        self.id = named.id;
        self.title = named.title;
    }

    /// The part of the `session.report.status` answer this store holds: the session's id and
    /// title, its turns counting the one running, and the tokens it has spent with the running
    /// turn's in them.
    pub fn status(&self) -> Value {
        json!({
            "sessionId": self.id,
            "title": self.title,
            "turns": self.turns,
            "tokens": self.total + self.running,
        })
    }

    /// The `session.report.cost` answer, the breakdown the terminal's `/cost` words ([`Spend`]):
    /// the session total with the running turn's tokens in it, one row per turn with its share of
    /// the total, and what the total holds that no turn's row accounts for, such as what a session
    /// resumed from an older record spent before it was charged by turn.
    pub fn cost(&self) -> Value {
        let Spend {
            total,
            rows,
            unattributed,
        } = Spend::of(self.total, &self.spend, self.turns, self.running);
        let rows: Vec<Value> = rows
            .iter()
            .map(|(turn, tokens)| {
                json!({
                    "turn": turn,
                    "tokens": tokens,
                    "share": share(*tokens, total),
                })
            })
            .collect();
        json!({
            "total": total,
            "turns": self.turns,
            "rows": rows,
            "unattributed": unattributed,
            "running": self.running,
        })
    }

    /// The `session.report.context` answer, the reading the terminal's `/context` words
    /// ([`ContextReading`]): a count against no known budget is not measured, a compacted
    /// conversation says so, and a count with no request to divide it by says that.
    /// `fallback_budget` is the budget the settings name now, for a count taken before any turn of
    /// this process read one.
    pub fn context(&self, fallback_budget: u64) -> Value {
        let measured = match self.occupancy {
            Occupancy::Measured { used, budget } => {
                Some((used, if budget > 0 { budget } else { fallback_budget }))
            }
            _ => None,
        };
        let compacted = self.occupancy == Occupancy::Compacted;
        match ContextReading::of(measured, compacted, self.last_request.as_ref()) {
            ContextReading::NotMeasured => context_answer("not-measured", 0, 0, &[]),
            ContextReading::Compacted => context_answer("compacted", 0, 0, &[]),
            ContextReading::NoRequest { used, budget } => {
                context_answer("no-request", used, budget, &[])
            }
            ContextReading::Measured {
                used,
                budget,
                sections,
            } => {
                let sections: Vec<Value> = sections
                    .into_iter()
                    .map(|(section, tokens)| {
                        json!({
                            "section": section_name(section),
                            "tokens": tokens,
                            "share": share(tokens, used),
                        })
                    })
                    .collect();
                context_answer("measured", used, budget, &sections)
            }
        }
    }

    /// The `session.report.request` answer: the instruction files the last request's system
    /// prompt was built from, with their bytes, and each span under the label its provenance gives
    /// (TRACE-9). Every string is drawn by `printable_spelled`, as the audit inspector's words are
    /// (TRACE-11), a span's text line by line, so the window pictures a control character with the
    /// terminal's one definition and holds no list of its own. That is a map from characters to characters, and nothing here
    /// branches on a span's text.
    pub fn request(&self) -> Value {
        let Some(view) = self.last_request.as_ref() else {
            return json!({ "none": true, "instructionFiles": [], "spans": [] });
        };
        let files: Vec<Value> = view
            .instruction_files()
            .into_iter()
            .map(|(path, bytes)| json!({ "path": printable_spelled(path), "bytes": bytes }))
            .collect();
        let spans: Vec<Value> = view
            .spans
            .iter()
            .map(|span| {
                json!({
                    "role": printable_spelled(span.role),
                    "label": printable_spelled(&span.provenance.label()),
                    "text": printable_lines(&span.text),
                })
            })
            .collect();
        json!({ "none": false, "instructionFiles": files, "spans": spans })
    }
}

/// `text` drawn line by line with `printable_spelled`, keeping its line breaks, as the terminal's
/// request view draws a span (TRACE-9). Every other control character is pictured.
fn printable_lines(text: &str) -> String {
    text.split('\n')
        .map(printable_spelled)
        .collect::<Vec<_>>()
        .join("\n")
}

fn context_answer(state: &str, total: u64, budget: u64, sections: &[Value]) -> Value {
    json!({ "state": state, "total": total, "budget": budget, "sections": sections })
}

/// The fixed name the window words a section by.
fn section_name(section: Section) -> &'static str {
    match section {
        Section::System => "system",
        Section::Instructions => "instructions",
        Section::Skills => "skills",
        Section::Tools => "tools",
        Section::Typed => "typed",
        Section::Planner => "planner",
        Section::Results => "results",
        Section::Other => "other",
    }
}
