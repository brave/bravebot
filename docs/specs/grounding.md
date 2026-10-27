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

- **writes:** calls that changed a file
- **commands:** calls that ran a program
- whether the last command came after the last write

They are counted by the rules [TURN-4](turns.md#TURN-4) already uses. A write the person declined or a mode refused changed nothing and does not count. A command stopped with the turn does count. One command line is one command, however many programs it starts. Each write call is one write, even when two calls touch the same file. A call that both ran a program and changed a file counts as the write coming last, since nothing ran after what it wrote.

Records are added in the order the work happened. The later record's counts are added to the earlier one's. A later record with a write settles the order by its own flag. A later record with commands and no write means a command ran after the last write. A record with neither leaves the order as it was. The same rule adds a delegate's record to the turn that takes it back, and a turn's record to a goal's.

A delegate in the turn's workspace adds its whole record. One in a checkout given the turn's changes adds its commands and none of its writes, as [TURN-4](turns.md#TURN-4) counts it ([CHECKOUT-23](checkouts.md#CHECKOUT-23)).

**Why an order and not numbers.** A goal spans turns and a delegate runs alongside its parent, so call numbers would have to be kept across both. Only whether a command came after the last write matters, and that combines record by record.

**Why count calls.** The record is already kept per call, and each call is something you approved. Two writes to one file are two approvals.

`verified-by: bravebot_agent::outcome::each_write_that_changed_a_file_is_counted`
`verified-by: bravebot_agent::outcome::a_command_after_the_last_write_is_recorded_as_after_it`
`verified-by: bravebot_agent::outcome::a_command_in_a_later_turn_is_after_a_write_in_an_earlier_one`
`verified-by: bravebot_agent::turn::a_delegates_counts_are_added_to_the_turns_where_it_worked_on_the_turns_files`
`verified-by: bravebot_agent::turn::a_turns_outcome_carries_the_summary_of_its_work`

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

<a id="GROUND-5"></a>
### GROUND-5: the judge still decides

Verdicts are handled exactly as before. A `MET` ends the goal even if the record shows no commands. bravebot does not compare the record with the verdict, the condition or the reason.

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

- **A compromised delegate controls its own counts.** A delegate that read untrusted content picks how many commands and writes it makes, and the planner and judge see those numbers. Each of those calls still passed a gate or a person. Kept checkout paths, which carry much more, already reach the planner.
- **A command is not a test run.** `ls` counts as a command. The judge is told the count says nothing about what ran or how it went.
