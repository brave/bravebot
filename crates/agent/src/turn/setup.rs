//! What a turn works out before its first round: the routing it precommits, what it runs as, the
//! tools and the model it is offered, and the system prompt put in front of every round.
//!
//! Each is decided from the task, the workspace and the names of the tools on offer. None reads a
//! file's content, labels anything or declassifies anything, which is why they can live apart
//! from the blocks of `one_turn` that read context files and present a tool's result.

use bravebot_aichat::protocol::Tool;
use bravebot_core::delegate::{ADDRESSED, Addressed, Definitions};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Denial, Policy, Routing};
use bravebot_i18n::t;

use super::{FOR_A_PERSON, OPENING, PLANNING, Task, TurnError};
use crate::report::Reporter;
use crate::request_view::{Prompt, Provenance};
use crate::styles::{self, Style};
use crate::tools;
use crate::workspace::Workspace;
use bravebot_config::Config;

/// The routing a turn precommits: every path and name the person gave it, each trusted for
/// having been theirs.
pub(super) fn precommit_routing(task: &Task) -> Routing {
    let mut routing = Routing::new();
    routing.insert_trusted("task", task.prompt.clone());
    for (index, file) in task.files.iter().enumerate() {
        routing.insert_trusted(format!("file_{index}"), file.clone());
    }
    for (index, path) in task.dropped_text.iter().enumerate() {
        routing.insert_trusted(format!("dropped_{index}"), path.clone());
    }
    for (index, attachment) in task.attachments.iter().enumerate() {
        routing.insert_trusted(format!("attachment_{index}"), attachment.path.clone());
    }
    if let Some(name) = &task.addressing {
        routing.insert_trusted(ADDRESSED, name.clone());
    }
    routing
}

/// Read once. A turn nobody is looping arranges its own later look, which is what a request to
/// report a change needs; a tick of a self-paced loop sets the pace of the next one; and a tick
/// the person gave an interval for decides nothing, because their interval already did.
///
/// The caller says whether there is anything to arrange at all, because only it knows: a
/// one-shot run and the desktop bridge each take one turn and exit, and a session running a
/// line this program wrote keeps no loop over it. A wait asked for there is discarded, so the
/// turn is offered no way to ask for one rather than told a watch it cannot have now exists.
///
/// A turn addressed to a definition arranges nothing later either, and arms no watch: what
/// either starts is a turn of the session's planner, holding what the definition took away
/// (ADDRESS-8).
pub(super) fn scheduling_for(task: &Task) -> tools::Scheduling {
    match task.tick {
        _ if task.addressing.is_some() => tools::Scheduling::NoLaterLook,
        Some(tick) if tick.self_paced => tools::Scheduling::PacingALoop,
        Some(_) => tools::Scheduling::TheirInterval,
        None if task.looking_again => tools::Scheduling::ArrangingALook,
        None => tools::Scheduling::NoLaterLook,
    }
}

/// Whether the turn may arm a watch. A definition's addressed turn may not (ADDRESS-8).
pub(super) fn arming_for(task: &Task) -> crate::watch::Arming {
    match task.addressing {
        Some(_) => crate::watch::Arming::Unavailable,
        None => task.arming,
    }
}

/// The tool that says when this turn is asked again is offered to every turn except a tick the
/// person timed, and describes a different job on either side of that. Nothing else changes.
///
/// A delegate is offered what its capabilities reach, minus the five no delegate ever gets, and
/// a way to delegate while it sits above the bottom of the tree. Derived from the set rather
/// than named per kind, so a tool cannot be offered to a run whose gates would refuse it on
/// every call.
///
/// A turn the person addressed to a definition is offered the planner's list less what the
/// kernel narrowed away. Decided here, before the prompt is composed and before anything is
/// sent, so a name matching nothing ends the turn having spent nothing (ADDRESS-5).
///
/// A delegate in a checkout is offered no `lsp`, whatever its kind holds: the session's
/// servers are rooted at the working directory, and a path in the checkout is outside it
/// (CHECKOUT-20).
pub(super) fn offered_tools<S: Sink>(
    task: &Task,
    workspace: &Workspace,
    delegates: &Definitions,
    advisor: Option<&str>,
    scheduling: tools::Scheduling,
    arming: crate::watch::Arming,
    policy: &mut Policy<'_, S>,
) -> Result<(Option<Addressed>, Vec<Tool>), Denial> {
    let Some(spec) = &task.delegate else {
        // What the command line took away is decided before the table is written, for the
        // reason a definition narrowed past `run` is below: a description naming a tool
        // that is no longer beside it sends the planner to a name that is not there.
        let running = match bravebot_core::tool_set::allows("run") {
            true => tools::Running::Offered,
            false => tools::Running::Withheld,
        };
        let mut offered =
            tools::for_planner(scheduling, arming, delegates, task.deadlines, running);
        if advisor.is_some() {
            tools::offer_advisor(&mut offered);
        }
        offered.retain(|tool| bravebot_core::tool_set::allows(&tool.function.name));
        let names: Vec<&str> = offered
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect();
        let addressed = policy.address(&names)?;
        if let Some(addressed) = &addressed {
            // Which names a definition keeps is read after the table is written, so a
            // definition narrowed past `run` leaves descriptions naming a tool that is no
            // longer beside them. Written again against what is left, which costs a second
            // table only in that case.
            if !addressed.tools().iter().any(|tool| tool == "run") {
                offered = tools::for_planner(
                    scheduling,
                    arming,
                    delegates,
                    task.deadlines,
                    tools::Running::Withheld,
                );
                if advisor.is_some() {
                    tools::offer_advisor(&mut offered);
                }
                offered.retain(|tool| bravebot_core::tool_set::allows(&tool.function.name));
            }
            offered.retain(|tool| addressed.tools().contains(&tool.function.name));
        }
        return Ok((addressed, offered));
    };
    let mut offered = tools::for_delegate(
        spec.capabilities(),
        spec.tools(),
        spec.may_delegate().then_some(delegates),
        task.deadlines,
    );
    if workspace.checkout().is_some() {
        offered.retain(|tool| tool.function.name != "lsp");
    }
    Ok((None, offered))
}

/// What a turn runs on and who pinned it there: the model a definition named where the turn is
/// addressed to one, and the command line's or the session's otherwise.
///
/// The definition's model where an addressed one named a model, and no turn at all where that
/// model needs a sign-in this machine has not made. Running it on the session's model instead
/// would spend past a boundary the definition drew (ADDRESS-11).
///
/// Unless the command line named a model, which outranks it. The definition's model is then
/// not asked for, so its sign-in and substitution checks do not apply, and the turn reports
/// that the definition's model was not used.
pub(super) struct ModelForTheTurn {
    /// The definition whose model this turn runs on, where one named a model: the addressed
    /// definition, or the delegate's. A skill loaded in the turn keeps that model (SKILL-15).
    /// Not a definition the command line outranked, whose model this turn does not run on.
    pub(super) pinned_by: Option<String>,
    /// Mutable in the caller because a skill loaded mid-turn may name a model of its own
    /// (SKILL-15).
    pub(super) turn_model: Option<String>,
    /// The model the definition wrote and the one it resolved to, where the definition's model is
    /// the one this turn asked for.
    pub(super) definition_model: Option<(String, String)>,
}

pub(super) fn model_for_the_turn<R: Reporter + ?Sized>(
    config: &Config,
    task: &Task,
    addressed: Option<&Addressed>,
    reporter: &mut R,
    notices: &mut Vec<crate::skills::Notice>,
) -> Result<ModelForTheTurn, TurnError> {
    let named =
        addressed.and_then(|addressed| addressed.model().map(|written| (addressed, written)));
    if let (true, Some((addressed, written))) = (task.model_outranks_a_definition, named) {
        let said = t!(
            agent_model_outranked,
            definition = addressed.name(),
            model = written
        );
        reporter.notice(said.clone());
        notices.push(crate::skills::Notice::from_message(said));
    }
    // A checkout is a delegate's, and this turn is the person's own, so it works in their
    // working directory and says so rather than leave the definition's line reading as applied
    // (CHECKOUT-2).
    if let Some(addressed) = addressed.filter(|a| a.asks_for_checkout()) {
        let said = t!(agent_checkout_not_applied, definition = addressed.name());
        reporter.notice(said.clone());
        notices.push(crate::skills::Notice::from_message(said));
    }
    let definition_model = named
        .filter(|_| !task.model_outranks_a_definition)
        .map(|(_, written)| (written.to_string(), config.model_named(written)));
    // The machine-level layer first, for the reason the sign-in check below is second
    // (BACKEND-48).
    if let (Some(addressed), Some((written, _))) = (addressed, &definition_model)
        && let Some((file, why)) = config.model_refused(written)
    {
        reporter.notice(t!(
            delegate_model_refused,
            definition = addressed.name(),
            model = written,
            reason = crate::backend::refusal_reason(file, why)
        ));
        return Err(TurnError::Precommit(
            "the addressed definition's model is refused by this machine's managed layer"
                .to_string(),
        ));
    }
    if let (Some(addressed), Some((written, resolved))) = (addressed, &definition_model)
        && crate::backend::Backend::needs_sign_in(config, resolved)
    {
        reporter.notice(t!(
            delegate_model_needs_sign_in,
            definition = addressed.name(),
            model = written
        ));
        return Err(TurnError::Precommit(
            "the addressed definition's model needs a sign-in first".to_string(),
        ));
    }
    let pinned_by = match (addressed, &task.delegate) {
        (Some(addressed), _) => definition_model
            .as_ref()
            .map(|_| addressed.name().to_string()),
        (None, Some(spec)) => spec.model().map(|_| spec.definition().to_string()),
        (None, None) => None,
    };
    let turn_model = definition_model
        .as_ref()
        .map(|(_, resolved)| resolved.clone())
        .or_else(|| task.model.clone());
    Ok(ModelForTheTurn {
        pinned_by,
        turn_model,
        definition_model,
    })
}

/// A delegate's is its kind's, and the planner cannot write a word of it: what it chose was a
/// name out of an enumerated set, and the set is the driver's. What both prompts share is the
/// middle of them, which is `PLANNING`.
///
/// The mode goes last of all, after the user's own standing instructions, because it is the more
/// specific thing: a rule about this turn rather than about how work is done here. On both
/// prompts, since a delegate writing files in plan mode would be the mode failing exactly where
/// nobody is watching the writes. Only plan mode says anything: see
/// `PermissionMode::instruction`.
///
/// `started_in` is the mode the turn began in, read once by the caller, which also updates it as
/// the rounds go. `style` is the output style the turn resolved, which may differ from the one the
/// task carries (INSTR-12). `memory` is what a definition's kept memory says, which the caller
/// reads through its policy and so keeps hold of.
pub(super) fn system_prompt(
    task: &Task,
    workspace: &Workspace,
    addressed: Option<&Addressed>,
    style: Option<&Style>,
    preamble: &crate::preamble::Preamble,
    started_in: crate::permission_mode::PermissionMode,
    memory: impl Fn(bool, &str) -> String,
) -> Prompt {
    let mode = started_in.instruction().unwrap_or_default();
    let checkout_notice = workspace
        .checkout()
        .map(crate::delegate::checkout_notice)
        .unwrap_or_default();
    let mut system = Prompt::default();
    match &task.delegate {
        Some(spec) => {
            system.push(
                Provenance::Driver,
                crate::delegate::prompt_for(
                    spec.capabilities(),
                    spec.prompt(),
                    &memory(spec.keeps_memory(), spec.definition()),
                    spec.may_delegate(),
                ),
            );
            system.push(Provenance::Driver, checkout_notice);
            system.extend(preamble.prompt.clone());
        }
        // `for_a_person` names tools only this side is offered, so it sits with the rest of what
        // only this side reads and ahead of `text`: the user's own instructions end that string and
        // have the last word over anything this program says about a machine.
        //
        // An addressed definition's words go after this program's and ahead of the user's own, for
        // the same reason: its file says what this turn is for, and their instructions still have
        // the last word.
        //
        // `--system-prompt` stands in for the opening alone (CLI-19): what follows it is what
        // the quarantine, the goal and the modes rest on.
        None => {
            match (task.system_prompts.replacing.as_deref(), style) {
                (Some(replacing), _) => {
                    system.push(Provenance::Trusted("command line"), replacing.trim())
                }
                // A file's words are the person's own configuration, trusted for sitting in
                // `~/.bravebot`; a built-in style's are this program's.
                (None, Some(style)) if style.origin == styles::Origin::Home => {
                    system.push(Provenance::Trusted("output style"), style.words.as_ref())
                }
                // A project's file the trust map vouched for, as its `AGENTS.md` is.
                (None, Some(style)) if style.origin == styles::Origin::Project => system.push(
                    Provenance::Trusted("project output style"),
                    style.words.as_ref(),
                ),
                (None, Some(style)) => system.push(Provenance::Driver, style.words.as_ref()),
                (None, None) => system.push(Provenance::Driver, OPENING),
            }
            system.push(Provenance::Driver, PLANNING);
            system.push(Provenance::Driver, FOR_A_PERSON);
            system.push(Provenance::Driver, preamble.for_a_person.clone());
            if let Some(addressed) = addressed {
                system.push(
                    Provenance::Trusted("agent definition"),
                    crate::delegate::addressed_prompt(
                        addressed,
                        &memory(addressed.keeps_memory(), addressed.name()),
                    ),
                );
            }
            system.extend(preamble.prompt.clone());
        }
    }
    system.push(Provenance::Driver, mode);
    system
}
