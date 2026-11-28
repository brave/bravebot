//! Model rosters and pickers: which models are offered and which service answers each row.

use super::*;

/// Naming the service does not distinguish the row: Brave serves some of its own roster through
/// Bedrock and says so in the display name it sends, so "(Bedrock)" appeared on both halves of the
/// picker and told a person nothing about which account was about to be billed.
#[test]
fn a_configured_tier_is_not_confusable_with_a_brave_model_served_through_bedrock() {
    use bravebot_config::env_var;

    let mut brave = listed("gpt-5.5", Some(102_400));
    brave.display_name = "GPT-5.5 (Bedrock)".to_string();

    let roster = combined(
        bedrock_models(&bedrock_configured(&[
            (env_var::USE_BEDROCK, "1"),
            (env_var::AWS_REGION, "us-west-2"),
            (env_var::AWS_PROFILE, "some-profile"),
            (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
        ])),
        Ok(vec![brave]),
    )
    .expect("a roster");

    assert_eq!(roster.len(), 2);
    // The profile is what decides which credentials sign, and no name off the wire carries it.
    let configured = roster[0].provider.as_deref().expect("a service");
    assert!(configured.contains("some-profile"), "{configured}");
    assert_eq!(
        roster[1].provider, None,
        "the Brave roster names no service"
    );
}

/// A model a `provider` block named has no tier, so there is no tier word to draw a row from.
/// Drawn from one anyway, every such model would appear as "Opus", and an account reaching four
/// of them would show four rows with three names between them.
#[test]
fn a_bedrock_model_a_block_named_is_shown_under_that_name() {
    use bravebot_config::bedrock::{Bedrock, Entry};

    let models = bedrock_models(
        &Bedrock::from_provider(
            "us-west-2".to_string(),
            Some("sso".to_string()),
            vec![
                Entry {
                    tier: None,
                    id: "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc".to_string(),
                    name: Some("GPT-5.6 Sol (Bedrock)".to_string()),
                    context_window: Some(1_050_000),
                    output_limit: Some(32_000),
                },
                Entry {
                    tier: None,
                    id: "openai.gpt-5.6-sol".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: None,
                },
            ],
        )
        .expect("a valid region"),
    );

    assert_eq!(models[0].display_name, "GPT-5.6 Sol (Bedrock)");
    assert_eq!(
        models[0].key, "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc",
        "the row carries the ARN a request names, not the word a person reads"
    );
    assert_eq!(
        models[0].conversation_tokens,
        Some(1_050_000),
        "a window the block stated was thrown away"
    );

    // Nothing friendlier was said about the second, so its id stands.
    assert_eq!(models[1].display_name, "openai.gpt-5.6-sol");
    assert_eq!(
        models[1].conversation_tokens,
        Some(bravebot_config::bedrock::CONTEXT_WINDOW)
    );

    // Both are reached through the person's own account, which a row has to say.
    assert!(models.iter().all(|model| model.provider.is_some()));
}

/// A profile is optional, and a row still has to say the tier is reached through the person's own
/// account rather than through Brave's.
#[test]
fn a_tier_with_no_profile_configured_still_names_the_account() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    assert_eq!(models[0].display_name, "Opus");
    assert_eq!(
        models[0].provider.as_deref(),
        Some(t!(picker_service_bedrock))
    );
}

/// The same condition Claude Code applies: a tier appears only when its variable names a model.
/// An entry invented for an unset tier is a choice that fails at the far end.
#[test]
fn a_tier_with_no_model_configured_is_not_offered() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_SONNET_MODEL, "sonnet-arn"),
    ]));

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].display_name, "Sonnet");
}

/// A settings block adds to the roster rather than replacing it. Replacing it left a person who
/// configured one tier with a picker offering exactly one model, and no way back to the Brave
/// models every build has.
#[test]
fn configured_tiers_are_offered_alongside_the_brave_roster() {
    use bravebot_config::env_var;

    let configured = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    let roster = combined(
        configured,
        Ok(vec![
            bravebot_aichat::models::Model::automatic(),
            listed("claude-sonnet", Some(102_400)),
        ]),
    )
    .expect("a roster");

    let keys: Vec<&str> = roster.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(
        keys,
        ["opus-arn", bravebot_config::DEFAULT_MODEL, "claude-sonnet"]
    );
}

/// The tiers need no network to know. Losing them because the other half was unreachable would
/// leave the only models this configuration can definitely reach unpickable.
#[test]
fn an_unreachable_listing_still_offers_the_configured_tiers() {
    use bravebot_config::env_var;

    let configured = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    let roster =
        combined(configured, Err("the endpoint is unreachable".into())).expect("the tiers survive");
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].key, "opus-arn");
}

/// BACKEND-48: the picker does not offer a model this machine may not request, so a person reads
/// a list of what they can actually ask for rather than being refused after choosing.
///
/// Both lists in one file, because a build that honoured one and not the other would pass a test
/// of either half: a deny entry has to take a row out of a list an allow entry let through.
#[test]
fn the_picker_does_not_offer_a_model_this_machine_may_not_request() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join("tui-models-picker");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let path = scratch.join("managed.json");
    std::fs::write(
        &path,
        r#"{"models": {
                "allow": ["approved-opus", "expensive-opus"],
                "deny": ["expensive-opus"]
            }}"#,
    )
    .expect("a managed file");

    let mut config = Config::from_lookup(|name| match name {
        bravebot_config::env_var::ENDPOINT => Some("https://example.invalid".into()),
        bravebot_config::env_var::KEY_ID => Some("a-key-id".into()),
        bravebot_config::env_var::SIGNING_KEY => Some("a-signing-key".into()),
        _ => None,
    })
    .expect("configured");

    let roster = vec![
        listed("approved-opus", None),
        listed("expensive-opus", None),
        listed("some-other-model", None),
    ];
    let offered = |config: &Config, roster: &[bravebot_aichat::models::Model]| -> Vec<String> {
        requestable(config, roster.to_vec())
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    // With no managed file the roster is offered whole, which is every machine today.
    assert_eq!(offered(&config, &roster).len(), 3);

    config.models = bravebot_config::Managed::at(&path).models().clone();
    assert_eq!(offered(&config, &roster), vec!["approved-opus".to_string()]);
}

/// With nothing configured there is nothing to fall back to, and a picker showing an empty list
/// would read as a backend with no models rather than as a listing that failed.
#[test]
fn an_unreachable_listing_with_no_tiers_configured_is_still_a_failure() {
    assert!(combined(vec![], Err("the endpoint is unreachable".into())).is_err());
}

/// `automatic` means "let the server choose", which Bedrock does not offer: a request names one
/// model and gets it or an error. It reaches the picker from the Brave half instead.
#[test]
fn the_bedrock_picker_does_not_offer_automatic() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    assert!(!models.iter().any(|model| model.is_automatic()));
}

/// Premium means the premium key. Reaching a model through somebody's own AWS account does
/// not involve one, and marking it premium would ask them to import a subscription to use what
/// they already pay for.
#[test]
fn bedrock_models_are_not_marked_premium() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    assert!(!models[0].premium);
}

/// The budget lookup shares this listing, so a Bedrock entry has to carry a window or a session
/// would fall back to the default and compact five times sooner than it had to.
#[test]
fn a_bedrock_entry_carries_the_window_the_budget_is_taken_from() {
    use bravebot_config::env_var;

    let models = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    assert_eq!(
        advertised_window(&models, Some("opus-arn")),
        Some(bravebot_config::bedrock::CONTEXT_WINDOW)
    );
}

/// A gateway's roster is what the file named and nothing else. Asking the gateway would list
/// hundreds of models across upstreams nobody configured, and would cost a round trip on a path
/// that has to work offline.
///
/// Each key is qualified by the provider's own id, which is what makes a remembered choice say
/// which service it was for.
#[test]
fn only_the_gateway_models_the_file_named_are_offered() {
    let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}, "moonshot/kimi-k2": {}}"#));
    let keys: Vec<&str> = models.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(
        keys,
        ["openrouter/moonshot/kimi-k2", "openrouter/z-ai/glm-4.6"]
    );
}

/// The same model may be reachable more than one way, billed and credentialled differently, and
/// nothing else on the row would say which was about to answer.
#[test]
fn a_gateway_row_says_which_service_answers_it() {
    let models = provider_models(&gateway(r#"{"anthropic/claude-sonnet-4.5": {}}"#));
    assert_eq!(models[0].display_name, "anthropic/claude-sonnet-4.5");
    assert_eq!(models[0].provider.as_deref(), Some("OpenRouter"));
}

/// The note left when a model is chosen names the service that answers it, because the row that
/// carried the service is gone by then, and a model from Brave's own roster names none. The
/// regressions it rejects are a note that drops the service, and one that invents a service for
/// Brave's roster.
#[test]
fn the_note_left_on_choosing_a_model_names_its_service() {
    let mut config = a_config_offering_three_tiers();
    let mut session = Session::new("none");
    let mut gateway_model = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#)).remove(0);
    let mut brave_model = gateway_model.clone();
    brave_model.display_name = "GPT-5.5".to_string();
    brave_model.provider = None;
    brave_model.key = "gpt-5.5".to_string();
    gateway_model.key = "openrouter/z-ai/glm-4.6".to_string();

    model_picked(
        &mut session,
        &mut config,
        gateway_model,
        crate::model_prompt::Scope::EverySession,
    );
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        said.iter()
            .any(|text| text.contains("z-ai/glm-4.6") && text.contains("OpenRouter")),
        "{said:?}"
    );

    let mut session = Session::new("none");
    model_picked(
        &mut session,
        &mut config,
        brave_model,
        crate::model_prompt::Scope::EverySession,
    );
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(said.iter().any(|text| text.contains("GPT-5.5")), "{said:?}");
    assert!(
        said.iter().all(|text| !text.contains("openrouter")),
        "{said:?}"
    );
}

/// The budget lookup shares this listing, so an entry has to carry a window. A model that stated
/// one gets it, and the figure has to survive reaching the picker or the session compacts against
/// a window belonging to a different service.
#[test]
fn a_gateway_entry_carries_the_window_the_budget_is_taken_from() {
    let models = provider_models(&gateway(
        r#"{"anthropic/claude-sonnet-4.5": {"limit": {"context": 1000000, "output": 64000}}}"#,
    ));
    assert_eq!(
        advertised_window(&models, Some("openrouter/anthropic/claude-sonnet-4.5")),
        Some(1_000_000)
    );
}

/// A window nobody stated is the conservative default rather than nothing at all. Reporting
/// nothing would leave the budget on a figure chosen for a different service, and a budget above
/// the real window does not delay compaction but removes it.
#[test]
fn a_gateway_model_with_no_stated_window_still_carries_one() {
    let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
    assert_eq!(
        advertised_window(&models, Some("openrouter/z-ai/glm-4.6")),
        Some(bravebot_config::provider::CONTEXT_WINDOW)
    );
}

/// A gateway is additive on the same terms Bedrock is: its models are offered beside the others
/// rather than in place of them, so nothing a person could reach before stops being reachable.
#[test]
fn gateway_models_are_offered_alongside_the_other_rosters() {
    use bravebot_config::env_var;

    let mut configured = bedrock_models(&bedrock_configured(&[
        (env_var::USE_BEDROCK, "1"),
        (env_var::AWS_REGION, "us-west-2"),
        (env_var::BEDROCK_OPUS_MODEL, "opus-arn"),
    ]));
    configured.extend(provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#)));

    let roster = combined(
        configured,
        Ok(vec![bravebot_aichat::models::Model::automatic()]),
    )
    .expect("a roster");
    let keys: Vec<&str> = roster.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "opus-arn",
            "openrouter/z-ai/glm-4.6",
            bravebot_config::DEFAULT_MODEL
        ]
    );
}

/// A fetched roster arrives roughly newest-first, which puts a model nobody has heard of at the
/// top and buries the one they work with. The model in force leads, and the rest sort by name so
/// a half-remembered one is where somebody would look for it.
#[test]
fn a_fetched_roster_leads_with_the_model_in_force() {
    // Last in the gateway's order and last alphabetically, so leading is the only way it gets
    // to the top: a test whose input already led with it would pass against no sorting at all.
    let roster = vec![
        listed("openrouter/anthropic/claude-sonnet-4.5", None),
        listed("openrouter/moonshot/kimi-k2", None),
        listed("openrouter/z-ai/glm-4.6", None),
    ];
    let ordered = in_reading_order(roster, "openrouter/z-ai/glm-4.6");
    let keys: Vec<&str> = ordered.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "openrouter/z-ai/glm-4.6",
            "openrouter/anthropic/claude-sonnet-4.5",
            "openrouter/moonshot/kimi-k2"
        ]
    );
}

/// Nothing in force is the ordinary first run. Every row still sorts by name rather than keeping
/// an order that means nothing to the person reading it.
#[test]
fn a_fetched_roster_nobody_has_chosen_from_is_still_sorted() {
    let roster = vec![
        listed("openrouter/z-ai/glm-4.6", None),
        listed("openrouter/anthropic/claude-sonnet-4.5", None),
    ];
    let ordered = in_reading_order(roster, bravebot_config::DEFAULT_MODEL);
    let keys: Vec<&str> = ordered.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "openrouter/anthropic/claude-sonnet-4.5",
            "openrouter/z-ai/glm-4.6"
        ]
    );
}

/// A gateway's models need no network to know, so a Brave listing that could not be fetched must
/// not withdraw them: that is the position somebody offline is most likely to be in.
#[test]
fn an_unreachable_listing_still_offers_the_gateway_models() {
    let configured = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
    let roster = combined(configured, Err("the endpoint is unreachable".into()))
        .expect("the gateway models survive");
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].key, "openrouter/z-ai/glm-4.6");
}

/// A configuration whose Brave credentials are blank and which reaches Google Vertex through a
/// block naming `models` where given. With a key, the key is in the block, so that it is found
/// whatever this process has exported. Without one, the block names a variable nothing sets.
fn a_config_with_google_vertex(key: Option<&str>, models: Option<&str>) -> Config {
    let credential = match key {
        Some(key) => {
            format!(r#""options": {{"project": "example-project-1", "apiKey": "{key}"}}"#)
        }
        None => r#""options": {"project": "example-project-1"},
                "env": ["BRAVEBOT_TEST_UNSET_VERTEX_KEY"]"#
            .to_string(),
    };
    let models = models
        .map(|models| format!(r#", "models": {models}"#))
        .unwrap_or_default();
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(&format!(
        r#"{{"provider": {{"google-vertex": {{{credential}{models}}}}}}}"#
    ))
    .providers()
    .to_vec();
    assert_eq!(config.providers.len(), 1, "the block configured no service");
    config
}

/// The rows `/model` offers for the Google Vertex service.
fn google_vertex_rows(config: &Config) -> Vec<String> {
    list_models_for_picker(config, None)
        .expect("a roster")
        .models
        .into_iter()
        .map(|model| model.key)
        .filter(|key| key.starts_with("google-vertex/"))
        .collect()
}

/// BACKEND-49: Vertex has no listing a key can call, so a service naming no models is offered the
/// list compiled in, in the order any fetched roster is read in. Offering it asks nothing, so a
/// key that is not found keeps no row out, as it keeps out none of the models a block names.
#[test]
fn the_picker_offers_the_compiled_models_for_a_google_vertex_service_naming_none() {
    for key in [Some("placeholder-key"), None] {
        let config = a_config_with_google_vertex(key, None);
        let compiled = config.providers[0]
            .compiled_roster()
            .expect("a compiled list");
        let mut expected: Vec<String> = compiled
            .iter()
            .map(|id| format!("google-vertex/{id}"))
            .collect();
        expected.sort();
        assert!(!expected.is_empty());
        assert_eq!(google_vertex_rows(&config), expected, "key {key:?}");
    }
}

/// BACKEND-49: a block naming models is offered those and no others. The one named is on no
/// compiled list, so a picker adding the two together would offer more than this one row.
#[test]
fn a_google_vertex_block_naming_models_is_offered_those_alone() {
    let config = a_config_with_google_vertex(
        Some("placeholder-key"),
        Some(r#"{"google/gemini-3-flash-preview": {}}"#),
    );
    assert_eq!(
        google_vertex_rows(&config),
        ["google-vertex/google/gemini-3-flash-preview"]
    );
}

/// BACKEND-5: the Brave roster is asked for only where this build holds the keys to sign for it.
/// The listing itself is unsigned, so a build pointed at AWS with the endpoint set and a key
/// blank would be answered, and would offer models whose every request then fails unsigned.
///
/// The same endpoint is asked once both keys are there, so a server that answered nothing cannot
/// be what keeps the roster out.
#[test]
fn the_brave_roster_is_offered_only_where_this_build_can_sign_for_it() {
    use bravebot_config::env_var;

    let (endpoint, asked) = a_listing_server(
        r#"[{"key": "brave-model", "display_name": "Brave Model",
                "capabilities": ["chat", "tools"], "options": {"access": "basic_and_premium"}}]"#,
    );
    let holding = |key_id: bool, signing_key: bool| {
        Config::from_lookup(|name| match name {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            env_var::ENDPOINT => Some(endpoint.clone()),
            env_var::KEY_ID if key_id => Some("a-key-id".into()),
            env_var::SIGNING_KEY if signing_key => Some("a-signing-key".into()),
            _ => None,
        })
        .expect("an account named on its own is a working configuration")
    };
    let offered = |config: &Config| -> Vec<String> {
        list_models(config, None)
            .expect("a roster")
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    for (key_id, signing_key) in [(false, false), (true, false), (false, true)] {
        let unsigned = holding(key_id, signing_key);
        let state = format!("key id held: {key_id}, signing key held: {signing_key}");
        assert_eq!(offered(&unsigned), ["opus-arn"], "{state}");
        assert!(!unsigned.serves_aichat(), "{state}");
    }
    assert_eq!(
        offered(&holding(true, true)),
        ["opus-arn", bravebot_config::DEFAULT_MODEL, "brave-model"]
    );
    assert_eq!(asked.try_iter().collect::<Vec<_>>(), ["/v1/models"]);
}

/// BACKEND-5: a gateway whose block names a credential nothing holds is not asked for its
/// models. Its listing would be refused and every row from it would fail the same way, so the
/// only useful thing to say about it is what `doctor` says: no credential found.
///
/// A block naming no credential is asked by the same server. That is a local Ollama's block, and
/// it shows the refusal is about the credential rather than about asking.
#[test]
fn a_gateway_whose_credential_nothing_holds_is_not_asked_for_its_models() {
    let (base, asked) = a_listing_server(r#"{"data": [{"id": "llama3"}]}"#);
    let with = |credential: &str| {
        let mut config = a_config_with_a_named_roster();
        config.providers = bravebot_config::Settings::parse(&format!(
            r#"{{"provider": {{"local": {{"options": {{"baseURL": "{base}/v1"}}{credential}}}}}}}"#
        ))
        .providers()
        .to_vec();
        assert_eq!(config.providers.len(), 1, "the block configured no gateway");
        config
    };
    let offered = |config: &Config| -> Vec<String> {
        list_models_for_picker(config, None)
            .expect("a roster")
            .models
            .into_iter()
            .map(|model| model.key)
            .filter(|key| key.starts_with("local/"))
            .collect()
    };

    assert_eq!(
        offered(&with(r#", "env": ["BRAVEBOT_TEST_UNSET_GATEWAY_KEY"]"#)),
        Vec::<String>::new()
    );
    assert_eq!(offered(&with("")), ["local/llama3"]);
    assert_eq!(asked.try_iter().collect::<Vec<_>>(), ["/v1/models"]);
}

/// The AWS tier `a_config_with_a_named_roster` names, and a gateway for each of `bases`, keyed
/// `one`, `two` and `three` in the order given, that names no models.
fn a_config_with_gateways_naming_no_models(bases: &[&str]) -> Config {
    let blocks: Vec<String> = bases
        .iter()
        .zip(["one", "two", "three"])
        .map(|(base, id)| format!(r#""{id}": {{"options": {{"baseURL": "{base}/v1"}}}}"#))
        .collect();
    let mut config = a_config_with_a_named_roster();
    config.providers =
        bravebot_config::Settings::parse(&format!(r#"{{"provider": {{{}}}}}"#, blocks.join(",")))
            .providers()
            .to_vec();
    assert_eq!(
        config.providers.len(),
        bases.len(),
        "a block configured no gateway"
    );
    config
}

fn keys_of(models: Vec<bravebot_aichat::models::Model>) -> Vec<String> {
    models.into_iter().map(|model| model.key).collect()
}

/// BACKEND-64: a lookup for one model's row waits on the gateway that answers that model and on
/// no other. A lookup that asks every gateway makes a start take the sum of their latencies to
/// learn the window of a model only one of them serves.
///
/// Each gateway is asked for a model of its own in turn, and a model of the AWS account and the
/// configured default, which is Brave's and unreachable in this build, ask neither. The faults it
/// rejects are a lookup that asks both, one that asks the first whatever is in force, and one
/// that asks a gateway for a model an account serves.
#[test]
fn a_lookup_for_one_models_row_asks_only_the_gateway_that_serves_it() {
    let (one, asked_one) = a_listing_server(r#"{"data": [{"id": "llama3"}]}"#);
    let (two, asked_two) = a_listing_server(r#"{"data": [{"id": "llama3"}]}"#);
    let config = a_config_with_gateways_naming_no_models(&[&one, &two]);
    let rows = |chosen: Option<&str>| keys_of(list_models(&config, chosen).expect("a roster"));
    let requests = || {
        (
            asked_one.try_iter().collect::<Vec<_>>(),
            asked_two.try_iter().collect::<Vec<_>>(),
        )
    };
    let (listing, nothing) = (vec!["/v1/models".to_string()], Vec::<String>::new());

    assert_eq!(rows(Some("one/llama3")), ["opus-arn", "one/llama3"]);
    assert_eq!(requests(), (listing.clone(), nothing.clone()));
    assert_eq!(rows(Some("two/llama3")), ["opus-arn", "two/llama3"]);
    assert_eq!(requests(), (nothing.clone(), listing));
    assert_eq!(rows(Some("opus-arn")), ["opus-arn"]);
    assert_eq!(rows(None), ["opus-arn"]);
    assert_eq!(requests(), (nothing.clone(), nothing));
}

/// BACKEND-64: Brave's roster is asked for the sake of a model Brave answers, and for no other. A
/// start on a gateway's model, or on an AWS account's, no longer waits on Brave's endpoint as well.
#[test]
fn a_lookup_asks_brave_only_for_a_model_brave_answers() {
    use bravebot_config::env_var;

    let (brave, asked_brave) = a_listing_server(
        r#"[{"key": "brave-model", "display_name": "Brave Model",
                "capabilities": ["chat", "tools"], "options": {"access": "basic_and_premium"}}]"#,
    );
    let (one, asked_one) = a_listing_server(r#"{"data": [{"id": "llama3"}]}"#);
    let mut config = Config::from_lookup(|name| match name {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-west-2".into()),
        env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
        env_var::ENDPOINT => Some(brave.clone()),
        env_var::KEY_ID => Some("a-key-id".into()),
        env_var::SIGNING_KEY => Some("a-signing-key".into()),
        _ => None,
    })
    .expect("a configuration that can sign for Brave");
    config.providers = a_config_with_gateways_naming_no_models(&[&one]).providers;
    assert!(config.serves_aichat());
    let rows = |chosen: Option<&str>| keys_of(list_models(&config, chosen).expect("a roster"));
    let requests = || (asked_brave.try_iter().count(), asked_one.try_iter().count());

    assert_eq!(
        rows(Some("brave-model")),
        ["opus-arn", bravebot_config::DEFAULT_MODEL, "brave-model"]
    );
    assert_eq!(requests(), (1, 0), "a Brave model asked a gateway");
    assert_eq!(rows(Some("one/llama3")), ["opus-arn", "one/llama3"]);
    assert_eq!(requests(), (0, 1), "a gateway's model asked Brave");
    assert_eq!(rows(Some("opus-arn")), ["opus-arn"]);
    assert_eq!(requests(), (0, 0), "an AWS account's model asked someone");
}

/// A loopback gateway listing the one model `id`. It answers only once `expected` requests to
/// the gateways sharing `gate` are in flight at once, or after five seconds, so asking them one
/// after the next fails a test and does not hang it. `met` says whether they were. `lag` is how
/// long it then waits to answer.
fn a_gateway_that_waits_for_its_peers(
    id: &'static str,
    gate: &std::sync::Arc<(std::sync::Mutex<usize>, std::sync::Condvar)>,
    expected: usize,
    met: &std::sync::mpsc::Sender<bool>,
    lag: std::time::Duration,
) -> String {
    use std::io::{BufRead, BufReader, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = format!("http://{}", listener.local_addr().expect("an address"));
    let (gate, met) = (gate.clone(), met.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (gate, met) = (gate.clone(), met.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut header = String::new();
                while matches!(reader.read_line(&mut header), Ok(read) if read > 2) {
                    header.clear();
                }
                let (arrived, signal) = &*gate;
                let mut here = arrived.lock().expect("the count");
                *here += 1;
                signal.notify_all();
                let (count, waited) = signal
                    .wait_timeout_while(here, std::time::Duration::from_secs(5), |here| {
                        *here < expected
                    })
                    .expect("the count");
                drop(count);
                let _ = met.send(!waited.timed_out());
                std::thread::sleep(lag);
                let body = format!(r#"{{"data": [{{"id": "{id}"}}]}}"#);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = reader.into_inner().write_all(response.as_bytes());
            });
        }
    });
    address
}

/// BACKEND-64: the picker asks every gateway at once, and offers their rows in the order the
/// configuration lists them whichever answered first.
///
/// Neither stand-in answers until the other has been asked, so a picker that asks them one after
/// the next fails by the first finding itself alone. The first lags, so the second answers
/// first. A picker that waited on the sum of its gateways' latencies is what made `/model`
/// slow as gateways were added.
#[test]
fn the_picker_asks_every_gateway_at_once_and_lists_them_in_the_configurations_order() {
    let gate = std::sync::Arc::new((std::sync::Mutex::new(0), std::sync::Condvar::new()));
    let (met, together) = std::sync::mpsc::channel();
    let one = a_gateway_that_waits_for_its_peers(
        "one-model",
        &gate,
        2,
        &met,
        std::time::Duration::from_millis(100),
    );
    let two =
        a_gateway_that_waits_for_its_peers("two-model", &gate, 2, &met, std::time::Duration::ZERO);
    let config = a_config_with_gateways_naming_no_models(&[&one, &two]);

    let picker = list_models_for_picker(&config, None).expect("a roster");

    assert_eq!(
        together.try_iter().collect::<Vec<_>>(),
        [true, true],
        "the gateways were asked one after the next"
    );
    assert_eq!(
        keys_of(picker.models),
        ["opus-arn", "one/one-model", "two/two-model"]
    );
    assert!(picker.problems.is_empty(), "{:?}", picker.problems);
}

/// BACKEND-64: asking for the window of the model a session opened on adopts nothing by
/// itself, and the window is adopted when the answer is taken. A start that adopted while it
/// asked would be waiting for the service again.
#[test]
fn asking_for_the_window_at_start_adopts_it_only_when_the_answer_is_taken() {
    let (base, _asked) =
        a_listing_server(r#"{"data": [{"id": "llama3", "context_length": 65536}]}"#);
    let mut config = a_config_with_gateways_naming_no_models(&[&base]);
    let mut session = Session::new("none");
    session.choose_model("one/llama3".to_string(), &config);

    ask_for_the_budget_of_the_model_in_force(&mut session, &config);
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET,
        "asking adopted the window"
    );

    assert!(adopt_the_awaited_budget(&mut session, &mut config, None));
    assert_eq!(config.context_budget, 65_536);
}

/// BACKEND-64: the loop's pass takes the window if it has arrived and does not wait for it, and
/// takes it once.
#[test]
fn a_pass_of_the_loop_takes_an_arrived_window_once_and_does_not_wait_for_one_that_has_not() {
    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    session.choose_model("opus-arn".to_string(), &config);
    let (answer, arrival) = std::sync::mpsc::channel();
    session.await_listing("opus-arn".to_string(), arrival);

    let began = std::time::Instant::now();
    assert!(!adopt_the_awaited_budget(&mut session, &mut config, None));
    assert!(
        began.elapsed() < std::time::Duration::from_secs(1),
        "the pass waited for an answer that had not come"
    );
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET
    );

    answer
        .send(Ok(vec![listed("opus-arn", Some(65_536))]))
        .expect("the lookup is awaited");
    assert!(adopt_the_awaited_budget(&mut session, &mut config, None));
    assert_eq!(config.context_budget, 65_536);
    assert!(
        !adopt_the_awaited_budget(&mut session, &mut config, None),
        "an answer was taken twice"
    );
}

/// BACKEND-64: the window of a model that is no longer the one in force is not adopted, and a
/// turn does not wait for it. The pick that replaced the model brought a window of its own, and
/// the answer about the one before it would overwrite that.
///
/// The lookup is still unanswered when the turn is sent, so a turn that waited for it before
/// asking which model it is about would hold for the whole bound, and the answer that came after
/// would be dropped on the pass of the loop.
#[test]
fn a_lookup_about_a_model_that_is_no_longer_in_force_is_dropped_and_not_waited_for() {
    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    session.choose_model("opus-arn".to_string(), &config);
    let (answer, arrival) = std::sync::mpsc::channel();
    session.await_listing("opus-arn".to_string(), arrival);
    session.choose_model("another-model".to_string(), &config);

    let began = std::time::Instant::now();
    assert!(!adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(BUDGET_WAIT)
    ));
    assert!(
        began.elapsed() < BUDGET_WAIT / 2,
        "the turn waited for a lookup about a model no longer in force"
    );

    // The answer that arrives afterwards finds nothing awaiting it.
    assert!(
        answer
            .send(Ok(vec![listed("opus-arn", Some(65_536))]))
            .is_err()
    );
    assert!(!adopt_the_awaited_budget(&mut session, &mut config, None));
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET
    );
}

/// BACKEND-64: a turn waits for a lookup once. One it has waited out stays for the loop to take,
/// and the next turn does not hold for the whole bound again.
#[test]
fn a_turn_waits_for_a_lookup_once_and_the_loop_still_takes_its_late_answer() {
    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    session.choose_model("opus-arn".to_string(), &config);
    let (answer, arrival) = std::sync::mpsc::channel();
    session.await_listing("opus-arn".to_string(), arrival);

    let first = std::time::Instant::now();
    assert!(!adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(std::time::Duration::from_millis(300))
    ));
    assert!(
        first.elapsed() >= std::time::Duration::from_millis(250),
        "the first turn did not wait"
    );

    let second = std::time::Instant::now();
    assert!(!adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(BUDGET_WAIT)
    ));
    assert!(
        second.elapsed() < std::time::Duration::from_millis(250),
        "the next turn waited for a lookup already waited out"
    );

    answer
        .send(Ok(vec![listed("opus-arn", Some(65_536))]))
        .expect("still awaited");
    assert!(adopt_the_awaited_budget(&mut session, &mut config, None));
    assert_eq!(config.context_budget, 65_536);
}

/// BACKEND-64: a turn waits for the window it is sized against, for a bounded time. An answer
/// that comes while it waits is adopted; a wait that runs out leaves the lookup awaited for the
/// loop to take; and a lookup whose thread ended without answering does not hold the turn for
/// the whole wait.
#[test]
fn a_turn_waits_for_the_window_it_is_sized_against_for_a_bounded_time() {
    let rows = || Ok(vec![listed("opus-arn", Some(65_536))]);
    let awaiting = || {
        let mut session = Session::new("none");
        let config = a_config_with_a_named_roster();
        session.choose_model("opus-arn".to_string(), &config);
        let (answer, arrival) = std::sync::mpsc::channel();
        session.await_listing("opus-arn".to_string(), arrival);
        (session, config, answer)
    };

    // Answered while the turn waits.
    let (mut session, mut config, answer) = awaiting();
    let sender = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        answer.send(rows()).expect("the lookup is awaited");
    });
    assert!(adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(BUDGET_WAIT)
    ));
    assert_eq!(config.context_budget, 65_536);
    sender.join().expect("the sender");

    // A wait that runs out leaves the lookup for the loop.
    let (mut session, mut config, answer) = awaiting();
    assert!(!adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(std::time::Duration::from_millis(50))
    ));
    answer.send(rows()).expect("still awaited");
    assert!(adopt_the_awaited_budget(&mut session, &mut config, None));
    assert_eq!(config.context_budget, 65_536);

    // A lookup that ended without an answer is dropped without waiting out the bound.
    let (mut session, mut config, answer) = awaiting();
    drop(answer);
    let began = std::time::Instant::now();
    assert!(!adopt_the_awaited_budget(
        &mut session,
        &mut config,
        Some(BUDGET_WAIT)
    ));
    assert!(began.elapsed() < BUDGET_WAIT / 2, "waited out the bound");
    assert!(session.take_awaited_listing(None).is_none());
}

/// BACKEND-19: a gateway that names no models and whose listing could not be read is said in the
/// picker, by the name its block gives it, with the way to name one of its models. A gateway
/// whose listing was read, one whose key is missing, which `doctor` says, and one that names its
/// models and so is not asked, are not mentioned, and each keeps the rows it has. The two ways a
/// listing is unreadable are both tried, a service that is not there and one that answers with
/// something that is not a listing. The regressions it rejects are a roster that fails silently,
/// a sentence for a gateway nobody could have asked, and one gateway's failure taking another's
/// rows with it.
#[test]
fn the_picker_says_which_gateway_could_not_be_listed_and_says_nothing_of_the_rest() {
    use models::scripted_account::nobody_signed_in;
    nobody_signed_in();
    // Held, since a server whose receiver is gone stops answering.
    let (readable, _asked_of_readable) = a_listing_server(r#"{"data": [{"id": "llama3"}]}"#);
    let (garbled, _asked_of_garbled) = a_listing_server("this is not a listing");
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(&format!(
        r#"{{"provider": {{
                "readable": {{"name": "Readable Gateway", "options": {{"baseURL": "{readable}/v1"}}}},
                "garbled": {{"name": "Garbled Gateway", "options": {{"baseURL": "{garbled}/v1"}}}},
                "down": {{"name": "Down Gateway", "options": {{"baseURL": "http://127.0.0.1:1/v1"}}}},
                "keyless": {{"name": "Keyless Gateway", "env": ["BRAVEBOT_TEST_UNSET_GATEWAY_KEY"],
                             "options": {{"baseURL": "http://127.0.0.1:1/v1"}}}},
                "named": {{"name": "Named Gateway", "options": {{"baseURL": "http://127.0.0.1:1/v1"}},
                           "models": {{"m": {{}}}}}}
            }}}}"#
    ))
    .providers()
    .to_vec();
    assert_eq!(config.providers.len(), 5, "a block configured no gateway");

    let mut session = Session::new("none");
    open_model_picker(
        &mut session,
        &config,
        crate::model_prompt::Scope::EverySession,
    );
    session.settle_picking();

    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    let rows: Vec<String> = picker.matching().iter().map(|m| m.key.clone()).collect();
    let mut said: Vec<String> = session.transcript.iter().map(|e| e.text.clone()).collect();
    said.sort();
    let mut expected = vec![
        t!(
            session_gateway_models_unreadable,
            gateway = "Down Gateway",
            id = "down"
        ),
        t!(
            session_gateway_models_unreadable,
            gateway = "Garbled Gateway",
            id = "garbled"
        ),
    ];
    expected.sort();
    assert_eq!(said, expected);
    for (gateway, id) in [("Down Gateway", "down"), ("Garbled Gateway", "garbled")] {
        let note = said.iter().find(|note| note.contains(gateway)).unwrap();
        assert!(
            note.contains(&format!(r#"`bravebot -i "<prompt>" --model {id}/<model>`"#))
                && note.contains(&format!(r#"`bravebot "<task>" --model {id}/<model>`"#)),
            "no command that names a model of {gateway}: {note}"
        );
        assert!(
            !note.contains("bravebot --model"),
            "a form that needs a task: {note}"
        );
    }
    for rows_of in ["readable/llama3", "named/m", "opus-arn"] {
        assert!(rows.contains(&rows_of.to_string()), "{rows_of}: {rows:?}");
    }
    for silent in ["down/", "garbled/", "keyless/"] {
        assert!(
            !rows.iter().any(|key| key.starts_with(silent)),
            "{silent} has rows: {rows:?}"
        );
    }
}

/// BACKEND-5, BACKEND-29: a model an AWS `provider` entry names is offered under a name the
/// Bedrock backend answers to, and that name reaches the entry's own account. Qualified by the
/// entry's id, as a gateway's rows are, the name reaches no Bedrock account and no gateway, and
/// the request goes to Brave's endpoint naming a model it does not serve.
#[test]
fn an_aws_provider_entrys_models_are_offered_under_names_bedrock_answers_to() {
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(
        r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-east-1"},
                "models": {"openai.gpt-5.6-sol": {}}}}}"#,
    )
    .providers()
    .to_vec();
    assert_eq!(
        config.providers.len(),
        1,
        "the block configured no AWS entry"
    );
    assert!(config.providers[0].bedrock.is_some());

    let offered: Vec<String> = list_models(&config, None)
        .expect("a roster")
        .into_iter()
        .map(|model| model.key)
        .collect();
    assert_eq!(offered, ["opus-arn", "openai.gpt-5.6-sol"]);
    let regions: Vec<Option<&str>> = offered
        .iter()
        .map(|key| {
            config
                .bedrock_for(key)
                .map(|account| account.region.as_str())
        })
        .collect();
    assert_eq!(regions, [Some("us-west-2"), Some("us-east-1")]);
}

/// A model chosen in an earlier session is read back off disk, and the window that came with it
/// is not: it is reported by the listing and nowhere else. Until this was looked up, a session
/// with room for a hundred thousand tokens compacted at twenty-four thousand.
#[test]
fn the_window_of_a_model_chosen_earlier_is_found_in_the_listing() {
    let models = [
        listed("claude-opus", Some(102_400)),
        listed("some-other-model", Some(8_000)),
    ];
    assert_eq!(
        advertised_window(&models, Some("claude-opus")),
        Some(102_400)
    );
}

/// BACKEND-14, COMPACT-9: with no pick the model in force is the configured default, and its
/// window is the budget. Looked up under `None` the default stood at 24,000 for a model
/// advertising 131,072, which is three quarters of the conversation it could have held.
#[test]
fn a_session_with_no_pick_adopts_the_window_of_the_configured_default() {
    let mut config = a_config_with_a_named_roster();
    assert_eq!(config.default_model, "opus-arn");
    let mut session = Session::new("none");
    assert_eq!(session.model(), None);

    adopt_budget_for_current_model(&mut session, &mut config);

    assert_eq!(
        config.context_budget,
        bravebot_config::bedrock::CONTEXT_WINDOW
    );
    assert!(!config.budget_is_guessed());
}

/// BACKEND-64, COMPACT-9: a session that starts on a model whose roster row advertises a window
/// takes the window as its budget and opens with no note about it, since a person who has done
/// nothing is told nothing; the same window taken from a model the person picks is said. The
/// faults it rejects: the start writing the note, and the pick going quiet with it.
#[test]
fn a_start_adopts_an_advertised_window_without_a_note_and_a_pick_says_it() {
    let said_budget = |session: &Session| {
        session
            .transcript
            .iter()
            .any(|entry| entry.text.contains("compacting above"))
    };
    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");

    adopt_budget_for_current_model(&mut session, &mut config);

    assert_eq!(
        config.context_budget,
        bravebot_config::bedrock::CONTEXT_WINDOW
    );
    assert!(!config.budget_is_guessed());
    assert!(!said_budget(&session), "the start said the budget");

    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    model_picked(
        &mut session,
        &mut config,
        listed("opus-arn", Some(65_536)),
        crate::model_prompt::Scope::EverySession,
    );

    assert_eq!(config.context_budget, 65_536);
    assert!(said_budget(&session), "the pick did not say the budget");
}

/// BACKEND-7: the model a `/agent` line's definition names answers the turn, so the turn
/// compacts against that model's window. The session's own budget is not moved, because the
/// next turn on the session's model has to compare with its own.
#[test]
fn a_turn_addressed_to_a_definition_naming_a_model_takes_that_models_window() {
    let config = a_config_with_a_named_roster();

    let turn = config_for_turn(&config, Some("opus-arn"));

    assert_eq!(
        turn.context_budget,
        bravebot_config::bedrock::CONTEXT_WINDOW
    );
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET,
        "the session's own budget moved"
    );
    assert_eq!(
        config_for_turn(&config, None).context_budget,
        config.context_budget,
        "a definition naming no model leaves the session's budget"
    );
}

/// The plan model's window, which a settings file states for it, so that no request is made and
/// the window is told apart from the default budget a session holds until it adopts one.
const A_PLAN_MODELS_WINDOW: u64 = 65_536;

const A_PLAN_MODEL: &str = "ollama/tiny";

/// A configuration whose plan model is served by a gateway its settings file states a window for.
fn a_config_naming_a_plan_model_with_a_window() -> Config {
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(&format!(
        r#"{{"provider": {{"ollama": {{
                "options": {{"baseURL": "http://localhost:11434/v1", "contextWindow": {A_PLAN_MODELS_WINDOW}}},
                "models": {{"tiny": {{}}}}}}}}}}"#
    ))
    .providers()
    .to_vec();
    config.plan_model = Some(A_PLAN_MODEL.into());
    config
}

/// BACKEND-56, BACKEND-7: a turn that begins in plan mode answers on the plan model, so its
/// window is the budget; a turn that begins anywhere else, and a turn whose plan model the
/// agent sets aside, keep the session's. The fault it rejects: the window read for the plan
/// model in every mode, or for a name the turn then does not use.
#[test]
fn a_plan_turn_takes_the_plan_models_window_and_no_other_turn_does() {
    use bravebot_agent::{LiveMode, PermissionMode};

    let config = a_config_naming_a_plan_model_with_a_window();
    let egress = bravebot_net::Egress::new();
    let settled = |config: &Config, mode| {
        let mode = LiveMode::new(mode);
        settle_the_turn(config, &egress, Task::new("it"), &mode, None)
    };

    let (task, turn) = settled(&config, PermissionMode::Plan);
    assert_eq!(task.began_in, Some(PermissionMode::Plan));
    assert_eq!(
        turn.context_budget, A_PLAN_MODELS_WINDOW,
        "a turn that begins in plan mode"
    );
    for mode in [PermissionMode::Ask, PermissionMode::AcceptEdits] {
        let (task, turn) = settled(&config, mode);
        assert_eq!(task.began_in, Some(mode));
        assert_eq!(
            turn.context_budget, config.context_budget,
            "a turn that begins in {mode:?}"
        );
    }
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET,
        "the session's own budget moved"
    );

    let mut unset = config.clone();
    unset.plan_model = None;
    assert_eq!(
        settled(&unset, PermissionMode::Plan).1.context_budget,
        config.context_budget,
        "no plan model is named"
    );

    let refused =
        std::env::temp_dir().join(format!("bravebot-plan-window-{}.json", std::process::id()));
    std::fs::write(&refused, r#"{"models": {"deny": ["ollama/tiny"]}}"#).expect("a managed file");
    let mut setting_it_aside = config.clone();
    setting_it_aside.models = bravebot_config::Managed::at(&refused).models().clone();
    let _ = std::fs::remove_file(&refused);
    assert_eq!(
        settled(&setting_it_aside, PermissionMode::Plan)
            .1
            .context_budget,
        config.context_budget,
        "a plan model the turn sets aside is not the one whose window is read"
    );
}

/// BACKEND-56, MODE-8: the mode is read once, when the turn is settled, and the task carries
/// that reading while its handle stays live for the rest of the turn. The fault it rejects: the
/// window sized for one reading and the turn begun on another, where the key that sets the mode
/// is pressed between them.
#[test]
fn a_mode_pressed_after_a_turn_is_settled_leaves_it_in_the_mode_its_window_was_sized_for() {
    use bravebot_agent::{LiveMode, PermissionMode};

    let config = a_config_naming_a_plan_model_with_a_window();
    let egress = bravebot_net::Egress::new();
    let mode = LiveMode::new(PermissionMode::Plan);
    let task = Task::new("it").with_permission_mode(mode.clone());

    let (task, turn) = settle_the_turn(&config, &egress, task, &mode, None);
    mode.set(PermissionMode::AcceptEdits);

    assert_eq!(task.began_in, Some(PermissionMode::Plan));
    assert_eq!(
        task.permission_mode.get(),
        PermissionMode::AcceptEdits,
        "what the turn decides after it opens follows the key (MODE-8)"
    );
    assert_eq!(turn.context_budget, A_PLAN_MODELS_WINDOW);
}

/// BACKEND-56, DELEGATE-22: the model the window is read for is the one the agent settles on,
/// so a definition's model outranks the plan model and a turn in any other mode does not reach
/// for it. The fault it rejects: this crate deciding the order itself and differing from the
/// turn.
#[test]
fn the_model_a_window_is_read_for_follows_the_order_the_agent_settles() {
    use bravebot_agent::PermissionMode::{AcceptEdits, Ask, Plan};

    let mut config = a_config_naming_a_plan_model_with_a_window();
    config.plan_model = None;
    let egress = bravebot_net::Egress::new();
    let task = Task::new("it");
    let named = |config: &Config, mode, definition| {
        model_answering_the_turn(config, &egress, mode, &task, definition)
    };

    assert_eq!(named(&config, Plan, None), None, "no plan model is named");
    config.plan_model = Some(A_PLAN_MODEL.into());
    assert_eq!(named(&config, Plan, None).as_deref(), Some(A_PLAN_MODEL));
    for mode in [Ask, AcceptEdits] {
        assert_eq!(named(&config, mode, None), None, "{mode:?}");
    }
    assert_eq!(
        named(&config, Plan, Some("a-definition-model")).as_deref(),
        Some("a-definition-model"),
        "a definition's model outranks the plan model"
    );
    let flagged = Task::new("it").model_outranks_a_definition(true);
    assert_eq!(
        model_answering_the_turn(&config, &egress, Plan, &flagged, Some("a-definition-model")),
        None,
        "the command line's model outranks both, and is the session's"
    );
}

/// BACKEND-7: where the listing for the addressed model cannot be read, the turn takes the
/// default budget and not the session model's window, which describes a different model. The
/// regression it rejects is `config_for_turn` returning early with the session's window.
#[test]
fn a_turn_addressed_to_a_model_whose_listing_fails_does_not_keep_the_session_window() {
    use bravebot_config::env_var;
    let (endpoint, _asked) = a_listing_server("this is not a roster");
    let mut config = Config::from_lookup(|name| match name {
        env_var::ENDPOINT => Some(endpoint.clone()),
        env_var::KEY_ID => Some("a-key-id".into()),
        env_var::SIGNING_KEY => Some("a-signing-key".into()),
        _ => None,
    })
    .expect("a configuration");
    assert!(config.adopt_window(Some(131_072)));
    assert!(list_models(&config, Some("other-model")).is_err());

    let turn = config_for_turn(&config, Some("other-model"));

    assert_eq!(turn.context_budget, bravebot_config::DEFAULT_CONTEXT_BUDGET);
    assert_eq!(config.context_budget, 131_072);
}

/// BACKEND-11: the list `/model` opens marks the model in force, and picking a row puts that
/// model in force. Run with the model set by settings and nothing picked, with a pick, and where
/// a row is typed to and chosen. The regressions it rejects are the pick alone read as the model
/// in force, which marks Brave's `automatic` for a session on the settings model, a default that
/// outranks a pick, and a row chosen that is not put in force.
#[test]
fn the_list_the_model_command_opens_marks_the_model_in_force_and_a_row_picked_is_put_in_force() {
    use models::scripted_account::offer;
    let mut config = a_config_with_a_named_roster();
    config.default_model = "chatgpt/gpt-5.5".to_string();
    offer(vec![
        bravebot_aichat::models::Model::automatic(),
        listed("chatgpt/gpt-5.5", Some(65_536)),
    ]);

    let mut session = Session::new("none");
    assert_eq!(
        picker_opened(&mut session, &config).1,
        ["chatgpt/gpt-5.5"],
        "the settings model was not the one marked"
    );

    session.name_model("opus-arn".to_string());
    assert_eq!(
        picker_opened(&mut session, &config).1,
        ["opus-arn"],
        "the default outranked the pick"
    );

    let mut session = Session::new("none");
    open_model_picker(
        &mut session,
        &config,
        crate::model_prompt::Scope::EverySession,
    );
    session.settle_picking();
    for letter in "opus".chars() {
        session.press_in_picking(KeyCode::Char(letter), KeyModifiers::NONE);
    }
    session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
    let picked = session.take_pick().expect("nothing was picked");
    carry_out_pick(&mut session, &mut config, picked);
    assert_eq!(
        session.model(),
        Some("opus-arn"),
        "the row picked was not put in force"
    );
}

/// BACKEND-39: a start that nothing serves offers the rows the account listed, in the order the
/// picker lists them, and none where the service listed none, with the listing that gave none so the
/// start can say why. The compiled names that stand in for a listing in the picker are not offered,
/// nor is anything where the listing was refused, the sign-in file could not be read, the sign-in
/// ended while the listing was asked for or the listing was read and is empty. The regressions it
/// rejects are the service's order in place of the picker's, which would number a row differently
/// from the `/model` the person meets next, a question over names the account may refuse, and a
/// listing that gave none reported as another.
#[test]
fn a_start_is_offered_what_the_account_listed_in_the_pickers_order_and_nothing_else() {
    use bravebot_agent::chatgpt::AccountModels;
    use bravebot_aichat::responses::protocol::Refusal;
    use models::scripted_account::{
        fail_to_list, fail_to_read_the_file, nobody_signed_in, offer, refuse,
    };
    let config = a_config_with_a_named_roster();
    let keys = |config: &Config| -> Vec<String> {
        account_models_to_start_on(config)
            .unwrap_or_default()
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    offer(vec![
        listed("chatgpt/gpt-b", None),
        listed("chatgpt/gpt-c", None),
        listed("chatgpt/gpt-a", None),
    ]);
    assert_eq!(
        keys(&config),
        ["chatgpt/gpt-a", "chatgpt/gpt-b", "chatgpt/gpt-c"]
    );

    offer(Vec::new());
    assert_eq!(
        account_models_to_start_on(&config),
        Err(AccountModels::Offered(Vec::new())),
        "a listing with no row offered one, or was not said to be empty"
    );
    fail_to_list();
    assert!(
        matches!(
            account_models_to_start_on(&config),
            Err(AccountModels::Compiled(_))
        ),
        "the compiled names were offered for a listing that could not be had"
    );
    fail_to_read_the_file();
    assert_eq!(
        account_models_to_start_on(&config),
        Err(AccountModels::Unreadable),
        "an unreadable sign-in offered rows"
    );
    for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
        refuse(refusal);
        assert_eq!(
            account_models_to_start_on(&config),
            Err(AccountModels::Refused(refusal)),
            "{refusal:?} offered rows"
        );
    }
    nobody_signed_in();
    assert_eq!(
        account_models_to_start_on(&config),
        Err(AccountModels::Absent),
        "a sign-in that ended while the listing was asked for offered rows"
    );
}

/// BACKEND-39: a start that nothing serves offers the rows the xAI account listed, in the order the
/// picker lists them, and none where the service listed none, with the listing that gave none so the
/// start can say why. A listing that could not be had offers nothing, since no list built into this
/// program stands in for it, and an account nobody signed in to is not asked about. The regressions
/// it rejects are the service's order in place of the picker's, which would number a row differently
/// from the `/model` the person meets next, a roster read from the ChatGPT account's answer, and a
/// failed listing reported as an empty one.
#[test]
fn a_start_is_offered_what_the_xai_account_listed_in_the_pickers_order_and_nothing_else() {
    use bravebot_agent::xai::AccountModels;
    use models::scripted_xai::{fail_to_list, offer};
    let config = a_config_with_a_named_roster();
    let keys = |config: &Config| -> Vec<String> {
        xai_models_to_start_on(config)
            .unwrap_or_default()
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    offer(the_xai_accounts_listing().into_iter().rev().collect());
    assert_eq!(keys(&config), ["xai/grok-4.5", "xai/grok-build-0.1"]);

    offer(Vec::new());
    assert_eq!(
        xai_models_to_start_on(&config),
        Err(AccountModels::Offered(Vec::new())),
        "a listing with no row offered one, or was not said to be empty"
    );
    fail_to_list();
    assert_eq!(
        xai_models_to_start_on(&config),
        Err(AccountModels::Unlisted),
        "a listing that could not be had offered rows or was said to be empty"
    );

    // Nobody signed in: no script on this thread, so the account answers as an absent one does.
    let fresh = std::thread::spawn(|| xai_models_to_start_on(&a_config_with_a_named_roster()))
        .join()
        .expect("a start on a thread of its own");
    assert_eq!(fresh, Err(AccountModels::Absent));
}

/// BACKEND-48: a start does not offer a model this machine may not request, as the picker does
/// not, so a person is not refused after choosing it.
#[test]
fn a_start_is_not_offered_a_model_the_machine_level_layer_refuses() {
    use models::scripted_account::offer;
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join("tui-start-models-refused");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let path = scratch.join("managed.json");
    std::fs::write(&path, r#"{"models": {"deny": ["chatgpt/gpt-b"]}}"#).expect("a managed file");
    let mut config = a_config_with_a_named_roster();
    let keys = |config: &Config| -> Vec<String> {
        account_models_to_start_on(config)
            .unwrap_or_default()
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    offer(vec![
        listed("chatgpt/gpt-a", None),
        listed("chatgpt/gpt-b", None),
    ]);
    assert_eq!(keys(&config), ["chatgpt/gpt-a", "chatgpt/gpt-b"]);

    config.models = bravebot_config::Managed::at(&path).models().clone();
    assert_eq!(keys(&config), ["chatgpt/gpt-a"]);
}

/// BACKEND-48: a start does not offer a model of the xAI account this machine may not request, as
/// the picker does not, so a person is not refused after choosing it. The regression it rejects is
/// the xAI rows taken from the listing without the machine-level layer's `models.deny`, which offers
/// a row that is refused once picked.
#[test]
fn a_start_is_not_offered_an_xai_model_the_machine_level_layer_refuses() {
    use models::scripted_xai::offer;
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join("tui-start-xai-models-refused");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let path = scratch.join("managed.json");
    std::fs::write(&path, r#"{"models": {"deny": ["xai/grok-4.5"]}}"#).expect("a managed file");
    let mut config = a_config_with_a_named_roster();
    let keys = |config: &Config| -> Vec<String> {
        xai_models_to_start_on(config)
            .unwrap_or_default()
            .into_iter()
            .map(|model| model.key)
            .collect()
    };

    offer(the_xai_accounts_listing());
    assert_eq!(keys(&config), ["xai/grok-4.5", "xai/grok-build-0.1"]);

    offer(the_xai_accounts_listing());
    config.models = bravebot_config::Managed::at(&path).models().clone();
    assert_eq!(keys(&config), ["xai/grok-build-0.1"]);

    // Where the layer refuses every row the account offers, the start is told the listing gave
    // none to choose from.
    std::fs::write(
        &path,
        r#"{"models": {"deny": ["xai/grok-4.5", "xai/grok-build-0.1"]}}"#,
    )
    .expect("a managed file");
    offer(the_xai_accounts_listing());
    config.models = bravebot_config::Managed::at(&path).models().clone();
    assert_eq!(
        xai_models_to_start_on(&config),
        Err(bravebot_agent::xai::AccountModels::Offered(Vec::new()))
    );
}

/// R20, R3, R4. A model picked for this session only is the one the next turn is asked for,
/// over a recorded pick and over a settings file's key, and records nothing: the file still
/// holds the earlier pick, and a second session opens on it. The faults it rejects are a pick
/// that goes through `choose_model` and so rewrites the file, a settings key or a recorded pick
/// that is read again after the pick, and a task built from anything but the session's model.
#[test]
fn a_session_only_pick_is_asked_for_over_every_other_source_and_the_next_session_ignores_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let config = a_config_needing_no_sign_in();
    let settings = bravebot_config::Settings::parse(r#"{"model": "the-files-model"}"#);
    bravebot_session::store::save_model("the-recorded-pick");

    let mut session = Session::new("none").with_stored_history();
    session.adopt_model(&settings, &config);
    assert_eq!(session.model(), Some("the-recorded-pick"));

    session.choose_model_for_this_session("only-this-time");

    assert_eq!(session.model(), Some("only-this-time"));
    let run_settings = RunSettings::from_settings(&settings);
    let task = run_settings.task("it", &session, &Permissions::new());
    assert_eq!(
        task.model.as_deref(),
        Some("only-this-time"),
        "the turn was not asked for the session-only pick"
    );
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some("the-recorded-pick"),
        "the session-only pick was recorded"
    );

    let mut next = Session::new("none").with_stored_history();
    next.adopt_model(&settings, &config);
    assert_eq!(
        next.model(),
        Some("the-recorded-pick"),
        "the next session did not open on the model it would have had"
    );
    assert!(!next.model_is_for_this_session());
}

/// R21. A session-only pick ranks as a recorded one does against the models that outrank the
/// session's: `planModel` in plan mode and the model a definition names. The fault it rejects
/// is a session-only pick that is given the command line's rank, so it answers a plan turn
/// or a definition's turn.
#[test]
fn a_session_only_pick_is_outranked_by_the_plan_model_and_a_definitions_as_a_recorded_one_is() {
    use bravebot_agent::PermissionMode::{AcceptEdits, Ask, Plan};

    let config = a_config_naming_a_plan_model_with_a_window();
    let egress = bravebot_net::Egress::new();
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    let permissions = Permissions::new();

    for every_session in [true, false] {
        let mut session = Session::new("none");
        if every_session {
            session.choose_model("a-picked-model", &config);
        } else {
            session.choose_model_for_this_session("a-picked-model");
        }
        let task = run_settings.task("it", &session, &permissions);
        assert_eq!(task.model.as_deref(), Some("a-picked-model"));
        let answering =
            |mode, definition| model_answering_the_turn(&config, &egress, mode, &task, definition);

        assert_eq!(
            answering(Plan, None).as_deref(),
            Some(A_PLAN_MODEL),
            "the pick answered a plan turn (every session: {every_session})"
        );
        for mode in [Ask, AcceptEdits] {
            assert_eq!(
                answering(mode, None),
                None,
                "{mode:?} (every session: {every_session})"
            );
        }
        for mode in [Plan, Ask] {
            assert_eq!(
                answering(mode, Some("a-definition-model")).as_deref(),
                Some("a-definition-model"),
                "{mode:?} (every session: {every_session})"
            );
        }
    }
}

/// R22, R4. A record written by a session that held a session-only pick does not hold it, and
/// a session resumed from it opens on the model a new session opens on: the recorded pick.
/// The faults it rejects are a record that is written with the model in force, and a resume
/// that puts a model back from the record.
#[test]
fn a_resume_after_a_session_only_pick_opens_on_the_recorded_pick() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-model-session-resumed");
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let config = a_config_needing_no_sign_in();
    let settings = bravebot_config::Settings::default();
    bravebot_session::store::save_model("the-recorded-pick");

    let mut session = Session::new("none").with_stored_history();
    session.adopt_model(&settings, &config);
    session.choose_model_for_this_session("only-this-time");
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    assert_eq!(record.model, None, "the record holds the session-only pick");

    // The session that wrote the record has ended, so nothing holds it.
    drop(stored);
    let mut resumed = Session::new("none").with_stored_history();
    resumed.adopt_model(&settings, &config);
    restore_resumed(&mut resumed, &config, &mut workspace, &record).expect("the record is free");

    assert_eq!(resumed.model(), Some("the-recorded-pick"));
    assert!(!resumed.model_is_for_this_session());
    assert_eq!(resumed.served_model(), None);
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some("the-recorded-pick")
    );
}

/// BACKEND-48 as `/advisor` meets it: a model this machine may not request is not taken, and
/// the refusal names the model and the file that refused it. Taking it would offer the planner
/// a tool whose every call fails.
#[test]
fn an_advisor_this_machine_may_not_request_is_refused() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join("tui-models-advisor");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let path = scratch.join("managed.json");
    std::fs::write(&path, r#"{"models": {"deny": ["an-expensive-model"]}}"#)
        .expect("a managed file");
    let mut config = a_config_needing_no_sign_in();
    config.models = bravebot_config::Managed::at(&path).models().clone();
    let mut session = Session::new("none");

    set_advisor(&mut session, &config, "an-expensive-model");

    assert_eq!(session.advisor(), None, "a refused model was taken");
    let said = session
        .transcript
        .last()
        .map(|entry| entry.text.clone())
        .unwrap_or_default();
    assert!(
        said.contains("an-expensive-model") && said.contains(&path.display().to_string()),
        "the refusal named neither the model nor the file: {said}"
    );
}

/// The inputs the roster lists for the model in force are taken for the session, which is the
/// default where nothing is picked: a picture put to a model the listing says takes text only is
/// held to that. Taken for the pick alone, a session on the settings model carried none, and a
/// picture went to a model the roster had said does not read one.
#[test]
fn the_inputs_the_roster_lists_are_those_of_the_model_in_force_which_is_the_default_unpicked() {
    use models::scripted_account::offer;
    let mut text_only = listed("chatgpt/text-only", None);
    text_only.advertised.input_modalities = vec!["text".to_string()];
    let mut config = a_config_with_a_named_roster();
    config.default_model = "chatgpt/text-only".to_string();
    offer(vec![text_only, listed("chatgpt/other", None)]);

    let mut session = Session::new("none");
    adopt_budget_for_current_model(&mut session, &mut config);

    assert_eq!(
        config.listed_inputs("chatgpt/text-only"),
        ["text".to_string()],
        "the default's inputs were not taken"
    );
}

/// BACKEND-36, RETRIEVE-23: the notes said when a terminal session opens name each key the
/// settings dropped or combined, once, in the sentences `doctor` uses, with no value. The unread
/// key, a key a settings file wrote twice, a key the managed file wrote twice and the managed
/// `retrieval` value that pins nothing are four facts, so a start that says three fails.
///
/// Rejects a start that leaves these to `doctor`, one that says a key once per layer, one that
/// prints what a key was set to, and a sentence that differs from `doctor`'s. The update check
/// is switched off by the file itself so the start asks nothing of a network.
#[test]
fn the_startup_notes_name_each_dropped_key_once_in_doctors_sentences_and_no_value() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let directory = tempfile::tempdir().expect("a scratch directory");
    let settings_file = directory.path().join("settings.json");
    std::fs::write(
        &settings_file,
        r#"{"updateCheck": false,
                "permissions": {"denyy": ["Read(a-rule-nobody-prints)"], "deny": [], "deny": []},
                "statusLine": "a-command-nobody-prints"}"#,
    )
    .expect("write the settings file");
    let settings = bravebot_config::Settings::layered(None, None, Some(&settings_file));
    let managed_file = directory.path().join("managed.json");
    std::fs::write(
        &managed_file,
        r#"{"retrieval": "false", "models": {"deny": ["a-model-nobody-prints"]}, "models": {}}"#,
    )
    .expect("write the managed file");
    let _machine = bravebot_config::managed_file_on_this_thread(&managed_file);
    let mut session = Session::new("none");

    say_startup_notes(
        &mut session,
        &a_config_needing_no_sign_in(),
        &settings,
        Vec::new(),
    );

    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    for sentence in bravebot_agent::dropped_keys::at_session_start_of_this_machine(
        &settings,
        bravebot_agent::dropped_keys::Spelling::Drawn,
    ) {
        assert_eq!(
            said.iter().filter(|line| **line == sentence).count(),
            1,
            "the sentence doctor says is not said once: {sentence}\n{said:?}"
        );
    }
    let named = |needle: &str| said.iter().filter(|line| line.contains(needle)).count();
    assert_eq!(named("permissions.denyy in "), 1, "{said:?}");
    assert_eq!(named("statusLine in "), 1, "{said:?}");
    assert_eq!(
        named("permissions.deny is written more than once in "),
        1,
        "{said:?}"
    );
    assert_eq!(named("models is written more than once in "), 1, "{said:?}");
    assert_eq!(named("retrieval in "), 1, "{said:?}");
    for value in [
        "a-rule-nobody-prints",
        "a-command-nobody-prints",
        "a-model-nobody-prints",
    ] {
        assert!(
            said.iter().all(|line| !line.contains(value)),
            "a value is said: {said:?}"
        );
    }
}

/// Settings whose keys are all read add no note about dropped keys. Rejects a start that says a
/// line for each file.
#[test]
fn the_startup_notes_say_nothing_about_dropped_keys_where_every_key_was_read() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let settings = bravebot_config::Settings::parse(
        r#"{"updateCheck": false, "permissions": {"deny": ["Read(.env)"]}}"#,
    );
    let mut session = Session::new("none");
    let _machine = bravebot_config::managed_file_on_this_thread(std::path::Path::new(
        "no-managed-file-is-here.json",
    ));

    say_startup_notes(
        &mut session,
        &a_config_needing_no_sign_in(),
        &settings,
        Vec::new(),
    );

    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        said.iter()
            .all(|line| !line.contains("not read by this build")
                && !line.contains("written more than once")),
        "{said:?}"
    );
}

/// BACKEND-48: a definition naming a model this machine may not request is refused where the
/// definition is read, so nothing is addressed to it and nothing is started for it.
///
/// A delegate definition is the one route to a model that no picker and no prompt stands in
/// front of: it is loaded from a vouched-for file, and a build that only checked the model a
/// session settles on would let this one through.
#[test]
fn a_definition_naming_a_model_this_machine_may_not_request_is_refused() {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join("tui-models-delegate");
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let path = scratch.join("managed.json");
    std::fs::write(&path, r#"{"models": {"deny": ["an-expensive-model"]}}"#)
        .expect("a managed file");

    let definitions = a_resolved_set(Some("an-expensive-model"));
    let mut config = a_config_needing_no_sign_in();

    assert!(
        definition_named(&config, &definitions, "rule-reviewer").is_ok(),
        "a machine with no managed file refused a definition"
    );

    config.models = bravebot_config::Managed::at(&path).models().clone();
    let refused = definition_named(&config, &definitions, "rule-reviewer")
        .expect_err("the definition was addressed")
        .into_message();
    assert!(
        refused.contains("an-expensive-model") && refused.contains(&path.display().to_string()),
        "the refusal named neither the model nor the file that refused it: {refused}"
    );
}

/// A cache figure measures one request a process sent, so a session resumed in another
/// reports none until a turn has run in it: a figure a rewind point carried across the resume
/// would arrive on the panel at the first `/undo`, beside a cost this session has not paid.
/// A rewind inside the process that measured the figure still puts it back, which is the half
/// of BACKEND-31 that keeping none in the record must not cost.
#[test]
fn a_resumed_rewind_reports_no_cache_figure_while_a_live_one_puts_it_back() {
    use bravebot_aichat::protocol::{Cached, Message};
    use bravebot_session::sessions::{self, Standing};

    fn save(
        stored: &mut sessions::Handle,
        session: &Session,
        conversation: &Conversation,
        root: &std::path::Path,
    ) {
        stored.save(
            "cache",
            Standing {
                conversation: &conversation.snapshot(),
                history: Some(session.turn_history()),
                turns: session.turns,
                tokens: session.tokens,
                split: Default::default(),
                spend: session.spend_by_turn(),
                timing: session.timing_by_turn(),
                model: None,
                todos: &session.todos_by_turn(),
                asides: &[],
                trust: &TrustStore::new(root),
                programs: &TrustedPrograms::new(),
                directories: &[],
                manifest: None,
                rewind: session.rewind.points(),
                checkouts: &[],
                touched: None,
                answered_by_mode: false,
            },
        );
    }

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-cache");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    let mut conversation = Conversation::new();

    // Two turns whose cache figures differ, so a figure put back can be told from the one it
    // replaced as well as from nothing at all.
    let figures = [
        Cached {
            read_tokens: 400,
            written_tokens: 50,
            stated: true,
        },
        Cached {
            read_tokens: 900,
            written_tokens: 100,
            stated: true,
        },
    ];
    for (index, cached) in figures.into_iter().enumerate() {
        let prompt = format!("say {index}");
        let start = conversation.recounted().len();
        type_line(&mut session, &prompt);
        session.submit().expect("the prompt is sent");
        let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
        session.rewind.open(point, prompt.clone());
        session.prompt_recorded(conversation.recounted().len());
        conversation.push(Message::user(&prompt));
        conversation.push(Message::assistant("said"));
        session.complete("said", vec![], 10);
        // What [`fold_outcome`] does on the path a finished turn takes.
        session.served_from_cache(cached, 1_000);
        session.record_turn(start, &conversation);
        save(&mut stored, &session, &conversation, &root);
    }

    assert_eq!(
        session.cached(),
        Some(figures[1]),
        "the panel is not reporting the last turn's figure"
    );
    assert_eq!(
        session
            .rewind
            .points()
            .last()
            .map(|point| point.snapshot.cached),
        Some(Some(figures[0])),
        "a live point does not hold what the turn before its own read, so a rewind in this \
             process has nothing to put back"
    );
    assert_eq!(
        session
            .rewind
            .points()
            .last()
            .and_then(|point| point.snapshot.cached_prompt_tokens),
        Some(1_000),
        "a live point does not hold the prompt total behind its cache figure"
    );

    // Read before the live rewind, which rewrites it: the resumed half below wants the record
    // as the two turns left it.
    let record = sessions::load(&root, stored.id()).expect("the record");

    // A rewind to a point this process made puts the figure back, the panel reporting what the
    // turn that is now the last one read.
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );
    assert_eq!(
        session.turns, 1,
        "the live rewind did not go back, so the figure it reports says nothing"
    );
    assert_eq!(
        session.cached(),
        Some(figures[0]),
        "a rewind in the process that measured the figures left the rewound turn's on the panel"
    );
    assert_eq!(
        session.cached_prompt_tokens(),
        Some(1_000),
        "a rewind in the process did not restore the preceding prompt total"
    );

    // A resume brings the points back without the figures, since the record keeps none.
    let conversation = Conversation::restored(record.conversation.clone());
    let mut resumed = Session::new("none");
    resumed.replay(
        &conversation,
        &record.title,
        &sessions::recall(&root, &record),
    );
    resumed.restore_spend(record.tokens, record.spend.clone());
    resumed.restore_rewind_points(record.rewind_points(&root), &conversation);
    let mut conversation = conversation;
    assert_eq!(
        resumed.cached(),
        None,
        "the resumed session reported a cache before any turn had run in it"
    );
    assert_eq!(
        resumed.cached_prompt_tokens(),
        None,
        "the resumed session reported a cache rate denominator"
    );

    rewind(
        &mut resumed,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );

    assert_eq!(
        resumed.turns, 1,
        "the rewind did not go back, so the figure it reports says nothing"
    );
    assert_eq!(
        resumed.cached(),
        None,
        "a rewind in a resumed session put back a cache figure the previous process measured"
    );
}

/// BACKEND-36, RETRIEVE-23: a session the terminal begins says what the managed layer the machine
/// holds dropped: the key the managed file wrote twice and the `retrieval` value that pins
/// nothing, once each, in the sentences `doctor` uses, and no value.
///
/// Rejects a start whose notes leave the machine's managed layer out, which leaves every managed
/// key to `doctor`. The note tests above put a layer in front of `say_startup_notes` and pass on
/// such a start, so only a test that begins the session reaches this.
#[test]
fn a_session_begun_says_what_its_managed_layer_dropped() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-begin-managed-notes");
    std::fs::create_dir_all(&root).expect("create");
    let managed_file = root.join("managed.json");
    std::fs::write(
        &managed_file,
        r#"{"retrieval": "false", "models": {"deny": ["a-model-nobody-prints"]}, "models": {}}"#,
    )
    .expect("write the managed file");
    let _machine = bravebot_config::managed_file_on_this_thread(&managed_file);
    let expected = bravebot_agent::dropped_keys::at_session_start_of_this_machine(
        &bravebot_config::Settings::default(),
        bravebot_agent::dropped_keys::Spelling::Drawn,
    );
    assert_eq!(
        expected.len(),
        2,
        "the fixture drops a repeated key and a retrieval value: {expected:?}"
    );
    let workspace = Workspace::new(&root).expect("a workspace");
    // Never drawn on: a session opened bypassing every permission is not asked about the
    // directory it opens in.
    let mut terminal = ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(std::io::stdout()),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 20, 5)),
        },
    )
    .expect("a terminal");
    let mut config = a_config_needing_no_sign_in();
    let mut servers = crate::state::Servers::default();

    let begun = begin_session(
        &mut terminal,
        &mut config,
        &workspace,
        &mut servers,
        Startup {
            confinement: String::new(),
            sockets_with_a_path_reachable: false,
            sandbox: crate::sandbox_command::Sandboxed::at_start(),
            start: Start::Fresh,
            named: None,
            links: Links::default(),
            skip_permissions: true,
            asked_for_auto: false,
            prompts: bravebot_agent::turn::SystemPrompts::default(),
        },
    )
    .unwrap_or_else(|ended| panic!("the session did not begin: {ended:?}"));

    let said: Vec<&str> = begun
        .session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    for sentence in &expected {
        assert_eq!(
            said.iter().filter(|line| *line == sentence).count(),
            1,
            "the sentence doctor says is not said once: {sentence}\n{said:?}"
        );
    }
    assert!(
        said.iter()
            .all(|line| !line.contains("a-model-nobody-prints")),
        "a value is said: {said:?}"
    );
}

/// `/clear` begins a session nobody has asked anything. A program vouched for, a server
/// approved, a run prompt drawn and a file agreed to be shown were answers about the session it
/// ends, and kept they would answer the next one's questions for it.
#[test]
fn clearing_forgets_every_answer_the_session_it_ends_gave() {
    let root = crate::testutil::scratch_dir("bravebot-app-clear-answers");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create");
    let mut session = Session::new("none");
    let rules = starting_rules(&mut session, &root.join("state"), &root);
    let mut answers = Answers::opening(TrustStore::new(&root), TrustedPrograms::new(), rules);
    give_answers(&mut answers, &root);

    answers.forget_what_was_agreed();

    assert_eq!(
        answers.programs,
        TrustedPrograms::new(),
        "the new session kept a program vouched for"
    );
    assert!(
        answers.servers.is_none(),
        "the new session kept the servers approved"
    );
    assert_eq!(
        answers.asked_about,
        AskedAbout::new(),
        "the new session kept the run prompts drawn"
    );
    assert!(
        !answers.exposed.holds(".env"),
        "the new session kept a file agreed to be shown"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// BACKEND-37: the transcript line of a rejected request names the model the person configured,
/// not the name the request carried to the service (which a gateway changes) and not any text
/// of the service's. The configured name holds a control character, which is written out in the
/// line, the status area and the export.
///
/// The regressions it rejects are the wire name in place of the configured one, a line that
/// leaves the model out, and the control character reaching the screen.
#[test]
fn a_rejected_request_is_said_with_the_configured_model_in_the_transcript() {
    let mut session = Session::new("none");
    type_line(&mut session, "read a file");
    session.submit().unwrap();
    let error = bravebot_agent::backend::BackendError::Attempted {
        usage: None,
        context_tokens: None,
        conceded: Default::default(),
        backend: None,
        retry_setting: bravebot_agent::RetrySetting::None,
        idle_gap: None,
        attempts: 1,
        cause: Box::new(bravebot_agent::backend::BackendError::from(
            bravebot_aichat::ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://example.test/PRIVATE_PATH".into(),
                status: 400,
                retry_after: None,
                body: None,
            }),
        )),
    };
    fold_outcome(
        &mut session,
        Err(turn::TurnError::from(error)),
        Trail::new(),
        Carried {
            asked: Default::default(),
            exposed: Default::default(),
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
        },
        Occupied {
            budget: 1000,
            guessed: false,
            last_request_tokens: 0,
        },
        Asked {
            name: "gateway/wire-name".into(),
            comparable: false,
            configured: "my-model\u{1b}[2J".to_string(),
        },
        Line {
            text: "read a file",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace_for_test(),
    );
    let reason = session.failure_said().expect("visible reason").to_string();
    assert!(reason.contains("(HTTP 400)"), "{reason}");
    assert!(
        reason.contains("using the model my-model\u{241b}[2J"),
        "{reason}"
    );
    assert!(!reason.contains("wire-name"), "{reason}");
    assert!(!reason.chars().any(char::is_control), "{reason:?}");
    let exported = render::as_markdown(&session, "test");
    assert!(!exported.contains('\u{1b}'), "{exported:?}");
    assert!(!exported.contains("PRIVATE_PATH"), "{exported}");
}

/// A session starts with suggestions off where a checkout's settings file turned them on and
/// the person's own did not (BACKEND-24), over the person's `false` and over nothing, and
/// stays on where the person's own file turned them on.
#[test]
fn a_checkouts_suggestions_do_not_turn_the_session_on() {
    let root = crate::testutil::scratch_dir("bravebot-app-checkout-suggestions");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&home).expect("scratch home");
    std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
    std::fs::create_dir_all(cwd.join(".git")).expect("scratch checkout");
    std::fs::write(
        cwd.join(".bravebot").join("settings.json"),
        r#"{"suggestions": true}"#,
    )
    .expect("write the project layer");
    let suggests = |home_text: &str| {
        std::fs::write(home.join("settings.json"), home_text).expect("write the home layer");
        let settings = bravebot_config::Settings::layered(Some(home.clone()), Some(&cwd), None);
        let mut session = Session::new("none");
        adopt_settings(&mut session, &settings, None);
        session.suggests()
    };

    assert!(
        !suggests(r#"{"suggestions": false}"#),
        "over the person's false"
    );
    assert!(!suggests("{}"), "over nothing");
    assert!(
        suggests(r#"{"suggestions": true}"#),
        "the person's own true"
    );
}
