---
id: GROUND
title: Telling the planner and the goal judge what actually ran
status: proposed
governs:
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/delegate_end.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/goal.rs
  - crates/agent/src/outcome.rs
  - crates/tui/src/goals.rs
  - crates/tui/src/state.rs
  - crates/tui/src/app.rs
documented-by:
  - none (gap: a user docs page, owed once this is built)
---

## Scope

Two model claims that today nobody checks:

- a delegate's report to the planner ([delegation.md](delegation.md))
- a goal judge's verdict ([goal.md](goal.md))

bravebot already knows what commands ran and what files were written. This spec makes it count them and show the counts next to each claim. It never blocks anything and never changes the claim.

Nothing here is built yet. Each clause says `verified-by: none` and lists the tests to write.

## Why it exists

- A delegate can say "tests pass" without running anything. The planner then repeats it to you as fact.
- A judge can rule a goal met because the planner said the tests pass, after compaction removed the run that would have shown otherwise.

## What exists today

- Each run keeps a small record of its work: the round a file first changed, and whether a program ran. Round numbers restart every turn. A delegate hands its record back, and the turn uses it for the "you changed code but didn't build it" question ([TURN-4](turns.md#TURN-4), [DELEGATE-9](delegation.md#DELEGATE-9)). The planner is not told any of it.
- A goal judge gets a copy of the conversation and the goal condition, nothing else ([GOAL-3](goal.md#GOAL-3)). The goal lives in the terminal app's session state, which builds the judge's request at the end of each turn.

## Clauses

<a id="GROUND-1"></a>
### GROUND-1: count commands and writes, in session order

Extend each run's work record with two counts:

- **writes:** calls that changed a file
- **commands:** calls that ran a program

Count them by the rules [TURN-4](turns.md#TURN-4) already uses. A write the person declined or a mode refused doesn't count. A command stopped when the turn was stopped does count.

One command line is one command, however many programs it starts. Each write call is one write, even when two calls touch the same file.

Number every call with a counter that keeps rising for the whole session. Record the numbers of the last write and the last command. "A command ran after the last write" means the last command's number is higher.

A delegate's record comes back whole: its two counts, plus whether its last command came after its last write. The parent gives it the next number when it collects the delegate. If the delegate's last write and last command both get that one number, the delegate's own flag decides which came first.

**Why a session counter.** Round numbers restart every turn, and a goal spans turns. A delegate runs alongside its parent, so its calls can't be put in the parent's order. Only its own last-write and last-command order matters, so that is what it reports.

**Why count calls.** The record is already kept per call, and each call is something you approved. Two writes to one file are two approvals.

Tests to write:

- `bravebot_agent::turn::each_write_that_changed_a_file_is_counted`
- `bravebot_agent::turn::a_write_the_person_refused_is_not_counted`
- `bravebot_agent::turn::a_line_with_a_pipeline_is_one_command`
- `bravebot_agent::turn::a_command_after_the_last_write_is_recorded_as_after_it`
- `bravebot_agent::turn::a_delegates_counts_are_added_to_the_turns_when_it_worked_in_the_workspace`
- `bravebot_agent::turn::a_command_in_a_later_turn_is_after_a_write_in_an_earlier_one`
- `bravebot_agent::turn::a_delegates_own_order_settles_whether_its_command_followed_its_write`

`verified-by: none`

<a id="GROUND-2"></a>
### GROUND-2: tell the planner what each delegate actually did

When a delegate is collected, add one sentence to the planner's message about it: "Delegate N ran X commands and made Y writes." Add "in its own checkout" when that applies. Say zero as zero.

Put the sentence next to the report, not inside it: after any note about limits or stopping, before the report. That way the planner still reads it when the report is a quarantined reference.

Add it however the delegate ended: answered, stopped or failed. If the delegate's thread died, nothing comes back and nothing is said.

**Why.** It answers "I ran the tests and they pass" from a delegate that ran nothing. The planner is about to build on the report, and to repeat it to you, so the planner needs to know.

Tests to write:

- `bravebot_agent::turn::a_delegates_report_is_put_beside_how_many_commands_it_ran_and_writes_it_made`
- `bravebot_agent::turn::a_delegate_that_ran_nothing_is_said_to_have_run_no_command`
- `bravebot_agent::turn::a_quarantined_report_carries_the_same_sentence_about_the_work`
- `bravebot_agent::turn::a_delegate_that_failed_is_still_said_to_have_done_what_it_did`
- `bravebot_agent::turn::a_delegate_in_a_checkout_is_said_to_have_worked_there`

`verified-by: none`

<a id="GROUND-3"></a>
### GROUND-3: show the person the same counts, without paths

The note you see when a delegate finishes gets the same two counts as GROUND-2. It does not list the files written.

**Why no paths.** You already saw each write when it was approved or shown in the transcript. A kept checkout's paths are already listed next to the report ([CHECKOUT-18](checkouts.md#CHECKOUT-18)). The note should stay one line.

Tests to write:

- `bravebot_agent::turn::the_note_for_a_finished_delegate_counts_its_commands_and_writes`
- `bravebot_agent::turn::the_note_for_a_finished_delegate_names_no_path`

`verified-by: none`

<a id="GROUND-4"></a>
### GROUND-4: give the goal judge the record since the goal was set

Add one paragraph to the judge's request, before the condition. It says how many commands ran and how many writes were made since the goal was set, and whether a command ran after the last write. It also tells the judge this is bravebot's own count and says nothing about whether any command succeeded.

Each turn returns a summary of its work with its outcome: the two counts and the numbers of its last write and last command. The goal state in the terminal app adds up these summaries and passes the total to the judge's request.

- A turn already running when the goal was set is not counted. The record starts with the first turn the goal drives.
- Turns while the goal is paused are counted.
- A delegate's work counts where [TURN-4](turns.md#TURN-4) counts it.
- Lines you run yourself at the `!` prompt are not counted.
- A new goal starts a new record. The record is dropped with its goal.

**Why.** The judge only rules a condition met when the conversation shows it ([GOAL-3](goal.md#GOAL-3)). Compaction can shrink the run that showed it to a sentence, or remove it. The count survives compaction. If the judge reads "no command has run since the last write" while the planner says the tests pass, it can answer `NOT MET`.

Tests to write:

- `bravebot_agent::goal::the_judge_is_given_the_commands_and_writes_since_the_goal_was_set`
- `bravebot_agent::goal::the_judge_is_told_whether_a_command_ran_after_the_last_write`
- `bravebot_agent::goal::the_judge_is_told_the_record_says_nothing_of_success`
- `bravebot_agent::turn::a_turns_outcome_carries_the_summary_of_its_work`
- `bravebot_tui::goals::the_record_adds_every_turn_the_goal_drives`
- `bravebot_tui::goals::a_turn_running_when_the_goal_was_set_is_not_counted`
- `bravebot_tui::goals::a_turn_while_the_goal_is_paused_is_counted`
- `bravebot_tui::goals::a_new_goal_starts_a_new_record`
- `bravebot_tui::state::the_judges_request_is_built_with_the_goals_record`

`verified-by: none`

<a id="GROUND-5"></a>
### GROUND-5: the judge still decides

Verdicts are handled exactly as before. A `MET` ends the goal even if the record shows no commands. bravebot does not compare the record with the verdict, the condition or the reason.

**Why.** Overriding the judge would mean bravebot deciding what the condition means. Many conditions need no command at all. The record is evidence for the judge, not a rule.

Tests to write:

- `bravebot_agent::goal::a_met_verdict_ends_the_goal_whatever_the_record_says`
- `bravebot_agent::goal::the_driver_reads_no_word_of_the_record_back`

`verified-by: none`

<a id="GROUND-6"></a>
### GROUND-6: every new sentence is fixed text plus numbers

Every sentence this spec adds is fixed text with numbers filled in. The only values are counts and a delegate's number. None of them contains any text from a report, a file, a path or a command.

The person's note comes from the catalog and is translated. Sentences for a model are fixed English, like bravebot's other messages to models.

**Why.** If a sentence included text from what a delegate did, the delegate could use it to talk to the planner or the judge.

No new row is needed in [LABEL-8](labels.md#LABEL-8). The counts are bravebot's record of its own actions, which [TURN-4](turns.md#TURN-4) already sends to the planner.

Tests to write:

- `bravebot_agent::turn::delegate_end::the_work_sentence_holds_no_path_and_no_word_of_the_report`
- `bravebot_agent::goal::the_record_put_to_the_judge_holds_no_path_and_no_command`

`verified-by: none`

## Changes to other specs

- [delegation.md](delegation.md): [DELEGATE-9](delegation.md#DELEGATE-9) gains the sentence from [GROUND-2](#GROUND-2).
- [turns.md](turns.md): the [TURN-4](turns.md#TURN-4) record gains the counts and the last-write and last-command numbers ([GROUND-1](#GROUND-1)). Its question is unchanged.
- [goal.md](goal.md): [GOAL-3](goal.md#GOAL-3)'s request gains the paragraph from [GROUND-4](#GROUND-4).
- [localization.md](localization.md): the catalogs gain the person's note from [GROUND-3](#GROUND-3).

## Out of scope

- Checking a processor's remark or a check's reason against the content. Considered and cut: you already see the real diff, and the work touches every interface for little gain.
- Compaction summaries and recalled facts.
- Whether a command succeeded. bravebot has the exit status, but what it means depends on the program.
- Understanding claims. Nothing parses "tests pass" in any language.
- Blocking or approving anything based on a count ([GROUND-5](#GROUND-5)).

## Known costs

- **A compromised delegate controls its own counts.** A delegate that read untrusted content picks how many commands and writes it makes, and the planner and judge see those numbers. Each of those calls still passed a gate or a person. Kept checkout paths, which carry much more, already reach the planner.
- **A command is not a test run.** `ls` counts as a command. The judge is told the count says nothing about what ran or how it went.
