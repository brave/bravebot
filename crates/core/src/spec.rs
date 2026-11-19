//! The lattice as Verus states it, for the contracts in this crate to name.
//!
//! Each definition here is written from the lattice's meaning, not copied from the function it
//! specifies, so a contract holds the code to the meaning. docs/development/verification.md says
//! how a contract is written and run.

use crate::command::{Asked, Plan, Route, Step, Steps};
use crate::label::{Confidentiality, Integrity, Label};
use crate::permissions::{Decision, Ruling};
use crate::policy::{OutputFacts, RunFacts, RunReason};
use std::path::PathBuf;
use vstd::prelude::*;

verus! {

pub open spec fn trusted_public() -> Label {
    Label { integrity: Integrity::Trusted, confidentiality: Confidentiality::Public }
}

/// One untrusted input makes the result untrusted.
pub open spec fn meet(a: Integrity, b: Integrity) -> Integrity {
    if a == Integrity::Untrusted || b == Integrity::Untrusted {
        Integrity::Untrusted
    } else {
        Integrity::Trusted
    }
}

/// One private input makes the result private.
pub open spec fn join(a: Confidentiality, b: Confidentiality) -> Confidentiality {
    if a == Confidentiality::Private || b == Confidentiality::Private {
        Confidentiality::Private
    } else {
        Confidentiality::Public
    }
}

/// The direction taint travels, an axis at a time: integrity stays or goes to untrusted, and
/// confidentiality stays or goes to private.
pub open spec fn degrades(from: Label, to: Label) -> bool {
    (from.integrity == to.integrity || to.integrity == Integrity::Untrusted)
        && (from.confidentiality == to.confidentiality
            || to.confidentiality == Confidentiality::Private)
}

/// Reflexive, so a relabel to the label a value already has succeeds; antisymmetric, so two labels
/// that each degrade to the other are one; and transitive, so two relabels reach no label that one
/// relabel could not.
pub proof fn degradation_is_a_partial_order(a: Label, b: Label, c: Label)
    ensures
        degrades(a, a),
        degrades(a, b) && degrades(b, a) ==> a == b,
        degrades(a, b) && degrades(b, c) ==> degrades(a, c),
{
}

/// `(T,priv)` and `(U,pub)` are incomparable, so no ranking of labels is sound.
pub proof fn the_middle_labels_are_incomparable()
    ensures
        !degrades(
            Label { integrity: Integrity::Trusted, confidentiality: Confidentiality::Private },
            Label { integrity: Integrity::Untrusted, confidentiality: Confidentiality::Public },
        ),
        !degrades(
            Label { integrity: Integrity::Untrusted, confidentiality: Confidentiality::Public },
            Label { integrity: Integrity::Trusted, confidentiality: Confidentiality::Private },
        ),
{
}

/// The label of a value computed from `a` and `b`: untrusted if either is, private if either is.
pub open spec fn taint(a: Label, b: Label) -> Label {
    Label {
        integrity: meet(a.integrity, b.integrity),
        confidentiality: join(a.confidentiality, b.confidentiality),
    }
}

/// Both inputs degrade into their taint, and the taint degrades into every label both inputs
/// degrade into, so it is the least upper bound of the two. The contracts on `Integrity::meet` and
/// `Confidentiality::join` state that they compute `meet` and `join`, so each step of `taint_all`
/// takes this bound.
pub proof fn taint_is_the_least_upper_bound(a: Label, b: Label, c: Label)
    ensures
        degrades(a, taint(a, b)),
        degrades(b, taint(a, b)),
        degrades(a, c) && degrades(b, c) ==> degrades(taint(a, b), c),
{
}

/// Every two labels have a greatest lower bound: a label that degrades into both, and into which
/// every other such label degrades. With the least upper bound, this makes the order a lattice.
pub proof fn every_two_labels_have_a_greatest_lower_bound(a: Label, b: Label)
    ensures
        exists|l: Label|
            #![trigger degrades(l, a), degrades(l, b)]
            degrades(l, a) && degrades(l, b) && forall|c: Label|
                #![trigger degrades(c, l)]
                degrades(c, a) && degrades(c, b) ==> degrades(c, l),
{
    let l = Label {
        integrity: if a.integrity == Integrity::Trusted || b.integrity == Integrity::Trusted {
            Integrity::Trusted
        } else {
            Integrity::Untrusted
        },
        confidentiality: if a.confidentiality == Confidentiality::Public
            || b.confidentiality == Confidentiality::Public {
            Confidentiality::Public
        } else {
            Confidentiality::Private
        },
    };
    assert(degrades(l, a) && degrades(l, b));
    assert forall|c: Label| degrades(c, a) && degrades(c, b) implies degrades(c, l) by {}
}

// A path is bytes Verus has no model of. Whether one is the null device is left uninterpreted, and
// `command::discards` is assumed to answer it.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExPathBuf(PathBuf);

pub uninterp spec fn null_device(path: PathBuf) -> bool;

/// A route an entry need not hold, because it opens no file: standard error joined to standard
/// output, or a stream sent to the null device.
pub open spec fn opens_no_file(route: Route) -> bool {
    match route {
        Route::Stdout { path, append: _ } => null_device(path),
        Route::Stderr { path, append: _ } => null_device(path),
        Route::Both { path } => null_device(path),
        Route::Stdin { path: _ } => false,
        Route::StderrToStdout => true,
    }
}

/// An entry holds a step's resolved file, the path that starts it and its arguments
/// (`programs::Command`), so a step may be remembered only where every other field that decides
/// what it does is empty. Every field is named, so a field added to `Step` does not compile here
/// until it is put on one side or the other.
pub open spec fn step_is_its_entry(step: Step) -> bool {
    let Step {
        // The name the line used, for a reader. What starts is `resolved` by `started_as`.
        program: _,
        resolved: _,
        started_as: _,
        args: _,
        // Which arguments a pattern produced. The arguments themselves are held.
        matched: _,
        environment,
        expansions,
        routes,
    } = step;
    environment@.len() == 0 && expansions@.len() == 0 && forall|i: int|
        0 <= i < routes@.len() ==> opens_no_file(#[trigger] routes@[i])
}

/// Every step a plan runs is its entry. A substitution is inside an expansion, which a step that is
/// its entry has none of, so there is no step below these.
pub open spec fn steps_are_their_entries(steps: Steps) -> bool
    decreases steps,
{
    match steps {
        Steps::Pipeline(steps) => forall|i: int|
            0 <= i < steps@.len() ==> step_is_its_entry(#[trigger] steps@[i]),
        Steps::Join { left, joiner: _, right } => steps_are_their_entries(*left)
            && steps_are_their_entries(*right),
        Steps::Group(inner) => steps_are_their_entries(*inner),
    }
}

/// An entry adds the tree a plan runs in, so a plan may be remembered only where every other field
/// that decides what it does is empty. Every field is named, for the reason a step's are.
pub open spec fn plan_is_its_entries(plan: Plan) -> bool {
    let Plan {
        // The line as the planner spelled it, for a reader. Two spellings that compile alike
        // are one plan.
        line: _,
        directory: _,
        steps,
        writes,
        reads,
        stdin,
    } = plan;
    steps_are_their_entries(steps) && writes@.len() == 0 && reads@.len() == 0 && stdin is None
}

/// A request adds to its plan only what an entry does not need to hold. Every field is named, for
/// the reason a step's are.
pub open spec fn request_is_its_entries(plan: Plan, asked: Asked) -> bool {
    let Asked { scopes, unconfined, protected } = asked;
    plan_is_its_entries(plan) && !scopes && !unconfined && !protected
}

/// A line is let through only by one of the four standing answers or by the audited table or a
/// rule, never by nothing.
pub(crate) open spec fn reason_asks(reason: RunReason) -> bool {
    match reason {
        RunReason::Ruled(ruling) => ruling != Ruling::Allow,
        RunReason::ReadProven | RunReason::DirectoryTrust | RunReason::Vouched
            | RunReason::Remembered => false,
        _ => true,
    }
}

/// What deciding whether to ask has to hold, whatever order the questions are asked in.
///
/// Private input, a requested scope, a write and an assignment are asked about every time, and so
/// is a line outside the root that no entry names (RUN-5, RUN-6, RUN-8, SANDBOX-26). A line goes
/// unasked only on an answer that covers it: an `allow` rule, a proof from the audited table, the
/// person's trust of the directory, a vouch, or a remembered line. An `ask` or `deny` rule is
/// never overturned by a standing answer. A fed reference is never let through by a standing answer
/// or the directory, none of which records what was fed (RUN-3).
pub(crate) open spec fn run_reason_is_sound(facts: RunFacts, reason: RunReason) -> bool {
    let RunFacts {
        releases_private,
        requested,
        writes,
        at_root,
        vouched,
        assignment,
        rule,
        read_proven_trusted,
        fed,
        held,
        remembered,
    } = facts;
    let unasked = !reason_asks(reason);
    (releases_private || requested || writes || assignment || (!at_root && !vouched)
        ==> !unasked)
        && (rule == Decision::Ruled(Ruling::Ask) || rule == Decision::Ruled(Ruling::Deny)
        ==> !unasked)
        && (unasked ==> match reason {
            RunReason::Ruled(ruling) => rule == Decision::Ruled(ruling)
                && ruling == Ruling::Allow,
            RunReason::ReadProven => read_proven_trusted,
            RunReason::DirectoryTrust => held && !fed,
            RunReason::Vouched => vouched && !fed,
            RunReason::Remembered => remembered && !fed,
            _ => false,
        })
}

/// What labelling a line's output has to hold.
///
/// Output is trusted only where the audited table proved it so or the person vouched for every
/// step, and never for a line that asked for a scope or to run unconfined (SANDBOX-26,
/// SANDBOX-29), and never on a vouch for a line carrying an assignment (RUN-8) or fed untrusted
/// content (RUN-3). Output is always private unless the table proved the label.
pub(crate) open spec fn output_label_is_sound(
    facts: OutputFacts,
    opaque: Label,
    label: Label,
) -> bool {
    let OutputFacts { requesting, proven, vouched, assignment, fed } = facts;
    (label.integrity == Integrity::Trusted ==> !requesting && (proven is Some || (vouched
        && !assignment && fed != Some(Integrity::Untrusted))))
        && (proven is None ==> label.confidentiality == Confidentiality::Private
        || label == opaque)
        && (requesting ==> label == opaque)
        && (proven is Some && !requesting ==> label == proven->Some_0)
}

/// What a decision a rule recorded counts as in a meet: an undecided boundary as untrusted, since
/// nobody has said.
pub open spec fn decision_integrity(decision: Option<Integrity>) -> Integrity {
    match decision {
        Some(integrity) => integrity,
        None => Integrity::Untrusted,
    }
}

/// `start` met with every decision in `bearing`, in order.
pub open spec fn meet_of_rules(start: Integrity, bearing: Seq<Option<Integrity>>) -> Integrity
    decreases bearing.len(),
{
    if bearing.len() == 0 {
        start
    } else {
        meet(meet_of_rules(start, bearing.drop_last()), decision_integrity(bearing.last()))
    }
}

/// A meet of rules is trusted exactly where the start is and every rule that bears on the path is
/// a trust (TRUST-3, TRUST-4). So a rule can only lower the answer: one distrust or one undecided
/// boundary anywhere under the path makes the whole of it untrusted.
pub proof fn a_meet_of_rules_is_trusted_only_where_every_rule_is(
    start: Integrity,
    bearing: Seq<Option<Integrity>>,
)
    ensures
        (meet_of_rules(start, bearing) == Integrity::Trusted) <==> (start == Integrity::Trusted
            && forall|i: int|
            0 <= i < bearing.len() ==> #[trigger] bearing[i] == Some(Integrity::Trusted)),
    decreases bearing.len(),
{
    if bearing.len() > 0 {
        let before = bearing.drop_last();
        a_meet_of_rules_is_trusted_only_where_every_rule_is(start, before);
        assert forall|i: int| 0 <= i < before.len() implies #[trigger] before[i] == bearing[i] by {}
        let last = bearing.len() - 1;
        assert(bearing[last] == bearing.last());
        if meet_of_rules(start, bearing) == Integrity::Trusted {
            assert forall|i: int| 0 <= i < bearing.len() implies #[trigger] bearing[i] == Some(
                Integrity::Trusted,
            ) by {
                if i < last {
                    assert(before[i] == bearing[i]);
                }
            }
        }
        if start == Integrity::Trusted && forall|i: int|
            0 <= i < bearing.len() ==> #[trigger] bearing[i] == Some(Integrity::Trusted) {
            assert forall|i: int| 0 <= i < before.len() implies #[trigger] before[i] == Some(
                Integrity::Trusted,
            ) by {
                assert(before[i] == bearing[i]);
            }
            assert(bearing[last] == Some(Integrity::Trusted));
        }
    }
}

/// How much a decision grants: a distrust least, then an undecided boundary, then a trust.
pub open spec fn strength(decision: Option<Integrity>) -> u8 {
    match decision {
        Some(Integrity::Untrusted) => 0,
        None => 1,
        Some(Integrity::Trusted) => 2,
    }
}

/// `a` grants no more than `b`. Written from the order itself rather than from `strength`, so a
/// `strength` that ranked an undecided boundary above a trust fails the contract on it.
pub open spec fn grants_no_more(a: Option<Integrity>, b: Option<Integrity>) -> bool {
    a == Some(Integrity::Untrusted) || a == b || (a is None && b == Some(Integrity::Trusted))
}

} // verus!
