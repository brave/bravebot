---
id: DELEGATE
title: Delegated agents
status: normative
governs:
  - crates/core/src/delegate.rs
  - crates/core/src/policy.rs
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
### DELEGATE-3: a kind is selected from a set the driver enumerated

The planner names a kind and cannot describe one. A name is compared against a fixed list and
matches or does not; a name matching nothing is refused, and there is no spelling of it that
reaches a capability set nobody wrote down.

`verified-by: bravebot_core::delegate::a_kind_is_selected_from_the_enumerated_set_and_nothing_else`
`verified-by: bravebot_core::delegate::every_advertised_name_resolves_to_the_kind_it_names`
`verified-by: bravebot_core::policy::a_kind_nobody_enumerated_is_refused`

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
### DELEGATE-5: the prompt belongs to the kind, and the planner writes no word of it

What a delegate is told about itself is a constant chosen by its kind. The planner supplies the
task and nothing else, so there is no sentence it can write that changes what a delegate is
rather than what it is doing.

`verified-by: bravebot_agent::delegate::each_kind_is_told_what_it_cannot_do`
`verified-by: bravebot_agent::delegate::every_kind_is_told_the_guidance_the_planner_is_told`

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

**Why the driver says it.** A report is prose a model had a hand in. An interface reading one to
decide which run it belonged to would be taking that decision from model output, which is the
thing this repository refuses everywhere else. Whose work a line is is a fact the driver already
holds.

`verified-by: bravebot_agent::turn::each_delegate_a_turn_spawns_is_numbered_and_its_work_reported_under_that_number`
`verified-by: bravebot_agent::turn::a_delegates_work_is_bracketed_by_the_announcements_the_interface_reads`

<a id="DELEGATE-15"></a>
### DELEGATE-15: delegates run alongside the turn and alongside each other

Starting one does not stop the turn. The call answers as soon as the kernel has approved the
delegate, the planner has its round back, and the work goes on behind it. A turn may have any
number going at once, and what one is doing has no bearing on what another may do.

Each holds its own conversation, quarantine, capabilities, routing grants and prompt history.
They share live file authority because their effects touch the same filesystem. Short capture
boundaries and active-path reservations do not serialize whole delegates.

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

## Known costs

- **A reference cannot be handed to a delegate.** A parent working in a directory nobody vouched
  for holds references and no filenames, and there is no argument for passing one on: quarantines
  do not cross, so a name from one would resolve to nothing or, worse, to something else. A
  delegate has to list and read its way to the same file itself, which costs a round and the
  tokens of a listing. Sharing one quarantine between the two would fix that and would put a
  second place for every reference to resolve, which is the more expensive mistake.

- **A person approving a write cannot see which delegate asked.** The confirmation shows the path
  and the diff, and does not say which of the runs in flight is asking. A person reading only the
  prompt is approving a change whose reason is one of several tasks they did not read.

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
