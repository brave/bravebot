---
id: SHELL
title: Shell mode
status: normative
governs:
  - crates/agent/src/shell.rs
  - crates/tui/src/app.rs
  - crates/tui/src/app/actions.rs
  - crates/tui/src/app/directories.rs
  - crates/tui/src/app/keys.rs
  - crates/tui/src/app/opening.rs
  - crates/tui/src/app/records.rs
  - crates/tui/src/app/side_requests.rs
  - crates/tui/src/app/turns.rs
  - crates/tui/src/render/transcript.rs
  - crates/tui/src/state.rs
guards:
  - symbol: Policy::label_user_command_output
documented-by: docs/website/docs/using/shell-mode.md
---

## Scope

The `!` prompt: a line the user typed, run through a real shell. It is not a tool and the planner
cannot reach it. The planner's own path for running a program is
[tools/run.md](tools/run.md), and is a different thing entirely.

## Why this is not a hole in the shell exclusion

Read the exclusion precisely. It is not "shell strings are dangerous bytes". It is that a person
cannot endorse a routing field a shell string does not have, and the reason one must be endorsed
is that the string came from the **planner**, which an attacker may have steered. Provenance is
what the rule is about, so a line the user typed is outside it.

## Clauses

<a id="SHELL-1"></a>
### SHELL-1: `!` on an empty prompt runs the line through `$SHELL -c`

Globs, `$VAR`, redirection, `&&` and `$(...)` all work as they do in the user's terminal. The
marker is a mode rather than a character: the prompt changes colour, backspace or escape leaves,
and the mode lasts one command. `$SHELL` falls back to a POSIX shell when unset. An empty line is
not run.

`verified-by: bravebot_agent::shell::a_glob_is_expanded_by_the_shell`
`verified-by: bravebot_agent::shell::a_variable_is_expanded_by_the_shell`
`verified-by: bravebot_agent::shell::a_redirection_writes_the_file`
`verified-by: bravebot_agent::shell::stages_are_piped_together`
`verified-by: bravebot_agent::shell::commands_joined_with_and_both_run`
`verified-by: bravebot_agent::shell::the_shell_falls_back_to_a_posix_one_when_the_variable_is_unset`
`verified-by: bravebot_agent::shell::an_empty_line_is_not_run`
`verified-by: bravebot_tui::shell_mode::the_prompt_marker_changes_in_shell_mode`
`verified-by: bravebot_tui::shell_mode::the_hint_names_the_shell_while_in_shell_mode`

<a id="SHELL-2"></a>
### SHELL-2: nothing asks

The approval prompt exists so a person endorses argv the **planner** proposed. Here the person is
the one it would have asked, so confirming their own keystroke would be theatre. `! rm -rf build`
simply runs.

`verified-by: bravebot_agent::shell::the_command_is_recorded_as_something_the_user_did`
`verified-by: bravebot_agent::shell::a_command_line_is_run_and_recorded_without_any_way_to_ask`

<a id="SHELL-3"></a>
### SHELL-3: the output is `(T,priv)` and reaches the planner in full, unless the line was typed `!!`

Not as a reference. This is the difference from a program the planner ran itself: after
`! cargo test` the user can say "fix the first failure" and the planner has already read the
errors. Output from a failing command reaches it too, since that is where the explanation is. A
cancelled command records nothing. What the screen shows when a command ends with a result is that
released text, and what it shows while the command runs, and of one that was stopped, is
[SHELL-7](#SHELL-7).

This is a first label from provenance, exactly like the label on a program's output or on the
user's own configuration. It is admissible for the reason a vouched-for command is: a person took
responsibility, and nothing inspected anything.

`verified-by: bravebot_agent::shell::what_a_command_printed_reaches_the_planners_context`
`verified-by: bravebot_agent::shell::a_failing_commands_output_still_reaches_the_planner`
`verified-by: bravebot_agent::shell::trusting_the_output_is_recorded_in_the_trail`
`verified-by: bravebot_agent::shell::a_cancelled_command_records_nothing`
`verified-by: bravebot_agent::shell::what_is_shown_is_what_the_gate_released`
`verified-by: bravebot_tui::shell_mode::output_is_not_drawn_as_quarantined`
`verified-by: bravebot_tui::shell_mode::output_cannot_draw_its_own_escapes`

<a id="SHELL-6"></a>
### SHELL-6: a line typed `!!` runs, and its output is drawn and never recorded

A second `!` typed before anything else on an armed line makes that one command private. Like the
first it is a mode and not a character, so the line that runs is what follows both. The command runs
exactly as in [SHELL-1](#SHELL-1) and nothing asks ([SHELL-2](#SHELL-2)). What it printed passes the
same gate and is drawn, with a line saying it was not sent, and nothing is pushed into the
conversation, so the next request carries neither the line nor its output. The prompt shows `!!` and
the hint says the output stays on the screen. Backspace on the empty line removes the second marker
first. A line queued behind a turn keeps its form. Only a keystroke makes a line private, so a paste
cannot (SHELL-4). This opts out of what SHELL-3 delivers and does not change the default: a plain `!`
still records.

`verified-by: bravebot_tui::app::a_second_bang_makes_the_command_private`
`verified-by: bravebot_tui::app::a_plain_command_after_a_private_one_is_recorded_again`
`verified-by: bravebot_tui::app::a_pasted_bang_does_not_make_the_command_private`
`verified-by: bravebot_tui::app::backspace_leaves_the_private_form_before_shell_mode`
`verified-by: bravebot_tui::app::a_private_command_leaves_the_conversation_unchanged`
`verified-by: bravebot_tui::state::a_queued_private_line_is_taken_as_private_only`
`verified-by: bravebot_tui::shell_mode::the_private_form_is_drawn_with_a_doubled_marker_and_says_where_the_output_goes`
`verified-by: bravebot_tui::shell_mode::private_output_is_drawn_and_says_it_went_no_further`
`verified-by: bravebot_agent::shell::a_private_command_is_released_for_the_screen_and_the_trail`

<a id="SHELL-7"></a>
### SHELL-7: the screen shows what a command has printed while it runs

While the command runs, what it has printed so far is drawn under the command, from standard output
and standard error in the order the two were read. It is drawn by the step that draws the output of
a command that has ended, so [VIEW-3](terminal-transcript.md#VIEW-3) and
[VIEW-30](terminal-transcript.md#VIEW-30) hold for it: a complete colour code is counted and not
drawn, and every other escape and control character is replaced with a visible glyph. A colour code
or a character that a read stops inside is drawn whole once the rest of it has arrived, and until
then no escape reaches the terminal. Only the last 200 lines and at most 32 KiB of what it has
printed are drawn, a last line with no newline yet being one of the lines, and a row says how many
lines came before them. A command that has printed nothing, or only blank lines, draws nothing under
the spinner.

The rows are for the screen alone. They are not labelled, nothing is released for them, and none of
them reaches the conversation. When the command ends they are replaced by the text
[SHELL-3](#SHELL-3) releases, so the entry is the released text and nothing is drawn twice. A pipe
that fails to read part way leaves the result holding what was read, which is what was drawn.

A command that is stopped, or that runs past [`shell::LIMIT`](../../crates/agent/src/shell.rs),
releases nothing. What it had printed stays on the screen as an entry above the note saying why it
ended, beginning with the row that says how many lines came before where the rows held only the end
of it. A second note says that what it printed was not sent to the model, and none of it reaches the
conversation. A command that is stopped before it has printed anything has only the first note. A
command whose output the gate refuses, or that cannot be read, leaves none of the rows.

**Why.** A build or a server start prints for minutes. Drawn only when it ends, its output leaves a
person unable to tell a slow command from a hung one or to decide to stop it from what it has
printed, and a person who stops it from an error needs the error to stay on the screen. The rows are
the output of a line the person typed, drawn on their own screen, so they may be drawn before the
gate has run. The gate still decides what the planner reads and what the transcript keeps of a
command that ended. The rows are not a prefix of the released text: that text puts standard error
after standard output, and the rows put each where it was read.

`verified-by: bravebot_agent::shell::what_a_command_prints_is_sent_before_the_command_ends`
`verified-by: bravebot_agent::shell::standard_error_is_sent_as_it_is_printed`
`verified-by: bravebot_agent::shell::a_character_a_read_stops_inside_is_sent_whole`
`verified-by: bravebot_agent::shell::the_pieces_put_together_are_what_the_command_printed`
`verified-by: bravebot_agent::shell::a_command_runs_on_when_nothing_is_reading_what_it_prints`
`verified-by: bravebot_agent::shell::a_stopped_command_has_sent_what_it_printed_and_is_still_cancelled`
`verified-by: bravebot_agent::shell::what_was_read_before_a_pipe_failed_is_kept`
`verified-by: bravebot_tui::app::a_frame_drawn_while_a_command_runs_shows_what_it_has_printed`
`verified-by: bravebot_tui::app::a_stopped_command_keeps_what_it_printed_on_the_screen_and_out_of_the_conversation`
`verified-by: bravebot_tui::app::a_command_that_ended_is_drawn_as_the_text_the_gate_released`
`verified-by: bravebot_tui::shell_mode::output_that_has_been_printed_is_drawn_while_the_command_runs`
`verified-by: bravebot_tui::shell_mode::a_command_that_ended_draws_its_output_once`
`verified-by: bravebot_tui::shell_mode::running_output_cannot_draw_its_own_escapes`
`verified-by: bravebot_tui::shell_mode::a_colour_code_split_between_two_pieces_is_counted_once_it_is_whole`
`verified-by: bravebot_tui::shell_mode::only_the_end_of_a_long_output_is_held_while_it_runs`
`verified-by: bravebot_tui::shell_mode::one_endless_line_is_held_to_a_bounded_size`
`verified-by: bravebot_tui::shell_mode::a_bound_inside_a_character_moves_to_the_end_of_it`
`verified-by: bravebot_tui::shell_mode::a_command_that_has_printed_only_blank_lines_draws_no_output`
`verified-by: bravebot_tui::shell_mode::the_blank_lines_ending_running_output_are_not_drawn`
`verified-by: bravebot_tui::shell_mode::a_line_still_being_written_counts_as_one_of_the_lines_held`
`verified-by: bravebot_tui::shell_mode::a_piece_larger_than_the_bound_is_cut_from_its_end_and_counted`
`verified-by: bravebot_tui::shell_mode::a_stopped_command_keeps_the_count_of_the_lines_before_the_rows`
`verified-by: bravebot_tui::app::a_command_stopped_before_it_printed_says_only_that_it_stopped`
`verified-by: bravebot_tui::app::what_a_command_printed_after_the_last_frame_is_taken_when_it_ends`
`verified-by: bravebot_tui::app::a_command_whose_output_was_refused_leaves_none_of_it_on_the_screen`
`verified-by: bravebot_tui::app::a_private_command_that_ended_is_drawn_as_released_and_says_it_was_not_sent`
`verified-by: bravebot_tui::app::the_view_left_open_under_a_running_command_does_not_draw_what_it_prints`

<a id="SHELL-4"></a>
### SHELL-4: only a line a human typed

Never argv the planner proposed, never text read from a file, never anything a processor produced,
never a line reconstructed from a transcript.

**Why.** The justification cannot be checked from the bytes, so it lives at the call site. Today
that is the TUI's shell mode and nothing else.

`verified-by: bravebot_tui::app::a_pasted_line_does_not_arm_shell_mode`
`verified-by: bravebot_tui::state::a_command_comes_back_as_words_and_not_as_a_command`

<a id="SHELL-5"></a>
### SHELL-5: the planner gets no shell tool, ever

Not behind a capability, not behind an approval prompt, not via MCP. If it could ask for one,
everything above is void.

`verified-by: bravebot_agent::tools::the_tool_set_is_reads_plus_gated_writes`
`verified-by: bravebot_agent::tools::no_shell_is_offered`
`verified-by: bravebot_agent::turn::a_shell_the_planner_names_is_not_dispatched`

## Known costs

- **The rows drawn while a command runs are drawn before the gate has run.** A change that made
  [SHELL-3](#SHELL-3)'s gate refuse a command's output would still leave the running rows on the
  screen until the command ended, and the entry kept for a stopped command would hold them. They are
  the output of a line the person typed, drawn on their own screen and reachable from nothing else,
  so drawing them says nothing about what the planner reads.
- **A partial line from one stream is joined to the next piece from the other.** The rows put each
  piece where it was read, as a terminal does, so text without a newline on standard output and a
  warning on standard error can share a row until the command ends and the released text separates
  them.
- **Output that rewrites its line with a carriage return draws every rewrite.** The `\r` of a
  progress meter is a control character, so [VIEW-3](terminal-transcript.md#VIEW-3) draws it as a
  glyph, and a line that is never ended grows to the byte bound. The command ending draws it the
  same way.
- **A command stopped while it prints can lose its last lines.** The entry kept for it holds what
  had been read when the command was found to have ended, and a line still being read when it was
  killed is not in it.
- **A line longer than the byte bound is kept from where the bound falls.** That can be inside a
  colour code, which then draws as the text of its end, since no escape is left to take out.
- **A paste into an armed command line is bytes the person did not type reaching the shell.** The
  text lands in the line verbatim, and a line put away comes back into an armed mode as the command
  being written, which is what [terminal-input.md](terminal-input.md) settles for the key that puts
  a line away and brings it back. What Enter asserts is the line in the box, which the person armed
  the mode for and is looking at, and that is the same assertion they make for a command typed one
  character at a time. What the mode may never do is arm itself, since then nobody chose to run
  anything at all.
- **`! cat notes-from-a-stranger.md` puts somebody else's words into the planner's context as
  though they were the user's.** Nothing inspects the bytes to catch that, exactly as nothing
  inspects a directory that was vouched for. It is the same assertion a person makes by vouching for a
  command at a run prompt, made once for one command. If the user would not press `a` for it, they should ask the agent to `run` it
  instead and have the output quarantined.
