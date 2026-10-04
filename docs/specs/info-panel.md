---
id: PANEL
title: Telling sessions apart
status: normative
governs:
  - crates/tui/src/title.rs
  - crates/tui/src/panel.rs
documented-by: docs/website/docs/using/sessions.md
---

## Scope

What lets somebody running many sessions at once tell which terminal holds which: the terminal's
title, and the info panel beside the transcript, which holds the session's name, its goal, the
issue and pull request it is for, its context and cache, the language servers and MCP servers it
has started, and its plan. Giving a session its links with `--issue` and `--pr`, from a URL in a
prompt, and from what `gh pr create` prints are proposed in
[brave/bravebot#1267](https://github.com/brave/bravebot/issues/1267) and not built. How a session
gets its name is [sessions.md](sessions.md), and the keys a settings file can move are
[INPUT-32](terminal-input.md#INPUT-32).

## Clauses

<a id="PANEL-1"></a>
### PANEL-1: the terminal's title names the session

Once a session has a name, the terminal's title is `bravebot · {name}`. It follows the name: it is
written when the name is first known (after the first turn, or at once on a resume or a `/rename`),
written again whenever the name changes, and set to the bare `bravebot` when the session loses its
name, as a `/clear` does, rather than left naming the session before it. Nothing is written before a
session has a name, and nothing is written again while the name stays the same.

`verified-by: bravebot_tui::title::a_rename_rewrites_the_title_and_an_unchanged_name_does_not`
`verified-by: bravebot_tui::title::no_title_is_written_before_a_name_and_a_lost_name_leaves_the_prefix`

<a id="PANEL-2"></a>
### PANEL-2: a name cannot act on the terminal

Control characters in the name are replaced by visible characters, as they are wherever the
interface draws text, so a name cannot end the escape sequence the title travels in and act on the
terminal. The title is cut to 60 columns, with an ellipsis where it was cut.

`verified-by: bravebot_tui::title::control_characters_in_a_name_cannot_reach_the_terminal`
`verified-by: bravebot_tui::title::a_long_name_is_cut_to_sixty_columns`

<a id="PANEL-3"></a>
### PANEL-3: the shell's title comes back

The title the terminal had is pushed before the first one is written and popped each time the
terminal is handed back, at exit and while an external editor holds it (xterm's `CSI 22;0t` and
`CSI 23;0t`); taking the terminal back writes the title again. The title is emptied just before each
pop, so a terminal that ignores the push and the pop is left with no title rather than one naming a
session that has ended.

`verified-by: bravebot_tui::title::the_old_title_is_pushed_once_before_the_first_title`
`verified-by: bravebot_tui::title::handing_back_pops_once_and_taking_over_writes_the_title_again`

<a id="PANEL-4"></a>
### PANEL-4: when no title is written

A `terminalTitle` setting of `false` turns all of it off, the push included, and any other value,
or none, leaves it on. An incognito session writes no title whatever the setting says: its name is
the first line of a prompt, and a terminal with no title stack, or one that saves its windows
between launches, would keep that line after the session ended.

`verified-by: bravebot_tui::title::nothing_is_written_when_the_title_is_turned_off`
`verified-by: bravebot_tui::title::only_false_turns_the_run_title_off`
`verified-by: bravebot_tui::title::an_incognito_session_writes_no_title`
`verified-by: bravebot_config::settings::only_a_boolean_turns_the_terminal_title_off`
`verified-by: bravebot_config::settings::the_terminal_title_switch_is_among_the_names_reported`

<a id="PANEL-5"></a>
### PANEL-5: a key shows and hides the panel, and the choice is kept

The `panel` action shows and hides the info panel. It is on `ctrl-x` unless a `keybindings` block
moves it ([INPUT-32](terminal-input.md#INPUT-32)), and a moved panel leaves `ctrl-x` doing nothing.
`/panel` does the same, and runs at once while a turn is working. Each press writes whether the
panel is open to `~/.bravebot/panel`, and the next start reads it, as the theme is
([SESSION-9](sessions.md#SESSION-9)). No file, or a file that says neither `open` nor `closed`,
leaves the panel closed. The choice is not in the session record, so a resume opens with the panel
as the last press left it.

**Why `ctrl-x`.** Ctrl-I is Tab to a terminal, Ctrl-H, Ctrl-M and Ctrl-[ are Backspace, Enter and
Escape, Ctrl-Z suspends, Ctrl-Q is flow control, and the box answers the other Ctrl letters. A
settings file that already gave `ctrl-x` to another action now names a chord the panel stands on,
and since two actions never share one ([terminal-input.md](terminal-input.md#INPUT-32)), that action
is back on its default until the file moves the panel too.

`verified-by: bravebot_tui::keybindings::the_panel_chord_is_ctrl_x_and_can_be_moved`
`verified-by: bravebot_tui::app::the_panel_key_and_the_panel_command_both_toggle_it`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`
`verified-by: bravebot_tui::persist::whether_the_panel_was_left_open_is_read_back_next_session`
`verified-by: bravebot_tui::persist::a_press_opens_the_next_session_too_and_a_corrupt_choice_leaves_it_closed`
`verified-by: bravebot_session::store::only_the_two_panel_words_are_a_choice`

<a id="PANEL-6"></a>
### PANEL-6: the panel takes its columns from the transcript, and only where the rest still fits

The panel is 36 columns on the right of the screen, its left edge a border, and runs from the top
row to the row above the hint line, which keeps the full width. The transcript and the input box
take the columns to its left and wrap to them. It is drawn only on a terminal at least 100 columns
wide, which leaves the transcript 64. A press to open it on a narrower terminal changes nothing and
leaves a note saying how wide the terminal has to be. A terminal narrowed below 100 columns while
the panel is open stops drawing it and draws it again when widened, with no press. A press to close
it closes it on a terminal of any width, so a panel closed while narrowed out of sight stays closed
when the terminal is widened. Whether the panel is drawn and whether it is open are separate facts,
and the clauses below are about the first.

`verified-by: bravebot_tui::panel::the_panel_is_drawn_from_a_hundred_columns_and_not_below`
`verified-by: bravebot_tui::panel::a_press_on_a_narrow_terminal_does_not_open_the_panel_and_does_close_it`
`verified-by: bravebot_tui::panel::the_transcript_and_the_input_wrap_to_the_columns_the_panel_leaves`

<a id="PANEL-7"></a>
### PANEL-7: the panel ends with the key that hides it

The panel's last row is `{chord} hide panel`, naming the chord in force. While the panel is closed
on a terminal wide enough for it, the hint line has a `{chord} info` part, and that part is the
first the line drops when the row is short ([INPUT-13](terminal-input.md#INPUT-13)). With the panel
open, or on a terminal too narrow for it, the line has no such part.

`verified-by: bravebot_tui::panel::the_last_row_names_the_chord_in_force`
`verified-by: bravebot_tui::panel::the_hint_line_offers_the_panel_only_while_it_is_closed_and_would_fit`
`verified-by: bravebot_tui::panel::the_info_part_is_the_first_the_hint_line_gives_up`

<a id="PANEL-8"></a>
### PANEL-8: the context and cache figures move into the panel while it is drawn

While the panel is drawn it holds the context reading, in whichever of the three states of
[INPUT-22](terminal-input.md#INPUT-22) it is in, and the cache hit rate
([BACKEND-31](backends.md#BACKEND-31)), and the hint line drops both. Below them the panel gives the
last turn's cache figures as two rows, what was read and what was written, never summed, and
neither row before a turn has reported one. When the panel is not drawn, or the terminal is too
short for the panel to show its Context section whole, the hint line has the reading and the hit
rate. The loop, the job count, the mode and the keys stay on the hint line in every case, because a
loop and a job spend while nobody is watching, and the panel can be closed or the terminal too
narrow for it.

`verified-by: bravebot_tui::panel::the_context_reading_moves_into_the_panel_in_each_state`
`verified-by: bravebot_tui::panel::the_cache_read_and_written_are_two_figures`
`verified-by: bravebot_tui::panel::the_loop_stays_on_the_hint_line_while_the_panel_is_drawn`
`verified-by: bravebot_tui::panel::the_context_reading_stays_on_the_hint_line_where_the_panel_is_too_short_for_it`

<a id="PANEL-9"></a>
### PANEL-9: the sections, in order, each left out when it has nothing

1. **Session.** The session's name, wrapped to at most three rows, the third cut with an ellipsis
   where more was left. Below it the directory, written from `~` as `/status` writes it, and the
   branch, as the resume list shows it: read when the session starts, resumes or moves to another
   directory, so a checkout made during a turn shows from the next of those. Each is one row, cut
   from the left with an ellipsis where it is longer, since the end is what tells two apart. A new
   name reaches an open panel on the next frame, not at the next key.
2. **Goal.** The condition, while a goal stands ([GOAL-1](goal.md#GOAL-1)).
3. **Links.** PANEL-12.
4. **Context.** PANEL-8. It always has a reading, so it is always there.
5. **Language servers.** PANEL-13.
6. **MCP servers.** PANEL-14.
7. **Plan.** PANEL-10.

A section with nothing to show has no heading. Control characters in any row are drawn as visible
characters, as they are wherever the interface draws text, since a directory or a branch is called
whatever whoever made it chose.

`verified-by: bravebot_tui::panel::the_sections_come_in_order_and_an_empty_one_leaves_no_heading`
`verified-by: bravebot_tui::panel::a_long_name_takes_three_rows_and_ends_in_an_ellipsis`
`verified-by: bravebot_tui::panel::control_characters_in_the_session_section_are_drawn_as_pictures`
`verified-by: bravebot_tui::panel::a_long_directory_and_branch_keep_their_ends`
`verified-by: bravebot_tui::panel::a_new_name_redraws_an_open_panel_and_nothing_else_does`

<a id="PANEL-10"></a>
### PANEL-10: the plan stays after the turn that wrote it

The plan is the session's last `todo_write` list ([TODO-1](tools/todo-write.md#TODO-1)), each row
with the mark [TODO-2](tools/todo-write.md#TODO-2) gives it, and a finished row struck through. It
stays after the turn that wrote it ends, is replaced by the next report that has rows, and is
emptied by `/clear`. A report with no rows leaves it as it was, since the session record does not
tell a turn that emptied its list from one that kept none. It is the last section and has the rows
the others leave, the language servers and MCP servers included. A plan longer than that shows the rows that fit less one. Where the task in
progress would fall below the cut, the rows shown move down the list until it is among them. The
last row counts the rows left out above the ones shown apart from those below them (`+12 earlier`,
`+15 earlier, +7 more`, `+3 more`).

`verified-by: bravebot_tui::panel::the_plan_stays_after_the_turn_and_clear_empties_it`
`verified-by: bravebot_tui::panel::a_plan_longer_than_the_room_keeps_the_task_in_progress_and_counts_each_side_of_it`

<a id="PANEL-11"></a>
### PANEL-11: the panel draws only on what the person and the driver said

Every row the panel draws comes from text the person typed (the session's name, which comes from a
prompt or a `/rename` ([SESSION-4](sessions.md#SESSION-4)), a goal's condition, and the links given
with `/issue` and `/pr`), the planner's own `todo_write` rows, the driver's own counters (the context
reading and the cache figures), the chord in force, the directory and branch the session runs in, the
programs of the language servers it started (PANEL-13), or the aliases of its MCP servers (PANEL-14).
No row comes from a reply, a tool result, a file's contents, a language server's reply, an issue or
pull request body, or anything a job printed.

**Why.** The panel is the interface speaking in its own voice, like the scroller's footer
([SCROLL-5](scroller.md#SCROLL-5)). Text from any of those drawn in it would read as the interface's
own sentence, and it would be in front of the person on every frame.

`verified-by: bravebot_tui::panel::nothing_a_turn_returned_reaches_the_panel`

<a id="PANEL-12"></a>
### PANEL-12: the person says which issue and pull request the session is for

`/issue <url>` and `/pr <url>` give the session its issue and its pull request. A value is one
`http` or `https` URL with a host, in printable ASCII. One with anything else in it (whitespace, a
newline, an escape, a direction override or a zero-width character), with no host, or with any
other scheme, is refused with a note that does not repeat it, and sets nothing. The bare word says what is set, and `clear` removes that link and
not the other. Each change is written to the session record at once, as a rename is
([SESSION-3](sessions.md#SESSION-3)), and gives up no rewind point, since no point holds a link.
Typed while a turn runs, they are carried out as `/rename` is ([CMD-8](commands.md#CMD-8)).

The Links section has a `Pull request` row and then an `Issue` row, each only while that link is
set: the word, then the link, cut from the left with an ellipsis where it is longer than the row,
since the number at its end is what tells two apart. A new link reaches an open panel on the next
frame.

`verified-by: bravebot_tui::app::the_issue_and_pr_commands_show_set_and_clear_their_own_link`
`verified-by: bravebot_tui::app::a_link_with_a_newline_an_escape_or_another_scheme_sets_nothing`
`verified-by: bravebot_tui::app::a_link_set_mid_turn_is_set_as_it_is_typed`
`verified-by: bravebot_session::sessions::only_one_web_address_on_one_line_is_a_link`
`verified-by: bravebot_tui::panel::the_links_section_has_a_row_for_each_link_that_is_set`
`verified-by: bravebot_tui::panel::a_long_link_keeps_its_end`
`verified-by: bravebot_tui::panel::a_new_name_redraws_an_open_panel_and_nothing_else_does`

<a id="PANEL-13"></a>
### PANEL-13: the language servers the session started

The Language servers section has one row for each language server the session has started
([LSP-8](tools/lsp.md#LSP-8)), in name order, and is left out until one has. A row is the program
the person approved ([LSP-5](tools/lsp.md#LSP-5)), from the fixed table of programs and never from
the server's own reply, so a server cannot put its own words into the interface. The rows stay
while a question waits on an approval or a server's answer, since the panel reads a list kept
beside the set rather than the set, and the section empties when the set is dropped, as `/cd` and
`/clear` drop it.

`verified-by: bravebot_tui::panel::each_started_language_server_has_a_row_and_none_leaves_no_heading`
`verified-by: bravebot_tui::panel::the_plan_gets_the_rows_the_server_sections_leave`
`verified-by: bravebot_agent::lsp::the_roster_names_the_program_a_turn_started_until_the_set_is_dropped`

<a id="PANEL-14"></a>
### PANEL-14: the MCP servers the session started

The MCP servers section has one row for each server that started and completed its handshake
([SERVERS-14](mcp-servers.md#SERVERS-14)), by the alias the person's settings gave it, in the order
they were started, and is left out where none did. Control characters in an alias are drawn as
visible characters, and an alias longer than a row is cut from the left, since the end of an alias
is what tells two apart.

`verified-by: bravebot_tui::panel::an_mcp_alias_is_drawn_pictured_and_cut_from_the_left`
`verified-by: bravebot_tui::panel::the_sections_come_in_order_and_an_empty_one_leaves_no_heading`
