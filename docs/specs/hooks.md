---
id: HOOK
title: Hooks
status: normative
governs:
  - crates/config/src/hooks.rs
  - crates/agent/src/hooks.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/turn/setup.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/turn/sentences.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/confirm.rs
  - crates/agent/src/permission_mode.rs
  - crates/ui-bridge/src/hooks.rs
  - crates/ui-files/src/main.rs
  - ui/src/shared/agent-settings.ts
  - ui/src/main/agent-settings.ts
  - ui/src/renderer/components/AgentSettings.tsx
documented-by: docs/website/docs/customize/hooks.md
---

## Scope

A command a person asked to have run when something happens: format after an edit, notify when a
turn is over. This file covers where one is declared, which moments exist, what a hook is handed,
what it may decide, what happens when one goes wrong, and who reads the declarations back.

What a hook is *not* is a way to write policy. Rules decided in advance about what to ask about and
what to refuse are [permissions.md](permissions.md), and they are rules rather than programs for
the reason the clauses below give: a program that decides is a program whose decision came from
whatever it read.

Running an ordinary subprocess because the planner asked for one is [tools/run.md](tools/run.md),
which is a different thing entirely: that one the model chooses, a person approves, and its output
comes back into the conversation.

## Clauses

<a id="HOOK-1"></a>
### HOOK-1: a hook is declared in the user's own state directory and nowhere else

Hooks are read from `hooks.json` in `~/.bravebot`, which is the directory holding what is the
user's own. No other file declares one. A checkout carries no hooks, a directory somebody happens
to start a session in carries none, and there is no layer, no merge and no override: one file, in
one place, and where the platform names no profile directory there are no hooks at all.

**Why.** The settings files are read from three places and two of them sit in a checkout, so a
command named in one would be a command that arrived with a clone. That is why
[backends.md](backends.md) says a settings file may name a destination and never a command, and
nothing here weakens it: these are not settings files.

Reading the user's own settings layer and ignoring the project ones is the alternative, and it is
weaker than it looks. Those three layers combine per name, by a table saying which blocks merge and
which are replaced, so "the user's own layer only" is a rule about that table rather than a property
of the file: an edit to the merge rules hands the name back to a project layer and nothing notices.
A separate file in the state directory cannot be carried by a checkout however the settings reader
changes. It costs a person who knows where a comparable tool keeps its hooks one more location to
learn.

`verified-by: bravebot_config::hooks::the_declarations_are_read_from_the_state_directory`
`verified-by: bravebot_config::hooks::no_state_directory_means_no_hooks`
`verified-by: by-construction (one path is ever built, by joining the file name onto the state directory the caller was given; nothing reads a hooks file from a working directory or an ancestor of one)`

<a id="HOOK-2"></a>
### HOOK-2: six moments, and each fires the hooks written for it

A declaration names one moment. These are the moments:

| Moment | Fires |
|---|---|
| `turn-started` | before the turn a person asked for sends anything |
| `tool-finished` | after one tool call has finished, whatever came of it |
| `turn-finished` | when the turn a person asked for is over, however it ended |
| `prompt-shown` | just before a prompt or an `ask_user` question is put to a person who can answer it |
| `checkout-made` | once the driver has written a checkout for a delegate, before that delegate's first request |
| `compacted` | after a summary has been adopted in place of the older part of the conversation |

A `tool-finished` entry may name a `tool`, and then fires for calls of that name alone; one that
names none fires for every call. A tool named on any of the other moments is about a call
that does not happen there, and that entry fires for nothing. Two entries on one moment are two
commands, run one after the other in the order the file listed them. A word this build does not
fire declares no hook and leaves the rest of the file in force.

The turn moments belong to the turn a person asked for. A delegate is a run inside that turn,
started by a call nobody typed, so a turn that spawns three delegates still fires `turn-started`
once. A call a delegate made is a call, and fires `tool-finished` like any other.

`prompt-shown` is about a question that reaches a person, so it fires once for each one that does and
for none that is answered without them: not where nobody can be asked ([PROMPT-9](prompting.md#PROMPT-9)),
and not where a permission mode answered in advance ([permission-modes.md](permission-modes.md)). A
delegate's question reaches the same person through the same confirmer and fires it once. It fires
before the question is drawn, so a notifier attached to it calls somebody back while the turn is
waiting on them, and the question is the same whatever the hook does (HOOK-6).

`checkout-made` is about a checkout and not about a delegate starting, so it fires once for each
checkout the driver writes for a delegate ([CHECKOUT-5](checkouts.md#CHECKOUT-5)), as that delegate
begins its run, before its first request. A delegate that works in the working directory fires
nothing, and so does one started inside a checkout, which works in the checkout its parent was given
([CHECKOUT-3](checkouts.md#CHECKOUT-3)). It fires from the delegate's own run, so a hook holds that
delegate and neither the planner nor the other delegates of the call, and a call that gives several
delegates a checkout runs their hooks at the same time. A person's setup command for a new checkout
is what it is for ([CHECKOUT-22](checkouts.md#CHECKOUT-22)).

`compacted` is about a conversation that was shortened, so it fires once for each summary adopted
([COMPACT-1](compaction.md#COMPACT-1), [COMPACT-10](compaction.md#COMPACT-10)) and for none that was
not: not one the gate refused, not one whose request failed or was stopped, and not a compaction
asked for where there was nothing to cut. It fires for a summary the budget forced, which includes
the one that answers a request the service refused as written
([COMPACT-14](compaction.md#COMPACT-14)), and for `/compact`, after the same point in each and
before the next request is sent. It does not fire for a delegate summarising the conversation of
its own, which the person never held. It is told the moment and nothing of the summary, the
messages it replaced or how many (HOOK-5), the compaction is the same whatever the hook does
(HOOK-6), and a hook that fails leaves the conversation shortened and is said to the person
(HOOK-7).

**Why.** A short vocabulary in a person's own terms, rather than the events the audit trail
records: those say which gate decided what, and somebody wanting their formatter to run should not
have to learn what a slot or a capability is. Each of these is something a person can point at in a
session that has happened.

`turn-finished` fires however the turn ended, including one that was cancelled or that failed on
its first request, because the commonest thing to attach to it is telling somebody who walked away,
and a turn that died is exactly when they want telling.

`verified-by: bravebot_config::hooks::each_moment_is_named_by_the_word_a_file_spells_it_with`
`verified-by: bravebot_config::hooks::a_moment_this_build_does_not_fire_is_not_a_hook`
`verified-by: bravebot_config::hooks::a_moment_fires_only_the_hooks_written_for_it`
`verified-by: bravebot_config::hooks::a_tool_named_on_a_turn_moment_fires_for_nothing`
`verified-by: bravebot_config::hooks::a_hook_naming_a_tool_fires_for_that_tool_alone`
`verified-by: bravebot_config::hooks::a_hook_naming_no_tool_fires_for_every_call`
`verified-by: bravebot_config::hooks::two_entries_for_one_moment_both_fire_in_order`
`verified-by: bravebot_agent::turn::a_hook_fires_when_the_turn_begins_and_when_it_is_over`
`verified-by: bravebot_agent::turn::a_hook_fires_when_the_tool_it_names_finishes`
`verified-by: bravebot_agent::turn::a_hook_fires_for_a_call_that_was_refused`
`verified-by: bravebot_agent::turn::a_delegate_fires_neither_turn_moment`
`verified-by: bravebot_agent::turn::a_hook_fires_when_the_turn_is_over_though_it_was_cancelled`
`verified-by: bravebot_agent::turn::a_delegate_does_not_fire_the_turn_s_own_moments`
`verified-by: bravebot_agent::turn::a_hook_fires_when_a_question_is_put_to_a_person`
`verified-by: bravebot_agent::turn::an_unattended_run_fires_no_prompt_hook`
`verified-by: bravebot_agent::permission_mode::the_alert_comes_before_a_question_the_mode_did_not_answer`
`verified-by: bravebot_agent::permission_mode::no_alert_comes_before_a_question_the_inner_confirmer_refuses_unasked`
`verified-by: bravebot_cli::main::a_one_shot_run_alerts_for_a_plan_it_puts_and_not_for_a_write_it_refuses`
`verified-by: bravebot_agent::turn::a_delegate_s_question_fires_prompt_shown_once`
`verified-by: bravebot_config::hooks::a_checkout_hook_fires_at_that_moment_alone`
`verified-by: bravebot_config::hooks::a_tool_named_on_the_checkout_moment_fires_for_nothing`
`verified-by: bravebot_agent::turn::a_hook_runs_in_the_checkout_made_for_a_delegate_before_its_first_round`
`verified-by: bravebot_agent::turn::a_delegate_given_no_checkout_fires_no_checkout_hook`
`verified-by: bravebot_agent::turn::a_fan_out_into_checkouts_fires_the_hook_once_for_each_checkout_and_in_that_one`
`verified-by: bravebot_agent::turn::a_delegate_started_inside_a_checkout_does_not_fire_the_hook_again`
`verified-by: bravebot_agent::turn::a_hook_fires_when_the_budget_forces_a_compaction`
`verified-by: bravebot_agent::turn::a_hook_fires_when_a_person_compacts_the_conversation`
`verified-by: bravebot_agent::turn::a_hook_fires_when_a_refused_request_is_answered_by_a_compaction`
`verified-by: bravebot_agent::turn::a_compaction_runs_no_hook_that_was_written_for_another_moment`
`verified-by: bravebot_agent::turn::a_summary_the_budget_forced_and_the_gate_refused_fires_no_hook`
`verified-by: bravebot_agent::turn::a_summary_a_person_asked_for_and_the_gate_refused_fires_no_hook`
`verified-by: bravebot_agent::turn::a_conversation_with_nothing_to_cut_fires_no_hook`
`verified-by: bravebot_agent::turn::a_summary_that_could_not_be_made_fires_no_hook`
`verified-by: bravebot_agent::turn::a_delegate_s_compaction_fires_no_compacted_hook`
`verified-by: bravebot_tui::app::the_thread_that_summarises_fires_the_hooks_in_the_sessions_directory`

<a id="HOOK-3"></a>
### HOOK-3: a hook is a program and its arguments, never a line a shell parses

A declaration names the program and each argument as a separate string. Nothing joins them into a
command line and nothing re-parses one, so an argument holding a space, a semicolon or a quote is
one argument and arrives as one. An entry whose command is a single string, whose arguments are not
all strings, or which names no program at all declares no hook.

**Why.** A line a shell parses is a place where quoting decides what runs, and what is quoted is a
path somebody wrote once and stopped reading. The rest of this repository refuses that already: the
command line the planner asks for is compiled rather than interpreted, and a subprocess is spawned
from a vector. A hook is the easiest place for a shell to reappear, so the rule is stated rather
than left to whoever writes the next one.

It is a deliberate difference from how a comparable tool spells a hook, whose command is a line.
Copying the spelling means copying the parse.

`verified-by: bravebot_config::hooks::a_hook_is_the_argument_vector_the_file_listed`
`verified-by: bravebot_config::hooks::a_command_written_as_one_string_is_not_a_hook`
`verified-by: bravebot_config::hooks::an_argument_that_is_not_a_string_drops_the_entry`
`verified-by: bravebot_config::hooks::an_entry_with_no_program_is_not_a_hook`
`verified-by: bravebot_agent::hooks::an_argument_holding_shell_syntax_arrives_as_one_argument`

<a id="HOOK-4"></a>
### HOOK-4: a hook runs where the work is, with the access the person's own shell would give it

A hook runs in the directory the turn is working in, so a relative path in one means what the
person who wrote it meant. For `checkout-made` that is the checkout's root. It is not confined, and
its environment is the one it was started from less this agent's own credentials, which are the
same names taken off a program the planner asked for, a gateway's token variables included
([RUN-12](tools/run.md#RUN-12)), and with its `PATH` reduced as described below.

A bare program name is the one exception to "the person's own shell would give it": it is looked up
as every program bravebot starts unconfined is ([SANDBOX-30](sandboxing.md#SANDBOX-30)), so a
`.venv/bin/ruff` a confined stage wrote is not the `ruff` that runs. A hook whose bare name is found
only there is reported as not started. A name with a separator in it is the person's path and is
never looked up.

The names a hook starts itself are looked up the same way. A hook is started with bravebot's `PATH`
reduced to the entries that lookup searches, whether it was named bare or by a path, so a
`#!/usr/bin/env node` shebang finds `node` only among them and a `.venv/bin/node` a confined stage
wrote is not the `node` that runs. A hook that starts a program by name found only in a directory
left out does not find it.

**Why.** The person named this command in a file in their own directory, so it is their program on
their machine, and [sandboxing.md](sandboxing.md) confines processes running code we did not write.
Whatever is decided there about bounding a program a person asked for reaches a hook too, and by
the same argument: what a formatter or a notifier needs cannot be enumerated in advance.

The credentials come off for the reason they come off a run: a person reads the program and the
arguments they wrote down, so a secret travelling beside those is something they were never shown.
A hook has no more use for one than a formatter has.

`verified-by: bravebot_agent::hooks::a_hook_runs_the_program_the_file_named`
`verified-by: bravebot_agent::hooks::a_hook_runs_in_the_directory_the_turn_is_working_in`
`verified-by: bravebot_agent::turn::a_hook_runs_in_the_checkout_made_for_a_delegate_before_its_first_round`
`verified-by: bravebot_agent::hooks::a_hook_is_not_handed_this_agent_s_own_credentials`
`verified-by: bravebot_agent::hooks::a_hook_is_not_handed_a_gateways_environment_token`
`verified-by: bravebot_agent::hooks::two_hooks_on_one_moment_run_in_the_order_the_file_listed_them`
`verified-by: bravebot_agent::hooks::a_hook_named_bare_never_starts_a_file_found_only_where_a_stage_may_write`
`verified-by: bravebot_agent::hooks::a_hook_named_by_a_path_is_not_looked_up`
`verified-by: bravebot_agent::hooks::a_hook_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written`

<a id="HOOK-5"></a>
### HOOK-5: a hook is told which moment fired, and a checkout's hook which two directories

A hook is given one line on its standard input: a JSON object whose `event` is the moment's own
word. No argument a model wrote, no bytes out of a file, and no path, with one exception: the hook
for `checkout-made` is given two directories in its environment. `BRAVEBOT_CHECKOUT` is the
checkout it was fired for and `BRAVEBOT_WORKING_DIRECTORY` is the working directory the checkout was
made from. They are the whole of what a hook is handed besides the moment, they are not on standard
input, and bravebot adds neither to the environment of the hook for any other moment. The tool a
`tool-finished` entry names selects which entries fire and reaches no process.

**Why.** There is then nothing to work out about what a hook is handed: the same line for a moment,
whatever the turn was doing. A path would be a string the model chose, and while a path a tool acts
on is trusted for deciding where that effect lands, handing one to a program on somebody's machine
is a wider claim than that. A hook that needs to know which tool ran declares one entry per tool.

A setup command for a new checkout has one question the moment cannot answer: where the files a
checkout lacks are, which is the directory it was made from. Neither directory is a string a model
wrote or one read out of a file. The checkout's is built from the state directory, the workspace's
key and the checkout's number, which are the driver's, and the working directory is the one the
person's session works in. Two are handed and no more, since a copy needs a source and a
destination. The destination is also the hook's own directory (HOOK-4), and the variable names it
for a command that has changed directory. They are variables rather than a field of the line
because a variable holds the bytes the file system holds, and a directory whose name is not text
would reach a hook as another directory through a line of JSON.

`verified-by: bravebot_agent::hooks::a_hook_is_told_the_moment_and_nothing_else`
`verified-by: bravebot_agent::hooks::the_line_a_hook_is_told_is_built_from_the_moment_alone`
`verified-by: bravebot_agent::hooks::a_hook_is_given_no_environment_of_its_own`
`verified-by: bravebot_agent::hooks::a_checkout_hook_is_handed_the_checkout_and_the_working_directory_it_was_made_from`
`verified-by: bravebot_agent::hooks::a_checkout_hook_is_handed_a_directory_whose_name_is_not_text_as_it_is`
`verified-by: bravebot_agent::hooks::a_checkout_hook_runs_in_a_checkout_whose_name_is_not_text_and_is_told_it_as_it_is`
`verified-by: bravebot_agent::hooks::a_hook_for_any_other_moment_is_given_neither_directory`
`verified-by: bravebot_agent::turn::a_hook_runs_in_the_checkout_made_for_a_delegate_before_its_first_round`

<a id="HOOK-6"></a>
### HOOK-6: a hook decides nothing

Nothing a hook returns or prints changes what happens next. Its exit status is used for one thing,
which is telling the person their hook is failing, and its output is not read at all. No hook
refuses a tool call, holds one up for an answer, alters an argument, or ends a turn.

**Why.** A hook runs beside a turn that is reading files, so anything it says is a function of
bytes nobody vouched for: a hook that grepped a file and exited non-zero would be that file
deciding whether the next tool call happens. Refusing is not the safe direction of that. A file
able to stop exactly the writes it did not want is choosing what the agent does, which is the thing
this repository exists to prevent, and it would do it through a channel no gate can see, because
what came back is one number from a program.

That is the cost, stated plainly: a secret scanner that blocks a write is not a hook, and cannot be
one. A rule about what to refuse is written as a rule ([permissions.md](permissions.md)), where the
decision is taken from something a person wrote rather than from something a program read.

`verified-by: bravebot_agent::turn::a_turn_whose_hook_failed_still_answers`
`verified-by: bravebot_agent::turn::a_failing_prompt_hook_leaves_the_question_alone`
`verified-by: bravebot_agent::turn::a_checkout_hook_that_fails_is_said_and_its_delegate_still_runs`
`verified-by: bravebot_agent::turn::a_setup_that_cannot_start_or_outstays_its_bound_leaves_the_delegate_running_in_a_kept_checkout`
`verified-by: bravebot_agent::turn::a_failing_hook_does_not_undo_the_compaction_the_budget_forced`
`verified-by: bravebot_agent::turn::a_failing_hook_does_not_undo_a_compaction_a_person_asked_for`
`verified-by: by-construction (a hook's standard output and standard error are discarded at the spawn and read by nothing; its status reaches one sentence said to the person and no branch)`

<a id="HOOK-7"></a>
### HOOK-7: a hook that goes wrong is said out loud, bounded, and ends nothing

A hook that could not be started, that ended badly, or that was still running when its bound ran out
and was stopped, is said to the person, naming the moment, the program and, for a hook that was stopped, the
bound in seconds. The bound is 30 seconds unless the entry states a `timeout`, which is a whole number
of seconds from 1 to 600; an entry whose `timeout` is anything else declares no hook, as HOOK-3 treats
a malformed argument. It reaches a live display as
it happens and the turn's own account of itself as well, so a run with nowhere to draw says it too.
A `compacted` hook that `/compact` fires has no turn and so no account, and reaches the live display
alone.
A turn that fails has no account, and a caller that draws nothing as the turn runs keeps what it was
told and says it beside the failure.
A moment inside a delegate is a moment of the turn that spawned it and reaches that turn's account,
a delegate keeping none anybody reads, and it reaches it whether or not the delegate went on to
answer. The turn carries on in every case. A hooks file that is missing, unparseable, larger than
64 KB, or that holds an entry this build cannot use, is that much of it not applying and never a
session that will not start.

**Why.** A hook holds the turn open while it runs, and a person watching cannot tell a slow hook
from a slow model, so one that does not finish is stopped rather than waited on. Nothing a hook
prints is read, so the report is the only way anybody learns their formatter has not run since they
mistyped its path, and silence there is a hook that quietly stopped working.

The ceiling on `timeout` is there for the same reason as the default: every hook is time added to
the turn, and a typo of an extra zero must not hold one open for hours. A `timeout` outside the range
drops the entry rather than being clamped, because the default or the ceiling would be a bound the
person did not write.

A mistake in this file must not be a program that will not open. It is the user's own file, written
by hand, and the failure it most often has is a typo.

`verified-by: bravebot_agent::hooks::a_hook_that_ends_badly_is_reported`
`verified-by: bravebot_agent::hooks::a_hook_whose_program_is_not_there_is_reported`
`verified-by: bravebot_agent::hooks::a_hook_that_outstays_the_bound_is_stopped`
`verified-by: bravebot_agent::hooks::a_hook_is_stopped_at_the_timeout_it_declared`
`verified-by: bravebot_agent::hooks::a_hook_may_outlive_the_default_bound_by_declaring_more`
`verified-by: bravebot_agent::turn::a_stopped_hook_says_the_timeout_it_declared`
`verified-by: bravebot_config::hooks::an_entry_may_state_its_own_timeout`
`verified-by: bravebot_config::hooks::an_unusable_timeout_drops_the_entry`
`verified-by: bravebot_agent::turn::a_turn_that_failed_still_says_what_its_hooks_said`
`verified-by: bravebot_cli::running::a_run_whose_turn_failed_still_says_what_its_hooks_said`
`verified-by: bravebot_ui_bridge::reporting::what_the_turn_said_is_kept_for_the_event_that_ends_it`
`verified-by: bravebot_agent::turn::a_hook_that_went_wrong_on_a_delegate_s_call_reaches_the_turn_s_notices`
`verified-by: bravebot_agent::turn::a_delegate_that_did_not_finish_still_tells_the_turn_what_its_hooks_said`
`verified-by: bravebot_agent::turn::a_checkout_hook_that_fails_is_said_and_its_delegate_still_runs`
`verified-by: bravebot_agent::hooks::a_file_that_declared_nothing_fires_nothing`
`verified-by: bravebot_config::hooks::an_unusable_entry_leaves_the_others_firing`
`verified-by: bravebot_config::hooks::an_unparseable_file_declares_no_hooks`
`verified-by: bravebot_config::hooks::an_oversized_file_declares_no_hooks`
`verified-by: bravebot_config::hooks::a_hooks_file_is_read_up_to_64_kb_and_not_a_byte_past_it`
`verified-by: bravebot_agent::hooks::the_default_bound_is_thirty_seconds`

<a id="HOOK-8"></a>
### HOOK-8: one reader decides what a hook is, and what it read is what is shown

The declarations are read in one place. A surface offering somebody their own hooks is told what that
reader made of the file rather than reading the text itself: the entries, which of them names a call
that does not happen at its moment, and whether anything in the file was passed over. A surface
composing the file back from the entries it was given does that only where it was told the whole file
was read, and otherwise says so and leaves the file to be edited by hand. It may decline to send a
field somebody left empty, and decides nothing else about what a hook is: a form that wrote an entry
this reader takes nothing from would have written a file it then refuses to edit.

**Why.** The rules above are lenient deliberately, and [HOOK-7](#HOOK-7) is why: an unknown moment,
a key this build does not read and an unusable entry are each that much of the file not applying,
because a mistake in it must not be a program that will not open. A second reader written for an
editor cannot hold that. Written strictly it refuses files a turn runs, so somebody is shown an error
about a hook that is firing; written leniently it drops whatever it did not recognise the next time
this file grows a key, which is somebody's own file rewritten without being asked. The desktop
interface had the first: its own parser, refusing any file carrying a key this reader passes over, and
its own pair of file operations to read one through.

Writing the file is a different question and stays where it is. Replacing a fixed name under a pinned
directory without following a link out of it is machinery, not a second opinion about what a hook is,
and what it writes is read back by the reader above, so what somebody is shown after saving is what a
turn would fire.

`verified-by: bravebot_config::hooks::a_file_read_for_showing_reports_the_text_it_was_read_from`
`verified-by: bravebot_config::hooks::whatever_went_unread_leaves_the_file_not_wholly_read`
`verified-by: bravebot_config::hooks::a_file_that_is_not_there_is_wholly_read_and_one_too_large_is_not`
`verified-by: bravebot_config::hooks::an_entry_says_whether_it_fires_for_nothing`
`verified-by: bravebot_ui_bridge::hooks::a_front_end_is_told_the_entries_the_agent_read`
`verified-by: bravebot_ui_bridge::hooks::a_file_the_agent_did_not_wholly_read_says_so`
`verified-by: bravebot_ui_bridge::dispatch::the_hooks_file_is_read_through_the_bridge`
`verified-by: by-construction (the desktop interface parses no hooks file: ui/src/shared/agent-settings.ts composes one out of the entries the agent reported and reads none, its main process offers a save channel and no read channel, and the helper binary that writes the file answers no read operation for it, which its own test asserts; ui/scripts/drive-agent-settings.mjs drives the panel against a file the agent reports it did not wholly read and asserts the form refuses it, and ui/scripts/drive-manual-walkthrough.mjs edits, fires and removes a real hook through that panel under CI)`

<a id="HOOK-9"></a>
### HOOK-9: a hook holding the turn is said to be running, by moment and program, while it runs

From just before a hook is started until it has ended, a display is told that a hook is running, for
which moment and which program. The moment is the word the hooks file spells it with and the program
is the first word of the command the person wrote there, so every byte is the driver's own or the
person's own and none is read from anything untrusted. The end is told however the hook ended: one
that finished, one that failed, one that could not be started and one stopped at its bound. Two hooks
on one moment are two pairs, one after the other. It is a transient status and not a notice: it
leaves no line behind, and a hook that went well is still said nothing about (HOOK-7). Nothing the
hook printed is part of it (HOOK-6), and nothing branches on it.

**Why.** A hook holds the turn open, and without this a person is told only once it has ended, so a
formatter running for twenty seconds after a write looks like a slow model. Naming the program lets
a person see whose command it is.

It is its own pair of reporter calls rather than a phase, because a phase has no end: the next phase
announcement is what replaces one, and a hook that runs after the last round has none to follow it.
A check is announced the same way, for the same reason.

`verified-by: bravebot_agent::hooks::a_hook_is_announced_before_it_starts_and_over_after_it_exits`
`verified-by: bravebot_agent::hooks::a_stopped_hook_is_over_only_after_it_was_stopped`
`verified-by: bravebot_agent::hooks::a_hook_that_never_started_is_still_closed`
`verified-by: bravebot_agent::hooks::two_hooks_are_two_pairs_in_order`
`verified-by: bravebot_agent::hooks::a_moment_with_no_hook_announces_nothing`
`verified-by: bravebot_agent::turn::a_reporter_is_told_which_hook_holds_the_turn`
`verified-by: bravebot_ui_bridge::reporting::a_running_hook_crosses_as_a_pair_naming_the_moment_and_the_program`
`verified-by: bravebot_tui::state::a_running_hook_names_the_indicator_and_gives_it_back`
`verified-by: bravebot_tui::app::a_hook_running_after_a_summary_is_named_and_its_trouble_said`

`prompt-shown` is the exception to the first sentence: it fires from the confirmer, which has no
display to hand, and a running status drawn over the prompt it precedes would be gone before the
prompt is. What it says when it goes wrong is still said (HOOK-7), as soon as the turn next has
somewhere to say it, which is the end of the call the question came up in.

## The file

```json
{
  "hooks": [
    { "on": "tool-finished", "tool": "write_file", "run": ["cargo", "fmt"], "timeout": 120 },
    { "on": "turn-finished", "run": ["/usr/bin/osascript", "-e", "display notification \"done\""] }
  ]
}
```

## Known costs

- **A hook is told the moment and not the path.** The formatter above runs over the project rather
  than over the file that changed, because HOOK-5 hands a hook nothing a model wrote. For a
  formatter that is a slower run of the same work; for something that wanted to act on one file it
  is the feature not being there. Widening what a hook is told is a decision to take when a use
  case needs it, and taking it late is cheap while taking it back is not.
- **Nothing a hook prints is kept.** Its output goes nowhere, so debugging one means having it
  write a file. Showing it would mean a program's bytes on the screen of a session whose whole
  arrangement is about which bytes reach which reader, and collecting it would mean holding output
  that nothing is allowed to read.
- **A hook runs while the turn waits.** Every hook is time added to the turn, up to the bound in
  HOOK-7 per hook, which is 30 seconds or the entry's own `timeout` up to 600. A `tool-finished` hook on every call pays that on every call. Running one
  without waiting is not offered, because then a formatter and the next tool call would be reading
  and writing the same file at once.
- **A hook named by a relative path runs what a stage may have written.** A name with a separator
  is the person's path and runs from the directory the turn works in (HOOK-4), which a stage of
  `run` may write, so `./scripts/format.sh` runs, unconfined and with the person's access, whatever a
  confined stage left at that name. Looking such a name up as a bare name is would refuse the
  person's own script in their own project. A hook that must not run a file a stage could write
  names its program by an absolute path outside the session's directories. A `checkout-made` hook
  runs in a checkout of HEAD, so there `./scripts/setup.sh` runs the repository's own file at that
  name, which is a command that arrived with a clone, and a hook that must not do that names its
  program by an absolute path outside the repository.
- **A tool a hook starts by name is found only among the entries bravebot searches.** A hook that is
  a script with an `env` shebang, or a launcher that starts another program by name, has that name
  looked up in the `PATH` entries a confined stage may not write (HOOK-4), as they stand when the
  hook fires: a directory a stage is given writes in later in the session leaves them from then on.
  A formatter that starts a tool from a project's `node_modules/.bin`, or from an activated
  virtualenv's `bin`, does not find it there. A hook that needs a tool from one of them names it by a
  path in its `run`, which runs whatever is at that path, as the entry above says.
- **`prompt-shown` holds the prompt back while it runs.** The hook runs before the question is
  drawn, up to the bound in HOOK-7, so a slow notifier delays the prompt it was meant to announce.
  Starting one in the background is not offered, for the reason a hook that does not wait is not.
  A hook that must return at once starts its own long work and exits.
- **`prompt-shown` covers questions the turn raises, and no others.** The trust question asked when
  an interface opens, and a question put to a person outside a turn, fire nothing: no turn is
  running to read the hooks. A confirmer that refuses without asking, such as a
  one-shot run's for everything but a plan, fires it for nothing it refuses. A channel that
  closed during the turn is found closed only when a question fails to send, so the question that
  finds it closed fires it once.
- **`checkout-made` holds its delegate back while it runs.** The delegate makes no request until its
  hook has ended, up to the bound in HOOK-7, so a setup that installs dependencies delays the work
  by as long as it takes, and one that needs longer than 30 seconds states a `timeout`. Stopping
  the turn or the delegate does not reach a hook that is running, which ends or reaches its bound.
  A call that gives several delegates a checkout runs the hooks at the same time, so they share the
  machine and a package manager's cache, and the status that names a running hook (HOOK-9) names
  the one that began last and clears when the first of them ends.
- **A hook stopped at its bound is stopped alone.** The program it started is killed and the
  programs that one started are not, so a setup script stopped in the middle of an install leaves
  the install running while the delegate starts.
- **`compacted` does not say who asked.** A compaction a person asked for and one the budget forced
  fire the same hooks with the same line, because HOOK-5 hands a hook the moment and nothing else. A
  hook that must tell them apart has no way to, and widening what a hook is told is a decision to
  take when a use case needs it. Two moment names would answer it without touching HOOK-5, at the
  cost of a person declaring a hook twice for the same reaction.
- **There is no moment before a compaction.** A hook runs after the summary has been adopted, so one
  that has to save something while the full history is still in the request has nowhere to attach.
  Firing before would be advisory only (HOOK-6) and would fire for a summary the gate then refuses
  ([COMPACT-1](compaction.md#COMPACT-1)) or that fails, which leaves the conversation as it was.
- **A stop key does not reach a `compacted` hook.** A summary that came back is kept however late the
  stop landed, and the hook then runs to its bound (HOOK-7), as `turn-finished` does after a turn
  that was cancelled. After `/compact` the screen is held by it, with its program named (HOOK-9),
  for up to that long.
- **A delegate's compaction fires nothing.** The delegate's conversation is its own, so a person
  whose delegates compact does not hear of it.
- **There is no moment for a session.** A session is many turns, and something that should happen
  once when the program opens fires per turn instead. The interactive interface is where a session
  begins and ends, and nothing there fires a hook yet.
