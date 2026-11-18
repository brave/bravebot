//! The lattice as Verus states it, for the contracts in this crate to name.
//!
//! Each definition here is written from the lattice's meaning, not copied from the function it
//! specifies, so a contract holds the code to the meaning. docs/development/verification.md says
//! how a contract is written and run.

use crate::label::{Confidentiality, Integrity, Label};
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

} // verus!
