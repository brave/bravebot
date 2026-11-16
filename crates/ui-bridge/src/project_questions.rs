//! The questions a session puts once its trust question is answered, about what the checkout's
//! settings files ask for: the `allow` rules a checkout proposed (PERM-15).
//!
//! What is asked and what a yes records is decided in [`bravebot_agent::opening`], so this front
//! asks what the terminal asks. Here is only what the bridge holds between putting a question and
//! being answered: the question, under a request id, and what comes after it. There is no default
//! and no dismissal: a turn is refused until every question is answered.

use bravebot_agent::granted::{Proposed, Store};
use bravebot_agent::opening::RuleQuestion;
use bravebot_config::Settings;
use serde_json::{Value, json};
use std::path::Path;

/// Where a session's opening questions stand.
#[derive(Clone, Default)]
pub(crate) enum Questions {
    /// Nothing is waiting: every question is answered, there was none, or this bridge puts none.
    #[default]
    Settled,
    /// The trust question is not answered yet. Holds the layers the session opened under, so the
    /// questions are about the files it read then (PERM-12).
    Unsettled(Box<Settings>),
    /// The question about the proposed rules is out under `request`.
    Rules {
        request: u64,
        question: RuleQuestion,
        settings: Box<Settings>,
    },
}

impl Questions {
    /// The method that answers the question that is out, which keeps the session from working
    /// until it is answered, or `None` where none is.
    pub(crate) fn reply_method(&self) -> Option<&'static str> {
        match self {
            Self::Rules { .. } => Some("rules.reply"),
            Self::Settled | Self::Unsettled(_) => None,
        }
    }

    /// The same questions under fresh request ids, for a fork that takes over what its parent had
    /// not answered: an answer meant for the parent's question is not one for the child's.
    pub(crate) fn renumbered(&self, next: &mut u64) -> Self {
        let mut copy = self.clone();
        if let Self::Rules { request, .. } = &mut copy {
            *request = take(next);
        }
        copy
    }

    /// The rules question as a window reads it, or null where none is out.
    pub(crate) fn rules_json(&self, project: &Path) -> Value {
        match self {
            Self::Rules {
                request, question, ..
            } => rules_question_json(*request, question, grant_record(project).as_ref()),
            _ => Value::Null,
        }
    }
}

/// The next request id, counted from one.
pub(crate) fn take(next: &mut u64) -> u64 {
    *next += 1;
    *next
}

/// The record of what was granted in `project`, read under the name a turn resolves it to, as the
/// terminal reads it. `None` where no state directory is known.
pub(crate) fn grant_record(project: &Path) -> Option<Store> {
    let home = bravebot_agent::home::directory()?;
    let root = project.canonicalize().ok()?;
    Some(Store::new(&home, &root))
}

/// Whether a yes to the rules is written to `store`, rather than holding for the session alone.
pub(crate) fn keeps(store: Option<&Store>) -> bool {
    store.is_some() && bravebot_agent::granted::may_be_added_to()
}

fn rule_json(rule: &Proposed) -> Value {
    // Spelled for a person (PROMPT-1): the rule as text, the file as a path. A window draws the
    // spelled ones and decides nothing from either.
    json!({
        "rule": rule.rule,
        "shownRule": bravebot_agent::confirm::shown_text(&rule.rule),
        "file": rule.path.display().to_string(),
        "shownFile": bravebot_agent::confirm::shown_path(&rule.path),
    })
}

/// The question about `question.asking`, as `rules.request` and the response that makes it due
/// carry it: the rules a yes grants, the ones held from before, and the record a yes is kept in,
/// which is null where it holds for the session alone.
fn rules_question_json(request: u64, question: &RuleQuestion, store: Option<&Store>) -> Value {
    let keeping = store.filter(|store| keeps(Some(store)));
    json!({
        "request": request,
        "rules": question.asking.iter().map(rule_json).collect::<Vec<_>>(),
        "grantedBefore": question.held.iter().map(rule_json).collect::<Vec<_>>(),
        "keeping": keeping.map(|store| store.path().display().to_string()),
        "shownKeeping": keeping.map(|store| bravebot_agent::confirm::shown_path(store.path())),
    })
}
