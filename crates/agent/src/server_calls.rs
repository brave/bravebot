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
//! The lists those tools are on are put to the person before anything is planned, as the start of
//! a turn puts them ([`Servers::settle`]), so a plan started in a session no turn has run in can
//! name a tool at once.

use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::Sink;
use bravebot_core::manifest::{self, Manifest, Step};
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
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
            lines.push_str(&format!(
                "  {}:{} ({})\n",
                tool.alias,
                tool.tool,
                arguments.join(", ")
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
                 required). A call is an action, so it comes after every read and transform. It \
                 fills no slot and no step can name what it returned.\n{}",
                self.listing()
            ),
        }
    }

    /// The capability to call each server a plan names, and no other.
    pub(crate) fn grants(plan: &Manifest) -> Vec<Capability> {
        plan.steps()
            .iter()
            .filter(|step| step.tool() == manifest::MCP_CALL)
            .map(|step| Capability::McpCall(ServerAlias::new(step.text("server"))))
            .collect()
    }

    /// Hold every call in `plan` to the list a person vouched for, before the plan is put to
    /// anybody: the tool is on the list, the arguments are a JSON object, each name in it is an
    /// argument the tool lists, and each argument it requires is there.
    ///
    /// Read from the plan, which is trusted, and compared with a list a person vouched for. Nothing
    /// a server answered takes part.
    pub(crate) fn check(&self, plan: &Manifest) -> Result<(), String> {
        for (index, step) in plan.steps().iter().enumerate() {
            if step.tool() != manifest::MCP_CALL {
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
            .filter(|(_, step)| step.tool() == manifest::MCP_CALL)
            .filter_map(|(index, step)| {
                arguments_of(step)
                    .ok()
                    .map(|arguments| (index, format!("arguments {}", Value::Object(arguments))))
            })
            .collect()
    }

    /// Make the call a step names, through the one function a turn's call goes through. The
    /// alias and tool come out of the routing lock, and the arguments out of the plan.
    ///
    /// The note for the step's line, or the reason the run stops here. A call that is refused,
    /// declined, not asked about, not made, or that no reply arrived to, ends the run, and the
    /// steps after it do not run. A reply that arrived ends nothing, whatever it holds: what the
    /// tool answered and what it said about its own failure is drawn in the margin and goes nowhere
    /// else, and a reply this client could not use is noted as that and draws nothing. None of them
    /// decides whether a later step runs: the steps were fixed before the server was reached, and
    /// what a server returns is not what changes them.
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
    ) -> Result<String, String> {
        let name = format!("{alias}:{word}");
        let Some(offer) = &self.offer else {
            return Err(format!("{name} is not offered to this run"));
        };
        let arguments = arguments_of(step)
            .map_err(|_| format!("the arguments of {name} must be a JSON object"))?;
        let (text, origin, left_out) = match crate::mcp::call(
            policy,
            offer,
            egress,
            confirmer,
            reporter,
            alias,
            word,
            &Value::Object(arguments),
            None,
        ) {
            crate::mcp::Called::Answered(output) => (
                output.text,
                crate::mcp::origin_of_reply(&name, false),
                output.pictures.len() + output.left_out,
            ),
            crate::mcp::Called::Failed(text) => (text, crate::mcp::origin_of_reply(&name, true), 0),
            // Nothing of the reply is kept, so there is nothing to draw, and the run goes on.
            crate::mcp::Called::Unusable(_) => {
                return Ok(format!("{name} failed: its reply could not be used"));
            }
            crate::mcp::Called::Problem(problem) => return Err(problem),
        };
        let shown = crate::turn::preview_for(policy, manifest::MCP_CALL, &text);
        reporter.quarantined(Shown {
            origin,
            reach: Reach::NoModel,
            label: text.label().to_string(),
            lines: shown.lines,
            preview: shown.preview,
        });
        Ok(format!(
            "{name} replied; {} not shown",
            crate::tools::tally(left_out, "picture or part", "pictures or parts")
        ))
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
