//! What the endpoint offers, and what a session adopts from a model: its window, its inputs, whether
//! it reads effort or structured output.

use super::*;
use bravebot_config::{Gateways, RosterAccounts, RosterScope};

/// The model a request names now: the pick where there is one, and otherwise the configured default,
/// which a `model` key of the settings file sets.
///
/// What the picker marks as current, what the roster is ordered around, and what the budget, the
/// level the roster says is read, the sign-in a session makes before a turn and the refusal of a level
/// are asked about, so that a session opened on a configured model is not taken for one on Brave's
/// `automatic`. The callers that hand a pick to `list_models`, `Config::adopt_window_of_model_in_force`
/// and `Config::runs_on_chatgpt_account` leave the default to them, which read it themselves.
pub(super) fn model_in_force<'a>(picked: Option<&'a str>, config: &'a Config) -> &'a str {
    picked.unwrap_or(&config.default_model)
}

/// Whether the service has refused a level for the model in force, which the turn or the manifest run
/// that has just ended may have learned. `refused` asks it of a model, so which model is asked is
/// reached without a service that has refused one.
pub(super) fn refused_level_in_force(
    session: &Session,
    config: &Config,
    refused: impl FnOnce(&Config, &str) -> bool,
) -> bool {
    refused(config, model_in_force(session.model(), config))
}

/// Ask the service that answers the model in force what it offers, for the row of that model
/// ([BACKEND-64](../../../../docs/specs/backends.md#BACKEND-64)).
///
/// The budget lookup a session does when it starts, and every other question about one model's row.
/// It waits on that service alone: a person with several gateways configured would otherwise wait
/// for each of them in turn to learn the window of a model only one of them serves. The picker asks
/// all of them, through [`list_models_for_picker`].
pub(super) fn list_models(
    config: &Config,
    chosen: Option<&str>,
) -> Result<Vec<bravebot_aichat::models::Model>, String> {
    roster(config, chosen, config.roster_scope_for_a_turn(chosen)).map(|roster| roster.models)
}

/// The rows a picker offers, and what a person is told where the rows of a service they use are not
/// among them.
pub(super) struct PickerRoster {
    pub(super) models: Vec<bravebot_aichat::models::Model>,
    /// One sentence for each service whose listing could not be read: a gateway that names no models
    /// and could not be asked, and the ChatGPT account where a sign-in is stored and its listing
    /// could not be had. A person who never signed in to ChatGPT is told nothing, and a listing that
    /// was read, even an empty one, is not a problem.
    pub(super) problems: Vec<String>,
}

/// [`list_models`] for the picker, which offers the accounts' models whatever is in force
/// ([CHATGPT-9](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9),
/// [XAI-19](../../../../docs/specs/xai-sign-in.md#XAI-19)).
pub(super) fn list_models_for_picker(
    config: &Config,
    chosen: Option<&str>,
) -> Result<PickerRoster, String> {
    roster(config, chosen, RosterScope::for_a_picker())
}

/// The rosters of the services `scope` names, asked at once
/// ([BACKEND-64](../../../../docs/specs/backends.md#BACKEND-64)).
///
/// The accounts are asked on this thread while the gateways and Brave are asked on threads of their
/// own, so what is waited for is the slowest of them and not the sum. A unit test scripts the
/// accounts per thread, which is the other reason they stay here.
fn roster(
    config: &Config,
    chosen: Option<&str>,
    scope: RosterScope<'_>,
) -> Result<PickerRoster, String> {
    let (gathered, (chatgpt, xai)) = gather(config, chosen, scope, || {
        (
            scope.accounts.chatgpt.then(|| ask_the_account(config)),
            scope.accounts.xai.then(|| ask_the_xai_account(config)),
        )
    });
    let Gathered {
        mut configured,
        mut problems,
        brave,
    } = gathered;

    // Additive like a gateway: a stored ChatGPT sign-in adds its models to the rows and takes none
    // away. Not asked where nothing is signed in, and a listing that cannot be had adds the compiled
    // names and is said, so a person who signed in does not take them for what the account reaches.
    if let Some(asked) = chatgpt {
        problems.extend(account_problem_said(&asked));
        configured.extend(account_rows(asked, model_in_force(chosen, config)));
    }
    // The xAI account joins the same way, and is said in the same place where its listing could not
    // be had ([XAI-19](../../../../docs/specs/xai-sign-in.md#XAI-19)).
    if let Some(asked) = xai {
        problems.extend(xai_problem_said(&asked));
        configured.extend(xai_rows(asked, model_in_force(chosen, config)));
    }
    let rostered = |models| PickerRoster {
        models: requestable(config, models),
        problems,
    };

    match brave {
        Some(listed) => combined(configured, listed).map(rostered),
        None => Ok(rostered(configured)),
    }
}

/// What the services of a roster answered, other than the accounts.
struct Gathered {
    /// The Bedrock account's rows and every gateway's, in the order the configuration lists them.
    configured: Vec<bravebot_aichat::models::Model>,
    /// What a person is told where a gateway's rows could not be had, in the order they were.
    problems: Vec<String>,
    /// Brave's listing, where it was asked.
    brave: Option<Result<Vec<bravebot_aichat::models::Model>, String>>,
}

/// The Bedrock account's rows, every gateway's and Brave's, with each request that goes out made at
/// once, and `meanwhile` run on this thread while they are in flight.
///
/// Which services are asked, how long each is waited for and the order the answers come back in are
/// [`bravebot_aichat::roster::ask`]'s, which the window uses as well. What is done here is naming
/// the rows and telling a person which gateway could not be read.
fn gather<U>(
    config: &Config,
    chosen: Option<&str>,
    scope: RosterScope<'_>,
    meanwhile: impl FnOnce() -> U,
) -> (Gathered, U) {
    use bravebot_aichat::roster::Entry;

    let (roster, meanwhile) = bravebot_aichat::roster::ask(config, scope, &Trail::new, meanwhile);
    let mut configured = Vec::new();
    let mut problems = Vec::new();
    for entry in roster.entries {
        match entry {
            // Bedrock is additive: its tiers come from configuration rather than from a listing,
            // since it has no listing endpoint and an ARN does not say which model it resolves to,
            // but the Brave roster is what everyone has and a settings block adds to it rather than
            // replacing it. The configured tiers come first: someone who named them went out of
            // their way to, and the alternative buries them under a roster they did not ask about.
            // An entry naming AWS reaches the Bedrock backend, so its rows are built the way that
            // backend's are, and a block that named none offers none rather than being asked what
            // it serves.
            Entry::Bedrock(bedrock) => configured.extend(bedrock_models(bedrock)),
            // A block that named its models is taken at its word and costs no round trip, which is
            // what keeps a configured gateway working offline.
            Entry::Named(provider) => configured.extend(provider_models(provider)),
            // Said by `doctor`, and again when a turn is sent without one. A credential that is
            // missing is not told in the words of a listing that failed, since trying again does
            // not change it.
            Entry::NoCredential(_) => {}
            Entry::Asked(_, Ok(listed)) => {
                configured.extend(in_reading_order(listed, model_in_force(chosen, config)));
            }
            // Why it could not be read is not told in the words of the error, which belong to the
            // service and to this process and are not for a picker's note.
            Entry::Asked(provider, Err(_)) => problems.push(t!(
                session_gateway_models_unreadable,
                gateway = provider.display_name(),
                id = provider.id.as_str()
            )),
        }
    }
    (
        Gathered {
            configured,
            problems,
            brave: roster.brave,
        },
        meanwhile,
    )
}

/// The rows of the Bedrock account and of every gateway the configuration names, in the order a
/// picker lists them, and what a person is told where a gateway's rows could not be had.
fn configured_rows(
    config: &Config,
    chosen: Option<&str>,
) -> (Vec<bravebot_aichat::models::Model>, Vec<String>) {
    let scope = RosterScope {
        accounts: RosterAccounts {
            chatgpt: false,
            xai: false,
        },
        gateways: Gateways::All,
        brave: false,
    };
    let (gathered, ()) = gather(config, chosen, scope, || ());
    (gathered.configured, gathered.problems)
}

/// What the ChatGPT account offers, asked of the serving host with the stored sign-in
/// ([CHATGPT-9](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-9)).
///
/// The one place this crate asks. A unit test never reaches the host, which is compiled in and would
/// be sent a stored sign-in's token from a machine that has one: it gets the answer it scripted on
/// its thread, or an account nobody signed in to, and can read how often it was asked.
fn ask_the_account(config: &Config) -> bravebot_agent::chatgpt::AccountModels {
    #[cfg(test)]
    {
        let _ = config;
        scripted_account::ask()
    }
    #[cfg(not(test))]
    {
        bravebot_agent::chatgpt::account_models(config, &mut Trail::new())
    }
}

/// The rows the ChatGPT account adds to a roster (CHATGPT-9), in the order a fetched gateway
/// roster is met in.
///
/// Split from the request so the joining is testable without a server. A listing that could not be
/// had adds the compiled list, and one the service refused in a code that names the account adds
/// nothing.
pub(super) fn account_rows(
    account: bravebot_agent::chatgpt::AccountModels,
    chosen: &str,
) -> Vec<bravebot_aichat::models::Model> {
    match account {
        bravebot_agent::chatgpt::AccountModels::Offered(rows)
        | bravebot_agent::chatgpt::AccountModels::Compiled(rows) => in_reading_order(rows, chosen),
        bravebot_agent::chatgpt::AccountModels::Absent
        | bravebot_agent::chatgpt::AccountModels::Unreadable
        | bravebot_agent::chatgpt::AccountModels::Refused(_) => Vec::new(),
    }
}

/// What a picker says where the ChatGPT account's models could not be had, or `None` where there is
/// nothing to say (CHATGPT-9).
///
/// A listing refused in a code that names the account is said in that clause's sentence, which does
/// not send the person to sign in again, since a new sign-in changes nothing there. An account
/// nobody signed in to, and one whose listing was read, are not a problem.
pub(super) fn account_problem_said(
    account: &bravebot_agent::chatgpt::AccountModels,
) -> Option<String> {
    match account {
        bravebot_agent::chatgpt::AccountModels::Unreadable => {
            Some(t!(session_chatgpt_models_unreadable).into())
        }
        bravebot_agent::chatgpt::AccountModels::Compiled(_) => {
            Some(t!(session_chatgpt_models_compiled).into())
        }
        bravebot_agent::chatgpt::AccountModels::Refused(refusal) => Some(t!(
            session_chatgpt_models_refused,
            refusal = bravebot_agent::chatgpt::said_of_refusal(*refusal)
        )),
        bravebot_agent::chatgpt::AccountModels::Absent
        | bravebot_agent::chatgpt::AccountModels::Offered(_) => None,
    }
}

/// What the xAI account offers, asked of the serving host with the stored sign-in
/// ([XAI-19](../../../../docs/specs/xai-sign-in.md#XAI-19)).
///
/// The one place this crate asks. A unit test never reaches the host, which is compiled in and would
/// be sent a stored sign-in's token from a machine that has one: it gets the answer it scripted on its
/// thread, or an account nobody signed in to, and can read how often it was asked.
fn ask_the_xai_account(config: &Config) -> bravebot_agent::xai::AccountModels {
    #[cfg(test)]
    {
        let _ = config;
        scripted_xai::ask()
    }
    #[cfg(not(test))]
    {
        bravebot_agent::xai::account_models(config, &mut Trail::new())
    }
}

/// The rows the xAI account adds to a roster, in the order a fetched gateway roster is met in.
///
/// Split from the request so the joining is testable without a server. A listing that could not be had
/// adds nothing, since no list built into this program stands in for it.
pub(super) fn xai_rows(
    account: bravebot_agent::xai::AccountModels,
    chosen: &str,
) -> Vec<bravebot_aichat::models::Model> {
    match account {
        bravebot_agent::xai::AccountModels::Offered(rows) => in_reading_order(rows, chosen),
        bravebot_agent::xai::AccountModels::Absent
        | bravebot_agent::xai::AccountModels::Unlisted => Vec::new(),
    }
}

/// What a picker says where the xAI account's models could not be had, or `None` where there is
/// nothing to say: a person who never signed in is told nothing, and a listing that was read, empty or
/// not, is not a failure. The note names the commands that work as typed: signing in again, and naming
/// a model.
pub(super) fn xai_problem_said(account: &bravebot_agent::xai::AccountModels) -> Option<String> {
    match account {
        bravebot_agent::xai::AccountModels::Unlisted => {
            Some(t!(session_xai_models_unreadable).into())
        }
        bravebot_agent::xai::AccountModels::Absent
        | bravebot_agent::xai::AccountModels::Offered(_) => None,
    }
}

/// The rows of the ChatGPT account that a start nothing serves offers to choose a model from
/// ([BACKEND-39](../../../../docs/specs/backends.md#BACKEND-39)): the account's rows as the picker
/// lists them, in the picker's order and without the models the machine-level layer refuses. Where
/// there is none, the listing that gave none, so the start can say why.
///
/// Only rows the service listed. Where the listing could not be had, the picker offers the compiled
/// names beside what it says, and a start that did the same would choose a model the account may
/// refuse for a person who has not yet seen a turn. A listing that was refused, that cannot be read
/// from the sign-in file, that was read and is empty, and an account whose sign-in ended while it was
/// asked for, are no rows at all.
pub fn account_models_to_start_on(
    config: &Config,
) -> Result<Vec<bravebot_aichat::models::Model>, bravebot_agent::chatgpt::AccountModels> {
    match ask_the_account(config) {
        listed @ bravebot_agent::chatgpt::AccountModels::Offered(_) => {
            match requestable(config, account_rows(listed, &config.default_model)) {
                rows if rows.is_empty() => {
                    Err(bravebot_agent::chatgpt::AccountModels::Offered(Vec::new()))
                }
                rows => Ok(rows),
            }
        }
        none => Err(none),
    }
}

/// The rows of the Bedrock account and the gateways a start nothing serves offers to choose a model
/// from ([BACKEND-39](../../../../docs/specs/backends.md#BACKEND-39)), as the picker lists them and
/// without the models the machine-level layer refuses.
///
/// Only the configured services: Brave's own roster is what the start was refused for. A gateway that
/// names no models is asked, and one that cannot be asked adds none, so an empty answer is a start
/// that goes on to its refusal.
pub fn configured_models_to_start_on(config: &Config) -> Vec<bravebot_aichat::models::Model> {
    requestable(config, configured_rows(config, None).0)
}

/// The rows of the xAI account that a start nothing serves offers to choose a model from
/// ([BACKEND-39](../../../../docs/specs/backends.md#BACKEND-39)), as
/// [`account_models_to_start_on`] offers the ChatGPT account's: the rows the listing gave, in the
/// picker's order and without the models the machine-level layer refuses. Where there is none, the
/// listing that gave none, so the start can say why.
///
/// No list is built into this program for the account, so a listing that could not be had is no row
/// here as it is none in the picker. A sign-in that ended while the listing was asked for, and a listing
/// that was read and is empty, are no rows at all.
pub fn xai_models_to_start_on(
    config: &Config,
) -> Result<Vec<bravebot_aichat::models::Model>, bravebot_agent::xai::AccountModels> {
    match ask_the_xai_account(config) {
        listed @ bravebot_agent::xai::AccountModels::Offered(_) => {
            match requestable(config, xai_rows(listed, &config.default_model)) {
                rows if rows.is_empty() => {
                    Err(bravebot_agent::xai::AccountModels::Offered(Vec::new()))
                }
                rows => Ok(rows),
            }
        }
        none => Err(none),
    }
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
/// Every entry is marked free. Premium here means the premium key, and reaching a model through
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
/// Every entry is marked free. Premium means the premium key, and a gateway reached with somebody's
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
            conversation_tokens: Some(provider.window_of(model)),
            // A block names models and never their parameters, so the level goes out to be judged
            // at the far end and the judgment is the only description there is: read one where
            // this service has already refused the field for this model.
            reads_effort: bravebot_aichat::reads_effort(&provider.completion_url(), &model.id),
            // A block names models and describes none of them, so there is nothing to carry.
            advertised: bravebot_aichat::models::Advertised::default(),
        })
        .collect()
}

/// Ask for the budget of the model already in force, without asking anyone to choose it again, on
/// a thread of its own so what a session opens on does not wait for a service
/// ([BACKEND-64](../../../../docs/specs/backends.md#BACKEND-64)).
///
/// A model chosen in an earlier session is read back off disk, and until this ran the window that
/// came with it was not: the budget stayed at the default and a session with room for a hundred
/// thousand tokens compacted at twenty-four, having said nothing about why.
///
/// The answer is taken by [`adopt_the_awaited_budget`], on each pass of the loop and at the start
/// of a turn.
///
/// A listing that cannot be fetched is not worth a word. The budget falls back to the default, which
/// is what it was before this existed, and a session that is merely offline should not open with a
/// complaint about a request nobody asked for.
pub(super) fn ask_for_the_budget_of_the_model_in_force(session: &mut Session, config: &Config) {
    note_whether_the_model_in_force_is_the_accounts(session, config);
    let (send, arrival) = mpsc::channel();
    let asked = config.clone();
    let chosen = session.model().map(str::to_string);
    let ask = move || {
        // The session ended before the answer came, so nobody is waiting for it.
        send.send(list_models(&asked, chosen.as_deref())).ok();
    };
    // A unit test asks before this returns, because the account it scripts is answered per thread.
    #[cfg(test)]
    ask();
    #[cfg(not(test))]
    drop(thread::spawn(ask));
    session.await_listing(model_in_force(session.model(), config).to_string(), arrival);
}

/// Adopt the budget asked for by [`ask_for_the_budget_of_the_model_in_force`], if its answer has
/// come, and say whether the screen changed.
///
/// `wait` is how long to wait for it. The loop passes none, and takes the answer on the pass it is
/// there. A turn passes the bound a listing request has, since a turn sized against a window nobody
/// has looked up compacts at the wrong place, which is worse than the pause.
///
/// A lookup about a model that is no longer the one in force is dropped before anything waits for
/// it. The pick that replaced that model brought a window of its own ([`model_picked`]), and the
/// window of the model before it is not the one in force. A turn waits for a lookup once: one it
/// has waited out stays for the loop to take and is not waited for again.
pub(super) fn adopt_the_awaited_budget(
    session: &mut Session,
    config: &mut Config,
    wait: Option<std::time::Duration>,
) -> bool {
    let in_force = model_in_force(session.model(), config).to_string();
    if !session.awaiting_listing_of(&in_force) {
        return false;
    }
    let Some(Ok(models)) = session.take_awaited_listing(wait) else {
        return false;
    };
    adopt_listing(session, config, &models);
    true
}

/// A session's start as a unit test sees it: the budget asked for and its answer taken, which the
/// test's own thread does in that order.
#[cfg(test)]
pub(super) fn adopt_budget_for_current_model(session: &mut Session, config: &mut Config) {
    ask_for_the_budget_of_the_model_in_force(session, config);
    adopt_the_awaited_budget(session, config, None);
}

/// How long a turn waits for the lookup that settles its budget: as long as a request for a
/// roster is allowed to take, and no longer.
pub(super) const BUDGET_WAIT: std::time::Duration = bravebot_aichat::models::LISTING_TIMEOUTS.reply;

/// Take what a roster says about the model in force: its window, what it takes, and whether it
/// reads an effort level.
fn adopt_listing(
    session: &mut Session,
    config: &mut Config,
    models: &[bravebot_aichat::models::Model],
) {
    // The model in force, not the pick: with no pick the configured default is what a request names
    // (BACKEND-14, COMPACT-9). `automatic` is a roster row that states no window, so it still leaves
    // the default in place. The window is taken without a note: it is the roster's own figure for a
    // model nobody chose in this session, and a start that opens with it reads as a report about
    // something the person did not do. The hint line carries the context used against it. A pick
    // says it ([`model_picked`]).
    config.adopt_window_of_model_in_force(session.model(), |in_force| {
        advertised_window(models, Some(in_force))
    });
    let in_force = model_in_force(session.model(), config).to_string();
    config.adopt_inputs(&in_force, advertised_inputs(models, Some(&in_force)));
    // On every listing, whether or not the window moved, because a budget that did not move can
    // still have stopped being one the endpoint advertised: nothing changed for compaction, and
    // what the hint line may claim did.
    session.update_budget(config.context_budget, config.budget_is_guessed());
    // The model in force, not the pick: with no pick the configured default is what a request names,
    // and it is the roster row for that one which says whether a level is read (BACKEND-22).
    session.note_model_reads_effort(reads_effort(
        models,
        model_in_force(session.model(), config),
    ));
}

/// Record whether the model in force, the pick or else the configured default, is one of the ChatGPT
/// account's, which decides whether the level a turn sends is the one chosen or the one the session
/// began with (CHATGPT-10).
///
/// Whatever a listing says, and before one is asked for: it is the configuration that names which
/// service answers a model.
pub(super) fn note_whether_the_model_in_force_is_the_accounts(
    session: &mut Session,
    config: &Config,
) {
    session.note_model_is_account(config.runs_on_chatgpt_account(session.model()));
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
///
/// An xAI account model is the exception that is not left to the listing: no level is sent to the
/// account whichever model is named ([XAI-16](../../../../docs/specs/xai-sign-in.md#XAI-16)), so it reads
/// none, and that holds for a model the listing offers, one it lists and the rule leaves out, an alias
/// and an id nobody listed, and where the listing could not be read. The run says so in each case,
/// which is what [`bravebot_agent::backend::refused_a_level`] answers for a name of the account.
pub fn adopt_listing_for_model(config: &mut Config, model: &str) -> bool {
    let withheld = bravebot_agent::backend::refused_a_level(config, model);
    let Ok(models) = list_models(config, Some(model)) else {
        return !withheld;
    };
    config.adopt_window(advertised_window(&models, Some(model)));
    config.adopt_inputs(model, advertised_inputs(&models, Some(model)));
    reads_effort(&models, model) && !withheld
}

/// The model other than the session's that answers a turn, whose window is the budget it compacts
/// against (BACKEND-7): the one a `/agent` line's definition names, and otherwise `planModel` for a
/// turn that begins in plan mode (BACKEND-56).
///
/// Settled by the agent's [`answering`](bravebot_agent::backend::answering), the function the turn
/// settles its own model with, so the window is read for the model the turn runs on. A plan model
/// the turn will set aside leaves the session's window with the session's model.
pub(super) fn model_answering_the_turn(
    config: &Config,
    egress: &bravebot_net::Egress,
    started_in: bravebot_agent::PermissionMode,
    task: &Task,
    addressed_model: Option<&str>,
) -> Option<String> {
    use bravebot_agent::backend::{Answering, answering};
    match answering(config, egress, started_in, task, addressed_model.is_some()) {
        Answering::Definition => addressed_model.map(str::to_string),
        Answering::Plan(model) => Some(model),
        Answering::PlanSetAside(_) | Answering::Task => None,
    }
}

/// What a turn is run with: the task told the mode the turn began in, and the configuration whose
/// window is that of the model that answers it (BACKEND-7, BACKEND-56).
///
/// The mode is read here, once. The window is sized from that reading and the task carries it, so
/// the turn begins in the mode its window was sized for even where the key that sets the mode is
/// pressed as the turn opens (MODE-8).
pub(super) fn settle_the_turn(
    config: &Config,
    egress: &bravebot_net::Egress,
    task: Task,
    mode: &bravebot_agent::LiveMode,
    addressed_model: Option<&str>,
) -> (Task, Config) {
    let started_in = mode.get();
    let task = task.beginning_in(started_in);
    let answering = model_answering_the_turn(config, egress, started_in, &task, addressed_model);
    let turn = config_for_turn(config, answering.as_deref());
    (task, turn)
}

/// The configuration a turn is run on: the session's, with the window of the model that answers it
/// where a `/agent` line's definition names one other than the session's (BACKEND-7).
///
/// A copy, so the session's own budget is what it was once the turn is over.
pub(super) fn config_for_turn(config: &Config, addressed_model: Option<&str>) -> Config {
    let mut turn = config.clone();
    if let Some(model) = addressed_model {
        // The session model's window belongs to a different model, so a listing that cannot be
        // fetched falls back to the default rather than keeping it.
        turn.adopt_window(None);
        adopt_listing_for_model(&mut turn, model);
    }
    turn
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

/// Open the model picker as a mode of the session, with its rows still to come.
///
/// The picker is drawn on the next frame and the endpoints are asked on a thread of their own, so
/// `/model` answers at once however long a gateway takes. The rows are taken by
/// [`Session::settle_picking`] when they arrive. A refusal or an unreachable endpoint closes the
/// picker, leaves the model as it was and says so.
///
/// The list is content and the choice is routing. Nothing here is quarantined, because there is no
/// planner context to keep it out of: the names are drawn for a person, and their pick is the
/// endorsement for the request field it lands in.
pub(super) fn open_model_picker(session: &mut Session, config: &Config) {
    let listing = listed_in_the_background(config, session.model());
    open_model_picker_awaiting(session, config, listing);
}

/// [`open_model_picker`] with the answer to the listing stated, so the picker's first frames can be
/// reached without an endpoint.
pub(super) fn open_model_picker_awaiting(
    session: &mut Session,
    config: &Config,
    listing: mpsc::Receiver<crate::model_prompt::Listed>,
) {
    // The model a prompt file named is where the cursor opens (CMD-17). Opening on it chooses
    // nothing: the person still presses Enter on the row.
    let wanted = session.take_model_to_offer();
    session.open_picking(crate::picking::Picking::Model(
        crate::model_prompt::Picker::loading(
            Some(model_in_force(session.model(), config)),
            listing,
        )
        .wanting(wanted),
    ));
}

/// Ask the endpoints for the rows of the picker away from the thread that draws and reads keys.
///
/// A unit test asks on its own thread before this returns, because the account it scripts is
/// answered per thread.
fn listed_in_the_background(
    config: &Config,
    chosen: Option<&str>,
) -> mpsc::Receiver<crate::model_prompt::Listed> {
    let (send, receive) = mpsc::channel();
    let config = config.clone();
    let chosen = chosen.map(str::to_string);
    let ask = move || {
        let listed = list_models_for_picker(&config, chosen.as_deref())
            .map(|PickerRoster { models, problems }| (models, problems));
        // The picker was left before the answer came, so nobody is waiting for it.
        send.send(listed).ok();
    };
    #[cfg(test)]
    ask();
    #[cfg(not(test))]
    drop(thread::spawn(ask));
    receive
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
    session.note_model_is_account(config.chatgpt_model(&chosen.key).is_some());
    if !chosen.reads_effort && session.effort().is_some() {
        session.note(t!(session_effort_not_read));
    }
    // A model the session moves to is sent the level the session began with where it is the
    // account's, which the person is told where it is not the one they chose.
    say_if_kept_for_account(session);
    session.choose_model(chosen.key, config);
}

/// What a unit test scripts for the xAI account's listing, and how often it was asked for.
#[cfg(test)]
pub(super) mod scripted_xai {
    use bravebot_agent::xai::AccountModels;
    use bravebot_aichat::models::Model;
    use std::cell::{Cell, RefCell};

    thread_local! {
        static ANSWER: RefCell<Option<Result<Vec<Model>, ()>>> = const { RefCell::new(None) };
        static ASKED: Cell<usize> = const { Cell::new(0) };
    }

    /// Offer `models` to whatever asks on this thread, and forget how often it was asked.
    pub(in crate::app) fn offer(models: Vec<Model>) {
        ANSWER.with(|scripted| *scripted.borrow_mut() = Some(Ok(models)));
        ASKED.with(|asked| asked.set(0));
    }

    /// Answer that the listing could not be had.
    pub(in crate::app) fn fail_to_list() {
        ANSWER.with(|scripted| *scripted.borrow_mut() = Some(Err(())));
        ASKED.with(|asked| asked.set(0));
    }

    /// How often the account was asked about on this thread since the last script.
    pub(in crate::app) fn asked() -> usize {
        ASKED.with(Cell::get)
    }

    pub(super) fn ask() -> AccountModels {
        ASKED.with(|asked| asked.set(asked.get() + 1));
        ANSWER.with(|scripted| match scripted.borrow().as_ref() {
            Some(Ok(models)) => AccountModels::Offered(models.clone()),
            Some(Err(())) => AccountModels::Unlisted,
            None => AccountModels::Absent,
        })
    }
}

/// What a unit test scripts for the account's listing, and how often it was asked for.
#[cfg(test)]
pub(super) mod scripted_account {
    use bravebot_agent::chatgpt::AccountModels;
    use bravebot_aichat::models::Model;
    use bravebot_aichat::responses::protocol::Refusal;
    use std::cell::{Cell, RefCell};

    /// What the account answers on this thread.
    enum Answer {
        Models(Vec<Model>),
        Unreadable,
        Compiled,
        Refused(Refusal),
    }

    thread_local! {
        static ANSWER: RefCell<Option<Answer>> = const { RefCell::new(None) };
        static ASKED: Cell<usize> = const { Cell::new(0) };
    }

    fn script(answer: Answer) {
        ANSWER.with(|scripted| *scripted.borrow_mut() = Some(answer));
        ASKED.with(|asked| asked.set(0));
    }

    /// Offer `models` to whatever asks on this thread, and forget how often it was asked.
    pub(in crate::app) fn offer(models: Vec<Model>) {
        script(Answer::Models(models));
    }

    /// Answer as a machine nobody signed in to does: nothing is offered and nothing is said.
    pub(in crate::app) fn nobody_signed_in() {
        ANSWER.with(|scripted| *scripted.borrow_mut() = None);
        ASKED.with(|asked| asked.set(0));
    }

    /// Answer that the listing could not be had, so the compiled list stands in for it.
    pub(in crate::app) fn fail_to_list() {
        script(Answer::Compiled);
    }

    /// Answer that nothing can be offered because the sign-in file could not be read.
    pub(in crate::app) fn fail_to_read_the_file() {
        script(Answer::Unreadable);
    }

    /// Answer that the listing was refused in a code that names the account.
    pub(in crate::app) fn refuse(refusal: Refusal) {
        script(Answer::Refused(refusal));
    }

    /// How often the account was asked about on this thread since the last script, or since it
    /// began.
    pub(in crate::app) fn asked() -> usize {
        ASKED.with(Cell::get)
    }

    pub(super) fn ask() -> AccountModels {
        ASKED.with(|asked| asked.set(asked.get() + 1));
        ANSWER.with(|scripted| match scripted.borrow().as_ref() {
            Some(Answer::Models(models)) => AccountModels::Offered(models.clone()),
            Some(Answer::Unreadable) => AccountModels::Unreadable,
            Some(Answer::Compiled) => {
                AccountModels::Compiled(bravebot_aichat::models::compiled_for_account(
                    &bravebot_config::provider::Provider::chatgpt_account(),
                ))
            }
            Some(Answer::Refused(refusal)) => AccountModels::Refused(*refusal),
            None => AccountModels::Absent,
        })
    }
}
