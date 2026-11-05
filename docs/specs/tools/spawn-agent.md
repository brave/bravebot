---
id: AGENT
title: spawn_agent
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/offered.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

The call that starts a delegated agent. `kind`, `mcp_servers` and `budget` are routing; `task` and `each`
are content. The call answers as soon as the kernel has approved it, and the report follows later.
What a delegate is, what it may do and how long it may live is [delegation.md](../delegation.md);
this spec is the call surface.

[CHECKOUT-1](../checkouts.md#CHECKOUT-1) adds a routing field, `isolation`, whose value `checkout`
asks for a checkout of the delegate's own and whose value `checkout_with_changes` also carries the
working directory's uncommitted changes into it ([CHECKOUT-23](../checkouts.md#CHECKOUT-23)). A
definition `kind` names may ask for a checkout itself ([CHECKOUT-2](../checkouts.md#CHECKOUT-2)), and
its delegate then has one whether or not the call asks.

## Clauses

<a id="AGENT-1"></a>
### AGENT-1: `kind` selects from a fixed set, and is the field a person could approve

It decides what the run holds, which makes it routing, and it is a name out of a list the driver
wrote rather than anything the call describes. That is what a person could approve on its own:
that a delegate may read, or read and run, or read and run and write. A name matching nothing in
the list is refused and reaches no capability set.

The list offered to a run is narrowed by the capabilities that run holds: a run that cannot write
is not offered the kind that writes ([DELEGATE-32](../delegation.md#DELEGATE-32)). The offer is
advisory, and a name outside the resolved set is refused ([DELEGATE-3](../delegation.md#DELEGATE-3)).

The description says that a delegate started after the planner was shown private content cannot run
processors, and that a task needing a picture or a file looked at goes first or to the person. The
`reader` kind text does not promise processors without condition
([DELEGATE-8](../delegation.md#DELEGATE-8)).

`verified-by: bravebot_core::policy::a_kind_nobody_enumerated_is_refused`
`verified-by: bravebot_core::delegate::a_kind_is_selected_from_the_enumerated_set_and_nothing_else`
`verified-by: bravebot_core::delegate::a_run_is_offered_the_kinds_its_own_capabilities_reach`
`verified-by: bravebot_agent::turn::a_checker_is_not_offered_the_worker_and_a_worker_it_starts_anyway_is_said_to_lack_writing`

<a id="AGENT-2"></a>
### AGENT-2: `task` is the whole of what the delegate is told, and it may not be private

It is the only thing steering the call, and it comes from a context holding nothing an attacker
wrote. The delegate cannot see the conversation it came from, so a task that leaves something out
is a delegate that never learns it. A compaction of the delegate's conversation sends the task
again after the summary ([COMPACT-19](../compaction.md#COMPACT-19)).

`verified-by: bravebot_core::policy::a_private_task_cannot_direct_a_delegate`
`verified-by: bravebot_core::policy::a_run_that_has_met_something_untrusted_cannot_delegate`

<a id="AGENT-3"></a>
### AGENT-3: the result is that a delegate started, and the report arrives on its own later

The call answers with the delegate's number as soon as the kernel has approved one, and the
planner has its round back. What the delegate says arrives as a message of its own, before the
planner is next asked what to do.

Where the kernel started the delegate without a capability its definition asked for, because the
run that started it does not hold the capability, the result names what it was started without in
one sentence of the driver's ([DELEGATE-33](../delegation.md#DELEGATE-33)). In a fan-out the
sentence is said once for the call.

Not as the result of this call. A result answers a call once, and by the time a delegate has
anything to say the call it came from was answered rounds earlier. A call that waited for the
report would hold the turn still until the delegate finished, so a turn could have one delegate
working and no more.

When the planner ends a round with a delegate working, the turn asks it again as soon as the first
delegate ends, with that report, and does not hold the report for the others
([DELEGATE-17](../delegation.md#DELEGATE-17)). The call has no field choosing between that and
waiting for every delegate of a batch, so nothing about it is for a person to approve.

`verified-by: bravebot_agent::turn::two_delegates_work_at_the_same_time`
`verified-by: bravebot_agent::turn::the_planner_is_asked_again_when_the_first_delegate_ends_and_the_turn_then_waits_for_the_rest`
`verified-by: bravebot_agent::turn::two_delegates_ending_together_are_both_collected_by_one_wake_whatever_their_labels`
`verified-by: bravebot_agent::turn::a_delegates_report_reaches_the_planner_that_asked_for_it`
`verified-by: bravebot_agent::tools::the_start_message_names_what_a_delegate_was_started_without`
`verified-by: bravebot_agent::turn::a_checker_is_not_offered_the_worker_and_a_worker_it_starts_anyway_is_said_to_lack_writing`
`verified-by: bravebot_agent::turn::a_fan_out_from_a_run_that_cannot_write_says_once_that_each_was_started_without_writing`

<a id="AGENT-4"></a>
### AGENT-4: what shape the report takes is not the tool's to decide

The delegate's answer is still labelled when it arrives, and presented like any other result.
The tool reads none of it: whether the planner is shown the words or a reference to them follows
from the label the delegate's own context earned.

`verified-by: bravebot_agent::turn::a_delegates_report_reaches_the_planner_that_asked_for_it`
`verified-by: bravebot_agent::turn::what_a_delegate_read_never_reaches_the_planner_that_asked`
`verified-by: bravebot_agent::turn::the_shape_a_report_takes_follows_the_label_of_the_delegate_that_wrote_it`

<a id="AGENT-5"></a>
### AGENT-5: one call may fan a task out, and every delegate it starts is gated on its own

`each` names one delegate per entry. Each is told the shared `task` followed by its own entry,
and is otherwise a delegate like any other: it passes the same gate, takes its own number, and
holds its own copy of what a person has vouched for. A call naming more than eight, or naming
none, is refused and starts nothing. The text of `each` says that one call starts at most that
many and that a further call with the same `task` starts more, and takes the figure from the one
the refusal enforces. A call that meets the turn's ceiling on delegates
([DELEGATE-7](../delegation.md#DELEGATE-7)) part-way starts the ones that fit and says how many did
not start, and why.

**Why.** A planner starting four delegates one call at a time writes four near-identical
paragraphs, and none of them starts until the last word of the last copy is written. The prose is
on the critical path and only the driver reads it.

**Why each is gated separately.** A fan-out is several runs. A gate that saw one of them would be
approving the rest on the strength of a sibling, and the entries are the part that differs.

**Why a ceiling at all.** Not authority: the planner can already have as many running one call at a time
as the turn's ceiling allows, and each passes the same gate either way. It is the cost of asking that changes, and a
field turning one sentence into an unbounded number of runs at once is worth a bound.

`verified-by: bravebot_agent::turn::one_call_can_fan_a_task_out_over_several_delegates`
`verified-by: bravebot_agent::turn::a_fan_out_past_the_ceiling_is_refused_and_starts_nothing`
`verified-by: bravebot_agent::turn::a_fan_out_that_meets_the_turns_ceiling_starts_what_fits_and_says_what_did_not`
`verified-by: bravebot_agent::turn::a_fan_out_that_names_no_delegate_is_refused_and_starts_nothing`
`verified-by: bravebot_agent::tools::the_each_field_says_one_call_starts_at_most_the_fan_out_limit`

<a id="AGENT-6"></a>
### AGENT-6: `mcp_servers` lists the servers a worker keeps, and can only take servers away

`mcp_servers` is optional: a list of server aliases, each written as it appears in the names of
the server's tools the planner is offered. A worker started by a call with a list holds the grant of
each listed server it would otherwise hold, and of no other server, so an empty list starts a worker
holding none. The list applies after the definition's own `mcpServers:` line
([DELEGATE-24](../delegation.md#DELEGATE-24)), so a server the definition leaves off stays off
whatever the call lists. A call without the field gives the worker what
[DELEGATE-4](../delegation.md#DELEGATE-4) gives it. A reader and a checker hold no server either way.
In a fan-out the list applies to every delegate the call starts. The trail names each grant the
list left out, and each listed server the delegate went without because its kind or definition
leaves it off.

A name the run holds no grant for is refused, nothing starts, and the refusal lists the servers the
run holds. The list must be public, and its names are compared with the run's grants only after
the integrity gate has passed, as `kind` is ([AGENT-1](#AGENT-1)).

**Why the call and not only a definition.** A definition is written before any session, so it
cannot know which servers a session will reach or what one task needs. The planner starting a
worker knows both. Without the field, a worker sent to fix a Rust bug holds the mail server its
parent holds for something else, and every call it makes to that server is one more prompt for the
person.

**Why refused rather than dropped.** A definition naming a server the session did not reach was
written before the session, so the name is dropped and said. The planner writes its list in the
turn, choosing from tools it is shown, so a name outside them is a mistake. Dropping it would start
a worker without a server the planner meant it to have.

**Why it widens nothing.** Each name selects from grants the run already holds, after every other
narrowing, so the most a list can do is leave the worker as it would have been
([DELEGATE-4](../delegation.md#DELEGATE-4)).

`verified-by: bravebot_core::policy::a_worker_spawned_keeping_no_server_holds_none`
`verified-by: bravebot_core::policy::a_worker_spawned_keeping_one_server_holds_only_that_one`
`verified-by: bravebot_core::policy::a_server_this_run_holds_no_grant_for_cannot_be_kept`
`verified-by: bravebot_core::policy::a_private_server_list_cannot_direct_a_delegate`
`verified-by: bravebot_core::policy::a_worker_delegate_holds_the_servers_its_parent_holds_and_no_other`
`verified-by: bravebot_agent::tools::a_server_list_that_is_not_a_list_of_names_is_refused`
`verified-by: bravebot_agent::mcp::a_worker_spawned_keeping_no_server_is_offered_no_servers_tool`
`verified-by: bravebot_core::policy::a_fallen_run_is_refused_before_a_server_list_is_compared_with_its_grants`

<a id="AGENT-7"></a>
### AGENT-7: `budget` bounds each delegate's tokens, and can only lower the definition's

`budget` is optional: a count of tokens above zero, written as a number or in the words `/limit`
takes (`500k`, `2m`). Each delegate the call starts carries the least of this figure and its
definition's `budget:` ([DELEGATE-29](../delegation.md#DELEGATE-29)), and of what its parent has
left under its own, so a call without the field leaves the definition's in force and a figure above
the definition's changes nothing. In a fan-out the figure applies to each delegate separately, and
one delegate's spend is not another's. A delegate that spends it loses its tools and answers with
what it has, and the planner is told so beside the report. Every request counts the whole
conversation so far, so a figure buys a few dozen requests and not a few dozen thousand tokens of
reading, and the field's description says so.

A value that is not a count of tokens above zero is refused with what the field takes, and nothing
starts. The figure must be public and is read only after the integrity gate has passed, as `kind`
is ([AGENT-1](#AGENT-1)); a private figure is refused, as a private `mcp_servers` list is
([AGENT-6](#AGENT-6)). Credits are not accepted, since they cannot be attributed to one delegate.

**Why the call may set it.** A figure that only lowers a bound leaves what the delegate may do as
it was, so there is nothing for it to widen. [DELEGATE-6](../delegation.md#DELEGATE-6) keeps the
round bound out of the call because a larger figure would be authority; a smaller one is not.

`verified-by: bravebot_agent::tools::a_budget_that_is_not_a_count_of_tokens_is_refused`
`verified-by: bravebot_core::policy::a_call_budget_can_only_lower_the_definitions`
`verified-by: bravebot_core::policy::a_private_budget_cannot_direct_a_delegate`
`verified-by: bravebot_core::policy::each_delegate_of_a_fan_out_carries_the_figure`
`verified-by: bravebot_agent::turn::a_call_budget_stops_a_delegate_and_cannot_raise_the_definitions`

<a id="AGENT-8"></a>
### AGENT-8: the text of `kind` names the round limit of each kind it lists

Each line of the `kind` parameter's description ends with the number of rounds a delegate of that
kind makes before it loses its tools, and what a round is: one reply with its tool calls. The
number is [`Kind::rounds`](../delegation.md#DELEGATE-6) of the line's kind. A line for a definition
carries its kind's number, not the definition's own `rounds:` and not the kind's ceiling.

**Why.** The planner writes the task and has no other way to learn how much work fits in a
delegate. A task sized to a figure it was never given ends with the delegate's tools removed part
way through.

**Why only the parameter text.** The tool's description names neither of the bounds on the tree
([DELEGATE-7](../delegation.md#DELEGATE-7)), and a delegate's own rounds are not one of them. The
most one call starts ([AGENT-5](#AGENT-5)) is a limit on a call and not a bound on the tree, and is
stated in the text of `each`.

`verified-by: bravebot_agent::tools::each_kind_the_planner_may_pick_names_the_rounds_it_allows`
`verified-by: bravebot_agent::tools::the_way_to_delegate_is_described_without_the_numbers_that_bound_it`
`verified-by: bravebot_agent::tools::the_each_field_says_one_call_starts_at_most_the_fan_out_limit`
