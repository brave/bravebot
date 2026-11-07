//! A plan step that calls one tool of an MCP server a person vouched for (MANIFEST-15).
//!
//! The step names a server by its alias and a tool by a word on a list a person vouched for, and
//! carries the arguments of the call as a JSON object the plan wrote. All three were written by a
//! planner whose context held the task and the driver's own words, so they are the plan's own and
//! are read here as a plan's fields are: compared with the vouched list, and shown to the person
//! in full before the plan is approved.
//!
//! The step is an action. It names no slot, so no argument of a call is taken from what an earlier
//! step produced, and the call fills none. What the tool answers is drawn in the margin every
//! untrusted byte is drawn in and goes to no model. The call itself is [`crate::mcp::call`], the
//! function a turn's call goes through, so a `deny` rule, a standing answer, the question a
//! declaration asking every call puts, the mode and the egress gate decide it here as they do
//! there. This module adds nothing to that decision and takes none of it.
//!
//! A tool its server marks read-only, and that the list the person vouched for says so of, may
//! also be called by a read step. The read is the same call, with the same literal arguments,
//! the same routing and the same question; it is a fetch rather than an action, so it comes
//! before every act, and what the tool answers is quarantined in the slot the step names, where
//! a transform and an answer can use it as they use anything else a fetch read.
//!
//! The lists those tools are on are put to the person before anything is planned, as the start of
//! a turn puts them ([`Servers::settle`]), so a plan started in a session no turn has run in can
//! name a tool at once.

use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::Sink;
use bravebot_core::manifest::{self, Manifest, Step};
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::slot::{SlotId, SlotStore};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::confirm::Confirmer;
use crate::mcp::Offer;
use crate::report::{Reach, Reporter, Shown};
use crate::turn::{Task, TurnError};

/// The tools of the servers a run holds a grant for whose lists a person vouched for.
///
/// Empty for a run whose session has had no list vouched, and [`Servers::settle`] is what puts a
/// list to the person before the run is planned.
#[derive(Debug, Clone, Default)]
pub(crate) struct Servers {
    offer: Option<Offer>,
}

/// What putting the lists to the person cost a run.
#[derive(Debug, Clone, Default)]
pub(crate) struct Settling {
    /// What the checks over the lists spent.
    pub(crate) usage: bravebot_aichat::protocol::Usage,
    /// How long the person kept the run waiting for their answers.
    pub(crate) waited: std::time::Duration,
}

impl Servers {
    /// Put the lists nobody has been asked about, of the servers `task` holds a grant for, to the
    /// person, as the start of a turn does (SERVERS-8, MANIFEST-15).
    ///
    /// Before anything is planned, so the planner is shown the names of a list that was vouched for
    /// and no others. The question, the check that precedes it, the permission mode's answer and a
    /// stop are the turn's own, since it is [`crate::mcp::Session::settle`] that decides them. A
    /// list is asked about once in a session, so a second run is put none it has already answered.
    ///
    /// Under a policy of its own. What the check read is not in the planner's policy, which begins
    /// afterwards holding the task and the driver's own words.
    ///
    /// No server is started and none is called here, apart from a server of the session that
    /// announced a change to its list, which is read again with the bound its declaration gives a
    /// request (SERVERS-15). A list that is not given in time is not offered and nobody is asked
    /// about it, so a plan naming one of its tools is refused for naming a capability that was not
    /// offered.
    #[expect(
        clippy::too_many_arguments,
        reason = "the run's collaborators, as `plan` and `execute` take them"
    )]
    pub(crate) fn settle<S: Sink, C: Confirmer, R: Reporter>(
        task: &Task,
        config: &Config,
        egress: &bravebot_net::Egress,
        confirmer: &mut C,
        reporter: &mut R,
        sink: &mut S,
        subscription: Option<&mut crate::PremiumKey>,
        cancel: &Cancel,
    ) -> Result<Settling, TurnError> {
        // `--tools` names this program's own tools, so a server's are not among them, and no list
        // is put to the person for a server whose tools would not be offered (SERVERS-8).
        let Some(session) = task
            .mcp
            .as_ref()
            .filter(|_| !bravebot_core::tool_set::names_only())
        else {
            return Ok(Settling::default());
        };
        let held = CapabilitySet::from_iter(task.servers.iter().cloned().map(Capability::McpCall));
        let mut routing = Routing::new();
        routing.insert_trusted("task", task.prompt.clone());
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter(std::iter::once(Capability::WebFetch).chain(held.iter())),
            sink,
        )
        .map_err(|d| TurnError::Precommit(d.to_string()))?;
        let mut waiting = crate::confirm::Timed::new(confirmer);
        let settled = session.settle(
            &mut policy,
            &mut crate::processor::Chat::new(config, egress, task.account_session())
                .spending(subscription)
                .on(task.model.as_deref())
                .stoppable_by(cancel),
            &mut waiting,
            reporter,
            task.permission_mode.get(),
            &held,
        );
        let _ = policy.finish();
        Ok(Settling {
            usage: settled.usage,
            waited: waiting.waited(),
        })
    }

    /// The tools `task`'s session offers, as a turn would offer them.
    pub(crate) fn of(task: &Task) -> Self {
        // `--tools` names this program's own tools, so a server's are not among them (SERVERS-8).
        if bravebot_core::tool_set::names_only() {
            return Self::default();
        }
        let held = CapabilitySet::from_iter(task.servers.iter().cloned().map(Capability::McpCall));
        Self {
            offer: task
                .mcp
                .as_ref()
                .map(|session| session.offer().holding(&held))
                .filter(|offer| !offer.is_empty()),
        }
    }

    /// Whether any tool is offered, which decides whether the planner is told a call is a step.
    pub(crate) fn any(&self) -> bool {
        self.offer.is_some()
    }

    /// The servers of a run that is offered `offer`, for a test of what the planner is told.
    #[cfg(test)]
    pub(crate) fn offering(offer: crate::mcp::Offer) -> Self {
        Self { offer: Some(offer) }
    }

    /// Whether any offered tool is marked read-only, which decides whether the planner is told a
    /// read is a step.
    pub(crate) fn any_read_only(&self) -> bool {
        self.offer
            .as_ref()
            .is_some_and(|offer| offer.callable().iter().any(|tool| tool.read_only))
    }

    /// Whether any offered tool has an argument its server's declaration lets a row of an `apply`
    /// step fill, which decides whether the planner is told an apply is a step.
    pub(crate) fn any_applicable(&self) -> bool {
        self.offer.as_ref().is_some_and(|offer| {
            offer
                .callable()
                .iter()
                .any(|tool| offer.apply_arguments(&tool.alias, &tool.tool).is_some())
        })
    }

    /// The tools a plan may call, one per line: `alias:tool`, then each argument by name, `*` after
    /// a required one, and its type where the list gave one.
    pub(crate) fn listing(&self) -> String {
        let Some(offer) = &self.offer else {
            return String::new();
        };
        let mut lines = String::new();
        for tool in offer.callable() {
            let arguments: Vec<String> = tool
                .arguments
                .iter()
                .map(|argument| {
                    let required = if argument.required { "*" } else { "" };
                    match &argument.kind {
                        Some(kind) => format!("{}{required}: {kind}", argument.name),
                        None => format!("{}{required}", argument.name),
                    }
                })
                .collect();
            let applies = offer
                .apply_arguments(&tool.alias, &tool.tool)
                .map(|kinds| {
                    let kinds: Vec<String> = kinds
                        .iter()
                        .map(|(name, kind)| format!("{name}: {kind}"))
                        .collect();
                    format!(" apply ({})", kinds.join(", "))
                })
                .unwrap_or_default();
            lines.push_str(&format!(
                "  {}:{} ({}){}{applies}\n",
                tool.alias,
                tool.tool,
                arguments.join(", "),
                if tool.read_only { " read-only" } else { "" }
            ));
        }
        lines
    }

    /// What the fit call is shown beside the capabilities, where a tool is offered.
    pub(crate) fn catalogue(&self) -> String {
        match self.any() {
            false => String::new(),
            true => format!(
                "\nTools a SERVER_CALL step may call, as alias:tool with its arguments (* is \
                 required). A call is an action, so it comes after every read and transform. What \
                 it returns is shown to the person as the call answers, so a task that asks only \
                 to see a tool's answer is done by the call. It fills no slot, so no later step \
                 can read, transform or answer with what it returned. Never guess the name of a \
                 folder, file or other thing the task does not give: use a tool that needs no \
                 name, or leave an optional name out.{}{}",
                match self.any_read_only() {
                    true =>
                        " A tool marked read-only may instead be called by a SERVER_READ \
                             step, which is a read: it puts what the tool returns in its \
                             out_slot, for a TRANSFORM or an ANSWER to use.",
                    false => "",
                },
                match self.any_applicable() {
                    true => APPLY_CATALOGUE,
                    false => "",
                }
                .to_string()
                    + "\n"
                    + &self.listing()
            ),
        }
    }

    /// The capability to call each server a plan names, and no other.
    pub(crate) fn grants(plan: &Manifest) -> Vec<Capability> {
        plan.steps()
            .iter()
            .filter(|step| manifest::is_server_call(step.tool()) || step.tool() == manifest::APPLY)
            .map(|step| Capability::McpCall(ServerAlias::new(step.text("server"))))
            .collect()
    }

    /// Hold every call in `plan` to the list a person vouched for, before the plan is put to
    /// anybody: the tool is on the list, the arguments are a JSON object, each name in it is an
    /// argument the tool lists, and each argument it requires is there. A read names a tool the
    /// list marks read-only.
    ///
    /// Read from the plan, which is trusted, and compared with a list a person vouched for. Nothing
    /// a server answered takes part.
    pub(crate) fn check(&self, plan: &Manifest) -> Result<(), String> {
        for (index, step) in plan.steps().iter().enumerate() {
            if step.tool() == manifest::APPLY {
                self.check_apply(plan, index, step)?;
                continue;
            }
            if !manifest::is_server_call(step.tool()) {
                continue;
            }
            let number = index + 1;
            let (alias, word) = (step.text("server"), step.text("tool"));
            let name = format!("{alias}:{word}");
            let offered = self.offer.as_ref().map(Offer::callable).unwrap_or_default();
            let Some(tool) = offered
                .iter()
                .find(|tool| tool.alias == alias && tool.tool == word)
            else {
                let tools: Vec<String> = offered
                    .iter()
                    .map(|tool| format!("{}:{}", tool.alias, tool.tool))
                    .collect();
                return Err(format!(
                    "step {number}: {name} is not a tool a person vouched for; the tools are {}",
                    tools.join(", ")
                ));
            };
            if step.tool() == manifest::MCP_READ && !tool.read_only {
                return Err(format!(
                    "step {number}: {name} is not marked read-only, so only SERVER_CALL may call it"
                ));
            }
            let arguments = arguments_of(step).map_err(|_| {
                format!("step {number}: the arguments of {name} must be a JSON object")
            })?;
            if let Some(unknown) = arguments
                .keys()
                .find(|key| !tool.arguments.iter().any(|argument| &argument.name == *key))
            {
                return Err(format!(
                    "step {number}: {name} takes no argument '{unknown}'"
                ));
            }
            if let Some(missing) = tool
                .arguments
                .iter()
                .find(|argument| argument.required && !arguments.contains_key(&argument.name))
            {
                return Err(format!(
                    "step {number}: {name} requires the argument '{}'",
                    missing.name
                ));
            }
        }
        Ok(())
    }

    /// The words after each call on the line a person approves: its arguments, as the server will
    /// receive them, in full. Compact JSON, so a line break or a control character in a value is
    /// escaped and the line stays one line. Decides nothing.
    pub(crate) fn notes(plan: &Manifest) -> BTreeMap<usize, String> {
        plan.steps()
            .iter()
            .enumerate()
            .filter_map(|(index, step)| match step.tool() {
                manifest::APPLY => shape_of(step).ok().map(|shape| {
                    let tools: Vec<String> = shape
                        .tools
                        .iter()
                        .map(|tool| {
                            let arguments: Vec<String> = tool
                                .arguments
                                .iter()
                                .map(|(name, kind)| {
                                    format!("{name}: {}", kind_in_words(kind, &shape))
                                })
                                .collect();
                            format!("{} ({})", tool.name, arguments.join(", "))
                        })
                        .collect();
                    (
                        index,
                        format!(
                            "{}; up to {} calls and {} new names, all put to you as one list \
                             first; the processor is told \"{}\"",
                            tools.join(", then "),
                            shape.max_rows,
                            shape.max_new,
                            step.text("instruction")
                        ),
                    )
                }),
                tool if manifest::is_server_call(tool) => arguments_of(step)
                    .ok()
                    .map(|arguments| (index, format!("arguments {}", Value::Object(arguments)))),
                _ => None,
            })
            .collect()
    }

    /// Make the call a step names, through the one function a turn's call goes through. The
    /// alias and tool come out of the routing lock, and the arguments out of the plan.
    /// `plan_approved` says a person approved the plan, outside plan mode, so the plan's approval
    /// answers the call's default question (MANIFEST-15).
    ///
    /// The note for the step's line, or the reason the run stops here. A call that is refused,
    /// declined, not asked about, not made, or that no reply arrived to, ends the run, and the
    /// steps after it do not run. A reply that arrived ends nothing, whatever it holds: what the
    /// tool answered and what it said about its own failure is drawn in the margin and goes nowhere
    /// else, and a reply this client could not use is noted as that and draws nothing. None of them
    /// decides whether a later step runs: the steps were fixed before the server was reached, and
    /// what a server returns is not what changes them.
    ///
    /// A read is given the slot its step names, and what the tool answered goes there instead,
    /// quarantined as anything a fetch reads is. Whatever reply arrived fills it, so a read goes
    /// on or ends by whether a reply arrived and never by which kind (MANIFEST-15): a failure the
    /// tool reported is drawn in the margin and its words fill the slot, and a reply this client
    /// could not use fills it with this process's own sentence saying so.
    #[expect(
        clippy::too_many_arguments,
        reason = "forwarded to `mcp::call`, which takes the same list"
    )]
    pub(crate) fn call<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        egress: &bravebot_net::Egress,
        confirmer: &mut C,
        reporter: &mut R,
        alias: &str,
        word: &str,
        step: &Step,
        into: Option<(SlotId, &mut SlotStore)>,
        plan_approved: bool,
    ) -> Result<String, String> {
        let name = format!("{alias}:{word}");
        let Some(offer) = &self.offer else {
            return Err(format!("{name} is not offered to this run"));
        };
        let arguments = arguments_of(step)
            .map_err(|_| format!("the arguments of {name} must be a JSON object"))?;
        let (text, origin, left_out, failed) = match crate::mcp::call(
            policy,
            offer,
            egress,
            confirmer,
            reporter,
            alias,
            word,
            &Value::Object(arguments),
            None,
            plan_approved,
        ) {
            crate::mcp::Called::Answered(output) => (
                output.text,
                crate::mcp::origin_of_reply(&name, false),
                output.pictures.len() + output.left_out,
                false,
            ),
            crate::mcp::Called::Failed(text) => {
                (text, crate::mcp::origin_of_reply(&name, true), 0, true)
            }
            // Nothing of the reply is kept, so there is nothing to draw, and a read's slot holds
            // this process's own words that the reply could not be used.
            crate::mcp::Called::Unusable(_) => {
                if let Some((slot, slots)) = into {
                    let said = bravebot_core::value::Labelled::trusted(format!(
                        "{name} replied, and its reply could not be used"
                    ));
                    policy
                        .quarantine(step.tool(), slot.clone(), &name, &said, slots)
                        .map_err(|d| d.to_string())?;
                    return Ok(format!(
                        "{name} failed: its reply could not be used, which is what {slot} holds"
                    ));
                }
                return Ok(format!("{name} failed: its reply could not be used"));
            }
            crate::mcp::Called::Problem(problem) => return Err(problem),
        };
        let parts = crate::tools::tally(left_out, "picture or part", "pictures or parts");
        match into {
            Some((slot, slots)) if !failed => {
                policy
                    .quarantine(step.tool(), slot.clone(), &name, &text, slots)
                    .map_err(|d| d.to_string())?;
                Ok(format!("{name} replied into {slot}; {parts} left out"))
            }
            into => {
                let shown = crate::turn::preview_for(policy, step.tool(), &text);
                reporter.quarantined(Shown {
                    origin,
                    reach: Reach::NoModel,
                    label: text.label().to_string(),
                    lines: shown.lines,
                    preview: shown.preview,
                });
                match into {
                    None => Ok(format!("{name} replied; {parts} not shown")),
                    Some((slot, slots)) => {
                        policy
                            .quarantine(step.tool(), slot.clone(), &name, &text, slots)
                            .map_err(|d| d.to_string())?;
                        Ok(format!(
                            "{name} reported a failure, shown above, and its words are in {slot}"
                        ))
                    }
                }
            }
        }
    }
}

/// What the planner is told an apply step is, where an offered tool lets a row fill an argument.
const APPLY_CATALOGUE: &str = " A tool listed with apply may be called by an APPLY step for each \
item of a list a SERVER_READ step read: an isolated processor proposes the calls, row by row, and \
the person approves the whole list before any is made. Its arguments give each argument of each \
tool a kind: a fixed value {\"literal\": \"text\"}, a field of an entry {\"entry\": \"field\"} \
(of the first list, or {\"entry\": \"list.field\"} of another), that field of several entries as \
one array {\"entries\": \"field\"} for an argument the tool takes as a list, new text for a value \
the call sets \"new\", or the value an earlier row of another tool gave \
{\"row\": \"tool.argument\"}, or a list of those kinds where the tool is listed with several, \
any one of which a row may give. Only the kinds the tool is listed with may be used, besides \
literal.";

impl Servers {
    /// Hold an apply step to the list a person vouched for and to the server's declaration: the
    /// server is offered and does not ask every call, each tool is on its list, every argument the
    /// tool requires is given a kind and no other argument is, and each kind but a literal is one
    /// the declaration names for that argument. Read from the plan alone.
    fn check_apply(&self, plan: &Manifest, index: usize, step: &Step) -> Result<(), String> {
        let number = index + 1;
        let alias = step.text("server");
        let Some(offer) = &self.offer else {
            return Err(format!(
                "step {number}: no server's tools are offered to this run"
            ));
        };
        let shape = shape_of(step)?;
        // A step whose every argument is the plan's literal fills nothing from the list, and only
        // a declaration lets an argument take anything else. No server asked about every call
        // holds one (SERVERS-26), so this also keeps such a server's calls each put on its own.
        let only_literals = shape.tools.iter().all(|tool| {
            tool.arguments
                .iter()
                .all(|(_, kind)| matches!(kind, bravebot_core::apply::Kind::Literal(_)))
        });
        if only_literals {
            return Err(format!(
                "step {number}: an APPLY step gives at least one argument a kind other than \
                 literal, which the declaration of {alias} lets a row fill"
            ));
        }
        // A list is what a fetch returned, never what a model wrote, so every entry a row names is
        // something the run read.
        for list in &shape.lists {
            let fetched = plan.steps()[..index].iter().any(|earlier| {
                earlier.tier() == manifest::Tier::Fetch
                    && earlier.out_slot().is_some_and(|slot| slot.as_str() == list)
            });
            if !fetched {
                return Err(format!(
                    "step {number}: the list {list} was not filled by a read, so an APPLY step \
                     cannot point into it"
                ));
            }
        }
        let offered = offer.callable();
        for tool in &shape.tools {
            let name = format!("{alias}:{}", tool.name);
            let Some(listed) = offered
                .iter()
                .find(|listed| listed.alias == alias && listed.tool == tool.name)
            else {
                return Err(format!(
                    "step {number}: {name} is not a tool a person vouched for"
                ));
            };
            let declared = offer.apply_arguments(alias, &tool.name);
            for (argument, kind) in &tool.arguments {
                if !listed.arguments.iter().any(|known| &known.name == argument) {
                    return Err(format!(
                        "step {number}: {name} takes no argument '{argument}'"
                    ));
                }
                if matches!(kind, bravebot_core::apply::Kind::Literal(_)) {
                    continue;
                }
                let words = declared_words(declared.and_then(|kinds| kinds.get(argument)));
                let wanted = kind.word();
                if let Some(missing) = wanted.split(',').find(|word| !words.contains(word)) {
                    return Err(format!(
                        "step {number}: the declaration of {alias} does not let a row give \
                         {name}'s '{argument}' as {missing}"
                    ));
                }
            }
            if let Some(missing) = listed.arguments.iter().find(|known| {
                known.required && !tool.arguments.iter().any(|(given, _)| given == &known.name)
            }) {
                return Err(format!(
                    "step {number}: {name} requires the argument '{}'",
                    missing.name
                ));
            }
        }
        Ok(())
    }

    /// Run an apply step: number the list for a processor, ask it for rows, put the calls they
    /// resolve to to the person as one list, and make the ones they keep, in order.
    ///
    /// The note for the step's line, or the reason the run stops here. A list nobody could be
    /// asked about, one the person declined, or a call refused or not made ends the run. A call
    /// whose tool reports a failure, by the protocol's own `isError`, ends nothing, and the calls
    /// that depend on it are not made.
    #[expect(
        clippy::too_many_lines,
        reason = "the step in order: its lists numbered, its rows proposed and resolved, the list put, and each call made and drawn"
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "the policy, chat, egress, people to ask, slots and the step one apply needs"
    )]
    pub(crate) fn apply<S: Sink, C: Confirmer, R: Reporter>(
        &self,
        policy: &mut Policy<'_, S>,
        chat: &mut crate::processor::Chat<'_>,
        egress: &bravebot_net::Egress,
        confirmer: &mut C,
        reporter: &mut R,
        slots: &mut SlotStore,
        index: usize,
        alias: &str,
        step: &Step,
    ) -> Result<(String, bravebot_aichat::protocol::Usage), String> {
        let Some(offer) = &self.offer else {
            return Err(format!("{alias} is not offered to this run"));
        };
        let mut shape = shape_of(step)?;
        // The type the vouched list gives each argument, which the resolver sends each value as.
        let callable = offer.callable();
        for tool in &mut shape.tools {
            let listed = callable
                .iter()
                .find(|listed| listed.alias == alias && listed.tool == tool.name);
            tool.types = listed
                .map(|listed| {
                    listed
                        .arguments
                        .iter()
                        .map(|argument| {
                            let declared = bravebot_core::apply::Declared {
                                kind: argument.kind.clone(),
                                items: argument.items.clone(),
                            };
                            (argument.name.clone(), declared)
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        // Each list is numbered for the processor under its own name, from the array at the
        // pointer the plan gave it, and the processor reads those numbered lists and nothing else.
        let names = shape.lists.clone();
        let pointers = pointers_of(step.text("entries"), &names)?;
        let mut lists = Vec::new();
        let mut numbered = Vec::new();
        for (at, name) in names.iter().enumerate() {
            let into = SlotId::new(format!("{name}_entries_{index}"));
            lists.push(
                bravebot_choice::number_a_list(
                    policy,
                    &SlotId::new(name),
                    &pointers[at],
                    &shape.entry_fields(at),
                    &into,
                    slots,
                )
                .map_err(|denial| denial.to_string())?,
            );
            numbered.push(into);
        }
        let named = names.join(", ");
        let instruction =
            bravebot_core::value::Labelled::trusted(step.text("instruction").to_string());
        let spec = policy
            .before_proposer(&format!("step_{index}"), &numbered, &instruction, slots)
            .map_err(|denial| denial.to_string())?;
        // A reply with no content at all is asked for once more, as a process step's is: an empty
        // turn from the model is not an answer, and the rows it would have proposed decide nothing
        // until the person sees them.
        // A second empty reply is the processor proposing nothing, which ends the step with no
        // call made rather than the run: a list that holds nothing the rows can take is answered
        // with nothing.
        let mut retried = false;
        let proposed = loop {
            match crate::processor::propose(policy, chat, slots, &spec, &lists, &shape) {
                Ok(proposed) => break proposed,
                Err(crate::processor::ProcessorError::Chat(error)) if error.is_empty_reply() => {
                    if retried {
                        return Ok((
                            format!(
                                "the processor gave no answer, twice ({error}), so no call to \
                                 {alias} was proposed"
                            ),
                            error.completed_usage().unwrap_or_default(),
                        ));
                    }
                    retried = true;
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        let usage = proposed.usage;
        let resolved = proposed.resolved;
        let shown = policy.proposal_for_display(&resolved);
        // No row at all is the processor saying nothing in the list needed a call, as it is told
        // to say it; rows that were all dropped are a proposal nothing could be made of.
        if let Some(dropped) = policy.proposes_nothing(&resolved) {
            let note = match dropped {
                false => format!("the processor found nothing in {named} that needed a call"),
                true => format!(
                    "the processor proposed no call that could be made{}",
                    dropped_note(&shown)
                ),
            };
            return Ok((note, usage));
        }
        let request = crate::confirm::ApplyRequest {
            alias: alias.to_string(),
            list: named.clone(),
            calls: shown
                .calls
                .iter()
                .map(|call| crate::confirm::ApplyCall {
                    tool: call.name.clone(),
                    arguments: call
                        .arguments
                        .iter()
                        .map(|(name, value, source)| {
                            (name.clone(), value.clone(), described(source, &names))
                        })
                        .collect(),
                    after: call.after,
                })
                .collect(),
            dropped: shown
                .dropped
                .iter()
                .map(|(why, count)| (why.describe().to_string(), *count))
                .collect(),
        };
        let decision = confirmer.confirm_apply(&request);
        if decision.unasked {
            return Err(format!(
                "the calls to {alias} are put to the person as one list, and nobody could be asked"
            ));
        }
        // Which calls are kept, and which a failure skips, are worked out in the policy layer from
        // the labelled list; the driver makes the calls it is handed, in order.
        let mut applying = policy
            .keep_applied(alias, &resolved, &decision.kept)
            .map_err(|denial| denial.message)?;
        while let Some(at) = applying.next_call() {
            let called = crate::mcp::call_applied(
                policy, offer, egress, confirmer, reporter, alias, at, &resolved,
            );
            match called {
                crate::mcp::Called::Answered(_) => applying.made(at),
                crate::mcp::Called::Failed(text) => {
                    applying.failed(at);
                    let name = format!("{alias}:{}", shown.calls[at].name);
                    let preview = crate::turn::preview_for(policy, manifest::APPLY, &text);
                    reporter.quarantined(Shown {
                        origin: crate::mcp::origin_of_reply(&name, true),
                        reach: Reach::NoModel,
                        label: text.label().to_string(),
                        lines: preview.lines,
                        preview: preview.preview,
                    });
                }
                crate::mcp::Called::Unusable(_) => applying.failed(at),
                crate::mcp::Called::Problem(problem) => {
                    show_what_came(policy, reporter, alias, &resolved, applying.came());
                    return Err(problem);
                }
            }
        }
        let (kept, made, failed, skipped) = applying.tally();
        show_what_came(policy, reporter, alias, &resolved, applying.came());
        Ok((
            format!(
                "made {made} of {kept} calls to {alias}; {failed} failed, {skipped} skipped as \
                 depending on one that failed{}",
                dropped_note(&shown)
            ),
            usage,
        ))
    }
}

use bravebot_core::apply::Came;

/// Draw every call of the list with what became of it, so the person sees what the step did as a
/// turn's calls show it. The values are the ones the question showed, released for display there,
/// so they are drawn behind the margin untrusted content is drawn behind and reach no model.
fn show_what_came<S: Sink, R: Reporter>(
    policy: &mut Policy<'_, S>,
    reporter: &mut R,
    alias: &str,
    resolved: &bravebot_core::value::Labelled<bravebot_core::apply::Resolved>,
    came: &[Came],
) {
    // Through the same display release the question used, which the trail records.
    let shown = policy.proposal_for_display(resolved);
    let preview: Vec<String> = shown
        .calls
        .iter()
        .zip(came)
        .map(|(call, came)| {
            let arguments: Vec<String> = call
                .arguments
                .iter()
                .map(|(name, value, _)| format!("{name}: {value}"))
                .collect();
            let (mark, said) = match came {
                Came::Made => ("✓", "made"),
                Came::Failed => ("✗", "failed"),
                Came::Skipped => ("↷", "skipped, as it depended on one that failed"),
                Came::LeftOut => ("·", "not made: left out of the list"),
                Came::Unmade => ("·", "not made: the run stopped"),
            };
            format!("{mark} {} ({}) {said}", call.name, arguments.join(", "))
        })
        .collect();
    reporter.quarantined(Shown {
        origin: format!("what became of the calls to {alias} you approved"),
        reach: Reach::NoModel,
        label: resolved.label().to_string(),
        lines: preview.len(),
        preview,
    });
}

/// Where a value of a resolved call came from, in this process's own words.
fn described(source: &bravebot_core::apply::Source, lists: &[String]) -> String {
    use bravebot_core::apply::Source;
    let name = |list: &usize| lists.get(*list).map_or("the list", String::as_str);
    match source {
        Source::Plan => "the plan".to_string(),
        Source::Entry { list, number } => format!("entry {number} of {}", name(list)),
        Source::Entries { list, numbers } => {
            let numbers: Vec<String> = numbers.iter().map(usize::to_string).collect();
            format!("entries {} of {}", numbers.join(", "), name(list))
        }
        Source::Processor => "the processor".to_string(),
        Source::Call(at) => format!("call {}", at + 1),
    }
}

/// Where the list is in each list's reply, as the plan's `entries` gives it: nothing for the reply
/// itself, one JSON pointer for every list, or an object naming a pointer for each list by name.
fn pointers_of(entries: &str, names: &[String]) -> Result<Vec<String>, String> {
    let entries = entries.trim();
    if !entries.starts_with('{') {
        return Ok(vec![entries.to_string(); names.len()]);
    }
    let Ok(Value::Object(by_list)) = serde_json::from_str::<Value>(entries) else {
        return Err("the entries of an APPLY step must be a pointer or an object of them".into());
    };
    if let Some(unknown) = by_list.keys().find(|key| !names.contains(key)) {
        return Err(format!(
            "the entries of an APPLY step name no list {unknown}"
        ));
    }
    Ok(names
        .iter()
        .map(|name| {
            by_list
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect())
}

/// How many rows were dropped, for the step's line, or nothing where none was.
fn dropped_note(resolved: &bravebot_core::apply::Resolved) -> String {
    let count: usize = resolved.dropped.values().sum();
    match count {
        0 => String::new(),
        _ => format!("; {count} proposed rows dropped"),
    }
}

/// The shape an apply step's fields give: its tools in the order `tools` names them, and for each
/// argument the kind its `arguments` object gives it. Read from the plan's own text.
pub(crate) fn shape_of(step: &Step) -> Result<bravebot_core::apply::Shape, String> {
    use bravebot_core::apply::{Shape, Tool};
    let lists: Vec<String> = step.reads().iter().map(ToString::to_string).collect();
    if lists.is_empty() {
        return Err("an APPLY step names at least one list".to_string());
    }
    let names: Vec<&str> = step
        .text("tools")
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    let Ok(Value::Object(by_tool)) = serde_json::from_str::<Value>(step.text("arguments")) else {
        return Err("the arguments of an APPLY step must be a JSON object of tools".to_string());
    };
    if names.is_empty() || by_tool.keys().any(|key| !names.contains(&key.as_str())) {
        return Err(
            "the arguments of an APPLY step name each of its tools and no other".to_string(),
        );
    }
    let mut tools: Vec<Tool> = Vec::new();
    for name in &names {
        let mut arguments = Vec::new();
        if let Some(given) = by_tool.get(*name) {
            let Value::Object(given) = given else {
                return Err(format!("the arguments of {name} must be a JSON object"));
            };
            for (argument, kind) in given {
                let kind = kind_of(name, argument, kind, &tools, &lists, true)?;
                arguments.push((argument.clone(), kind));
            }
        }
        // A call depends on at most one earlier call, so unticking or failing that one is all that
        // can leave it without a value.
        let rows = arguments
            .iter()
            .filter(|(_, kind)| takes_a_row(kind))
            .count();
        if rows > 1 {
            return Err(format!(
                "{name} takes a row in more than one argument, and a call may depend on one \
                 earlier call only"
            ));
        }
        tools.push(Tool {
            types: Default::default(),
            name: (*name).to_string(),
            arguments,
        });
    }
    let cap = |field: &str, default: usize, most: usize| {
        step.count(field).map_or(default, |count| {
            usize::try_from(count).unwrap_or(most).min(most)
        })
    };
    Ok(Shape {
        lists,
        tools,
        max_rows: cap("max_rows", 50, 100),
        max_new: cap("max_new", 12, 50),
    })
}

/// Whether a row may give this argument the new text of an earlier row.
fn takes_a_row(kind: &bravebot_core::apply::Kind) -> bool {
    use bravebot_core::apply::Kind;
    match kind {
        Kind::Row { .. } => true,
        Kind::Either(kinds) => kinds.iter().any(takes_a_row),
        _ => false,
    }
}

/// The kind the plan gives `argument` of `name`: `"new"`, `{"literal": text}`, `{"entry": field}`,
/// `{"row": "tool.argument"}` naming new text an earlier tool of the step gives, or, where
/// `choice` is allowed, a list of the kinds but a literal, any one of which a row may give.
fn kind_of(
    name: &str,
    argument: &str,
    value: &Value,
    tools: &[bravebot_core::apply::Tool],
    lists: &[String],
    choice: bool,
) -> Result<bravebot_core::apply::Kind, String> {
    use bravebot_core::apply::Kind;
    let none = || format!("{name}'s '{argument}' has no kind it can take");
    // `field` names a field of the first list, and `list.field` one of the list of that name.
    let of_list = |value: &str| -> Result<(usize, String), String> {
        match value.split_once('.') {
            Some((list, field)) if lists.iter().any(|known| known == list) => Ok((
                lists.iter().position(|known| known == list).unwrap_or(0),
                field.to_string(),
            )),
            _ => Ok((0, value.to_string())),
        }
    };
    match value {
        Value::String(word) if word == "new" => Ok(Kind::New),
        Value::Array(kinds) if choice && kinds.len() > 1 => {
            let kinds = kinds
                .iter()
                .map(|kind| kind_of(name, argument, kind, tools, lists, false))
                .collect::<Result<Vec<_>, _>>()?;
            match kinds.iter().any(|kind| matches!(kind, Kind::Literal(_))) {
                true => Err(none()),
                false => Ok(Kind::Either(kinds)),
            }
        }
        Value::Object(object) if object.len() == 1 => match object.iter().next() {
            Some((key, Value::String(value))) if key == "literal" => {
                Ok(Kind::Literal(value.clone()))
            }
            Some((key, value @ (Value::Number(_) | Value::Bool(_)))) if key == "literal" => {
                Ok(Kind::Literal(value.to_string()))
            }
            Some((key, Value::String(value))) if key == "entry" => {
                let (list, field) = of_list(value)?;
                Ok(Kind::Entry { list, field })
            }
            Some((key, Value::String(value))) if key == "entries" => {
                let (list, field) = of_list(value)?;
                Ok(Kind::Entries { list, field })
            }
            Some((key, Value::String(value))) if key == "row" => {
                let (tool, field) = value.split_once('.').ok_or_else(|| {
                    format!("{name}'s '{argument}': a row is written tool.argument")
                })?;
                let at = tools
                    .iter()
                    .position(|earlier| earlier.name == tool)
                    .ok_or_else(|| {
                        format!(
                            "{name}'s '{argument}' names a row of {tool}, which is not an earlier \
                             tool of the step"
                        )
                    })?;
                let made = tools[at]
                    .arguments
                    .iter()
                    .any(|(earlier, kind)| earlier == field && *kind == Kind::New);
                if !made {
                    return Err(format!(
                        "{name}'s '{argument}' names {tool}'s '{field}', which is not new text"
                    ));
                }
                Ok(Kind::Row {
                    tool: at,
                    argument: field.to_string(),
                })
            }
            _ => Err(none()),
        },
        _ => Err(none()),
    }
}

/// The words of the kinds a declaration names for an argument, one word or several joined by
/// commas.
fn declared_words(declared: Option<&String>) -> Vec<&str> {
    declared
        .map(|words| words.split(',').map(str::trim).collect())
        .unwrap_or_default()
}

/// How a kind reads on the line a person approves, in this process's own words.
fn kind_in_words(kind: &bravebot_core::apply::Kind, shape: &bravebot_core::apply::Shape) -> String {
    use bravebot_core::apply::Kind;
    match kind {
        // A number or a switch is sent as one, so it reads as one; text reads quoted.
        Kind::Literal(value) => match serde_json::from_str::<Value>(value) {
            Ok(Value::Number(_) | Value::Bool(_)) => value.clone(),
            _ => format!("\"{value}\""),
        },
        Kind::Entry { list, field } => format!(
            "the {field} of an entry of {}",
            shape.lists.get(*list).map_or("the list", String::as_str)
        ),
        Kind::Entries { list, field } => format!(
            "the {field} of each of several entries of {}",
            shape.lists.get(*list).map_or("the list", String::as_str)
        ),
        Kind::New => "a new name the processor writes".to_string(),
        Kind::Row { tool, argument } => format!(
            "the {argument} given by an earlier {}",
            shape
                .tools
                .get(*tool)
                .map_or("call", |tool| tool.name.as_str())
        ),
        Kind::Either(kinds) => kinds
            .iter()
            .map(|kind| kind_in_words(kind, shape))
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

/// The arguments a call step carries: the JSON object its text holds, or an empty one where the
/// plan gave none.
fn arguments_of(step: &Step) -> Result<Map<String, Value>, ()> {
    let text = step.text("arguments");
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(arguments)) => Ok(arguments),
        _ => Err(()),
    }
}
