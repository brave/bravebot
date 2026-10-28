//! Asking the services of a roster at once, and giving what they answer back in the order the
//! configuration lists them ([BACKEND-64](../../../docs/specs/backends.md#BACKEND-64)).
//!
//! The terminal and the window build their rosters from this, so which gateways and whether Brave
//! are asked, the bound on each request and the order of the answers are written once. What a row is
//! called and how a service that could not be read is told to a person differ between them, and stay
//! with each.

use crate::models::{self, Model, concurrently, listing_egress};
use bravebot_config::bedrock::Bedrock;
use bravebot_config::provider::{Credential, Provider};
use bravebot_config::{Config, RosterScope};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::Egress;

/// What one entry of the configuration contributes to a roster.
#[derive(Debug)]
pub enum Entry<'config> {
    /// An AWS account, from the tier variables or from a `provider` entry that names AWS. It has no
    /// listing to ask, so its rows are the models the configuration names.
    Bedrock(&'config Bedrock),
    /// A gateway whose block names its models. It is offered those and asked nothing, which is what
    /// keeps a configured gateway working offline.
    Named(&'config Provider),
    /// A gateway whose block names no models and a credential nothing holds, with no list compiled in
    /// for it. It is not asked, since the listing would come back refused.
    NoCredential(&'config Provider),
    /// A gateway whose block names no models, which was asked: its rows, or why they could not be had.
    Asked(&'config Provider, Result<Vec<Model>, String>),
}

/// One request of a roster, made on a thread of its own.
type Ask<'asks> = Box<dyn FnOnce() -> Result<Vec<Model>, String> + Send + 'asks>;

/// What the services of a roster answered, other than the accounts.
#[derive(Debug)]
pub struct Roster<'config> {
    /// The AWS accounts and the gateways, in the order the configuration lists them: the account the
    /// tier variables name first, and then each `provider` entry. A gateway the scope leaves out is
    /// not here.
    pub entries: Vec<Entry<'config>>,
    /// Brave's listing, where the scope asks it and this build can sign for it.
    pub brave: Option<Result<Vec<Model>, String>>,
}

/// Ask the services `scope` names, each request at once, and run `meanwhile` on this thread while
/// they are in flight.
///
/// `sink` makes the trail a request is recorded in. It is called on the thread that makes the
/// request, since a trail is not shared between requests.
///
/// A gateway whose block names no models is asked, because the alternative is a gateway configured
/// exactly as the tool this block's shape came from configures it, offering nothing. A block that
/// names a credential nothing holds is not asked, and a service with a list compiled in needs none
/// to be offered it. A block naming no credential at all is asked unauthenticated: that is somebody
/// saying the gateway wants none, and it is the block a local Ollama is configured with.
///
/// Brave is asked where `scope` says to and this build can sign for it. A build from source pointed
/// only at Bedrock has blank Brave credentials, and asking with them would list a roster whose every
/// request then fails unsigned.
///
/// Every request is made through [`listing_egress`], which gives up on a service that does not
/// answer in seconds.
pub fn ask<'config, S: Sink, U>(
    config: &'config Config,
    scope: RosterScope<'_>,
    sink: &(impl Fn() -> S + Sync),
    meanwhile: impl FnOnce() -> U,
) -> (Roster<'config>, U) {
    // Where each answer goes, so the order does not depend on which request answered first.
    enum Place<'config> {
        Settled(Entry<'config>),
        Asked(&'config Provider),
    }
    let mut places = Vec::new();
    places.extend(
        config
            .bedrock
            .as_ref()
            .map(|bedrock| Place::Settled(Entry::Bedrock(bedrock))),
    );
    let mut asks: Vec<Ask<'_>> = Vec::new();
    for provider in &config.providers {
        if let Some(bedrock) = provider.bedrock.as_ref() {
            places.push(Place::Settled(Entry::Bedrock(bedrock)));
        } else if !provider.models.is_empty() {
            places.push(Place::Settled(Entry::Named(provider)));
        } else if scope.asks_gateway(provider) {
            let credential = provider.credential(|name| std::env::var(name).ok());
            if matches!(credential, Credential::Absent) && provider.compiled_roster().is_none() {
                places.push(Place::Settled(Entry::NoCredential(provider)));
            } else {
                places.push(Place::Asked(provider));
                asks.push(Box::new(move || ask_gateway(sink, provider, &credential)));
            }
        }
    }
    let asks_brave = scope.brave && config.serves_aichat();
    if asks_brave {
        asks.push(Box::new(|| ask_brave(sink, config)));
    }

    let (answers, meanwhile) = concurrently(asks, meanwhile);

    // The gateways were put before Brave, so the answers are met in the order the places were.
    let mut answers = answers.into_iter();
    let unanswered = || Err(String::from("no answer"));
    let entries = places
        .into_iter()
        .map(|place| match place {
            Place::Settled(entry) => entry,
            Place::Asked(provider) => {
                Entry::Asked(provider, answers.next().unwrap_or_else(unanswered))
            }
        })
        .collect();
    let brave = asks_brave.then(|| answers.next().unwrap_or_else(unanswered));
    (Roster { entries, brave }, meanwhile)
}

/// Run a listing request under a policy that may fetch and nothing else.
///
/// A policy exists because `bravebot-net` is the only way out to the network and its gate takes one.
/// Routing is the listing itself: this is not a turn, nothing is read from the workspace, and no
/// model is involved, so there is no prompt to anchor it to. The caller names the destinations,
/// which come from configuration, and `request` makes the request under the policy.
fn within_policy<S: Sink, T>(
    sink: &impl Fn() -> S,
    routing: Routing,
    request: impl FnOnce(&mut Policy<'_, S>, &Egress) -> Result<T, crate::ChatError>,
) -> Result<T, String> {
    let mut sink = sink();
    let egress = listing_egress();
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .map_err(|denial| denial.to_string())
    .and_then(|mut policy| request(&mut policy, &egress).map_err(|error| error.to_string()))
}

/// Ask one gateway what it offers.
///
/// What comes back names the gateway the same way a configured model from it does, so one service
/// does not appear twice under two names in the same list.
fn ask_gateway<S: Sink>(
    sink: &impl Fn() -> S,
    provider: &Provider,
    credential: &Credential,
) -> Result<Vec<Model>, String> {
    let token = bearer(credential);
    // Resolved beside the token, from the same environment, for the same reason: a value may name a
    // variable, and one read at startup is stale once somebody exports another.
    let headers = provider.request_headers(|name| std::env::var(name).ok());
    // Both destinations are the gateway's own endpoint, which came from configuration. Nothing
    // fetched decides either, which is what makes asking a service for a list of names safe at all.
    let mut routing = Routing::new();
    routing.insert_trusted("models", provider.models_url());
    routing.insert_trusted("account-models", provider.account_models_url());
    within_policy(sink, routing, |policy, egress| {
        models::list_from_gateway(policy, provider, token, &headers, egress)
    })
}

/// The token a roster request is made with, where the block named one.
///
/// `None` is not an error here: a gateway configured without a credential is asked without one,
/// which is what a local Ollama wants. The listing reads `None` as a reason not to ask the
/// account-scoped route at all, since there is no account to scope an answer to. `Absent` reaches
/// this only for a service with a compiled list, which is offered without a request.
fn bearer(credential: &Credential) -> Option<&str> {
    match credential {
        Credential::Token(token) => Some(token.expose()),
        Credential::NotNeeded | Credential::Absent => None,
    }
}

/// Ask Brave's endpoint what it offers.
fn ask_brave<S: Sink>(sink: &impl Fn() -> S, config: &Config) -> Result<Vec<Model>, String> {
    let mut routing = Routing::new();
    routing.insert_trusted("models", config.models_url());
    within_policy(sink, routing, |policy, egress| {
        models::list(policy, config, egress)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::{Gateways, RosterAccounts};
    use bravebot_core::event::NullSink;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    fn configured(providers: &str) -> Config {
        let mut config = Config::from_lookup(|key| {
            match key {
                "SERVICES_KEY_AICHAT" => Some("test-signing-key"),
                "BRAVE_SERVICES_KEY_ID" => Some("test-key-id"),
                "BRAVE_AI_CHAT_ENDPOINT" => Some("https://brave.example.invalid"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured");
        config.providers =
            bravebot_config::Settings::parse(&format!(r#"{{"provider": {providers}}}"#))
                .providers()
                .to_vec();
        config
    }

    /// The one decision about a credential: which of the three states carries a bearer token into the
    /// listing. The header itself, and the account-first fallback the answer decides, belong to the
    /// listing.
    #[test]
    fn model_discovery_respects_all_three_credential_states() {
        assert_eq!(
            bearer(&Credential::Token(bravebot_config::Secret::new(
                "test-token"
            ))),
            Some("test-token")
        );
        // Asked without one rather than not asked, which is what a local Ollama wants.
        assert_eq!(bearer(&Credential::NotNeeded), None);
        assert_eq!(bearer(&Credential::Absent), None);
    }

    fn everything() -> RosterScope<'static> {
        RosterScope {
            accounts: RosterAccounts {
                chatgpt: false,
                xai: false,
            },
            gateways: Gateways::All,
            brave: false,
        }
    }

    fn listing(rows: &[&str]) -> Vec<u8> {
        let data: Vec<String> = rows
            .iter()
            .map(|id| format!(r#"{{"id": "{id}"}}"#))
            .collect();
        crate::stand_in::http(
            "200 OK",
            format!(r#"{{"data": [{}]}}"#, data.join(",")).as_bytes(),
        )
    }

    /// A server on this machine that takes every connection and never answers on it.
    fn a_server_that_never_answers() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 2 {
                    line.clear();
                }
                held.push(reader);
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// BACKEND-64: a roster gives up on a gateway that takes the connection and never answers, in
    /// seconds, and the gateways that answered keep their rows.
    ///
    /// The request is made through the roster's own entry point, so the egress it uses is the one a
    /// terminal or a window gets. Asked through an egress with the default bounds, which allow a
    /// reply ten minutes, the roster would not return and the watchdog here ends the test. The wait
    /// is the reply bound itself, ten seconds.
    #[test]
    fn a_roster_gives_up_on_a_gateway_that_takes_the_connection_and_never_answers() {
        let healthy = crate::stand_in::serve_paths(vec![("/v1/models", listing(&["fine"]))]);
        let hung = a_server_that_never_answers();
        let config = configured(&format!(
            r#"{{"healthy": {{"options": {{"baseURL": "{healthy}/v1"}}}},
                "hung": {{"options": {{"baseURL": "{hung}/v1"}}}}}}"#
        ));
        let (done, finished) = std::sync::mpsc::channel();

        let began = Instant::now();
        std::thread::spawn(move || {
            let (roster, ()) = ask(&config, everything(), &|| NullSink, || ());
            let kinds: Vec<(String, bool)> = roster
                .entries
                .iter()
                .map(|entry| match entry {
                    Entry::Asked(provider, answer) => (provider.id.clone(), answer.is_ok()),
                    other => (format!("{other:?}"), false),
                })
                .collect();
            let _ = done.send(kinds);
        });
        let kinds = finished
            .recv_timeout(Duration::from_secs(40))
            .expect("the roster did not return, so a request is not bounded");

        assert!(
            began.elapsed() < Duration::from_secs(30),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(
            kinds,
            [("healthy".to_string(), true), ("hung".to_string(), false)]
        );
    }

    /// BACKEND-64: the entries come back in the order the configuration lists them, with each kind
    /// of gateway told apart: one that names its models, one whose credential nothing holds, one that
    /// is refused, and one that is asked and answers. A scope that names one gateway leaves the
    /// others that name no models out, and still offers the models a block names.
    ///
    /// The regressions it rejects are a gateway asked with a credential nothing holds, a gateway the
    /// scope leaves out that is asked or listed, a gateway that names its models that is asked, and
    /// entries in the order the answers came back, which here is the reverse of the configuration's
    /// since the refused one answers first.
    #[test]
    fn the_entries_are_in_the_configurations_order_and_each_kind_of_gateway_is_told_apart() {
        let answers = crate::stand_in::serve_paths(vec![("/v1/models", listing(&["m"]))]);
        let config = configured(&format!(
            r#"{{"a-named": {{"options": {{"baseURL": "http://127.0.0.1:1/v1"}},
                              "models": {{"m": {{}}}}}},
                "b-keyless": {{"env": ["BRAVEBOT_TEST_UNSET_ROSTER_KEY"],
                               "options": {{"baseURL": "http://127.0.0.1:1/v1"}}}},
                "c-refused": {{"options": {{"baseURL": "http://127.0.0.1:1/v1"}}}},
                "d-answers": {{"options": {{"baseURL": "{answers}/v1"}}}}}}"#
        ));
        let described = |scope| {
            let (roster, ()) = ask(&config, scope, &|| NullSink, || ());
            assert!(roster.brave.is_none());
            roster
                .entries
                .iter()
                .map(|entry| match entry {
                    Entry::Named(provider) => format!("named {}", provider.id),
                    Entry::NoCredential(provider) => format!("no credential {}", provider.id),
                    Entry::Asked(provider, Ok(rows)) => {
                        format!("answered {} {}", provider.id, rows.len())
                    }
                    Entry::Asked(provider, Err(_)) => format!("refused {}", provider.id),
                    Entry::Bedrock(_) => "bedrock".to_string(),
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(
            described(everything()),
            [
                "named a-named",
                "no credential b-keyless",
                "refused c-refused",
                "answered d-answers 1"
            ]
        );
        assert_eq!(
            described(RosterScope {
                gateways: Gateways::Only("d-answers"),
                ..everything()
            }),
            ["named a-named", "answered d-answers 1"]
        );
    }
}
