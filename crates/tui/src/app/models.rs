//! What the endpoint offers, and what a session adopts from a model: its window, its inputs, whether
//! it reads effort or structured output.

use super::*;

/// Ask the endpoint what it offers.
///
/// Shared by the picker and by the budget lookup a session does when it starts, because both want
/// the same listing and a second copy of the policy setup would be a second place for the gate to
/// be got wrong.
pub(super) fn list_models(
    config: &Config,
    chosen: Option<&str>,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    // Bedrock is additive. Its tiers come from configuration rather than from a listing, since it has
    // no listing endpoint and an ARN does not say which model it resolves to, but the Brave roster is
    // what everyone has and a settings block adds to it rather than replacing it.
    //
    // The configured tiers come first: someone who named them went out of their way to, and the
    // alternative buries them under a roster they did not ask about.
    let mut configured = config
        .bedrock
        .as_ref()
        .map(bedrock_models)
        .unwrap_or_default();

    // Additive on the same terms. A block that named its models is taken at its word and costs no
    // round trip, which is what keeps a configured gateway working offline. One that named none is
    // asked, because the alternative is a gateway configured exactly as the tool this block's shape
    // came from configures it, offering nothing. A service with no listing to ask is offered the list
    // compiled in instead.
    for provider in &config.providers {
        // An entry naming AWS reaches the Bedrock backend, so its rows are built the way that
        // backend's are: the models the block named, and nothing fetched. There is no listing
        // endpoint to ask, which is why a block that named none offers none rather than being
        // asked what it serves.
        if let Some(bedrock) = provider.bedrock.as_ref() {
            configured.extend(bedrock_models(bedrock));
            continue;
        }
        match provider.models.is_empty() {
            false => configured.extend(provider_models(provider)),
            true => configured.extend(in_reading_order(
                fetch_gateway_models(provider).unwrap_or_default(),
                chosen.unwrap_or(&config.default_model),
            )),
        }
    }

    // A build from source pointed only at Bedrock has blank Brave credentials, and asking with them
    // would list a roster whose every request then fails unsigned.
    if !config.serves_aichat() {
        return Ok(requestable(config, configured));
    }

    combined(configured, fetch_models(config)).map(|models| requestable(config, models))
}

/// The roster without the models the machine-level layer refuses (BACKEND-48).
///
/// Here rather than at the moment a pick is recorded, because refusing after somebody has read a row
/// and chosen it is a worse answer than not offering the row: the choice is theirs to make off a list,
/// and a list holding a name that cannot be asked for is a list that misdescribes the machine.
///
/// The names are compared as a request would carry them, which is what a roster row holds. A tier the
/// configured account named is listed by its own name rather than by the word, so nothing here needs
/// the tier resolution [`Config::model_named`] does.
pub(super) fn requestable(
    config: &Config,
    models: Vec<bravebot_aichat::models::Model>,
) -> Vec<bravebot_aichat::models::Model> {
    if config.models.is_empty() {
        return models;
    }
    models
        .into_iter()
        .filter(|model| config.model_refused(&model.key).is_none())
        .collect()
}

/// The tiers and the listing as one roster.
///
/// Split from the request so the joining is testable without a server. A listing nobody could fetch
/// is not worth losing the tiers over: they need no network to know, and a picker that refused
/// everything because one half was unreachable would leave the working half unpickable.
pub(super) fn combined(
    configured: Vec<bravebot_aichat::models::Model>,
    listed: Result<Vec<bravebot_aichat::models::Model>, String>,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    match listed {
        Ok(listed) => Ok(configured.into_iter().chain(listed).collect()),
        Err(_) if !configured.is_empty() => Ok(configured),
        Err(problem) => Err(problem),
    }
}

/// A fetched gateway roster in the order a person should meet it: what a session would use, then
/// everything else by name.
///
/// The gateway's own order is roughly newest-first, which puts a model somebody has never heard of at
/// the top and buries the one they work with. A configured roster needs none of this: the file is
/// already the order they chose.
///
/// Alphabetical by the key, so the provider id groups the rows and a name somebody half-remembers is
/// where they would look for it. Sorted rather than capped, because a picker filters as it is typed
/// and dropping rows would decide somebody may not choose a model their gateway serves.
pub(super) fn in_reading_order(
    mut models: Vec<bravebot_aichat::models::Model>,
    chosen: &str,
) -> Vec<bravebot_aichat::models::Model> {
    models.sort_by(|left, right| {
        let ranked = |model: &bravebot_aichat::models::Model| model.key != chosen;
        ranked(left)
            .cmp(&ranked(right))
            .then_with(|| left.key.cmp(&right.key))
    });
    models
}

/// The models a Bedrock configuration offers, strongest tier first.
///
/// One entry per tier that names a model, and nothing else. A tier whose variable is unset is one
/// this configuration cannot reach: an ARN cannot be derived from a model name, so an entry invented
/// for it would be a choice that fails at the far end for a reason nothing here could explain.
///
/// No `automatic` among them. There it means "let the server choose", which Bedrock does not offer: a
/// request names one model and gets it or an error. The entry still reaches the picker, from the Brave
/// half of the roster, where it is a choice that backend can honour.
///
/// Every entry is marked free. Premium here means a Leo subscription, and reaching a model through
/// somebody's own AWS account does not involve one.
///
/// Every entry says whose account answers. Both rosters are offered together and a bare tier name
/// would sit beside a Brave entry for the same model, where the two are reached and billed
/// differently and nothing else would say which was about to answer.
///
/// Not "Bedrock" alone: the Brave roster already says that of models it serves through its own AWS
/// account, so the word distinguishes nothing. The profile is the useful thing, being what decides
/// which credentials sign the request, and the account is all that can be said without one.
pub(super) fn bedrock_models(
    bedrock: &bravebot_config::bedrock::Bedrock,
) -> Vec<bravebot_aichat::models::Model> {
    bedrock
        .models()
        .iter()
        .map(|entry| bravebot_aichat::models::Model {
            key: entry.id.clone(),
            // The tier word where a tier named it, because the account it is reached through is the
            // same for all of them and is said once, over the section these rows sit in. A model a
            // `provider` block named has no tier, and an inference-profile ARN is not a name
            // anybody reads, so what that block called it is what a row says.
            display_name: entry.display_name().to_string(),
            premium: false,
            provider: Some(match bedrock.profile.as_deref() {
                Some(profile) => t!(picker_service_bedrock_profile, profile = profile),
                None => t!(picker_service_bedrock).to_string(),
            }),
            // The figure a tier gets is a property of what an opaque profile ARN gets rather than of
            // a particular model. A block that stated one knew better.
            conversation_tokens: Some(entry.window()),
            // True until this model has refused the field. Bedrock has no listing to say which
            // parameters a model takes, so the only way to find out is to have sent one, and a row
            // that went on claiming otherwise would report a level as in force after the requests
            // carrying it had stopped.
            reads_effort: bravebot_agent::backend::bedrock_reads_effort(&entry.id),
            // Bedrock has no listing, so nothing has described these models to us.
            advertised: bravebot_aichat::models::Advertised::default(),
        })
        .collect()
}

/// The models a gateway was configured to offer, in the order the file listed them.
///
/// One entry per named model, and nothing else. A gateway's own roster runs to hundreds of models
/// across upstreams most people will never use, so asking it would produce a listing nobody could
/// pick from, and asking it at all would cost a round trip on a path that must still work offline.
///
/// Every entry is marked free. Premium means a Leo subscription, and a gateway reached with somebody's
/// own bearer token does not involve one. What it costs them is between them and the gateway.
///
/// Every entry says which service answers it, because the same slug may be reachable more than one
/// way: `anthropic/claude-sonnet-4.5` through a gateway and Brave's own Sonnet are different bills
/// and different credentials, and nothing else about the model would say which was about to answer.
pub(super) fn provider_models(
    provider: &bravebot_config::provider::Provider,
) -> Vec<bravebot_aichat::models::Model> {
    provider
        .models
        .iter()
        .map(|model| bravebot_aichat::models::Model {
            // Qualified by the provider's own id, because the key is what a choice is remembered as
            // and what later selects a backend. The same slug may be reachable through more than one
            // service, and the bare name says nothing about which was picked.
            key: format!("{}/{}", provider.id, model.id),
            // The slug whole, since it is what a request names, and unqualified, since the gateway
            // is said over the section rather than on every row under it.
            display_name: model.id.clone(),
            premium: false,
            provider: Some(provider.display_name().to_string()),
            // Stated or assumed, never absent: a window is what the budget is taken from, and
            // reporting nothing would leave the session on a default chosen for a different service.
            conversation_tokens: Some(model.window()),
            // A block names models and never their parameters, so the level goes out to be judged
            // at the far end and the judgment is the only description there is: read one where
            // this service has already refused the field for this model.
            reads_effort: bravebot_aichat::reads_effort(
                &provider.chat_completions_url(),
                &model.id,
            ),
            // A block names models and describes none of them, so there is nothing to carry.
            advertised: bravebot_aichat::models::Advertised::default(),
        })
        .collect()
}

/// Take the budget for the model already in force, without asking anyone to choose it again.
///
/// A model chosen in an earlier session is read back off disk, and until this ran the window that
/// came with it was not: the budget stayed at the default and a session with room for a hundred
/// thousand tokens compacted at twenty-four, having said nothing about why.
///
/// A listing that cannot be fetched is not worth a word. The budget falls back to the default, which
/// is what it was before this existed, and a session that is merely offline should not open with a
/// complaint about a request nobody asked for.
pub(super) fn adopt_budget_for_current_model(session: &mut Session, config: &mut Config) {
    let Ok(models) = list_models(config, session.model()) else {
        return;
    };
    if config.adopt_window(advertised_window(&models, session.model())) {
        session.note(t!(session_context_budget, budget = config.context_budget));
    }
    let in_force = session.model().unwrap_or(&config.default_model).to_string();
    config.adopt_inputs(&in_force, advertised_inputs(&models, session.model()));
    // Outside the note, because a budget that did not move can still have stopped being one the
    // endpoint advertised: nothing changed for compaction, and what the hint line may claim did.
    session.update_budget(config.context_budget, config.budget_is_guessed());
    // The model in force, not the pick: with no pick the configured default is what a request names,
    // and it is the roster row for that one which says whether a level is read (BACKEND-22).
    session.note_model_reads_effort(reads_effort_in_force(
        &models,
        session.model(),
        &config.default_model,
    ));
}

/// Take on a level the service refused while a turn was running.
///
/// [`adopt_budget_for_current_model`] asks the listing once, at startup, and [`choose_model`] asks
/// it again; between the two there was nothing. A service that refuses the field answers the same
/// question the listing answers, and it answers it mid-turn, so a session that never asked again
/// went on reporting a level as in force for the rest of its life after the requests carrying one
/// had stopped (BACKEND-22).
///
/// Only ever subtracts, which is why it takes the refusal rather than the whole answer: a roster
/// row that states its parameters and does not name the field has already said this model reads no
/// level, and a turn that learned nothing is not the roster taking that back.
///
/// Said out loud as well, for the reason `/model` says it when a picked model reads none: a level
/// charged for and discarded at the far end answers exactly like one that was honoured, so a
/// session that took it away in silence would leave somebody believing every later turn was
/// thinking harder than it was. Said on the turn that learned it and not again, the condition
/// holding for the rest of the session.
///
/// The refusal is read by the caller, where the configuration says which service was answering.
/// Passed in rather than asked for here so the rule is a function of its argument: what a service
/// refused is remembered for the life of the process, and a test that had to arrange one could not
/// arrange it back.
pub(super) fn note_a_refused_level(session: &mut Session, refused: bool) {
    if !refused || !session.model_reads_effort() {
        return;
    }
    session.note_model_reads_effort(false);
    say_if_unread(session);
}

/// Take on what the listing says about the model in force, where there is no session to hold it.
///
/// A one-shot run puts a model in force without anybody picking one: the command line named it, or
/// it was read back off disk from a session that has ended. The listing is the only place a window
/// is ever reported, so this is the only place such a run can learn one, and without it the run
/// compacts against the default: a figure a narrow window never reaches, so compaction cannot fire
/// at all, and one a wide window passes three quarters of the way through the conversation it could
/// have held.
///
/// Whether that model reads an effort level is the other thing the same listing answers, and it
/// comes back rather than being kept, a run having nowhere to keep it. Answered here because the
/// listing is fetched once: a caller asking separately would spend a second round trip on a
/// question the first answer already held (BACKEND-22).
///
/// Silent, where [`adopt_budget_for_current_model`] notes the new budget, because a run has nobody
/// watching and no transcript to put a line in. A listing that cannot be fetched leaves the default
/// in place for the same reason it does in a session, and leaves the level to go out: neither is
/// the roster saying otherwise.
pub fn adopt_listing_for_model(config: &mut Config, model: &str) -> bool {
    let Ok(models) = list_models(config, Some(model)) else {
        return true;
    };
    config.adopt_window(advertised_window(&models, Some(model)));
    config.adopt_inputs(model, advertised_inputs(&models, Some(model)));
    reads_effort(&models, model)
}

/// Whether a reply matching a schema can be asked of `model`, as far as its listing says.
///
/// True for a model no listing describes and for a row that states no parameters, which is every
/// Brave roster row and every settings-file model: neither is the roster saying the field would be
/// ignored, and refusing a run on the strength of silence would decide against the person. False
/// only where a row states its parameters and names neither of the two a gateway uses for it.
pub fn reads_structured_output(config: &Config, model: &str) -> bool {
    list_models(config, Some(model))
        .map(|models| lists_structured_output(&models, model))
        .unwrap_or(true)
}

/// Whether the roster lets `chosen` be asked for a schema. Split from the fetch so the matching is
/// testable without a server.
pub(super) fn lists_structured_output(
    models: &[bravebot_aichat::models::Model],
    chosen: &str,
) -> bool {
    models
        .iter()
        .find(|model| model.key == chosen)
        .and_then(|model| model.advertised.parameters.as_ref())
        .is_none_or(|parameters| {
            parameters
                .iter()
                .any(|it| it == "structured_outputs" || it == "response_format")
        })
}

/// Whether the roster says the model a request names reads an effort level: the pick where there is
/// one, and the configured default where there is none.
///
/// Split out so the choice of which name is looked up is testable without a server.
pub(super) fn reads_effort_in_force(
    models: &[bravebot_aichat::models::Model],
    picked: Option<&str>,
    default_model: &str,
) -> bool {
    reads_effort(models, picked.unwrap_or(default_model))
}

/// Whether the roster says `chosen` reads an effort level.
///
/// Split from the fetch so the matching is testable without a server. True for a model no listing
/// described, which covers a name from a settings file and a listing that could not be fetched:
/// neither is the roster saying a level would be ignored, and only the roster saying so is a reason
/// to withhold what somebody asked for.
pub(super) fn reads_effort(models: &[bravebot_aichat::models::Model], chosen: &str) -> bool {
    models
        .iter()
        .find(|model| model.key == chosen)
        .is_none_or(|model| model.reads_effort)
}

/// The window advertised for `chosen`, or `None` where the listing does not describe it.
///
/// Split from the fetch so the matching is testable without a server. Nothing chosen means
/// `automatic`, whose model is resolved per request, so no entry's window is the one in force.
pub(super) fn advertised_window(
    models: &[bravebot_aichat::models::Model],
    chosen: Option<&str>,
) -> Option<u64> {
    let name = chosen?;
    models
        .iter()
        .find(|model| model.key == name)?
        .conversation_tokens
}

/// What the listing says `chosen` takes, or nothing where it does not describe it.
///
/// Split from the fetch for the reason [`advertised_window`] is, and empty for the same cases:
/// neither `automatic` nor a model the listing leaves out is the roster saying it takes only text,
/// so a picture put to one goes ahead (VET-4).
pub(super) fn advertised_inputs<'a>(
    models: &'a [bravebot_aichat::models::Model],
    chosen: Option<&str>,
) -> &'a [String] {
    chosen
        .and_then(|name| models.iter().find(|model| model.key == name))
        .map_or(&[], |model| model.advertised.input_modalities.as_slice())
}

/// Ask the endpoint what it offers, let the user pick, and remember what they picked.
///
/// A refusal or an unreachable endpoint leaves the model as it was and says so. That is the right
/// outcome for a list nobody could fetch: guessing a set of names would offer choices the backend
/// may not have, and a picker showing only "automatic" would look like a server with one model.
///
/// The list is content and the choice is routing. Nothing here is quarantined, because there is no
/// planner context to keep it out of: the names are drawn for a person, and their pick is the
/// endorsement for the request field it lands in.
pub(super) fn choose_model(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut Session,
    config: &mut Config,
) {
    match list_models(config, session.model()) {
        Ok(models) => {
            let chosen = crate::model_prompt::choose(terminal, models, session.model(), |frame| {
                render::draw(frame, session);
            });
            if let Some(chosen) = chosen {
                model_picked(session, config, chosen);
            }
        }
        Err(detail) => session.note(t!(session_models_unavailable, problem = detail)),
    }
}

/// Put the model the person chose in force for the next turn, and say so.
pub(super) fn model_picked(
    session: &mut Session,
    config: &mut Config,
    chosen: bravebot_aichat::models::Model,
) {
    // The listing is the only place a window is ever reported, so the budget is taken
    // here, while the entry that named it is in hand. Said once when it changes, since
    // a budget belongs to the model rather than to a turn.
    if config.adopt_window(chosen.conversation_tokens) {
        session.note(t!(session_context_budget, budget = config.context_budget));
    }
    session.update_budget(config.context_budget, config.budget_is_guessed());
    config.adopt_inputs(&chosen.key, &chosen.advertised.input_modalities);
    // Which service answers is said here as well as in the picker: the row that
    // carried it is gone by the time the note is read, and the same slug reached
    // through two services is two bills.
    session.note(match chosen.provider.as_deref() {
        Some(service) => t!(
            session_using_model_from,
            model = &chosen.display_name,
            service = service
        ),
        None => t!(session_using_model, model = &chosen.display_name),
    });
    session.note_model_reads_effort(chosen.reads_effort);
    if !chosen.reads_effort && session.effort().is_some() {
        session.note(t!(session_effort_not_read));
    }
    session.choose_model(chosen.key, config);
}

/// Ask the endpoints what they offer and open the model picker as a mode of the session, for a turn
/// that is running.
///
/// A refusal or an unreachable endpoint leaves the model as it was and says so, as at rest.
pub(super) fn open_model_picker(session: &mut Session, config: &Config) {
    match list_models(config, session.model()) {
        Ok(models) => session.open_picking(crate::picking::Picking::Model(
            crate::model_prompt::Picker::new(models, session.model()),
        )),
        Err(detail) => session.note(t!(session_models_unavailable, problem = detail)),
    }
}
