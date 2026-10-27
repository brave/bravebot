---
id: OPEN
title: Open loops, the unfinished business a session leaves
status: proposed
governs:
  - crates/agent/src/turn.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/state.rs
  - crates/tui/src/app/turns.rs
documented-by:
  - none (gap: a user docs page, owed once this is built)
---

## Scope

bravebot finishes a turn and says nothing about what is left undone. Today a session can end with code changed and never built, a test run that failed and was never run again, todos still open, a job still running, a kept checkout never applied, and files not committed. Each of these is in bravebot's own records. Nobody puts them together.

This spec has bravebot list them. You see one line when the list changes. The planner is told at the start of the next turn. `/unfinished` shows the whole list.

Nothing is built yet. Each clause lists the tests to write and says `verified-by: none`.

This builds on GROUND (brave/bravebot#3412), which keeps the count of writes and commands and whether a command ran after the last write. It cannot be built before GROUND merges.

## Why it exists

- A turn that changed code and built nothing tells you so ([TURN-4](turns.md#TURN-4)), but only for that turn. Two turns later the change is still unbuilt and nothing says it.
- The planner forgets. A failed test run three turns ago, or a todo left open before compaction, is gone from what it reads.
- You come back to a session and have to reconstruct where it stood.

For each open loop, bravebot also says whose move it is.

## Clauses

<a id="OPEN-1"></a>
### OPEN-1: an open loop is one of seven kinds, found by fixed rules

An open loop is one of these, and nothing else:

| Kind | Open while |
|---|---|
| unbuilt changes | a file has been written since the last command ran, by the work record of [TURN-4](turns.md#TURN-4) with the order GROUND-1 adds |
| a failed command | the last command line that ran ended with a non-zero status, or was stopped |
| open todos | the task list has items that are not done and not cancelled ([TODO-2](tools/todo-write.md#TODO-2), [TODO-3](tools/todo-write.md#TODO-3)) |
| running jobs | a background job is still running ([RUN-15](tools/run.md#RUN-15)) |
| kept checkouts | a delegate's checkout was kept and has been neither applied nor removed ([CHECKOUT-15](checkouts.md#CHECKOUT-15)) |
| uncommitted files | in a git repository, a file this session wrote differs from `HEAD`, and `commit` is `off` ([COMMIT-1](commits.md#COMMIT-1)) |
| a stopped goal | a goal is paused, or still set after it stopped sending the work back ([GOAL-20](goal.md#GOAL-20)) |

Each rule reads only bravebot's own records: the work record, exit statuses, the task list, the job table, the checkout list, the goal, and git's status of the files the session wrote. No model decides whether a loop is open, and nothing a model says closes one. A loop closes when its rule stops holding: a command runs after the last write, a later command ends with status 0, the todo is marked done, the job ends, the checkout is applied or removed, the files are committed or put back, the goal is cleared or resumed.

**Why fixed rules.** A model asked "is anything unfinished?" can be talked out of the answer by the content it read. A rule over bravebot's own records cannot.

**Why these seven.** Each is something bravebot already records and can check cheaply at the end of a turn. Open review comments on a pull request and a failing CI run are left out, since both need a network request and their content is untrusted ([Out of scope](#out-of-scope)).

Tests to write:

- `bravebot_agent::open_loops::a_write_after_the_last_command_is_unbuilt_changes`
- `bravebot_agent::open_loops::a_command_that_exited_non_zero_is_a_failed_command_until_another_runs`
- `bravebot_agent::open_loops::a_cancelled_todo_is_not_open`
- `bravebot_agent::open_loops::a_kept_checkout_is_open_until_applied_or_removed`
- `bravebot_agent::open_loops::a_file_written_and_committed_is_not_uncommitted`
- `bravebot_agent::open_loops::with_commit_turn_no_file_is_uncommitted`
- `bravebot_agent::open_loops::a_paused_goal_is_a_stopped_goal`
- `bravebot_agent::open_loops::nothing_the_planner_says_closes_a_loop`

`verified-by: none`

<a id="OPEN-2"></a>
### OPEN-2: each loop is the planner's move, yours, or a wait

| Whose move | Kinds |
|---|---|
| the planner's | unbuilt changes, a failed command, open todos |
| yours | kept checkouts, uncommitted files, a stopped goal |
| a wait | running jobs |

The planner can build, rerun and finish its own todos. Applying a checkout, committing and restarting a goal are yours, since each changes your files, your history or what the session works towards. A running job needs nobody until it ends.

**Why say whose move.** A list of everything unfinished is noise. A list that says "this one is yours" is a to-do.

Tests to write:

- `bravebot_agent::open_loops::each_kind_has_one_owner`

`verified-by: none`

<a id="OPEN-3"></a>
### OPEN-3: you see one line when a turn ends with the list changed

When a turn ends and the set of open loops differs from the set at the end of the last turn, bravebot shows one line:

```
open: changes not built, the last command failed, 2 todos (planner); 1 kept checkout, 3 files not committed (you); 1 job running
```

The line names each kind with its count, grouped by whose move it is. It names no file, no command and no todo text. An empty list after a non-empty one shows `open: nothing`. A list the same as last time shows nothing, so a loop you have decided to leave open does not repeat every turn.

The line replaces the closing line of [TURN-4](turns.md#TURN-4) ("files changed this turn and no command was run..."). Unbuilt changes is the same fact, kept across turns.

**Why only on change.** A reminder repeated every turn stops being read. A loop you chose to leave open stays in `/unfinished` and does not nag.

Tests to write:

- `bravebot_agent::turn::a_turn_that_leaves_loops_open_ends_with_one_line_naming_them`
- `bravebot_agent::turn::the_same_open_loops_are_not_said_twice`
- `bravebot_agent::turn::closing_the_last_loop_says_nothing_is_open`
- `bravebot_agent::turn::the_line_names_no_path_command_or_todo_text`

`verified-by: none`

<a id="OPEN-4"></a>
### OPEN-4: the planner is told at the start of the next turn

When the set of open loops has changed since the planner was last told, the next turn begins with one message from bravebot:

> Open from earlier in this session: changes not built since the last write; the last command failed; 2 todos are open. Yours to act on if they still matter. Also open, for the person: 1 kept checkout, 3 files not committed. 1 job is still running.

It is fixed text with counts and the job names bravebot minted ([RUN-15](tools/run.md#RUN-15)). It holds no path, no command and no todo text. It asks nothing and requires nothing: the planner may decide a loop no longer matters.

**Why at the start, not the end.** At the end of a turn the planner has stopped. At the start of the next one it is deciding what to do, and the message survives compaction because it is sent fresh.

**Why the planner's loops only as work.** The person's loops are named so the planner does not try to commit or apply a checkout on its own. Telling it about them as its work would invite exactly that.

Tests to write:

- `bravebot_agent::turn::the_next_turn_begins_with_the_open_loops_that_changed`
- `bravebot_agent::turn::loops_the_planner_was_already_told_about_are_not_repeated`
- `bravebot_agent::turn::the_message_to_the_planner_holds_no_path_command_or_todo_text`

`verified-by: none`

<a id="OPEN-5"></a>
### OPEN-5: `/unfinished` shows every open loop in full

`/unfinished` lists each open loop with what you need to act on it:

- unbuilt changes: the files written since the last command
- a failed command: the line and its exit status
- open todos: each todo's text
- running jobs: each job's name and how long it has run
- kept checkouts: each one's number and path, and `/checkouts` to apply or remove it
- uncommitted files: each file
- a stopped goal: its condition and why it stopped

With nothing open it says `nothing is open`. It starts no turn and sends nothing to the planner.

**Why `/unfinished`.** `/open` reads like opening a file, and `/loops` is one letter from `/loop`.

**Why paths here and not in the line.** You asked for the details, and they are shown to you only. The line and the planner's message stay counts.

Tests to write:

- `bravebot_tui::state::unfinished_lists_each_open_loop_with_what_closes_it`
- `bravebot_tui::state::unfinished_with_nothing_open_says_so`
- `bravebot_tui::state::unfinished_starts_no_turn`

`verified-by: none`

<a id="OPEN-6"></a>
### OPEN-6: a resumed session shows what it left open

The session record ([SESSION](sessions.md)) keeps the work record and the last command's status. On resume, bravebot works out the open loops again from the record and from the files on disk, shows the line from [OPEN-3](#OPEN-3), and tells the planner on the first turn as in [OPEN-4](#OPEN-4).

What a resume can know:

- unbuilt changes and a failed command, from the record
- kept checkouts, from the record ([CHECKOUT-15](checkouts.md#CHECKOUT-15))
- uncommitted files, from git
- open todos, running jobs and a stopped goal are not kept across a resume today, so they are not shown

A record written before this spec reads as having no work record and no failed command.

Tests to write:

- `bravebot_session::sessions::the_record_keeps_the_work_record_and_the_last_commands_status`
- `bravebot_session::sessions::a_record_from_before_open_loops_reads_as_nothing_open`
- `bravebot_tui::state::a_resumed_session_shows_what_it_left_open`

`verified-by: none`

<a id="OPEN-7"></a>
### OPEN-7: open loops never block, and every new sentence is fixed text

No open loop stops a turn, holds a prompt, or keeps the session working. Ending with loops open is allowed: often the right answer is to leave them. Keeping the work going until a condition holds is `/goal`'s job ([GOAL-1](goal.md#GOAL-1)), not this one's.

The line and the planner's message are fixed English with counts and job names, like the other sentences bravebot writes to the planner. The person's line comes from the catalog and is translated ([LOCALE](localization.md)). `/unfinished` shows paths, command lines and todo text to you only, released for display and never to the planner.

**Why never block.** A reminder that refuses to let you stop becomes the thing you fight. The point is that you know, not that you comply.

Tests to write:

- `bravebot_agent::turn::a_turn_with_loops_open_ends_as_it_would_have`
- `bravebot_agent::open_loops::the_planners_message_is_fixed_text_and_counts`

`verified-by: none`

## Changes to other specs

- [turns.md](turns.md): [TURN-4](turns.md#TURN-4)'s line to the person is replaced by [OPEN-3](#OPEN-3)'s. Its question to the planner during the turn is unchanged.
- [sessions.md](sessions.md): the record gains the work record and the last command's status ([OPEN-6](#OPEN-6)).
- [commands.md](commands.md): `/unfinished` is added ([OPEN-5](#OPEN-5)).
- [localization.md](localization.md): the catalogs gain the person's line and `/unfinished`'s text.

## Out of scope

- Open review comments on a pull request, and failing CI. Both need a network request and an authenticated client, and what comes back is untrusted. A later spec can add them through a connector.
- Acting on a loop. bravebot names loops and never builds, commits or applies anything because one is open.
- Model-found loops, such as "the reply promised a follow-up". Only the fixed rules of [OPEN-1](#OPEN-1) count.

## Known costs

- **Uncommitted files reads git status at the end of each turn**, for the files the session wrote. In a large repository with many written files this is a measurable cost per turn.
- **A failed command can be a command that was meant to fail**, such as a test written to fail first. It shows as open until a later command succeeds.
- **Todo state is the planner's word.** A todo marked done is taken as done. bravebot does not check the work behind it.
