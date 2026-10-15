//! The sentences about a delegate that has ended, and about the checkout it worked in.
//!
//! Every one is built from what the driver already holds: which way the run came back, its round
//! count and the bound its spec fixed, whether a person stopped it, and the checkout's recorded
//! state. None reads a report, which is why they can live apart from the presentation of one in
//! `collect_delegates`.

use std::time::Duration;

use bravebot_core::delegate::{DelegateId, Finish};

use super::{TOOL_BUDGET_SPENT, TurnError};
use crate::tools;

/// How a delegate's run came back, from the way the result arrived, the run's own counts and the
/// fixed name of a failure, so the record says why it stopped without holding what any service or
/// tool said (TRACE-8).
pub(super) fn finish_of<T>(
    delegated: &Result<T, TurnError>,
    ran: Option<(Duration, usize)>,
) -> Finish {
    match (delegated, ran) {
        (_, None) => Finish::Lost,
        (Ok(_), Some((took, rounds))) => Finish::Answered { took, rounds },
        (Err(error), Some((took, rounds))) => match error.ending() {
            crate::outcome::Ending::Stopped { .. } => Finish::Stopped { took, rounds },
            ending => Finish::Failed {
                took,
                rounds,
                why: ending
                    .diagnosis()
                    .map_or("internal", |diagnosis| diagnosis.category.name()),
            },
        },
    }
}

/// The three sentences around a delegate's report: the person's note, the driver's account of a
/// stop, and the planner's account of a limit.
///
/// Two counts the driver already holds: the rounds this delegate made, and the bound its spec
/// fixed before it started. Nothing here reads the report, so the comparison is between
/// driver-held numbers and never between bytes a delegate wrote (DELEGATE-26).
pub(super) fn answered_sentences(
    kind: &str,
    id: DelegateId,
    rounds: usize,
    bound: usize,
    stop_honoured: bool,
) -> (String, String, String) {
    let tally = tools::tally(rounds, "round", "rounds");
    let reached = rounds >= bound;
    let note = match (stop_honoured, reached) {
        (true, _) => format!(
            "the person stopped a {kind} delegate after {tally} and it answered with \
             what it had"
        ),
        (false, true) => format!(
            "a {kind} delegate reached its limit of {tally} and answered with what it \
             had, so it may not be finished"
        ),
        (false, false) => format!("a {kind} delegate answered after {tally}"),
    };
    // Said before the report, in the driver's own words, as whose work a report is
    // is (DELEGATE-14). The report keeps the label its own context earned.
    let stopped = match stop_honoured {
        true => format!("The person stopped the {kind} delegate {id} before it reported. "),
        false => String::new(),
    };
    // Beside the report rather than inside it, so the planner reads the same sentence
    // whether it is shown the words or handed a reference to them (AGENT-4). The
    // delegate is told on its limiting round to say what stopped it (DELEGATE-6), and
    // that is model text the planner may never see.
    let bound_text = match (stop_honoured, reached) {
        (false, true) => format!(
            " It stopped at its limit of {} and may not be finished.",
            tools::tally(bound, "round", "rounds")
        ),
        _ => String::new(),
    };
    (note, stopped, bound_text)
}

/// The person's note and the planner's message for a delegate that ended without answering.
pub(super) fn unfinished_account(
    finish: &Finish,
    ran: Option<(Duration, usize)>,
    bound: usize,
    id: DelegateId,
) -> (String, String) {
    // The same comparison the answering arm makes, on the same two driver-held counts.
    // A delegate that spent its whole budget and then failed to answer is the common
    // way the bound is reached rather than a corner of it, and the planner told only
    // that it did not finish retries the identical task (DELEGATE-26).
    let reached = ran.is_some_and(|(_, rounds)| rounds >= bound);
    let bound_text = match reached {
        true => format!(
            " It had spent its limit of {}, so the same task will not get further.",
            tools::tally(bound, "round", "rounds")
        ),
        false => String::new(),
    };
    // The same fixed name the trail records, and never the error's own text.
    let note = match finish {
        Finish::Failed { why, .. } => format!("the delegate could not finish ({why})"),
        Finish::Stopped { .. } => "the delegate was stopped".to_string(),
        _ => "the delegate could not finish".to_string(),
    };
    let note = match reached {
        true => format!(
            "{note}, having spent its limit of {}",
            tools::tally(bound, "round", "rounds")
        ),
        false => note,
    };
    let body = format!("{TOOL_BUDGET_SPENT} The delegate {id} did not finish.{bound_text}");
    (note, body)
}

/// What the planner is told about a delegate's checkout, and the clause the person's note gains.
pub(super) fn checkout_account(
    retired: crate::workspace::Retired,
    checkout: &crate::workspace::CheckoutInfo,
) -> (String, String) {
    use crate::workspace::Retired;
    match retired {
        Retired::Removed => (
            format!(
                "Its checkout {} was removed, since nothing was done in it.",
                checkout.id()
            ),
            "its checkout was removed".to_string(),
        ),
        // The size is the person's alone. It is a measure of files programs wrote, and the
        // planner decides nothing by it.
        Retired::Kept => (
            format!(
                "Its checkout {} of commit {} was kept at {}, since something was done in \
                 it. {} apply_checkout with checkout \"{}\" brings the files it names \
                 back into the working directory, asking the person about each.",
                checkout.id(),
                checkout.commit(),
                checkout.path().display(),
                crate::delegate::checkout_candidates(&checkout.candidates()),
                checkout.id()
            ),
            match checkout.size() {
                Some(size) => format!(
                    "its checkout {} was kept, taking {} on disk",
                    checkout.id(),
                    size.spelled()
                ),
                None => format!("its checkout {} was kept", checkout.id()),
            },
        ),
        Retired::Stuck => (
            format!(
                "Its checkout {} of commit {} could not be removed and is at {}.",
                checkout.id(),
                checkout.commit(),
                checkout.path().display()
            ),
            "its checkout could not be removed".to_string(),
        ),
    }
}
