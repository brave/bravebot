//! The lattice as Verus states it, for the contracts in this crate to name.
//!
//! Each definition here is written from the lattice's meaning, not copied from the function it
//! specifies, so a contract holds the code to the meaning. docs/development/verification.md says
//! how a contract is written and run.

use crate::command::{Plan, Route, Step, Steps};
use crate::label::{Confidentiality, Integrity, Label};
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

} // verus!
