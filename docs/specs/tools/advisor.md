---
id: ADVISOR
title: advisor
status: normative
governs:
  - crates/agent/src/advisor.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Putting a question to a second model that was shown the conversation. `question` is the only
argument besides the `why` every tool takes ([TOOL-5](tool-surface.md#TOOL-5)). The result is
the advisor's reply as text. Which model answers is a choice the session makes, not the planner
([cli.md](../cli.md)).

## Clauses

<a id="ADVISOR-1"></a>
### ADVISOR-1: the tool exists only where the session named a model to consult

A session that names no advisor is offered no `advisor` tool, and a call to it is answered as any
other unknown name is. A delegate is never offered it and is refused it if it calls it anyway.

**Why.** The tool spends a request on a model chosen for being stronger than the planner. A
session that did not choose one has no model to spend it on, and a delegate's task came from a
planner that already has the tool.

`verified-by: bravebot_agent::turn::the_advisor_tool_is_offered_only_when_a_model_is_named`
`verified-by: bravebot_agent::turn::a_session_without_an_advisor_cannot_call_one`
`verified-by: bravebot_agent::tools::a_call_to_an_advisor_nobody_was_offered_is_answered_as_an_unknown_name`
`verified-by: bravebot_agent::tools::every_tool_offered_asks_why_it_is_being_called`

<a id="ADVISOR-2"></a>
### ADVISOR-2: the question is the only thing the planner decides

The call takes a short question, written by the planner, and nothing else: no model, no context
to add, no tools to grant. The advisor is sent the request the planner was sent on that round,
unchanged, followed by one message that carries the question, and the request offers no tools.

**Why.** Whatever the context holds, a model shown exactly that context is shown nothing the
planner was not already shown. A field for the model would let the planner route the conversation to a
destination it chose. A tool for the advisor would make it a second planner.

`verified-by: bravebot_agent::turn::the_advisor_is_asked_with_the_planners_context_and_no_tools`
`verified-by: bravebot_agent::advisor::the_request_offers_no_tools_and_ends_with_the_question`

<a id="ADVISOR-3"></a>
### ADVISOR-3: a private question is not sent

The question is sent to a model as the body of a request. One labelled private is refused, with a
sentence that says to describe what to ask rather than paste what was read.

`verified-by: bravebot_core::policy::a_private_question_is_not_put_to_the_advisor`

<a id="ADVISOR-4"></a>
### ADVISOR-4: the reply is labelled by the context the advisor was shown

The reply comes back as the result of the call, labelled from the planner's context the way the
planner's own words are. The planner reads it while that context has met nothing untrusted, and is
given a reference to it once the context has. It is never promoted on the strength of having come
from a stronger model.

**Why.** The advisor read what the planner read. If that included content nobody vouched for, the
reply may have been steered by it, and the label is what records that.

`verified-by: bravebot_agent::turn::the_advisor_is_asked_with_the_planners_context_and_no_tools`
`verified-by: bravebot_agent::turn::an_advisor_that_was_shown_untrusted_content_is_quarantined`

<a id="ADVISOR-5"></a>
### ADVISOR-5: each call is counted, with its model and its cost

The tokens an advisor call used are added to the turn's total, and the trail records the model
asked, which call of the turn it was, and what it cost.

**Why.** A call sends the whole conversation to a larger model. A turn whose figure left that out
would understate what it spent.

`verified-by: bravebot_agent::turn::an_advisor_call_is_counted_and_recorded`
`verified-by: bravebot_core::policy::a_consultation_is_recorded_with_its_model_and_cost`

<a id="ADVISOR-6"></a>
### ADVISOR-6: a turn may ask at most three times

A fourth call in one turn is refused with a sentence telling the planner to decide from the advice
it has, and sends no request. A call that failed counts.

**Why.** A planner unhappy with an answer can ask again, and each ask is a large request. The
bound is on a loop and not a budget.

`verified-by: bravebot_agent::turn::a_turn_may_ask_its_advisor_only_three_times`
`verified-by: bravebot_agent::tools::a_call_to_an_advisor_nobody_was_offered_is_answered_as_an_unknown_name`

<a id="ADVISOR-7"></a>
### ADVISOR-7: a failed or refused call is worded by the driver

A call the service could not answer is reported as the category of the failure and nothing the
service said. A cancelled turn ends as it does anywhere else. A model the machine-level settings
refuse is not asked, whichever route named it, and the planner is told to carry on without it.

`verified-by: bravebot_agent::turn::a_failed_advisor_call_tells_the_planner_only_the_category`
`verified-by: bravebot_agent::turn::an_advisor_model_the_machine_refuses_is_not_asked`
