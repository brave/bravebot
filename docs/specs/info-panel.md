---
id: PANEL
title: Telling sessions apart
status: normative
governs:
  - crates/tui/src/title.rs
  - crates/tui/src/panel.rs
  - crates/tui/src/sidebar_prompt.rs
  - crates/tui/src/pointer.rs
  - crates/tui/src/state/references.rs
  - crates/agent/src/links.rs
documented-by: docs/website/docs/using/sessions.md
---

## Scope

What lets somebody running many sessions at once tell which terminal holds which: the terminal's
title, and the info panel beside the transcript, which holds the session's name, its goal, the
issues and pull requests it is for and has met, the model and effort it runs on, its context and
cache, its token total, the language servers and MCP servers it has started, and its plan, and the
checklist `/sidebar` opens that turns each section on or off ([PANEL-17](#PANEL-17)). A session
gets its own links from `/issue` and `/pr`, from `--issue` and `--pr`, and from the planner's
[`set_link`](tools/set-link.md) for an address the person typed. It lists the other issues and pull
requests it has met, which [PANEL-16](#PANEL-16) collects from the lines the person typed, from the
planner's own tool-call arguments and from content whose label is trusted. A session does not get its
links from what `gh pr create` prints. That needs a reader that branches on untrusted run output
([RUN-4](tools/run.md#RUN-4)), which is an exception no maintainer has accepted, so it is
not built ([brave/bravebot#1267](https://github.com/brave/bravebot/issues/1267)). The sections of
met items are collected without it, and an issue the agent creates with `gh issue create` is listed
only once a later argument or typed line names its number. How a session
gets its name is [sessions.md](sessions.md), and the keys a settings file can move are
[INPUT-32](terminal-input.md#INPUT-32). Listing the sessions that keep running after their terminal
is closed is proposed in [background-sessions.md](background-sessions.md) and not built.

## Clauses

<a id="PANEL-1"></a>
### PANEL-1: the terminal's title names the session

Once a session has a name, the terminal's title is `bravebot · {name}`. It follows the name: it is
written when the name is first known (when the first prompt is sent, or at once on a resume or a `/rename`),
written again whenever the name changes, and set to the bare `bravebot` when the session loses its
name, as a `/clear` does, rather than left naming the session before it. Nothing is written before a
session has a name unless [PANEL-15](#PANEL-15) has a state to show, and nothing is written again
while the name and the state stay the same.

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
as the last press left it. The width the panel was last dragged to ([PANEL-6](#PANEL-6)) is kept
the same way in `~/.bravebot/panel-width`, a single whole number of columns; no file, or one that
holds anything else, leaves the panel at 36. The sections the person turned off with `/sidebar`
([PANEL-17](#PANEL-17)) are kept the same way in `~/.bravebot/sidebar`, and are not in the session
record either.

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
`verified-by: bravebot_session::store::only_a_positive_whole_number_is_a_panel_width`
`verified-by: bravebot_tui::persist::the_dragged_width_is_read_back_next_session_and_a_corrupt_one_leaves_36`
`verified-by: bravebot_tui::persist::a_press_that_closes_the_panel_is_read_back_next_session`
`verified-by: bravebot_tui::persist::the_sections_turned_off_are_read_back_next_session_and_turning_them_on_replaces_them`
`verified-by: bravebot_session::store::the_sidebar_file_is_its_trimmed_nonblank_lines_and_nothing_else`
`verified-by: bravebot_session::store::an_oversize_sidebar_file_is_no_choice_and_one_at_the_bound_is_read`
`verified-by: bravebot_session::store::a_pipe_where_the_sidebar_file_should_be_is_no_choice_and_holds_nothing_up`
`verified-by: bravebot_tui::panel::references_over_a_panel_with_nothing_above_them_leave_the_context_and_the_plan_their_rows`

<a id="PANEL-6"></a>
### PANEL-6: the panel takes its columns from the transcript, and only where the rest still fits

The panel is 36 columns on the right of the screen unless it has been dragged to another width, its left edge a border, and runs from the top
row to the row above the hint line, which keeps the full width. The transcript and the input box
take the columns to its left and wrap to them. It is drawn only on a terminal at least 100 columns
wide, which leaves the transcript 64. A press to open it on a narrower terminal changes nothing and
leaves a note saying how wide the terminal has to be. A terminal narrowed below 100 columns while
the panel is open stops drawing it and draws it again when widened, with no press. A press to close
it closes it on a terminal of any width, so a panel closed while narrowed out of sight stays closed
when the terminal is widened. Whether the panel is drawn and whether it is open are separate facts,
and the clauses below are about the first.

Dragging the border moves it: a left-button press on the border column starts a drag, the drag sets
the panel to the columns right of the pointer, and the release ends it and keeps the width
([PANEL-5](#PANEL-5)). The width is held between 24 columns and what leaves the transcript 64, so
the 100-column threshold holds at the default width. A terminal resized while the panel is open
draws it at the stored width clamped to the new size, and the stored width is left as it was. A
press on the border starts no selection in either pane ([MOUSE-2](mouse.md#MOUSE-2)), and the
release of a drag that began there copies nothing. A drag that the button does not end, because the
window lost focus, the terminal was resized or another press landed, ends as the release ends it,
and keeps the width ([MOUSE-49](mouse.md#MOUSE-49)). The drag has no key: the width is a choice about
where the room goes and nothing else changes with it, and a person for whom the panel does not fit
hides it with the chord ([PANEL-5](#PANEL-5)).

In a terminal that draws a pointer shape on request, the pointer is the resize one while it is over
the border column, from the panel's first row to its last, and while a drag of the border is held.
It is asked for with OSC 22 when the shape changes and not for each report of the pointer, and it is
given back when the pointer leaves the border, when the panel closes, and before the terminal is
handed back. Kitty, Ghostty and foot are asked, with the name `ew-resize`. Any other terminal is
written nothing and keeps its own pointer. Every terminal is told of motion with no button held
([INPUT-33](terminal-input.md#INPUT-33)), which is how the pointer is found over the border.

`verified-by: bravebot_tui::panel::the_panel_is_drawn_from_a_hundred_columns_and_not_below`
`verified-by: bravebot_tui::panel::a_press_on_a_narrow_terminal_does_not_open_the_panel_and_does_close_it`
`verified-by: bravebot_tui::panel::the_transcript_and_the_input_wrap_to_the_columns_the_panel_leaves`
`verified-by: bravebot_tui::panel::a_drag_on_the_border_moves_it_and_the_transcript_rewraps`
`verified-by: bravebot_tui::panel::the_width_is_clamped_at_both_ends`
`verified-by: bravebot_tui::panel::a_narrowed_terminal_reclamps_without_forgetting_the_choice`
`verified-by: bravebot_tui::panel::a_drag_from_the_border_never_selects_or_copies`
`verified-by: bravebot_tui::app::tests::scrollbar::a_drag_of_the_panels_border_ends_when_focus_is_lost_the_terminal_resizes_or_another_press_lands`
`verified-by: bravebot_tui::panel::the_hint_line_runs_under_the_panel_to_the_right_edge`
`verified-by: bravebot_tui::panel::the_resize_pointer_is_over_the_border_column_and_nowhere_else`
`verified-by: bravebot_tui::panel::a_drag_holds_the_resize_pointer_and_its_end_follows_the_border`
`verified-by: bravebot_tui::panel::a_panel_closed_under_the_pointer_gives_the_resize_pointer_back`
`verified-by: bravebot_tui::pointer::a_shape_is_written_when_it_changes_and_not_while_it_stays`
`verified-by: bravebot_tui::pointer::a_terminal_with_no_known_name_is_written_nothing`
`verified-by: bravebot_tui::pointer::only_terminals_documented_to_take_css_names_are_asked`

<a id="PANEL-7"></a>
### PANEL-7: the panel ends with the key that hides it

The panel's last row is `{chord} hide panel`, naming the chord in force, and in the files state the
keys of that state instead ([FILES-4](files-mode.md#FILES-4)). While the panel is closed
on a terminal wide enough for it, the hint line has a `{chord} info` part, and that part is the
first the line drops when the row is short ([INPUT-13](terminal-input.md#INPUT-13)). With the panel
open, or on a terminal too narrow for it, the line has no such part.

`verified-by: bravebot_tui::panel::the_last_row_names_the_chord_in_force`
`verified-by: bravebot_tui::files_mode::the_last_row_of_the_panel_names_the_keys_of_the_state`
`verified-by: bravebot_tui::panel::the_hint_line_offers_the_panel_only_while_it_is_closed_and_would_fit`
`verified-by: bravebot_tui::panel::the_info_part_is_the_first_the_hint_line_gives_up`

<a id="PANEL-8"></a>
### PANEL-8: the context and cache figures move into the panel while it is drawn

While the panel is drawn it holds the context reading, in whichever of the three states of
[INPUT-22](terminal-input.md#INPUT-22) it is in, and the cache hit rate
([BACKEND-31](backends.md#BACKEND-31)), and the hint line drops both. Below them the panel gives the
cache figures as two rows, what was read and what was written, never summed. A turn in flight
gives its running totals once one of its rounds has reported a completed request, and the last
turn's figures until then, so a first turn shows them after its first round rather than at its
end. The hit rate reads the same figures, on the hint line as well. Neither row is drawn before
any turn or round has reported one. When the panel is not drawn, or the terminal is too
short for the panel to show its Context section whole, the hint line has the reading and the hit
rate. The loop, the job count, the mode and the keys stay on the hint line in every case, because a
loop and a job spend while nobody is watching, and the panel can be closed or the terminal too
narrow for it. The same holds while the Context section is turned off with `/sidebar`
([PANEL-17](#PANEL-17)): the panel has no Context section then, so the hint line keeps the reading
and the hit rate, and the token total and the two cache rows are not drawn.

`verified-by: bravebot_tui::panel::the_context_reading_moves_into_the_panel_in_each_state`
`verified-by: bravebot_tui::panel::the_cache_read_and_written_are_two_figures`
`verified-by: bravebot_tui::app::a_first_turn_in_flight_draws_the_cache_figures_after_its_first_round`
`verified-by: bravebot_tui::app::each_round_of_a_turn_replaces_the_cache_figures_with_the_turns_running_totals`
`verified-by: bravebot_tui::app::a_report_with_no_completed_request_leaves_the_cache_figures_where_they_were`
`verified-by: bravebot_tui::panel::the_loop_stays_on_the_hint_line_while_the_panel_is_drawn`
`verified-by: bravebot_tui::panel::the_context_reading_stays_on_the_hint_line_where_the_panel_is_too_short_for_it`
`verified-by: bravebot_tui::panel::the_context_reading_and_the_hit_rate_stay_on_the_hint_line_while_the_context_section_is_off`
`verified-by: bravebot_tui::panel::the_reading_moves_to_the_panel_where_the_sections_left_on_have_the_room`

<a id="PANEL-9"></a>
### PANEL-9: the sections, in order, each left out when it has nothing

1. **Session.** The session's name, wrapped to at most three rows, the third cut with an ellipsis
   where more was left. A word of the name that is a whole GitHub pull request or issue address,
   `https://github.com/{owner}/{repo}/pull/{n}` or `.../issues/{n}`, with only the characters
   GitHub allows in the owner and repository and only digits in the number, is drawn as
   `{owner}/{repo}#{n}`. Its cells are a terminal link (OSC 8) to the address, and a selection
   over them copies the shorthand. An address of any other shape is drawn as written. Below it the
   directory, written from `~` as `/status` writes it, and the
   branch, as the resume list shows it: read when the session starts, resumes or moves to another
   directory, so a checkout made during a turn shows from the next of those. Each is one row, cut
   from the left with an ellipsis where it is longer, since the end is what tells two apart. Last
   comes the session's id, the one `--resume`, `--fork` and `/resume` take, drawn whole over two
   rows split after its last hyphen, since a lookup reads the whole id and a fragment finds
   nothing. It is there once the session has a record on disk, as the line printed when the
   session ends is ([SESSION-8](sessions.md#SESSION-8)), and follows a resume, `/branch` and
   `/clear`. The record is written when a session's first prompt is sent, and not only when its
   turn ends, so the id is there for as long as a first turn runs, which can be hours. A session
   with no prompt sent has none. A new
   name reaches an open panel on the next frame, not at the next key. A click on a row of the name
   puts `/rename` and the name in the box ([MOUSE-30](mouse.md#MOUSE-30)).
2. **Model.** The model the person chose, one row cut from the left with an ellipsis where it is
   longer, and the effort level in force below it, as `effort high`. A model the configuration
   names is drawn by the name the `/model` picker gives its row, with the service after a tier's
   name, as `Sonnet (Bedrock)`, and not by the request name, which for a tier is an inference-profile
   ARN. `/status` draws it the same way and keeps the request name on the line beneath, since an
   error from the service quotes that. A model the configuration does not name is drawn by its
   request name. The model is the person's
   choice, never the name an endpoint replied with, so a reply
   that names another model changes nothing here. The effort row is left out where the model reads
   none. Both come from the session's settings, so the section is left out while the person has
   chosen no model and no effort level is in force.
3. **Goal.** The condition, while a goal stands ([GOAL-1](goal.md#GOAL-1)).
4. **Pull requests.** A row for the session's own pull request link and one for each pull request
   the session met ([PANEL-12](#PANEL-12), [PANEL-16](#PANEL-16)). A row is `{owner}/{repo}#{n}`,
   cut from the left with an ellipsis where it is longer than the row, and its cells are a terminal
   link (OSC 8) to the item's address.
5. **Issues.** The same for the session's own issue link and the issues it met. The two sections
   are cut to the room the sections below them and the Plan's reserve leave, the pull requests
   having their room before the issues do. A section that does not fit whole keeps the rows that
   do and ends with a row `+{n} more` that counts the rest. A section with no room for a row is
   left out with its heading, so a heading is never drawn over nothing and a count is never drawn
   over no row.
6. **Context.** PANEL-8. It always has a reading, so it is always there. Under the reading it
   has the session's token total, as `/cost` opens with it, in tokens and not money, counting what
   the turn in flight has spent so far, the requests its delegates have completed included as
   they complete, and left out until a token has been spent.
7. **Language servers.** PANEL-13.
8. **MCP servers.** PANEL-14.
9. **Files.** A `{n} files` row with the lines added and removed over every file the session wrote.
   A click on that row opens the files state ([MOUSE-20](mouse.md#MOUSE-20)), as `/diff` does
   ([FILES-3](files-mode.md#FILES-3)). It is left out until a file has been written. The Plan takes
   the rows it leaves, as PANEL-13 says of the server sections.
10. **Plan.** PANEL-10.

A section with nothing to show has no heading. A section the person turned off with `/sidebar`
([PANEL-17](#PANEL-17)) is left out the same way, with its heading, its rows and the blank row
before it, whatever it holds, and it is not counted in the room the other sections share. Control characters in any row are drawn as visible
characters, as they are wherever the interface draws text, since a directory or a branch is called
whatever whoever made it chose.

`verified-by: bravebot_tui::panel::the_sections_come_in_order_and_an_empty_one_leaves_no_heading`
`verified-by: bravebot_tui::panel::the_sections_of_references_are_cut_to_the_room_with_a_count_of_the_rest`
`verified-by: bravebot_tui::panel::the_references_do_not_move_where_the_context_section_fits`
`verified-by: bravebot_tui::panel::a_long_name_takes_three_rows_and_ends_in_an_ellipsis`
`verified-by: bravebot_tui::panel::a_github_address_in_the_name_is_drawn_as_its_shorthand`
`verified-by: bravebot_tui::panel::an_address_of_any_other_shape_is_drawn_as_written`
`verified-by: bravebot_tui::panel::the_shorthand_links_to_the_address_and_nothing_else_does`
`verified-by: bravebot_tui::select::a_link_is_copied_as_the_text_it_shows`
`verified-by: bravebot_tui::panel::control_characters_in_the_session_section_are_drawn_as_pictures`
`verified-by: bravebot_tui::panel::a_long_directory_and_branch_keep_their_ends`
`verified-by: bravebot_tui::panel::the_session_id_is_drawn_whole_over_two_rows`
`verified-by: bravebot_tui::panel::the_id_row_follows_whether_the_session_has_a_record`
`verified-by: bravebot_tui::panel::the_id_rows_count_toward_where_the_context_section_fits`
`verified-by: bravebot_tui::panel::the_model_section_shows_the_choice_and_the_effort_in_force_and_ignores_a_reply`
`verified-by: bravebot_tui::panel::the_model_section_names_a_tier_rather_than_the_tail_of_its_arn`
`verified-by: bravebot_tui::status::the_model_line_names_a_tier_and_keeps_its_arn_beneath`
`verified-by: bravebot_tui::status::a_name_a_provider_block_chose_is_shown_without_a_service_added`
`verified-by: bravebot_config::lib::a_request_name_finds_its_entry_in_either_kind_of_account`
`verified-by: bravebot_tui::panel::the_token_total_is_a_context_row_that_sums_the_session`
`verified-by: bravebot_agent::turn::the_token_total_moves_while_a_delegate_works_and_counts_it_once`
`verified-by: bravebot_tui::panel::a_new_name_redraws_an_open_panel_and_nothing_else_does`
`verified-by: bravebot_tui::panel::the_files_section_has_the_totals_and_leaves_the_plan_its_rows`
`verified-by: bravebot_tui::app::tests::herdr::sending_the_first_prompt_gives_the_panel_an_id_before_the_turn_ends`
`verified-by: bravebot_tui::panel::a_section_turned_off_is_not_drawn_and_no_other_section_goes_with_it`
`verified-by: bravebot_tui::panel::a_section_turned_off_has_no_click_target`
`verified-by: bravebot_tui::panel::the_references_are_not_cut_for_the_rows_of_a_section_turned_off`

<a id="PANEL-10"></a>
### PANEL-10: the plan stays after the turn that wrote it

The plan is the session's last `todo_write` list ([TODO-1](tools/todo-write.md#TODO-1)), each row
with a mark for its status, and a finished row struck through. It
stays after the turn that wrote it ends, is replaced by the next report that has rows, and is
emptied by `/clear`. A report with no rows leaves it as it was, since the session record does not
tell a turn that emptied its list from one that kept none. It is the last section and has the rows
the others leave, the language servers and MCP servers included, and the rows of a section turned
off with `/sidebar` ([PANEL-17](#PANEL-17)) are rows it has. Turned off itself, it is not drawn.

A plan longer than that shows the rows that fit less the row after them that says what was left out,
and less the key row where that is drawn. The rows shown are chosen from each row's status and from
nothing else, and a status that is not recognised reads as outstanding
([TODO-2](tools/todo-write.md#TODO-2)). The task in progress comes first, then the pending rows,
each in the plan's order, and then, where room is left, the finished rows (completed or cancelled)
nearest the first row still outstanding, or nearest the end of a plan with none. The rows shown are
drawn in the plan's order. The row after them counts each kind of row left out in words of its own,
the rows still to do first and a kind with none left out not named (`+6 done`,
`+3 to do, +40 done, +2 cancelled`), so a count of finished work is not read as work still to come
and a cancelled row is not counted as done. Where the panel is too narrow for every count the row
is cut with an ellipsis, and the count of the rows still to do is the last to be cut.

A finished row whose words a click opened ([MOUSE-32](mouse.md#MOUSE-32)) takes the rows its words
take, and counts as that many rows of the room. It is chosen after the rows still to do and before
the other finished rows, so what a person asked to read stays drawn, and where the room is less than
its rows it is given what is left. The model writes a plan of any length, so the work of laying the
rows out grows with the number of tasks and not with its square, with every task open or not.

The row that counts keeps its place however little room there is, and a row still to do gives up
its place to it. A plan of ten rows with two still to do, in a section of two rows, draws the first
of the two and counts the other (`+1 to do, +8 done`), and in a section of one row draws the count
alone (`+2 to do, +8 done`). Drawing both of the two and counting the eight finished rows takes
three rows.

**Why.** What somebody glancing at the panel looks for is what is still to do. A window that starts
at the top of a long plan fills with finished rows and leaves the unfinished ones behind a count
that does not say they are there. Dropping the count to draw one more row still to do would leave
the finished rows out without saying so, and the count is the one row that says rows still to do
are not drawn.

The section's last row names the key that prints the whole plan, the chord in force
([INPUT-32](terminal-input.md#INPUT-32)), as the panel's last row names the key that hides it. It
reads `alt-a whole plan` by default and is cut with an ellipsis to the width of the panel, so a plan
the panel cannot draw whole, or draws with its text cut, can be read in full. The row is drawn where
the section has three rows of room or more, the plan does not fill them exactly, and every row still
to do is drawn beside it: the plan is shorter than the room, or the room less the key row and the
row that counts what was left out holds every row still to do. So the row takes the place of no row
still to do and of no task of a plan that fits the room, and only of finished rows in a plan too
long for it. The `?` list names the key whatever the plan holds.

A row that is neither completed nor cancelled is drawn as work being done only while a turn is
running. While none is, as after the turn that wrote the list has ended and in a resumed session,
such a row is left open: its mark is hollow (`□`) where work being done has the filled one, so the
two differ under `NO_COLOR`, and its text is dim. A compaction or another working state that is
not a turn leaves it open too. Rows that are completed or cancelled are drawn as they are in any
other plan.

A click on a task moves the view to the turn that wrote it, as the keys that move the view to a
prompt do ([MOUSE-31](mouse.md#MOUSE-31), [SCROLL-3](scroller.md#SCROLL-3)), and one on the mark of a
finished task opens the whole of its words, which the transcript holds whole
([MOUSE-32](mouse.md#MOUSE-32)).

**Why.** Nothing is working on a row once the turn has ended. A filled mark beside an answer that
says the work is finished reads as work still going.

`verified-by: bravebot_tui::panel::the_plan_stays_after_the_turn_and_clear_empties_it`
`verified-by: bravebot_tui::panel::an_empty_report_after_one_with_rows_keeps_that_plan`
`verified-by: bravebot_tui::state::a_failed_or_stopped_turn_after_an_empty_report_keeps_its_plan`
`verified-by: bravebot_tui::state::a_rewind_or_an_unsent_prompt_leaves_no_plan_after_an_empty_report`
`verified-by: bravebot_tui::panel::a_long_plan_shows_the_rows_still_to_do_before_the_finished_ones`
`verified-by: bravebot_tui::panel::the_rows_shown_keep_the_order_of_the_plan`
`verified-by: bravebot_tui::panel::the_task_in_progress_is_kept_when_there_are_more_rows_still_to_do_than_room`
`verified-by: bravebot_tui::panel::room_left_over_goes_to_the_finished_rows_nearest_the_first_one_still_to_do`
`verified-by: bravebot_tui::panel::the_row_that_counts_what_was_left_out_names_each_kind_in_words_of_its_own`
`verified-by: bravebot_tui::panel::the_panel_draws_a_plan_of_none_one_twelve_and_fifty_rows_and_at_the_least_height`
`verified-by: bravebot_tui::panel::the_rows_drawn_and_counted_follow_the_status_and_never_the_text`
`verified-by: bravebot_tui::panel::the_last_row_of_the_plan_names_the_key_that_prints_it_whole`
`verified-by: bravebot_tui::panel::the_key_row_never_takes_the_place_of_a_row_still_to_do`
`verified-by: bravebot_tui::panel::the_row_naming_the_key_is_cut_to_the_width_of_a_narrow_panel`
`verified-by: bravebot_tui::panel::the_whole_plan_is_printed_with_every_task_its_status_and_none_of_its_text_cut`
`verified-by: bravebot_tui::panel::the_key_and_the_command_print_the_same_whole_plan_and_cut_none_of_it`
`verified-by: bravebot_tui::panel::the_work_of_laying_out_a_long_plan_grows_with_its_length`
`verified-by: bravebot_tui::panel::a_row_a_turn_left_open_is_drawn_apart_from_work_being_done`
`verified-by: bravebot_tui::panel::a_working_session_that_is_not_in_a_turn_draws_open_rows_as_left_open`
`verified-by: bravebot_tui::sessions::a_resumed_session_draws_the_plan_it_was_left_with_as_left_open`
`verified-by: bravebot_tui::panel::the_rows_a_section_turned_off_frees_go_to_the_plan`

<a id="PANEL-11"></a>
### PANEL-11: the panel draws only on what the person and the driver said

Every row the panel draws comes from text the person typed (the session's name, which comes from a
prompt or a `/rename` ([SESSION-4](sessions.md#SESSION-4)), a goal's condition, and the links given
with `/issue` and `/pr`, or `--issue` and `--pr`, or recorded by `set_link` for an address the person
typed ([SETLINK-1](tools/set-link.md#SETLINK-1))), the planner's own `todo_write` rows, the rows of the
Pull requests and Issues sections, which are the items [PANEL-16](#PANEL-16) collects from the lines
the person typed, from the planner's own tool-call arguments once the gate that reads them has said the
context is trusted, and from content whose label is trusted, the driver's own counters (the context
reading, the cache figures and the token total), the chord in force, the directory and branch the
session runs in, the session's id (made by the driver from the operating system's random source, so
no reply, tool result or file supplies it), the programs of the language servers it started (PANEL-13), the aliases of its MCP
servers (PANEL-14), the model the person chose and the effort level in force, or the paths of the files
the session wrote with the counts the kernel made ([FILES-2](files-mode.md#FILES-2)). No row comes from a
reply, an untrusted tool result, a file's contents, a language server's reply, an issue or
pull request body, or anything a job printed. A row of the two sections of references is built from
an owner, a repository and a number that [PANEL-16](#PANEL-16) validated, so a reply or a result that
names an item adds no row, and a row cannot hold a character the validation does not allow. The
checklist `/sidebar` opens ([PANEL-17](#PANEL-17)) has one row for each section, drawn from the
section's fixed name in the message catalog. The lines of `~/.bravebot/sidebar` are compared with the
fixed ids of the sections and are never drawn, so no text from a file, a reply or a tool result
reaches it.

The files state, and no other state of the panel, also draws the changes of a write, under
[FILES-6](files-mode.md#FILES-6) and [FILES-7](files-mode.md#FILES-7). Those rows are in a region of
their own, under a header, with a gutter and sign that no section of the panel has, and the ordinary
sections are not drawn beside them, so the reason for the rule still holds.

**Why.** The panel is the interface speaking in its own voice, like the scroller's footer
([SCROLL-5](scroller.md#SCROLL-5)). Text from any of those drawn in it would read as the interface's
own sentence, and it would be in front of the person on every frame.

No click on the panel changes this ([MOUSE-33](mouse.md#MOUSE-33)).

`verified-by: bravebot_tui::panel::nothing_a_turn_returned_reaches_the_panel`
`verified-by: bravebot_tui::panel::control_characters_in_a_link_row_are_drawn_as_pictures`
`verified-by: bravebot_tui::app::a_click_on_any_target_of_the_panel_never_draws_what_a_tool_returned`
`verified-by: bravebot_tui::panel::a_line_of_the_file_that_names_no_section_is_never_drawn`

<a id="PANEL-12"></a>
### PANEL-12: the person says which issue and pull request the session is for

`/issue <url>` and `/pr <url>` give the session its issue and its pull request. A value is one
`http` or `https` URL with a host, in printable ASCII. One with anything else in it (whitespace, a
newline, an escape, a direction override or a zero-width character), with no host, or with any
other scheme, is refused with a note that does not repeat it, and sets nothing. The bare word says what is set, and `clear` removes that link and
not the other. Each change is written to the session record at once, as a rename is
([SESSION-3](sessions.md#SESSION-3)), and gives up no rewind point, since no point holds a link.
Typed while a turn runs, they are carried out as `/rename` is ([CMD-8](commands.md#CMD-8)).

`--issue <url>` and `--pr <url>` give the session a command line opens the same links, under the
same check, and so does only that session: not one `/resume` switches to. They are taken out of the
line wherever they stand, each of a kind may be given twice and the last is used, and a value that
fails the check, or none, stops the line before the session opens with [the status for an argument](cli.md),
in a sentence that does not repeat it. A link the flag gives replaces the one a resumed
record held and is written to the record as `/issue` is; a link it does not give is left as it was.
A line that opens no interactive session (a one-shot run, `--plain`, `--bg`, or a command that
starts none) is refused with them rather than dropping them.

The planner's [`set_link`](tools/set-link.md) gives the session a link under the same check, and
only for an address that is a whole word of a line the person typed ([SETLINK-1](tools/set-link.md#SETLINK-1)). It
replaces the link of that kind and is written to the record as `/issue` is.

`verified-by: bravebot_agent::turn::an_address_the_person_typed_reaches_set_link_and_leaves_the_turn`

The Pull requests section starts with the session's own pull request link and the Issues section
with its own issue link, each only while that link is set, and the items the session met follow
([PANEL-16](#PANEL-16)). A row has no label: the heading says what the rows are. A link that is a
GitHub pull request or issue address of the shape [PANEL-9](#PANEL-9) gives, whose path names the
kind of its section, is drawn as `{owner}/{repo}#{n}`, cut from the left with an ellipsis where it is
longer than the row, since the number at its end is what tells two apart. The item it names is not
drawn a second time among the items met. The cells drawn are a terminal link (OSC 8) to the address,
and a selection over them copies the shorthand. An address of any other shape, including one with a
path after the number, one of another kind than its section, and one on another host, is drawn as
written, cut in the same way, and is not a link. A new link reaches an open panel on the next frame.
A click on a row opens the address that was set in the person's browser, as `/open` followed by that
address does ([MOUSE-41](mouse.md#MOUSE-41), [CMD-30](commands.md#CMD-30)), and on a row for an item
the session met opens the address built from the item's validated parts.

`verified-by: bravebot_tui::app::the_issue_and_pr_commands_show_set_and_clear_their_own_link`
`verified-by: bravebot_tui::app::a_link_with_a_newline_an_escape_or_another_scheme_sets_nothing`
`verified-by: bravebot_tui::app::a_link_set_mid_turn_is_set_as_it_is_typed`
`verified-by: bravebot_session::sessions::setting_a_link_neither_makes_nor_gives_up_a_rewind_point`
`verified-by: bravebot_tui::app::links_from_the_command_line_replace_what_it_names_and_leave_the_other`
`verified-by: bravebot_tui::app::the_command_line_links_and_prompt_reach_the_session_once_its_record_is_open`
`verified-by: bravebot_cli::main::issue_and_pr_flags_are_taken_out_and_set_their_own_link`
`verified-by: bravebot_cli::main::a_value_that_is_not_one_web_address_is_refused_and_not_repeated`
`verified-by: bravebot_cli::main::only_a_line_that_opens_a_session_takes_links`
`verified-by: bravebot_session::sessions::only_one_web_address_on_one_line_is_a_link`
`verified-by: bravebot_tui::panel::a_link_the_person_gave_is_the_first_row_of_the_section_of_its_kind`
`verified-by: bravebot_tui::panel::what_the_session_met_follows_its_own_link_in_the_section_of_its_kind`
`verified-by: bravebot_tui::panel::an_own_link_with_a_path_after_its_number_is_drawn_as_written_and_not_repeated`
`verified-by: bravebot_tui::panel::a_row_for_what_the_session_met_links_to_the_address_its_reference_builds`
`verified-by: bravebot_tui::panel::a_long_link_keeps_its_end`
`verified-by: bravebot_tui::panel::a_github_address_in_a_link_row_is_drawn_as_its_shorthand`
`verified-by: bravebot_tui::panel::a_link_row_of_any_other_shape_is_drawn_as_written_and_not_linked`
`verified-by: bravebot_tui::panel::a_link_row_shorthand_links_to_its_own_address_and_nothing_else_does`
`verified-by: bravebot_tui::panel::a_long_shorthand_keeps_its_end_and_is_linked_as_drawn`
`verified-by: bravebot_tui::panel::a_link_row_below_the_panels_height_is_not_linked`
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

A click on a row says which language servers the session has started, as `/lsp` does
([MOUSE-22](mouse.md#MOUSE-22), [CMD-29](commands.md#CMD-29)).

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

A server whose tool list the person answered no to ([SERVERS-8](mcp-servers.md#SERVERS-8)) is not
listed, because the model is offered nothing from it for the rest of the session, and the section
goes with the last of them. A server whose list nobody has been asked about yet is listed, since the
list is put to the person before the next turn plans. `/status` and `/mcp` still name a server that
was declined, with how its tools stand ([SERVERS-14](mcp-servers.md#SERVERS-14),
[SERVERS-19](mcp-servers.md#SERVERS-19)).

`verified-by: bravebot_tui::panel::an_mcp_alias_is_drawn_pictured_and_cut_from_the_left`
`verified-by: bravebot_tui::panel::a_server_whose_tools_were_declined_is_not_listed_and_the_section_goes_with_the_last`
`verified-by: bravebot_tui::state::only_the_servers_whose_tools_were_declined_are_left_out_of_the_offered_ones`
`verified-by: bravebot_tui::panel::the_sections_come_in_order_and_an_empty_one_leaves_no_heading`

<a id="PANEL-15"></a>
### PANEL-15: the title says what the session is doing

A marker stands in front of the title, so a tab bar that cuts the title still shows it, and counts
toward the 60 columns of [PANEL-2](#PANEL-2). While a turn or a command is running the marker is
`✦`. While an approval, a question or a confirmation is open and the run is waiting for the person
it is `[!]`. With neither, as at the box, there is no marker and the title is as PANEL-1 gives it.
The marker is written before the session has a name, since the first turn is when a tab most needs
to say it is waiting. It is written on a change of state and not again while the state stays the
same, with no animation, so the title is not rewritten every frame. Which marker applies is the
driver's own state, a turn running or a prompt open, and not anything read from what the turn
produced ([PANEL-11](#PANEL-11)). [PANEL-4](#PANEL-4) holds: no marker is written where no title is.

`verified-by: bravebot_tui::title::a_change_of_state_rewrites_the_title_and_a_repeated_state_does_not`
`verified-by: bravebot_tui::title::a_state_is_written_before_the_session_has_a_name`
`verified-by: bravebot_tui::title::the_marker_leads_the_title_and_counts_toward_sixty_columns`
`verified-by: bravebot_tui::title::no_state_is_written_when_the_title_is_turned_off`
`verified-by: bravebot_tui::app::a_question_turns_the_title_to_waiting_and_other_messages_do_not`
`verified-by: bravebot_tui::app::every_approval_a_manifest_step_draws_turns_the_title_to_waiting`
`verified-by: bravebot_tui::app::a_manifest_step_marks_the_title_waiting_while_it_asks_and_working_after`

<a id="PANEL-16"></a>
### PANEL-16: the issues and pull requests a session names are collected from what the person and the planner wrote

The Pull requests and Issues sections ([PANEL-9](#PANEL-9)) and the lists `/issues` and `/prs` print
([CMD-31](commands.md#CMD-31)) hold the items the session has met. An item is an owner, a repository
and a number, with the kind issue or pull request, and it is built in one place and only from an owner
of up to 39 letters, digits and hyphens, a repository of up to 100 letters, digits, hyphens,
underscores and dots (not `.` or `..`) and a number of one to nine digits with no leading zero. The
shorthand `{owner}/{repo}#{n}` and the address a click opens are written from those parts alone, so
nothing else a string held reaches a row. Two sightings are one item where the owner (without regard to
case), the repository and the number agree, and the list keeps the order they were first met and the
first spelling.

One reader takes items out of a string. It calls no model, no network and no `gh`, and touches no
file. It finds a full `https://github.com/{owner}/{repo}/issues/{n}` or `/pull/{n}` address, which
names its kind, and a word `{owner}/{repo}#{n}`, which names none and is listed as an issue until a
later sighting names it. In a command it also finds the number a `gh pr` or `gh issue` subcommand works
on (`view`, `comment`, `checkout`, `diff`, `checks`, `merge`, `review`, `close`, `reopen`, `edit`,
`ready`, `develop`, `lock`, `unlock`, `pin`, `unpin`, `transfer`). That number is the first word after
the subcommand, or the first one after only `-R {repo}`, `--repo {repo}`, `--repo={repo}` or `-R{repo}`,
so the `30` in `gh pr checks --interval 30` is not an item, as no number is read once any other word
comes first. The reader also finds the number after `Closes`,
`Fixes` or `Resolves` and their other forms, as `#n` or `{owner}/{repo}#n`. A number alone takes its
repository from `--repo`, `--repo=`, `-R` or `GH_REPO=` in the same segment of the command, a segment
ending at a newline, `;`, `|` or `&`, and otherwise from the repository of the session's own pull
request link, or failing that its issue link, when that is a GitHub address. A repository that is on
another host or is not well formed names nothing, and a number with no repository names nothing. A bare
`#n` in prose, a number in `gh pr list` or `gh api`, and a title such as `Add x (#12)` name nothing.
A string is read up to its first 64 KiB, cut at the end of a word, so a word the cut falls inside is
dropped and never shortened to a different number, and it gives at most 64 items.

There are three sources, and only these.

1. **Lines the person typed.** Every line the box sends is read, whether a prompt, a queued line, a
   command or a shell line, and so are the addresses `--issue` and `--pr` gave when the session opened.
   What the person wrote is theirs, as [SETLINK-1](tools/set-link.md#SETLINK-1) holds.
2. **The planner's tool-call arguments.** The `command` of `run` and the address of `fetch_url` and
   `download_url` are read where the call is made, after the same gate that lets the driver read an
   argument at all ([LABEL-5](labels.md#LABEL-5)), and only then. A context that has met something
   untrusted is refused that read, so it collects nothing from the planner's arguments, and the call
   fails as any call whose argument the gate refuses. The reader has no gate of its own and
   declassifies nothing. The call is read before the
   credential, directory, rules and approval checks, so a command one of them then refuses still names
   what it was about. Only a turn whose front end keeps links (the terminal and the desktop, as
   [SETLINK-2](tools/set-link.md#SETLINK-2) says) collects. A delegate, a one-shot run and a plain run
   collect nothing.
3. **Content whose label is trusted.** A tool result is read only after its label is asked, and only
   where that says trusted ([LABEL-5](labels.md#LABEL-5)), as a read of a file the person vouched for
   is. One such read is one `allow` event in the trail. A result that is untrusted, such as what a
   program printed, is not read and adds no event, so an address in `gh pr view` output never
   reaches the panel.

No other text is read: not an assistant reply, an untrusted tool result, a file that was not vouched
for, an issue body or a language server's reply. What the reader returns is appended to a list and
nothing else. It decides no permission, route, prompt, refusal or sentence to the planner, and no code
in the driver branches on an item beyond the kind the validating constructor set, which picks the
section. The items travel to the terminal as the turn finds them, one message for each call that named
something new, so `/issues` and the panel are current while a turn runs and a failed or stopped turn
has handed back what it found. A turn keeps at most 200 and the session at most 500, the first met,
and the items of the session are written with each turn's save, kept by a resume and a fork and dropped
by `/clear` ([SESSION-3](sessions.md#SESSION-3)).

**Why.** The panel and the lists are the interface speaking in its own voice, so a row is made only
from what a person or the planner's own context wrote, never from what a page, a reply or a program
supplied ([PANEL-11](#PANEL-11)). The reader returns a value that only the screen uses, so what it
reads cannot decide anything.

`verified-by: bravebot_agent::links::a_reference_is_built_only_from_what_github_allows_in_a_name_and_a_number`
`verified-by: bravebot_agent::links::a_reference_writes_its_shorthand_and_the_address_of_its_kind`
`verified-by: bravebot_agent::links::two_references_are_the_same_item_by_owner_repository_and_number`
`verified-by: bravebot_agent::links::a_full_address_gives_the_kind_its_path_names_and_what_follows_the_number_is_ignored`
`verified-by: bravebot_agent::links::an_owner_repository_number_word_is_read_as_an_issue_whose_kind_was_not_named`
`verified-by: bravebot_agent::links::a_word_that_is_not_a_reference_gives_nothing`
`verified-by: bravebot_agent::links::what_a_sentence_puts_around_a_reference_is_taken_off`
`verified-by: bravebot_agent::links::only_the_first_part_of_a_long_string_is_read_and_the_cut_falls_on_a_character`
`verified-by: bravebot_agent::links::a_word_the_cut_falls_inside_is_dropped_and_not_shortened`
`verified-by: bravebot_agent::links::one_string_adds_at_most_sixty_four_references_and_the_first_are_kept`
`verified-by: bravebot_agent::links::a_list_holds_each_item_once_and_a_named_kind_settles_an_assumed_one_only`
`verified-by: bravebot_agent::links::a_full_list_keeps_what_it_holds_and_drops_the_rest`
`verified-by: bravebot_agent::links::the_item_a_gh_command_works_on_is_read_with_the_kind_of_its_noun`
`verified-by: bravebot_agent::links::a_number_is_read_only_first_or_after_a_repository_flag`
`verified-by: bravebot_agent::links::a_bare_number_takes_the_commands_repository_then_the_default_and_otherwise_nothing`
`verified-by: bravebot_agent::links::a_repository_that_is_not_one_on_github_is_not_replaced_by_the_default`
`verified-by: bravebot_agent::links::a_closing_keyword_names_an_issue_and_a_number_with_no_keyword_names_nothing`
`verified-by: bravebot_agent::links::an_address_in_any_argument_of_a_command_is_read`
`verified-by: bravebot_agent::links::a_command_that_names_an_item_in_two_forms_lists_it_once_with_the_named_kind`
`verified-by: bravebot_agent::links::a_line_that_is_not_valid_shell_is_still_read_as_words`
`verified-by: bravebot_agent::tools::tests::references::a_command_the_planner_wrote_names_the_items_it_works_on_even_where_the_call_then_fails`
`verified-by: bravebot_agent::tools::tests::references::a_bare_number_in_a_command_belongs_to_the_sessions_repository_and_not_to_a_guess`
`verified-by: bravebot_agent::tools::tests::references::an_address_the_planner_fetches_or_downloads_names_its_item`
`verified-by: bravebot_agent::tools::tests::references::a_context_that_has_met_something_untrusted_collects_nothing_from_the_planners_arguments`
`verified-by: bravebot_agent::tools::tests::references::a_turn_that_keeps_no_links_collects_nothing`
`verified-by: bravebot_agent::tools::tests::references::what_a_call_names_leaves_dispatch_in_its_output`
`verified-by: bravebot_agent::references::a_turn_that_fails_after_a_call_has_still_reported_what_the_call_named`
`verified-by: bravebot_agent::references::an_item_is_reported_once_and_again_only_when_a_later_call_settles_its_kind`
`verified-by: bravebot_agent::references::a_bare_number_in_a_planners_command_takes_the_repository_the_task_names`
`verified-by: bravebot_agent::references::a_turn_whose_front_end_keeps_no_links_reports_nothing`
`verified-by: bravebot_agent::references::what_a_vouched_file_names_is_listed`
`verified-by: bravebot_agent::references::what_a_program_printed_is_not_listed_and_adds_nothing_to_the_trail`
`verified-by: bravebot_tui::state::references::every_line_the_box_sends_is_read_for_what_it_names`
`verified-by: bravebot_tui::state::references::an_item_met_again_keeps_its_place_and_a_named_kind_settles_a_guess`
`verified-by: bravebot_tui::state::references::the_session_keeps_the_first_five_hundred_it_met_in_order`
`verified-by: bravebot_tui::state::references::the_list_of_a_kind_starts_with_the_own_link_and_leaves_out_what_is_not_one`
`verified-by: bravebot_tui::app::the_addresses_the_command_line_gave_are_in_the_list_when_the_session_opens`
`verified-by: bravebot_tui::from_worker::the_references_a_turn_reports_are_taken_by_both_loops_and_never_handed_back`
`verified-by: bravebot_tui::remote_confirm::references_a_turn_found_travel_when_found_without_an_answer`
`verified-by: bravebot_tui::sessions::a_turn_that_fails_after_naming_an_item_has_already_given_the_session_the_item`
`verified-by: bravebot_tui::panel::what_the_session_met_follows_its_own_link_in_the_section_of_its_kind`
`verified-by: bravebot_tui::panel::a_row_for_what_the_session_met_links_to_the_address_its_reference_builds`
`verified-by: bravebot_tui::panel::the_sections_of_references_are_cut_to_the_room_with_a_count_of_the_rest`
`verified-by: bravebot_tui::panel::the_references_do_not_move_where_the_context_section_fits`

<a id="PANEL-17"></a>
### PANEL-17: `/sidebar` turns each section of the panel on or off, and the choice is kept

`/sidebar` opens a bordered checklist over the transcript, titled `info panel`, with one row for each
section of the panel in the order [PANEL-9](#PANEL-9) gives: Session, Model, Goal, Pull requests,
Issues, Context, Language servers, MCP servers, Files and Plan. A row reads `[x] Name` while its
section is drawn and `[ ] Name` while it is turned off, and it is there whether or not the section
has anything to show. Up and Down, and `j` and `k`, walk the list and stop at both ends. Space or
Enter turns the row under the cursor on or off, and the cursor stays on it. A left press on a row
puts the cursor on it and turns it on that one press, where the other pickers take a row on a second
press ([MOUSE-15](mouse.md#MOUSE-15)), because a press here changes one box and closes nothing. The
wheel moves the cursor, a press outside the panel closes it ([MOUSE-47](mouse.md#MOUSE-47)), and a
press on the border, on the lines under the list or below the last row changes nothing. Escape and
Ctrl-C close the checklist and keep every change already made, so there is no cancel that undoes
them. Nothing is said in the transcript.

The panel follows each change on the next frame, with no other key, at rest and while a turn runs.
A section turned off is left out with its heading and rows ([PANEL-9](#PANEL-9)), has no click
target, and the rows it frees go to the sections that remain, the Plan taking what is left
([PANEL-10](#PANEL-10)). A section turned back on that has nothing to show is still left out. With
every section turned off the panel is still drawn, with its last row, `{chord} hide panel`
([PANEL-7](#PANEL-7)), as the only row. That row is still a click target, and the checklist never
closes the panel. The files state ([FILES-4](files-mode.md#FILES-4)) does not read the choice.
Turning Context off leaves the context reading and the cache hit rate on the hint line, as they are
where the panel is not drawn ([PANEL-8](#PANEL-8)).

`/sidebar` is the bare word ([CMD-32](commands.md#CMD-32)). It does not open the panel and does not
write `panel`, so with the panel closed a change shows when the panel is next opened.

The choice is written to `~/.bravebot/sidebar` on each change, as the panel's state is
([PANEL-5](#PANEL-5)), so a crash or Ctrl-C while the checklist is open loses nothing. The file holds
one section id on each line for each section turned off: `session`, `model`, `goal`,
`pull-requests`, `issues`, `context`, `language-servers`, `mcp-servers`, `files` and `plan`. The
ids are fixed words and are not the translated headings. The file is written empty when every
section is on again. It is read once, at the start. No file, an empty file, one that cannot be read,
one that is not a regular file (a pipe is not opened), one longer than 1024 bytes (the file the
program writes is about 150) and one with a line that is not a section id each leave every section on, and a section the file
does not name is on, so a section a later version adds is on with no migration. The ids are compared
with the fixed list and never drawn ([PANEL-11](#PANEL-11)). The choice is the person's and is the
same in every directory and session. It is not in the session record.

**Why.** A person who watches many sessions needs some of the sections and not others, and each
section drawn takes its heading, a blank row and its rows out of the room the other sections and the
Plan share. The file names the sections turned off so that an absent file means every section on,
which is how the panel has always drawn, and a section added later is on with no migration.

`verified-by: bravebot_tui::sidebar_prompt::a_row_for_each_section_in_the_panels_order_checked_while_it_is_drawn`
`verified-by: bravebot_tui::sidebar_prompt::space_and_enter_turn_the_row_under_the_cursor_off_and_on_and_the_cursor_stays`
`verified-by: bravebot_tui::sidebar_prompt::a_change_reports_every_section_that_is_off_and_not_only_the_last`
`verified-by: bravebot_tui::sidebar_prompt::the_arrows_and_j_and_k_walk_the_list_and_stop_at_both_ends`
`verified-by: bravebot_tui::sidebar_prompt::escape_and_ctrl_c_close_the_list_and_keep_what_was_changed`
`verified-by: bravebot_tui::sidebar_prompt::a_space_with_a_modifier_toggles_nothing`
`verified-by: bravebot_tui::sidebar_prompt::one_press_on_a_row_turns_it_off_and_the_next_turns_it_on_again`
`verified-by: bravebot_tui::sidebar_prompt::a_press_turns_off_the_row_it_landed_on_and_not_the_one_under_the_cursor`
`verified-by: bravebot_tui::sidebar_prompt::the_wheel_walks_a_press_outside_closes_and_a_press_off_the_rows_changes_nothing`
`verified-by: bravebot_tui::sidebar_prompt::the_panel_draws_the_boxes_the_title_and_the_keys`
`verified-by: bravebot_tui::list_picker::a_picker_that_takes_on_the_first_press_does_and_the_others_still_need_two`
`verified-by: bravebot_tui::app::tests::pickers::the_bare_word_sidebar_opens_the_checklist_and_nothing_else_happens`
`verified-by: bravebot_tui::app::tests::pickers::the_panel_follows_each_toggle_at_once_and_escape_keeps_them_at_rest_and_while_a_turn_runs`
`verified-by: bravebot_tui::app::tests::pickers::a_press_on_a_row_of_the_checklist_turns_its_section_off_and_a_press_outside_keeps_that`
`verified-by: bravebot_tui::app::a_section_turned_off_mid_turn_is_saved_at_once_and_the_turn_is_not_touched`
`verified-by: bravebot_tui::panel::each_section_has_a_fixed_id_and_a_bit_of_its_own`
`verified-by: bravebot_tui::panel::a_section_turned_off_is_not_drawn_and_no_other_section_goes_with_it`
`verified-by: bravebot_tui::panel::a_section_turned_off_has_no_click_target`
`verified-by: bravebot_tui::panel::the_rows_a_section_turned_off_frees_go_to_the_plan`
`verified-by: bravebot_tui::panel::a_name_that_is_not_a_section_leaves_every_section_on_and_a_section_not_named_is_on`
`verified-by: bravebot_tui::panel::a_line_of_the_file_that_names_no_section_is_never_drawn`
`verified-by: bravebot_tui::panel::with_every_section_off_the_panel_is_drawn_with_only_its_hide_row_and_stays_open`
`verified-by: bravebot_tui::panel::the_files_state_is_drawn_the_same_with_every_section_off`
`verified-by: bravebot_tui::panel::the_hint_line_offers_the_panel_the_same_with_every_section_off`
`verified-by: bravebot_tui::panel::the_context_reading_and_the_hit_rate_stay_on_the_hint_line_while_the_context_section_is_off`
`verified-by: bravebot_tui::persist::the_sections_turned_off_are_read_back_next_session_and_turning_them_on_replaces_them`
`verified-by: bravebot_tui::persist::a_sidebar_file_that_does_not_read_cleanly_leaves_every_section_on`
`verified-by: bravebot_session::store::the_sidebar_file_is_its_trimmed_nonblank_lines_and_nothing_else`
`verified-by: bravebot_tui::state_directory::a_recorded_choice_is_readable_only_by_its_owner`

## Known costs

- **A terminal that is not asked shows no resize pointer.** Over the border it draws its own pointer,
  so a person there finds that the border drags by trying it. iTerm2 and xterm read the sequence
  ([PANEL-6](#PANEL-6)) but take other names for the shape, a subset of X11's that iTerm2 does not
  list and X11's own for xterm, and each is asked once the name it takes for this shape is confirmed
  in that terminal. VTE-based terminals and Alacritty do not read the sequence. A terminal not asked
  still sends the position of the pointer with no button held ([INPUT-33](terminal-input.md#INPUT-33)),
  which is read as the last of a burst ([INPUT-49](terminal-input.md#INPUT-49)) and draws a frame only
  when the target under the pointer changes.

- **A prompt or a picker opened under the pointer leaves the resize pointer in place.** The prompts
  and pickers read their own events and none of them reads the pointer, so a pointer on the border
  when one opens keeps its shape until the prompt closes and the pointer next moves.

- **An issue the agent creates appears only when a later argument or typed line names its number
  ([PANEL-16](#PANEL-16)).** The address `gh issue create` prints is a program's output and is never
  read, so the list does not hold it until something the person or the planner wrote names it.

- **A word shaped like `owner/repo#n` is read as a reference ([PANEL-16](#PANEL-16)).** A path with a
  number after a hash, such as `src/main.rs#12`, has a valid owner and repository by GitHub's rules, so
  it is listed as an issue. The list is for the person to look at and nothing acts on it.

- **A command a rule then refuses still counts ([PANEL-16](#PANEL-16)).** The command is read where
  the planner's argument is read, before the checks that refuse it, so the items it names are listed
  though it did not run.

- **The reader is bounded by count, and what it drops is not shown ([PANEL-16](#PANEL-16)).** It
  reads 64 KiB of a string and keeps 64 items of it, a turn keeps 200 and a session 500, the first
  met. A log that names more leaves the rest out with no note, and a record keeps what the list held
  at the last save, so a session that ends without one loses what it named since.

- **The desktop collects the list and draws none of it ([PANEL-16](#PANEL-16)).** The desktop bridge
  keeps links so that a turn collects, and the window draws no such list, so each turn there reads the
  planner's `run` and fetch arguments and the trusted results for a list nobody sees. The trail of a
  desktop turn gains one `allow` event for each trusted result read, as the terminal's does.

- **A pull request met as `owner/repo#n` is listed under Issues until something names it
  ([PANEL-16](#PANEL-16)).** An address with `/pull/` or a `gh pr` command settles its kind.

- **An own link that is not on GitHub is drawn as written and is not a reference
  ([PANEL-12](#PANEL-12)).** It is not in `/issues` or `/prs` either.

- **Assistant reply text is never read ([PANEL-16](#PANEL-16)).** An item the model names only in a
  sentence is not listed. Reading it would need a gate of its own and a reversal of
  [PANEL-11](#PANEL-11).

- **A context that has met something untrusted collects nothing from the planner's arguments
  ([PANEL-16](#PANEL-16)).** The read goes through the same gate as every argument
  ([LABEL-5](labels.md#LABEL-5)), so the sections stop growing from the planner's commands once a page
  or an untrusted file has been read, though what the person types still counts.

- **At exactly 100 columns the checklist covers 7 of the panel's left columns ([PANEL-17](#PANEL-17)).**
  The checklist is 42 columns wide and centred in the frame, and the panel needs 100 columns to be
  drawn at all. On a wider terminal it does not touch the panel.

- **The choice of sections is global, not per session ([PANEL-17](#PANEL-17)).** `~/.bravebot/sidebar`
  is one file for every directory and session, and a resume draws the sections as the last change
  left them.

- **A section the file does not name is on, and one wrong line turns every section on
  ([PANEL-17](#PANEL-17)).** A file edited by hand with a misspelt id is read as if it were absent,
  not as if the line were skipped.

- **Turning a section off while the panel is closed shows nothing ([PANEL-17](#PANEL-17)).** `/sidebar`
  does not open the panel, so the change is first seen the next time it is opened.
