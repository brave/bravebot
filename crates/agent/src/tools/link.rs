//! `set_link`: the planner records the issue or pull request a session is for, where the person
//! wrote the address (SETLINK-1).

use super::{Produced, Tools, named_argument};
use crate::links::{LinkKind, typed_in};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use serde_json::Value;

/// Record a link the person typed.
///
/// The one field that decides anything is `url`, and it is accepted only when it is a whole word
/// of a line the person typed to this session, so the planner chooses nothing the person did not
/// write and a person reading the address could approve it alone (TOOL-2). The text is read
/// through the gate every planner argument goes through, then held to what a link may be
/// (PANEL-12), and only then compared with the person's lines.
///
/// Nothing is stored here. The address travels back with the turn, and whatever owns the session
/// record reads it as a link again before keeping it.
pub(super) fn set_link<S: Sink>(
    policy: &mut Policy<'_, S>,
    tools: &Tools,
    arguments: &Value,
) -> Produced {
    if tools.delegated {
        return Produced::problem("error: no such tool 'set_link'");
    }
    let kind = match arguments.get("kind").and_then(Value::as_str) {
        Some("issue") => LinkKind::Issue,
        Some("pull_request") => LinkKind::PullRequest,
        _ => return Produced::problem("error: 'kind' is \"issue\" or \"pull_request\""),
    };
    let Some(named) = named_argument(arguments, "url") else {
        return Produced::problem("error: 'url' is required, as an http or https address");
    };
    let url = match policy.read_planner_argument("set_link", "url", &named) {
        Ok(url) => url,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let Some(url) = bravebot_core::url::web_address(&url) else {
        return Produced::problem(
            "error: 'url' is not a link: it must be one http or https address with a host and \
             nothing else in it. Nothing was recorded.",
        );
    };
    if !typed_in(tools.typed, url) {
        return Produced::problem(
            "refused: that address is not in a line the user typed, so nothing was recorded. Only \
             an address the user wrote can be recorded; do not retry with another.",
        );
    }
    let (which, note) = match kind {
        LinkKind::Issue => (
            "issue",
            bravebot_i18n::t!(set_link_recorded_issue, url = url).to_string(),
        ),
        LinkKind::PullRequest => (
            "pull request",
            bravebot_i18n::t!(set_link_recorded_pull_request, url = url).to_string(),
        ),
    };
    Produced::new(
        Labelled::trusted(format!("recorded: the session's {which} link is now {url}")),
        "",
        note,
    )
    .linking(kind, url)
}
