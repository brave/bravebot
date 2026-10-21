---
id: FILES
title: The files the session wrote
status: normative
governs:
  - crates/tui/src/files_mode.rs
  - crates/tui/src/panel.rs
documented-by: docs/website/docs/using/sessions.md
---

## Scope

A state of the info panel ([info-panel.md](info-panel.md)) that lists every file the session wrote
and shows every edit to the highlighted one, so reviewing what a session changed does not mean
finding each write in the transcript. The transcript draws each write's changes under its call, the
first twelve rows of them ([SCROLL-10](scroller.md#SCROLL-10)), and `/hunks` lists the hunks the
last turn wrote so that one can be put back ([SESSION-37](sessions.md#SESSION-37)). Neither lists
the files of the whole session.

Everything the state draws is held by the session already, in the activity entries of the
transcript. Nothing is read from disk, run, or sent to a model. The state is for the terminal and for
what the session's own write calls changed. The desktop window, a source that shows the working tree
against `HEAD`, and putting an edit back from the state are not built, and each is a Known cost
below.

## Clauses

<a id="FILES-1"></a>
### FILES-1: a file is listed once a write call has changed it

A file is listed when a finished `write_file` call, a finished `edit_file` call, or a write step of a
manifest changed it. That call is an edit, and the edit is the unit the mode counts and draws. An
`edit_file` call that takes several passages for one file is one edit. A call that was refused,
failed or has not finished is not an edit, and a file that only such calls named is not listed. A
file changed by a program `run` started, or by the person, is not listed, because no write call
produced changes to draw. A rewind does not account for those changes either
([SESSION-19](sessions.md#SESSION-19)). Rows are in the order the files were first written, and a
file written again keeps its row.

The driver marks the call, as it marks one that failed: the finished call carries the counts of
FILES-2 only when the write changed a file, and the state lists the calls that carry them. A
`write_file` call or write step whose body leaves every line of an existing file as it was changed
nothing and carries none, and a call that created a file always carries them, whatever the file
holds. Nothing is decided from the call's text.

The call also carries the file it landed on, as the workspace resolves the name. Rows, and the file
count in the Files section, are keyed on that and not on the text of the call row, so two files that
the row draws under one name stay two rows. Names the workspace resolves to one file (`a.rs` and
`./a.rs`) are one row, drawn under the name of the first call
that wrote it. The resolved name is a key only and is never drawn.

`verified-by: bravebot_tui::files_mode::a_file_is_listed_once_a_write_call_has_changed_it`
`verified-by: bravebot_tui::files_mode::a_refused_failed_or_running_write_lists_nothing`
`verified-by: bravebot_tui::files_mode::a_second_write_keeps_the_files_row_and_adds_an_edit`
`verified-by: bravebot_tui::files_mode::several_passages_of_one_edit_call_are_one_edit`
`verified-by: bravebot_tui::files_mode::rows_are_keyed_on_the_file_a_call_landed_on`
`verified-by: bravebot_agent::turn::a_write_file_call_carries_its_counts_and_the_file_it_landed_on`
`verified-by: bravebot_agent::turn::a_write_file_call_that_created_a_file_carries_its_counts`
`verified-by: bravebot_agent::turn::an_edit_whose_lines_were_held_back_still_carries_its_counts`
`verified-by: bravebot_agent::manifest::a_write_step_carries_its_counts_and_the_file_it_landed_on`

<a id="FILES-2"></a>
### FILES-2: counts and line numbers are made with the diff

The kernel makes, with each diff, the number of lines it added and removed and a pair of line numbers
(before and after the edit) for each row of `changes`. The activity carries them to the terminal in
lists and numbers beside `changes`. `Change` and the JSON the desktop window reads for it are
unchanged. The totals in the list heading and in the Files section are sums of the per-edit counts.
No code in the terminal counts `Added` or `Removed` rows, or counts rows to find a line number,
because the rows are content released for a screen and counting them is a read of it
([LABEL-6](labels.md#LABEL-6)). `Diff::added` and `Diff::removed` already hold the counts inside the
kernel.

`verified-by: bravebot_agent::turn::what_a_write_added_and_removed_is_counted_inside_the_kernel_before_it_is_released`
`verified-by: bravebot_agent::diff::each_row_of_a_diff_has_the_line_number_it_has_on_each_side`
`verified-by: bravebot_tui::files_mode::the_totals_are_sums_of_the_counts_the_edits_carry`

<a id="FILES-3"></a>
### FILES-3: `/files` enters the state and leaves it

`/files` draws the panel in its files state and opens the panel if it was closed. While the state is
open, `/files`, `q` and Escape end it. Ending it returns the panel to what it was before: closed if
it was closed, in its ordinary state if it was open. Entering and leaving write nothing to
`~/.bravebot/panel` ([PANEL-5](info-panel.md#PANEL-5)). The `panel` chord hides the panel from either
state and ends the state with it. The state is not kept across a start or a resume.

With no file written, `/files` answers with a note and changes nothing. On a terminal too narrow for
the state (FILES-4) it answers with a note saying how wide the terminal has to be, as
[PANEL-6](info-panel.md#PANEL-6) does. It runs at once while a turn works, as `/panel` does
([CMD-8](commands.md#CMD-8)).

`verified-by: bravebot_tui::app::files_opens_the_panel_in_its_files_state_and_q_puts_it_back`
`verified-by: bravebot_tui::app::leaving_the_state_closes_a_panel_it_opened`
`verified-by: bravebot_tui::app::entering_and_leaving_the_state_writes_no_panel_choice`
`verified-by: bravebot_tui::app::the_panel_chord_ends_the_state_with_the_panel`
`verified-by: bravebot_tui::app::files_with_nothing_written_answers_with_a_note`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`

<a id="FILES-4"></a>
### FILES-4: the state has its own width and draws none of the ordinary sections

The panel in the state is half the terminal's columns, at least 60 and at most 100, with its left
edge a border as in [PANEL-6](info-panel.md#PANEL-6). Dragging the border does nothing in the state, and the width the panel
was dragged to is kept for the ordinary state. It is drawn on a terminal of at least 100 columns,
which leaves the transcript at least 40. None of the sections of [PANEL-9](info-panel.md#PANEL-9) is
drawn while it is. Top to bottom it holds a heading row, the list (FILES-5), a rule, the highlighted
file's name and edit count, and its edits (FILES-6). The context reading and cache rate move to the
hint line, as they do when the panel is not drawn ([PANEL-8](info-panel.md#PANEL-8)). A terminal
narrowed below 100 columns while the state is open stops drawing it and draws it again when widened.

The heading row is `{n} files changed` with `+{added}` and `-{removed}` beside it, from the sums of
FILES-2.

`verified-by: bravebot_tui::files_mode::the_state_is_half_the_columns_between_sixty_and_a_hundred`
`verified-by: bravebot_tui::files_mode::the_state_is_not_drawn_below_a_hundred_columns_and_comes_back_when_widened`
`verified-by: bravebot_tui::files_mode::dragging_the_border_does_nothing_in_the_state_and_keeps_the_dragged_width`
`verified-by: bravebot_tui::files_mode::no_ordinary_section_is_drawn_in_the_state`
`verified-by: bravebot_tui::files_mode::the_hint_line_has_the_context_reading_while_the_state_is_open`

<a id="FILES-5"></a>
### FILES-5: the list has one row for each file

A row is the path as the call row draws it, then the file's added and removed counts at the right
edge, `+{a}` in the colour of an added row and `-{r}` in that of a removed row. A count of nothing is
left out, so a file that only had lines removed shows `-{r}` alone. A path longer than its room is cut
from the left with an ellipsis, since the end of a path is what tells two apart. Control characters
in a path are drawn as visible characters, as they are wherever the interface draws text.

The state opens with the last file written highlighted. Up, Down, `k` and `j` move the highlight and
stop at each end. Home and End, or `g` and `G`, go to the first and last. Where there are more files
than rows, the rows drawn are the ones around the highlight, and the first and last rows say how many
are left out above and below (`↑ 1 more above`, `↓ 8 more below`).

`verified-by: bravebot_tui::files_mode::a_row_has_the_path_and_the_counts_of_its_edits`
`verified-by: bravebot_tui::files_mode::a_long_path_keeps_its_end`
`verified-by: bravebot_tui::files_mode::a_control_character_in_a_path_is_drawn_as_a_picture`
`verified-by: bravebot_tui::files_mode::the_state_opens_on_the_last_file_written`
`verified-by: bravebot_tui::files_mode::the_highlight_stops_at_each_end`
`verified-by: bravebot_tui::files_mode::a_long_list_keeps_the_highlight_on_screen_and_counts_each_side`

<a id="FILES-6"></a>
### FILES-6: the edits of the highlighted file are drawn in order

Below the list are all the edits to the highlighted file, oldest first, and they change as the
highlight does. Each edit has a header row, `edit {i} of {n}`, the turn it was made in, and its
counts. Under it are the edit's changes as [the transcript draws a
write](terminal-transcript.md): `+` rows, `-` rows and unchanged rows, with each run of unchanged
lines left out shown as `{n} unchanged lines`. Every change is drawn, and not the first twelve. A
gutter holds the line numbers of FILES-2: the number after the edit on a kept or added row, and the
number before it on a removed row. A line longer than the panel wraps to its width, and a
continuation row has a blank gutter and sign.

The edits open at their top. PageUp and PageDown scroll them. `[` and `]` move to the previous and
next edit header.

`verified-by: bravebot_tui::files_mode::the_edits_of_a_file_are_drawn_oldest_first_each_under_its_header`
`verified-by: bravebot_tui::files_mode::every_change_of_an_edit_is_drawn_and_not_twelve`
`verified-by: bravebot_tui::files_mode::the_gutter_holds_the_numbers_the_kernel_made`
`verified-by: bravebot_tui::files_mode::a_long_line_wraps_to_the_panel_and_its_continuation_has_a_blank_gutter`
`verified-by: bravebot_tui::files_mode::the_bracket_keys_move_between_edit_headers`

<a id="FILES-7"></a>
### FILES-7: each edit keeps its own marking, and no combined change is drawn

An edit the activity marks untrusted (the file it replaced was not vouched for) is drawn in the
marked block: the bar on every drawn row, control characters replaced by visible characters, and each
line broken to the width by the step that draws the margin
([VIEW-3](terminal-transcript.md#VIEW-3)). An edit made over a file the map vouched for is drawn
without the bar. The edits to one file can differ in this, so the marking belongs to the edit and the
file has none. The header rows, the list and the heading carry no file content and are the
interface's own words.

No change from the file's first "before" to its last "after" is drawn. That would compare two
contents that need not share a label, which only `Policy::render_pair_in_place` may do
([reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md)).

`verified-by: bravebot_tui::files_mode::an_untrusted_edit_is_marked_on_every_row_it_reaches`
`verified-by: bravebot_tui::files_mode::a_file_with_a_trusted_and_an_untrusted_edit_marks_only_the_second`
`verified-by: bravebot_tui::files_mode::an_edit_cannot_paint_a_bar_in_the_margin_column`
`verified-by: bravebot_tui::files_mode::the_state_draws_no_net_change_of_a_file`

<a id="FILES-8"></a>
### FILES-8: what arrives moves nothing

An edit that finishes while the state is open is added at the end of its file's edits, or as a new
last row of the list for a file not yet listed. The highlight and the scroll offset stay where the
reader put them, in the frame the edit arrives in. The totals update. The last row of a list longer
than its room says how many files are below and how many of them arrived since the reader last
pressed a key, as [SCROLL-8](scroller.md#SCROLL-8) does for the transcript. A turn starting, ending
or being queued, a loop tick, a watch firing and a goal sending the work back leave the state open
where it is ([WATCH-10](watching.md#WATCH-10)).

`verified-by: bravebot_tui::files_mode::an_edit_arriving_leaves_the_highlight_and_the_scroll_where_they_were`
`verified-by: bravebot_tui::files_mode::the_list_says_a_file_has_arrived_below`
`verified-by: bravebot_tui::files_mode::what_starts_without_a_press_leaves_the_files_state_where_its_reader_put_it`

<a id="FILES-9"></a>
### FILES-9: the keys are the state's, and the box keeps its line

While the state is open every key is answered by it, and a key it does not name does nothing. A
character does not reach the box, Enter sends nothing, and a paste or a drop does not reach the line,
as in [SCROLL-2](scroller.md#SCROLL-2). The line keeps its text and caret and comes back as it was
when the state ends. The box is drawn without its caret, and the hint line names the state's keys
(`↑↓ file`, `PgUp PgDn scroll`, `[ ] edit`, `q back`) in the interface's own words. Ctrl-C ends the
state and does nothing else, so a turn in flight goes on.

A press on a list row highlighting it, and the wheel over the edits scrolling them, are not built
until [#2685](https://github.com/brave/bravebot/issues/2685) adds clicking to the terminal. The keys
do the same work until then.

`verified-by: bravebot_tui::app::a_typed_character_does_not_reach_the_box_while_the_files_state_is_open`
`verified-by: bravebot_tui::app::a_key_the_state_does_not_name_does_nothing`
`verified-by: bravebot_tui::app::the_line_comes_back_untouched_when_the_state_ends`
`verified-by: bravebot_tui::app::ctrl_c_ends_the_state_and_leaves_the_turn_running`
`verified-by: bravebot_tui::render::the_box_is_drawn_without_its_caret_while_the_state_is_open`

<a id="FILES-10"></a>
### FILES-10: nothing reaches a model, and the state shows what the session wrote

The state reads the activity entries the transcript already holds and keeps no second copy, so a
bound on those entries is a bound here. It is held only by the interface: no edit, count or row is
sent to a model or written to the session record, and `/files` is not sent to the planner
([CMD-2](commands.md#CMD-2)).

The session record holds no write's changes ([SESSION-2](sessions.md#SESSION-2)), so a resumed
session lists only edits made since it was resumed, and the heading says that earlier edits are not
kept. `/clear` removes every edit and ends the state. `/undo` and `/rewind` remove the edits of the
turns they undo, and the state ends if none is left. `/hunks <number>`, a person's own edit and a
later program change nothing in the state, since it draws what the session wrote.

Every word the state says is in the catalogue ([localization.md](localization.md)).

`verified-by: bravebot_tui::files_mode::nothing_the_state_holds_reaches_the_request_or_the_record`
`verified-by: bravebot_tui::files_mode::a_resumed_session_lists_only_the_edits_made_since`
`verified-by: bravebot_tui::files_mode::clearing_ends_the_files_state_and_forgets_its_edits`
`verified-by: bravebot_tui::app::undoing_a_turn_removes_its_edits_from_the_list`
`verified-by: bravebot_tui::files_mode::the_state_draws_only_catalogue_words`

## Known costs

- **A write a delegate made is not listed.** A delegate's calls are kept under the entry that stands
  for it and are bounded by what a delegate keeps, so the state reads the entries of the session's
  own turns.
- **The desktop window has no files state.** The state is built on the terminal's panel and its
  keys, and the window draws its own panel, so a person using the window reads writes in the
  transcript.
- **There is no source that shows the working tree against `HEAD`.** The state draws what the
  session's own write calls changed, which is what [FILES-10](#FILES-10) bounds, and a change made
  by another program is not in it.
- **An edit cannot be put back from the state.** `/hunks` does that for the last turn's hunks
  ([SESSION-37](sessions.md#SESSION-37)), and the state only reads.
