//! Typed audit events.
//!
//! The kernel never prints. Everything observable leaves through [`Event`], so the
//! audit trail is machine-readable and a stray `println!` cannot interleave with it
//! or leak content into a log.
//!
//! Events carry labels and decisions, never slot contents.

use crate::capability::Capability;
use crate::delegate::DelegateId;
use crate::label::Label;
use crate::slot::SlotId;
use std::fmt;

/// Which principle a refusal upholds. Useful for explaining a block to a user
/// without re-deriving why it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Principle {
    /// Untrusted data attempted to influence where an action goes.
    IntegrityGate,
    /// Private data attempted to leave without declassification.
    Confinement,
    /// An operation was attempted without the capability it requires.
    Capability,
    /// A confinement boundary could not be established, so the operation was refused
    /// rather than run unconfined.
    ConfinementUnavailable,
}

impl Principle {
    /// The name a program reads this by.
    ///
    /// Deliberately not the localised sentence a refusal is explained in. A caller deciding what
    /// to do about a refusal matches on this, and a name that changed with the reader's language
    /// would make that impossible.
    pub fn name(&self) -> &'static str {
        match self {
            Self::IntegrityGate => "integrity-gate",
            Self::Confinement => "confinement",
            Self::Capability => "capability",
            Self::ConfinementUnavailable => "confinement-unavailable",
        }
    }
}

/// The role a field plays in an action.
///
/// The asymmetry between these two is the anti-injection mechanism: routing decides
/// *where* an effect lands and must be trusted, while content is merely carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Decides where an action goes: a path, a URL, a command name. Must be `(T,pub)`.
    Routing,
    /// The payload. May be untrusted; must not be private at release time.
    Content,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Routing => f.write_str("routing"),
            Self::Content => f.write_str("content"),
        }
    }
}

/// One thing that happened, or was refused.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A gate allowed an operation.
    GatePassed { gate: &'static str, detail: String },
    /// A gate refused an operation.
    GateBlocked {
        gate: &'static str,
        detail: String,
        reason: String,
        /// Which principle the refusal upholds.
        ///
        /// Carried on the event rather than left to whoever explains it, because the gate that
        /// refused is the only thing that knows, and a reader working it back out of the reason
        /// would be matching on a sentence.
        principle: Principle,
    },
    /// A slot was written.
    SlotWritten { slot: SlotId, label: Label },
    /// A slot was reserved for a file that has not been read yet.
    ///
    /// Distinct from [`Event::SlotWritten`], which follows when something needs the bytes. A
    /// trail that recorded only the writing would say a file was read at a moment nothing had
    /// touched it.
    SlotDeferred {
        slot: SlotId,
        label: Label,
        origin: String,
    },
    /// A capability produced data at a label.
    Observed {
        capability: Capability,
        label: Label,
    },
    /// Untrusted content was authorised for release.
    Declassified {
        slot: SlotId,
        from: Label,
        to: Label,
        reason: &'static str,
    },
    /// A field was checked immediately before an effect fired.
    ActionField {
        tool: String,
        field: String,
        role: Role,
        label: Label,
        allowed: bool,
    },
}

/// Somewhere for events to go. Implemented outside the kernel: a terminal renderer, a
/// JSONL file, or both.
pub trait Sink {
    fn emit(&mut self, event: Event);

    /// Whose gates the events after this one are, where they are a delegate's.
    ///
    /// A turn and the delegates it spawned record into one trail, so a record that did not say
    /// which run took it would leave the turn's decisions and its delegates' interleaved with
    /// nothing telling them apart, and two delegates of the same kind reading identically.
    ///
    /// Said beside the event rather than carried on it, because an event is what a gate decided
    /// and the run that took it is a fact about the run. The driver says it, for the reason it
    /// says whose a report is: it already holds the answer, and reading the record back to work
    /// the answer out would be taking it from prose a model had a hand in.
    ///
    /// Ignored by a sink that keeps no attribution, and by every run that spawns nothing: a trail
    /// told nothing is a turn's own.
    fn recording_for(&mut self, _delegate: Option<DelegateId>) {}
}

/// Discards everything. For tests that do not assert on the trail.
#[derive(Debug, Default)]
pub struct NullSink;

impl Sink for NullSink {
    fn emit(&mut self, _event: Event) {}
}

/// Retains events in order. For tests, and for replaying a run's trail.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Vec<Event>,
    /// Which run took the decision at the same index, where it was a delegate's.
    ///
    /// Kept beside the events rather than in them because [`RecordingSink::events`] is what
    /// nearly every reader of this wants, and a record type there would put an attribution in
    /// front of every assertion about a trail that has only one run in it.
    from: Vec<Option<DelegateId>>,
    /// Whose events are arriving, until something says otherwise.
    recording: Option<DelegateId>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Every event, with the run whose gate took it where that was a delegate rather than the
    /// turn.
    pub fn recorded(&self) -> impl Iterator<Item = (Option<DelegateId>, &Event)> {
        self.from.iter().copied().zip(self.events.iter())
    }

    /// Every refusal, in order.
    pub fn blocked(&self) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(|e| matches!(e, Event::GateBlocked { .. }))
    }

    /// Whether the run completed without a single refusal.
    pub fn clean(&self) -> bool {
        self.blocked().next().is_none()
            && !self
                .events
                .iter()
                .any(|e| matches!(e, Event::ActionField { allowed: false, .. }))
    }
}

impl Sink for RecordingSink {
    fn emit(&mut self, event: Event) {
        self.events.push(event);
        self.from.push(self.recording);
    }

    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        self.recording = delegate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_sink_keeps_order() {
        let mut sink = RecordingSink::new();
        sink.emit(Event::GatePassed {
            gate: "first",
            detail: String::new(),
        });
        sink.emit(Event::GatePassed {
            gate: "second",
            detail: String::new(),
        });
        assert_eq!(sink.events().len(), 2);
        assert!(matches!(
            sink.events()[0],
            Event::GatePassed { gate: "first", .. }
        ));
    }

    /// One trail holds a turn's decisions and its delegates', so a record that did not say which
    /// run took it would leave them interleaved and unattributable. The turn's own are left
    /// unnamed, which is what "the turn, or one delegate of it" means when nothing has spawned.
    #[test]
    fn a_record_says_which_run_took_the_decision() {
        let mut sink = RecordingSink::new();
        let gate = |gate: &'static str| Event::GatePassed {
            gate,
            detail: String::new(),
        };

        sink.emit(gate("the turn's"));
        sink.recording_for(Some(DelegateId::nth(1)));
        sink.emit(gate("the first delegate's"));
        sink.recording_for(Some(DelegateId::nth(2)));
        sink.emit(gate("the second delegate's"));
        sink.recording_for(None);
        sink.emit(gate("the turn's again"));

        let took: Vec<Option<u32>> = sink
            .recorded()
            .map(|(from, _)| from.map(DelegateId::position))
            .collect();
        assert_eq!(took, vec![None, Some(1), Some(2), None]);
    }

    #[test]
    fn a_run_with_no_refusals_is_clean() {
        let mut sink = RecordingSink::new();
        sink.emit(Event::SlotWritten {
            slot: SlotId::new("s"),
            label: Label::untrusted_public(),
        });
        assert!(sink.clean());
    }

    #[test]
    fn a_blocked_gate_makes_a_run_unclean() {
        let mut sink = RecordingSink::new();
        sink.emit(Event::GateBlocked {
            gate: "action",
            detail: "field=path".into(),
            reason: "untrusted routing".into(),
            principle: Principle::IntegrityGate,
        });
        assert!(!sink.clean());
        assert_eq!(sink.blocked().count(), 1);
    }

    /// A refused action field counts as unclean even without a GateBlocked event, so a
    /// caller cannot report success by only emitting the field-level record.
    #[test]
    fn a_refused_action_field_makes_a_run_unclean() {
        let mut sink = RecordingSink::new();
        sink.emit(Event::ActionField {
            tool: "write_file".into(),
            field: "path".into(),
            role: Role::Routing,
            label: Label::untrusted_public(),
            allowed: false,
        });
        assert!(!sink.clean());
    }

    #[test]
    fn null_sink_discards() {
        let mut sink = NullSink;
        sink.emit(Event::GatePassed {
            gate: "x",
            detail: String::new(),
        });
    }
}
