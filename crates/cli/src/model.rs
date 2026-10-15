//! Which model a run or session asks for, and what is said where nothing serves it.

use crate::import;
use bravebot_config::Config;
use bravebot_i18n::t;
use std::path::Path;

/// What to say to somebody whose configuration names no service that can serve a turn.
///
/// Three routes, each a line, in the order they are worth taking today. The two that reach a
/// service directly lead; importing a Leo Premium subscription is last and says why, since those
/// models are reached through Brave's AI gateway and it has open problems of its own. It is still
/// the shortest route for somebody who already subscribes, which is why it is offered at all.
///
/// `a_service_is_configured` replaces all three with one line. That person set a service up and
/// left the model this build baked in, which a settings block copied out of another tool does by
/// design, so what they need is the key that names one of their own models.
///
/// `refused` is what was wrong with a stored subscription, where something was. It leads, because
/// it changes what the person should do: a batch that could not be read is one import away.
///
/// Built as a string rather than printed, so the ending that carries it leads the block with its
/// identifier the way it does every other failure, and so `doctor` can say the same thing. It says it too: a report reading "configuration OK" about a machine where a
/// session refuses to start is the first thing somebody in that position goes and looks at.
///
/// Nothing here names `doctor` itself, for that reason. A line telling somebody to run the
/// command they are reading the output of is a line that has to be edited out of one of the two
/// places it appears, which is how the two come to say different things.
///
/// `looked` is what Claude Code and opencode hold, where nobody was asked about it: what was left
/// behind comes before the routes, so a person whose setup was looked at is told so (IMPORT-4), and
/// a source holding something importable names the command that asks (IMPORT-8).
pub(crate) fn how_to_configure_a_model(
    refused: Option<&str>,
    a_service_is_configured: bool,
    looked: &import::Looked,
) -> String {
    let mut lines = vec![t!(onboarding_no_model).to_string()];
    if let Some(problem) = refused {
        lines.push(t!(onboarding_subscription_unusable, problem = problem));
    }
    lines.push(String::new());
    if !looked.left().is_empty() {
        lines.extend(looked.left().iter().cloned());
        lines.push(String::new());
    }
    if let Some(command) = looked.command() {
        lines.push(command);
        lines.push(String::new());
    }

    // One line or three routes, never both. Somebody who has a service set up and only the wrong
    // model in force is one settings key away, and three ways to set up a service is three things
    // to read past on the way to the one that applies.
    if a_service_is_configured {
        lines.push(t!(onboarding_name_a_configured_model).to_string());
    } else {
        lines.push(t!(onboarding_pick_one).to_string());
        for route in [
            t!(onboarding_bedrock),
            t!(onboarding_openrouter),
            t!(onboarding_leo),
        ] {
            lines.push(format!("  - {route}"));
        }
    }

    lines.push(String::new());
    lines.push(t!(onboarding_where_to_read).to_string());
    lines.join("\n")
}

/// How to configure a service, where nothing configured serves `model`, or `None` where something
/// does.
///
/// A model the machine-level layer refuses answers here too, and says something else: there is no
/// service to configure, so what it names is the file that refused and which of its lists did
/// (BACKEND-48).
pub(crate) fn nothing_serves(config: &Config, model: &str) -> Option<String> {
    match bravebot_agent::backend::serving(config, &bravebot_net::Egress::new(), model) {
        bravebot_agent::backend::Serving::NothingConfigured {
            subscription,
            a_service_is_configured,
        } => Some(how_to_configure_a_model(
            subscription.as_deref(),
            a_service_is_configured,
            &import::looked(a_service_is_configured),
        )),
        bravebot_agent::backend::Serving::Refused { file, why } => {
            Some(managed_refusal(model, &file, why))
        }
        bravebot_agent::backend::Serving::Configured => None,
    }
}

/// The model `--advisor` names, resolved like `--model`, or what to say where nothing could ask it.
pub(crate) fn resolve_advisor(config: &Config, name: &str) -> Result<String, String> {
    let advisor = config.model_named(name);
    match nothing_serves(config, &advisor) {
        Some(how) => Err(how),
        None => Ok(advisor),
    }
}

/// What a start or a run says where the machine-level layer refuses `model` (BACKEND-48).
///
/// Here rather than at each site because four of them say it: a one-shot run, the report, the start
/// that opens an interactive session, and that same start again once an import has written a model.
/// The model is named as well as the file, since a tier word resolves to a name the person reading
/// this wrote nowhere.
pub(crate) fn managed_refusal(
    model: &str,
    file: &Path,
    why: bravebot_config::ModelRefusal,
) -> String {
    t!(
        managed_model_refused,
        model = model,
        reason = bravebot_agent::backend::refusal_reason(file, why)
    )
    .to_string()
}

/// The model a run asks for: the one the command line named, else the one a session would read.
///
/// A run started from a script resolves a model the way a session opening in the same directory
/// does, so a script reaches the model somebody already chose without an interactive step, and
/// neither surface has a model the other cannot ask for. Below both is the configured model,
/// which is what an absent record, or one a checkout outranks, leaves in force.
///
/// The command line outranks the record because it names a model for one run and nothing else,
/// which is the only way a script can pin one against a choice made elsewhere.
pub(crate) fn model_asked_for(named: Option<String>, stored: Option<String>) -> Option<String> {
    named.or(stored)
}

/// The model this run or session will ask a service for.
///
/// The same three sources the task below is built from, in the same order: a name given on the
/// command line, the one a session recorded where no checkout's settings outrank it and nothing
/// sets it aside, and the configured default. `named` is the raw argument, resolved against the
/// configuration here for the reason the task resolves it, since a tier word names a model only
/// the configuration knows.
///
/// A function because the question is asked before the task exists, and because an answer that
/// differed from the task's would refuse a run over a model it was never going to request.
pub(crate) fn model_for_this_run(named: Option<&str>, config: &Config) -> String {
    model_asked_for(
        named.map(|name| config.model_named(name)),
        bravebot_agent::backend::pick(
            config,
            bravebot_session::store::model(
                bravebot_session::store::load_model(),
                &bravebot_config::Settings::load(),
            ),
        )
        .into_model(),
    )
    .unwrap_or_else(|| config.default_model.clone())
}

/// The complaint a run has when one model was asked for and another answered, if it has one.
///
/// The endpoint substitutes rather than refusing, so the reported name is the only trace there is.
/// About the model in force whatever named it: the flag, a remembered choice and the settings key
/// all pin a model somebody expects to be answered by.
///
/// Nothing where the name asks for whichever model the server picks rather than for a particular
/// one, because resolving to a model is what that name is for. Nothing where the backend does not
/// report the name it was asked for either, where a reply that says something else is the
/// indirection working rather than a different model answering.
pub(crate) fn model_not_served(asked: &str, comparable: bool, served: &str) -> Option<String> {
    if !comparable || asked == bravebot_config::DEFAULT_MODEL || asked == served {
        return None;
    }
    Some(t!(
        session_model_substituted,
        asked = asked,
        served = served
    ))
}
