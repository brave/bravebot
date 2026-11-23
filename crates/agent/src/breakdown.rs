//! What `/context` and `/cost` report, worked out once for every client (SHARE-001): the terminal
//! and the desktop bridge each word the result and decide nothing of their own.

use crate::request_view::{RequestView, Section};
use std::collections::BTreeMap;

/// What `/context` says about the session's context, in its order of cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextReading {
    /// No request has been measured against a known budget.
    NotMeasured,
    /// Compaction shortened the conversation and no request has been measured since.
    Compacted,
    /// The last request came to `used` tokens against `budget`, and no request built in this
    /// process is there to divide it by.
    NoRequest { used: u64, budget: u64 },
    /// The last request came to `used` tokens against `budget`, split into
    /// [`RequestView::sections`].
    Measured {
        used: u64,
        budget: u64,
        sections: Vec<(Section, u64)>,
    },
}

impl ContextReading {
    /// The reading for a session whose last count was `measured`, as `(used, budget)` with the
    /// budget the caller knows for it, or `None` when nothing has been measured; `compacted` when
    /// compaction shortened the conversation and nothing was measured since; and `last_request`
    /// the last request the session's own turn built.
    ///
    /// A count against a budget of zero is not measured: with no budget known, the reading has
    /// nothing to be measured against.
    pub fn of(
        measured: Option<(u64, u64)>,
        compacted: bool,
        last_request: Option<&RequestView>,
    ) -> Self {
        let (used, budget) = match measured {
            Some((used, budget)) if budget > 0 => (used, budget),
            _ if compacted => return Self::Compacted,
            _ => return Self::NotMeasured,
        };
        match last_request {
            None => Self::NoRequest { used, budget },
            Some(view) => Self::Measured {
                used,
                budget,
                sections: view.sections(used),
            },
        }
    }
}

/// What `/cost` says the session has spent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spend {
    /// The session total, with what the running turn has spent so far in it.
    pub total: u64,
    /// What each turn was charged, by turn number, the running turn's tokens in its row. Turn zero
    /// is what was spent before the first turn.
    pub rows: BTreeMap<usize, u64>,
    /// What the total holds that no row accounts for, such as what a session resumed from a record
    /// written before turns were charged separately spent before the resume.
    pub unattributed: u64,
}

impl Spend {
    /// The breakdown for a session whose ended turns were charged `charged` in all and `spend` by
    /// turn, whose turn number `turns` counts the one running, and whose running turn has spent
    /// `running` so far. A turn still running is charged when it ends, so asked mid-turn (CMD-8)
    /// what it has spent is added to its own row and to the total here.
    pub fn of(charged: u64, spend: &BTreeMap<usize, u64>, turns: usize, running: u64) -> Self {
        let mut rows = spend.clone();
        if running > 0 {
            *rows.entry(turns).or_default() += running;
        }
        let total = charged + running;
        let unattributed = total.saturating_sub(rows.values().sum());
        Self {
            total,
            rows,
            unattributed,
        }
    }
}

/// `part` as a whole percentage of `whole`, rounded down, and zero of a zero whole: the share a
/// `/cost` or `/context` row is shown with.
pub fn share(part: u64, whole: u64) -> u64 {
    match whole {
        0 => 0,
        whole => part.saturating_mul(100) / whole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> RequestView {
        RequestView {
            model: String::new(),
            spans: Vec::new(),
            tools: vec!["read".to_owned()],
            tools_bytes: 10,
        }
    }

    #[test]
    fn a_count_against_no_known_budget_is_not_measured_unless_compacted() {
        assert_eq!(
            ContextReading::of(Some((500, 0)), false, Some(&view())),
            ContextReading::NotMeasured
        );
        assert_eq!(
            ContextReading::of(None, false, Some(&view())),
            ContextReading::NotMeasured
        );
        assert_eq!(
            ContextReading::of(Some((500, 0)), true, Some(&view())),
            ContextReading::Compacted
        );
    }

    #[test]
    fn a_compacted_conversation_says_so_before_the_request_is_asked_for() {
        assert_eq!(
            ContextReading::of(None, true, None),
            ContextReading::Compacted
        );
    }

    #[test]
    fn a_count_with_no_request_to_divide_it_by_says_that() {
        assert_eq!(
            ContextReading::of(Some((500, 1000)), false, None),
            ContextReading::NoRequest {
                used: 500,
                budget: 1000
            }
        );
    }

    #[test]
    fn a_measured_count_is_split_into_the_requests_sections() {
        let view = view();
        assert_eq!(
            ContextReading::of(Some((500, 1000)), true, Some(&view)),
            ContextReading::Measured {
                used: 500,
                budget: 1000,
                sections: view.sections(500),
            }
        );
    }

    #[test]
    fn the_running_turn_is_added_to_its_own_row_and_to_the_total() {
        let spend = BTreeMap::from([(1, 100), (2, 50)]);
        assert_eq!(
            Spend::of(150, &spend, 3, 25),
            Spend {
                total: 175,
                rows: BTreeMap::from([(1, 100), (2, 50), (3, 25)]),
                unattributed: 0,
            }
        );
    }

    #[test]
    fn what_no_row_accounts_for_is_unattributed() {
        let spend = BTreeMap::from([(2, 40)]);
        let breakdown = Spend::of(100, &spend, 2, 0);
        assert_eq!(breakdown.total, 100);
        assert_eq!(breakdown.rows, spend);
        assert_eq!(breakdown.unattributed, 60);
    }

    #[test]
    fn a_share_rounds_down_and_is_zero_of_nothing() {
        assert_eq!(share(1, 3), 33);
        assert_eq!(share(5, 0), 0);
    }
}
