//! The lattice as Verus states it, for the contracts in this crate to name.
//!
//! Each definition here is written from the lattice's meaning, not copied from the function it
//! specifies, so a contract holds the code to the meaning. docs/development/verification.md says
//! how a contract is written and run.

use crate::command::{Asked, Expansion, Plan, Route, Step, Steps, ValuePart};
use crate::label::{Confidentiality, Integrity, Label};
use crate::permissions::{Decision, Ruling};
use crate::policy::{OutputFacts, RunFacts, RunReason};
use crate::programs::Command;
use crate::remembered::{Remembered, RememberedArg, RememberedLine, RememberedStep, Shape};
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

/// Whether two held paths are the same bytes. Uninterpreted, and `compare::same_path_buf` is
/// assumed to answer it.
pub uninterp spec fn same_path_buf(held: PathBuf, seen: PathBuf) -> bool;

/// Two argument lists of one length, equal one for one.
pub open spec fn same_texts(a: Seq<String>, b: Seq<String>) -> bool {
    a.len() == b.len() && forall|i: int| 0 <= i < a.len() ==> (#[trigger] a[i])@ == b[i]@
}

/// A vouched entry covers a step exactly where all four of its fields are the step's: the file,
/// the path that starts it, the arguments and the tree (RUN-8). Every field is named, so a field
/// added to `Command` stops the build here.
pub open spec fn command_is(
    command: Command,
    program: PathBuf,
    started_as: PathBuf,
    args: Seq<String>,
    directory: PathBuf,
) -> bool {
    let Command {
        program: held_program,
        started_as: held_started_as,
        args: held_args,
        directory: held_directory,
    } = command;
    same_path_buf(held_program, program) && same_path_buf(held_started_as, started_as)
        && same_texts(held_args@, args) && same_path_buf(held_directory, directory)
}

/// An entry is a step, in a tree, exactly where `command_is` says so of the step's resolved file,
/// the path that starts it and its arguments. A step's assignments and routes are no entry's.
pub open spec fn entry_is_step(command: Command, step: Step, directory: PathBuf) -> bool {
    let Step {
        program: _,
        resolved,
        started_as,
        args,
        // The person vouched for the expanded arguments, whichever a pattern produced.
        matched: _,
        // Refused before an entry is consulted, by `Plan::carries_an_assignment` and the write
        // question, which `run_reason_is_sound` states.
        environment: _,
        expansions: _,
        routes: _,
    } = step;
    command_is(command, resolved, started_as, args@, directory)
}

pub open spec fn some_entry_is(listed: Seq<Command>, step: Step, directory: PathBuf) -> bool {
    exists|i: int| 0 <= i < listed.len() && entry_is_step(#[trigger] listed[i], step, directory)
}

/// Every step a plan runs is an entry, in the plan's tree: each step of each pipeline, part for
/// part, and each step inside a `$(...)`, which runs before the step it feeds.
pub open spec fn steps_vouched(listed: Seq<Command>, steps: Steps, directory: PathBuf) -> bool
    decreases steps, 1nat,
{
    match steps {
        Steps::Pipeline(list) => forall|i: int|
            0 <= i < list@.len() ==> step_vouched(listed, #[trigger] list@[i], directory),
        Steps::Join { left, joiner: _, right } => steps_vouched(listed, *left, directory)
            && steps_vouched(listed, *right, directory),
        Steps::Group(inner) => steps_vouched(listed, *inner, directory),
    }
}

pub open spec fn step_vouched(listed: Seq<Command>, step: Step, directory: PathBuf) -> bool
    decreases step, 1nat,
{
    some_entry_is(listed, step, directory) && forall|i: int|
        0 <= i < step.expansions@.len() ==> expansion_vouched(
            listed,
            #[trigger] step.expansions@[i],
            directory,
        )
}

pub open spec fn expansion_vouched(
    listed: Seq<Command>,
    expansion: Expansion,
    directory: PathBuf,
) -> bool
    decreases expansion, 1nat,
{
    forall|i: int|
        0 <= i < expansion.parts@.len() ==> part_vouched(
            listed,
            #[trigger] expansion.parts@[i],
            directory,
        )
}

pub open spec fn part_vouched(listed: Seq<Command>, part: ValuePart, directory: PathBuf) -> bool
    decreases part, 1nat,
{
    match part {
        ValuePart::Substitution { source: _, steps } => steps_vouched(listed, *steps, directory),
        ValuePart::Text(_) | ValuePart::Variable(_) => true,
    }
}

/// Whether a text is a number a slot admits. Uninterpreted, and `compare::is_number` is assumed
/// to answer it.
pub uninterp spec fn is_number(text: Seq<char>) -> bool;

pub open spec fn same_environment(a: Seq<(String, String)>, b: Seq<(String, String)>) -> bool {
    a.len() == b.len() && forall|i: int|
        0 <= i < a.len() ==> (#[trigger] a[i]).0@ == b[i].0@ && a[i].1@ == b[i].1@
}

/// Two routes are the same stream, the same way of writing it, and the same path.
pub open spec fn same_route(a: Route, b: Route) -> bool {
    match (a, b) {
        (Route::Stdout { path, append }, Route::Stdout { path: other, append: other_append })
            => append == other_append && same_path_buf(path, other),
        (Route::Stderr { path, append }, Route::Stderr { path: other, append: other_append })
            => append == other_append && same_path_buf(path, other),
        (Route::Stdin { path }, Route::Stdin { path: other }) => same_path_buf(path, other),
        (Route::Both { path }, Route::Both { path: other }) => same_path_buf(path, other),
        (Route::StderrToStdout, Route::StderrToStdout) => true,
        _ => false,
    }
}

pub open spec fn same_routes(a: Seq<Route>, b: Seq<Route>) -> bool {
    a.len() == b.len() && forall|i: int| 0 <= i < a.len() ==> same_route(#[trigger] a[i], b[i])
}

/// A held argument admits exactly its own text, or a number where it is a slot.
pub open spec fn arg_admits(held: RememberedArg, seen: Seq<char>) -> bool {
    match held {
        RememberedArg::Literal(text) => text@ == seen,
        RememberedArg::Number => is_number(seen),
    }
}

pub open spec fn args_admit(held: Seq<RememberedArg>, seen: Seq<String>) -> bool {
    held.len() == seen.len() && forall|i: int|
        0 <= i < held.len() ==> arg_admits(#[trigger] held[i], seen[i]@)
}

/// A remembered step covers a step exactly where every field it holds is the step's, the
/// arguments one for one, and the step carries no expansion, which no entry holds (RUN-19). Both
/// structs are named field by field, so a field added to either stops the build here.
pub open spec fn remembered_step_matches(held: RememberedStep, step: Step) -> bool {
    let Step {
        program,
        resolved,
        started_as,
        args,
        // Which arguments a pattern produced. The arguments themselves are compared.
        matched: _,
        environment,
        expansions,
        routes,
    } = step;
    let RememberedStep {
        program: held_program,
        resolved: held_resolved,
        started_as: held_started_as,
        args: held_args,
        environment: held_environment,
        routes: held_routes,
    } = held;
    expansions@.len() == 0 && held_program@ == program@ && same_path_buf(held_resolved, resolved)
        && same_path_buf(held_started_as, started_as)
        && same_environment(held_environment@, environment@)
        && same_routes(held_routes@, routes@) && args_admit(held_args@, args@)
}

pub open spec fn steps_match(held: Seq<RememberedStep>, seen: Seq<Step>) -> bool {
    held.len() == seen.len() && forall|i: int|
        0 <= i < held.len() ==> remembered_step_matches(#[trigger] held[i], seen[i])
}

/// A remembered line covers a line exactly where both are joined the same way, part for part, and
/// each step matches the step in its place.
pub open spec fn shape_matches(held: Shape, seen: Steps) -> bool
    decreases held,
{
    match (held, seen) {
        (Shape::Pipeline(held), Steps::Pipeline(seen)) => steps_match(held@, seen@),
        (
            Shape::Join { left: held_left, joiner: held_joiner, right: held_right },
            Steps::Join { left, joiner, right },
        ) => held_joiner == joiner && shape_matches(*held_left, *left) && shape_matches(
            *held_right,
            *right,
        ),
        (Shape::Group(held), Steps::Group(seen)) => shape_matches(*held, *seen),
        _ => false,
    }
}

/// Some line of a list covers a plan exactly where one of them matches it.
pub open spec fn some_line_covers(lines: Seq<RememberedLine>, plan: Plan) -> bool {
    exists|i: int| 0 <= i < lines.len() && shape_matches(#[trigger] lines[i].steps, plan.steps)
}

/// The remembered lines cover a plan exactly where an entry, or a line in a scope, matches it
/// (RUN-19, RUN-20). The files a write may create a credential in say nothing about a line.
pub open spec fn remembered_covers(remembered: Remembered, plan: Plan) -> bool {
    let entries = crate::remembered::spec_entries(remembered);
    let scopes = crate::remembered::spec_scopes(remembered);
    (exists|i: int|
        0 <= i < entries.len() && shape_matches(#[trigger] entries[i].line.steps, plan.steps))
        || (exists|i: int|
        0 <= i < scopes.len() && some_line_covers(#[trigger] scopes[i].scope.lines@, plan))
}

} // verus!
