---
id: GROUND
title: Telling the planner and the goal judge what actually ran
status: normative
governs:
  - crates/agent/src/outcome.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/turn/result.rs
  - crates/agent/src/goal.rs
  - crates/tui/src/goals.rs
  - crates/tui/src/state.rs
  - crates/tui/src/app/turns.rs
  - crates/tui/src/app/side_requests.rs
  - crates/cli/src/goal_run.rs
documented-by:
  - docs/website/docs/customize/agents.md
  - docs/website/docs/reference/commands.md
---

## Scope

Two model claims that nothing else checks:

- a delegate's report to the planner ([delegation.md](delegation.md))
- a goal judge's verdict ([goal.md](goal.md))

bravebot already knows what commands ran and what files were written. This spec has it count them and show the counts next to each claim. It never blocks anything and never changes the claim.

## Why it exists

- A delegate can say "tests pass" without running anything, and the planner then repeats it to you as fact.
- A judge can rule a goal met because the planner said the tests pass, after compaction removed the run that would have shown otherwise.

## Clauses

<a id="GROUND-1"></a>
### GROUND-1: count commands and writes, and whether a command came after the last write

Each run's work record ([TURN-4](turns.md#TURN-4)) also keeps:

- **writes:** calls that changed a file, including a `run` line that left a file in the workspace different from how it found it, other than a write that landed in the session's own directory
- **commands:** calls that ran a program
- whether the last command came after the last write

They are counted by the rules [TURN-4](turns.md#TURN-4) already uses. A write the person declined or a mode refused changed nothing and does not count. A command stopped with the turn does count. One command line is one command, however many programs it starts. Each write call is one write, even when two calls touch the same file. A call that both ran a program and changed a file counts as the write coming last, since nothing ran after what it wrote.

Whether a `run` line changed a file is the driver's own measurement. The workspace tree is listed before the line starts and again after it has stopped and any destination a scan refused has been put back (one the line created is removed, so it counts for nothing; one that existed is written back with its old bytes and a new modification time, so it counts), and the line changed a file if a path was added or removed or a file's length or modification time differs. Names, lengths and times are compared and no file is opened, so the driver reads only metadata and the result is one fact, a yes or a no. A program chooses the names, lengths and times it leaves, so it can make that fact true or false by what it writes, as it can with its exit status. The directories every walk of the tree steps over, such as `target`, `node_modules`, `.git` and `__pycache__`, are not listed, so an ordinary build that writes only there is a command and not a write, and "a command ran after the last write" stays true after it. The session stores are not listed either, so a workspace rooted at the home directory does not count the session records a running session saves as the line's write. The line need not have ended well: one that failed halfway has still left what it got to. A line the person declined or a mode refused ran nothing and counts neither. A line that was moved to the background or started there is recorded as a command and no write, because the listing after the line is taken only for a line that stopped in the foreground: nothing it writes, before or after the move, is counted.

A write that landed in the session's own directory is not counted. What a turn puts there is for its own use, outside the project and gone when the session ends, so it leaves nothing to build, and a rewind does not restore it ([sessions.md](sessions.md) records it as a coverage gap). It does not change the order either: a project write followed by a write there is still a write with no command after it, and a command after a project write still follows it. For a write call, whether it landed there is decided on the resolved destination, never on the name typed or on what the file holds. The listing of a `run` line covers the workspace tree only, so what a line writes in that directory is not listed while the directory is outside the tree. Delegates and goals add records that already exclude it, so the sentences of [GROUND-2](#GROUND-2) to [GROUND-4](#GROUND-4) leave it out too.

Records are added in the order the work happened. The later record's counts are added to the earlier one's. A later record with a write settles the order by its own flag. A later record with commands and no write means a command ran after the last write. A record with neither leaves the order as it was. The same rule adds a delegate's record to the turn that takes it back, and a turn's record to a goal's.

A delegate in the turn's workspace adds its whole record. One in a checkout given the turn's changes adds its commands and none of its writes, as [TURN-4](turns.md#TURN-4) counts it ([CHECKOUT-23](checkouts.md#CHECKOUT-23)).

**Why an order and not numbers.** A goal spans turns and a delegate runs alongside its parent, so call numbers would have to be kept across both. Only whether a command came after the last write matters, and that combines record by record.

**Why count calls.** The record is already kept per call, and each call is something you approved. Two writes to one file are two approvals.

`verified-by: bravebot_agent::outcome::each_write_that_changed_a_file_is_counted`
`verified-by: bravebot_agent::tools::a_write_into_the_session_directory_is_not_a_changed_file`
`verified-by: bravebot_agent::turn::a_turn_that_only_writes_in_the_session_directory_leaves_nothing_unbuilt`
`verified-by: bravebot_agent::turn::an_edit_a_download_and_a_picture_landing_in_the_session_directory_count_no_write`
`verified-by: bravebot_agent::outcome::a_command_after_the_last_write_is_recorded_as_after_it`
`verified-by: bravebot_agent::outcome::a_command_in_a_later_turn_is_after_a_write_in_an_earlier_one`
`verified-by: bravebot_agent::turn::a_delegates_counts_are_added_to_the_turns_where_it_worked_on_the_turns_files`
`verified-by: bravebot_agent::turn::a_file_a_worker_delegate_changed_through_a_run_line_is_a_write_of_the_turn`
`verified-by: bravebot_agent::turn::a_file_a_delegate_in_a_checkout_changed_through_a_run_line_is_not_a_write_of_the_turn`
`verified-by: bravebot_agent::turn::a_turns_outcome_carries_the_summary_of_its_work`
`verified-by: bravebot_agent::turn::a_run_line_that_changes_a_file_is_recorded_as_a_write`
`verified-by: bravebot_agent::turn::a_run_line_that_writes_only_build_output_is_not_a_write`
`verified-by: bravebot_agent::turn::a_run_line_that_changes_nothing_is_not_a_write`
`verified-by: bravebot_agent::turn::a_run_line_that_writes_and_then_exits_nonzero_is_recorded_as_a_write`
`verified-by: bravebot_agent::turn::a_run_line_that_writes_and_then_cannot_open_a_redirection_is_recorded_as_a_write`
`verified-by: bravebot_agent::turn::a_declined_run_line_is_neither_a_write_nor_a_command`
`verified-by: bravebot_agent::turn::a_change_made_by_a_run_line_is_an_unbuilt_change_until_a_command_runs_after_it`
`verified-by: bravebot_agent::workspace::tree_mark::a_new_a_removed_and_a_rewritten_file_each_differ`
`verified-by: bravebot_agent::workspace::tree_mark::what_lands_in_a_directory_every_walk_skips_does_not_differ`
`verified-by: bravebot_agent::workspace::tree_mark::what_lands_in_the_stores_under_a_workspace_holding_home_does_not_differ`
`verified-by: bravebot_agent::workspace::tree_mark::a_tree_past_the_bound_reports_no_change`
`verified-by: bravebot_agent::workspace::tree_mark::a_link_to_a_directory_is_one_entry_and_is_not_followed`
`verified-by: bravebot_agent::workspace::tree_mark::an_unreadable_directory_marks_nothing_and_the_rest_of_the_tree_is_still_marked`

<a id="GROUND-2"></a>
### GROUND-2: tell the planner what each delegate actually did

When a delegate is collected, the planner's message about it gains one sentence: "It ran 2 commands and made 1 write." It ends "in its own checkout" where the delegate worked in one. Zero is said as zero.

The sentence sits beside the report, not inside it: after any sentence about a limit or a budget, and before the report. So the planner reads it when the report is a quarantined reference too.

It is said however the delegate ended: answered, stopped or failed. If the delegate's thread died, nothing came back and nothing is said.

**Why.** It answers "I ran the tests and they pass" from a delegate that ran nothing. The planner is about to build on the report, and to repeat it to you.

`verified-by: bravebot_agent::turn::a_delegates_report_is_put_beside_how_many_commands_it_ran_and_writes_it_made`
`verified-by: bravebot_agent::turn::a_delegate_that_ran_nothing_is_said_to_have_run_no_command`
`verified-by: bravebot_agent::turn::a_quarantined_report_carries_the_same_sentence_about_the_work`
`verified-by: bravebot_agent::turn::a_delegate_that_failed_is_still_said_to_have_done_what_it_did`
`verified-by: bravebot_agent::turn::delegate_end::a_delegate_in_a_checkout_is_said_to_have_worked_there`

<a id="GROUND-3"></a>
### GROUND-3: show the person the same counts, without paths

The note you see when a delegate finishes gains the same two counts: "it ran 2 commands and made 1 write". It does not list the files written.

**Why no paths.** You already saw each write when it was approved or shown in the transcript. A kept checkout's paths are already listed next to the report ([CHECKOUT-18](checkouts.md#CHECKOUT-18)). The note stays one line.

`verified-by: bravebot_agent::turn::the_note_for_a_finished_delegate_counts_its_commands_and_writes`

<a id="GROUND-4"></a>
### GROUND-4: give the goal judge the record since the goal was set

The judge's request gains one paragraph before the condition. It says how many commands ran and how many writes were made since the goal was set, and whether a command ran after the last write, or that no file was written. It says this is bravebot's own count and not something the agent said, and that it says nothing about what any command did or whether it succeeded.

A turn hands back its record however it ended, beside its outcome and decisions. The goal adds up the records of the turns since it was set and gives the total to the judge's request. The same holds for a one-shot run's goal.

- A turn already running when the goal was set is not counted. The record starts with the next turn.
- Turns while the goal is paused are counted, since the work is done whether or not it is judged.
- A stopped or failed turn is counted. Its writes are on disk all the same.
- A delegate's work counts where [GROUND-1](#GROUND-1) adds it.
- Lines you run yourself at the `!` prompt are not turns and are not counted.
- A new goal starts a new record. The record is dropped with its goal.

**Why.** The judge only rules a condition met when the conversation shows it ([GOAL-3](goal.md#GOAL-3)). Compaction can shrink the run that showed it to a sentence, or remove it. The count survives compaction. If the judge reads "No command ran after the last write" while the planner says the tests pass, it can answer `NOT MET`.

`verified-by: bravebot_agent::goal::the_judge_is_given_the_commands_and_writes_since_the_goal_was_set`
`verified-by: bravebot_agent::goal::the_judge_is_told_whether_a_command_ran_after_the_last_write`
`verified-by: bravebot_agent::turn::a_turns_outcome_carries_the_summary_of_its_work`
`verified-by: bravebot_tui::goals::the_record_adds_every_turn_the_goal_drives`
`verified-by: bravebot_tui::goals::a_turn_running_when_the_goal_was_set_is_not_counted`
`verified-by: bravebot_tui::goals::a_turn_while_the_goal_is_paused_is_counted`
`verified-by: bravebot_tui::state::a_new_goal_starts_a_new_record`
`verified-by: bravebot_cli::running::a_goal_runs_judge_is_given_the_record_of_what_ran`

<a id="GROUND-5"></a>
### GROUND-5: the judge still decides

The verdict alone decides the goal. A `MET` ends it even if the record shows no commands. bravebot does not compare the record with the verdict, the condition or the reason.

**Why.** Overriding the judge would mean bravebot deciding what the condition means. Many conditions need no command at all. The record is evidence for the judge, not a rule.

`verified-by: bravebot_agent::goal::a_met_verdict_ends_the_goal_whatever_the_record_says`

<a id="GROUND-6"></a>
### GROUND-6: every new sentence is fixed text plus numbers

Every sentence this spec adds is fixed English with counts filled in, like the other sentences about a delegate and like bravebot's other messages to models. None holds any text from a report, an error, a file, a path or a command.

**Why.** If a sentence carried text from what a delegate or a turn did, it could use that to talk to the planner or the judge.

No new row is needed in [LABEL-8](labels.md#LABEL-8). The counts are bravebot's record of its own actions, which [TURN-4](turns.md#TURN-4) already sends to the planner.

`verified-by: bravebot_agent::turn::the_note_for_a_finished_delegate_counts_its_commands_and_writes`
`verified-by: bravebot_agent::goal::the_record_put_to_the_judge_holds_no_path_and_no_command`

## Out of scope

- Checking a processor's remark or a check's reason against the content. You already see the real diff, and the work would touch every interface for little gain.
- Compaction summaries and recalled facts.
- Whether a command succeeded. bravebot has the exit status, but what it means depends on the program.
- Understanding claims. Nothing parses "tests pass" in any language.
- Blocking or approving anything based on a count ([GROUND-5](#GROUND-5)).

## Known costs

- **A run line's writes are seen only inside the workspace tree, and only for a line that ends in the foreground.** A file written outside the workspace root or in a directory added by name is not counted, and neither is anything written by a line that moved to the background or started there, which is recorded as a command and no write. A file rewritten to the same length within the file system's time resolution is not seen. A tree of more than 100,000 files is not compared, and a line run there counts no write. The directories to skip are chosen by name, inside a git repository and outside one, and no `.gitignore` is read.
- **A line that writes into a skipped directory is a command and not a write.** A source file kept under `build`, `dist` or `vendor` is not seen.
- **A file changed by something else while a line runs is counted as that line's write.** Another worker delegate in the same workspace, a background job started earlier, an editor, another session or the person changing a file during the line is indistinguishable from the line changing it, so a line that only reads can be recorded as a write, and two lines that overlap can each count the same change.
- **A build that writes outside the skipped directories counts as a write.** Object files next to the sources, `.coverage`, `out/` and `bin/` are listed, so a line that produces them is a write and "a command ran after the last write" is false until another command runs.
- **A compromised delegate controls its own counts.** A delegate that read untrusted content picks how many commands and writes it makes, and the planner and judge see those numbers. Each of those calls still passed a gate or a person. Kept checkout paths, which carry much more, already reach the planner.
- **A write through a link that lands in the session's own directory is not counted**
  ([GROUND-1](#GROUND-1)), so it is hidden from the unbuilt-changes warning
  ([open-loops.md](open-loops.md#known-costs)).
- **A command is not a test run.** `ls` counts as a command. The judge is told the count says nothing about what ran or how it went.
