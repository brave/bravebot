//! The questions a session puts once its trust question is answered, about what the checkout's
//! settings files ask for: the `allow` rules a checkout proposed (PERM-15), then each directory
//! named in `additionalDirectories` (TRUST-9).
//!
//! What is asked and what a yes records is decided in [`bravebot_agent::opening`], so this front
//! asks what the terminal asks. Here is only what the bridge holds between putting a question and
//! being answered: the question, under a request id, and what comes after it. There is no default
//! and no dismissal: a turn is refused until every question is answered.

use bravebot_agent::Workspace;
use bravebot_agent::granted::{Proposed, Store};
use bravebot_agent::opening::RuleQuestion;
use bravebot_config::Settings;
use serde_json::{Value, json};
use std::collections::VecDeque;
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
    /// The question about the proposed rules is out under `request`, and the named directories
    /// are asked about after it.
    Rules {
        request: u64,
        question: RuleQuestion,
        settings: Box<Settings>,
        folders: VecDeque<Named>,
    },
    /// The question about one named directory is out under `request`, and `rest` come after it.
    Folder {
        request: u64,
        folder: Named,
        rest: VecDeque<Named>,
        settings: Box<Settings>,
    },
}

/// A directory the settings files named, as a yes would open it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Named {
    /// Where it resolves to, canonical, which is what the person is asked about: a name is
    /// canonicalized as it is opened, so `shared` pointing elsewhere is asked about as where it
    /// points.
    pub directory: String,
    /// Each name in the files that resolves there, as written.
    pub names: Vec<String>,
}

/// A name in the files that cannot be opened at all, which is said rather than asked about.
#[derive(Clone, Debug)]
pub(crate) struct Refused {
    pub name: String,
    pub problem: String,
}

impl Questions {
    /// The method that answers the question that is out, which keeps the session from working
    /// until it is answered, or `None` where none is.
    pub(crate) fn reply_method(&self) -> Option<&'static str> {
        match self {
            Self::Rules { .. } => Some("rules.reply"),
            Self::Folder { .. } => Some("folder.reply"),
            Self::Settled | Self::Unsettled(_) => None,
        }
    }

    /// The same questions under fresh request ids, for a fork that takes over what its parent had
    /// not answered: an answer meant for the parent's question is not one for the child's.
    pub(crate) fn renumbered(&self, next: &mut u64) -> Self {
        let mut copy = self.clone();
        if let Self::Rules { request, .. } | Self::Folder { request, .. } = &mut copy {
            *request = take(next);
        }
        copy
    }

    /// The question about the next of `folders`, or settled where none is left.
    pub(crate) fn next_folder(
        mut folders: VecDeque<Named>,
        settings: Box<Settings>,
        next: &mut u64,
    ) -> Self {
        match folders.pop_front() {
            Some(folder) => Self::Folder {
                request: take(next),
                folder,
                rest: folders,
                settings,
            },
            None => Self::Settled,
        }
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

    /// The folder question as a window reads it, or null where none is out.
    pub(crate) fn folder_json(&self) -> Value {
        match self {
            Self::Folder {
                request,
                folder,
                rest,
                ..
            } => json!({
                "request": request,
                "directory": folder.directory,
                "shownDirectory": bravebot_agent::confirm::shown_path(Path::new(&folder.directory)),
                "named": folder.names,
                "shownNamed": folder.names.iter()
                    .map(|name| bravebot_agent::confirm::shown_text(name))
                    .collect::<Vec<_>>(),
                "remaining": rest.len(),
            }),
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

/// Resolve the directories `names` would open against `workspace`, before anything is asked, so
/// what a person is shown is what they would be granting. A path two names reach is one question,
/// and a name that cannot be opened is refused with why.
pub(crate) fn named_folders(
    workspace: &Workspace,
    names: &[String],
) -> (VecDeque<Named>, Vec<Refused>) {
    let home = bravebot_agent::home::profile();
    let mut folders: VecDeque<Named> = VecDeque::new();
    let mut refused = Vec::new();
    for name in names {
        let resolved = bravebot_agent::opening::named_to_open(
            workspace,
            std::slice::from_ref(name),
            home.as_deref(),
        );
        for (name, problem) in resolved.refused {
            refused.push(Refused {
                name,
                problem: problem.to_string(),
            });
        }
        for directory in resolved.paths {
            match folders.iter_mut().find(|held| held.directory == directory) {
                Some(held) => held.names.push(name.clone()),
                None => folders.push_back(Named {
                    directory,
                    names: vec![name.clone()],
                }),
            }
        }
    }
    (folders, refused)
}

/// The names refused, as a window reads them.
pub(crate) fn refused_json(refused: &[Refused]) -> Value {
    refused
        .iter()
        .map(|refused| {
            json!({
                "directory": refused.name,
                "shownDirectory": bravebot_agent::confirm::shown_text(&refused.name),
                "problem": refused.problem,
                "shownProblem": bravebot_agent::confirm::shown_text(&refused.problem),
            })
        })
        .collect()
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
