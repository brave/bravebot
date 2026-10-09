//! The most a session may spend before it asks (TURN-8).
//!
//! A count of tokens the backends reported, or of Leo Premium credentials spent, compared with a
//! number the person set. Nothing here reads a reply or a tool result, so the stop depends on no
//! content.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bravebot_config::limit::{Limit, Unit};

use bravebot_core::ask::{Answer, Asking, Choice, Question, Series};
use bravebot_core::event::Sink;
use bravebot_core::policy::{AfterSpendLimit, Policy};
use bravebot_i18n::t;

use crate::confirm::Confirmer;
use crate::report::Reporter;

/// How many times a figure that is not a usable limit is asked about again before the turn stops.
const MOST_ASKS: usize = 3;

/// A session's limit and the credentials it has spent, shared between the session that sets the
/// one and the turns that add to the other.
///
/// Shared rather than copied into each turn so that `/limit` typed while a turn runs reaches that
/// turn's next request, and so that the figure a person gives at the question is the session's
/// afterwards.
#[derive(Debug, Clone, Default)]
pub struct SpendLimit(Arc<Shared>);

#[derive(Debug, Default)]
struct Shared {
    limit: Mutex<Option<Limit>>,
    credits: AtomicU64,
}

impl SpendLimit {
    pub fn new(limit: Option<Limit>) -> Self {
        let handle = Self::default();
        handle.set(limit);
        handle
    }

    /// The limit, or `None` for no limit.
    pub fn limit(&self) -> Option<Limit> {
        *self.0.limit.lock().unwrap_or_else(|held| held.into_inner())
    }

    /// Set the limit. A figure of zero is no limit: a ceiling of nothing would stop the first
    /// request.
    pub fn set(&self, limit: Option<Limit>) {
        *self.0.limit.lock().unwrap_or_else(|held| held.into_inner()) =
            limit.filter(|limit| limit.figure > 0);
    }

    /// Credentials the session's turns have spent, delegates included.
    pub fn credits(&self) -> u64 {
        self.0.credits.load(Ordering::Relaxed)
    }

    /// Count one credential spent.
    pub(crate) fn add_credit(&self) {
        self.0.credits.fetch_add(1, Ordering::Relaxed);
    }

    /// Start counting credentials again, for a session that begins afresh.
    pub fn forget_credits(&self) {
        self.0.credits.store(0, Ordering::Relaxed);
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
fn asking(spent: u64, limit: Limit, rejected: Option<&str>) -> Asking {
    static ASKED: AtomicU64 = AtomicU64::new(0);
    let unit = noun(limit.unit);
    let mut prompt = t!(
        spend_limit_reached,
        spent = spent,
        limit = limit.figure,
        unit = unit
    )
    .to_string();
    if let Some(text) = rejected {
        prompt.push(' ');
        prompt.push_str(&t!(
            spend_limit_not_a_limit,
            text = text,
            spent = spent,
            unit = unit
        ));
    }
    prompt.push(' ');
    prompt.push_str(match limit.unit {
        Unit::Tokens => t!(spend_limit_how_to_raise),
        Unit::Credits => t!(spend_limit_how_to_raise_credits),
    });
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

/// The noun a limit's sentences count in.
pub fn noun(unit: Unit) -> &'static str {
    match unit {
        Unit::Tokens => t!(limit_unit_tokens),
        Unit::Credits => t!(limit_unit_credits),
    }
}

/// What the session has spent, in the unit a limit counts.
fn spent_in(unit: Unit, limit: &SpendLimit, tokens: u64) -> u64 {
    match unit {
        Unit::Tokens => tokens,
        Unit::Credits => limit.credits(),
    }
}

/// Let the turn go on while it is under its limit, and put the question once it is not.
///
/// `tokens` is the session's total including this turn so far, and the credentials spent are read
/// from `limit`, whichever the limit counts. The person is asked whether to stop, to go on without
/// a limit, or to go on under the figure they type; the answer is recorded in the trail and, when
/// it changes the limit, written back to `limit`. No permission rule and no mode answers it, and
/// an interface that cannot ask stops the turn.
pub(crate) fn hold<S, C, R>(
    limit: &SpendLimit,
    tokens: u64,
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
    let Some(ceiling) = limit.limit() else {
        return Held::Go;
    };
    let spent = spent_in(ceiling.unit, limit, tokens);
    if spent < ceiling.figure {
        return Held::Go;
    }
    let unit = noun(ceiling.unit);
    let mut rejected: Option<String> = None;
    for _ in 0..MOST_ASKS {
        let answers = confirmer.ask_user(&asking(spent, ceiling, rejected.as_deref()));
        let then = match answers.as_slice() {
            [Answer::Chosen(rows)] if rows.as_slice() == [WITHOUT_A_LIMIT] => {
                AfterSpendLimit::Lifted
            }
            [Answer::Chosen(rows)] if rows.as_slice() == [STOP] => AfterSpendLimit::Stopped,
            [Answer::Typed(text)] => {
                match bravebot_config::limit::parse(text, ceiling.unit)
                    .filter(|raised| raised.unit == ceiling.unit && raised.figure > spent)
                {
                    Some(raised) => AfterSpendLimit::Raised(raised.figure),
                    None => {
                        rejected = Some(text.clone());
                        continue;
                    }
                }
            }
            _ => AfterSpendLimit::Unanswered,
        };
        policy.record_spend_limit(round, spent, ceiling.figure, ceiling.unit.word(), then);
        return match then {
            AfterSpendLimit::Raised(figure) => {
                limit.set(Some(Limit {
                    unit: ceiling.unit,
                    figure,
                }));
                reporter.notice(t!(spend_limit_raised, limit = figure, unit = unit));
                Held::Go
            }
            AfterSpendLimit::Lifted => {
                limit.set(None);
                reporter.notice(t!(spend_limit_lifted).to_string());
                Held::Go
            }
            AfterSpendLimit::Stopped | AfterSpendLimit::Unanswered => {
                reporter.notice(t!(
                    spend_limit_stopped,
                    spent = spent,
                    limit = ceiling.figure,
                    unit = unit
                ));
                Held::Stop
            }
        };
    }
    policy.record_spend_limit(
        round,
        spent,
        ceiling.figure,
        ceiling.unit.word(),
        AfterSpendLimit::Unanswered,
    );
    reporter.notice(t!(
        spend_limit_stopped,
        spent = spent,
        limit = ceiling.figure,
        unit = unit
    ));
    Held::Stop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_limit_is_none_until_one_is_set_and_zero_clears_it() {
        let limit = SpendLimit::default();
        assert_eq!(limit.limit(), None);
        limit.set(Some(Limit::tokens(500)));
        assert_eq!(limit.limit(), Some(Limit::tokens(500)));
        limit.set(Some(Limit::credits(0)));
        assert_eq!(limit.limit(), None);
        limit.set(Some(Limit::credits(3)));
        limit.set(None);
        assert_eq!(limit.limit(), None);
        assert_eq!(
            SpendLimit::new(Some(Limit::credits(7))).limit(),
            Some(Limit::credits(7))
        );
    }

    #[test]
    fn a_clone_shares_the_figure_and_the_credits_with_the_session_that_set_it() {
        let session = SpendLimit::new(Some(Limit::tokens(100)));
        let turn = session.clone();
        turn.set(Some(Limit::credits(9)));
        turn.add_credit();
        turn.add_credit();
        assert_eq!(session.limit(), Some(Limit::credits(9)));
        assert_eq!(session.credits(), 2);
        session.forget_credits();
        assert_eq!(turn.credits(), 0);
    }

    #[test]
    fn every_question_has_a_key_of_its_own() {
        let first = asking(10, Limit::tokens(10), None);
        let second = asking(10, Limit::tokens(10), None);
        assert_ne!(first.prompts[0].key, second.prompts[0].key);
    }

    #[test]
    fn the_first_row_is_the_one_that_stops() {
        let asked = asking(10, Limit::tokens(10), None);
        assert_eq!(asked.prompts[0].rows[STOP].label, t!(spend_limit_stop));
        assert_eq!(
            asked.prompts[0].rows[WITHOUT_A_LIMIT].label,
            t!(spend_limit_without_a_limit)
        );
    }

    #[test]
    fn the_question_names_what_the_limit_counts() {
        let tokens = asking(10, Limit::tokens(10), None).prompts[0]
            .question
            .clone();
        let credits = asking(10, Limit::credits(10), None).prompts[0]
            .question
            .clone();
        assert!(
            tokens.contains("10 tokens") && tokens.contains("such as 2m"),
            "{tokens}"
        );
        assert!(
            credits.contains("10 credits") && credits.contains("such as 200"),
            "{credits}"
        );
        assert!(!credits.contains("tokens"), "{credits}");
    }
}
