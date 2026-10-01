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
  - crates/cli/src/main.rs
  - crates/cli/src/plain.rs
  - crates/tui/src/status.rs
guards:
  - symbol: Policy::address
  - symbol: Session::address
documented-by:
  - docs/website/docs/reference/commands.md
  - docs/website/docs/customize/agents.md
  - docs/website/docs/reference/cli.md
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

`--agent <name>` on the command line selects a definition for every turn of an interactive session,
a session in lines or a one-shot run ([CLI-17](cli.md#CLI-17)). The name is matched against the same
set before the first turn, and each turn then carries it as a `/agent` line's turn does.

The desktop front end addresses nothing. [MEMORY-10](definition-memory.md#MEMORY-10) specifies
that every turn in a desktop bot's conversation addresses the bot's definition, and nothing yet
builds it.

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
model, and the skills it names ([DELEGATE-23](delegation.md#DELEGATE-23)).
[MEMORY-4](definition-memory.md#MEMORY-4) adds a fifth: where the memory it keeps is, which the run
reads itself. It supplies nothing else, and in particular it does not
make the run a delegate. The person is at the keyboard, so the run holds the screen, the confirmer,
the task list and the way to ask a question, exactly as any turn of theirs does.

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
like an instruction, and the rule that keeps a skill from being a command is exactly that.

**What the alternatives cost.** `@name` is taken: `@` names a file and putting a second meaning on it
would make one prefix mean two things at the moment a person is typing fastest. A bare
`name: task` line would make every sentence containing a colon a dispatch. A word plus an argument
is the shape every other command here already has, and the whole word is the command, so
`/agents are useful` stays a prompt.

A turn stopped before it did anything puts the whole line back in the box, name and all, as a
stopped prompt is put back. The task alone would be a prompt for the session's own planner, and
Enter on it would run the work under everything the definition was there to take away. In a session
started under that definition ([CLI-17](cli.md#CLI-17)) only the task comes back, because Enter on
it already addresses the definition.

`verified-by: bravebot_tui::app::a_session_can_address_a_definition`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_agent_is_a_prompt`
`verified-by: bravebot_tui::app::a_stopped_addressed_turn_puts_the_whole_agent_line_back_in_the_box`
`verified-by: bravebot_tui::app::a_stopped_turn_comes_back_naming_a_definition_only_where_the_session_would_not`

<a id="ADDRESS-3"></a>
### ADDRESS-3: only a line a person typed into the box, or the command line that started the session

The line comes off the input box and from nowhere else. Never a line the planner produced, never
text read out of a file, never anything a processor returned, never one reconstructed from a
transcript. A model that writes `/agent` has written six characters, and they reach a person's
screen as six characters.

**Why.** An addressed run works under a prompt the session's planner did not write and under a
narrowing it did not choose, which is a decision a turn is not allowed to take on its own. The
endorsement for it is the keystroke, so the keystroke is the only thing that may produce one.

The command line that started a session or a run is the other source. `--agent <name>`
([CLI-17](cli.md#CLI-17)) is an argument a person typed, so the reasoning above applies to it, and
it is read before any turn exists, so no turn's output can set it. It names the definition for every
turn of the session, while a `/agent` line names one for a single turn.

[MEMORY-10](definition-memory.md#MEMORY-10), which nothing yet builds, adds a second source: a
turn in a desktop bot's conversation addresses that bot's definition. The name comes from the
conversation a person opened rather than from a line, and nothing a turn produced chooses it, so
what stands in for the keystroke is making the bot and opening its conversation.

`verified-by: bravebot_tui::app::a_line_addressing_a_definition_queued_while_a_turn_ran_addresses_it_when_the_turn_ends`
`verified-by: bravebot_cli::main::the_agent_flag_is_taken_out_with_the_name_it_gave`
`verified-by: bravebot_tui::app::a_name_from_the_command_line_is_worked_under_where_it_was_written_and_refused_where_not`
`verified-by: by-construction (in the interface a name reaches a turn only through Session::address or Session::work_under; the first's one caller settles the Action::Address that only the /agent branch of dispatch_command returns, and dispatch_command is reached from the input box's key handler and from the queue that handler filled; the second's one caller is the event loop before its first turn, with the name main took off the command line; a session in lines and a one-shot run put a name on a Task only from that same argument; and the turn reads the name off the session as it starts, so nothing a turn produced sets one)`

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
are this program's own, and whatever definitions resolved from a source somebody vouched for, less
any in the project a `deny` rule covers ([PERM-7](permissions.md#PERM-7)). A name is compared
against it and matches or does not. A name matching nothing runs nothing and says so, and there is
no spelling of it that reaches a file nobody vouched for.

`/agent` with a name and no task runs nothing and says a task is needed, rather than starting a
definition on an empty one.

**Why.** Comparing the name decides nothing an attacker steers, because the name came from a
keystroke. The set it is compared against is the thing an attacker would want to write instead, and
that is already settled: a definition loads from a trusted source or it is dropped, so a name
nobody vouched for never entered the set to be matched.

A one-shot run does not ask whether to trust its directory, so it counts the checkout's definitions
without reading them. A name that only the checkout defines is refused, and the refusal says how
many definitions were not read ([CLI-17](cli.md#CLI-17)).

`verified-by: bravebot_core::policy::a_name_this_session_did_not_resolve_is_refused_with_the_names_it_did`
`verified-by: bravebot_agent::turn::a_name_this_session_did_not_resolve_sends_nothing_and_lists_what_it_did`
`verified-by: bravebot_agent::agents::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_agent::agents::a_definition_a_deny_rule_covers_is_selectable_nowhere_through_a_link_to_it`
`verified-by: bravebot_tui::app::a_name_this_session_did_not_resolve_runs_nothing_and_lists_what_it_did`
`verified-by: bravebot_tui::app::a_name_with_no_task_runs_nothing_and_says_a_task_is_needed`
`verified-by: bravebot_cli::running::a_run_refuses_a_definition_only_an_untrusted_checkout_holds_and_says_it_counted_one`

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

A skill's name is offered in that list, and the difference is what taking the row writes. A skill
row writes a prompt naming the skill, which the planner is asked about like any other sentence. A
definition's name is only ever the argument of `/agent`, so a row for one would write a command
line that starts a run.

`verified-by: bravebot_tui::app::the_bare_word_says_what_this_session_resolved`
`verified-by: by-construction (the completion list reads the command table and the resolved skills and nothing else, nothing is offered after the word of a command line, and the resolved set of definitions exists only in the event loop after a line is submitted, so nothing drawn while a person types can hold a definition name)`

## What it may do

<a id="ADDRESS-7"></a>
### ADDRESS-7: a definition narrows what the person already holds, and widens nothing

The run holds the session's own capabilities, intersected with the kind the definition names and
with the tools it names. A definition of a wider kind than the session gets a run with the
session's reach. A definition naming a tool the session is not offered gets a run without it, and
the trail says what was dropped. The run is told what it holds in the words a delegate is told
them, less the sentence about an agent that asked it, because the planner's own guidance around
them is written for a turn that can edit and run.

An MCP server's grant is held on a delegate's terms ([DELEGATE-4](delegation.md#DELEGATE-4),
[DELEGATE-24](delegation.md#DELEGATE-24)): a run under a worker keeps each server this session
reached that its definition selects, which is every one where it names neither its tools nor its
servers, is put the lists of those as the session's own turn is, and is offered their tools beside
the ones its definition leaves it. It holds no other server's grant, is offered none of that
server's tools, and that server's list is not put to the person for it. A run under a `reader` or
a `checker` holds no server's grant. A `checker` and a `worker` hold a language server and a
`reader` does not ([LSP-9](tools/lsp.md#LSP-9)), so a run addressed to either of the first two
keeps the `lsp` tool the session holds.

**Why.** Addressing a definition is a person choosing which of their own capabilities to work
under, so it can only take away. This is the same direction a delegate's capabilities are computed
in and for the same reason: a checked-in file may choose what a run is for and may never choose
what it may reach. A file that could add a capability would be a configuration file handing out
authority, which is the one thing a definition is not.

`verified-by: bravebot_core::policy::an_addressed_turn_holds_only_what_the_session_and_the_kind_both_hold`
`verified-by: bravebot_core::policy::a_definition_wider_than_the_session_gets_the_sessions_reach`
`verified-by: bravebot_core::policy::a_turn_addressed_to_a_checker_or_a_worker_keeps_the_language_server`
`verified-by: bravebot_core::policy::a_delegate_an_addressed_turn_spawns_holds_no_more_than_the_turn`
`verified-by: bravebot_core::policy::an_addressed_turn_is_offered_only_the_tools_its_definition_named`
`verified-by: bravebot_agent::turn::a_tool_an_addressed_definition_left_out_is_refused_when_the_model_calls_it`
`verified-by: bravebot_agent::turn::an_addressed_turn_is_told_what_its_definition_left_it`
`verified-by: bravebot_agent::mcp::an_addressed_reader_is_offered_no_servers_tool_and_asks_about_no_list`
`verified-by: bravebot_agent::mcp::an_addressed_worker_is_put_the_list_and_calls_the_servers_tool`
`verified-by: bravebot_core::policy::an_addressed_worker_keeps_the_sessions_servers_and_a_reader_gives_them_up`
`verified-by: bravebot_core::policy::an_addressed_worker_naming_its_servers_keeps_only_those`
`verified-by: bravebot_core::policy::an_addressed_reader_naming_servers_holds_none_and_the_trail_says_why`
`verified-by: bravebot_agent::mcp::an_addressed_worker_naming_one_server_is_put_its_list_alone`

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
the narrowing, and would make a turn the run chose an addressed one, which
[ADDRESS-3](#ADDRESS-3) leaves to a person. In a session started under a definition
([CLI-17](cli.md#CLI-17)) every turn is addressed, so the later turn would keep the narrowing. The
two are still withheld there, because the run would still choose when that turn happens, and a
definition is offered the same tools whether `/agent` or `--agent` selected it.
[MEMORY-10](definition-memory.md#MEMORY-10), which nothing yet builds, would address two kinds of
turn nobody typed in a bot's conversation, the desktop's own turn and the fire of a watch a person
armed there, and neither is this kind: the run chooses a later look, and chooses neither of those.

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

A person can also select a definition before the session starts, with `--agent`
([CLI-17](cli.md#CLI-17)). Every turn that session sends is addressed to the definition, including
`/loop` ticks and `/goal` rounds. A `/agent` line naming another definition addresses that one for
one turn, and the next line goes to the session's definition again. The input box does not change,
and `/status` names the definition, because the note shown at the start scrolls away.

**Why.** A mode would have to answer what happens to the conversation so far, what the box looks
like while it is on, what key leaves it, and what a queued line means when the mode changes under
it. What a person wants from a file they checked in is that it does the piece of work they named,
and the line they typed is where they named it. Addressing it twice costs one word.

`--agent` does not raise those questions. The choice is made once, before there is a conversation,
a queued line or a key to leave by, and a person who wants the planner back starts a new session.
Ticks and rounds are addressed too, because addressing only narrows what a turn can do
([ADDRESS-7](#ADDRESS-7)). An unaddressed tick would have more tools than the turns the person
typed.

`verified-by: bravebot_tui::app::an_addressed_line_runs_the_named_definition_on_its_task_for_one_turn`
`verified-by: bravebot_tui::app::a_session_started_under_a_definition_addresses_every_turn_a_loop_tick_included`
`verified-by: bravebot_tui::app::a_definition_named_on_the_line_lasts_one_turn_under_the_one_the_session_works_under`
`verified-by: bravebot_tui::status::the_report_names_the_definition_every_turn_is_addressed_to`
`verified-by: bravebot_agent::turn::the_turn_after_an_addressed_one_holds_the_exchange_and_not_the_definition`

<a id="ADDRESS-11"></a>
### ADDRESS-11: a definition naming a model this machine cannot use does not run, and says which asked for what

Where the definition names a model that needs a sign-in this machine has not made, nothing runs and
the person is told which definition asked for which model. The session's own model is not
substituted. Where the endpoint answers with a model other than the one asked for, that is said
too, naming the definition and the model it named, and a reply from the model the definition named
is not reported as the session's model substituted. A delegate the run spawns whose own definition
names no model inherits the addressed definition's, not the session's. Where `--model` named a model
for a one-shot run ([CLI-17](cli.md#CLI-17)), that model is asked for in place of the definition's,
and the run says which model the definition asked for and was not given.

**Why.** A definition naming a cheap model is often a cost boundary, and running it on the
session's model would spend past that boundary without anybody choosing to. The person is at the
keyboard, so a refusal costs them a sign-in and a second line, which is the outcome they would have
chosen. The substitution is said because the endpoint substitutes rather than refuses a name it
will not serve, so without it a definition could ask for one model and be answered by another every
time. The command line outranks the definition for the reason in [CLI-9](cli.md#CLI-9): `--model`
names the model for this one invocation. The run says so, so that whoever reads it knows the
definition did not get the model it asked for.

`verified-by: bravebot_agent::turn::an_addressed_definition_whose_model_needs_a_sign_in_sends_nothing_and_says_so`
`verified-by: bravebot_tui::app::a_definition_whose_model_needs_a_sign_in_runs_nothing_and_says_so`
`verified-by: bravebot_agent::turn::an_addressed_definition_answered_by_another_model_says_so`
`verified-by: bravebot_agent::turn::a_delegate_an_addressed_turn_spawns_inherits_the_definitions_model`
`verified-by: bravebot_tui::app::addressing_a_definition_that_names_a_model_carries_that_model`
`verified-by: bravebot_tui::app::an_addressed_turn_is_held_against_the_model_its_definition_named`
`verified-by: bravebot_agent::turn::a_model_the_command_line_named_outranks_the_definitions_and_the_turn_says_so`

<a id="ADDRESS-12"></a>
### ADDRESS-12: the driver says which definition answered

A reply produced by an addressed run is drawn under the definition's name, and the name is the one
the driver resolved rather than anything the reply says about itself.

[MEMORY-10](definition-memory.md#MEMORY-10), which nothing yet builds, draws a desktop bot's reply
under the name the bot is shown under instead. That name comes from the row the person opened, as
the definition's name does, so no reply chooses it there either.

**Why.** A reply is model output. An interface reading one to decide which definition produced it
would be taking that decision from model output, which is the thing this repository refuses
everywhere else. Which definition was addressed is a fact the driver already holds, because it is
the one that matched the name.

`verified-by: bravebot_tui::render::a_reply_from_an_addressed_turn_is_drawn_under_the_name_the_driver_matched`

## Open questions

- **Whether a definition may say that it is meant to be addressed.** Nothing above lets a file
  exclude itself from what a planner may select, or from what a person may address. A field saying
  which is one more thing a definition means, and the argument for it is that a helper written for
  one planner to call is noise in the list a person reads. The argument against is that the set is
  small and a person reading a name they do not recognise loses nothing by it.

- **What the desktop front end does with this.** [definition-memory.md](definition-memory.md)
  proposes an answer: a definition keeps a memory, which is built, and each desktop bot becomes a
  definition its conversation addresses, which is not, so until it is the desktop addresses
  nothing.

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

- **A session started under a definition cannot be resumed under it.** The session record does not
  keep the name, so `--agent` is refused with `--resume`, `--continue` and `--fork`, and a session
  resumed without it goes to the planner from then on.

- **A self-paced `/loop` in such a session stops after one tick.** The tick is addressed, so it
  cannot schedule the next one ([ADDRESS-8](#ADDRESS-8)). A `/loop` with an interval is scheduled
  by the session and keeps running.

- **The name is drawn only in the session that addressed it.** The session record does not keep
  which definition answered, so a resumed session and an exported transcript show the reply as the
  session's own. What the reply says is kept whole; only the heading is missing.

- **A definition's body is trusted exactly as far as a configuration file somebody pasted is.**
  That cost is already recorded for a definition a planner spawns, and addressing one does not add
  to it: the body still decides only what the run is told it is for, and never what it may reach.
  What changes is that the run holding it can ask the person questions, so a body can shape how a
  question is phrased to somebody who is about to answer it.
