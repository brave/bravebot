---
id: DELEGATE
title: Delegated agents
status: normative
governs:
  - crates/core/src/delegate.rs
  - crates/core/src/policy.rs
  - crates/agent/src/agents.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/report.rs
  - crates/agent/src/timing.rs
  - crates/agent/src/shared.rs
guards:
  - symbol: Policy::before_delegate
  - symbol: Policy::adopt_from_delegate
  - symbol: Policy::sink
  - symbol: Task::delegated
documented-by:
  - docs/website/docs/how-it-works.md
  - docs/website/docs/reference/tools.md
---

## Scope

A second planner, spawned by the first, with its own context and a narrower set of capabilities.
What fixes it before it exists, what it may do, what crosses back, and what does not. What a
processor is, and why that one holds nothing at all, is [processors.md](processors.md).

## Why it exists

A planner that runs the build reads the whole log. A planner that asks a delegate to run the
build is told what failed. The work happens either way and only one of them spends the
conversation on it, so a long task in a large tree stops being a race between finishing and
filling the context.

Nothing here is about trusting a second model more than the first. It is about where the reading
lands.

## What a delegate is

<a id="DELEGATE-1"></a>
### DELEGATE-1: a delegate is a planner, not a processor with tools

| | |
|---|---|
| Tools | its kind's, which are what its capabilities reach, and never a way to delegate |
| Memory | none of its parent's exchange: it begins with the task it was given |
| Conversation | a loop of its own, bounded |
| Reads | whatever its capabilities and the paths a person vouched for allow |
| Writes | files, each shown to a person first, and slots in a quarantine of its own |

A processor holds nothing at all because a processor with one tool would be a second planner with
untrusted content in its context. A delegate is the other half of that sentence: it holds
capabilities and it holds no untrusted content. Content it may not read is quarantined and it is
handed a reference, exactly as its parent would be, so there is no point in the run where
untrusted bytes and a capability are in the same context.

`verified-by: bravebot_agent::tools::a_delegate_is_offered_only_the_tools_its_capabilities_reach`
`verified-by: bravebot_agent::turn::what_a_delegate_could_not_read_is_quarantined_from_it_too`

<a id="DELEGATE-2"></a>
### DELEGATE-2: a run that has met something untrusted cannot delegate

The task is composed by the run that spawns the delegate and is the whole of what steers it, so a
run whose own context has met untrusted bytes composes tasks that are a function of them.
Delegating from one is refused outright rather than narrowed.

Private content is refused too, on either the task or the kind: the user's data may not become
another planner's prompt.

**Why.** This is the clause the rest rests on. Without it, delegation is a way for untrusted
content to reach a planner's context by the long way round, and every other rule here would be
describing something that had already failed.

`verified-by: bravebot_core::policy::a_run_that_has_met_something_untrusted_cannot_delegate`
`verified-by: bravebot_core::policy::a_private_task_cannot_direct_a_delegate`

## What is fixed before it runs

<a id="DELEGATE-3"></a>
### DELEGATE-3: a kind is selected from a set the driver resolved

The planner names a kind and cannot describe one. A name is compared against a set the driver
fixed before the turn and matches or does not; a name matching nothing is refused, and there is no
spelling of it that reaches a capability set nobody wrote down.

The set holds the three kinds, which are this program's own, and whatever definitions
[DELEGATE-19](#DELEGATE-19) resolved from files. It is fixed before the turn and never added to
while one runs, and the refusal names what it holds, so a planner is told what it may actually
select rather than the three that were compiled in.

**What comparing a name decides, and why that is sound.** A name has to be compared against the
set to select anything at all, and comparing it decides nothing an attacker steers because this
run has met nothing an attacker wrote ([DELEGATE-2](#DELEGATE-2)). That covers the name the planner
said. The *set* it is compared against is covered by [DELEGATE-20](#DELEGATE-20): once the
enumeration can come from a file, the enumeration is the thing an attacker would want to write, so
[LABEL-5](labels.md#LABEL-5)'s rule that a decision may be taken only from trusted content applies
to it and not only to the selection.

`verified-by: bravebot_core::delegate::a_kind_is_selected_from_the_enumerated_set_and_nothing_else`
`verified-by: bravebot_core::delegate::every_advertised_name_resolves_to_the_kind_it_names`
`verified-by: bravebot_core::delegate::a_name_no_definition_carries_selects_nothing`
`verified-by: bravebot_core::delegate::the_three_kinds_are_in_every_set`
`verified-by: bravebot_core::policy::a_kind_nobody_enumerated_is_refused`
`verified-by: bravebot_core::policy::a_refusal_names_the_definitions_this_session_resolved`
`verified-by: bravebot_agent::tools::the_kinds_the_planner_is_offered_are_the_ones_this_session_resolved`

<a id="DELEGATE-4"></a>
### DELEGATE-4: a delegate holds its kind's capabilities, narrowed by its parent's

The intersection, computed before the delegate exists and never widened afterwards. Delegation
redistributes authority and never creates it, so a delegate can hold nothing the run that spawned
it did not already hold, and a kind asking for more gets a delegate without it.

The three kinds are ordered, so choosing a wider one never costs a narrower one's reach:

| Kind | Holds | For |
|---|---|---|
| `reader` | reading | finding something out |
| `checker` | reading, running programs | finding out whether something works |
| `worker` | reading, running programs, writing files | finishing a sub-task |

Every kind additionally reaches the network, because a planner is a model call and the request out
is egress like any other. No tool a delegate is offered reaches it, so what it buys is the ability
to ask the endpoint on that delegate's behalf. A kind without it is a kind that cannot think.

A delegate naming a tool that reaches the network anyway is answered the way any other unknown
name is. The capability is held, so no gate refuses that call, which is why the absence from the
tool list is written out a second time as a refusal.

`verified-by: bravebot_core::policy::a_delegate_holds_no_more_than_the_run_that_spawned_it`
`verified-by: bravebot_core::delegate::the_kinds_are_ordered_by_what_they_hold`
`verified-by: bravebot_core::delegate::every_kind_can_reach_the_endpoint_and_nothing_else_remote`
`verified-by: bravebot_agent::tools::no_kind_is_offered_a_tool_that_reaches_the_network`
`verified-by: bravebot_agent::turn::a_delegate_naming_fetch_url_reaches_no_host`
`verified-by: bravebot_agent::delegate::no_kind_is_told_it_may_reach_the_network`

<a id="DELEGATE-5"></a>
### DELEGATE-5: the prompt belongs to the definition, and the planner writes no word of it

What a delegate is told about itself is a constant. The planner supplies the task and nothing
else, so there is no sentence it can write that changes what a delegate is rather than what it is
doing.

The constant may come from a definition rather than from this program, and that is the whole of
what a definition's body is. It is admissible for one reason: a definition is trusted
configuration or it does not load ([DELEGATE-20](#DELEGATE-20)). The clause that stops a *planner*
authoring a delegate's self-description is untouched.

The driver's own words bracket the body and are never replaceable by it: the guidance every
planner here gets comes before it, and what the delegate cannot do comes after. A body that could
displace the second would be a checked-in file telling a delegate it may do what its kind cannot,
which is [DELEGATE-4](#DELEGATE-4)'s sentence read backwards.

**What it cannot do is chosen from what it holds, not from its kind.** The two stopped agreeing
the moment either could be narrowed: a `worker` spawned by a run that cannot write, or one whose
definition named only read tools, holds no write. Told its kind's paragraph it would plan around
a write it is not offered and could not make, which is the opposite of what saying this is for.

**And composed from that set rather than picked from the kind nearest to it.** A held set is not a
point on the ladder of kinds: writing without running is a set a definition naming one write tool
produces and no kind has, and the nearest kind to it holds both. So reading, running and writing
are asked about separately, and what a delegate is told it can do is what it is offered.

`verified-by: bravebot_agent::delegate::each_kind_is_told_what_it_cannot_do`
`verified-by: bravebot_agent::delegate::every_kind_is_told_the_guidance_the_planner_is_told`
`verified-by: bravebot_agent::delegate::a_body_cannot_displace_what_a_kind_cannot_do`
`verified-by: bravebot_agent::delegate::a_definition_with_no_body_leaves_the_prompt_as_it_was`
`verified-by: bravebot_agent::delegate::a_narrowed_delegate_is_told_what_it_holds_rather_than_what_its_kind_holds`
`verified-by: bravebot_agent::turn::a_definition_names_the_delegate_a_turn_runs_and_says_what_it_is_for`
`verified-by: bravebot_agent::delegate::a_delegate_holding_writing_and_not_running_is_told_it_cannot_run_a_program`
`verified-by: bravebot_agent::delegate::what_a_delegate_is_told_it_can_do_is_what_it_is_offered`
`verified-by: bravebot_agent::delegate::a_delegate_holding_no_reading_is_not_told_it_may_read`

<a id="DELEGATE-6"></a>
### DELEGATE-6: a delegate is bounded, and the bound is its kind's

Every kind carries a round limit and the call cannot set one. On the limiting round the delegate
loses its tools rather than its run, and answers with what it has.

**Not a safety property.** A gate refuses on the last round what it refuses on the first. It
bounds futility, and it applies here because nobody is coming to stop a delegate: the person is
watching the turn, and a turn that has started several has no more idea than they do which of
them is making progress.

`verified-by: bravebot_core::delegate::every_kind_carries_a_bound`
`verified-by: bravebot_core::policy::a_delegates_bound_comes_from_its_kind`

<a id="DELEGATE-7"></a>
### DELEGATE-7: a delegate cannot delegate

No kind is offered the tool, and a call to it from inside a delegate is answered the way any
other unknown name is. Two refusals rather than one, because the depth is what bounds the whole
tree and a bound resting on the tool list alone rests on the model reading it.

**Why.** The bound on a tree of delegates is the product of the bounds, which is a number nobody
chose. And a person approving a write at the third level has no way to see which task it belongs
to.

`verified-by: bravebot_agent::tools::a_delegate_is_never_offered_a_way_to_delegate`
`verified-by: bravebot_agent::turn::a_call_to_spawn_agent_from_inside_a_delegate_does_nothing`

## What comes back

<a id="DELEGATE-8"></a>
### DELEGATE-8: the report is model output, labelled by the context that produced it

A delegate's answer is labelled at the integrity of its own context and presented to its parent
through the same gate every other result passes. Ordinarily that context has met nothing
untrusted and the words are shown; where it has, the answer is quarantined and the parent is
handed a reference to it.

Nothing is relabelled, and nothing is trusted on a delegate's say-so. The label is the one its
own context earned.

`verified-by: bravebot_agent::turn::a_delegates_report_reaches_the_planner_that_asked_for_it`
`verified-by: bravebot_agent::turn::what_a_delegate_reported_reaches_the_person_watching`

<a id="DELEGATE-9"></a>
### DELEGATE-9: nothing but the report crosses back

The exchange, the tool results, the narration and the quarantine all end with the delegate. A
reference minted inside one names nothing afterwards, and none of it can be asked for later.

**Why.** This is the feature rather than a restriction on it. A delegate whose reading reached
its parent's context would have moved the log rather than absorbed it.

`verified-by: bravebot_agent::turn::what_a_delegate_read_never_reaches_the_planner_that_asked`

<a id="DELEGATE-10"></a>
### DELEGATE-10: a delegate's effects are gated on their own

Every write and every run passes the same gates with its own single-use endorsement, so a person
sees the path and the diff whoever proposed them. An approval given inside a delegate cannot be
replayed by its parent, and one the parent already holds does not carry in.

Delegation saves context. It never saves an approval.

`verified-by: bravebot_agent::turn::a_delegates_write_is_approved_on_its_own`

<a id="DELEGATE-11"></a>
### DELEGATE-11: what a person vouched for outlives the delegate

The paths and the commands a person answered about come back to the session. They are standing
decisions about their own machine, and the record of them belongs to the session rather than to
whichever run happened to be going when they made it: a delegate told once that the build may run
must not leave the next one asking again.

File decisions are shared while the delegates run. A completed effect or a person's answer is
visible before collection, and collection never applies a child's file snapshot over the live
record. Untouched siblings therefore erase nothing, and collection order cannot choose which
write wins. [trust-map.md](trust-map.md) governs capture and effect ordering.

Exact command approvals still cross back as differences from the seeded command list. Neither
one-use grants nor prompt history cross back, and no command grant is widened.

`verified-by: bravebot_agent::turn::overlapping_delegate_writes_follow_effect_order_in_both_collection_orders`

**Whether or not it finished.** A delegate that stopped on a failed model call has no report and
no round count for the parent to take, and it hands the record back anyway. A person answered
inside it, and an answer is a standing decision about their own machine rather than a part of the
work that failed. A record coming back only from a run that reported would leave the next one
asking about the build this one was already told it could run. A run that stopped before it
settled anything hands back the copy it was seeded with, which takes nothing away. A delegate
whose thread died holds no record to hand back, and that is the one case an answer does not
survive it.

Nothing else about a delegate's policy survives it.

`verified-by: bravebot_core::policy::what_a_person_vouched_for_inside_a_delegate_is_kept`
`verified-by: bravebot_core::policy::what_a_person_vouched_for_running_inside_a_delegate_is_kept`
`verified-by: bravebot_core::policy::what_each_of_two_delegates_vouched_for_survives_the_other`
`verified-by: bravebot_core::policy::a_delegate_that_answered_nothing_takes_nothing_away`
`verified-by: bravebot_agent::turn::a_delegate_that_stopped_after_a_person_vouched_still_brings_the_answer_home`

<a id="DELEGATE-12"></a>
### DELEGATE-12: a delegate puts no question of its own to a person

It is offered no way to ask one and no task list to write to, and a call to either anyway is
answered the way any other unknown name is. Its task came from a planner rather than from the
person, so a question about it asks somebody to arbitrate something they never set up, and the
list on the screen belongs to the turn they are actually watching.

Two refusals rather than one, because a model naming a tool it was never offered is ordinary, and
a rule resting on the tool list alone rests on the model reading it.

What it could not settle goes in the report, and the parent asks.

`verified-by: bravebot_agent::tools::a_delegate_is_offered_no_task_list_and_no_way_to_ask`
`verified-by: bravebot_agent::delegate::no_kind_is_told_it_may_ask_a_person_or_delegate`
`verified-by: bravebot_agent::turn::a_delegate_naming_ask_user_or_todo_write_reaches_neither_the_person_nor_the_screen`

<a id="DELEGATE-13"></a>
### DELEGATE-13: one trail records both runs

A delegate's gates report into the same audit trail as the turn that spawned it, named so the two
can be told apart. A nested run recording somewhere else would leave a hole in the record exactly
over the part of the turn nobody watched.

The name is the delegate's number, minted by the driver in the order the turn spawned them. A
record says which run took the decision it holds, and the turn's own records are left unnamed.

`verified-by: bravebot_agent::turn::one_trail_records_the_delegate_and_the_turn_that_spawned_it`
`verified-by: bravebot_core::delegate::a_description_names_what_it_holds_but_never_the_task`
`verified-by: bravebot_core::event::a_record_says_which_run_took_the_decision`
`verified-by: bravebot_session::audit::a_delegates_records_are_named_and_the_turns_own_are_not`
`verified-by: bravebot_session::audit::the_written_record_names_the_delegate_that_took_the_decision`

<a id="DELEGATE-14"></a>
### DELEGATE-14: each delegate is numbered, and every report about one says which

The driver numbers them in the order the turn spawned them and says, before each report, whose
work it describes: one delegate, or the turn itself. Nothing works it out from the report.

A number rather than a position in the sequence. Reports arrive in the order the work happened,
which is not the order it was asked for, and two delegates of the same kind produce lines that
read identically.

The line beside the number names the definition rather than its kind, since "a reader" stops
telling anybody anything the moment two definitions are readers. That name is content from a file
somebody vouched for, which is the one reason it may be printed: a name nobody vouched for never
entered the set it was selected from ([DELEGATE-20](#DELEGATE-20)).

**Why the driver says it.** A report is prose a model had a hand in. An interface reading one to
decide which run it belonged to would be taking that decision from model output, which is the
thing this repository refuses everywhere else. Whose work a line is is a fact the driver already
holds.

`verified-by: bravebot_agent::turn::each_delegate_a_turn_spawns_is_numbered_and_its_work_reported_under_that_number`
`verified-by: bravebot_agent::turn::a_delegates_work_is_bracketed_by_the_announcements_the_interface_reads`
`verified-by: bravebot_agent::turn::a_definition_names_the_delegate_a_turn_runs_and_says_what_it_is_for`
`verified-by: bravebot_core::delegate::a_description_names_the_definition_and_the_kind_behind_it`

<a id="DELEGATE-15"></a>
### DELEGATE-15: delegates run alongside the turn and alongside each other

Starting one does not stop the turn. The call answers as soon as the kernel has approved the
delegate, the planner has its round back, and the work goes on behind it. A turn may have any
number going at once, and what one is doing has no bearing on what another may do.

Each holds its own conversation, quarantine, capabilities, routing grants and prompt history.
They share live file authority because their effects touch the same filesystem. A capture boundary
spans one operation, so a large listing or search can delay other captures until it finishes.
Reservations are per path, so writes to other paths proceed.

**Why.** A turn that asked three questions waits on the slowest and not on the sum. Running them
one at a time would also make the reading order the asking order, so a build would have to finish
before a search it has nothing to do with could begin.

`verified-by: bravebot_agent::turn::two_delegates_work_at_the_same_time`
`verified-by: bravebot_agent::turn::each_delegate_a_turn_spawns_is_numbered_and_its_work_reported_under_that_number`

<a id="DELEGATE-16"></a>
### DELEGATE-16: one person is asked one question at a time, and one trail records them all

However many runs are going, the confirmer, the reporter and the audit trail are each single and
each is taken for one call at a time. A delegate wanting a write approved while somebody is
reading another delegate's diff waits for them to finish reading it.

**Why.** The alternative is two questions on one screen, which is a person answering neither
properly, and two trails, which is a record with a hole in it exactly over the part of the turn
nobody watched.

`verified-by: bravebot_agent::turn::a_delegates_write_is_approved_on_its_own`
`verified-by: bravebot_agent::turn::one_trail_records_the_delegate_and_the_turn_that_spawned_it`

<a id="DELEGATE-17"></a>
### DELEGATE-17: a delegate does not outlive the turn that spawned it

A turn does not answer while something it started is still working. Where the planner answers
first, the reports are waited for, put in front of it, and it answers again knowing what came
back. A turn that has run out of tool calls waits too, and answers without being asked again.

**Why.** A person told the turn is over reasonably believes nothing of theirs is being read or
written any more. A delegate still running is still doing both, and can still put a write in
front of them for a turn they were told had finished.

`verified-by: bravebot_agent::turn::a_turn_does_not_answer_while_a_delegate_is_still_working`
`verified-by: bravebot_agent::turn::two_delegates_work_at_the_same_time`

<a id="DELEGATE-18"></a>
### DELEGATE-18: inference wait counts elapsed time once

A parent's inference time includes the union of delegate model request intervals, clipped to the
parent's actual join windows. All intervals use the same monotonic clock. Overlapping requests
count once, and sequential collections cannot charge the same instant twice. For a wait from 100
to 200, requests spanning `[80, 150)` and `[120, 180)` contribute 80, the interval `[100, 180)`.

A request completed before a join adds no inference time to that join. Background work during a parent's
own request, tool, approval or overhead time adds none either. Gaps between delegate requests and
other non-inference parts of a join remain overhead. Delegate tool and approval durations do not
become parent tool or stalled time. These rules also apply when collecting failed or cancelled
delegates, and when a parent collects outstanding delegates while ending a failed or cancelled turn.

Request intervals span whole planner, processor, vetting and compaction calls, including failed
calls. Planner calls include retries and retry waits. Durations are rounded once when the turn
reports milliseconds. Tokens, cache usage and request counts describe all work performed;
elapsed time does not change their additive accounting.
Nested delegates are refused by [DELEGATE-7](#DELEGATE-7).

**Why.** Adding concurrent request durations would report time the parent did not spend waiting.
A delegate's final duration alone cannot say which part fell inside a wait. Retaining intervals
also lets a later collection account for its requests during earlier joins without charging them
again.

`verified-by: bravebot_agent::timing::delegate_requests_cover_only_their_union_inside_a_wait`
`verified-by: bravebot_agent::timing::successive_collections_charge_each_covered_instant_once`
`verified-by: bravebot_agent::turn::overlapping_delegate_requests_charge_one_elapsed_wait`
`verified-by: bravebot_agent::turn::failed_parent_keeps_overlapping_delegate_retry_waits`
`verified-by: bravebot_agent::turn::reporting_between_delegate_joins_is_not_inference_wait`
`verified-by: bravebot_agent::turn::completed_delegate_requests_do_not_charge_background_time`
`verified-by: bravebot_agent::turn::delegate_collection_keeps_success_and_failure_wait_time`
`verified-by: bravebot_agent::turn::failed_parent_keeps_delegate_wait_time`
`verified-by: bravebot_agent::turn::cancelled_parent_keeps_delegate_wait_time`
`verified-by: bravebot_agent::turn::cancellation_cleanup_does_not_charge_completed_delegate_requests`
`verified-by: bravebot_agent::turn::cancelled_cleanup_retains_inflight_delegate_inference`
`verified-by: bravebot_agent::turn::delegate_read_output_vetting_covers_parent_wait`
`verified-by: bravebot_agent::turn::stopped_parents_collect_outstanding_delegate_usage_once`
`verified-by: bravebot_agent::turn::delegate_processor_compaction_and_vetting_requests_cover_parent_waits`

## Definitions

<a id="DELEGATE-19"></a>
### DELEGATE-19: a definition names a kind, and never describes one

A definition is a file saying what a kind of delegate is *for*. `kind:` is required and resolves
through the enumerated three, so what a file chooses is which of them this delegate is. A file
naming a kind that resolves to nothing is not a definition and does not load.

A definition may not take a kind's own name. `reader`, `checker` and `worker` are in every set
and mean what they have always meant, because a file free to claim one could rename the narrowest
kind to the widest, and the planner picking the narrowest thing that can do the job would be
picking from a list whose order had stopped being true.

A definition may then name `tools:`, and that is a narrowing and only a narrowing. What it selects
is intersected with its kind's reach and with the parent's own set, so the intersection
[DELEGATE-4](#DELEGATE-4) takes gains a third term and keeps its direction: a definition naming a
tool its kind does not reach is a definition loaded without it, and the trail says what was
dropped.

A name that is not a tool selects **nothing**, and is reported dropped like any other. `*` is such
a name rather than a way to ask for all of them, and so is every name in another agent's
vocabulary. A definition ported from one, whose whole list is names nothing here has, starts a
delegate with no tools and the trail says which names it was: a fallback to the weakest capability
would instead leave it holding something for a list of names none of which is a tool it gets, and
silence would leave somebody with a delegate that answered having done nothing.

Two things survive every narrowing. Reaching the network, because a planner is a model call and
one that cannot make a request cannot think, and no tool a delegate is offered reaches it. And
reading, because a write is a read of the file followed by a write of it, so a definition naming
`edit_file` alone and holding no read would be a delegate whose one tool is refused on every call.
Neither is a widening: every kind holds both already.

The narrowing is taken in the kernel rather than where the file was read. Which capabilities a
delegate holds is a decision, and a loader that took it would have moved one out of the kernel,
which [layering.md](layering.md) forbids.

**Why a file may not name the capability set.** Comparable tools let one: a definition's `tools:`
*is* what the agent holds, and a file is free to assert a shell where nothing granted one. That
cannot be had here. [DELEGATE-4](#DELEGATE-4) says delegation redistributes authority and never
creates it, and a checked-in file granting a capability would make the file the author of
authority rather than the person who vouched for it.

Keys other than the ones this reads are ignored rather than refused, per
[SKILL-1](skills.md#SKILL-1), so a definition written for another agent loads here. That is a
deliberate divergence from the strict schemas those tools use, and it is what lets one checked-in
directory serve several of them.

`verified-by: bravebot_agent::agents::a_definition_carries_a_name_a_description_a_kind_and_a_body`
`verified-by: bravebot_agent::agents::a_kind_nobody_enumerated_is_not_a_definition`
`verified-by: bravebot_agent::agents::a_definition_needs_a_kind_and_a_description`
`verified-by: bravebot_agent::agents::an_asterisk_is_a_name_and_never_the_whole_list`
`verified-by: bravebot_agent::agents::a_key_nothing_here_reads_is_ignored_rather_than_refused`
`verified-by: bravebot_core::delegate::a_definition_can_only_narrow_what_its_kind_holds`
`verified-by: bravebot_core::delegate::naming_tools_drops_the_capabilities_no_named_tool_reaches`
`verified-by: bravebot_core::delegate::a_definition_narrowed_to_one_tool_can_still_reach_the_endpoint`
`verified-by: bravebot_core::delegate::a_definition_that_names_no_tools_holds_its_kinds_own_set`
`verified-by: bravebot_core::delegate::a_name_that_is_not_a_tool_selects_no_capability`
`verified-by: bravebot_core::delegate::a_tools_line_written_for_another_agent_is_reported_name_by_name`
`verified-by: bravebot_core::delegate::a_definition_that_names_only_a_write_tool_can_still_read`
`verified-by: bravebot_core::delegate::a_definition_cannot_take_a_kinds_own_name`
`verified-by: bravebot_agent::agents::a_definition_cannot_take_a_kinds_own_name`
`verified-by: bravebot_core::policy::a_definition_is_delegated_as_the_kind_it_names`
`verified-by: bravebot_core::policy::a_definition_naming_a_tool_its_kind_lacks_is_delegated_without_it`
`verified-by: bravebot_core::policy::a_definition_cannot_widen_past_the_run_that_spawned_it`
`verified-by: bravebot_agent::tools::a_definition_confines_a_delegate_to_the_tools_it_named`
`verified-by: bravebot_agent::tools::the_capability_that_gates_a_tool_here_is_the_one_the_kernel_reads`

<a id="DELEGATE-20"></a>
### DELEGATE-20: a definition is resolved from trusted sources before the turn, or it is dropped

Two roots, resolved afresh every turn where skills are, least specific first so the project has
the last word ([INSTR-1](instructions.md#INSTR-1), [INSTR-4](instructions.md#INSTR-4)):
`~/.bravebot/agents/<name>.md`, trusted by provenance, and
`<workspace>/.bravebot/agents/<name>.md`, trusted only if the trust map says so.

This is [SKILL-3](skills.md#SKILL-3) through [SKILL-6](skills.md#SKILL-6) and it transfers whole,
because a definition is the same shape as a skill and a larger version of the same cost: a skill's
body is guidance a turn may follow, and a definition's body is the whole of what a second planner
is told it is.

- The workspace directory is checked for trust **before it is enumerated at all**, because a file
  name is content too.
- A source that fails `read_trusted_content` is **dropped entirely, never quarantined**. A
  reference in place of an instruction is no use to anybody: an instruction is either followed or
  absent.
- What was skipped is **counted, never named**, because a file in an untrusted project can be
  named to read like an instruction and a notice would put that on the person's screen.
- A file with no `name` is not a definition and is skipped in silence, so a note kept beside the
  definitions is not an error. A file *with* a name that fails to be a definition is reported, and
  its origin is named, because by then the source it came from was one somebody vouched for.

Two files in one directory resolve by file name, so which of them is live is the same on every
machine.

**A later definition of the same name replaces the one before it and never widens it.** It has the
last word about what the name is *for*, taking over the description, the body, the model and the
skills ([DELEGATE-23](#DELEGATE-23)), and none at all about what it may do. Both fields that decide
that are met with the one it replaced:

- It is loaded as the **narrower of the two kinds**, so a project cannot turn a `reader` a person
  wrote in their own directory into a `worker`.
- Its **`tools:` line is met name by name** with the one it replaced, an absent line being that
  kind's whole set and so the wider of the two. A list narrows and only narrows
  ([DELEGATE-19](#DELEGATE-19)), so the narrowing a person wrote cannot be handed back, and two
  lists with no name in common leave a delegate with none, exactly as a list naming another
  agent's vocabulary does.

Met on the fields rather than on the capability set they come to, because two tools one capability
reaches are two different things a delegate may do, and a meet taken on capabilities alone would
hand back every other tool that capability reaches.

DELEGATE-19 says a checked-in file granting a capability would make the file the author of
authority rather than the person who vouched for it, and a wider `kind:` for a name that person
already defined is that grant written another way. The vouch that let the project's file be read at
all is a decision about the checkout, taken once and with the longest path prefix winning, rather
than a decision about this name.

The rule is the same wherever the two files sit, including two in one directory. Which root a
definition came from is not something the kernel knows, and giving it that would be an arrangement
of files that gets around the rule; the cost is that a person's own two files of one name narrow
each other, which is a configuration where one was already silently winning and where the notice
below now says so.

**What the replacement asked for and did not get is named, with the file that cut it down.** The
kind it wrote where it is loaded as a narrower one, and the tools it is confined to where its own
line did not stand. That is [PERM-14](permissions.md#PERM-14)'s reason for naming a dropped `allow`
rule, and it reads the same way in this direction: a narrowing nobody is told about leaves whoever
wrote either file believing what they wrote is in force. Both files may be named, because by here
each came from a source somebody vouched for, which is what separates this from the count above.

`verified-by: bravebot_agent::agents::a_definition_nobody_vouched_for_is_counted_and_never_named`
`verified-by: bravebot_agent::agents::a_definition_in_a_vouched_for_project_is_selectable`
`verified-by: bravebot_agent::agents::a_definition_in_the_users_own_directory_is_selectable`
`verified-by: bravebot_agent::agents::the_three_kinds_are_selectable_wherever_a_session_runs`
`verified-by: bravebot_agent::agents::a_workspace_definition_shadows_a_home_one_of_the_same_name`
`verified-by: bravebot_agent::agents::two_definitions_in_one_directory_resolve_by_file_name`
`verified-by: bravebot_agent::agents::a_file_that_claims_to_be_a_definition_and_is_not_says_so`
`verified-by: bravebot_agent::agents::a_project_cannot_widen_the_kind_a_persons_own_definition_named`
`verified-by: bravebot_agent::agents::a_project_cannot_hand_back_a_tool_a_persons_own_definition_took_away`
`verified-by: bravebot_core::delegate::a_later_definition_replaces_one_of_the_same_name`
`verified-by: bravebot_core::delegate::a_later_definition_cannot_widen_the_kind_the_one_it_replaces_named`
`verified-by: bravebot_core::delegate::a_later_definition_cannot_undo_the_tools_the_one_it_replaces_named`
`verified-by: bravebot_core::delegate::a_later_definition_cannot_widen_a_tool_list_within_one_capability`
`verified-by: bravebot_core::delegate::tool_lists_with_nothing_in_common_meet_at_nothing`
`verified-by: bravebot_core::delegate::a_narrowing_carries_through_a_third_definition_of_the_same_name`
`verified-by: bravebot_core::delegate::a_later_definition_takes_over_the_skills_the_one_it_replaces_named`

<a id="DELEGATE-21"></a>
### DELEGATE-21: a definition's name may not open with `-` or carry a colon

A name opening with `-` reads as a command-line switch wherever it is printed beside other words.
A colon separates a namespace from a name everywhere one is written, so it is reserved even though
nothing here namespaces anything yet, and it is reserved against the characters that normalise to
one as well: a fullwidth colon folds to `:`, and a name that could be spelled two ways is a name
two definitions can claim.

A file whose name fails this is a definition that does not load, and the notice says which file
it was, exactly as [DELEGATE-20](#DELEGATE-20) says for any other file that claims to be a
definition and is not: by then the source it came from is one somebody vouched for, and what is
named is its path rather than the name it asked for. A file from a directory nobody vouched for is
never reached at all, and is counted with the rest.

There is no length limit and no character class. A name is compared, never resolved against
anything, so what it may hold is [SKILL-8](skills.md#SKILL-8)'s question and not this one.

`verified-by: bravebot_agent::agents::a_name_that_is_or_folds_to_a_colon_is_refused`

<a id="DELEGATE-22"></a>
### DELEGATE-22: a definition may name the model its delegate runs on

`model:` is optional. A definition naming one runs its delegate on that model, resolved the way any
named model is, so a tier alias such as `haiku` means that tier's configured model and anything
else is sent as written. A definition naming none, or naming `inherit`, runs on the model of the
turn that spawned it.

The name is configuration, not content. A definition loads only from a source somebody vouched for
([DELEGATE-20](#DELEGATE-20)), so the file is the endorsement for the request field the name lands
in, as a person's pick in `/model` is for the parent's.

A model that needs a sign-in this machine has not made is not swapped for the turn's. The delegate
does not run, and the person is told which definition asked for which model.

A delegate answered by a model other than the one its definition named says so, naming the
definition and the model it asked for. It is compared the way a session's own model is: against the
name that was sent, and not where the name is a handle the reply resolves, as a Bedrock profile or
the automatic name is. The name that answered is left out, since a notice is the driver's own words,
and what the comparison decides is whether a sentence is shown, which reaches no planner.

**Why `inherit` names none.** It is how other agents' definitions say so, and one ported from them
would otherwise send the word as a model name.

**Why refuse rather than fall back.** A definition naming a cheap model is often a cost boundary,
and running it on the turn's model would spend past that boundary without anybody choosing to. A
sign-in is no alternative either: a delegate runs on a worker thread with nowhere to show one.

**Why the substitution is said.** The endpoint substitutes rather than refuses a name it will not
serve, a misspelt one or a premium one this run holds no subscription for, so without it a
definition could ask for one model and have every delegate it starts answered by another.

`verified-by: bravebot_agent::agents::a_definition_reads_a_model_name`
`verified-by: bravebot_agent::agents::an_empty_model_name_in_a_definition_is_ignored`
`verified-by: bravebot_agent::agents::a_definition_naming_inherit_names_no_model`
`verified-by: bravebot_core::delegate::a_definition_may_name_a_model_and_the_spec_carries_it`
`verified-by: bravebot_agent::turn::a_delegate_uses_the_model_its_definition_selected`
`verified-by: bravebot_agent::turn::a_delegate_whose_model_needs_a_sign_in_does_not_run_and_says_so`
`verified-by: bravebot_agent::turn::a_delegate_answered_by_a_model_other_than_its_definitions_says_so`

<a id="DELEGATE-23"></a>
### DELEGATE-23: a definition may name the skills its delegate is offered

`skills:` is optional and is read as `tools:` is, on one line or as a list. A definition naming
none offers its delegate every skill the turn found. A definition naming some offers the ones of
those the turn found and no others: the rest are neither listed in the delegate's prompt nor
loadable by it. An empty line names none, and its delegate is offered no skill.

A name selects out of what the turn found and adds nothing to it, so a skill the turn could not
load, whether from a directory nobody vouched for or from nowhere, is one no definition can offer.
A name nothing found selects nothing, and the turn says so with the rest of what it says about what
it found, naming the definition and the name.

**Why a replacement takes the list over.** A skill is guidance, and loading one is gated by what a
delegate holds whichever skills it is listed. Which of them it is told about changes what it is
told and not what it may do, so the list goes with the body rather than with the kind and the
tools ([DELEGATE-20](#DELEGATE-20)).

**Why say a name nothing found.** It is the reason [DELEGATE-19](#DELEGATE-19) reports a tool name
that is not a tool: without it a misspelt name reads to whoever wrote it as a skill the delegate
has.

`verified-by: bravebot_agent::agents::a_definition_reads_the_skills_it_names`
`verified-by: bravebot_core::delegate::a_definition_may_name_skills_and_the_spec_carries_them`
`verified-by: bravebot_agent::turn::a_definition_offers_its_delegate_only_the_skills_it_names`

## Known costs

- **A definition is trusted exactly as far as a configuration file somebody pasted is.** That is
  the cost [SKILL-1](skills.md#SKILL-1) already records for skills, and it is larger here: a
  skill's body is guidance the turn may follow, while a definition's body is the whole of what a
  second planner is told it is. Read one before installing it. What a definition can never do is
  hold a capability, which is what keeps the cost to what a person told a delegate rather than
  what one may reach.

- **The set of characters that fold to a colon is written down rather than derived.** There are
  four, and normalising a name to find them would be a dependency for four code points. A
  character Unicode adds to that set later is one [DELEGATE-21](#DELEGATE-21) would not catch
  until the list is extended.

- **A definition cannot point its delegate at an MCP server.** Holding a server is a capability and
  no kind holds one, so a delegate holds no server's grant and is offered no server's tool
  ([SERVERS-9](mcp-servers.md#SERVERS-9)), and a definition naming one would widen its delegate
  past its kind, which [DELEGATE-4](#DELEGATE-4) forbids. An `mcpServers:` key is ignored like any
  other key this does not read, so a definition written to talk to one server says nothing about
  it here.

- **A definition's model is checked by using it.** Whether the endpoint serves a name is learned
  from its reply, so a definition naming one it does not serve has its delegate run on a substitute
  and the person hears so afterwards. Checking first would mean the driver holding the endpoint's
  roster, which only the interface fetches.

- **A reference cannot be handed to a delegate.** A parent working in a directory nobody vouched
  for holds references and no filenames, and there is no argument for passing one on: quarantines
  do not cross, so a name from one would resolve to nothing or, worse, to something else. A
  delegate has to list and read its way to the same file itself, which costs a round and the
  tokens of a listing. Sharing one quarantine between the two would fix that and would put a
  second place for every reference to resolve, which is the more expensive mistake.

- **A person approving a write cannot see which delegate asked.** The confirmation shows the path
  and the diff, and does not say which of the runs in flight is asking. A person reading only the
  prompt is approving a change whose reason is one of several tasks they did not read. Definitions
  make this bite harder: three `reader`s with different definitions are three different jobs where
  they used to be three of the same.

- **A delegate's task is a guess about what it will need.** It cannot come back for more and it
  cannot ask, so a task missing a detail is a delegate that reports having been unable to finish,
  and the round it spent is spent. The alternative is a channel back to the planner, which is a
  conversation, and a conversation is the context this exists to avoid.

- **Everything a delegate spent on something other than a request reads as the parent's own
  remainder.** [DELEGATE-18](#DELEGATE-18) charges its requests and nothing else, so a join spent
  running a build, waiting for somebody to answer a prompt, or between two of the delegate's own
  requests lands in the figure [sessions.md](sessions.md) leaves over. On a turn that ran no
  delegate that figure is the harness's own time and is read as such, and on one that delegated it
  is not. Charging the rest to the parent's tool or stalled figures would put seconds there that
  the parent did not spend there, which is the double count the partition exists to avoid.
