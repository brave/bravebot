//! The most a session may spend before it asks (TURN-8).
//!
//! A count of tokens the backends reported, compared with a number the person set. Nothing here
//! reads a reply or a tool result, so the stop depends on no content.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bravebot_core::ask::{Answer, Asking, Choice, Question, Series};
use bravebot_core::event::Sink;
use bravebot_core::policy::{AfterSpendLimit, Policy};
use bravebot_i18n::t;

use crate::confirm::Confirmer;
use crate::report::Reporter;

/// How many times a figure that is not a usable limit is asked about again before the turn stops.
const MOST_ASKS: usize = 3;

/// A session's limit, shared between the session that sets it and the turn that reads it.
///
/// Shared rather than copied into each turn so that `/limit` typed while a turn runs reaches that
/// turn's next request, and so that the figure a person gives at the question is the session's
/// afterwards.
#[derive(Debug, Clone, Default)]
pub struct SpendLimit(Arc<AtomicU64>);

impl SpendLimit {
    pub fn new(tokens: Option<u64>) -> Self {
        let limit = Self::default();
        limit.set(tokens);
        limit
    }

    /// The limit in tokens, or `None` for no limit.
    pub fn tokens(&self) -> Option<u64> {
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            tokens => Some(tokens),
        }
    }

    /// Set the limit. Zero is no limit: a ceiling of nothing would stop the first request.
    pub fn set(&self, tokens: Option<u64>) {
        self.0.store(tokens.unwrap_or(0), Ordering::Relaxed);
    }
}

/// What the turn does after the limit was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// Under the limit, or the person raised or lifted it.
    Go,
    /// The person chose to stop, or could not be asked.
    Stop,
}

/// The row that ends the turn. First, so a bare Enter on the question is the safe answer.
const STOP: usize = 0;
/// The row that goes on with no limit.
const WITHOUT_A_LIMIT: usize = 1;

/// The question, built from counts and fixed words.
///
/// Its key is new each time. An interface remembers the answer to a question by its key, and an
/// answer to this one is about the moment it was asked: the same figures can come back after a
/// stop, and drawing nothing then would stop the turn on an answer nobody gave.
fn asking(spent: u64, limit: u64, rejected: Option<&str>) -> Asking {
    static ASKED: AtomicU64 = AtomicU64::new(0);
    let mut prompt = t!(spend_limit_reached, spent = spent, limit = limit).to_string();
    if let Some(text) = rejected {
        prompt.push(' ');
        prompt.push_str(&t!(spend_limit_not_a_limit, text = text, spent = spent));
    }
    prompt.push(' ');
    prompt.push_str(t!(spend_limit_how_to_raise));
    let mut asking = bravebot_core::ask::asking(&Series::new(vec![Question::new(
        t!(spend_limit_header),
        prompt,
        vec![
            Choice::new(t!(spend_limit_stop), None),
            Choice::new(t!(spend_limit_without_a_limit), None),
        ],
        false,
    )]));
    for prompt in &mut asking.prompts {
        prompt.key = format!("spend limit {}", ASKED.fetch_add(1, Ordering::Relaxed));
    }
    asking
}

/// Let the turn go on while it is under its limit, and put the question once it is not.
///
/// `spent` is the session's total including this turn so far. The person is asked whether to stop,
/// to go on without a limit, or to go on under the figure they type; the answer is recorded in the
/// trail and, when it changes the limit, written back to `limit`. No permission rule and no mode
/// answers it, and an interface that cannot ask stops the turn.
pub(crate) fn hold<S, C, R>(
    limit: &SpendLimit,
    spent: u64,
    round: usize,
    policy: &mut Policy<'_, S>,
    confirmer: &mut C,
    reporter: &mut R,
) -> Held
where
    S: Sink,
    C: Confirmer + ?Sized,
    R: Reporter + ?Sized,
{
    let Some(ceiling) = limit.tokens() else {
        return Held::Go;
    };
    if spent < ceiling {
        return Held::Go;
    }
    let mut rejected: Option<String> = None;
    for _ in 0..MOST_ASKS {
        let answers = confirmer.ask_user(&asking(spent, ceiling, rejected.as_deref()));
        let then = match answers.as_slice() {
            [Answer::Chosen(rows)] if rows.as_slice() == [WITHOUT_A_LIMIT] => {
                AfterSpendLimit::Lifted
            }
            [Answer::Chosen(rows)] if rows.as_slice() == [STOP] => AfterSpendLimit::Stopped,
            [Answer::Typed(text)] => {
                match bravebot_config::limit::parse_tokens(text).filter(|raised| *raised > spent) {
                    Some(raised) => AfterSpendLimit::Raised(raised),
                    None => {
                        rejected = Some(text.clone());
                        continue;
                    }
                }
            }
            _ => AfterSpendLimit::Unanswered,
        };
        policy.record_spend_limit(round, spent, ceiling, then);
        return match then {
            AfterSpendLimit::Raised(raised) => {
                limit.set(Some(raised));
                reporter.notice(t!(spend_limit_raised, limit = raised));
                Held::Go
            }
            AfterSpendLimit::Lifted => {
                limit.set(None);
                reporter.notice(t!(spend_limit_lifted).to_string());
                Held::Go
            }
            AfterSpendLimit::Stopped | AfterSpendLimit::Unanswered => {
                reporter.notice(t!(spend_limit_stopped, spent = spent, limit = ceiling));
                Held::Stop
            }
        };
    }
    policy.record_spend_limit(round, spent, ceiling, AfterSpendLimit::Unanswered);
    reporter.notice(t!(spend_limit_stopped, spent = spent, limit = ceiling));
    Held::Stop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_limit_is_none_until_one_is_set_and_zero_clears_it() {
        let limit = SpendLimit::default();
        assert_eq!(limit.tokens(), None);
        limit.set(Some(500));
        assert_eq!(limit.tokens(), Some(500));
        limit.set(None);
        assert_eq!(limit.tokens(), None);
        assert_eq!(SpendLimit::new(Some(7)).tokens(), Some(7));
    }

    #[test]
    fn a_clone_shares_the_figure_with_the_session_that_set_it() {
        let session = SpendLimit::new(Some(100));
        let turn = session.clone();
        turn.set(Some(900));
        assert_eq!(session.tokens(), Some(900));
    }

    #[test]
    fn every_question_has_a_key_of_its_own() {
        let first = asking(10, 10, None);
        let second = asking(10, 10, None);
        assert_ne!(first.prompts[0].key, second.prompts[0].key);
    }

    #[test]
    fn the_first_row_is_the_one_that_stops() {
        let asked = asking(10, 10, None);
        assert_eq!(asked.prompts[0].rows[STOP].label, t!(spend_limit_stop));
        assert_eq!(
            asked.prompts[0].rows[WITHOUT_A_LIMIT].label,
            t!(spend_limit_without_a_limit)
        );
    }
}
