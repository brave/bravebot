---
id: ADDRESS
title: Addressing a definition
status: normative
governs:
  - crates/tui/src/app.rs
  - crates/tui/src/state.rs
  - crates/tui/src/render.rs
  - crates/core/src/delegate.rs
  - crates/core/src/policy.rs
  - crates/agent/src/agents.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/turn.rs
guards:
  - symbol: Policy::address
  - symbol: Session::address
documented-by:
  - docs/website/docs/reference/commands.md
  - docs/website/docs/customize/agents.md
---

## Scope

How a person runs a definition themselves, in place of describing what they want and hoping the
planner selects one. What the line looks like, what it starts, what that thing may do, and how long
it lasts.

What a definition is, where one is read from and what it is trusted for is
[delegation.md](delegation.md) and is unchanged here. What a delegate is and what it may not do is
that file too, and nothing below relaxes any of it. The `/` surface every command shares is
[commands.md](commands.md).

## What exists today

A definition is a file naming a kind, narrowing that kind's tools and optionally naming a model and
the skills it is offered; it is resolved before every turn from the person's own directory and from
a checkout they vouched for, and a source nobody vouched for is counted rather than named. All of
that is [delegation.md](delegation.md)'s and holds whoever selects from the set.

The interactive session selects from it with `/agent <name> <task>`. The interface resolves the set
when the line is submitted, and a name matching nothing, a missing task or a model needing a sign-in
runs nothing and says so. Otherwise the next turn carries the name. The driver resolves the set
again for that turn and the kernel decides the rest in `Policy::address`: the capabilities, the tools
offered, and the refusal of a name that is not there. The driver offers only those tools and refuses
a call to any other by name, and the interface draws the reply under the name the driver matched.

`bravebot -p` and the desktop front end address nothing. Both are open questions below.

## The comparison, and what it is worth

This is not a case where a comparable tool has made an argument this program has failed to answer.
None of the three has made an argument at all.

| | What it offers | What it says about why |
|---|---|---|
| Claude Code | a person may name a subagent in a prompt | documented as a fallback for when automatic delegation is not enough |
| opencode | a `primary` mode, cycled with Tab | where the boundary sits, never why it sits there |
| Codex | an `/agent` command | shipped after upvotes, with no statement that single-agent had been deliberate |

This program is the only one of the four with a written position, so what is on the table is
whether the convenience is worth the surface, not whether it is behind somebody else's reasoning.
The position is worth keeping where it is kept for a reason, and the reasons are specific: each of
the three clauses standing in the way is about a run nobody is watching. That is what
[ADDRESS-1](#ADDRESS-1) turns on.

## What it is

<a id="ADDRESS-1"></a>
### ADDRESS-1: what a person addresses is a run of their own, and never a delegate

A definition supplies four things to the run a person addresses: the prompt, the narrowing, the
model, and the skills it names ([DELEGATE-23](delegation.md#DELEGATE-23)). It supplies nothing
else, and in particular it does not make the run a delegate. The person is at the keyboard, so the
run holds the screen, the confirmer, the task list and the way to ask a question, exactly as any
turn of theirs does.

**Why.** The clauses in [delegation.md](delegation.md) that would forbid this are each about a run
nobody is looking at. A delegate puts no question to a person because its task came from a planner
and a question about it asks somebody to arbitrate something they never set up. Nothing but its
report crosses back because absorbing the log is the whole point of spawning one. It does not
outlive its turn because a person told the turn is over reasonably believes nothing of theirs is
still being read or written.

A person who typed the line set the task up, is watching the result, and has not been told anything
is over. So the answer is a different kind of run and not a delegate with three clauses relaxed:
relaxing them would leave the reasons behind and keep the words, and the next thing spawned by a
planner would inherit the relaxation.

`verified-by: bravebot_agent::turn::an_addressed_turn_runs_under_its_definitions_prompt_model_and_kind`

## The line

<a id="ADDRESS-2"></a>
### ADDRESS-2: the word is written in this program, and the name is an argument

`/agent <name> <task>`. `agent` is a string literal in the command table like every other command
word, allocated when this program is built. The definition's name is not a command, is not in the
table, and does not put a word on the `/` surface: it is an argument on a line somebody typed,
which is where content belongs.

**Why.** The surface stays one word wide however many definitions a machine holds, which is what
keeps it small enough to reason about. A name read from a directory could never be a command word
itself: a definition's name is written by whoever wrote the definition, it can be composed to read
like an instruction, and the rule that keeps skills off this surface is exactly that.

**What the alternatives cost.** `@name` is taken: `@` names a file and putting a second meaning on it
would make one prefix mean two things at the moment a person is typing fastest. A bare
`name: task` line would make every sentence containing a colon a dispatch. A word plus an argument
is the shape every other command here already has, and the whole word is the command, so
`/agents are useful` stays a prompt.

A turn stopped before it did anything puts the whole line back in the box, name and all, as a
stopped prompt is put back. The task alone would be a prompt for the session's own planner, and
Enter on it would run the work under everything the definition was there to take away.

`verified-by: bravebot_tui::app::a_session_can_address_a_definition`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_agent_is_a_prompt`
`verified-by: bravebot_tui::app::a_stopped_addressed_turn_puts_the_whole_agent_line_back_in_the_box`

<a id="ADDRESS-3"></a>
### ADDRESS-3: only a line a person typed into the box

The line comes off the input box and from nowhere else. Never a line the planner produced, never
text read out of a file, never anything a processor returned, never one reconstructed from a
transcript. A model that writes `/agent` has written six characters, and they reach a person's
screen as six characters.

**Why.** An addressed run works under a prompt the session's planner did not write and under a
narrowing it did not choose, which is a decision a turn is not allowed to take on its own. The
endorsement for it is the keystroke, so the keystroke is the only thing that may produce one.

`verified-by: bravebot_tui::app::a_line_addressing_a_definition_queued_while_a_turn_ran_addresses_it_when_the_turn_ends`
`verified-by: by-construction (a name reaches a turn only through Session::address; its one caller settles the Action::Address that only the /agent branch of dispatch_command returns; dispatch_command is reached from the input box's key handler and from the queue that handler filled; and the turn takes the name off the session as it starts, so nothing a turn produced sets one)`

<a id="ADDRESS-4"></a>
### ADDRESS-4: a conversation that has met untrusted content addresses a definition anyway

Delegating from a run whose context has handled untrusted content is refused outright. Addressing
a definition from such a session is not, and neither the name nor the task is narrowed because of
it.

**Why.** The refusal in [delegation.md](delegation.md) is about who composes the thing that steers
the new run. A delegate's task is composed by the run that spawns it, so a run that has met
untrusted bytes composes tasks that are a function of them. An addressed run's task and the name
selecting its definition are both the line a person typed ([ADDRESS-3](#ADDRESS-3)), so nothing an
attacker wrote reaches either. Refusing here anyway would take the words of that clause and leave
its reason behind, and it would make a person's own tools less reachable the longer their session
went on.

`verified-by: bravebot_core::policy::a_context_that_has_met_untrusted_content_addresses_anyway`

## Which definition

<a id="ADDRESS-5"></a>
### ADDRESS-5: a name selects from the set this session resolved, or it selects nothing

The set is the one [delegation.md](delegation.md) fixes before every turn: the three kinds, which
are this program's own, and whatever definitions resolved from a source somebody vouched for. A
name is compared against it and matches or does not. A name matching nothing runs nothing and says
so, and there is no spelling of it that reaches a file nobody vouched for.

`/agent` with a name and no task runs nothing and says a task is needed, rather than starting a
definition on an empty one.

**Why.** Comparing the name decides nothing an attacker steers, because the name came from a
keystroke. The set it is compared against is the thing an attacker would want to write instead, and
that is already settled: a definition loads from a trusted source or it is dropped, so a name
nobody vouched for never entered the set to be matched.

`verified-by: bravebot_core::policy::a_name_this_session_did_not_resolve_is_refused_with_the_names_it_did`
`verified-by: bravebot_agent::turn::a_name_this_session_did_not_resolve_sends_nothing_and_lists_what_it_did`
`verified-by: bravebot_agent::agents::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_tui::app::a_name_this_session_did_not_resolve_runs_nothing_and_lists_what_it_did`
`verified-by: bravebot_tui::app::a_name_with_no_task_runs_nothing_and_says_a_task_is_needed`

<a id="ADDRESS-6"></a>
### ADDRESS-6: a resolved name is printed where somebody asked for it, and never offered in the box

The bare word `/agent` says what this session resolved, and so does the refusal a name matching
nothing produces. Those names may be shown, because each came from a file somebody vouched for and
the ones that did not were counted rather than named.

No definition name is ever a row in the completion list, a suggestion under the box, or anything
else drawn while a person is typing.

**Why.** The two are not the same act. A printed answer follows a line somebody submitted, is read,
and no key turns it back into a line. A completion row arrives unasked, is drawn as though this
program had written it, and Enter submits the highlighted one: a name composed to read like an
instruction would then be one keystroke from dispatching itself. Discovery is what a completion
list is for, and the bare word gives it on demand instead.

`verified-by: bravebot_tui::app::the_bare_word_says_what_this_session_resolved`
`verified-by: by-construction (the completion list reads the command table and nothing else, and the resolved set exists only in the event loop after a line is submitted, so nothing drawn while a person types can hold a definition name)`

## What it may do

<a id="ADDRESS-7"></a>
### ADDRESS-7: a definition narrows what the person already holds, and widens nothing

The run holds the session's own capabilities, intersected with the kind the definition names and
with the tools it names. A definition of a wider kind than the session gets a run with the
session's reach. A definition naming a tool the session is not offered gets a run without it, and
the trail says what was dropped. The run is told what it holds in the words a delegate is told
them, less the sentence about an agent that asked it, because the planner's own guidance around
them is written for a turn that can edit and run.

No kind holds an MCP server, so no addressed run does: it is offered none of the tools of the
servers this session reached, and no server's list is put to the person for it.

**Why.** Addressing a definition is a person choosing which of their own capabilities to work
under, so it can only take away. This is the same direction a delegate's capabilities are computed
in and for the same reason: a checked-in file may choose what a run is for and may never choose
what it may reach. A file that could add a capability would be a configuration file handing out
authority, which is the one thing a definition is not.

`verified-by: bravebot_core::policy::an_addressed_turn_holds_only_what_the_session_and_the_kind_both_hold`
`verified-by: bravebot_core::policy::a_definition_wider_than_the_session_gets_the_sessions_reach`
`verified-by: bravebot_core::policy::a_delegate_an_addressed_turn_spawns_holds_no_more_than_the_turn`
`verified-by: bravebot_core::policy::an_addressed_turn_is_offered_only_the_tools_its_definition_named`
`verified-by: bravebot_agent::turn::a_tool_an_addressed_definition_left_out_is_refused_when_the_model_calls_it`
`verified-by: bravebot_agent::turn::an_addressed_turn_is_told_what_its_definition_left_it`
`verified-by: bravebot_agent::mcp::an_addressed_turn_is_offered_no_servers_tool_and_asks_about_no_list`

<a id="ADDRESS-8"></a>
### ADDRESS-8: the tools withheld from every delegate are not withheld here, except what arms a later turn

Asking a person, writing the task list, fetching a URL, asking for a second opinion on quarantined
content and spawning a delegate are offered to an addressed run wherever the session holds them,
subject to [ADDRESS-7](#ADDRESS-7) like anything else. Scheduling a next turn and watching a file
are not offered, and nothing else the run is offered tells it to use them.

**Why.** Each is kept from a delegate for a reason that names the thing this run is not. A question
and a task list need somebody watching, and somebody is. Fetching a URL and vetting content both
end at a prompt, and this run can raise one. Spawning a delegate is refused inside a delegate
because the bound on a tree of them is a product nobody chose and because a person approving a
write three levels down cannot see which task it belongs to; an addressed run is the session's own
turn, at the depth every other turn starts from.

**The two that are withheld are withheld for a reason of this clause's own.** What a next turn or a
watch's fire starts is a turn of the session's planner ([ADDRESS-10](#ADDRESS-10)), holding
everything the session holds. A reader addressed so that nothing is written could otherwise arm a
turn that writes, with nobody typing anything. Carrying the name onto that later turn would keep
the narrowing, and would make a turn nobody typed an addressed one, which is the question
[ADDRESS-3](#ADDRESS-3) answers no to for now.

**The cost of this clause is that one file reads two ways.** A definition naming `ask_user` under
`tools:` is a definition loaded without it when a planner spawns it, and with it when a person
addresses it. The alternative is withholding a tool from a person who has it, for a reason that
does not apply to them.

`verified-by: bravebot_core::policy::the_tools_no_delegate_is_offered_are_offered_to_an_addressed_turn`
`verified-by: bravebot_agent::turn::an_addressed_turn_runs_under_its_definitions_prompt_model_and_kind`
`verified-by: bravebot_agent::turn::an_addressed_turn_arranges_no_later_look_and_arms_no_watch`

## How long it lasts

<a id="ADDRESS-9"></a>
### ADDRESS-9: the exchange is the session's own

What the run read, what it ran and what it answered are in the transcript, in the record of the
session, and in the context the next prompt is answered from. Nothing is absorbed and nothing is
summarised into a report.

**Why.** A report exists so that a planner is told what failed instead of reading the whole log,
and the person watching this run has read it already. Handing them a summary of something they
watched happen would cost them the detail and buy nobody anything: the context this would save is
the context they are already looking at.

Addressing a definition therefore buys reachability and spends context, where delegating to one
buys context and spends reachability. The two are the same set of files reached two ways, and which
one a person wants depends on whether they are watching.

`verified-by: bravebot_agent::turn::the_turn_after_an_addressed_one_holds_the_exchange_and_not_the_definition`

<a id="ADDRESS-10"></a>
### ADDRESS-10: the definition's prompt lasts the turn it was addressed in

The next line is a prompt for the session's own planner unless it addresses a definition again.
There is no mode, nothing about the input box changes, and nothing has to be left.

**Why.** A mode would have to answer what happens to the conversation so far, what the box looks
like while it is on, what key leaves it, and what a queued line means when the mode changes under
it. What a person wants from a file they checked in is that it does the piece of work they named,
and the line they typed is where they named it. Addressing it twice costs one word.

`verified-by: bravebot_tui::app::an_addressed_line_runs_the_named_definition_on_its_task_for_one_turn`
`verified-by: bravebot_agent::turn::the_turn_after_an_addressed_one_holds_the_exchange_and_not_the_definition`

<a id="ADDRESS-11"></a>
### ADDRESS-11: a definition naming a model this machine cannot use does not run, and says which asked for what

Where the definition names a model that needs a sign-in this machine has not made, nothing runs and
the person is told which definition asked for which model. The session's own model is not
substituted. Where the endpoint answers with a model other than the one asked for, that is said
too, naming the definition and the model it named, and a reply from the model the definition named
is not reported as the session's model substituted. A delegate the run spawns whose own definition
names no model inherits the addressed definition's, not the session's.

**Why.** A definition naming a cheap model is often a cost boundary, and running it on the
session's model would spend past that boundary without anybody choosing to. The person is at the
keyboard, so a refusal costs them a sign-in and a second line, which is the outcome they would have
chosen. The substitution is said because the endpoint substitutes rather than refuses a name it
will not serve, so without it a definition could ask for one model and be answered by another every
time.

`verified-by: bravebot_agent::turn::an_addressed_definition_whose_model_needs_a_sign_in_sends_nothing_and_says_so`
`verified-by: bravebot_tui::app::a_definition_whose_model_needs_a_sign_in_runs_nothing_and_says_so`
`verified-by: bravebot_agent::turn::an_addressed_definition_answered_by_another_model_says_so`
`verified-by: bravebot_agent::turn::a_delegate_an_addressed_turn_spawns_inherits_the_definitions_model`
`verified-by: bravebot_tui::app::addressing_a_definition_that_names_a_model_carries_that_model`
`verified-by: bravebot_tui::app::an_addressed_turn_is_held_against_the_model_its_definition_named`

<a id="ADDRESS-12"></a>
### ADDRESS-12: the driver says which definition answered

A reply produced by an addressed run is drawn under the definition's name, and the name is the one
the driver resolved rather than anything the reply says about itself.

**Why.** A reply is model output. An interface reading one to decide which definition produced it
would be taking that decision from model output, which is the thing this repository refuses
everywhere else. Which definition was addressed is a fact the driver already holds, because it is
the one that matched the name.

`verified-by: bravebot_tui::render::a_reply_from_an_addressed_turn_is_drawn_under_the_name_the_driver_matched`

## Open questions

- **Whether the one-shot command line gets the same reach.** `bravebot -p` composes a task from an
  argument, which is a person's own line by the same argument [ADDRESS-3](#ADDRESS-3) makes for the
  input box, so a switch naming a definition would be sound. Whether it is wanted is a separate
  question from whether the interactive surface is, and answering it here would put a second
  surface in a spec that has not had its first agreed to.

- **Whether a definition may say that it is meant to be addressed.** Nothing above lets a file
  exclude itself from what a planner may select, or from what a person may address. A field saying
  which is a fifth thing a definition means, and the argument for it is that a helper written for
  one planner to call is noise in the list a person reads. The argument against is that the set is
  small and a person reading a name they do not recognise loses nothing by it.

- **What the desktop front end does with this.** Its bots carry a name, a purpose, a model and a
  memory file, which is most of a definition plus a store of what happened. Whether those become
  definitions addressed this way depends on this surface existing first, and on a definition
  gaining a memory and a checkout of its own, which is issue #727 and not settled here.

## Known costs

- **The name is typed in full, every time.** No completion and no history of names, so a person
  addressing a definition with a long name types it or recalls the whole line. That is the price of
  [ADDRESS-6](#ADDRESS-6), and it falls on the person who wrote the file and knows what it is
  called, which is the right place for it to fall.

- **One definition file means two slightly different things.** Which tools it ends up with depends
  on whether a planner spawned it or a person addressed it, because five of them are withheld from
  one and not the other. A person reading a definition cannot tell what it will hold without
  knowing how it is reached.

- **A long addressed run fills the context it was asked to do the work in.** Everything it reads
  lands in the session, so addressing a definition for a job that reads a large tree costs what
  doing the job in the session costs. A person who wanted the reading kept out of their context
  wanted a delegate, and the two are reached differently on purpose.

- **An addressed run cannot ask to be asked again, or be told when a file changes.** A person who
  wants a definition to look again addresses it again. That is the price of
  [ADDRESS-8](#ADDRESS-8)'s exception, and a person who wanted the session to keep looking can ask
  the session rather than the definition.

- **The name is drawn only in the session that addressed it.** The session record does not keep
  which definition answered, so a resumed session and an exported transcript show the reply as the
  session's own. What the reply says is kept whole; only the heading is missing.

- **A definition's body is trusted exactly as far as a configuration file somebody pasted is.**
  That cost is already recorded for a definition a planner spawns, and addressing one does not add
  to it: the body still decides only what the run is told it is for, and never what it may reach.
  What changes is that the run holding it can ask the person questions, so a body can shape how a
  question is phrased to somebody who is about to answer it.
