//! `web_search`: the Brave Search API as one tool (`docs/specs/tools/web-search.md`).
//!
//! The request is built by [`crate::web_search::prepare`] from the planner's own words, sent through
//! the egress gate with the key in one header, and the body that comes back is carried to a
//! reference exactly as a fetched page is. Nothing here reads a byte of the reply or of a vendor
//! error: a failure is worded from the kind of failure alone.

use super::{Produced, Tools, argument};
use crate::confirm::{Confirmer, Decision};
use crate::web_search::{self, Prepared};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use serde_json::{Map, Value};

/// The host every search goes to, which is written in the configuration crate and not by any call.
fn host_of_base(base: &str) -> String {
    bravebot_core::url::host_of(base).unwrap_or_default()
}

/// The planner's own arguments, each string read through the gate for the planner's own words.
///
/// `why` and `read` are the two arguments every tool shares: neither reaches the request. A value
/// that is not a string is a JSON literal and names nothing. The gate is asked under a field name
/// of this file's own, so a refusal repeats nothing the planner typed.
fn read_arguments<S: Sink>(
    policy: &mut Policy<'_, S>,
    arguments: &Value,
) -> Result<Map<String, Value>, Box<Produced>> {
    let Some(object) = arguments.as_object() else {
        return Err(Box::new(Produced::problem(
            "error: web_search takes an object",
        )));
    };
    let mut read = Map::new();
    for (name, value) in object {
        if name == "why" || name == "read" || value.is_null() {
            continue;
        }
        let value = match value {
            Value::String(_) => {
                let Some(labelled) = argument(arguments, name) else {
                    continue;
                };
                match policy.read_planner_argument("web_search", "arguments", &labelled) {
                    Ok(text) => Value::String(text),
                    Err(denial) => {
                        return Err(Box::new(Produced::problem(format!("refused: {denial}"))));
                    }
                }
            }
            other => other.clone(),
        };
        read.insert(name.clone(), value);
    }
    Ok(read)
}

pub(super) fn web_search<S: Sink, C: Confirmer>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    arguments: &Value,
) -> Produced {
    let read = match read_arguments(policy, arguments) {
        Ok(read) => read,
        Err(refusal) => return *refusal,
    };
    let base = tools.chat.config.web_search_base().to_string();
    let Prepared { service, url } = match web_search::prepare(&read, &base) {
        Ok(prepared) => prepared,
        Err(refusal) => return Produced::problem(refusal.words()),
    };
    let host = host_of_base(&base);

    // A key that cannot be used is said before anybody is asked about a search that could not be
    // made. The words name the settings and never a variable, a path, the content or the reason the
    // source was refused: a variable's name is a string the person wrote, and one pasted into the
    // wrong field is the key itself.
    let Ok(key) = tools.chat.config.web_search_key() else {
        return Produced::problem(NO_KEY);
    };

    // Before the person is asked, as a fetch's is: a rule that refuses is a statement that the
    // search does not happen.
    if let Err(denial) = policy.before_search_rules() {
        return Produced::problem(format!(
            "refused: {denial}. Do not retry this search and do not look for another route to the \
             web: say in your reply what you needed."
        ));
    }
    if policy.search_needs_approval() {
        let request = crate::confirm::FetchRequest {
            url: url.clone(),
            host: host.clone(),
            redirected_from: None,
            classified: crate::confirm::Classified::NotAsked,
        };
        if confirmer.confirm_fetch(&request) == Decision::Reject {
            policy.refuse_unapproved_search("web_search");
            return Produced::problem(
                "refused: the user did not approve this search. Do not retry the same search; \
                 ask what they would prefer.",
            );
        }
    }

    policy.endorse_search(&url);
    let label = match policy.before_search(&url) {
        Ok(label) => label,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };

    // The key is sent in one header, to the socket the request names and to no other: a hop that
    // leaves it is sent without. The `Accept` header is this program's own.
    let request = bravebot_net::Request::get(&url)
        .header("accept", "application/json")
        .socket_header("x-subscription-token", key.expose());
    let fetched = tools
        .chat
        .egress
        .fetch_watching(policy, request, label, Some(tools.cancel));
    policy.search_finished();
    drop(key);

    match fetched {
        Ok(response) => {
            let text = policy.render_in_place("web_search", &response.body, |bytes| {
                String::from_utf8_lossy(&bytes).into_owned()
            });
            let mut produced = Produced::new(
                text,
                format!("what a {} search returned", service.name()),
                format!("{}", response.status),
            )
            .of_content()
            .capped(response.truncated);
            // Always: a body from the index is never trusted.
            produced.untrusted = true;
            produced
        }
        Err(error) => Produced::problem(failure_words(&error)),
    }
}

/// What the planner is told when no usable key is configured. It names the two settings and no
/// variable and no path.
const NO_KEY: &str = "error: web_search cannot be used: the key named by the settings \
                      webSearch.env or webSearch.keyFile is missing or cannot be used as a key. Say \
                      so in your reply; only the user can fix it.";

/// The planner's sentence for a failed request, chosen from the kind of failure and never from
/// anything the service wrote (TOOL-4). The status number is formatted into one sentence that is
/// the same for every status and says what the usual numbers mean, so nothing the service sent
/// chooses which sentence the planner reads (WEBSEARCH-10).
fn failure_words(error: &bravebot_net::EgressError) -> String {
    use bravebot_net::EgressError;
    let said = match error {
        EgressError::Status { status, .. } => format!(
            "the search service answered with HTTP {status}. Usually 401 or 403 means the key was \
             refused and only the user can replace it, 402 means the plan behind the key does not \
             cover the request, 400 or 422 means an argument was not accepted, and 429 means the \
             key is being rate limited. Do not retry a refused key, a plan or a rate limit; say so \
             in your reply. Change an argument before trying again after a rejected request"
        ),
        EgressError::Denied(_) | EgressError::AddressRefused { .. } => {
            "the request was refused before it was sent or while it was followed, and nothing \
             was fetched from anywhere but the search service"
                .to_string()
        }
        EgressError::OutOfTime { .. } => {
            "the search service did not answer in the time given".to_string()
        }
        EgressError::Stopped { .. } => "the search was stopped".to_string(),
        EgressError::TooManyRedirects { .. }
        | EgressError::MissingLocation { .. }
        | EgressError::InsecureRedirect { .. } => {
            "the search service answered with a redirect that was not followed".to_string()
        }
        EgressError::InvalidUrl { .. } | EgressError::Transport { .. } => {
            "the search service could not be reached".to_string()
        }
    };
    format!("error: the search failed: {said}")
}
