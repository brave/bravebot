//! Turning the agent's types into JSON, and reading answers back.
//!
//! One function per row of the protocol's type table, so the projection lives in one
//! place and a change to it is a change to one function. Pure: nothing here does I/O,
//! which is what makes the whole table testable in a few milliseconds.
//!
//! Two rules run through all of it.
//!
//! **Send the discriminant, not the prose.** [`Landing::describe`] and
//! [`Reach::describe`] return sentences meant for a screen. They are wording, they will
//! change, and a client that matches on them is matching on prose — which is how a
//! message that merely mentions a refusal becomes one. The tag is the contract; a
//! front-end may render its own words from it.
//!
//! **Unknown input degrades toward less trust.** Reading a tag we do not recognise gives
//! the more restrictive variant, never the more permissive one, mirroring what
//! `Snapshot` and `Record::trust_map` already do upstream. There is no error case for a
//! decision, because refusing to parse an answer and refusing the write it answers are
//! the same outcome and only one of them is honest about it. [`composed`] is the one
//! function here that refuses, and it reads a claim a request makes rather than an answer
//! to a question: there is no quieter reading of a client asking for a tag it may not
//! have, and a turn that went out untagged instead would be drawn as a prompt nobody
//! typed.

use bravebot_agent::confirm::{
    Decision, Intent, OutputRequest, RunDecision, RunRequest, VetRequest, VouchRequest,
    WriteRequest,
};
use bravebot_agent::conversation::{Composed, Said};
use bravebot_agent::diff::Change;
use bravebot_agent::report::{Activity, Landing, Phase, Reach, Shown};
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::todo::{Row, Status};
use serde_json::{Value, json};

use crate::protocol::Failure;

/// Unchanged lines shown either side of a change, for orientation.
///
/// The same figure the terminal confirmer uses, so the two front-ends show a reviewer
/// the same amount of context and an approval means the same thing in both.
const CONTEXT_LINES: usize = 2;

// ---------------------------------------------------------------- enums, outbound

pub fn intent(intent: Intent) -> &'static str {
    match intent {
        Intent::Create => "create",
        Intent::Overwrite => "overwrite",
        Intent::Edit => "edit",
    }
}

pub fn phase(phase: Phase) -> &'static str {
    match phase {
        Phase::Planning => "planning",
        Phase::Thinking => "thinking",
        Phase::Compacting => "compacting",
        Phase::Reconnecting => "reconnecting",
    }
}

pub fn reach(reach: Reach) -> &'static str {
    match reach {
        Reach::NotThePlanner => "not_the_planner",
        Reach::NoModel => "no_model",
    }
}

pub fn landing(landing: Landing) -> &'static str {
    match landing {
        Landing::Context => "context",
        Landing::Quarantined => "quarantined",
        Landing::Reserved => "reserved",
    }
}

pub fn status(status: Status) -> &'static str {
    match status {
        Status::Pending => "pending",
        Status::Active => "active",
        Status::Done => "done",
    }
}

// ---------------------------------------------------------------- inbound

/// Read a decision the front-end sent.
///
/// Total, and deliberately so. Only the exact word "approve" approves; anything else —
/// a typo, a tag from a newer client, a null, a number — is a refusal. An approval is
/// the one thing in this protocol that must never be produced by accident, so it is the
/// one thing that gets no benefit of the doubt.
pub fn decision(value: &Value) -> Decision {
    match value.as_str() {
        Some("approve") => Decision::Approve,
        _ => Decision::Reject,
    }
}

/// Read an answer to a run, which is two answers rather than one.
///
/// `remember` is only ever read from an approval. A refusal that remembered would be a
/// standing "never ask me again" nobody asked for, and the kernel has no such state to
/// put it in — but the more important reason is that a client sending
/// `{"decision": "reject", "remember": true}` has said something incoherent, and the
/// reading that costs least is the one that forgets.
///
/// Anything other than a literal `true` is a no, on the same grounds as [`decision`]:
/// remembering is nearly as consequential as approving, since it answers every later
/// question about the same command.
pub fn run_decision(decision_value: &Value, remember_value: &Value) -> RunDecision {
    match decision(decision_value) {
        Decision::Approve if remember_value.as_bool() == Some(true) => {
            RunDecision::approve_always()
        }
        Decision::Approve => RunDecision::approve(),
        Decision::Reject => RunDecision::reject(),
    }
}

// ---------------------------------------------------------------- structures

/// One line of a diff.
///
/// `Elided` carries a count rather than text: it stands for a run of unchanged lines
/// nobody needs to see, and giving it a `text` field would invite a client to render an
/// empty string where the agent meant "forty lines you do not care about".
pub fn change(change: &Change) -> Value {
    match change {
        Change::Kept(text) => json!({ "kind": "kept", "text": text }),
        Change::Added(text) => json!({ "kind": "added", "text": text }),
        Change::Removed(text) => json!({ "kind": "removed", "text": text }),
        Change::Elided(lines) => json!({ "kind": "elided", "lines": lines }),
    }
}

/// A call the turn made, as the person watching sees it.
///
/// `note: null` means the call has not finished. That is the only thing distinguishing a
/// running line from one that finished with nothing to say, so it is sent as an explicit
/// null rather than an absent key: a client reading an absent key as "finished, no note"
/// would draw every in-flight call as complete.
///
/// `waitedSeconds` is what the call spent at a model of its own, null for the nearly all of
/// them that asked none. Whole seconds, as `remainingSeconds` is: what it answers is whether a
/// slow call was slow because a model was, and that question is not decided in milliseconds.
pub fn activity(activity: &Activity) -> Value {
    json!({
        "verb": activity.verb,
        "target": activity.target,
        "note": activity.note,
        "failed": activity.failed,
        "untrusted": activity.untrusted,
        "changes": activity.changes.iter().map(change).collect::<Vec<_>>(),
        "waitedSeconds": activity.waited.map(|waited| waited.as_secs()),
    })
}

/// Quarantined content, released for a screen and stopping there.
///
/// The preview is already trimmed by the kernel to twelve lines of at most a hundred and
/// sixty characters, and `lines` is the true total, so a client can say what it left out.
/// A preview that stops without saying so reads as the whole thing.
pub fn shown(shown: &Shown) -> Value {
    json!({
        "origin": shown.origin,
        "reach": reach(shown.reach),
        "label": shown.label,
        "preview": shown.preview,
        "lines": shown.lines,
    })
}

/// One task from the plan a turn is working to.
pub fn row(row: &Row) -> Value {
    json!({ "content": row.content, "status": status(row.status) })
}

/// A whole transcript, each prompt carrying the ordinal `session.fork` cuts on.
///
/// The ordinal is counted here because it is the agent's own count of what the user said, and
/// [`crate::fork::cut`] resolves it against that count. A client that counts instead holds a
/// second copy of the rule, and the two agree only for as long as nobody adds a kind of user
/// message: a turn nudged for spending its tool budget and a shell line a person ran themselves
/// are both `Said::User` here, neither is drawn as a prompt by a window watching a live session,
/// and a fork of anything after one of them arrives short and is refused.
///
/// Sent beside each thing said rather than composed from the list, so that a client drawing one
/// kind of user message differently costs nobody a working fork.
pub fn recounted(said: &[Said]) -> Vec<Value> {
    let mut prompt = 0usize;
    said.iter()
        .map(|entry| {
            let mut value = self::said(entry);
            if matches!(entry, Said::User(_)) {
                value["prompt"] = json!(prompt);
                prompt += 1;
            }
            value
        })
        .collect()
}

/// One thing said, from a conversation nobody watched happen.
///
/// A `Tool` line says only that a call happened and what it was about. The record does
/// not store what came of it, so there is no note here and a client must not draw one:
/// live turns get `tool.finished` with an outcome, replayed ones do not, and inventing
/// one would be worse than the gap.
///
/// A line somebody composed rather than typed crosses as a tag of its own and the fields the
/// client needs to write its own row, and its text is dropped here rather than sent. The
/// projection carries it for a transcript that draws the message plainly, and this client does not
/// draw one: sending both would offer a choice between a tag and a sentence, and the sentence is
/// the one whose words came out of a file.
fn said(said: &Said) -> Value {
    match said {
        Said::User(text) => json!({ "kind": "user", "text": text }),
        Said::Assistant(text) => json!({ "kind": "assistant", "text": text }),
        Said::Tool(text) => json!({ "kind": "tool", "text": text }),
        Said::Composed {
            why: Composed::Attached { path },
            ..
        } => json!({ "kind": "attached", "path": path }),
        Said::Composed {
            why: Composed::Watch { number, path },
            ..
        } => json!({ "kind": "watch", "number": number, "path": path }),
        // No field of its own, because the row a transcript draws for this is about the turn
        // rather than about anything in it, and no prose either: the words are a prompt the front
        // end composed, and a client offered both would be offered the choice the tag removes.
        Said::Composed {
            why: Composed::Consolidation,
            ..
        } => json!({ "kind": "consolidation" }),
    }
}

/// The word a `turn.send` may carry to say the front end composed its prompt, if it carries one.
///
/// The one tag a request may claim, and the reason this is a closed match rather than a parse of
/// whatever the field holds. [`Composed::Attached`] and [`Composed::Watch`] are the agent's own
/// account of what a turn did, so a front end able to name either would be a front end able to
/// have a transcript draw a file's row, or a watch's, around a line a person typed, which is the
/// escape the tags exist to close. Naming one of those is refused rather than ignored: a client
/// asking for a tag it may not have has misunderstood the protocol, and a turn that quietly went
/// out untagged would be drawn as a prompt nobody typed.
///
/// Absent is the ordinary case and means a person typed it.
pub fn composed(value: Option<&Value>) -> Result<Option<Composed>, Failure> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(word)) if word == "consolidation" => Ok(Some(Composed::Consolidation)),
        Some(other) => Err(Failure::bad_request(format!(
            "`composed` may only say `consolidation`, not {other}"
        ))),
    }
}

/// A write awaiting a person's decision.
///
/// The complete body is **not** sent, only the diff. A reviewer reads a few lines rather
/// than spotting a difference in a whole file, and putting `contents` on the wire invites
/// a front-end to show that instead. `existing` says whether anything would be lost
/// without shipping what it was.
///
/// `untrusted` means the body came from somewhere nobody vouched for. Reviewing that is a
/// different act from reviewing the model's own work, and a front-end must not make the
/// two look alike.
///
/// `credentials` is why a write the path's own rule would not have asked about is being
/// asked about at all: the scan inferred a secret in the body and put the guess to a
/// person. Leaving it off the wire would send a front-end an approval prompt for an
/// ordinary-looking write with the reason for it removed.
pub fn write_request(id: u64, request: &WriteRequest) -> Value {
    let diff = &request.diff;
    json!({
        "request": id,
        "path": request.path,
        "intent": intent(request.intent),
        "untrusted": request.untrusted,
        "remark": request.remark.as_ref().map(|r| json!({"preview": r.preview, "lines": r.lines, "label": r.label})),
        // Already a kind, a location and a masked preview each, so sending them repeats no
        // character of the value. The terminal draws these beside the diff for the same
        // reason `CONTEXT_LINES` is shared: the decision is about these lines, and an
        // approval has to mean the same thing in both front-ends.
        "credentials": request.credentials,
        "existing": request.existing.is_some(),
        "added": diff.added(),
        "removed": diff.removed(),
        // False when the diff had to give up on an exact answer, which happens on two
        // large and wholly dissimilar files: the table it needs is quadratic and is
        // bounded rather than allowed to allocate without limit. A reviewer is entitled
        // to know they are reading an approximation of the change.
        "exact": diff.is_exact(),
        "changes": diff.condensed(CONTEXT_LINES).iter().map(change).collect::<Vec<_>>(),
    })
}

/// A pipeline awaiting a person's decision.
///
/// Every stage carries the name the model wrote **and** what it resolved to, because they
/// are not the same claim: `$PATH` decides what `grep` means, and somebody vouching for a
/// program should be looking at the binary they are vouching for. A front-end that shows
/// only one of the two is showing the wrong one.
///
/// `display` is the agent's own rendering of the argv, so both front-ends put the same
/// characters in front of a reviewer — argument boundaries included, which is where a
/// space in a filename stops being cosmetic.
///
/// `releasesPrivate` is a second and independent reason to be careful, on confidentiality
/// rather than integrity: bytes going into a program are released somewhere this policy
/// stops governing. It is sent separately from the stages because it is not a property of
/// any one of them.
pub fn run_request(id: u64, request: &RunRequest) -> Value {
    let stages: Vec<Value> = request
        .plan
        .steps()
        .iter()
        .map(|stage| {
            json!({
                "program": stage.program,
                "resolved": stage.resolved,
                "args": stage.args,
                "display": stage.as_written(),
            })
        })
        .collect();

    json!({
        "request": id,
        "stages": stages,
        "directory": request.directory(),
        // The line as the planner spelled it, which is not what an approval binds to: two
        // spellings that compile to the same plan are one endorsement. It is sent because a
        // reader with only the plan has nothing to compare it against, and a compiler that got
        // the line wrong would then produce a prompt indistinguishable from a correct one. Empty
        // for a call that was spelled as argv stages rather than as a line, and a front end
        // draws nothing for those; the field is sent either way, so a front end can tell a call
        // with no line from a build that does not send the field at all.
        "line": request.plan.line,
        "plan": request.plan.steps.display(),
        "writes": request.plan.writes,
        "stdin": request.stdin,
        "releasesPrivate": request.releases_private(),
        // What the line reaches that nothing here holds: a container daemon, a tool already
        // logged in, the ssh agent, a machine's metadata service. Nothing is handed over when one
        // is used and nobody can refuse it, so the only thing available is that whoever grants it
        // is told what they are granting. Sent as the kind and the word that named it rather than
        // as a sentence, for the reason a vetting verdict is: the words belong to whichever front
        // end draws them. Empty for nearly every line, and a front end draws nothing for those.
        "ambient": request
            .ambient_authority()
            .iter()
            .map(|spent| json!({ "authority": spent.authority.name(), "named": spent.named }))
            .collect::<Vec<_>>(),
        // What approving-and-remembering would cover, which is the thing the second
        // answer needs to be about. A pipeline vouches for all of its stages: one that
        // still had to ask about a stage would not have stopped asking.
        "vouches": request
            .would_vouch_for()
            .iter()
            .map(|command| json!({ "program": command.program, "args": command.args, "display": command.display() }))
            .collect::<Vec<_>>(),
        "summary": request.summary(),
    })
}

/// A command's output the planner has asked to read.
///
/// The bytes are here in full, and that is the point rather than an oversight: a person
/// deciding whether the model may read something must be reading it themselves, so a
/// front-end that truncates this is asking them to approve what they cannot see. Unlike
/// every other question in this file the answer rests on the content, which is why
/// [`crate::turn::BridgeConfirmer::confirm_read_output`] cannot fall back to anything
/// except no.
///
/// It is released for a screen and stops there. Nothing here may be fed back to the model
/// by the front-end; approving is how it reaches the planner, and that path runs through
/// the kernel.
pub fn output_request(id: u64, request: &OutputRequest) -> Value {
    json!({
        "request": id,
        "command": request.command,
        "reference": request.reference,
        "lines": request.lines,
        "output": request.output,
        "vetting": vetting(request.verdict, request.reason.as_deref()),
        "summary": request.summary(),
    })
}

/// A quarantined file the model would like to read.
///
/// `truncated` is load-bearing: a preview that stops without saying so reads as the whole
/// file, and a person vouching for a path on the strength of its first few lines should
/// know that is what they are doing.
pub fn vouch_request(id: u64, request: &VouchRequest) -> Value {
    json!({
        "request": id,
        "path": request.path,
        "preview": request.preview,
        "truncated": request.truncated,
        "vetting": vetting(request.verdict, request.reason.as_deref()),
    })
}

// ---------------------------------------------------------------- asking

/// A series of questions, released for display.
///
/// The prompts are already shaped by the kernel — one row per choice, in order, nothing
/// filtered or reordered — so this copies rather than formats. That is the point of the
/// shaping happening up there: what the model wrote must not be able to decide which
/// options a person is shown the existence of, and a front-end that built rows itself would
/// be the place that decision crept back in.
///
/// `index` travels with every row so a selection can be reported back as data rather than
/// by matching label text, and `key` travels with every prompt so a front-end can tell two
/// questions apart without reimplementing the rule for what makes them different.
pub fn ask_request(id: u64, asking: &Asking) -> Value {
    json!({
        "request": id,
        "prompts": asking
            .prompts
            .iter()
            .map(|prompt| {
                json!({
                    "header": prompt.header,
                    "question": prompt.question,
                    "multiple": prompt.multiple,
                    "key": prompt.key,
                    "rows": prompt
                        .rows
                        .iter()
                        .map(|row| json!({
                            "index": row.index,
                            "label": row.label,
                            "detail": row.detail,
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// Read the answers a front-end sent.
///
/// Total, like every other reader here. Anything unreadable is [`Answer::Declined`] — which
/// is a real answer rather than an error, and the one that assumes least about what somebody
/// meant.
///
/// A typed answer wins over choices when both are present. It is the more specific thing to
/// have done, and a client that sends both has said something ambiguous that should not
/// resolve into a selection nobody made.
pub fn answers(value: &Value) -> Vec<Answer> {
    let Some(list) = value.as_array() else {
        // Not a list at all. Nothing here claims a person answered anything, which is
        // exactly what the empty reply means.
        return Vec::new();
    };
    list.iter().map(answer).collect()
}

fn answer(value: &Value) -> Answer {
    let Some(object) = value.as_object() else {
        return Answer::Declined;
    };

    if let Some(text) = object.get("typed").and_then(Value::as_str) {
        return Answer::Typed(text.to_string());
    }

    match object.get("chosen").and_then(Value::as_array) {
        Some(chosen) => Answer::Chosen(
            chosen
                .iter()
                .filter_map(Value::as_u64)
                .map(|index| index as usize)
                .collect(),
        ),
        None => Answer::Declined,
    }
}

/// Hold answers to the questions they answer.
///
/// Three things are checked, and each of them is a way a front-end could otherwise put words
/// in a person's mouth:
///
/// - **An index must name a choice that exists.** An out-of-range one is dropped rather than
///   passed on, because what it would select is undefined and "undefined" must not become a
///   selection.
/// - **A single-choice question gets at most one.** The model asked for one; a reply with
///   three has not answered *that* question, and taking the first is the reading that adds
///   nothing.
/// - **A selection with nothing left in it is a decline**, not an empty choice. Somebody who
///   picked only options that do not exist has not picked anything.
///
/// Extra answers beyond the questions asked are dropped. Missing ones are simply absent: the
/// kernel reads a short list as declines for the rest, so there is nothing to pad with and
/// padding would be this code answering on somebody's behalf.
pub fn fitted(answers: Vec<Answer>, asking: &Asking) -> Vec<Answer> {
    answers
        .into_iter()
        .zip(&asking.prompts)
        .map(|(answer, prompt)| match answer {
            Answer::Chosen(indices) => {
                let mut kept: Vec<usize> = indices
                    .into_iter()
                    .filter(|index| *index < prompt.rows.len())
                    .collect();
                if !prompt.multiple {
                    kept.truncate(1);
                }
                if kept.is_empty() {
                    Answer::Declined
                } else {
                    Answer::Chosen(kept)
                }
            }
            other => other,
        })
        .collect()
}

/// Advice for the person, never an approval or planner input.
fn vetting(verdict: bravebot_core::vetting::Verdict, reason: Option<&str>) -> Value {
    json!({ "verdict": verdict.word(), "reason": reason, "detail": match verdict {
        bravebot_core::vetting::Verdict::Inconclusive(detail) => Some(detail), _ => None,
    } })
}

pub fn vet_request(id: u64, request: &VetRequest) -> Value {
    json!({ "request": id, "origin": request.origin, "expects": request.expects,
        "content": request.content, "lines": request.lines,
        "vetting": vetting(request.verdict, request.reason.as_deref()) })
}
