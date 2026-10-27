//! The sentences about a delegate that has ended, and about the checkout it worked in.
//!
//! Every one is built from what the driver already holds: which way the run came back, its round
//! count and the bound its spec fixed, whether a person stopped it, and the checkout's recorded
//! state. None reads a report, which is why they can live apart from the presentation of one in
//! `collect_delegates`.

use std::time::{Duration, Instant};

use bravebot_core::delegate::{DelegateId, Finish};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;

use super::{TOOL_BUDGET_SPENT, TurnError, Worked, Working};
use crate::report::Reporter;
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

/// What a spent token budget adds beside a delegate's report: a clause for the person's note and
/// a sentence for the planner, both empty where the delegate carried no budget or stayed under it.
///
/// Two counts the driver holds: the budget its spec fixed and the tokens its backend reported,
/// never a report's bytes (DELEGATE-29). `answered` says whether there is a report to sit beside:
/// where there is none, the sentence says the same task will not get further.
pub(super) fn budget_sentences(
    budget: Option<u64>,
    tokens: u64,
    answered: bool,
) -> (String, String) {
    let Some(budget) = budget.filter(|budget| tokens >= *budget) else {
        return (String::new(), String::new());
    };
    match answered {
        true => (
            format!("it spent its budget of {budget} tokens, so it may not be finished"),
            format!(" It spent its budget of {budget} tokens and may not be finished."),
        ),
        false => (
            format!("having spent its budget of {budget} tokens"),
            format!(
                " It had spent its budget of {budget} tokens, so the same task will not get \
                 further."
            ),
        ),
    }
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
        Retired::Stuck => stuck_checkout(checkout.id(), checkout.commit(), checkout.path()),
    }
}

/// What the delegate did, for the person's note and the planner's message: how many calls ran a
/// program and how many changed a file, and where (GROUND-2, GROUND-3).
///
/// Fixed words and two counts the driver holds. No path, no command and no word of a report, so
/// a delegate cannot speak to the planner through them (GROUND-6).
pub(super) fn work_sentences(work: crate::outcome::Work, in_checkout: bool) -> (String, String) {
    let did = format!(
        "ran {} and made {}{}",
        tools::tally(work.commands, "command", "commands"),
        tools::tally(work.writes, "write", "writes"),
        match in_checkout {
            true => " in its own checkout",
            false => "",
        }
    );
    (format!("it {did}"), format!(" It {did}."))
}

/// What is said of a checkout nothing was done in that could not be removed: where it is and the
/// commit it holds, for the planner, and the fact alone for the person.
fn stuck_checkout(id: &str, commit: &str, path: &std::path::Path) -> (String, String) {
    (
        format!(
            "Its checkout {id} of commit {commit} could not be removed and is at {}.",
            path.display()
        ),
        "its checkout could not be removed".to_string(),
    )
}

/// The bounds a delegate's spec fixed, which how it ended is counted against.
#[derive(Clone, Copy)]
pub(super) struct Limit {
    /// The most rounds of tool calls its spec allows (TRACE-8).
    pub rounds: usize,
    /// The most tokens its spec allows (DELEGATE-29).
    pub budget: Option<u64>,
}

/// A delegate taken back: how it ended, what it cost, and the checkout it leaves to account for.
///
/// What `collect_delegates` presents from, once the decisions, clock and record of the run are
/// already taken back. Nothing here is a report that has been presented.
pub(super) struct Joined {
    pub id: DelegateId,
    pub limit: Limit,
    pub stop: bravebot_core::cancel::DelegateStop,
    /// The run's result, or why none came back.
    pub delegated: Result<crate::delegate::Delegated, TurnError>,
    /// How long the run took and how many rounds it made, where it came back at all.
    pub ran: Option<(Duration, usize)>,
    /// What its own backend reported it spent on a failed run: the part that completed.
    pub partial: crate::outcome::Spent,
    pub finish: Finish,
    /// What its own backend reported it spent.
    pub tokens: u64,
    /// What it did, and whether it did it in a checkout of its own, where it came back at all.
    pub work: Option<(crate::outcome::Work, bool)>,
    /// The checkout it worked in, and what retiring it did.
    pub retired: Option<(
        crate::workspace::Retired,
        std::sync::Arc<crate::workspace::CheckoutInfo>,
    )>,
}

/// Join a delegate's thread and take back what it decided, how long it waited and how it ended.
///
/// Nothing here reads a report: the presentation of one stays with `collect_delegates`.
#[expect(
    clippy::too_many_arguments,
    reason = "the delegate, policy, reporter and the turn's wait, spend, notice and workspace tallies are separate state"
)]
pub(super) fn join_delegate<S: Sink, R: Reporter>(
    working: Working<'_>,
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
    waits: &mut crate::timing::DelegateWait,
    spent: &mut crate::timing::Elapsed,
    notices: &mut Vec<String>,
    worked: &mut Worked,
    round: usize,
) -> Joined {
    let id = working.id;
    reporter.delegate_waiting(id);
    // After the report above, not before it: what a delegate's requests are clipped to is the
    // wait itself, and a reporter that draws a screen or blocks on the lock another delegate
    // holds is the parent's own overhead. Starting the window first would charge whatever a
    // request happened to overlap of it as inference the parent never spent waiting.
    let joined_at = Instant::now();
    #[cfg(test)]
    if let Some(started) = working.join_started {
        // A closed receiver means the test observer has already exited.
        let _ = started.send(());
    }
    let (delegated, ran, partial, requests, work) = match working.handle.join() {
        Ok((ended, partial, requests)) => {
            // Before anything else, and on both of the ways a run can end. A person who
            // vouched for the build inside this delegate is not asked again by a delegate
            // spawned after it, and one whose run failed had the same person answer the same
            // question (DELEGATE-11): taking the record back only from a delegate that
            // reported would leave the next run asking.
            policy.adopt_from_delegate(&working.seeded, &ended.vouched);
            // Also on both of the ways a run can end, and for a reason of the same shape:
            // the person whose formatter would not start is owed the sentence whether or not
            // the delegate that fired it went on to report.
            notices.extend(ended.notices);
            // On both of the ways a run can end too, since a stopped delegate leaves the files
            // it changed and has run what it ran. Only from one that shares this turn's
            // workspace, or from one in a checkout given its changes: a delegate in a checkout
            // of its own built the commit the checkout holds, which has none of this turn's
            // changes in it (TURN-4).
            match working.checkout.as_deref() {
                None => worked.adopt(round, ended.worked),
                // A checkout given the working directory's changes was built and tested with
                // them, so a program run there counts as run against this turn's change, and
                // what it wrote there is not this turn's (CHECKOUT-23).
                Some(checkout)
                    if checkout
                        .carried()
                        .is_some_and(|carried| carried.changed_anything()) =>
                {
                    worked.adopt_run(ended.worked)
                }
                Some(_) => {}
            }
            let ran = Some((ended.took, ended.rounds));
            let work = Some((ended.worked.work, working.checkout.is_some()));
            (ended.delegated, ran, partial, requests, work)
        }
        // A thread that panicked is a delegate that stopped, which is all anybody can be
        // told about it: what it was doing died with it, and the turn is still running.
        // Nothing came back, not even a record, so there is no adoption either: "nothing
        // moved" and "nothing is known" are different things, and the trail should not
        // record the second as the first.
        Err(_) => (
            Err(TurnError::Precommit(
                "the delegate stopped without finishing".to_string(),
            )),
            None,
            Default::default(),
            Vec::new(),
            None,
        ),
    };

    spent.inference += waits.collected(crate::timing::Interval::since(joined_at), requests);

    let finish = finish_of(&delegated, ran);
    // What its own backend reported it spent, which on a failed run is the part that completed.
    let tokens = match &delegated {
        Ok(delegated) => delegated.usage.total(),
        Err(_) => partial.tokens,
    };
    policy.record_delegate_end_within(
        id,
        working.rounds,
        working.budget.map(|budget| (budget, tokens)),
        finish,
    );

    // After the decisions it made are taken back, so a rule that distrusts a path survives the
    // withdrawal of the rest (CHECKOUT-12).
    let retired = working
        .checkout
        .as_ref()
        .map(|checkout| (checkout.retire(&policy.file_authority()), checkout.clone()));
    if let Some((crate::workspace::Retired::Removed, checkout)) = &retired {
        crate::workspace::record_checkout(
            policy.sink(),
            crate::workspace::Happened::Removed,
            checkout.path(),
        );
    }

    Joined {
        id,
        limit: Limit {
            rounds: working.rounds,
            budget: working.budget,
        },
        stop: working.stop,
        delegated,
        ran,
        partial,
        finish,
        tokens,
        work,
        retired,
    }
}

#[cfg(test)]
mod tests {
    use super::{stuck_checkout, unfinished_account, work_sentences};
    use bravebot_core::delegate::{DelegateId, Finish};
    use std::path::Path;
    use std::time::Duration;

    /// TRACE-8. The person's note names the cause of each ending that handed nothing back: the
    /// fixed name of a failure, a stop, and a loss. A stop must not read as a failure, nor a loss as
    /// one the delegate stopped for, since each calls for a different response.
    #[test]
    fn the_note_for_a_delegate_that_did_not_answer_names_why_it_ended() {
        let took = Duration::from_secs(3);
        let spent = |bound| Some((took, bound));
        let note = |finish: Finish, ran, bound| {
            unfinished_account(&finish, ran, bound, DelegateId::nth(2)).0
        };
        assert_eq!(
            note(
                Finish::Failed {
                    took,
                    rounds: 4,
                    why: "unavailable"
                },
                spent(4),
                9
            ),
            "the delegate could not finish (unavailable)"
        );
        assert_eq!(
            note(Finish::Stopped { took, rounds: 4 }, spent(4), 9),
            "the delegate was stopped"
        );
        assert_eq!(note(Finish::Lost, None, 9), "the delegate could not finish");
        // A failure at the limit says so as well, after the cause rather than instead of it.
        assert_eq!(
            note(
                Finish::Failed {
                    took,
                    rounds: 9,
                    why: "transport"
                },
                spent(9),
                9
            ),
            "the delegate could not finish (transport), having spent its limit of 9 rounds"
        );
    }

    /// GROUND-2, GROUND-3. A delegate that worked in a checkout of its own is said to have, since
    /// its writes there are not in the working directory.
    #[test]
    fn a_delegate_in_a_checkout_is_said_to_have_worked_there() {
        let work = crate::outcome::Work {
            writes: 3,
            commands: 1,
            ran_after_last_write: false,
        };
        assert_eq!(
            work_sentences(work, true),
            (
                "it ran 1 command and made 3 writes in its own checkout".to_string(),
                " It ran 1 command and made 3 writes in its own checkout.".to_string()
            )
        );
        assert_eq!(
            work_sentences(work, false).1,
            " It ran 1 command and made 3 writes."
        );
    }

    /// CHECKOUT-18. A checkout that could not be removed is reported with its number, the commit it
    /// holds and where it is, which is all the planner has to go on, and the person's line says it
    /// was not removed rather than that it was.
    #[test]
    fn a_checkout_that_could_not_be_removed_is_named_with_its_place_and_commit() {
        let (said, mark) = stuck_checkout(
            "c4",
            "0123456789abcdef0123456789abcdef01234567",
            Path::new("/state/checkouts/project/c4"),
        );
        assert_eq!(
            said,
            "Its checkout c4 of commit 0123456789abcdef0123456789abcdef01234567 could not be \
             removed and is at /state/checkouts/project/c4."
        );
        assert_eq!(mark, "its checkout could not be removed");
    }
}
