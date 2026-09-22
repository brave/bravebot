//! The audit trail, as a file rather than a screen.
//!
//! The same events Ctrl-T shows, written down so they can be read after the fact: which gate
//! checked what, the label every value carried, and what was released. One JSON object per line,
//! because a session's trail is appended a turn at a time and read with whatever is to hand.
//!
//! The two axes are written out in words rather than as the compact form the screen uses.
//! `(U,priv)` is right for a line of a terminal, where the reader has the legend in front of
//! them; `"integrity": "untrusted", "confidentiality": "private"` is right for a file being read
//! six months later by someone answering a question about what happened.
//!
//! Shaping events here rather than deriving it in the kernel is deliberate. `bravebot-core` has no
//! dependencies at all, and a serialisation format is a presentation concern: the same events
//! already become terminal lines a few modules away.
//!
//! Nothing in here is content. Every field is a gate name, a capability, a label, a path or a
//! slot id, which is the same reason the trail can be shown on a screen without a release.

use bravebot_agent::report::DelegateId;
use bravebot_core::event::{Event, Role, Sink};
use bravebot_core::label::{Confidentiality, Integrity, Label};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// An event, when it happened, and which run took it.
#[derive(Debug, Clone)]
pub struct Stamped {
    /// Seconds since the epoch, taken as the event was emitted.
    pub at: u64,
    /// The delegate whose gate decided it, where the turn did not decide it itself.
    ///
    /// A turn and its delegates record here together, so a record that did not say which run it
    /// came from would leave the two interleaved with nothing telling them apart.
    pub from: Option<DelegateId>,
    pub event: Event,
}

/// Collects a run's events, noting the time of each as it arrives.
///
/// The time has to be taken here because there is nowhere later to take it. The trail used to be
/// stamped when it was written to disk, which happens once, at the end of a turn: every event in
/// a turn therefore carried the same second, and a trail whose events all happened at once cannot
/// say which came first, how long a step took, or when a turn ended. That is precisely what
/// somebody reading a session back needs it for.
///
/// The clock is read in this crate rather than in the kernel, which has none and needs none.
#[derive(Debug, Default)]
pub struct Trail {
    events: Vec<Stamped>,
    /// Whose events are arriving, until something says otherwise.
    recording: Option<DelegateId>,
}

impl Trail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[Stamped] {
        &self.events
    }

    /// Every event as the transcript shows it, each named with the run whose gate took it.
    pub fn lines(&self) -> Vec<TrailLine> {
        self.events
            .iter()
            .map(|stamped| as_line(&stamped.event, stamped.from))
            .collect()
    }
}

impl Sink for Trail {
    fn emit(&mut self, event: Event) {
        self.events.push(Stamped {
            at: now(),
            from: self.recording,
            event,
        });
    }

    fn recording_for(&mut self, delegate: Option<DelegateId>) {
        self.recording = delegate;
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One event as the transcript shows it: the words, and whether it is a refusal.
///
/// The transcript holds these rather than the events themselves, because half of what it shows
/// did not happen in this process. A trail read back off disk is a *record* of an event and not
/// the event: an [`Event`] names its gate with a `&'static str` supplied by the code that emitted
/// it, and a name read out of a file only looks like one. Keeping the distinction means a resumed
/// trail cannot be mistaken for something a gate in this process decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailLine {
    pub text: String,
    /// Whether this is something a gate refused, which is drawn differently.
    pub blocked: bool,
}

/// One event, as the transcript shows it, named with the run whose gate took it.
///
/// The turn's own records are left unnamed: naming every line would name the whole of the common
/// case in order to tell apart the part of it that has more than one run in it.
pub fn as_line(event: &Event, from: Option<DelegateId>) -> TrailLine {
    let line = worded(event);
    match from {
        Some(delegate) => line.attributed_to(&delegate.to_string()),
        None => line,
    }
}

/// One event, in words.
///
/// The wording lives here rather than in the renderer so that [`recalled`] can produce the same
/// words for the same event. Two spellings of one line would drift the moment either changed.
fn worded(event: &Event) -> TrailLine {
    match event {
        Event::GatePassed { gate, detail } => TrailLine::passed(format!("{gate}: {detail}")),
        Event::GateBlocked { gate, reason, .. } => TrailLine::blocked(format!("{gate}: {reason}")),
        Event::Observed { capability, label } => {
            TrailLine::passed(format!("{capability} produced {label}"))
        }
        Event::SlotWritten { slot, label } => TrailLine::passed(format!("slot {slot} at {label}")),
        Event::SlotDeferred {
            slot,
            label,
            origin,
        } => TrailLine::passed(format!("slot {slot} holds {origin}, unread, at {label}")),
        Event::Declassified { slot, from, to, .. } => {
            TrailLine::passed(format!("released {slot} {from} → {to}"))
        }
        Event::ActionField {
            tool,
            field,
            role,
            label,
            // The verdict is not read here. Whether a record is a refusal is the kernel's own
            // answer, and this used to be one of the places that worked it out again.
            allowed: _,
        } => TrailLine {
            text: format!("{tool}.{field} [{}] {label}", role_word(*role)),
            blocked: event.is_refusal(),
        },
    }
}

/// One event read back out of the audit file.
///
/// `None` for a line this build does not recognise, which is left out rather than shown as a
/// mangled one: an audit that cannot be read faithfully should say less, not say it wrong.
pub fn recalled(event: &Value) -> Option<TrailLine> {
    let line = read_back(event)?;
    Some(match event["delegate"].as_str() {
        Some(delegate) => line.attributed_to(delegate),
        None => line,
    })
}

/// Whether a stored record is a refusal.
///
/// The verdict the kernel wrote, where the file has one. A file written before the verdict was
/// recorded carries only the shape, so that is the fallback, and it reads a `action_field` whose
/// `allowed` cannot be read as a refusal: a check whose answer nobody can read is not one to draw
/// as though it passed.
fn refused(event: &Value) -> bool {
    match event["refusal"].as_bool() {
        Some(verdict) => verdict,
        None => match event["kind"].as_str() {
            Some("gate_blocked") => true,
            Some("action_field") => event["allowed"].as_bool() != Some(true),
            _ => false,
        },
    }
}

/// The words for a stored event, before the run that took it is put in front of them.
fn read_back(event: &Value) -> Option<TrailLine> {
    let text = match event["kind"].as_str()? {
        "gate_passed" => format!("{}: {}", event["gate"].as_str()?, event["detail"].as_str()?),
        "gate_blocked" => format!("{}: {}", event["gate"].as_str()?, event["reason"].as_str()?),
        "observed" => format!(
            "{} produced {}",
            event["capability"].as_str()?,
            label_text(&event["label"])
        ),
        "slot_written" => format!(
            "slot {} at {}",
            event["slot"].as_str()?,
            label_text(&event["label"])
        ),
        "slot_deferred" => format!(
            "slot {} holds {}, unread, at {}",
            event["slot"].as_str()?,
            event["origin"].as_str()?,
            label_text(&event["label"])
        ),
        "declassified" => format!(
            "released {} {} → {}",
            event["slot"].as_str()?,
            label_text(&event["from"]),
            label_text(&event["to"])
        ),
        "action_field" => format!(
            "{}.{} [{}] {}",
            event["tool"].as_str()?,
            event["field"].as_str()?,
            event["role"].as_str()?,
            label_text(&event["label"])
        ),
        _ => return None,
    };
    Some(TrailLine {
        text,
        blocked: refused(event),
    })
}

impl TrailLine {
    /// The same line, said to be a delegate's.
    ///
    /// The name is taken as text because half of these are read back off disk, where a run's
    /// number is a string that only looks like one this process minted.
    fn attributed_to(self, delegate: &str) -> Self {
        Self {
            text: format!("{delegate} {}", self.text),
            blocked: self.blocked,
        }
    }

    fn passed(text: String) -> Self {
        Self {
            text,
            blocked: false,
        }
    }

    fn blocked(text: String) -> Self {
        Self {
            text,
            blocked: true,
        }
    }
}

fn role_word(role: Role) -> &'static str {
    match role {
        Role::Routing => "routing",
        Role::Content => "content",
    }
}

/// A stored label in the compact form a terminal shows.
///
/// Unrecognised words read as the more restrictive axis, matching how everything else in this
/// tree degrades. Nothing turns on it here, since this is a line on a screen, but a label drawn
/// as better than it was would be the wrong thing to be relaxed about.
fn label_text(label: &Value) -> String {
    let integrity = if label["integrity"] == "trusted" {
        "T"
    } else {
        "U"
    };
    let confidentiality = if label["confidentiality"] == "public" {
        "pub"
    } else {
        "priv"
    };
    format!("({integrity},{confidentiality})")
}

/// One event, as it is written down, with the run whose gate took it where that was a delegate.
///
/// The field is left out of the turn's own records rather than written as null, so the trail of a
/// turn that spawned nothing is the file it always was.
pub fn as_json(event: &Event, from: Option<DelegateId>) -> Value {
    let mut written = shaped(event);
    // The kernel's verdict, beside what it decided. A refusal is two shapes rather than one, so
    // every reader that worked the pair out for itself was a reader that could be given a third
    // shape and go on answering for two: a screen filtering the evidence on `allowed` alone drops
    // a `gate_blocked` as readily as it keeps a record whose verdict it cannot read.
    written["refusal"] = Value::Bool(event.is_refusal());
    if let Some(delegate) = from {
        written["delegate"] = Value::String(delegate.to_string());
    }
    written
}

/// What a gate decided, as its own fields.
fn shaped(event: &Event) -> Value {
    match event {
        Event::GatePassed { gate, detail } => json!({
            "kind": "gate_passed",
            "gate": gate,
            "detail": detail,
        }),
        Event::GateBlocked {
            gate,
            detail,
            reason,
            // Left out of the record on purpose. What is written here is read back as a line for
            // a person, and a name for a program has nobody to read it there.
            principle: _,
        } => json!({
            "kind": "gate_blocked",
            "gate": gate,
            "detail": detail,
            "reason": reason,
        }),
        Event::SlotWritten { slot, label } => json!({
            "kind": "slot_written",
            "slot": slot.as_str(),
            "label": label_json(*label),
        }),
        Event::SlotDeferred {
            slot,
            label,
            origin,
        } => json!({
            "kind": "slot_deferred",
            "slot": slot.as_str(),
            "origin": origin,
            "label": label_json(*label),
        }),
        Event::Observed { capability, label } => json!({
            "kind": "observed",
            "capability": capability.to_string(),
            "label": label_json(*label),
        }),
        Event::Declassified {
            slot,
            from,
            to,
            reason,
        } => json!({
            "kind": "declassified",
            "slot": slot.as_str(),
            "from": label_json(*from),
            "to": label_json(*to),
            "reason": reason,
        }),
        Event::ActionField {
            tool,
            field,
            role,
            label,
            allowed,
        } => json!({
            "kind": "action_field",
            "tool": tool,
            "field": field,
            "role": role_word(*role),
            "label": label_json(*label),
            "allowed": allowed,
        }),
    }
}

/// A label, with both axes named.
fn label_json(label: Label) -> Value {
    json!({
        "integrity": match label.integrity {
            Integrity::Trusted => "trusted",
            Integrity::Untrusted => "untrusted",
        },
        "confidentiality": match label.confidentiality {
            Confidentiality::Public => "public",
            Confidentiality::Private => "private",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::capability::Capability;
    use bravebot_core::slot::SlotId;

    /// The four words the whole model is stated in have to appear in the file, or an audit is a
    /// list of gate names with no answer to the question it exists for.
    #[test]
    fn both_axes_are_written_out_in_words() {
        let written = as_json(
            &Event::Observed {
                capability: Capability::FileRead,
                label: Label::untrusted_private(),
            },
            None,
        );
        assert_eq!(written["label"]["integrity"], "untrusted");
        assert_eq!(written["label"]["confidentiality"], "private");

        let written = as_json(
            &Event::Observed {
                capability: Capability::FileRead,
                label: Label::trusted_public(),
            },
            None,
        );
        assert_eq!(written["label"]["integrity"], "trusted");
        assert_eq!(written["label"]["confidentiality"], "public");
    }

    /// A refusal is the line an audit is read for, and it has to say what was refused and why.
    #[test]
    fn a_refusal_records_what_it_refused_and_why() {
        let written = as_json(
            &Event::GateBlocked {
                gate: "trusted-read",
                detail: "edit_file".to_string(),
                reason: "content is untrusted".to_string(),
                principle: bravebot_core::event::Principle::IntegrityGate,
            },
            None,
        );
        assert_eq!(written["kind"], "gate_blocked");
        assert_eq!(written["gate"], "trusted-read");
        assert_eq!(written["reason"], "content is untrusted");
    }

    /// A release is the other one: it is the moment content left, and the audit says where from
    /// and to what.
    #[test]
    fn a_release_records_both_ends_of_it() {
        let written = as_json(
            &Event::Declassified {
                slot: SlotId::new("ref:3"),
                from: Label::untrusted_private(),
                to: Label::untrusted_public(),
                reason: "shown to the user",
            },
            None,
        );
        assert_eq!(written["slot"], "ref:3");
        assert_eq!(written["from"]["confidentiality"], "private");
        assert_eq!(written["to"]["confidentiality"], "public");
    }

    #[test]
    fn a_field_check_records_the_role_it_was_checked_as() {
        let written = as_json(
            &Event::ActionField {
                tool: "write_file".to_string(),
                field: "path".to_string(),
                role: Role::Routing,
                label: Label::trusted_public(),
                allowed: true,
            },
            None,
        );
        assert_eq!(written["role"], "routing");
        assert_eq!(written["allowed"], true);
    }

    /// Every kind, so the round trip below covers the whole enum rather than the easy half.
    fn every_kind() -> Vec<Event> {
        vec![
            Event::GatePassed {
                gate: "capability",
                detail: "file_read granted".to_string(),
            },
            Event::GateBlocked {
                gate: "trusted-read",
                detail: "edit_file".to_string(),
                reason: "content is untrusted".to_string(),
                principle: bravebot_core::event::Principle::IntegrityGate,
            },
            Event::Observed {
                capability: Capability::FileRead,
                label: Label::untrusted_private(),
            },
            Event::SlotWritten {
                slot: SlotId::new("ref:2"),
                label: Label::untrusted_private(),
            },
            Event::Declassified {
                slot: SlotId::new("ref:3"),
                from: Label::untrusted_private(),
                to: Label::untrusted_public(),
                reason: "shown to the user",
            },
            Event::ActionField {
                tool: "write_file".to_string(),
                field: "path".to_string(),
                role: Role::Routing,
                label: Label::trusted_public(),
                allowed: true,
            },
            Event::ActionField {
                tool: "fetch".to_string(),
                field: "url".to_string(),
                role: Role::Routing,
                label: Label::untrusted_public(),
                allowed: false,
            },
        ]
    }

    /// A trail read back off disk must read exactly as it did when it happened. Two spellings of
    /// one line is the failure this guards: the file and the screen would drift apart the moment
    /// either was reworded, and nobody would notice until they compared a resumed session with a
    /// live one.
    ///
    /// The run that took the decision is part of that. A resumed session whose delegates' records
    /// had lost their names would show one run's decisions where there were three.
    #[test]
    fn a_stored_event_reads_back_as_the_line_it_was() {
        for event in every_kind() {
            for from in [None, Some(DelegateId::nth(2))] {
                assert_eq!(
                    recalled(&as_json(&event, from)),
                    Some(as_line(&event, from)),
                    "{event:?} did not come back as the line it was shown as"
                );
            }
        }
    }

    /// Records from a turn and from the delegates it spawned land in one trail, so a record that
    /// did not name its run would leave the two interleaved and unattributable, and two delegates
    /// of the same kind would read identically.
    #[test]
    fn a_delegates_records_are_named_and_the_turns_own_are_not() {
        let mut trail = Trail::new();
        let gate = |detail: &str| Event::GatePassed {
            gate: "precommit",
            detail: detail.to_string(),
        };

        trail.emit(gate("the turn fixed its routing"));
        trail.recording_for(Some(DelegateId::nth(1)));
        trail.emit(gate("the first delegate fixed its routing"));
        trail.recording_for(Some(DelegateId::nth(2)));
        trail.emit(gate("the second delegate fixed its routing"));
        trail.recording_for(None);
        trail.emit(gate("the turn carried on"));

        let said: Vec<String> = trail.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(
            said,
            vec![
                "precommit: the turn fixed its routing",
                "d1 precommit: the first delegate fixed its routing",
                "d2 precommit: the second delegate fixed its routing",
                "precommit: the turn carried on",
            ]
        );
    }

    /// The file is what somebody reads months later, and a name only on the screen would answer
    /// the question for the session that is still open and not for the one that is not.
    #[test]
    fn the_written_record_names_the_delegate_that_took_the_decision() {
        let written = as_json(
            &Event::GatePassed {
                gate: "precommit",
                detail: "routing fixed".to_string(),
            },
            Some(DelegateId::nth(3)),
        );
        assert_eq!(written["delegate"], "d3");

        let turns_own = as_json(
            &Event::GatePassed {
                gate: "precommit",
                detail: "routing fixed".to_string(),
            },
            None,
        );
        assert!(
            turns_own.get("delegate").is_none(),
            "a turn's own record must not claim to be a delegate's: {turns_own}"
        );
    }

    /// A refusal must still be a refusal after the round trip, since that is what colours it red
    /// and a refusal drawn as an ordinary line is the one thing the trail exists to make loud.
    #[test]
    fn a_refusal_is_still_a_refusal_when_it_is_read_back() {
        let blocked = as_json(
            &Event::GateBlocked {
                gate: "action",
                detail: String::new(),
                reason: "injection blocked".to_string(),
                principle: bravebot_core::event::Principle::IntegrityGate,
            },
            None,
        );
        let line = recalled(&blocked).expect("a refusal reads back");
        assert!(line.blocked);
        assert!(line.text.contains("injection blocked"));

        let refused_field = as_json(
            &Event::ActionField {
                tool: "fetch".to_string(),
                field: "url".to_string(),
                role: Role::Routing,
                label: Label::untrusted_public(),
                allowed: false,
            },
            None,
        );
        assert!(
            recalled(&refused_field)
                .expect("a field reads back")
                .blocked
        );
    }

    /// A line from a newer build is left out rather than drawn as a mangled one: an audit that
    /// cannot be read faithfully should say less, not say it wrong.
    #[test]
    fn an_event_this_build_does_not_know_is_left_out() {
        assert_eq!(recalled(&json!({"kind": "invented_later"})), None);
        assert_eq!(recalled(&json!({"gate": "capability"})), None);
        assert_eq!(recalled(&Value::Null), None);
    }

    /// A truncated line must not become a line claiming something happened that did not. The last
    /// line of an audit is exactly the one a killed session leaves half-written.
    #[test]
    fn a_half_written_event_is_left_out() {
        assert_eq!(recalled(&json!({"kind": "gate_passed"})), None);
        assert_eq!(
            recalled(&json!({"kind": "gate_blocked", "gate": "action"})),
            None
        );
    }

    /// A verdict nobody can read is drawn as a refusal. A check whose answer is missing is not
    /// one to show as though it passed.
    #[test]
    fn a_field_check_with_no_verdict_reads_as_refused() {
        let line = recalled(&json!({
            "kind": "action_field",
            "tool": "write_file",
            "field": "path",
            "role": "routing",
            "label": {"integrity": "trusted", "confidentiality": "public"},
        }))
        .expect("a field with no verdict still reads back");
        assert!(line.blocked);
    }

    /// The verdict, beside what was decided. Every reader of a record needs it, and one that
    /// works it back out of the kind and the fields has to be taught every shape a refusal takes:
    /// a shape it was not taught drops out of the evidence rather than into it.
    #[test]
    fn every_record_carries_the_kernels_own_verdict() {
        for event in every_kind() {
            assert_eq!(
                as_json(&event, None)["refusal"],
                json!(event.is_refusal()),
                "{event:?} was not written down with the verdict the kernel took"
            );
        }
        // Both answers appear, so the assertion above is not satisfied by a constant.
        assert!(every_kind().iter().any(Event::is_refusal));
        assert!(!every_kind().iter().all(Event::is_refusal));
    }

    /// The reader reads the verdict rather than deciding again. A record whose fields say one
    /// thing and whose verdict says another cannot be written by this build, and it is the
    /// fixture that fails the moment a reader goes back to re-deriving.
    #[test]
    fn a_recorded_verdict_is_read_rather_than_recomputed() {
        let contradicted = json!({
            "kind": "action_field",
            "tool": "write_file",
            "field": "path",
            "role": "routing",
            "label": {"integrity": "trusted", "confidentiality": "public"},
            "allowed": false,
            "refusal": false,
        });
        assert!(!recalled(&contradicted).expect("a field reads back").blocked);
        let blocked_gate_that_passed = json!({
            "kind": "gate_blocked",
            "gate": "action",
            "reason": "untrusted routing",
            "refusal": false,
        });
        assert!(
            !recalled(&blocked_gate_that_passed)
                .expect("a gate reads back")
                .blocked
        );
    }

    /// One line per event, so a file can be read with ordinary tools.
    #[test]
    fn an_event_fits_on_one_line() {
        let written = as_json(
            &Event::GatePassed {
                gate: "capability",
                detail: "file_read granted".to_string(),
            },
            None,
        )
        .to_string();
        assert!(!written.contains('\n'));
    }
}
