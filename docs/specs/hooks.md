---
id: HOOK
title: Hooks
status: normative
governs:
  - crates/config/src/hooks.rs
  - crates/agent/src/hooks.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/delegate.rs
documented-by: docs/website/docs/customize/hooks.md
---

## Scope

A command a person asked to have run when something happens: format after an edit, notify when a
turn is over. This file covers where one is declared, which moments exist, what a hook is handed,
what it may decide, and what happens when one goes wrong.

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
### HOOK-2: three moments, and each fires the hooks written for it

A declaration names one moment. These are the moments:

| Moment | Fires |
|---|---|
| `turn-started` | before the turn a person asked for sends anything |
| `tool-finished` | after one tool call has finished, whatever came of it |
| `turn-finished` | when the turn a person asked for is over, however it ended |

A `tool-finished` entry may name a `tool`, and then fires for calls of that name alone; one that
names none fires for every call. A tool named on either of the other two moments is about a call
that does not happen there, and that entry fires for nothing. Two entries on one moment are two
commands, run one after the other in the order the file listed them. A word this build does not
fire declares no hook and leaves the rest of the file in force.

The turn moments belong to the turn a person asked for. A delegate is a run inside that turn,
started by a call nobody typed, so a turn that spawns three delegates still fires `turn-started`
once. A call a delegate made is a call, and fires `tool-finished` like any other.

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
`verified-by: bravebot_agent::turn::a_delegate_does_not_fire_the_turn_s_own_moments`

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
person who wrote it meant. It is not confined, and its environment is the one it was started from
less this agent's own credentials, which are the same names taken off a program the planner asked
for.

**Why.** The person named this command in a file in their own directory, so it is their program on
their machine, and [sandboxing.md](sandboxing.md) confines processes running code we did not write.
Whatever is decided there about bounding a program a person asked for reaches a hook too, and by
the same argument: what a formatter or a notifier needs cannot be enumerated in advance.

The credentials come off for the reason they come off a run: a person reads the program and the
arguments they wrote down, so a secret travelling beside those is something they were never shown.
A hook has no more use for one than a formatter has.

`verified-by: bravebot_agent::hooks::a_hook_runs_the_program_the_file_named`
`verified-by: bravebot_agent::hooks::a_hook_runs_in_the_directory_the_turn_is_working_in`
`verified-by: bravebot_agent::hooks::a_hook_is_not_handed_this_agent_s_own_credentials`
`verified-by: bravebot_agent::hooks::two_hooks_on_one_moment_run_in_the_order_the_file_listed_them`

<a id="HOOK-5"></a>
### HOOK-5: a hook is told which moment fired, and nothing else

A hook is given one line on its standard input: a JSON object whose `event` is the moment's own
word. No path, no argument a model wrote, no bytes out of a file, and nothing added to its
environment. The tool a `tool-finished` entry names selects which entries fire and reaches no
process.

**Why.** There is then nothing to work out about what a hook is handed: the same three lines,
whatever the turn was doing. A path would be a string the model chose, and while a path a tool acts
on is trusted for deciding where that effect lands, handing one to a program on somebody's machine
is a wider claim than that. A hook that needs to know which tool ran declares one entry per tool.

`verified-by: bravebot_agent::hooks::a_hook_is_told_the_moment_and_nothing_else`
`verified-by: bravebot_agent::hooks::the_line_a_hook_is_told_is_built_from_the_moment_alone`

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
`verified-by: by-construction (a hook's standard output and standard error are discarded at the spawn and read by nothing; its status reaches one sentence said to the person and no branch)`

<a id="HOOK-7"></a>
### HOOK-7: a hook that goes wrong is said out loud, bounded, and ends nothing

A hook that could not be started, that ended badly, or that was still running after 30 seconds and
was stopped, is said to the person, naming the moment and the program. It reaches a live display as
it happens and the turn's own account of itself as well, so a run with nowhere to draw says it too.
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

A mistake in this file must not be a program that will not open. It is the user's own file, written
by hand, and the failure it most often has is a typo.

`verified-by: bravebot_agent::hooks::a_hook_that_ends_badly_is_reported`
`verified-by: bravebot_agent::hooks::a_hook_whose_program_is_not_there_is_reported`
`verified-by: bravebot_agent::hooks::a_hook_that_outstays_the_bound_is_stopped`
`verified-by: bravebot_agent::turn::a_turn_that_failed_still_says_what_its_hooks_said`
`verified-by: bravebot_cli::running::a_run_whose_turn_failed_still_says_what_its_hooks_said`
`verified-by: bravebot_ui_bridge::reporting::what_the_turn_said_is_kept_for_the_event_that_ends_it`
`verified-by: bravebot_agent::turn::a_hook_that_went_wrong_on_a_delegate_s_call_reaches_the_turn_s_notices`
`verified-by: bravebot_agent::turn::a_delegate_that_did_not_finish_still_tells_the_turn_what_its_hooks_said`
`verified-by: bravebot_agent::hooks::a_file_that_declared_nothing_fires_nothing`
`verified-by: bravebot_config::hooks::an_unusable_entry_leaves_the_others_firing`
`verified-by: bravebot_config::hooks::an_unparseable_file_declares_no_hooks`
`verified-by: bravebot_config::hooks::an_oversized_file_declares_no_hooks`

## The file

```json
{
  "hooks": [
    { "on": "tool-finished", "tool": "write_file", "run": ["cargo", "fmt"] },
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
  HOOK-7 per hook. A `tool-finished` hook on every call pays that on every call. Running one
  without waiting is not offered, because then a formatter and the next tool call would be reading
  and writing the same file at once.
- **There is no moment for a session.** A session is many turns, and something that should happen
  once when the program opens fires per turn instead. The interactive interface is where a session
  begins and ends, and nothing there fires a hook yet.
