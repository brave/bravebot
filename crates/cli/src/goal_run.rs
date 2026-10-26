//! A one-shot run that works towards a condition (CLI-35).
//!
//! `--goal` is `/goal` for a run nobody is watching: the turn runs, a judge reads the exchange and
//! says whether the condition holds, and the work goes back until it does. The condition, the
//! judge, the verdicts and the budget of rounds are the ones the interface uses
//! ([`bravebot_agent::goal`], [`bravebot_tui::goals`]), so this module is the loop around them and
//! nothing more. Nothing here reads a verdict: [`bravebot_agent::goal::assess`] has passed it
//! through the gate that decides whether the driver may hold it, and what is matched below is the
//! variant that came out.

use crate::exit::Ending;
use crate::json;
use crate::progress::{self, Progress};
use bravebot_agent::goal::{self, GoalError, Verdict};
use bravebot_agent::turn::{self, CompletedTurn, Outcome, Task};
use bravebot_agent::{Confirmer, Conversation, Spent, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::programs::TrustedPrograms;
use bravebot_i18n::t;
use bravebot_net::Egress;
use bravebot_tui::goals::{Next, Running};
use std::io::Write;

/// Why a run that worked towards a condition stopped with it unmet.
#[derive(Debug)]
pub(crate) enum Unmet {
    /// The rounds were spent. `last` is what the judge said the last time it was asked.
    Spent { rounds: usize, last: String },
    /// Rounds in a row made no tool call, so the work is not sent back again.
    Stalled { last: String },
    /// The judge said no amount of further work would satisfy the condition.
    Impossible { reason: String },
    /// The judge's answer did not begin with a verdict.
    Unreadable,
    /// The exchange had met untrusted content, so the driver may not read the verdict.
    Quarantined,
    /// The check could not be made.
    Unchecked { ending: Ending, problem: String },
}

impl Unmet {
    /// The ending of a run that stopped here. A check that could not be made ends as any other
    /// request that failed does, since what a caller can do about it is what it can do about that.
    pub(crate) fn ending(&self) -> Ending {
        match self {
            Self::Unchecked { ending, .. } => *ending,
            _ => Ending::Unmet,
        }
    }

    /// What the run says about stopping, which is also the result object's `message`.
    pub(crate) fn said(&self) -> String {
        let last = |reason: &str| match reason.is_empty() {
            true => String::new(),
            false => format!("\n{}", t!(goal_last_check, reason = drawn(reason))),
        };
        match self {
            Self::Spent {
                rounds,
                last: reason,
            } => {
                format!("{}{}", t!(goal_spent, rounds = *rounds), last(reason))
            }
            Self::Stalled { last: reason } => format!("{}{}", t!(cli_goal_stalled), last(reason)),
            Self::Impossible { reason } => {
                t!(cli_goal_impossible, reason = drawn(reason))
            }
            Self::Unreadable => t!(cli_goal_unreadable).to_string(),
            Self::Quarantined => t!(cli_goal_quarantined).to_string(),
            Self::Unchecked { problem, .. } => {
                t!(cli_goal_failed, problem = drawn(problem))
            }
        }
    }
}

/// One line on stderr, dropped where nobody is reading.
///
/// Progress is written the same way. A closed stderr is a caller that stopped listening, and a
/// panic over it would end a run that has work on disk and nothing recorded of it.
fn say(line: impl std::fmt::Display) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// What a judge wrote, as a terminal may be shown it.
///
/// The reason is a model's sentence, so a control character in it is pictured rather than sent
/// to the screen, and a line break is kept as one.
fn drawn(text: &str) -> String {
    text.lines()
        .map(progress::printable)
        .collect::<Vec<_>>()
        .join("\n  ")
}

/// What the rounds before the last one add to the figures a run reports, beyond what the reporter
/// keeps because a stream and a failed run need it as the run goes: the tokens and the time.
#[derive(Default)]
pub(crate) struct Earlier {
    steps: usize,
    /// Whether a gate refused something in any of them.
    refused: bool,
    notices: Vec<String>,
}

impl Earlier {
    fn add(&mut self, round: &Outcome) {
        self.steps += round.steps;
        self.refused |= !round.clean;
        self.notices
            .extend(new_notices(&self.notices, &round.notices));
    }

    /// Make `last`, the final round's outcome, the account of the whole run: its rounds of tool
    /// calls, whether a gate refused anything, what it cost, how long it took and what it had to
    /// say, with the earlier rounds'.
    ///
    /// `before` is what the reporter counted ahead of that round, the judge's checks included.
    pub(crate) fn fold_into(self, last: &mut Outcome, before: Spent) {
        last.steps += self.steps;
        // A run something was refused in did not do what it was asked, whichever round it was.
        last.clean = last.clean && !self.refused;
        last.tokens += before.tokens;
        last.output_tokens += before.output_tokens;
        last.cached.add(before.cached);
        last.timing.add(before.timing);
        let said = std::mem::take(&mut last.notices);
        let mut notices = new_notices(&self.notices, &said);
        notices.splice(0..0, self.notices);
        last.notices = notices;
    }
}

/// What a round said that no earlier round did. A round's own repeats stay: one sentence said twice
/// in a round is that round's to say, and [`Progress`] keeps the same.
fn new_notices(earlier: &[String], round: &[String]) -> Vec<String> {
    round
        .iter()
        .filter(|notice| !earlier.contains(notice))
        .cloned()
        .collect()
}

/// What the rounds need that is the run's rather than a round's.
pub(crate) struct Towards<'a> {
    pub(crate) condition: &'a str,
    pub(crate) config: &'a Config,
    pub(crate) egress: &'a Egress,
    pub(crate) workspace: &'a Workspace,
    /// The first round's task, which the later ones are made from.
    pub(crate) task: &'a Task,
    /// The model the rounds ask for, which the judge asks too unless `summaryModel` names another
    /// (GOAL-19).
    pub(crate) model: &'a str,
    /// The token a stop reaches the run by. Every round and every check is stoppable by it, so a
    /// stop sent to a run that reads lines (CLI-36) ends the run and not only its first round.
    pub(crate) cancel: &'a Cancel,
}

/// How a run that worked towards a condition ended.
pub(crate) struct Ended {
    /// The last round, which is the run's reply.
    pub(crate) completed: CompletedTurn,
    /// Why the condition was not met, and `None` where it was or where a round never answered.
    pub(crate) unmet: Option<Unmet>,
    pub(crate) earlier: Earlier,
}

/// The first round's task without what came with the person's prompt, which the rounds that send
/// the work back are made from.
///
/// The files, excerpts, pictures and piped input belong to the line the person typed. Attached to
/// the driver's sentence they would be read again every round, and the piped bytes (CLI-3) would be
/// presented again as well. Made once, so a large attachment is not copied to be dropped each round.
fn without_the_prompts_extras(task: &Task) -> Task {
    let mut bare = task.clone();
    bare.composed = None;
    bare.files.clear();
    bare.attachments.clear();
    bare.dropped_text.clear();
    bare.session_excerpts.clear();
    bare.images.clear();
    bare.piped = None;
    bare.written_by_the_driver()
}

/// Which ending a check that could not be made has: a backend that was not there is told apart, as
/// it is for a turn (CLI-6).
fn ending_of_a_check(failure: &GoalError) -> Ending {
    match failure {
        GoalError::Chat(backend) => crate::exit::ending_of_a_request(backend),
        _ => Ending::Failed,
    }
}

/// Carry a run on from its first round until the judge says the condition holds, or it cannot.
///
/// `first` is the round that answered the person's prompt. Each round after it is the same turn
/// over the same conversation, sent the driver's own sentence ([`goal::carry_on`]), with the trust
/// the last one ended with. The reporter keeps counting across them.
pub(crate) fn work_towards<C: Confirmer + Send>(
    towards: &Towards<'_>,
    first: CompletedTurn,
    conversation: &mut Conversation,
    confirmer: &mut C,
    reporter: &mut Progress<std::io::Stderr>,
    sink: &mut json::Streaming,
) -> Ended {
    let mut running = Running::begin(towards.condition.to_string(), 0);
    let bare = without_the_prompts_extras(towards.task);
    let mut earlier = Earlier::default();
    let mut completed = first;
    // No call has been made before the first round, and a count of calls is what GOAL-20 stops on.
    let mut calls_before = 0;

    loop {
        let stop = |completed: CompletedTurn, unmet: Option<Unmet>, earlier: Earlier| Ended {
            completed,
            unmet,
            earlier,
        };
        // A round that failed has no answer to judge, and ends the run as it would end a turn.
        if completed.outcome.is_err() {
            return stop(completed, None, earlier);
        }
        let made = reporter.calls().len() - calls_before;

        // The check goes out under the trust the round ended with, as it does in a session, and
        // reports to nobody: what a turn says about its subscription it has said.
        let check = goal::Check::of(conversation, towards.condition);
        let judged = turn::goal(
            towards.config,
            towards.egress,
            towards.task.account_session(),
            check,
            Some(towards.model),
            &mut Progress::new(std::io::sink()),
            sink,
            completed.decisions.trust.clone(),
            towards.cancel,
        );
        let assessed = match judged {
            Ok(assessed) => assessed,
            Err(failure) => {
                // What the attempts that completed cost is spent whether or not the answer was
                // usable.
                if let GoalError::Chat(backend) = &failure
                    && let Some(usage) = backend.completed_usage()
                {
                    let mut cost = Spent::default();
                    cost.add_usage(usage);
                    reporter.charge(cost);
                }
                let unmet = Unmet::Unchecked {
                    ending: ending_of_a_check(&failure),
                    problem: failure.to_string(),
                };
                return stop(completed, Some(unmet), earlier);
            }
        };
        let mut cost = Spent::default();
        cost.add_usage(assessed.usage);
        reporter.charge(cost);

        let reason = match assessed.verdict {
            Verdict::Met { reason } => {
                match reason.is_empty() {
                    true => say(t!(goal_met_unsaid)),
                    false => say(t!(goal_met, reason = drawn(&reason))),
                }
                return stop(completed, None, earlier);
            }
            Verdict::NotMet { reason } => reason,
            Verdict::Impossible { reason } => {
                return stop(completed, Some(Unmet::Impossible { reason }), earlier);
            }
            Verdict::Unreadable => return stop(completed, Some(Unmet::Unreadable), earlier),
            Verdict::Quarantined => return stop(completed, Some(Unmet::Quarantined), earlier),
        };

        match running.not_met(reason.clone(), made) {
            Next::SendBack => {}
            Next::Spent => {
                let unmet = Unmet::Spent {
                    rounds: running.rounds(),
                    last: reason,
                };
                return stop(completed, Some(unmet), earlier);
            }
            Next::Stalled => {
                return stop(completed, Some(Unmet::Stalled { last: reason }), earlier);
            }
        }
        match reason.is_empty() {
            true => say(t!(goal_not_met_unsaid)),
            false => say(t!(goal_not_met, reason = drawn(&reason))),
        }

        if let Ok(round) = &completed.outcome {
            earlier.add(round);
        }
        // What the last round decided comes with the next one, as it does between the turns of a
        // session: the trust it ended with, the programs vouched for, the advice already given
        // and the files already agreed to be shown.
        let mut next = bare.clone();
        next.prompt = goal::carry_on(towards.condition, &reason);
        next.asked_about = completed.decisions.asked_about.clone();
        next.exposed = completed.decisions.exposed.clone();
        let (trust, programs): (_, TrustedPrograms) = (
            completed.decisions.trust.clone(),
            completed.decisions.programs.clone(),
        );
        reporter.next_round();
        calls_before = reporter.calls().len();
        completed = turn::resume(
            towards.config,
            towards.egress,
            towards.workspace,
            &next,
            conversation,
            confirmer,
            reporter,
            sink,
            trust,
            programs,
            None,
            towards.cancel,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What came with the person's line belongs to the line: the next round is the driver's
    /// sentence, over a conversation that already holds all of it.
    #[test]
    fn the_work_sent_back_keeps_the_run_and_drops_what_came_with_the_prompt() {
        let first = Task::new("get the build green")
            .with_file("notes.txt")
            .with_session_excerpt("abc", "an earlier question")
            .with_piped_input("a build log")
            .working_towards(Some("the build is green".to_string()));

        let next = without_the_prompts_extras(&first);

        assert!(next.files.is_empty(), "{:?}", next.files);
        assert!(next.session_excerpts.is_empty());
        assert_eq!(next.piped, None, "the piped bytes were presented again");
        assert!(
            next.driver_wrote_the_prompt,
            "the driver's sentence was passed off as the person's"
        );
        assert_eq!(
            next.working_towards.as_deref(),
            Some("the build is green"),
            "a later round is not told what it is judged against"
        );
        assert_eq!(
            first.prompt, "get the build green",
            "the first round's task changed"
        );
        assert_eq!(first.files, ["notes.txt"]);
    }

    /// CHATGPT-10: the rounds of a goal run are made from the first round's task, and they are one
    /// session to the ChatGPT account with it, whether the first request has been made when a round
    /// is made or not. The regression it rejects is a round made from the first one's fields in a
    /// task of its own, which would begin a session of its own and make each round a cache the next
    /// never hits.
    #[test]
    fn the_rounds_of_a_goal_run_are_one_session_to_the_account() {
        let first = Task::new("get the build green");

        let next = without_the_prompts_extras(&first);

        assert_eq!(
            next.account_session().identifier(),
            first.account_session().identifier()
        );
        assert_eq!(
            without_the_prompts_extras(&first)
                .account_session()
                .identifier(),
            first.account_session().identifier()
        );
    }

    /// An unmet condition has a status of its own, and a check that could not be made has the one
    /// any other failed request has, since what a caller does about it is the same.
    #[test]
    fn an_unmet_condition_and_an_unmade_check_end_on_different_statuses() {
        let spent = Unmet::Spent {
            rounds: 10,
            last: String::new(),
        };
        assert_eq!(spent.ending(), Ending::Unmet);
        assert_eq!(Unmet::Unreadable.ending(), Ending::Unmet);
        assert_eq!(Unmet::Quarantined.ending(), Ending::Unmet);
        let unmade = Unmet::Unchecked {
            ending: Ending::Unreachable,
            problem: "no route".to_string(),
        };
        assert_eq!(unmade.ending(), Ending::Unreachable);
    }

    /// A judge's reason is a model's sentence, and it goes to a terminal.
    #[test]
    fn what_the_judge_said_cannot_move_the_cursor() {
        let said = Unmet::Impossible {
            reason: "no\x1b[2Jbuild".to_string(),
        }
        .said();

        assert!(!said.contains('\x1b'), "{said:?}");
        assert!(said.contains("no\u{241b}[2Jbuild"), "{said:?}");
    }
}
