//! What a session's `/cost`, `/context` and `/request` reports are answered from while a turn
//! holds its state (CMD-8).
//!
//! A turn holds the session's [`State`] locked from its start to its end, so a report read from
//! there would wait for the turn. The turn's [`crate::turn::BridgeReporter`] writes into a
//! [`Held`] shared with the session instead, and the bridge brings it up to date with the state
//! when a turn ends and whenever the state is replaced.

use crate::running::State;
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
    }

    /// The `session.report.cost` answer, worked out as the terminal's `/cost` works it out: the
    /// session total with the running turn's tokens in it, one row per turn with its share of the
    /// total, and what the total holds that no turn's row accounts for, such as what a session
    /// resumed from an older record spent before it was charged by turn.
    pub fn cost(&self) -> Value {
        let mut spend = self.spend.clone();
        if self.running > 0 {
            *spend.entry(self.turns).or_default() += self.running;
        }
        let total = self.total + self.running;
        let rows: Vec<Value> = spend
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
            "unattributed": total.saturating_sub(spend.values().sum()),
            "running": self.running,
        })
    }

    /// The `session.report.context` answer, in the terminal's `/context` order of cases: a count
    /// against no known budget is not measured, a compacted conversation says so, and a count with
    /// no request to divide it by says that. `fallback_budget` is the budget the settings name now,
    /// for a session no turn of which has started in this process.
    ///
    /// The sections are [`RequestView::sections`], so the split is the one the terminal draws.
    pub fn context(&self, fallback_budget: u64) -> Value {
        let (used, budget) = match self.occupancy {
            Occupancy::Measured { used, budget } => {
                (used, if budget > 0 { budget } else { fallback_budget })
            }
            Occupancy::Compacted => return context_answer("compacted", 0, 0, Vec::new()),
            Occupancy::NotMeasured => return context_answer("not-measured", 0, 0, Vec::new()),
        };
        if budget == 0 {
            return context_answer("not-measured", 0, 0, Vec::new());
        }
        let Some(view) = self.last_request.as_ref() else {
            return context_answer("no-request", used, budget, Vec::new());
        };
        let sections = view
            .sections(used)
            .into_iter()
            .map(|(section, tokens)| {
                json!({
                    "section": section_name(section),
                    "tokens": tokens,
                    "share": share(tokens, used),
                })
            })
            .collect();
        context_answer("measured", used, budget, sections)
    }

    /// The `session.report.request` answer: the instruction files the last request's system
    /// prompt was built from, with their bytes, and each span under the label its provenance gives
    /// (TRACE-9). The text of a span is carried to the window as it was sent and nothing here reads
    /// it.
    pub fn request(&self) -> Value {
        let Some(view) = self.last_request.as_ref() else {
            return json!({ "none": true, "instructionFiles": [], "spans": [] });
        };
        let files: Vec<Value> = view
            .instruction_files()
            .into_iter()
            .map(|(path, bytes)| json!({ "path": path, "bytes": bytes }))
            .collect();
        let spans: Vec<Value> = view
            .spans
            .iter()
            .map(|span| {
                json!({
                    "role": span.role,
                    "label": span.provenance.label(),
                    "text": span.text,
                })
            })
            .collect();
        json!({ "none": false, "instructionFiles": files, "spans": spans })
    }
}

/// `part` as a whole percentage of `whole`, rounded down as the terminal's rows are.
fn share(part: u64, whole: u64) -> u64 {
    match whole {
        0 => 0,
        whole => part.saturating_mul(100) / whole,
    }
}

fn context_answer(state: &str, total: u64, budget: u64, sections: Vec<Value>) -> Value {
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
