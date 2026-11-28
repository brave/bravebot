---
id: OPEN
title: Open loops, the unfinished business a session leaves
status: normative
governs:
  - crates/agent/src/open_loops.rs
  - crates/agent/src/outcome.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/workspace/checkouts.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/state/open_loops.rs
  - crates/agent/src/goal/standing.rs
  - crates/tui/src/goals.rs
  - crates/tui/src/app/turns.rs
  - crates/ui-bridge/src/open_loops.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/ui-bridge/src/running.rs
  - crates/ui-bridge/src/turn.rs
  - ui/src/renderer/apply.ts
  - ui/src/renderer/transcript.ts
  - ui/src/renderer/components/Transcript.tsx
  - ui/src/shared/protocol.ts
  - ui/src/renderer/live.ts
  - ui/src/renderer/App.tsx
  - ui/scripts/drive-open-loops.mjs
  - ui/scripts/drive-open-loops-turn-end.mjs
documented-by:
  - docs/website/docs/reference/commands.md
---

## Scope

bravebot finishes a turn and says nothing about what is left undone. A session can end with code changed and never built, a test run that failed and was never run again, todos still open, a job still running, a kept checkout never applied, and files not committed. Each of these is in bravebot's own records. Nobody puts them together.

This spec has bravebot list them. You see one line when the list changes. The planner is told at the start of the next turn. `/unfinished` shows the whole list.

It builds on [GROUND-1](grounding.md#GROUND-1), which counts writes and commands and records whether a command ran after the last write.

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
| unbuilt changes | a file has been written since the last command ran, by the work record ([GROUND-1](grounding.md#GROUND-1)) |
| a failed command | the last command line that ran ended with a non-zero status, or was stopped |
| open todos | the task list has items that are not done and not cancelled ([TODO-2](tools/todo-write.md#TODO-2), [TODO-3](tools/todo-write.md#TODO-3)) |
| running jobs | a background job is still running ([RUN-15](tools/run.md#RUN-15)) |
| kept checkouts | a delegate's checkout was kept and has been neither applied nor removed ([CHECKOUT-15](checkouts.md#CHECKOUT-15)). Applied means its files came back, by `/checkouts apply` or the planner's `apply_checkout` ([CHECKOUT-14](checkouts.md#CHECKOUT-14)), and the record keeps it |
| uncommitted files | in a git repository, a file this session wrote differs from `HEAD`, and `commit` is `off` ([COMMIT-1](commits.md#COMMIT-1)) |
| a stopped goal | a goal is paused, or still set after it stopped sending the work back ([GOAL-20](goal.md#GOAL-20)) |

Each rule reads only bravebot's own records: the work record, exit statuses, the task list, the job table, the checkout list, the goal, and git's status of the files the session wrote. No model decides whether a loop is open, and nothing a model says closes one. A loop closes when its rule stops holding: a command runs after the last write, a later command ends with status 0, the todo is marked done, the job ends, the checkout is applied or removed, the files are committed or put back, the goal is cleared or resumed.

A write into the session's own directory is not a written file for this rule. What a turn puts there is for its own use, outside the project and gone when the session ends, so it leaves nothing to build (a rewind does not restore it either, and [sessions.md](sessions.md) records it as a coverage gap for the same reason). The work record does not count the write, so the loop does not open for it, the planner is not told of it ([OPEN-4](#OPEN-4)), and `/unfinished` does not list the file under unbuilt changes ([OPEN-5](#OPEN-5)). The test is the resolved destination of the write, never the name typed and never what the file holds, so a link in the project that lands in that directory is not counted. Applying a checkout writes into the source tree and still counts. A session with no such directory counts every write.

**Why fixed rules.** A model asked "is anything unfinished?" can be talked out of the answer by the content it read. A rule over bravebot's own records cannot.

**Why these seven.** Each is something bravebot already records and can check cheaply at the end of a turn. Open review comments on a pull request and a failing CI run are left out, since both need a network request and their content is untrusted ([Out of scope](#out-of-scope)).

`verified-by: bravebot_agent::open_loops::a_write_after_the_last_command_is_unbuilt_changes`
`verified-by: bravebot_agent::turn::a_change_made_by_a_run_line_is_an_unbuilt_change_until_a_command_runs_after_it`
`verified-by: bravebot_agent::tools::tests::loading_skills::a_write_into_the_session_directory_is_not_a_changed_file`
`verified-by: bravebot_agent::turn::a_turn_that_only_writes_in_the_session_directory_leaves_nothing_unbuilt`
`verified-by: bravebot_agent::turn::an_edit_a_download_and_a_picture_landing_in_the_session_directory_count_no_write`
`verified-by: bravebot_tui::state::open_loops::unfinished_does_not_list_a_file_in_the_session_directory_as_unbuilt`
`verified-by: bravebot_agent::open_loops::a_command_that_exited_non_zero_is_a_failed_command_until_another_runs`
`verified-by: bravebot_agent::turn::a_turns_record_says_whether_its_last_command_failed`
`verified-by: bravebot_agent::workspace::a_kept_checkout_is_open_until_applied_or_removed`
`verified-by: bravebot_agent::workspace::open_loops_keep_only_checkouts_that_ended_and_were_not_applied`
`verified-by: bravebot_agent::turn::a_typed_checkouts_apply_asks_about_each_recorded_file`
`verified-by: bravebot_agent::turn::a_file_a_program_wrote_in_a_checkout_comes_back_by_its_status`
`verified-by: bravebot_agent::open_loops::a_file_written_and_committed_is_not_uncommitted`
`verified-by: bravebot_agent::open_loops::a_name_that_is_not_utf8_is_told_apart_from_its_lossy_lookalike`
`verified-by: bravebot_tui::goals::rounds_with_no_tool_call_in_a_row_stop_the_goal_without_clearing_it`
`verified-by: bravebot_tui::state::open_loops::unfinished_lists_each_open_loop_with_what_closes_it`
`verified-by: bravebot_ui_bridge::open_loops_turn_end::undoing_a_turn_that_wrote_takes_its_write_out_of_the_open_loops`

<a id="OPEN-2"></a>
### OPEN-2: each loop is the planner's move, yours, or a wait

| Whose move | Kinds |
|---|---|
| the planner's | unbuilt changes, a failed command, open todos |
| yours | kept checkouts, uncommitted files, a stopped goal |
| a wait | running jobs |

The planner can build, rerun and finish its own todos. Applying a checkout, committing and restarting a goal are yours, since each changes your files, your history or what the session works towards. A running job needs nobody until it ends.

**Why say whose move.** A list of everything unfinished is noise. A list that says "this one is yours" is a to-do.

`verified-by: bravebot_agent::open_loops::each_kind_has_one_owner`

<a id="OPEN-3"></a>
### OPEN-3: you see one line when a turn ends with the list changed

When a turn ends and the set of open loops differs from the set at the end of the last turn, bravebot shows one line:

```
open: changes not built, the last command failed, 2 todos (planner); 1 kept checkout, 3 files not committed (you); 1 job running
```

The line names each kind with its count, grouped by whose move it is. It names no file, no command and no todo text. An empty list after a non-empty one shows `open: nothing`. A list the same as last time shows nothing, so a loop you have decided to leave open does not repeat every turn.

The line replaces the closing line of [TURN-4](turns.md#TURN-4) ("files changed this turn and no command was run..."). Unbuilt changes is the same fact, kept across turns.

**Why only on change.** A reminder repeated every turn stops being read. A loop you chose to leave open stays in `/unfinished` and does not nag.

`verified-by: bravebot_tui::state::open_loops::a_turn_that_leaves_loops_open_ends_with_one_line_naming_them`
`verified-by: bravebot_agent::turn::a_caller_that_shows_open_loops_is_not_also_told_of_unbuilt_changes`
`verified-by: bravebot_agent::open_loops::the_persons_line_groups_kinds_by_owner_and_names_no_path`
`verified-by: bravebot_agent::open_loops::the_persons_line_with_nothing_open_is_open_nothing`
`verified-by: bravebot_ui_bridge::open_loops_turn_end::a_desktop_turn_ends_with_the_open_loops_when_they_changed`
`verified-by: bravebot_ui_bridge::open_loops_turn_end::a_cancelled_turn_carries_the_open_loops_on_its_error`

<a id="OPEN-4"></a>
### OPEN-4: the planner is told at the start of the next turn

When the set of open loops has changed since the planner was last told, the next turn begins with one message from bravebot:

> Open from earlier in this session: changes not built since the last write; the last command failed; 2 todos are open. Yours to act on if they still matter. Also open, for the person: 1 kept checkout, 3 files not committed. 1 job is still running.

It is fixed text with counts and the job names bravebot minted ([RUN-15](tools/run.md#RUN-15)). It holds no path, no command and no todo text. It asks nothing and requires nothing: the planner may decide a loop no longer matters.

The running jobs it names are the ones still running when the message is sent, in the turn's first round after the finishes of jobs that ended between turns are told ([CMDLINE-14](tools/command-line.md#CMDLINE-14)). A job whose finish is told in that round is not named as running, and where it was the only open loop no message is sent.

The record of what the planner was told keeps the set the last turn ended with, so where a job ended between turns the next turn after can repeat the message about the other loops once.

In the desktop app, undoing a turn tells the planner again: the next turn begins with the loops the session now leaves open, whether or not they changed, because the rewound conversation may no longer hold the message it was last told. Undoing the only turn leaves nothing open, so nothing is told.

**Why at the start, not the end.** At the end of a turn the planner has stopped. At the start of the next one it is deciding what to do, and the message survives compaction because it is sent fresh.

**Why the planner's loops only as work.** The person's loops are named so the planner does not try to commit or apply a checkout on its own. Telling it about them as its work would invite exactly that.

`verified-by: bravebot_agent::turn::the_next_turn_begins_with_the_open_loops_that_changed`
`verified-by: bravebot_agent::turn::a_job_whose_finish_is_told_is_not_named_as_running_in_the_open_loops`
`verified-by: bravebot_tui::kept_jobs_tests::a_job_that_exited_between_turns_is_not_named_as_running_to_the_planner`
`verified-by: bravebot_tui::state::open_loops::loops_the_planner_was_already_told_about_are_not_repeated`
`verified-by: bravebot_agent::open_loops::the_planners_message_is_fixed_text_and_counts`
`verified-by: bravebot_ui_bridge::open_loops_tests::a_desktop_turn_tells_the_planner_of_open_loops_once_per_changed_set`
`verified-by: bravebot_agent::turn::telling_open_loops_reaches_the_planner_and_keeps_the_unbuilt_line`
`verified-by: bravebot_ui_bridge::open_loops_tests::undoing_a_turn_tells_the_planner_of_the_loops_left_open`
`verified-by: bravebot_ui_bridge::open_loops_tests::undoing_the_only_turn_leaves_nothing_open`
`verified-by: bravebot_ui_bridge::open_loops::the_first_turn_after_a_reopen_tells_the_planner_what_was_left_open`

<a id="OPEN-5"></a>
### OPEN-5: `/unfinished` shows every open loop in full

`/unfinished` lists each open loop with what you need to act on it:

- unbuilt changes: the files written since the last command, leaving out those that landed in the
  session's own directory ([OPEN-1](#OPEN-1))
- a failed command: the line and its exit status
- open todos: each todo's text
- running jobs: each job's name and how long it has run
- kept checkouts: each one's number and path, and `/checkouts` to apply or remove it
- uncommitted files: each file
- a stopped goal: its condition and why it stopped

With nothing open it says `nothing is open`. It starts no turn and sends nothing to the planner.

**Why `/unfinished`.** `/open` reads like opening a file, and `/loops` is one letter from `/loop`.

**Why paths here and not in the line.** You asked for the details, and they are shown to you only. The line and the planner's message stay counts.

`verified-by: bravebot_tui::state::open_loops::unfinished_lists_each_open_loop_with_what_closes_it`
`verified-by: bravebot_tui::state::open_loops::unfinished_does_not_list_a_file_in_the_session_directory_as_unbuilt`

<a id="OPEN-6"></a>
### OPEN-6: a resumed session shows what it left open

The session record ([SESSION](sessions.md)) keeps the work record, the last command's status and the files the session wrote. On resume, bravebot works out the open loops again from the record and from the files on disk, shows the line from [OPEN-3](#OPEN-3), and tells the planner on the first turn as in [OPEN-4](#OPEN-4).

What a resume can know:

- unbuilt changes and a failed command, from the record
- open todos, from the task lists the record keeps
- kept checkouts and whether each was applied, from the record ([CHECKOUT-15](checkouts.md#CHECKOUT-15))
- uncommitted files, from git, for the files the record says the session wrote
- running jobs and a stopped goal end with the session's process, so they are not shown

A record with no work record or last-command status reads as nothing open.

The desktop window shows the same counts as one sentence in a notice at the top of the chat, which stays in view while the chat opens at its end: `Left open from earlier: changes not built, the last command failed; 1 kept checkout, for you to act on.` The planner's loops come first with no owner named, then the person's. The notice goes when the next turn starts or a rewind undoes turns, since either changes what it described.

Undoing a turn takes its writes and commands out of the record, so a resume afterwards reports nothing that turn left open.

`verified-by: bravebot_session::sessions::the_record_keeps_the_work_record_and_the_last_commands_status`
`verified-by: bravebot_session::sessions::a_record_from_before_open_loops_reads_as_nothing_open`
`verified-by: bravebot_tui::state::open_loops::a_resumed_session_shows_what_it_left_open`
`verified-by: bravebot_ui_bridge::open_loops_tests::a_resumed_session_tells_the_planner_on_its_first_turn_only`
`verified-by: bravebot_ui_bridge::open_loops_tests::the_record_keeps_the_sessions_work_and_written_files`
`verified-by: bravebot_ui_bridge::open_loops_turn_end::a_desktop_turn_that_writes_a_file_saves_the_work_and_the_file`
`verified-by: bravebot_ui_bridge::running::a_resumed_session_carries_the_work_and_the_files_its_record_kept`
`verified-by: bravebot_ui_bridge::open_loops::a_desktop_turn_saves_its_work_and_the_files_it_wrote`
`verified-by: bravebot_ui_bridge::open_loops::a_reopened_session_replies_with_what_it_left_open`
`verified-by: bravebot_ui_bridge::open_loops::a_reopened_session_that_left_nothing_open_replies_with_no_line`
`verified-by: bravebot_ui_bridge::open_loops::a_rewound_turn_leaves_nothing_open_on_reopen`
`verified-by: bravebot_ui_bridge::checkouts::a_reopened_session_counts_the_kept_checkout_it_did_not_apply`
`verified-by: bravebot_ui_bridge::written_tests::a_delegates_write_is_not_kept_as_one_the_session_wrote`
`verified-by: bravebot_agent::open_loops::the_sentence_a_resume_shows_names_the_same_counts`
`verified-by: bravebot_ui_bridge::open_loops::a_record_with_failing_work_and_open_todos_reads_as_those_loops`
`verified-by: bravebot_ui_bridge::open_loops::a_record_from_before_open_loops_reads_as_nothing_open`
`verified-by: bravebot_ui_bridge::open_loops::a_written_file_that_differs_from_head_counts_as_uncommitted`
`verified-by: bravebot_ui_bridge::open_loops::the_first_turn_after_a_reopen_tells_the_planner_what_was_left_open`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/drive-open-loops.mjs takes turns through the real bridge, reopens each chat in a real window and holds that the open-loops session notice shows the bridge's sentence and stays in view at the end of a chat longer than the window, that with every kind a resume knows open it names each one with the icon on the first line at the minimum window size, and that a reopened chat with nothing open and a new chat show no such notice)`

<a id="OPEN-7"></a>
### OPEN-7: open loops never block, and every new sentence is fixed text

No open loop stops a turn, holds a prompt, or keeps the session working. Ending with loops open is allowed: often the right answer is to leave them. Keeping the work going until a condition holds is `/goal`'s job ([GOAL-1](goal.md#GOAL-1)), not this one's.

The line and the planner's message are fixed English with counts and job names, like the other sentences bravebot writes to the planner. The person's line comes from the catalog and is translated ([LOCALE](localization.md)). `/unfinished` shows paths, command lines and todo text to you only, released for display and never to the planner.

**Why never block.** A reminder that refuses to let you stop becomes the thing you fight. The point is that you know, not that you comply.

`verified-by: bravebot_agent::turn::a_turns_outcome_carries_the_summary_of_its_work`
`verified-by: bravebot_agent::open_loops::the_planners_message_is_fixed_text_and_counts`

## Changes to other specs

- [turns.md](turns.md): [TURN-4](turns.md#TURN-4)'s line to the person is replaced by [OPEN-3](#OPEN-3)'s. Its question to the planner during the turn is unchanged.
- [sessions.md](sessions.md): the record gains the work record, the last command's status and the files the session wrote, and each kept checkout whether it was applied ([OPEN-6](#OPEN-6)).
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
- **`commit` is not built yet** ([COMMIT-1](commits.md#COMMIT-1) is proposed), so every git repository counts as `commit = off` until it is.
- **A write through a link that lands in the session's own directory is not counted.** The test is
  where the write landed ([OPEN-1](#OPEN-1)), so such a write is hidden from the unbuilt-changes
  warning, whatever the name that was typed. The person can still see it in the files state
  ([FILES-1](files-mode.md#FILES-1)).
- **A repository whose `.git` is a file**, a worktree git made, counts no uncommitted files, since bravebot reads only a `.git` directory without starting git.
- **A file whose name is not UTF-8 is never counted as uncommitted**, since bravebot's reader of git's status lists only names that are text. It is never taken for another file whose name reads the same.
