//! Which model a run or session asks for, and what is said where nothing serves it.

use crate::import;
use crate::plain::Prompting;
use crate::progress::printable;
use bravebot_agent::chatgpt::{AccountFile, Model};
use bravebot_config::Config;
use bravebot_i18n::t;
use std::io::{BufRead, Write};
use std::path::Path;

/// What to say to somebody whose configuration names no service that can serve a turn.
///
/// Four routes, each a line, in the order they are worth taking today. The three that reach a
/// service directly lead: AWS Bedrock, a gateway, and signing in with a ChatGPT account. Importing a
/// Leo Premium subscription is last and says why, since those models are reached through Brave's AI
/// gateway and it has open problems of its own. It is still the shortest route for somebody who
/// already subscribes, which is why it is offered at all.
///
/// `a_service_is_configured` replaces all four with one line. That person set a service up and
/// left the model this build baked in, which a settings block copied out of another tool does by
/// design, so what they need is the key that names one of their own models. Where the service is a
/// stored ChatGPT sign-in the line names the commands that open a session or run a task on one of the
/// account's models.
///
/// `chatgpt` is what the sign-in file holds. A file that cannot be read is said before the routes,
/// with the remedy, since it is not a service and the person has to know why a sign-in they made is
/// not used.
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
/// `why_no_model_was_offered` is the sentence for a start that asked the account for its models and
/// was offered none, said under the first line: the sign-in ended while it was asked, or the listing
/// could not be had, was refused or is empty (BACKEND-39).
///
/// `looked` is what Claude Code and opencode hold, where nobody was asked about it: what was left
/// behind comes before the routes, so a person whose setup was looked at is told so (IMPORT-4), and
/// a source holding something importable names the command that asks (IMPORT-8).
pub(crate) fn how_to_configure_a_model(
    refused: Option<&str>,
    a_service_is_configured: bool,
    chatgpt: AccountFile,
    looked: &import::Looked,
    why_no_model_was_offered: Option<&str>,
) -> String {
    let mut lines = vec![t!(onboarding_no_model).to_string()];
    if let Some(problem) = refused {
        lines.push(t!(onboarding_subscription_unusable, problem = problem));
    }
    if let Some(why) = why_no_model_was_offered {
        lines.push(why.to_string());
    }
    if chatgpt == AccountFile::Unreadable
        && let crate::chatgpt::Standing::Unusable(reason) = crate::chatgpt::standing()
    {
        lines.push(reason);
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

    // One line or the routes, never both. Somebody who has a service set up and only the wrong
    // model in force is one settings key away, and four ways to set up a service is four things
    // to read past on the way to the one that applies.
    if a_service_is_configured && chatgpt == AccountFile::Stored {
        lines.push(t!(onboarding_chatgpt_signed_in).to_string());
        lines.push(t!(chatgpt_use_by_name).to_string());
    } else if a_service_is_configured {
        lines.push(t!(onboarding_name_a_configured_model).to_string());
    } else {
        lines.push(t!(onboarding_pick_one).to_string());
        for route in [
            t!(onboarding_bedrock),
            t!(onboarding_openrouter),
            t!(onboarding_chatgpt),
            t!(onboarding_leo),
        ] {
            lines.push(format!("  - {route}"));
        }
    }

    lines.push(String::new());
    lines.push(t!(onboarding_where_to_read).to_string());
    lines.join("\n")
}

/// Whether `serving` is a start that asks which of the ChatGPT account's models to use
/// (BACKEND-39): nothing serves the model in force, no model was named on the command line, and the
/// stored sign-in is the way out. A person who can be asked, and an account that lists a model, are
/// the further conditions [`ask_which_of_the_accounts_models`] puts.
pub(crate) fn asks_which_of_the_accounts_models(
    serving: &bravebot_agent::backend::Serving,
    named: Option<&str>,
) -> bool {
    named.is_none()
        && matches!(
            serving,
            bravebot_agent::backend::Serving::NothingConfigured {
                chatgpt: AccountFile::Stored,
                ..
            }
        )
}

/// What a start that asks which of the account's models to use came to.
pub(crate) enum Asked {
    /// Nobody could be asked, so nothing was asked of the account or of the person.
    Nobody,
    /// A person was asked and gave no row, or the pick could not be remembered, which was said.
    Declined,
    /// The account offered no row. The sentence says why, for the refusal. The sign-in file may have
    /// changed, since asking for the listing removes a sign-in that has ended.
    NothingOffered(String),
    /// The pick is the model the start reads back.
    InForce,
}

/// Ask which of the ChatGPT account's models the start is to use, and record the answer as `/model`
/// records a pick (BACKEND-39).
///
/// Asked where a person can be, as an import is, and where the account lists a model: an account
/// whose listing is unreadable, refused or empty offers nothing to choose from, and is said so by
/// [`Asked::NothingOffered`]. Nothing is read from stdin where it is not a terminal. An answer that is
/// not the number of a row declines, as an empty answer and the end of the input do, and nothing is
/// recorded for a decline.
///
/// What is said afterwards follows what the start reads back, not what was written. A name over the
/// bytes the store reads back, and a write that failed, leave the start on another model, and it says
/// so and ends as a decline does.
///
/// The listing is asked of the serving host with the stored sign-in, which may renew the access token
/// and write the sign-in file as any turn does, or remove it where the sign-in has ended.
pub(crate) fn ask_which_of_the_accounts_models(config: &Config) -> Asked {
    if !import::a_person_can_be_asked() {
        return Asked::Nobody;
    }
    let models = match bravebot_tui::app::account_models_to_start_on(config) {
        Ok(models) => models,
        Err(listing) => return Asked::NothingOffered(why_no_model_was_offered(&listing)),
    };
    // The process's own stdin, whose buffer later readers share, as the import's question has it.
    ask_among(config, &models, t!(chatgpt_start_heading))
}

/// Whether `serving` is a start that asks which of a configured service's models to use
/// (BACKEND-39): nothing serves the model in force, no model was named on the command line, and a
/// Bedrock or gateway block is configured, which a stored sign-in beside it does not change.
pub(crate) fn asks_which_of_the_configured_models(
    serving: &bravebot_agent::backend::Serving,
    named: Option<&str>,
) -> bool {
    named.is_none()
        && matches!(
            serving,
            bravebot_agent::backend::Serving::NothingConfigured {
                a_service_is_configured: true,
                chatgpt: AccountFile::NotStored,
                ..
            }
        )
}

/// Ask which of the models of the configured Bedrock account and gateways the start is to use, and
/// record the answer as `/model` records a pick (BACKEND-39), as
/// [`ask_which_of_the_accounts_models`] does for a sign-in.
///
/// A service that lists no model, because its block names none and it could not be asked, offers
/// nothing to choose from, and the start goes on to its refusal as a decline does.
pub(crate) fn ask_which_of_the_configured_models(config: &Config) -> Asked {
    if !import::a_person_can_be_asked() {
        return Asked::Nobody;
    }
    let models = bravebot_tui::app::configured_models_to_start_on(config);
    if models.is_empty() {
        return Asked::Declined;
    }
    ask_among(config, &models, t!(configured_start_heading))
}

/// List `models` under `heading`, ask which, and record the pick.
fn ask_among(config: &Config, models: &[Model], heading: impl AsRef<str>) -> Asked {
    // The process's own stdin, whose buffer later readers share, as the import's question has it.
    let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
    let Some(chosen) = which_of(&mut asking, models, heading.as_ref()) else {
        return Asked::Declined;
    };
    bravebot_session::store::record_model(config, &chosen.key);
    let name = printable(&chosen.display_name);
    let read_back = model_for_this_run(None, config) == chosen.key;
    asking.say(&match read_back {
        true => t!(chatgpt_start_chosen, name = name),
        false => t!(chatgpt_start_not_remembered, name = name),
    });
    match read_back {
        true => Asked::InForce,
        false => Asked::Declined,
    }
}

/// The sentence for a listing that gave no row: the sign-in command's for each way a listing is not
/// had or is empty, and the one for an ended sign-in where the sign-in was found to have ended, which
/// the listing reports as nobody being signed in.
fn why_no_model_was_offered(listing: &bravebot_agent::chatgpt::AccountModels) -> String {
    match listing {
        bravebot_agent::chatgpt::AccountModels::Absent => t!(failure_account_ended).to_string(),
        // The sentence for a listing that gave no row is the same whoever is asked, so what a start
        // would ask is no part of it.
        other => crate::chatgpt::said_of_listing(other, false),
    }
}

/// List `models` by number and ask which, or `None` for an answer that is not the number of one.
///
/// The names are the service's and a person reads them on a terminal that is not drawing a frame, so
/// they are shown as pictures where they hold a control character.
fn which_of<'a, R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    models: &'a [Model],
    heading: &str,
) -> Option<&'a Model> {
    asking.say(heading);
    for (number, model) in (1..).zip(models) {
        asking.say(&format!(
            "  {number}. {}",
            t!(
                chatgpt_start_row,
                name = printable(&model.display_name),
                key = printable(&model.key)
            )
        ));
    }
    let answer = asking.answer(t!(chatgpt_start_question))?;
    crate::plain::numbered_row(&answer, models)
}

/// Whether a start with no model named would ask which of the account's models to use, setting aside
/// the terminals and the mode it also needs ([`asks_which_of_the_accounts_models`]).
///
/// For the sign-in command, which tells a person what to run next. A person with a Bedrock or a gateway
/// block, or with a model in force that something serves, is not asked, and is told so.
pub(crate) fn a_start_would_ask(config: &Config) -> bool {
    let model = model_for_this_run(None, config);
    let serving = bravebot_agent::backend::serving(config, &bravebot_net::Egress::new(), &model);
    asks_which_of_the_accounts_models(&serving, None)
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
            chatgpt,
        } => Some(how_to_configure_a_model(
            subscription.as_deref(),
            a_service_is_configured,
            chatgpt,
            &import::looked(a_service_is_configured),
            None,
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

/// Puts `--processor-model` in force for this run, or says why no processor could run on it.
///
/// The flag outranks the `processorModel` setting, and `inherit` names no model, so it puts
/// processors back on the run's own whatever the setting names. A model the machine refuses or no
/// service answers for stops the run before it starts, as `--advisor`'s does, so a manifest run does
/// not spend both planning calls before its first processing step meets the refusal. A sign-in is
/// asked of the processor when it starts, as for the setting, since the run's own sign-in comes
/// first and may be the one that covers the model (CLI-34).
///
/// Nothing is put in force when it is refused.
pub(crate) fn put_processor_model_in_force(
    config: &mut Config,
    flag: Option<&str>,
) -> Result<(), String> {
    let Some(flag) = flag else {
        return Ok(());
    };
    let named = bravebot_config::processor_model_named(flag);
    if let Some(model) = named.as_deref().map(|name| config.model_named(name)) {
        // Not `nothing_serves`, whose advice is about the run's own model: following it would not
        // fix a processor model, and it names neither the model nor what it was named for.
        match bravebot_agent::backend::serving(config, &bravebot_net::Egress::new(), &model) {
            bravebot_agent::backend::Serving::Refused { file, why } => {
                return Err(managed_refusal(&model, &file, why));
            }
            bravebot_agent::backend::Serving::NothingConfigured { .. } => {
                return Err(bravebot_agent::report::processor_model_refusal(
                    &bravebot_agent::compact::SideModelRefusal::NotServed(model),
                ));
            }
            bravebot_agent::backend::Serving::Configured => {}
        }
    }
    config.processor_model = named;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A row of the account as the picker shows it, with the fields the question reads.
    fn an_account_row(key: &str, name: &str) -> Model {
        let mut row = Model::automatic();
        row.key = key.to_string();
        row.display_name = name.to_string();
        row.provider = Some("ChatGPT".to_string());
        row
    }

    /// What the question put and which row an answer typed picked.
    fn asked_which(rows: &[Model], typed: &str) -> (Option<String>, String) {
        let mut shown = Vec::new();
        let picked = {
            let mut asking = Prompting::new(std::io::Cursor::new(typed.to_string()), &mut shown);
            which_of(&mut asking, rows, t!(chatgpt_start_heading)).map(|row| row.key.clone())
        };
        (picked, String::from_utf8(shown).expect("text"))
    }

    /// BACKEND-39: the rows are numbered from one in the order they are given, each with the name a
    /// person reads and the name to give `--model`, and the number typed picks the row it numbers.
    /// The regressions it rejects are a count from zero, a row shown without the name that goes after
    /// `--model`, and a question put before the rows.
    #[test]
    fn the_rows_are_numbered_from_one_in_the_order_given_and_the_number_typed_picks_its_row() {
        let rows = [
            an_account_row("chatgpt/gpt-a", "GPT A"),
            an_account_row("chatgpt/gpt-b", "GPT B"),
            an_account_row("chatgpt/gpt-c", "GPT C"),
        ];

        let (picked, shown) = asked_which(&rows, "2\n");

        assert_eq!(picked.as_deref(), Some("chatgpt/gpt-b"));
        let at = |words: &str| {
            shown
                .find(words)
                .unwrap_or_else(|| panic!("{words:?} was not shown: {shown}"))
        };
        let order = [
            at(t!(chatgpt_start_heading)),
            at("  1. GPT A (chatgpt/gpt-a)"),
            at("  2. GPT B (chatgpt/gpt-b)"),
            at("  3. GPT C (chatgpt/gpt-c)"),
            at(t!(chatgpt_start_question)),
        ];
        assert!(order.is_sorted(), "{shown}");
        for (typed, key) in [("1\n", "chatgpt/gpt-a"), ("3\n", "chatgpt/gpt-c")] {
            assert_eq!(asked_which(&rows, typed).0.as_deref(), Some(key));
        }
    }

    /// BACKEND-39: an answer that is not the number of a row picks none. The regressions it rejects are
    /// a number past the last row clamped to it, zero or a negative number wrapped round to a row, a
    /// sign or a second word read as part of a number, an empty answer or the end of the input taken
    /// for the first row, and a name typed where a number is asked for.
    #[test]
    fn an_answer_that_is_not_the_number_of_a_row_picks_none() {
        let rows = [
            an_account_row("chatgpt/gpt-a", "GPT A"),
            an_account_row("chatgpt/gpt-b", "GPT B"),
        ];

        for typed in [
            "\n",
            "",
            "0\n",
            "3\n",
            "-1\n",
            "+1\n",
            "1 2\n",
            "1.0\n",
            "gpt-a\n",
            " \n",
            "99999999999999999999999\n",
        ] {
            assert_eq!(asked_which(&rows, typed).0, None, "{typed:?}");
        }
    }

    /// BACKEND-39: the names are the service's text and are shown as pictures where they hold a
    /// control character, as the import's questions show what another program's file said. The
    /// regression it rejects is a name that reaches the terminal as it arrived, which would move the
    /// cursor of the terminal the question is on.
    #[test]
    fn a_name_the_service_gave_is_shown_without_its_control_characters() {
        let rows = [an_account_row(
            "chatgpt/gpt-\u{1b}[2Jx",
            "name\u{1b}]52;c;aGk=\u{7}",
        )];

        let (_, shown) = asked_which(&rows, "1\n");

        assert!(
            !shown.contains('\u{1b}') && !shown.contains('\u{7}'),
            "{shown:?}"
        );
        assert!(shown.contains("name"), "{shown}");
    }

    /// BACKEND-39: each way to configure a service is named by what to type or write, the ChatGPT
    /// sign-in among them, and Leo Premium comes last because its line tells the reader to prefer
    /// the others. A list that left the sign-in out sends somebody who has the account to a gateway
    /// they do not need.
    #[test]
    fn the_routes_of_an_unconfigured_start_each_name_what_to_type_or_write_and_leave_leo_last() {
        let said = how_to_configure_a_model(
            None,
            false,
            AccountFile::NotStored,
            &import::Looked::default(),
            None,
        );
        let at = |route: &str| {
            said.find(route)
                .unwrap_or_else(|| panic!("{route} is not a route: {said}"))
        };
        let routes = [
            at("amazon-bedrock"),
            at("OpenRouter"),
            at("bravebot auth login chatgpt"),
            at("bravebot auth login leo"),
        ];
        assert!(routes.is_sorted(), "{said}");
    }

    /// BACKEND-39: somebody who has a service set up and only the wrong model in force is told to
    /// name a model, and is not sent to set up another service.
    #[test]
    fn a_configured_service_is_told_to_name_a_model_and_is_not_offered_the_routes() {
        let said = how_to_configure_a_model(
            None,
            true,
            AccountFile::NotStored,
            &import::Looked::default(),
            None,
        );
        assert!(said.contains("`model` key"), "{said}");
        for route in [
            "amazon-bedrock",
            "OpenRouter",
            "bravebot auth login chatgpt",
            "bravebot auth login leo",
        ] {
            assert!(!said.contains(route), "{route} was offered: {said}");
        }
    }

    /// BACKEND-39: somebody whose ChatGPT sign-in is stored and whose model in force is Brave's own
    /// is told to name one of the account's models, and is not shown the routes they have just
    /// followed. A list that kept showing the routes would send them to sign in again.
    #[test]
    fn a_stored_chatgpt_sign_in_is_told_to_name_one_of_the_accounts_models_and_not_offered_the_routes()
     {
        let said = how_to_configure_a_model(
            None,
            true,
            AccountFile::Stored,
            &import::Looked::default(),
            None,
        );
        assert!(said.contains(t!(onboarding_chatgpt_signed_in)), "{said}");
        assert!(said.contains(t!(chatgpt_use_by_name)), "{said}");
        assert!(said.contains("chatgpt/<model>"), "{said}");
        for route in [
            "amazon-bedrock",
            "OpenRouter",
            "bravebot auth login chatgpt",
            "bravebot auth login leo",
            "`model` key",
        ] {
            assert!(!said.contains(route), "{route} was offered: {said}");
        }
    }
}
