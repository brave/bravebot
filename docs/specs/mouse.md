---
id: MOUSE
title: Selecting text with the mouse
status: normative
governs:
  - crates/tui/src/select.rs
  - crates/tui/src/text_links.rs
  - crates/tui/src/file_preview.rs
  - crates/agent/src/file_preview.rs
  - crates/tui/src/state/targets.rs
  - crates/tui/src/state/menu.rs
  - crates/tui/src/state/hover.rs
  - crates/tui/src/app/keys.rs
  - crates/tui/src/input.rs
  - crates/tui/src/confirm.rs
  - crates/tui/src/panel.rs
  - crates/tui/src/list_picker.rs
  - crates/tui/src/model_prompt.rs
  - crates/tui/src/theme_prompt.rs
  - crates/tui/src/resume.rs
  - crates/tui/src/sessions_view.rs
  - crates/tui/src/render/hint.rs
  - crates/tui/src/render/hover.rs
  - crates/tui/src/render/menu.rs
documented-by:
  - docs/website/docs/using/transcript.md
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/reference/commands.md
  - docs/website/docs/using/interactive-mode.md
---

## Scope

What the mouse does in the terminal interface. A drag selects text in the pane it began in and
copies it, and the clauses about selecting say what a selection takes, how it follows the transcript
as the view scrolls and when it is put away ([MOUSE-1](#MOUSE-1)). A click is a shortcut to something
that has a key or a command, and the clause for each click names it: the clicks in the transcript and
in a delegate's view, in the info panel and its files state, on the hints and the text of the box under the transcript,
in the pickers and the lists, and on a link in the assistant's words, a web address with ctrl held and
a file path, shown or opened in the editor. The wheel, a press outside a panel and the scrollbar are
the same kind of shortcut. What a click acts on is found in one record the frame wrote as it drew
([MOUSE-50](#MOUSE-50)). Where the terminal reports the pointer moving, the target under it is held
and said ([MOUSE-51](#MOUSE-51)), and a right click on a reply, a delegate's block or a selection
opens a small menu ([MOUSE-52](#MOUSE-52)). A report at a column or a row past 223 is the cell it names
([MOUSE-53](#MOUSE-53)). No mouse event answers a question the interface puts to a person
([MOUSE-44](#MOUSE-44)), and a press on a panel drawn over the frame reaches nothing beneath it
([MOUSE-45](#MOUSE-45)). The keyboard route to each click is in its clause, and `/open` is the one for
the links ([CMD-30](commands.md#CMD-30)).

What is not built, each named in the clause it bears on:

- A check of the interface inside tmux and GNU screen. What a multiplexer forwards is read from its
  documents and source, and the interface has not been run inside either ([MOUSE-53](#MOUSE-53)).
- Branching from an earlier turn. The menu's `branch this session` copies the whole session, since
  nothing cuts a copy at a turn ([MOUSE-52](#MOUSE-52)).
- A wheel that reads `tui.wheelRows` in an approval or other question that scrolls. It moves the body
  three rows, and it does not walk the questions that move a cursor among answers, the questions asked
  before a session starts, the reading of a credential or the question of deleting a record
  ([MOUSE-46](#MOUSE-46)).
- A key for the mark of a finished task, which opens words the transcript already holds, or for
  resizing the panel with a drag ([MOUSE-32](#MOUSE-32), [PANEL-6](info-panel.md#PANEL-6)).
- A link from a task of the plan to a delegate: no task belongs to a delegate, so a click on one never
  opens a delegate's view ([MOUSE-31](#MOUSE-31)).
- A triple click that takes a paragraph or a message block of the transcript. It takes the row the
  screen draws, and a line the wrap cut over several rows is taken a row at a time
  ([MOUSE-6](#MOUSE-6)).
- A modifier that every terminal sends. Terminals do not report the Cmd key, and one that keeps
  ctrl-click or shift for its own use never sends it, so the link clicks need ctrl, a right click can be
  left to the terminal with `tui.contextMenu`, and the bypass modifier is the terminal's
  ([MOUSE-4](#MOUSE-4), [MOUSE-7](#MOUSE-7), [MOUSE-40](#MOUSE-40)).
- A file that the editor is opened on can act on the editor, as any file opened there by hand can, and
  a file path is shown or opened only where the click on it acts ([MOUSE-42](#MOUSE-42),
  [MOUSE-43](#MOUSE-43)).

## Clauses

<a id="MOUSE-1"></a>
### MOUSE-1: a selection belongs to the pane it began in

The screen is recorded each frame as panes: the transcript, the status line, the prompt box, what is
offered beneath it, the footer and, when it is drawn, the info panel. The prompt box's pane is the
cells inside its border, which holds no text. A press inside a pane starts a selection that belongs to
that pane. The pointer is held inside the pane's rectangle, so a drag past its right edge stops at its
last column and a drag past its bottom edge stops at its last row, and the copy and the highlight read
only that pane's cells. The far end of a selection is the cell the pointer is on and not a cell it
covers, so a pointer on the last column of a pane takes that column in: a pane that ends where the
screen does, as the info panel does, has no cell further right to take it from. A drag from the transcript across the info
panel copies no panel text, and the reverse. A view that records no panes, such as the scroller,
treats the whole screen as one pane, except for the transcript it draws, which is a pane of its own
([MOUSE-34](#MOUSE-34)). A transcript ends in a blank row, and one shorter than its pane leaves
blank cells under that. A pointer on any of them is at the end of the last row that holds text,
whether the drag began above them or on them, so the words that go on being written on that row do
not put the selection away ([MOUSE-34](#MOUSE-34)).

In every view the cells that are marked are the cells the copy takes, and the cells of a panel drawn
over the view, as the scroller's list of keys is, are no cells of what the selection is of. A terminal
one row or one column across is drawn with what little it has, and a press, a drag, a double click, a
triple click and a release at any cell of it read and paint only cells the screen has. A history
search holds the mouse ([MOUSE-47](#MOUSE-47)), so a drag in it marks and copies nothing.

Letting go of the button hands the text selected to the clipboard in every loop that reads the
mouse: at rest, while a turn runs and while a command runs. A press and a release on one cell hand
over nothing, and the notice that says how much was copied is drawn only when the clipboard took
the text.

**The keyboard.** A selection is the mouse's own, so what stands for it without one is getting the
text another way: the terminal's own selection, which the terminal's bypass modifier gives back while
the mouse is captured and `tui.mouse` set to `false` gives back always ([MOUSE-4](#MOUSE-4)), `/copy`
for a reply, `/copy block` for a fenced block of one and `/copy text` for a reply as drawn
([CMD-11](commands.md#CMD-11)), and `v` in the scroller, which opens the whole transcript in the
editor ([SCROLL-6](scroller.md#SCROLL-6)). The clauses about what a selection takes and how it is
made ([MOUSE-2](#MOUSE-2) and [MOUSE-3](#MOUSE-3), [MOUSE-5](#MOUSE-5) to [MOUSE-8](#MOUSE-8) and
[MOUSE-34](#MOUSE-34) to [MOUSE-39](#MOUSE-39)) are about this one act.

`verified-by: bravebot_tui::app::a_drag_from_the_transcript_never_copies_the_panel`
`verified-by: bravebot_tui::app::a_drag_from_the_panel_never_copies_the_transcript`
`verified-by: bravebot_tui::select::a_sweep_past_the_edge_of_its_pane_stops_at_the_edge`
`verified-by: bravebot_tui::select::a_drag_that_ends_on_the_last_column_of_a_pane_takes_it_in`
`verified-by: bravebot_tui::app::tests::selection::a_drag_to_the_last_column_of_the_screen_copies_the_last_character_of_a_panel_row`
`verified-by: bravebot_tui::app::tests::box_text::a_drag_across_the_box_copies_its_words_and_not_its_border`
`verified-by: bravebot_tui::select::rows_between_the_ends_are_cut_to_the_pane`
`verified-by: bravebot_tui::select::a_highlight_stays_inside_its_pane`
`verified-by: bravebot_tui::app::a_drag_past_the_bottom_stops_at_the_last_row_of_the_pane`
`verified-by: bravebot_tui::app::tests::selection::a_drag_below_the_last_row_of_a_short_transcript_copies_down_to_the_end_of_it`
`verified-by: bravebot_tui::app::tests::selection::a_selection_ended_below_a_reply_that_is_being_written_survives_its_next_words`
`verified-by: bravebot_tui::app::tests::selection::a_selection_ended_below_a_reply_that_wraps_as_it_is_written_survives_it`
`verified-by: bravebot_tui::app::tests::selection::a_selection_that_went_below_the_text_follows_the_pointer_back_into_it`
`verified-by: bravebot_tui::app::tests::selection::a_drag_from_the_blank_rows_under_the_text_up_into_it_copies_from_the_end_of_the_text`
`verified-by: bravebot_tui::app::tests::selection::a_drag_below_the_last_row_in_the_scroller_copies_down_to_the_end_of_it`
`verified-by: bravebot_tui::app::tests::selection::a_selection_held_under_the_scrollers_key_list_is_not_marked_over_it`
`verified-by: bravebot_tui::app::a_terminal_one_row_high_takes_a_mouse_report_anywhere_without_panicking`
`verified-by: bravebot_tui::app::a_release_over_the_transcript_hands_the_clipboard_its_text_and_none_of_the_panel`
`verified-by: bravebot_tui::app::a_release_over_the_panel_hands_the_clipboard_its_text_and_none_of_the_transcript`
`verified-by: bravebot_tui::app::a_release_at_rest_hands_the_clipboard_what_was_swept`
`verified-by: bravebot_tui::app::a_release_while_a_turn_runs_hands_the_clipboard_what_was_swept`
`verified-by: bravebot_tui::app::a_click_hands_over_nothing_and_a_refused_copy_is_not_reported`
`verified-by: bravebot_tui::app::a_drag_in_a_history_search_begins_no_selection_and_copies_nothing`
`verified-by: bravebot_tui::app::tests::selection::every_loop_that_answers_the_mouse_carries_out_the_copy_a_release_asks_for`

<a id="MOUSE-2"></a>
### MOUSE-2: a press between panes starts nothing

A press that lands in no recorded pane begins no selection and clears the one held. A press on the
info panel's border column starts a resize instead ([PANEL-6](info-panel.md#PANEL-6), which says why
the resize has no key), which also begins no selection and copies nothing on release. In the files
state, where the border is not a handle ([FILES-4](files-mode.md#FILES-4)), a press on it starts
nothing at all.

`verified-by: bravebot_tui::select::a_press_between_recorded_panes_belongs_to_none`
`verified-by: bravebot_tui::app::a_press_outside_every_pane_starts_no_selection`
`verified-by: bravebot_tui::panel::a_drag_from_the_border_never_selects_or_copies`
`verified-by: bravebot_tui::app::a_press_on_the_panels_border_starts_no_selection_in_the_files_state`

<a id="MOUSE-3"></a>
### MOUSE-3: a selection does not outlive its pane

A selection follows its pane's rectangle as the next frame lays it out. It is dropped when the pane
is no longer drawn, as when the info panel is closed, and when the terminal changes size in either
direction, because the cells it named then hold other text. A selection of the transcript is also
dropped when the transcript is drawn at another width, as opening or closing the info panel or
dragging its border does, since the text then wraps into other rows ([MOUSE-34](#MOUSE-34)).

`verified-by: bravebot_tui::app::closing_the_panel_drops_a_selection_inside_it`
`verified-by: bravebot_tui::app::a_width_change_drops_the_selection`
`verified-by: bravebot_tui::app::a_height_change_drops_the_selection`
`verified-by: bravebot_tui::app::a_height_only_resize_in_the_scroller_drops_the_selection`
`verified-by: bravebot_tui::app::a_resize_in_the_middle_of_a_drag_leaves_nothing_to_extend_or_copy`
`verified-by: bravebot_tui::app::toggling_the_panel_drops_a_selection_of_the_transcript_it_would_leave_over_other_text`

<a id="MOUSE-4"></a>
### MOUSE-4: mouse capture can be turned off

`tui.mouse` set to the boolean `false`, in any settings file, stops the interface asking the terminal
to report the mouse: the wheel and a drag are the terminal's own, and the interface does no selecting
of its own. Any other value, including the string `"false"`, leaves capture on. While the mouse is
reported the `?` list names the drag that selects and the terminal's bypass modifier, Shift on most
terminals and Option on macOS iTerm2 and Terminal.app, and the right click while the menu it opens is
on ([MOUSE-52](#MOUSE-52)). With `tui.mouse` false it names none of them, since the terminal's own
selection needs no modifier then and the interface reads no drag. It draws none of the words that
stand for a click either, which no click reaches: the word that copies a fenced block
([MOUSE-27](#MOUSE-27)) and the label that leaves a delegate's view ([MOUSE-24](#MOUSE-24)).

`verified-by: bravebot_config::settings::only_a_boolean_turns_mouse_capture_off`
`verified-by: bravebot_tui::app::a_session_with_the_mouse_off_never_asks_for_mouse_reporting`
`verified-by: bravebot_tui::app::the_mouse_is_captured_unless_the_settings_turn_it_off`
`verified-by: bravebot_tui::render::the_shortcuts_name_the_terminals_own_selection`
`verified-by: bravebot_tui::render::the_key_list_names_the_mouse_only_where_it_is_reported`
`verified-by: bravebot_tui::app::tests::chrome::the_word_that_copies_a_block_is_drawn_only_where_the_mouse_is_reported`
`verified-by: bravebot_tui::app::tests::chrome::the_way_back_out_of_a_delegates_view_is_drawn_only_where_the_mouse_is_reported`

<a id="MOUSE-5"></a>
### MOUSE-5: a selection in the info panel takes its text and nothing around it

The info panel draws a border column and a space before each row's text. A press on the border
column resizes the panel ([MOUSE-2](#MOUSE-2)). A selection that begins on the space starts at the
first column of the text, and the highlight stops at the end of each row's
text. The copy takes the text alone, with no border, indent or trailing blanks. The session id is
drawn over two rows because it does not fit on one, and a copy that spans both rows reads it as one
line. Only a selection of the panel reads them so: the transcript is drawn beside the panel on the
same rows, and its lines are not joined by it.

`verified-by: bravebot_tui::select::a_panel_selection_takes_the_text_and_not_the_border_or_indent`
`verified-by: bravebot_tui::select::a_panel_highlight_covers_the_text_and_not_the_chrome_or_padding`
`verified-by: bravebot_tui::select::a_joined_row_runs_on_into_the_next_without_a_break`
`verified-by: bravebot_tui::app::a_drag_over_the_session_id_copies_the_id_alone`
`verified-by: bravebot_tui::select::the_rows_the_panel_joins_are_joined_only_in_a_selection_of_the_panel`
`verified-by: bravebot_tui::app::a_copy_of_the_transcript_beside_the_session_id_keeps_its_lines_apart`

<a id="MOUSE-6"></a>
### MOUSE-6: a double click takes a word and a triple click a row

Two presses of the left button on the same cell within 400 milliseconds select the word there, and
a third the whole row, inside the pane the press belongs to. The row is the row the screen draws: a
line the wrap cut over several rows is taken one row at a time, and the paragraph or the message block
that the issue allows as a reading of a line is not taken. A selection that spans the rows of one
wrapped line copies them as that line ([MOUSE-8](#MOUSE-8)), and a reply is copied whole by its marker
or `/copy` ([MOUSE-26](#MOUSE-26)). A word is a run of letters, digits and
underscores, a run of blanks, or a run of anything else. A character drawn two cells wide is one
character: both of its cells belong to the word it is in. A row is taken up to its last character,
without the blanks the widget padded it with. Dragging on from such a press grows the selection by
whole words or whole rows at the loose end. A fourth press starts the count again, and a press on
another cell or after the interval is a single press.

`verified-by: bravebot_tui::select::a_double_click_takes_the_word_under_it`
`verified-by: bravebot_tui::select::a_word_stops_at_punctuation`
`verified-by: bravebot_tui::select::a_word_of_wide_characters_is_taken_whole`
`verified-by: bravebot_tui::select::a_drag_after_a_double_click_grows_by_words`
`verified-by: bravebot_tui::select::a_triple_click_takes_the_row_without_its_padding`
`verified-by: bravebot_tui::select::a_drag_after_a_triple_click_grows_by_rows`
`verified-by: bravebot_tui::select::a_word_never_leaves_its_pane`
`verified-by: bravebot_tui::select::a_press_counts_as_part_of_a_run_only_on_the_same_cell_in_time`
`verified-by: bravebot_tui::app::a_double_click_selects_a_word_and_a_triple_click_a_row`
`verified-by: bravebot_tui::app::two_presses_on_different_cells_are_not_a_double_click`

<a id="MOUSE-7"></a>
### MOUSE-7: a shift-click extends the selection held

A press with shift held, inside the pane the selection held belongs to, moves the selection's loose
end to the pointer and keeps its anchor. The pane is the whole of its rectangle, so a press on the
blank before the text of a row of the info panel, which is no text, extends a selection of the panel
to the first column of the text. A shift-press anywhere else, or with no selection held, is an
ordinary press. Terminals that keep shift for their own selection never report it.

`verified-by: bravebot_tui::app::a_shift_click_extends_the_selection_held`
`verified-by: bravebot_tui::app::a_shift_click_in_another_pane_begins_a_new_selection`
`verified-by: bravebot_tui::app::tests::selection::a_shift_click_on_the_indent_of_a_panel_row_extends_the_selection_held`

<a id="MOUSE-8"></a>
### MOUSE-8: a wrapped line copies as one line

A line the transcript is wider than is drawn over several rows. A copy that spans those rows reads
them as the one line they are, so a wrapped paragraph is pasted as a paragraph and not with a line
break at every row the screen cut it at. A row the wrap broke at a word is joined to the next with
one space, even when the word ended in the last column and the blank after it was dropped, and a
row cut inside a word with none. Whether a break is at a blank is told by the characters between the
last one drawn on the rows before it and the first one drawn on the row, whatever the wrap did with a
run of blanks there: the cells it left empty at the end of a row are not text, a blank it left at the
start of the next row is, and a no-break space is a character of the row it is on, kept at the end of
one. It is told from the start of a line as far as the rows on the screen reach, so a line of a
million characters costs the rows shown. Lines
the author broke are still separate lines. This holds in the transcript and in the scroller, and it is not applied to any other pane: the rows are screen rows, and the info
panel is drawn on the same ones.

`verified-by: bravebot_tui::select::a_wrapped_row_joins_the_row_below_it`
`verified-by: bravebot_tui::select::a_blank_dropped_at_the_last_column_is_still_a_space`
`verified-by: bravebot_tui::render::transcript::a_break_is_told_from_one_inside_a_word`
`verified-by: bravebot_tui::render::transcript::a_break_is_at_a_blank_whatever_the_wrap_did_with_the_run_of_them`
`verified-by: bravebot_tui::render::transcript::a_wrapped_line_copies_as_the_line_it_is_whatever_the_blanks_at_its_breaks`
`verified-by: bravebot_tui::render::transcript::the_breaks_of_the_rows_shown_of_a_very_long_line_are_read_from_its_start`
`verified-by: bravebot_tui::app::tests::wrapped::a_wrapped_reply_copies_as_its_words_whatever_the_blanks_and_the_width`
`verified-by: bravebot_tui::select::a_wrap_in_the_transcript_does_not_join_the_rows_of_another_pane`
`verified-by: bravebot_tui::app::a_wrapped_reply_copies_as_one_paragraph`
`verified-by: bravebot_tui::app::a_release_in_the_scroller_hands_the_clipboard_the_paragraph_that_was_swept`
`verified-by: bravebot_tui::app::a_whole_transcript_copies_as_the_lines_it_is_made_of`

<a id="MOUSE-9"></a>
### MOUSE-9: a click on a call in the scroller expands it

In the scroller, a left click that sweeps nothing on a row of a call that has more to show than it
draws expands that call, and a second click collapses it, as `x` does ([SCROLL-10](scroller.md#SCROLL-10)).
The press and the release are on one cell and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). A click on a row that belongs to no call, with
the key list open over the transcript, or in a delegate's view, does nothing to any call. The key
stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_call_in_the_scroller_expands_and_collapses_it`
`verified-by: bravebot_tui::app::a_click_off_a_call_or_a_drag_over_one_expands_nothing`
`verified-by: bravebot_tui::app::a_click_with_the_scroller_help_open_expands_nothing`
`verified-by: bravebot_tui::app::a_click_in_a_delegates_view_expands_nothing`
`verified-by: bravebot_tui::app::a_click_on_every_row_of_a_call_in_the_scroller_expands_it`

<a id="MOUSE-10"></a>
### MOUSE-10: a click on a delegate in the scroller opens its view

In the scroller, a left click that sweeps nothing on a row of a delegate's block opens the view of
that delegate, and the key that closes the view returns to the scroller. The press and the release
are on one cell and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). A
click on a row that belongs to no delegate, with the key list open over the transcript, or where a
view is already open, opens nothing. The watch key (Ctrl-L, [WATCH-7](watching.md#WATCH-7)) opens
the views of the delegates from the scroller as it does at the prompt ([SCROLL-2](scroller.md#SCROLL-2)),
the list where there are several and the view where there is one, and stays the way to do it without
a mouse. At the plain prompt the click is [MOUSE-23](#MOUSE-23).

`verified-by: bravebot_tui::app::a_click_on_a_delegate_in_the_scroller_opens_that_delegates_view`
`verified-by: bravebot_tui::app::a_drag_over_a_delegate_or_a_click_off_one_opens_no_view`
`verified-by: bravebot_tui::app::a_click_with_the_scroller_help_open_opens_no_delegate`
`verified-by: bravebot_tui::app::the_watch_key_opens_the_delegates_from_the_scroller_and_closing_returns_to_it`
`verified-by: bravebot_tui::app::a_click_on_every_row_of_a_delegates_block_in_the_scroller_opens_its_view`

<a id="MOUSE-11"></a>
### MOUSE-11: a click on the panel's model or effort row asks what the command asks

In the info panel, a left click that sweeps nothing on the row that names the model asks for the
model picker, as `/model` does, and one on the row that names the effort asks for the effort picker,
as `/effort` does. The commands are typed at the plain prompt, so a click over the scroller or a history search asks for nothing, and [MOUSE-29](#MOUSE-29) says what the click does while something is running. Where a definition in force names the model the click is refused as the command
is ([CLI-17](cli.md#CLI-17)), with the same note. The press and the release are on one cell and the
press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)), and a press that began
elsewhere and was let go on the row is not a click on it. A double click asks for the picker once:
the presses that follow a click that acted begin nothing ([MOUSE-50](#MOUSE-50)). The border
column is the panel's resize handle ([PANEL-6](info-panel.md#PANEL-6)) and a row that is not drawn,
for want of room or because the effort is not one the model reads, is not a target. The command
stays the way to do it without a mouse, and the click acts only on rows the person's own choices
put there ([PANEL-11](info-panel.md#PANEL-11)).

`verified-by: bravebot_tui::app::a_click_on_the_panels_model_or_effort_row_opens_that_picker`
`verified-by: bravebot_tui::app::a_click_off_the_models_rows_asks_for_no_picker`
`verified-by: bravebot_tui::app::a_drag_over_the_model_row_opens_no_picker_and_a_double_click_opens_it_once`
`verified-by: bravebot_tui::app::a_press_elsewhere_let_go_on_the_model_row_opens_no_picker`
`verified-by: bravebot_tui::app::a_click_on_the_model_row_over_the_scroller_opens_no_picker`
`verified-by: bravebot_tui::app::a_click_on_the_model_row_under_a_standing_definition_is_refused_as_the_command_is`
`verified-by: bravebot_tui::panel::only_the_drawn_model_and_effort_rows_are_click_targets`

<a id="MOUSE-12"></a>
### MOUSE-12: a click on the panel's context reading asks what `/context` asks

In the info panel, a left click that sweeps nothing on the row that reads how full the context is
says what `/context` says. As for [MOUSE-11](#MOUSE-11), the press and the release are on one cell
and the press is a single click, so a drag is a selection, and a
click over the scroller or a history search asks for nothing. The heading and the rows below the
reading are not targets, nor is a reading cut for want of room. The command stays the way to do it
without a mouse, and what the click says is the session's own reading, with nothing a tool returned
in it ([PANEL-11](info-panel.md#PANEL-11)).

`verified-by: bravebot_tui::app::a_click_on_the_panels_context_reading_reports_the_context`
`verified-by: bravebot_tui::app::a_drag_over_the_context_reading_or_a_click_in_the_scroller_reports_nothing`
`verified-by: bravebot_tui::panel::only_the_context_reading_is_a_context_click_target`

<a id="MOUSE-13"></a>
### MOUSE-13: a click on a panel's MCP server row asks what `/mcp` asks

In the info panel, a left click that sweeps nothing on the row of a started MCP server lists the
servers, as a bare `/mcp` does. As for [MOUSE-12](#MOUSE-12), the press and the release are on one
cell and the press is a single click, so a drag is a selection,
and a click over the scroller or a history search asks for nothing. The heading and the rows around
the servers are not targets, nor is a row cut for want of room, and a language server's row is a
target of its own ([MOUSE-22](#MOUSE-22)). The command stays the way to do it without a mouse, and
the listing is the session's own, with nothing a tool returned in it
([PANEL-11](info-panel.md#PANEL-11)).

`verified-by: bravebot_tui::app::a_click_on_the_panels_mcp_server_row_lists_the_servers`
`verified-by: bravebot_tui::app::a_drag_over_an_mcp_server_row_or_a_click_in_the_scroller_lists_nothing`
`verified-by: bravebot_tui::panel::only_a_started_mcp_servers_rows_are_click_targets`

<a id="MOUSE-14"></a>
### MOUSE-14: a click on the permission mode under the box cycles it

On the row under the box, a left click that sweeps nothing on the permission mode, where the row
draws one, moves to the next mode round the ladder the key that cycles it goes round
([MODE-8](permission-modes.md#MODE-8)), except that it never moves into bypass. As for
[MOUSE-12](#MOUSE-12), the press and the release are on one cell and the press is a single click, so a
drag is a selection, and a click over the scroller or a history search does nothing. The rest of the row is not a target, nor is a mode the row does not draw: the
ordinary mode has no name there, and a row too narrow for the mode is cleared whole. While a command
the person typed is running the click does nothing, as the key does not: nothing reads the keys of
the row then ([MOUSE-48](#MOUSE-48)). The other hints of the row are [MOUSE-48](#MOUSE-48)'s.

Bypass answers every later question without asking ([MODE-5](permission-modes.md#MODE-5)). A click
can be made by accident, or by a terminal that reports a position it was not at, and it must not be
able to grant that. From plan a click moves to the ordinary mode, and a click in bypass moves to the
ordinary mode too. The key reaches bypass where it is reachable. The key stays the way to do it
without a mouse, and is the only way into bypass.

`verified-by: bravebot_tui::app::a_click_on_the_footers_permission_mode_cycles_it`
`verified-by: bravebot_tui::app::a_click_on_the_permission_mode_moves_as_the_key_does_and_never_into_bypass`
`verified-by: bravebot_tui::app::a_click_on_the_permission_mode_in_bypass_moves_out_of_it`
`verified-by: bravebot_tui::app::a_drag_over_the_mode_or_a_click_beside_it_leaves_the_permission_mode_alone`

<a id="MOUSE-15"></a>
### MOUSE-15: a press on a row of a picker moves the cursor, and a second takes it

In the list picker that `/effort`, `/config` and `/theme` open, and in the model picker that
`/model` opens, at rest or while a turn runs, a left press on a row of the list puts the cursor on
it, and a second press within the double-click interval on the row the first one named takes the row, as
Enter does. A press on the border, on the lines under the list or below the last row, a press of another
button, a drag and a release leave the picker open with the cursor where it was, and no mouse event
cancels it except the press [MOUSE-47](#MOUSE-47) says does. Rows are counted from the first row
drawn, so a list that has scrolled to keep the cursor in view is read as it is drawn. A press on a row
that is drawn moves the cursor and leaves the rows where they are, whether or not the list has scrolled,
so a double click in place takes the row pointed at. A key or a turn of the wheel moves the cursor and
the rows as it always did, with the cursor on the last row drawn once it is past the first page. The
second press is held to the row and not to the cell: after the wheel has moved the rows, a press on the
cell of the first row is a first press on the row drawn there now, and one where the first row is
drawn now takes it. In the model
picker the rows are the models, and a press on a service's heading, on the space between two
sections or on the box the search is typed into is on no model. In `/theme` the theme under the
cursor is in force as the cursor reaches it, by a press as by a key ([MOUSE-17](#MOUSE-17)).

In the search over the prompts already sent, which Ctrl-R opens, a left press on a prompt puts the
cursor on it and leaves the list where it is, and a second on the same prompt takes it into the box, as
Enter does, and sends nothing. A press on the title, on the box the search is typed into, on the line of keys, or on the prompt
shown in full beside the list is on no prompt. The search holds the input, so a press anywhere on the
screen is its own and begins no selection of what the transcript above it says.

In the list of sessions that `--resume` and `/resume` open, a left press on a session puts the cursor
on it, on any of the three rows it takes, and leaves the list where it is, and a second on the same
session takes it as Enter does: it resumes the session, or says why it cannot be, which is the sentence Enter gives for a manifest run
and for a record a background session or another process holds. The list `bravebot sessions` opens
takes a press the same way, and a second press attaches. A roster read again between two presses that
puts another session at the cell does not make them one double click. A press on a heading, on the space between
two groups, on the search box, the project or the line of keys, or below the last session is on no
session. The question of deleting a record is answered by the next key and by nothing else, so while
it stands no press or turn of the wheel does anything to it or to the list.

In the list of commands, skills or files that is offered under the box as a line is typed,
a left click that sweeps nothing on a row puts the cursor on it, and a second click on the same
cell within the double-click interval takes it into the box as Tab does. The list draws its items from
the first and does not move with a click, so a double click in place takes the row pointed at. It never runs a command:
Enter does. The click is read on its release, so that a drag across the list still selects what it
covers, and it takes only what the frame drew, so a row the pane had no room for is not there to
click. The keys that `?` lists have nothing to take, so a click on one does nothing, and a click in
the box is [MOUSE-16](#MOUSE-16)'s. The keys stay the way to do it without a mouse.

`verified-by: bravebot_tui::list_picker::a_press_moves_the_cursor_and_a_second_press_takes_the_row`
`verified-by: bravebot_tui::list_picker::two_presses_on_different_rows_or_far_apart_take_nothing`
`verified-by: bravebot_tui::list_picker::a_press_inside_the_panel_but_off_the_rows_and_anything_but_a_left_press_leave_it_alone`
`verified-by: bravebot_tui::list_picker::a_press_finds_its_row_in_a_list_that_has_scrolled`
`verified-by: bravebot_tui::list_picker::a_double_click_in_place_takes_the_row_pointed_at_on_every_row_of_a_list_scrolled_or_not`
`verified-by: bravebot_tui::list_picker::the_picker_opened_at_rest_takes_the_row_double_clicked_in_place`
`verified-by: bravebot_tui::list_picker::a_press_where_the_wheel_has_since_drawn_another_row_is_a_first_press_on_it`
`verified-by: bravebot_tui::list_picker::a_key_after_a_press_moves_the_list_as_the_keys_alone_do`
`verified-by: bravebot_tui::list_picker::the_panel_and_its_rows_are_drawn_where_placed_says_they_are`
`verified-by: bravebot_tui::list_picker::the_picker_opened_at_rest_reads_the_pointer`
`verified-by: bravebot_tui::theme_prompt::the_pointer_previews_the_row_and_an_outside_press_puts_the_old_theme_back`
`verified-by: bravebot_tui::app::tests::pickers::a_press_on_a_row_moves_the_cursor_and_a_second_takes_it_in_a_picker_opened_mid_turn`
`verified-by: bravebot_tui::app::tests::pickers::presses_on_two_rows_or_off_the_rows_take_nothing_in_a_picker_opened_mid_turn`
`verified-by: bravebot_tui::app::tests::pickers::the_preferences_picker_opened_mid_turn_takes_a_press_and_the_wheel_too`
`verified-by: bravebot_tui::app::tests::pickers::a_double_click_in_place_takes_the_model_pointed_at_on_every_row_of_a_list_scrolled_or_not`
`verified-by: bravebot_tui::model_prompt::a_press_on_a_model_moves_the_cursor_and_a_second_takes_it`
`verified-by: bravebot_tui::model_prompt::two_presses_on_different_models_or_far_apart_take_nothing`
`verified-by: bravebot_tui::model_prompt::a_press_on_no_model_and_any_other_mouse_event_leave_the_picker_alone`
`verified-by: bravebot_tui::model_prompt::a_press_finds_its_model_in_a_list_that_has_scrolled_under_a_held_heading`
`verified-by: bravebot_tui::model_prompt::a_double_click_in_place_takes_the_model_pointed_at_on_every_row_of_a_list_scrolled_or_not`
`verified-by: bravebot_tui::model_prompt::a_press_where_the_wheel_has_since_drawn_another_model_is_a_first_press_on_it`
`verified-by: bravebot_tui::model_prompt::a_key_after_a_press_moves_the_list_as_the_keys_alone_do`
`verified-by: bravebot_tui::app::tests::pickers::the_model_picker_opened_mid_turn_takes_a_press_the_wheel_and_a_press_outside`
`verified-by: bravebot_tui::app::tests::history_search::a_press_on_a_prompt_moves_the_cursor_and_a_second_takes_it_into_the_box`
`verified-by: bravebot_tui::app::tests::history_search::two_presses_on_different_prompts_or_far_apart_take_nothing`
`verified-by: bravebot_tui::app::tests::history_search::each_row_drawn_is_the_prompt_a_press_on_it_picks_in_a_window_that_has_slid`
`verified-by: bravebot_tui::app::tests::history_search::a_double_click_in_place_takes_the_prompt_pointed_at_on_every_row_of_a_list_scrolled_or_not`
`verified-by: bravebot_tui::app::tests::history_search::a_press_where_the_wheel_has_since_drawn_another_prompt_is_a_first_press_on_it`
`verified-by: bravebot_tui::app::tests::history_search::a_key_after_a_press_moves_the_window_as_the_keys_alone_do`
`verified-by: bravebot_tui::app::tests::history_search::a_press_on_the_panel_off_the_matches_and_any_other_event_leave_the_search_alone`
`verified-by: bravebot_tui::app::tests::completions::a_click_on_a_command_moves_the_cursor_and_a_second_takes_it_into_the_box`
`verified-by: bravebot_tui::app::tests::completions::a_double_click_in_place_takes_the_row_pointed_at_on_every_row_of_a_list_whole_or_cut`
`verified-by: bravebot_tui::app::tests::completions::clicks_on_two_rows_or_a_drag_over_the_list_take_nothing`
`verified-by: bravebot_tui::app::tests::completions::a_click_off_the_rows_and_a_click_of_another_button_take_nothing`
`verified-by: bravebot_tui::app::tests::completions::the_list_of_shortcuts_and_a_list_that_has_gone_are_not_rows_to_take`
`verified-by: bravebot_tui::app::tests::completions::each_row_drawn_is_the_item_a_click_on_it_picks_under_other_rows_and_in_little_room`
`verified-by: bravebot_tui::app::tests::completions::a_row_of_a_list_that_has_gone_since_the_frame_is_no_target`
`verified-by: bravebot_tui::app::tests::completions::a_click_on_a_row_of_the_list_offered_while_a_turn_runs_takes_it_the_same`
`verified-by: bravebot_tui::app::tests::completions::a_click_on_a_file_moves_the_cursor_and_a_second_writes_it_after_the_at`
`verified-by: bravebot_tui::resume::a_press_on_a_session_moves_the_cursor_and_a_second_takes_it`
`verified-by: bravebot_tui::resume::a_second_press_on_a_session_that_cannot_be_resumed_says_why`
`verified-by: bravebot_tui::resume::a_press_off_the_sessions_and_any_other_mouse_event_leave_the_list_alone`
`verified-by: bravebot_tui::resume::each_session_drawn_is_the_one_a_press_on_it_picks_in_a_list_that_has_scrolled`
`verified-by: bravebot_tui::resume::a_double_click_in_place_takes_the_session_pointed_at_on_every_entry_of_a_list_scrolled_or_not`
`verified-by: bravebot_tui::resume::a_press_where_the_wheel_has_since_drawn_another_session_is_a_first_press_on_it`
`verified-by: bravebot_tui::resume::a_key_after_a_press_moves_the_list_as_the_keys_alone_do`
`verified-by: bravebot_tui::resume::while_the_question_of_deleting_stands_the_pointer_does_nothing`
`verified-by: bravebot_tui::resume::the_list_reads_the_pointer_before_the_question_of_deleting_is_asked`
`verified-by: bravebot_tui::sessions_view::a_press_on_a_session_moves_the_cursor_and_a_second_attaches`
`verified-by: bravebot_tui::sessions_view::a_press_on_no_session_and_any_other_mouse_event_leave_the_list_alone`
`verified-by: bravebot_tui::sessions_view::each_session_drawn_is_the_one_a_press_on_it_picks_in_a_list_that_has_scrolled`
`verified-by: bravebot_tui::sessions_view::a_roster_read_again_between_two_presses_does_not_attach_the_session_that_moved_under_them`
`verified-by: bravebot_tui::sessions_view::the_view_asks_for_the_buttons_and_a_held_pointer_and_gives_the_mouse_and_screen_back`

<a id="MOUSE-16"></a>
### MOUSE-16: a click in the prompt box puts the caret there

In the prompt box, a left click that sweeps nothing on a cell of its text puts the caret before the
character drawn there. As for [MOUSE-12](#MOUSE-12), the press and the release are on one cell and
the press is a single click, so a drag is a selection. The cell is
read as the box drew it: the same wrap, the window of rows around the caret and the prompt character
or indent that opens each row. A cell on the prompt character lands at the start of its row and a cell
past the end of a row at its end, which for a row the wrap broke at a space is before that space, since
the place after it is the start of the next row; the border is not text. A character the box draws as
nothing, as it draws a tab, is no cell, and the cell after it is the character drawn there. A cell on a
marker lands at the marker's start, because the arrows never rest inside one. A click over the scroller, a history search or a
picker, in the vi style of box, and in the files state does nothing to the caret. A report at the
largest row or column a terminal can name is a cell outside the box. The arrow keys stay the way to
move it without a mouse.

`verified-by: bravebot_tui::app::a_click_in_the_box_puts_the_caret_where_it_landed`
`verified-by: bravebot_tui::app::a_click_in_the_box_counts_a_wide_character_as_one`
`verified-by: bravebot_tui::app::a_click_on_a_marker_puts_the_caret_before_it`
`verified-by: bravebot_tui::app::tests::box_text::a_click_right_of_a_row_broken_at_a_space_puts_the_caret_at_the_end_of_that_row`
`verified-by: bravebot_tui::app::tests::box_text::a_click_after_a_control_character_lands_on_the_character_drawn_there`
`verified-by: bravebot_tui::app::tests::box_text::a_click_in_a_box_that_has_scrolled_lands_on_the_word_drawn_there`
`verified-by: bravebot_tui::app::tests::box_text::a_click_at_the_largest_coordinates_is_a_cell_outside_the_box`
`verified-by: bravebot_tui::app::a_drag_a_double_click_or_a_click_off_the_text_leaves_the_caret_alone`
`verified-by: bravebot_tui::app::a_click_in_the_box_does_nothing_where_another_mode_is_in_use`
`verified-by: bravebot_tui::app::a_click_on_a_row_of_a_line_that_has_changed_since_it_was_drawn_lands_nowhere`
`verified-by: bravebot_tui::app::a_click_at_the_largest_column_or_row_lands_outside_the_box`
`verified-by: bravebot_tui::app::a_mouse_report_at_the_last_coordinate_overflows_nothing`

<a id="MOUSE-17"></a>
### MOUSE-17: a click on a row of the `/theme` picker previews it, and a second takes it

In the `/theme` picker, at rest or while a turn runs, a left press on a row of the list puts the
cursor on it, with the theme under the cursor in force as it is for a key, and a second press on the
same row within the double-click interval takes the theme. As for [MOUSE-15](#MOUSE-15), a press on
the border or under the list, a drag, a release and a press of another button leave the picker open
on the theme it had. The wheel and a press outside the panel are [MOUSE-46](#MOUSE-46) and
[MOUSE-47](#MOUSE-47), and a press outside puts back the theme the picker opened on. The keys stay
the way to do it without a mouse.

`verified-by: bravebot_tui::theme_prompt::a_click_previews_the_row_and_a_second_click_takes_it`
`verified-by: bravebot_tui::theme_prompt::two_presses_on_a_drawn_row_return_its_theme`
`verified-by: bravebot_tui::theme_prompt::a_press_on_the_panel_off_the_list_leaves_the_theme_alone`
`verified-by: bravebot_tui::app::tests::pickers::a_press_on_a_row_of_the_theme_picker_opened_mid_turn_previews_it_and_a_press_outside_puts_the_old_one_back`

<a id="MOUSE-18"></a>
### MOUSE-18: a click on a file's row, the `x` or a control of the bar in the files state acts as the key does

In the files state of the info panel ([FILES-4](files-mode.md#FILES-4)), a left click that sweeps
nothing acts on what the layout drew under it:

- On a file's row of the list it highlights that file, as Up and Down do
  ([FILES-5](files-mode.md#FILES-5)): the edits of another file open at their top, and a click on
  the file that is highlighted leaves its edits where they are scrolled to. The rows of the list
  stay where they were drawn.
- On the `x` at the right end of the heading row, which is the `x` and the space on each side of it,
  it ends the state as `q` does, and the panel goes back to what it was before the state
  ([FILES-3](files-mode.md#FILES-3)).
- On the control for the previous or the next edit in the bar under the file's name, which is the
  control and the space on each side of it, it puts that edit's header at the top, as Left and Right
  do ([FILES-6](files-mode.md#FILES-6)). Two quick clicks on one control take one step, since the
  press that follows a click that acted begins nothing ([MOUSE-50](#MOUSE-50)).

Nothing else is a target: not the rows that count the files left out or the edit rows out of view,
the rest of the heading, the rule, the edits, the position in the bar, a control that is dim, the
border column, a row the height of the panel is too short to draw, or the last row of the panel,
which names the keys. As for [MOUSE-12](#MOUSE-12), the press and the release are on one cell and
the press is a single click, so a drag is a selection and a double click acts once
([MOUSE-50](#MOUSE-50)), and a press that began elsewhere and was let go on a target is not a click
on it. A press on
a row whose file has left the list since the frame was drawn is ignored. A press is mapped by the
rows the layout drew and never by a file's name or an edit's text
([FILES-10](files-mode.md#FILES-10)). The keys stay the way to do it without a mouse.

`verified-by: bravebot_tui::files_mode::a_click_on_a_file_row_highlights_it_and_shows_its_edits_from_the_top`
`verified-by: bravebot_tui::files_mode::a_drag_a_press_from_elsewhere_or_a_double_click_highlights_no_file`
`verified-by: bravebot_tui::files_mode::a_click_on_a_row_that_is_not_a_file_highlights_nothing`
`verified-by: bravebot_tui::files_mode::a_click_on_a_file_that_has_left_the_list_selects_nothing`
`verified-by: bravebot_tui::files_mode::only_the_drawn_file_rows_are_click_targets`
`verified-by: bravebot_tui::files_mode::the_click_targets_of_every_frame_are_exactly_the_controls_it_drew`
`verified-by: bravebot_tui::files_mode::a_click_on_a_row_leaves_the_rows_of_the_list_where_they_were_drawn`
`verified-by: bravebot_tui::files_mode::the_geometry_places_each_row_where_rows_draws_it`
`verified-by: bravebot_tui::files_mode::a_click_on_the_close_control_ends_the_state_as_q_does`
`verified-by: bravebot_tui::files_mode::a_click_on_the_heading_beside_the_close_control_leaves_the_state_open`
`verified-by: bravebot_tui::files_mode::the_close_control_ends_the_heading_row_beside_a_cut_note_about_earlier_edits`
`verified-by: bravebot_tui::files_mode::the_bar_names_the_edit_at_the_top_and_stepping_moves_it`
`verified-by: bravebot_tui::files_mode::left_right_h_and_l_step_between_edits_as_the_clicks_do`
`verified-by: bravebot_tui::files_mode::only_the_enabled_controls_of_the_bar_are_click_targets`
`verified-by: bravebot_tui::files_in_french::a_cut_note_leaves_the_close_control_at_the_end_of_the_heading_row_in_french`

<a id="MOUSE-19"></a>
### MOUSE-19: the wheel scrolls the pane of the files state under the pointer

With the pointer over the panel in the files state ([FILES-4](files-mode.md#FILES-4)), a turn of the
wheel scrolls the pane under it: over the heading, the list and the rule it scrolls the rows of the
list, and over every row under the rule it scrolls the edits. One turn moves the rows
`tui.wheelRows` names ([SCROLL-3](scroller.md#SCROLL-3)), and each pane stops at its ends. Scrolling
the list does not move the highlight, and a key that moves the highlight brings it back into view. A
list that fits its rows has nothing to scroll and stays drawn around the highlight. Over the
transcript, or where the panel is not drawn, the wheel scrolls the transcript. Which pane the
pointer is over is read from the two panes the frame recorded as it drew, the list and the edits,
which are found whatever is drawn on them ([MOUSE-50](#MOUSE-50)).

`verified-by: bravebot_tui::files_mode::the_wheel_over_the_list_scrolls_its_rows_and_moves_nothing_else`
`verified-by: bravebot_tui::files_mode::a_key_that_moves_the_highlight_brings_it_back_into_view`
`verified-by: bravebot_tui::files_mode::the_wheel_over_the_edits_scrolls_them_by_the_rows_a_wheel_event_moves`
`verified-by: bravebot_tui::files_mode::the_wheel_over_the_transcript_scrolls_the_transcript_and_not_the_panel`
`verified-by: bravebot_tui::files_mode::the_wheel_scrolls_the_pane_under_the_pointer_on_every_row_of_every_frame`
`verified-by: bravebot_tui::files_mode::the_geometry_places_each_row_where_rows_draws_it`
`verified-by: bravebot_tui::files_mode::a_wheel_turn_over_a_list_that_fits_leaves_it_following_the_highlight`
`verified-by: bravebot_tui::files_mode::a_wheel_turn_with_the_panel_not_drawn_scrolls_the_transcript_and_nothing_else`

<a id="MOUSE-20"></a>
### MOUSE-20: a click on the Files section of the panel opens the files state

In the ordinary state of the panel, a left click that sweeps nothing on the heading of the Files
section or on the row under it, which reads `{n} files`, does what `/diff` does
([FILES-3](files-mode.md#FILES-3)): it draws the files state, or answers with the note `/diff`
gives. As for [MOUSE-11](#MOUSE-11), the press and the release are on one cell and the press is a
single click, so a drag is a selection and a double click acts once ([MOUSE-50](#MOUSE-50)), and a
click over the scroller or a history search asks for nothing, since the command is typed at the plain
prompt. The
rows around the section are not targets, nor is a heading or a totals row cut for want of room. The
command stays the way to do it without a mouse, and what the click draws is what the session's own
write calls changed ([FILES-10](files-mode.md#FILES-10)).

`verified-by: bravebot_tui::files_mode::a_click_on_the_files_heading_or_the_row_under_it_opens_the_state`
`verified-by: bravebot_tui::files_mode::a_drag_a_double_click_or_a_click_over_the_scroller_or_a_search_opens_no_state`
`verified-by: bravebot_tui::panel::only_the_files_heading_and_its_totals_row_are_click_targets`

<a id="MOUSE-21"></a>
### MOUSE-21: a click on the panel's hide row hides the panel

In the ordinary state of the info panel, a left click that sweeps nothing on the row that names the
key hiding the panel hides it, as the key does ([PANEL-5](info-panel.md#PANEL-5),
[PANEL-7](info-panel.md#PANEL-7)). As for [MOUSE-12](#MOUSE-12), the press and the release are on one
cell and the press is a single click, so a drag is a selection and a double click acts once
([MOUSE-50](#MOUSE-50)), and a click over the scroller or a history search does nothing. The border
column is the resize
handle, no other row is a target for this, and the files state has none
([MOUSE-18](#MOUSE-18)). The key stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_the_panels_hide_row_hides_the_panel`
`verified-by: bravebot_tui::app::a_drag_over_the_hide_row_or_a_click_in_the_scroller_leaves_the_panel_open`
`verified-by: bravebot_tui::panel::only_the_hide_row_is_a_hide_target`

<a id="MOUSE-22"></a>
### MOUSE-22: a click on a language server's row says what `/lsp` says

In the info panel, a left click that sweeps nothing on the row of a started language server says
which language servers the session has started, as `/lsp` does ([CMD-29](commands.md#CMD-29)), and
is carried out as [MOUSE-29](#MOUSE-29) says of a click that stands for a command. As for
[MOUSE-12](#MOUSE-12), the press and the release are on one cell and the press is a single click,
so a drag is a selection, and a click over the scroller or a
history search says nothing. The heading and the rows around the servers are not targets, nor is a
row cut for want of room. The command stays the way to do it without a mouse, and what the click
says is the session's own list of the programs the person approved, with nothing a server said in
it ([PANEL-11](info-panel.md#PANEL-11), [PANEL-13](info-panel.md#PANEL-13)).

`verified-by: bravebot_tui::app::a_click_on_a_panel_row_does_what_the_command_typed_does_at_rest`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_does_what_the_command_typed_does_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_drag_over_a_language_server_row_or_a_click_in_the_scroller_says_nothing`
`verified-by: bravebot_tui::app::a_double_click_on_a_language_server_row_says_it_once`
`verified-by: bravebot_tui::panel::only_a_started_language_servers_rows_are_click_targets`

<a id="MOUSE-23"></a>
### MOUSE-23: a click on a delegate at rest or while work runs opens its view

At the plain prompt, and while a turn or a command runs, a left click that sweeps nothing on a row of
a delegate's block in the transcript opens the view of that delegate, as the watch key does on it
(Ctrl-L, [WATCH-7](watching.md#WATCH-7); [MOUSE-10](#MOUSE-10) is the same click in the scroller). The press and the release are on one cell
and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)), and a press that began elsewhere and was let go on the block is not a click on it. The info
panel is drawn on the same rows as the transcript and is not part of it. The row between two blocks, a
click with a picker, a history search or the files state in use, and a click where a view is already
open open nothing. What decides is the cell clicked and which delegate was drawn there, and nothing
the delegate's task or report says. The delegate is the one the block holds, found by its place
among the delegates the session has started and not by its number, which every turn begins at one
again. The watch key stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_delegate_at_rest_opens_that_delegates_view`
`verified-by: bravebot_tui::app::a_click_on_a_delegate_while_a_turn_runs_opens_its_view`
`verified-by: bravebot_tui::app::a_click_off_a_delegate_or_a_drag_over_one_opens_no_view_at_rest`
`verified-by: bravebot_tui::app::a_click_on_a_delegate_opens_no_view_where_another_mode_is_in_use`
`verified-by: bravebot_tui::app::a_click_opens_the_delegate_of_the_block_when_two_turns_number_theirs_alike`
`verified-by: bravebot_tui::app::a_click_on_every_row_of_a_delegates_block_at_rest_opens_its_view`

<a id="MOUSE-24"></a>
### MOUSE-24: a click on the way back out of a delegate's view leaves it

In a delegate's view, a left click that sweeps nothing on the way back drawn at the right of the
header, or on a row of the sentence that closes the view, leaves the view outright and returns to
what it was opened over: the transcript at rest, or the scroller. It is the close of Ctrl-L
([WATCH-9](watching.md#WATCH-9)) and not the step back of `q`, so a person who opened the view with
a click is one click from where they were whatever the list holds. As for [MOUSE-23](#MOUSE-23), the
press and the release are on one cell and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). The way back answers on the label, on the blank cell before it and on the last
column after it. A sentence that wraps answers on every row it is drawn over. The words under the
sentence, the header's own words and its task are not targets. The label is the interface's own and
not text of the view, so a drag over the header copies the words and not it, and the highlight does
not paint it. The label is drawn where two blank cells after the header's words, the label and the
last column fit, and a header with less room draws none. The keys stay the way to do it without a
mouse.

`verified-by: bravebot_tui::app::a_click_on_the_back_label_leaves_a_delegates_view`
`verified-by: bravebot_tui::app::a_click_on_the_sentence_that_closes_a_delegates_view_leaves_it`
`verified-by: bravebot_tui::app::a_drag_over_the_way_back_or_the_closing_sentence_leaves_the_view_open`
`verified-by: bravebot_tui::app::a_click_on_the_closing_sentence_returns_to_the_scroller_it_was_opened_over`
`verified-by: bravebot_tui::app::a_drag_along_the_header_copies_no_way_back`
`verified-by: bravebot_tui::app::a_header_with_no_room_for_the_label_draws_none`
`verified-by: bravebot_tui::app::the_label_is_drawn_when_two_blanks_and_one_more_cell_fit_beside_the_words`
`verified-by: bravebot_tui::app::the_cells_that_answer_as_the_way_back_are_the_label_and_a_blank_either_side`
`verified-by: bravebot_tui::app::a_click_on_each_row_a_closing_sentence_wraps_over_leaves_the_view`
`verified-by: bravebot_tui::select::a_copy_and_a_highlight_leave_out_the_cells_drawn_for_a_click`

<a id="MOUSE-25"></a>
### MOUSE-25: a click on a call at rest opens the scroller on it, expanded

Expansion is drawn only in the scroller ([SCROLL-10](scroller.md#SCROLL-10)), so at the plain prompt
and while work runs a left click that sweeps nothing on a row of a call that has more to show than it
draws, a result past its glimpse or a write past its twelve lines, opens the scroller with that call
expanded, as Ctrl-O ([SCROLL-1](scroller.md#SCROLL-1)) followed by `x` does on it. The call's heading stays on the screen row it was
drawn on. Where the expanded call would then end below the screen, the view moves on far enough to
show the whole of it, and where the call is taller than the screen its heading is the top row. The
call is laid out at the size the scroller is drawn at and not at the width the transcript has beside
the info panel. In the scroller a click on the call collapses it and another expands it again
([MOUSE-9](#MOUSE-9)). As for [MOUSE-23](#MOUSE-23), the press and the release are on one cell and the
press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). A call with nothing more to show, the info panel on the call's row, a picker, a history
search, the files state and a delegate's view open nothing. What decides is the cell clicked and
whether the call has more to show than it draws, and nothing a result says. The keys stay
the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_call_at_rest_opens_the_scroller_on_it_expanded`
`verified-by: bravebot_tui::app::a_click_on_a_long_write_at_rest_opens_the_scroller_on_every_change`
`verified-by: bravebot_tui::app::a_click_off_a_call_or_a_drag_over_one_opens_no_scroller_at_rest`
`verified-by: bravebot_tui::app::a_click_on_a_call_opens_no_scroller_where_another_mode_is_in_use`
`verified-by: bravebot_tui::app::a_click_on_a_call_while_a_turn_runs_opens_the_scroller_on_it`
`verified-by: bravebot_tui::app::a_click_on_every_row_of_a_call_at_rest_opens_the_scroller_on_it`
`verified-by: bravebot_tui::state::opening_the_scroller_on_a_call_shows_it_where_it_was_or_whole`

<a id="MOUSE-26"></a>
### MOUSE-26: a click on the marker that leads a reply copies it

In the transcript, at the plain prompt, while work runs and in the scroller, a left click that sweeps
nothing on the two cells that lead the first row of a reply, the marker and the blank after it,
copies that reply as `/copy` copies it ([CMD-11](commands.md#CMD-11)): the words the planner wrote
and not the rows the screen drew them as, with every control character but a line break and a tab
left out, and how many characters went drawn at the right of the hint row. Where the reply is under
a line saying which definition answered, the marker is on the row beneath it. A reply still arriving
is not a reply yet, as it is not for `/copy`, so its marker copies nothing until the round ends. A
reply of nothing but blanks has nothing to copy. As for [MOUSE-23](#MOUSE-23), the press and the
release are on one cell and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). The words beside the marker, the rows a wrapped reply continues
on, the info panel on the marker's row, and a picker, a history search, the files state or a
delegate's view open nothing. The loops that run while a turn, a compaction or a command runs carry
the copy out and leave the view where it was. What decides is the cell clicked and which reply was
drawn there. `/copy` stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_replys_marker_copies_that_reply_as_copy_does`
`verified-by: bravebot_tui::app::a_click_off_a_replys_marker_copies_no_reply`
`verified-by: bravebot_tui::app::a_reply_still_arriving_has_no_marker_to_click`
`verified-by: bravebot_tui::app::a_reply_is_copied_by_its_marker_while_a_turn_runs_and_in_the_scroller`
`verified-by: bravebot_tui::app::a_click_on_a_replys_marker_copies_nothing_where_another_mode_is_in_use`
`verified-by: bravebot_tui::app::the_loops_that_run_while_work_does_copy_the_reply_a_click_named`

<a id="MOUSE-27"></a>
### MOUSE-27: a click on the word at the right of a fenced block's header copies the block

A fenced block ([VIEW-38](terminal-transcript.md#VIEW-38)) in a reply that is an entry of the
transcript draws the word `copy` at the right of its header row, one column short of the edge, where
the row has room for it beside what the fence named and the block holds a line. A left click that
sweeps nothing on the cells of the word copies that block alone: the lines between its fences as the
reply wrote them, one line to a line however many rows the screen drew it over, with no fence, no
margin or indent the screen drew and no marker, and with the indent the fence was written at taken
off. It goes the way `/copy` and a sweep go ([CMD-11](commands.md#CMD-11)), to the clipboard tool
first and to the terminal by OSC 52 otherwise, with every control character but a line break and a
tab left out and the count of characters drawn at the right of the hint row. The word is the
interface's own and no part of the text around it: a drag over the block copies its header and lines
and not the word, the highlight passes over it, a search in the scroller finds the words of a reply
and neither the word nor the blanks before it, and `v` leaves it out of the rows it hands the editor
([SCROLL-6](scroller.md#SCROLL-6)). As for [MOUSE-23](#MOUSE-23), the press and the
release are on one cell and the press is a single click, so a drag is a selection
([MOUSE-1](#MOUSE-1)). The cells beside the word, the name the fence gave, the lines of
the block, the info panel on the header's row, and a picker, a history search, the files state or a
delegate's view open nothing. It works at the plain prompt, while work runs and in the scroller. A
reply still arriving draws no word, since it is not something to copy yet, and a block nobody closed
is the rest of the reply once it has ended. A block with nothing in it, or with nothing but blank
lines, has no word. `/copy block` is the way to do it without a mouse
([CMD-11](commands.md#CMD-11)): it copies the same words, and the blocks it counts back from the latest
are the blocks that hold words, whether or not the row has room to draw the word.

`verified-by: bravebot_tui::app::a_click_on_a_blocks_copy_word_copies_the_block_alone`
`verified-by: bravebot_tui::app::a_click_off_a_blocks_copy_word_copies_no_block`
`verified-by: bravebot_tui::app::a_drag_over_a_block_does_not_copy_its_copy_word`
`verified-by: bravebot_tui::app::a_block_in_a_reply_still_arriving_has_no_copy_word_until_it_ends`
`verified-by: bravebot_tui::app::a_blocks_copy_word_works_in_the_scroller_and_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_click_on_a_blocks_copy_word_copies_nothing_where_another_mode_is_in_use`
`verified-by: bravebot_tui::render::transcript::the_word_that_copies_a_block_sits_where_the_layout_says_at_every_width`
`verified-by: bravebot_tui::render::a_search_finds_what_a_reply_says_and_not_the_word_that_copies_its_block`
`verified-by: bravebot_tui::app::a_typed_block_command_copies_the_words_the_click_on_that_block_copies`
`verified-by: bravebot_tui::app::a_typed_block_command_runs_mid_turn_and_counts_the_blocks_that_have_a_word`

<a id="MOUSE-28"></a>
### MOUSE-28: a click on a prompt waiting behind a turn takes it back into the box

A prompt typed and sent while a turn runs waits beneath the box in two rows, its words and its mark
([INPUT-10](terminal-input.md#INPUT-10)). A left click that sweeps nothing on either row takes that
prompt back into the box for editing, together with every prompt typed after it, one to a line in the
order they were typed. A prompt comes back only with the ones after it, because the queue gives up
its lines from the end the person types at, so a click on the oldest is Up and takes them all
([INPUT-18](terminal-input.md#INPUT-18)). What was in the box stays below what comes back, what each
prompt named is staged again, the prompts stay in the history, and what came back is out of the
turn's reach, as for the key. A prompt the turn has already been given does not come back, and the
click leaves the box as it was. It is a take-back and not a cancel: the words are in the box, and
clearing the box is what cancels them. A command a click on the info panel queued
([MOUSE-29](#MOUSE-29)) is no prompt anybody typed, so it does not come back: it stays waiting, in its
place among what is left, since in the box Enter would send it to the planner as words. As for
[MOUSE-23](#MOUSE-23), the press and the release are
on one cell and the press is a single click, so a drag is a selection ([MOUSE-1](#MOUSE-1)). The rows beside the queue, the info panel on the same rows, and a
picker or a history search open nothing. The rows are recorded as drawn, so a line put away or a file
attached above the queue moves them with it. The key stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_waiting_prompt_takes_it_and_the_ones_after_it_back_into_the_box`
`verified-by: bravebot_tui::app::a_click_on_the_oldest_waiting_prompt_leaves_what_the_key_leaves`
`verified-by: bravebot_tui::app::a_click_on_the_newest_waiting_prompt_takes_that_one_and_keeps_the_line_in_the_box`
`verified-by: bravebot_tui::app::a_click_off_a_waiting_prompt_or_a_drag_over_one_takes_nothing_back`
`verified-by: bravebot_tui::app::a_click_on_a_prompt_the_turn_has_taken_leaves_the_box_alone`
`verified-by: bravebot_tui::app::a_click_on_a_waiting_prompt_takes_nothing_back_where_another_mode_is_in_use`
`verified-by: bravebot_tui::app::a_line_put_away_above_the_queue_does_not_move_what_a_click_takes`
`verified-by: bravebot_tui::app::a_file_attached_above_the_queue_does_not_move_what_a_click_takes`
`verified-by: bravebot_tui::state::taking_back_from_a_place_in_the_queue_gives_back_that_prompt_and_the_ones_after`
`verified-by: bravebot_tui::state::a_click_cannot_take_back_what_the_turn_has_already_been_given`
`verified-by: bravebot_tui::state::a_command_line_a_click_queued_is_not_given_back_to_the_box`
`verified-by: bravebot_tui::app::a_click_queued_command_stays_waiting_when_the_prompts_come_back`

<a id="MOUSE-29"></a>
### MOUSE-29: a click that stands for a command does what the command typed would do where it is

A click on the panel's model, effort, context, MCP server or language server row or on its Files
section ([MOUSE-11](#MOUSE-11), [MOUSE-12](#MOUSE-12), [MOUSE-13](#MOUSE-13), [MOUSE-22](#MOUSE-22),
[MOUSE-20](#MOUSE-20)) is carried out as Enter on the command it stands for would be, and leaves the
box as it was. At the plain prompt that is what Enter does there. While a
turn runs it is carried out at once where the table says the command runs mid-turn and no line
waiting ahead of it could change what it acts on ([CMD-8](commands.md#CMD-8)): `/model` and
`/effort` ask for their picker as a mode of the session, `/context` is said as a command's
answer is, so what the turn did is not counted as what it said, and `/diff` draws the files state or
says why it cannot in the same way. Otherwise it waits in the queue
with the note a typed line gets, as `/mcp` does, which waits for the turn to end. Where the session
is working and no turn is running, as under a summary or an aside, every command waits, and so it does
while a command the person typed is running, which reads no line typed after it and has nothing to
carry a click out with. It waits in the queue with a note that says it waits for the command to end,
and is carried out as Enter on it would be once the command has ended. A click is never dropped, and
the line it queues is not remembered as one that was typed. While a
picker is open the click is on the picker and asks for nothing, as it does over the scroller and a
history search. The command stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_panel_row_does_what_the_command_typed_does_at_rest`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_does_what_the_command_typed_does_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_queues_the_command_where_no_turn_is_running`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_waits_for_the_command_that_is_running`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_leaves_a_line_being_typed_in_the_box`
`verified-by: bravebot_tui::app::a_click_on_a_panel_row_under_an_open_picker_asks_for_nothing`

<a id="MOUSE-30"></a>
### MOUSE-30: a click on the session's name puts the command that renames it in the box

In the info panel, a left click that sweeps nothing on a row of the session's name puts `/rename`
and the whole name in the box, with the caret after it, for the person to edit and send with Enter,
which is how `/rename` is carried out ([CMD-8](commands.md#CMD-8) says what it does while a turn
runs). The click renames nothing and sends nothing. The name is the session's own, which comes from
a prompt or from `/rename` ([PANEL-11](info-panel.md#PANEL-11)), and it is the whole name and not
the rows the panel drew: a long name is wrapped there, and an address in it is drawn as a shorthand.
Control characters in it are left out, since no key types one, but for a tab, which the panel
draws as a space and which is one here. The box takes the line where typing the command would put it:
empty, in the ordinary style, outside shell mode. Over a line somebody has begun, in shell mode and in
the vi style of box, the box is left as it was and a note says the line is in use, so a draft is never
replaced. While a turn runs the note is said as a command's answer is, so stopping the turn gives its
prompt back as it does when nothing was clicked ([CMD-8](commands.md#CMD-8)). As for
[MOUSE-12](#MOUSE-12), the press and the release
are on one cell and the press is a single click, so a drag is a selection, and a click over the scroller or a history search puts nothing in the box. The heading,
the directory, the branch and the id are not targets. `/rename` stays the way to do it without a
mouse.

`verified-by: bravebot_tui::app::a_click_on_the_session_name_puts_rename_and_the_name_in_the_box`
`verified-by: bravebot_tui::app::a_click_on_the_session_name_while_a_turn_runs_puts_rename_in_the_box`
`verified-by: bravebot_tui::app::a_name_with_control_characters_is_put_in_the_box_without_them`
`verified-by: bravebot_tui::app::a_tab_in_the_name_is_put_in_the_box_as_the_space_the_panel_draws`
`verified-by: bravebot_tui::app::the_note_of_a_click_on_the_name_while_a_turn_runs_is_not_something_the_turn_did`
`verified-by: bravebot_tui::app::a_click_that_cannot_put_rename_in_the_box_leaves_it_alone`
`verified-by: bravebot_tui::app::a_double_click_on_the_session_name_puts_rename_in_the_box_once`
`verified-by: bravebot_tui::panel::only_the_rows_of_the_sessions_name_are_name_targets`

<a id="MOUSE-31"></a>
### MOUSE-31: a click on a task of the plan moves the view to the turn that wrote it

In the info panel, a left click that sweeps nothing on a task of the plan moves the transcript to
where the turn that wrote the plan drew that task, and puts that row at the top of the view, as `{`
and `}` do for a prompt ([SCROLL-3](scroller.md#SCROLL-3)). A plan whose turn has ended is drawn in
the transcript under that turn's last entry, one line for each task ([PANEL-10](info-panel.md#PANEL-10)),
and the line of the task clicked is the row. The plan the turn in flight has reported is drawn above
the box and not in the transcript, so the view goes to the end of the transcript, where that turn's
words are. The row is worked out from where the plan is held and how the transcript wraps, laying
out the entry that holds the plan and taking the rows above it from the frame, and nothing in a
task's words decides it. No task belongs to a delegate: a delegate is offered no list
to write to ([DELEGATE-12](delegation.md#DELEGATE-12)), so a click never opens a delegate's view. As
for [MOUSE-12](#MOUSE-12), the press and the release are on one cell and the press is a single click,
so a drag is a selection, and a click over the scroller or a history search moves nothing. The
heading, the row that counts what was left out and the row that names the key are not targets, and
the rows of a plan longer than the room are the tasks they draw and not their places among the rows.
The keys that move the view reach the turn that wrote the task, a prompt at a time
with `{` and `}` in the scroller, and the scroller's search reaches the task's words, but no key puts
the task's own row at the top, which is what the click adds.

`verified-by: bravebot_tui::panel::a_click_on_a_task_moves_the_view_to_the_row_its_turn_drew_it_on`
`verified-by: bravebot_tui::panel::a_click_on_a_task_of_a_plan_longer_than_the_room_goes_to_that_task`
`verified-by: bravebot_tui::panel::a_click_on_a_task_of_the_turn_in_flight_moves_the_view_to_the_end`
`verified-by: bravebot_tui::panel::a_click_that_is_not_on_a_task_leaves_the_view_where_it_was`
`verified-by: bravebot_tui::render::a_click_on_a_task_measures_the_entry_that_holds_the_plan_and_not_the_entries_above`

<a id="MOUSE-32"></a>
### MOUSE-32: a click on the mark of a finished task opens the whole of its words

The info panel draws a finished task ([PANEL-10](info-panel.md#PANEL-10)) on one row, its words cut
to the row and struck through. A left click that sweeps nothing on the mark of such a task, the
glyph and the space on each side of it, opens the whole of its words in place: wrapped to the row
over as many rows as they take, up to six, the last cut where more was left, and no longer struck
through, so that they can be read. A click on the mark again, or on a row of the open words below
the first, closes them: the rest of the first row is the task's own, and goes to the task
([MOUSE-31](#MOUSE-31)). Every finished task has a mark that does this, whether its words fit the
row or not: which mark opens is read from the task's status and never from its words. A click on the
rest of a task's row goes to the task ([MOUSE-31](#MOUSE-31)), and so does a click on the mark of a
task still to do, in progress or dropped. The detail is the task's own words, which the planner
wrote and the panel already draws, and the click puts nothing else in the panel
([PANEL-11](info-panel.md#PANEL-11)). Once the turn that wrote the plan has ended the words are
drawn whole in the transcript under it, so what a click shows is not only there. The rows an open
task takes come out of the room the plan has ([PANEL-10](info-panel.md#PANEL-10)): the tasks below
it move down, finished tasks that are not open are left out first, the rows still to do stay drawn,
and the row that counts what was left out follows. Which tasks are
open is kept by their place in the plan as it stands, so a later report, a new turn and anything
that empties the plan close every one, and the turn that wrote the plan ending leaves them as they
were. As for [MOUSE-12](#MOUSE-12), the press and the release are on one cell and the press is a
single click, so a drag is a selection, and a click over the scroller or a history search opens
nothing.

**The keyboard.** The mark has no key. Once the turn that wrote the plan has ended, the transcript
holds the words of a task whole under it, which the keys that move the view and the scroller's
search reach ([SCROLL-3](scroller.md#SCROLL-3)). While that turn runs the plan the panel draws is
the list it has reported, which is in no entry of the transcript yet ([MOUSE-31](#MOUSE-31)), so the
mark is the one way to read a task's whole words then. Opening them changes only how many rows of
the panel are drawn.

`verified-by: bravebot_tui::panel::the_mark_of_a_finished_task_opens_the_whole_of_its_words_and_closes_them_again`
`verified-by: bravebot_tui::panel::the_mark_is_the_glyph_and_its_two_blanks_and_the_words_open_to_six_rows`
`verified-by: bravebot_tui::panel::only_the_mark_of_a_finished_task_opens_its_words`
`verified-by: bravebot_tui::panel::an_open_task_takes_rows_from_the_plan_and_the_count_follows`
`verified-by: bravebot_tui::panel::what_was_opened_is_closed_when_the_plan_is_replaced_or_dropped`
`verified-by: bravebot_tui::panel::a_drag_on_a_mark_or_a_click_on_it_over_the_scroller_leaves_the_task_closed`
`verified-by: bravebot_tui::panel::a_double_click_on_a_mark_opens_the_words_once`

<a id="MOUSE-33"></a>
### MOUSE-33: no click puts what a tool returned in the panel

A click on a target of the info panel acts on what the person or the driver put there, and puts
nothing new there ([PANEL-11](info-panel.md#PANEL-11)). Whichever target is clicked, at the plain
prompt and while a turn runs, the cells the panel draws afterwards, what the click says in the
transcript and what it puts in the box are made from the inputs that clause lists, and none holds a
word of what a tool returned, of what a call showed or of a reply. A task's detail is the task's own
words ([MOUSE-32](#MOUSE-32)), the place a task's click goes to is read from where the plan is held
([MOUSE-31](#MOUSE-31)), and the commands the clicks stand for say what they say at the keyboard
([MOUSE-29](#MOUSE-29)).

`verified-by: bravebot_tui::app::a_click_on_any_target_of_the_panel_never_draws_what_a_tool_returned`

<a id="MOUSE-34"></a>
### MOUSE-34: a selection in the transcript stays on the text it was put on

The transcript scrolls, so a selection that began in it counts the transcript's own rows and the
screen's columns. Scrolling with the wheel or the scroller's keys, and the rows a turn adds below,
move the view, and the highlight moves with the words it was put on. Only the rows in view are
marked, so a selection can have an end that is out of view, and the copy reads every row it
reaches ([MOUSE-35](#MOUSE-35)). A press on the transcript begins such a
selection wherever the transcript is drawn that takes a press: in the plain view, the scroller and a
delegate's view. A history search holds the mouse ([MOUSE-47](#MOUSE-47)), so a press there begins
none. A press on the rest of a view that records no panes begins a selection of screen rows
([MOUSE-1](#MOUSE-1)). The layout counts the rows of the transcript in sixteen bits, so a transcript
of 65535 rows or more has rows that cannot be told apart, and a press on it counts the rows of the
screen as a press in a pane that does not scroll does: the copy is what the screen showed.

The rows are those of one layout, so the selection is dropped when the transcript is drawn at
another width or in another view (the scroller, a delegate's view or `/request`), when it has fewer
rows than the selection reaches, and when `x` expands or collapses a call ([SCROLL-10](scroller.md#SCROLL-10)),
which changes how many rows the call takes. Opening and closing the info panel and resizing the
terminal drop it as [MOUSE-3](#MOUSE-3) says. What the frame marks is placed by the view that frame
draws, so the frame that follows a wheel turn marks the words and not the rows they were on. A press
or a drag handled before that frame is placed by the view as the wheel left it, and selects what is
under the pointer then.

The rows are also told by the text above them. Each end of the selection is told by the text of every
line above the line its row is in, its place among the rows of that line, and the words that line
began with when the end was set. The first frame after a press or a drag reads them. When a frame
draws either end's row with another telling, as when a call above it finishes and takes more rows,
thinking is taken down above the reply it was drawn over, or a line is put in at its place, the
selection is dropped and that frame marks nothing, so a copy is the text that was swept or nothing.
What is written below a selection leaves it alone. So does the line an end is on getting longer, since
the words it began with are where they were, and so does text drawn over a line, as a search match is.
An end that is below the text is on the last row that holds text ([MOUSE-1](#MOUSE-1)), so a reply
being written above the blank row that closes the transcript leaves it alone in the plain view, in
the scroller and with the view scrolled back to the tail by a drag held below it
([MOUSE-37](#MOUSE-37)).
So does a word the interface draws on a line for a click, as the one that copies a fenced block, which
a header gains when its reply ends: it is no text of the reply, and nor are the blanks drawn before it.
The opening screen, whose face moves, is one telling until the first entry ends it.

A delegate's view is told by the delegate's number and not by its place in the list the view opens,
which moves with every row put ahead of it while its lines stay where they are, so a selection of
those lines is kept when one is, and dropped when the view steps to another delegate.

`verified-by: bravebot_tui::select::a_transcript_selection_stays_on_its_text_as_the_view_scrolls`
`verified-by: bravebot_tui::select::a_selection_with_an_end_out_of_view_is_painted_and_read_where_it_is_in_view`
`verified-by: bravebot_tui::select::a_word_taken_in_the_transcript_follows_the_view_too`
`verified-by: bravebot_tui::select::a_transcript_selection_does_not_follow_rows_that_hold_other_text`
`verified-by: bravebot_tui::select::a_selection_outside_the_buffer_reads_and_paints_nothing`
`verified-by: bravebot_tui::select::a_mark_tells_a_row_by_the_text_above_it_and_the_words_its_line_began_with`
`verified-by: bravebot_tui::select::what_is_above_a_line_is_told_by_the_text_of_each_line_above_it`
`verified-by: bravebot_tui::select::a_mark_leaves_out_the_cells_drawn_for_a_click_and_the_blanks_before_them`
`verified-by: bravebot_tui::select::the_first_frame_sets_what_an_end_is_told_by_and_a_later_one_must_agree`
`verified-by: bravebot_tui::app::a_call_finishing_above_a_selection_does_not_leave_it_over_other_text`
`verified-by: bravebot_tui::app::a_call_finishing_above_a_selection_in_a_delegates_view_does_not_leave_it_over_other_text`
`verified-by: bravebot_tui::app::thinking_taken_down_above_a_selection_does_not_leave_it_over_other_text`
`verified-by: bravebot_tui::app::calls_finishing_and_a_reply_arriving_below_a_selection_leave_it_alone`
`verified-by: bravebot_tui::app::a_selection_ending_on_a_line_still_being_written_survives_the_line_growing`
`verified-by: bravebot_tui::app::tests::selection::a_selection_ended_below_a_reply_being_written_survives_its_next_words_in_the_scroller`
`verified-by: bravebot_tui::app::tests::selection::a_drag_held_below_a_reply_being_written_that_scrolls_to_its_tail_keeps_the_selection`
`verified-by: bravebot_tui::app::tests::selection::a_change_above_an_end_below_a_reply_being_written_still_puts_the_selection_away`
`verified-by: bravebot_tui::app::a_reply_becoming_an_entry_keeps_a_selection_of_its_rows`
`verified-by: bravebot_tui::app::a_reply_with_a_block_becoming_an_entry_keeps_a_selection_from_its_header_down`
`verified-by: bravebot_tui::app::a_prompt_queued_under_the_box_leaves_a_selection_of_the_transcript_held`
`verified-by: bravebot_tui::app::a_search_over_a_selection_does_not_drop_it`
`verified-by: bravebot_tui::app::a_row_put_ahead_of_the_watched_delegate_leaves_a_selection_of_its_lines_alone`
`verified-by: bravebot_tui::app::stepping_to_another_delegate_drops_a_selection_of_the_one_before`
`verified-by: bravebot_tui::app::a_selection_on_the_opening_screen_goes_when_the_opening_ends`
`verified-by: bravebot_tui::app::a_wheel_turn_leaves_the_selection_on_the_text_it_was_put_on`
`verified-by: bravebot_tui::app::the_scrollers_keys_leave_the_selection_on_the_text_it_was_put_on`
`verified-by: bravebot_tui::app::the_keys_that_scroll_at_rest_leave_the_selection_on_the_text_it_was_put_on`
`verified-by: bravebot_tui::app::a_click_that_sweeps_nothing_reaches_its_target_while_a_selection_of_rows_is_held`
`verified-by: bravebot_tui::app::a_click_that_sweeps_nothing_reaches_a_link_a_path_a_block_and_the_panel_while_rows_are_held`
`verified-by: bravebot_tui::app::rows_a_turn_adds_below_do_not_move_the_selection_off_its_text`
`verified-by: bravebot_tui::app::a_release_straight_after_a_wheel_turn_copies_the_text_that_was_swept`
`verified-by: bravebot_tui::app::tests::selection::a_drag_straight_after_a_wheel_turn_selects_the_text_under_the_pointer_now`
`verified-by: bravebot_tui::app::tests::selection::a_press_straight_after_a_wheel_turn_begins_on_the_text_under_the_pointer_now`
`verified-by: bravebot_tui::app::a_drag_that_began_with_the_view_scrolled_back_selects_the_rows_under_it`
`verified-by: bravebot_tui::app::a_transcript_that_shrinks_drops_a_selection_that_reached_what_went`
`verified-by: bravebot_tui::app::opening_the_scroller_drops_a_selection_held_in_the_plain_view`
`verified-by: bravebot_tui::app::expanding_a_call_drops_a_selection_held_in_the_scroller`
`verified-by: bravebot_tui::app::tests::selection::a_transcript_of_more_rows_than_a_layout_counts_is_selected_by_the_rows_of_the_screen`

<a id="MOUSE-35"></a>
### MOUSE-35: a selection of the transcript copies every row it reaches

A selection of the transcript that reaches rows that are not on the screen copies them, in order,
with the rows that are. The rows are read from the transcript's own rendering: the lines released
for display, wrapped by the same widget at the same width as the screen. A row read from it is the
row the screen shows for it while it is in view, so the same row gives the same text whether it is
in view or not. Whatever the row is read from, a line the wrap cut is joined to the next as
[MOUSE-8](#MOUSE-8) says, trailing blanks are dropped, and none of the info panel's text is in it,
since the selection belongs to the transcript. The highlight covers the rows that are on the screen
([MOUSE-34](#MOUSE-34)).

Only what the draw shows is read. Nothing comes from the session's state or the conversation, so a
row the screen could never have shown is never copied. A copy lays the transcript's lines out once,
which costs in proportion to the transcript, and draws the rows of the selection 256 at a time, so
what it holds drawn at once is bounded and does not grow with the selection. A line the wrap cut over
more rows than a window holds is joined across the windows as it is within one.

`verified-by: bravebot_tui::app::a_selection_longer_than_the_screen_copies_in_full`
`verified-by: bravebot_tui::app::a_selection_wholly_out_of_view_copies_its_rows_and_nothing_of_the_panel`
`verified-by: bravebot_tui::app::a_row_reads_the_same_on_the_screen_and_off_it`
`verified-by: bravebot_tui::app::a_whole_transcript_copies_as_the_lines_it_is_made_of`
`verified-by: bravebot_tui::app::tests::selection::a_line_that_wraps_over_several_windows_of_rows_copies_as_one_line`
`verified-by: bravebot_tui::select::a_selection_longer_than_the_rows_drawn_at_once_copies_each_row_once_in_order`
`verified-by: bravebot_tui::select::rows_either_side_of_a_boundary_between_windows_join_as_they_do_within_one`
`verified-by: bravebot_tui::select::a_selection_of_rows_not_on_the_screen_is_cut_at_its_ends_and_takes_whole_words_and_rows`

<a id="MOUSE-36"></a>
### MOUSE-36: a character drawn two cells wide copies once

A character that takes two columns fills the first and leaves the second blank. A copy takes the
character once and nothing for the second cell, in every pane and for rows read from the
transcript's rendering ([MOUSE-35](#MOUSE-35)) as for rows read off the screen. A selection that
begins on the second cell begins after the character, and one that ends on the first cell includes
it. The highlight marks both cells of a character the copy takes and neither cell of one it leaves
out.

`verified-by: bravebot_tui::select::a_character_two_cells_wide_is_copied_once`
`verified-by: bravebot_tui::select::what_a_highlight_marks_of_a_character_two_cells_wide_is_what_a_copy_takes`
`verified-by: bravebot_tui::app::a_drag_over_wide_characters_in_the_box_copies_them_once`
`verified-by: bravebot_tui::app::a_whole_transcript_copies_as_the_lines_it_is_made_of`

<a id="MOUSE-37"></a>
### MOUSE-37: a drag held outside the transcript scrolls it and extends the selection

While the left button is held and has moved a selection of the transcript, a pointer past the top
or the bottom of the transcript scrolls the view toward it, and the end of the selection that
follows the pointer goes to the row that comes into view, so a selection can be longer than one
screen. Past the bottom is the status line, the box, what is offered beneath it and the footer, or
the footer where the scroller is open. Past the top is above the first row, and the first row of a
transcript that begins on the first row of the screen, since a terminal reports a pointer dragged out
of its window at the nearest cell inside it.

The view does not scroll for a pointer inside the transcript, for a press that has not moved, after
the button is let go or a pointer moves with no button down, for another button, after the window
loses focus, for a selection of any other pane, or where the view is already at the first row or at
the tail.

A pointer held still sends no report, so each loop that reads the mouse steps the view on its own
clock: at rest, while a turn runs and while a command runs. A step is owed for each 40 milliseconds
since the last, up to three to a loop that was away, and moves one row for a pointer one row past
the edge and up to four rows for one further. The view scrolls in the scroller as it does at rest.
At the tail the view cannot scroll further and follows what a turn writes, so a pointer held below
the transcript keeps the end of the selection on the last row in view that holds text, and what is
copied reaches the rows the turn goes on to add. Rows added below a view that is held do not move the selection
([MOUSE-34](#MOUSE-34)). The wheel and the keys scroll the view with no drag held.

`verified-by: bravebot_tui::app::a_drag_held_below_the_transcript_scrolls_it_and_extends_the_selection`
`verified-by: bravebot_tui::app::a_drag_held_on_the_first_row_scrolls_the_transcript_back_and_extends_the_selection`
`verified-by: bravebot_tui::app::a_drag_held_below_a_transcript_a_turn_is_writing_under_follows_it_to_the_tail`
`verified-by: bravebot_tui::app::tests::selection::a_drag_held_below_a_reply_being_written_that_scrolls_to_its_tail_keeps_the_selection`
`verified-by: bravebot_tui::app::nothing_scrolls_unless_a_held_drag_is_past_an_edge_of_the_transcript`
`verified-by: bravebot_tui::app::a_view_at_the_end_the_drag_is_past_stays_where_it_is`
`verified-by: bravebot_tui::app::the_steps_owed_follow_the_time_and_the_distance_past_the_edge`
`verified-by: bravebot_tui::select::a_pointer_is_past_an_edge_of_the_transcript_only_beyond_it_or_where_the_screen_ends`
`verified-by: bravebot_tui::app::tests::selection::every_loop_that_answers_the_mouse_steps_a_drag_held_outside_the_transcript`

<a id="MOUSE-38"></a>
### MOUSE-38: Escape puts a selection away before it does anything else

A selection that has taken something in stays marked after the copy, until the next click. It is held
whether or not its words are in view, as when the view has been scrolled past them, and nothing is
marked then. Escape puts it away, and the count of what was copied with it, and does nothing else. It is the nearest
thing there is to put away, as [SCROLL-1](scroller.md#SCROLL-1) orders them: with one held, the
first Escape does not clear the box, leave INSERT in vi's style, abandon an enhancement, stop a turn
or a command, close the scroller, a history search, a delegate's view or the files state, and the
press after it does what Escape does there. At rest, while a turn runs and in each of those modes the
order is the same. A picker standing over the session takes the key before the selection does. A
press that swept nothing holds no selection, and Escape does what it did.

A turn in flight is stopped by the second press of Escape, or at once by Ctrl-C, which a selection
does not take ([INPUT-4](terminal-input.md#INPUT-4)).

`verified-by: bravebot_tui::app::escape_puts_a_held_selection_away_before_it_clears_the_box`
`verified-by: bravebot_tui::app::escape_puts_a_held_selection_away_before_it_stops_a_turn_and_the_next_stops_it`
`verified-by: bravebot_tui::app::escape_puts_a_held_selection_away_before_a_mode_takes_it`
`verified-by: bravebot_tui::app::escape_is_not_taken_by_a_click_and_a_picker_takes_it_before_a_selection`
`verified-by: bravebot_tui::app::tests::selection::escape_puts_a_held_selection_away_before_it_stops_a_command_and_the_next_stops_it`
`verified-by: bravebot_tui::app::tests::selection::escape_puts_a_held_selection_away_before_the_scroller_under_a_command_and_the_command`
`verified-by: bravebot_tui::app::tests::selection::escape_puts_a_held_selection_away_before_it_stops_a_request_or_a_goal_check`
`verified-by: bravebot_tui::app::tests::selection::escape_puts_a_held_selection_away_before_the_files_state_and_a_delegates_view_take_it`
`verified-by: bravebot_tui::app::tests::selection::escape_puts_away_a_selection_of_the_status_line_or_the_offered_list_first`

<a id="MOUSE-39"></a>
### MOUSE-39: the notice of a copy names the pane where that is not the transcript

The notice that says how much a copy took keeps its count, as `106 chars to clipboard` and
`1 char to clipboard`. After a copy from the info panel, the box, the status line, the list under
the box or the footer it adds where it came from, as `106 chars to clipboard, from the panel`. A drag
there is the surprise, and the name says which text is on the clipboard. After a copy from the
transcript, which is where a drag is expected, and after one from a view that records no panes, it
says the count and nothing more. A reply copied with `/copy` names no pane. The notice goes at the
next click, the next prompt or Escape ([MOUSE-38](#MOUSE-38)). Each sentence is a
message in the catalog, with its count.

`verified-by: bravebot_tui::render::a_copy_names_the_pane_it_came_from_unless_that_is_the_transcript`
`verified-by: bravebot_tui::render::a_copy_of_one_character_from_a_named_pane_reads_naturally`
`verified-by: bravebot_tui::render::a_copy_says_how_much_it_took`
`verified-by: bravebot_tui::app::the_notice_of_a_copy_names_the_pane_it_was_taken_from`

<a id="MOUSE-40"></a>
### MOUSE-40: a click with ctrl held on a web address in the assistant's words opens it in the browser

A web address in the assistant's words is underlined, and a left click on it with ctrl held that
sweeps nothing starts the person's browser at it. The assistant's words are the replies in the
transcript, in the scroller and in a delegate's view, and the reply still arriving. What a person
typed, a note, a failure, a program's output, a quarantined block ([VIEW-3](terminal-transcript.md#VIEW-3)),
a call, a table cell and the answer an aside shows are text, as they are for
[VIEW-32](terminal-transcript.md#VIEW-32). So are the lines of a fenced block of a reply, which are drawn
as they were written and are not scanned.

An address is `http` or `https`, matched without regard to case, at the start of a word: after a blank
or at the start of the line, or after `*`, `_`, `~`, `(`, `[`, `<`, `"` or `'`. It runs to the next
blank, `<`, `>`, `"` or backtick, or to the `](` that ends the text of a markdown link and opens its
target, less the punctuation a sentence puts after it (`.`, `,`, `:`, `;`, `!`, `?`, `'`, `*`, `_`, `~`)
and a closing bracket with no opening one before it. So `(https://example.test/a)` is the address
inside the brackets, `https://example.test/a_(b)` is the whole, and
`[https://example.test/a](https://example.test/a)` holds two addresses, the text and the target. It has
a host, which starts with a letter or a digit, and is printable ASCII throughout, with no
backslash: a run with any other character in it, such as a letter with an accent or a direction
override, is not an address at all, since what follows could be a lookalike of the host or hide it. A
browser reads a backslash as a slash, so the host of `https://evil.test\.github.com/` is `evil.test`
and not what it reads like. An address naming a user,
as in `https://github.com@example.test/`, is not one, because the part a person reads first is not the
host. A span of inline code is an address when it holds nothing else. What is inside an address is not
markdown.

What opens is what the cells show. The matcher marks the characters of an address as it styles a
line, the frame is drawn, and the marks are read off the cells the widget put them in. The address is
those cells' characters, joined across the rows of one wrapped line where the wrap fell inside the
address and not across a row the wrap broke at a blank, and it is tested once more as the opener tests
an address ([CHATGPT-13](chatgpt-sign-in.md#CHATGPT-13)). An address only partly on the screen, because
its line begins above the area or it runs to the last column of the last row, is not one. A cell
linked by an escape sequence to another address, as an issue number is
([VIEW-32](terminal-transcript.md#VIEW-32)), is not read: the address in the text is the only one that
opens. The cells of the address a click found are the cells the record of the frame holds for it
([MOUSE-50](#MOUSE-50)), which a pointer over the screen is told from as well
([MOUSE-51](#MOUSE-51)), so what it says it would open and what a click opens are the same.

The press and the release are on one cell, the press is a single click, nothing was swept and ctrl is
held as the button is let go, whether or not shift or alt is held with it. A click without ctrl, a
drag, and a double click and a triple click made without ctrl are selections, and a click on the
blank beside an address opens nothing. The rest of a double click made with ctrl, which opened the
address once, does nothing ([MOUSE-50](#MOUSE-50)). Nothing opens
while a history search is open, under a picker or under the scroller's list of keys, which cover the
cells. It opens while a turn or a command runs, since it changes nothing the work holds. Terminals do
not report the Cmd key, so the modifier is ctrl, and a terminal that keeps ctrl-click for its own menu
never reports it.

The browser is started by `bravebot_tui::opener`, which `auth login chatgpt` uses as well
([SANDBOX-30](sandboxing.md#SANDBOX-30)). Where it could not be started the person is told so in a note
that does not repeat the address, which is text the assistant wrote.

**What decides.** The person's click, on cells they can see underlined. The matcher decides only which
cells are underlined and where a click lands, and what it reads is text already released for the screen
([layering.md](layering.md#LAYER-1)): nothing in the driver reads an address, and no address chooses an
effect. The address is the routing of the browser start, and its integrity is not what authorises it:
the click is the person's endorsement of the exact address as it is drawn, which a person can approve
alone since it is all they are asked to approve ([ROUTE-2](routing.md#ROUTE-2),
[ROUTE-8](routing.md#ROUTE-8)), and nothing relabels the address on the way. Opening it in the person's
own browser is not a fetch by bravebot, so the network rules that decide what bravebot fetches
([fetch-url.md](tools/fetch-url.md)) have nothing to say about it.

**The keyboard.** `/open` lists the web addresses and file paths of the latest reply, `/open <number>`
opens an address of that list in the browser, and `/open <address>` opens an address typed after it,
which passes the check the click's address does. Each is carried out by the function the click uses and
says the same where no browser could be started ([CMD-30](commands.md#CMD-30)). They work while a turn or
a command runs, as the click does. What decides is the person's typed command naming a number in a list
they read or the exact address, and nothing in the address.

`verified-by: bravebot_tui::link_clicks::a_ctrl_click_on_an_address_in_a_reply_asks_for_the_browser`
`verified-by: bravebot_tui::link_clicks::what_opens_is_the_address_and_not_the_sentence_around_it`
`verified-by: bravebot_tui::link_clicks::a_click_without_ctrl_a_drag_or_a_double_click_opens_nothing`
`verified-by: bravebot_tui::link_clicks::an_address_wrapped_over_rows_opens_whole_from_any_of_them`
`verified-by: bravebot_tui::link_clicks::an_address_outside_the_assistants_words_is_not_a_link`
`verified-by: bravebot_tui::link_clicks::an_address_in_a_table_cell_is_not_a_link`
`verified-by: bravebot_tui::link_clicks::an_address_the_opener_would_refuse_is_not_a_link`
`verified-by: bravebot_tui::link_clicks::an_address_is_underlined_and_no_mark_reaches_the_terminal`
`verified-by: bravebot_tui::link_clicks::what_a_click_opens_is_what_the_screen_shows_underlined_everywhere`
`verified-by: bravebot_tui::link_clicks::a_link_that_is_covered_or_whose_screen_is_something_else_opens_nothing`
`verified-by: bravebot_tui::link_clicks::an_address_in_the_scroller_opens_as_it_does_at_rest`
`verified-by: bravebot_tui::link_clicks::a_number_linked_to_an_issue_is_not_an_address_to_open`
`verified-by: bravebot_tui::link_clicks::an_address_in_a_reply_still_arriving_opens`
`verified-by: bravebot_tui::link_clicks::an_address_or_a_path_in_a_fenced_block_is_not_a_link`
`verified-by: bravebot_tui::link_clicks::what_the_open_command_lists_is_what_a_click_reaches_in_every_shape_the_link_tests_use`
`verified-by: bravebot_tui::text_links::an_address_stops_where_the_sentence_resumes`
`verified-by: bravebot_tui::text_links::only_a_scheme_that_starts_a_word_starts_an_address`
`verified-by: bravebot_tui::text_links::a_run_with_anything_but_printable_ascii_is_not_an_address`
`verified-by: bravebot_tui::text_links::an_address_naming_a_user_is_not_an_address`
`verified-by: bravebot_tui::text_links::a_run_of_schemes_is_measured_in_one_pass`
`verified-by: bravebot_tui::text_links::the_cells_of_an_address_are_read_back_and_underlined`
`verified-by: bravebot_tui::text_links::a_wrap_inside_the_address_joins_its_rows_and_a_wrap_at_a_blank_does_not`
`verified-by: bravebot_tui::app::tests::wrapped::two_addresses_a_blank_apart_open_as_each_is_drawn_whatever_the_blanks_before_them`
`verified-by: bravebot_tui::text_links::an_address_only_partly_in_the_area_is_not_a_link`
`verified-by: bravebot_tui::text_links::what_the_cells_show_decides_what_is_a_link`
`verified-by: bravebot_tui::text_links::cells_outside_the_area_are_not_read`
`verified-by: bravebot_tui::text_links::a_browser_that_could_not_be_started_is_said_and_one_that_was_is_not`
`verified-by: bravebot_tui::markdown::an_address_is_marked_wherever_a_reply_may_put_one`
`verified-by: bravebot_tui::markdown::a_run_that_is_not_an_address_is_not_marked`
`verified-by: bravebot_tui::markdown::what_is_inside_an_address_is_not_styled`
`verified-by: bravebot_tui::markdown::with_addresses_off_an_address_is_plain_text`
`verified-by: bravebot_tui::markdown::a_run_of_schemes_that_is_not_an_address_is_measured_once`
`verified-by: bravebot_tui::opener::only_a_web_address_with_a_host_in_printable_ascii_is_openable`
`verified-by: bravebot_tui::link_clicks::a_number_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use`
`verified-by: bravebot_tui::app::a_number_typed_while_a_turn_runs_starts_the_browser_for_an_address`
`verified-by: bravebot_tui::link_clicks::what_is_typed_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use`
`verified-by: bravebot_tui::link_clicks::a_typed_address_the_opener_would_refuse_opens_nothing_and_says_so`
`verified-by: bravebot_tui::link_clicks::a_typed_address_opens_while_a_turn_runs_and_a_typed_name_is_refused`

<a id="MOUSE-41"></a>
### MOUSE-41: a click on the info panel's pull request or issue row opens the link the person gave

In the info panel, a left click that sweeps nothing on the `Pull request` row or the `Issue` row opens
the address `/pr` or `/issue` set ([PANEL-12](info-panel.md#PANEL-12)) in the person's browser. It
needs no modifier, since the panel draws only what the person and the driver said and nothing a tool
returned reaches it ([PANEL-11](info-panel.md#PANEL-11)), and a click on a row of the person's own is
not one that could take them somewhere nobody chose. The address is the one that was set, text the
person typed, so it is routing on its own integrity ([ROUTE-2](routing.md#ROUTE-2)), held to one `http`
or `https` address with a host in printable ASCII ([PANEL-12](info-panel.md#PANEL-12)). It is not the text
the row draws for it: the shorthand `{owner}/{repo}#{n}`, or the address cut from the left to fit. It is
never built from a repository the person did not name. An address naming a user, as in
`https://github.com@example.test/`, is refused by the opener ([MOUSE-40](#MOUSE-40)), so its row is
drawn and is not a target. The word beside the link and the link are one target, and the heading, the
rows around them and the border column, which is the panel's resize handle
([PANEL-6](info-panel.md#PANEL-6)), are not.

As for [MOUSE-12](#MOUSE-12), the press and the release are on one cell and the press is a single
click, so a drag is a selection, and a click over the scroller, a
history search or a picker opens nothing. The browser is started as for MOUSE-40, and a click opens
the link while a turn or a command runs. A pointer over the row is told from the same record, so
what it says it would open and what a click opens are the same ([MOUSE-51](#MOUSE-51)).

**The keyboard.** `/pr` and `/issue` with no argument say the address that is set, and `/open` followed by
that address opens it in the browser as the click does ([CMD-30](commands.md#CMD-30)).

`verified-by: bravebot_tui::link_clicks::a_click_on_the_panels_pull_request_or_issue_row_opens_what_the_person_gave`
`verified-by: bravebot_tui::link_clicks::a_link_row_opens_the_address_as_it_was_set_and_not_the_text_drawn_for_it`
`verified-by: bravebot_tui::link_clicks::a_click_off_the_link_rows_a_drag_or_a_double_click_opens_nothing`
`verified-by: bravebot_tui::link_clicks::a_link_the_opener_would_refuse_is_drawn_and_not_a_target`
`verified-by: bravebot_tui::link_clicks::a_cleared_link_is_no_target_and_a_new_one_is`
`verified-by: bravebot_tui::link_clicks::a_link_row_under_a_picker_opens_nothing`
`verified-by: bravebot_tui::link_clicks::a_click_on_the_panel_and_a_click_on_the_transcript_each_open_their_own_address`
`verified-by: bravebot_tui::link_clicks::the_address_a_panel_row_opens_is_opened_by_typing_it_after_the_open_command`

<a id="MOUSE-42"></a>
### MOUSE-42: a click on a file path in the assistant's words shows the file

A left click with no modifier held that sweeps nothing on a file path in the assistant's words adds
the lines of that file to the end of the transcript, under a call, `Preview(name)`, that says how many
lines the file has. It is carried out once the time a second press would take has passed
([MOUSE-6](#MOUSE-6)): a second press on the cell makes the double click that selects the word, and
a press anywhere else drops it, so selecting a file name to copy it shows nothing. A click left
waiting for two seconds, so long that something else must have happened, is dropped. So is one that falls due where
a click on a path is no longer answered: a key can have opened the scroller or a search, or a turn can
have begun, while it waited. The press and the release are on one cell and the press is a single
click, so a drag and a triple click are selections, and a click with ctrl, shift or alt held is not
this click. It is answered at the plain prompt, and with the files state open, where the transcript is
drawn beside the panel as it is at the plain prompt. It is not answered in the scroller, in a
delegate's view, in a history search, under a picker, nor while a turn or a command runs, since the
lines are shown at the end of the transcript and a running turn is drawing there.

A path is text on the screen, found as an address is ([MOUSE-40](#MOUSE-40)): the matcher marks it,
the scan reads the marks off the cells, and what a click acts on is the characters those cells show,
joined across the rows of one wrapped line, and none of it underlined, since whether a name is a file
is not known until it is opened. A row the wrap left one cell short because the next character is wide
goes on into the next row as a row that ends in the last column does. A span of inline code is one when it holds nothing else. A path is
made of letters and digits, `_`, `-`, `.`, `/`, `+` and `@`, in at most 1024 bytes, with a character
that is not printable ASCII allowed where it is a letter or a digit. It has no `..` anywhere in it
(`../a.rs` and `wait...what` are not names), no `//`, no leading `-` and no `@` without a `/`, and its last part has an extension of one to ten letters and
digits with a letter among them, so `README.md` and `src/main.rs` are paths and `1.5.0`, `Makefile`,
`and/or` and `me@example.com` are not. A one-letter extension is taken after a `/` and for `.c` and
`.h`, so `e.g` is a word. `:line`, `:line:column` and `:first-last` after the name say the line the
view starts at. A file with no extension is not a target, and `example.com` is, which is how a name
reads and says nothing of the file system. The words are the same as for MOUSE-40, and a table cell
holds no path either.

What is shown is decided by the file system and the person's rules and by no byte of the file. The name
is resolved by the file tools' own rules, as a typed `@path` is ([NAME-5](naming-files.md#NAME-5)): a
file inside the working directory or a directory opened for the session, where an absolute name that
lands inside the working directory is taken as the name relative to it. A `..`, a name outside every
open directory, a link that leads out, a directory and a name with nothing at it are one answer, that
the path names no file there, said without repeating the name, since it is text the assistant wrote and
a note is the interface's own words. No trust rule extends this reach
([TRUST-10](trust-map.md#TRUST-10)), so a file outside is not shown whatever the map says of it. A
`deny` rule that covers the file refuses it under every name the file has, as a read by a file tool does
([PERM-7](permissions.md#PERM-7)): a hard link to a denied file is refused, and so is any file whose
other names could not be found while a rule about a path is in force. The file is opened once, without
waiting for a writer, and its kind and size are those of the open file: a pipe put there since the name
was resolved is refused, a file over a megabyte is refused with its size, and no more than a megabyte
is read of one that grew. A file whose bytes are not valid UTF-8 and an empty file have no lines. That
the bytes are valid UTF-8 is the only thing asked of them, and the answer is rows or none for the
screen.

The lines are untrusted and private whatever the trust map says of the path, the label a write's
review reads the file it replaces under ([LABEL-8](labels.md#LABEL-8)). They are drawn as a
quarantined block ([VIEW-3](terminal-transcript.md#VIEW-3)): behind the bar on every row, with the
name as the workspace spells it and the label in the heading, a line saying that no model has them,
and every control character made visible. At most 24 lines are shown, each cut at 160 characters and
numbered with its place in the file, starting three lines above the one the path names and moved back
so that the window ends at the last line. The view goes to the end of the transcript. The lines are cut
inside the kernel's gate for reshaping content and released to the screen under the witness for
display ([LABEL-4](labels.md#LABEL-4)), which the audit trail records without a line of the file. They
are in no conversation and no saved record, and nothing reads them back.

**What decides.** The person's click on cells they can see, and nothing in the bytes under it: the
matcher chooses which cells a click lands on and the file system chooses whether there is a file. The
name is the routing of a read for the screen, and it is authorised by the click, the person's endorsement
of the exact path as drawn ([ROUTE-8](routing.md#ROUTE-8)), and not by what the assistant's words are
labelled. Showing a file to the person who asked to see it is not a read by the planner or the driver,
which hold none of it.

**Known cost.** `bravebot_agent::file_preview::preview` is a public function that takes any name and
returns the released lines as strings. That the click is the endorsement of the name holds because the
terminal client calls it from one place, `bravebot_tui::file_preview::show`, which a carried-out click
and `/open` reach. No gate and no pinned count of callers makes it so, and a second caller is found by
reading the diff.

**Known cost, at the edge of the view.** A path that ends one cell short of the last column of the last
row in view, because the next character is wide and is drawn on the row below the view, is read as
ending where it is drawn: the rows the scan joins are the rows in view.

**The keyboard.** `/open <number>` shows the file of a path in the list `/open` printed, and `/open <path>`
the file a path typed after it names, as the click does: the same action, so the same refusals and the
same notes, and only where a click on a path acts ([CMD-30](commands.md#CMD-30)). While a turn runs the
command says that a file is shown when nothing is running and shows nothing. A typed name is read as a
click reads the place after it and otherwise whole, so one a click could not reach, with no extension or
with a blank in it, can be shown too. `@src/main.rs` in a prompt sends the file to the planner
([NAME-1](naming-files.md#NAME-1)), which is a different act.

`verified-by: bravebot_tui::link_clicks::a_click_on_a_path_waits_for_a_second_press_and_then_asks_for_the_file`
`verified-by: bravebot_tui::link_clicks::a_double_click_on_a_path_selects_a_word_and_shows_nothing`
`verified-by: bravebot_tui::link_clicks::a_click_that_waited_far_too_long_is_not_carried_out`
`verified-by: bravebot_tui::link_clicks::a_click_on_a_path_is_due_after_400_milliseconds_and_dropped_after_two_seconds`
`verified-by: bravebot_tui::link_clicks::a_click_with_a_modifier_a_sweep_or_beside_the_path_waits_for_nothing`
`verified-by: bravebot_tui::link_clicks::a_path_wrapped_over_rows_is_one_path_from_any_of_them`
`verified-by: bravebot_tui::link_clicks::a_path_with_a_wide_character_is_one_path_wherever_the_wrap_falls`
`verified-by: bravebot_tui::link_clicks::a_click_on_a_path_is_not_carried_out_where_the_session_went_while_it_waited`
`verified-by: bravebot_tui::link_clicks::a_link_is_answered_with_the_files_state_open`
`verified-by: bravebot_tui::link_clicks::a_click_on_a_path_where_the_file_would_not_be_seen_waits_for_nothing`
`verified-by: bravebot_tui::link_clicks::the_file_is_shown_under_a_call_in_a_quarantined_block`
`verified-by: bravebot_tui::link_clicks::a_file_with_escape_sequences_cannot_draw_on_the_screen`
`verified-by: bravebot_tui::link_clicks::a_path_that_names_no_file_in_reach_says_so_without_repeating_it`
`verified-by: bravebot_tui::link_clicks::a_denied_file_and_a_file_too_large_are_refused_in_words`
`verified-by: bravebot_tui::app::a_click_on_a_path_is_held_to_the_working_directory_and_only_the_preview_to_the_deny_rules`
`verified-by: bravebot_tui::text_links::what_reads_as_a_path_and_the_place_after_it`
`verified-by: bravebot_tui::text_links::what_does_not_read_as_a_path`
`verified-by: bravebot_tui::text_links::a_path_stops_where_the_sentence_resumes`
`verified-by: bravebot_tui::text_links::a_path_starts_a_word`
`verified-by: bravebot_tui::text_links::a_path_of_1024_bytes_is_one_and_one_of_1025_is_not`
`verified-by: bravebot_tui::text_links::a_path_may_begin_with_a_letter_that_is_not_ascii_and_not_with_a_symbol`
`verified-by: bravebot_tui::text_links::a_closing_brace_with_no_opener_is_not_part_of_the_path_or_the_address`
`verified-by: bravebot_tui::text_links::a_long_run_is_measured_in_one_pass`
`verified-by: bravebot_tui::text_links::the_cells_of_a_path_are_read_back_and_not_underlined`
`verified-by: bravebot_tui::text_links::a_wrap_inside_a_path_joins_its_rows`
`verified-by: bravebot_tui::text_links::a_wide_character_that_left_a_row_one_cell_short_does_not_end_the_path`
`verified-by: bravebot_tui::text_links::a_path_with_a_wide_character_is_one_run`
`verified-by: bravebot_tui::text_links::what_the_cells_of_a_path_show_decides_what_is_a_link`
`verified-by: bravebot_tui::text_links::a_run_of_one_kind_does_not_join_a_run_of_another`
`verified-by: bravebot_tui::markdown::a_path_is_marked_wherever_a_reply_may_put_one`
`verified-by: bravebot_tui::markdown::a_run_that_is_not_a_path_is_not_marked`
`verified-by: bravebot_tui::markdown::emphasis_around_a_name_is_still_emphasis`
`verified-by: bravebot_tui::markdown::with_paths_off_a_path_is_plain_text`
`verified-by: bravebot_tui::markdown::a_run_of_possible_paths_that_is_not_one_is_measured_once`
`verified-by: bravebot_agent::file_preview::a_file_in_the_working_directory_is_shown_numbered_and_untrusted`
`verified-by: bravebot_agent::file_preview::a_path_the_map_vouches_for_is_still_shown_as_untrusted`
`verified-by: bravebot_agent::file_preview::the_window_starts_before_the_line_asked_for_and_ends_at_the_end_of_the_file`
`verified-by: bravebot_agent::file_preview::a_long_line_is_cut`
`verified-by: bravebot_agent::file_preview::a_name_that_is_not_a_file_in_reach_shows_nothing`
`verified-by: bravebot_agent::file_preview::an_absolute_name_inside_the_working_directory_is_the_file_it_names`
`verified-by: bravebot_agent::file_preview::a_file_a_deny_rule_covers_is_not_read`
`verified-by: bravebot_agent::file_preview::a_name_no_rule_can_be_written_about_refuses_only_the_file_that_has_it`
`verified-by: bravebot_agent::file_preview::a_pipe_put_where_the_file_was_is_refused_and_not_waited_on`
`verified-by: bravebot_agent::file_preview::a_file_of_one_byte_over_the_most_is_refused_by_the_size_of_the_open_file`
`verified-by: bravebot_agent::workspace::a_source_longer_than_the_most_is_refused_after_reading_one_byte_past_it`
`verified-by: bravebot_agent::file_preview::each_alias_in_the_table_is_refused_by_a_deny_rule_unless_it_is_a_listed_gap`
`verified-by: bravebot_agent::file_preview::a_file_whose_other_names_cannot_be_found_is_refused_while_a_deny_rule_is_in_force`
`verified-by: bravebot_agent::file_preview::a_large_file_is_refused_and_one_that_is_not_text_has_no_rows`
`verified-by: bravebot_agent::file_preview::the_release_is_recorded_as_one_for_a_screen`
`verified-by: bravebot_tui::link_clicks::a_number_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use`
`verified-by: bravebot_tui::link_clicks::a_file_is_shown_where_a_click_on_a_path_acts_and_nowhere_else`
`verified-by: bravebot_tui::app::a_number_typed_while_a_summary_runs_waits_and_shows_the_file_when_it_is_over`
`verified-by: bravebot_tui::link_clicks::what_is_typed_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use`
`verified-by: bravebot_tui::link_clicks::a_typed_name_is_shown_or_refused_in_the_words_the_click_on_a_path_gets`

<a id="MOUSE-43"></a>
### MOUSE-43: a click with ctrl held on a file path in the assistant's words opens the file in the editor

A left click with ctrl held that sweeps nothing on a file path in the assistant's words
([MOUSE-42](#MOUSE-42) says what one is) opens the file in the person's editor, at the line the path
gave where the editor takes one. The press and the release are on one cell and the press is a single
click, so a drag is a selection, and a click without ctrl
is the one that shows the file. Terminals do not report the Cmd key, so the modifier is ctrl, and a
terminal that keeps ctrl-click for its own menu never reports it.

The editor takes the terminal, so it is asked for at the plain prompt, in the scroller, which is
the transcript, and with the files state open, and not while something else is drawing the screen: a running turn or command, a
history search and a picker, as the key that edits a prompt and `v` are not
([SCROLL-6](scroller.md#SCROLL-6)). It is not asked for in a delegate's view, where a name may be
another file than it is here, since a delegate may work in a checkout of its own
([CHECKOUT-7](checkouts.md#CHECKOUT-7)).

The name is resolved as MOUSE-42 resolves it, by the file tools' own rules, so a file inside the working
directory or a directory opened for the session is opened and a name that climbs out, leads out or
names no file is told so in a note that does not repeat it. A `deny` rule does not refuse it: the rule
is about what bravebot reads, and here bravebot reads nothing, the person's own program does. The editor
is the one `$VISUAL` and `$EDITOR` name, looked up and started as for a prompt
([SANDBOX-30](sandboxing.md#SANDBOX-30)), given the file as an absolute path and, for the editors whose
way of taking a line is known, the line and the column: `+LINE` for vim and the editors that follow it,
`+LINE,COLUMN` for nano, `+LINE:COLUMN` for emacs, micro and kakoune, `--goto FILE:LINE:COLUMN` for
Visual Studio Code and what is built on it, and `FILE:LINE:COLUMN` for Sublime Text, Zed and Helix. Any
other editor is given the file alone. The terminal is handed over as it is for `v`, the screen is cleared
on the way back, and a failure to start the editor is said. Nothing is copied and nothing is read back:
the file is the person's and whatever the editor saved is in it. An exit status is reported without
saying the file is unchanged, which it may not be.

A file that an editor opens can act on the editor: a modeline, a configuration kept in the directory.
That is the cost of opening a file there by hand, and the file is one the person chose by clicking a
name they could see. The file is one the workspace holds, which a checkout may have written, so the
editor is started without this agent's own credentials in its environment
([CRED-14](credential-protection.md#CRED-14)), as the browser is. The editor a prompt is edited in, and
`v`, are started with all of it, since only the person's own words go to them.

**What decides.** The person's click, with ctrl, on cells they can see, and nothing in the bytes under
it. The path is routing for the editor start and is the characters on the screen, which a person can
approve alone, and the click is that endorsement of the exact path as drawn
([ROUTE-8](routing.md#ROUTE-8)). The plain click and the ctrl-click are two gestures on one path, so
neither can be made by a reply.

**The keyboard.** `/open edit <number>` opens the file of a path in the list `/open` printed in the
editor, at the line and column the path gave, and `/open edit <path>` the file a path typed after it
names, as the click with ctrl held does: the same action, so the same resolution of the name, the same
refusals and the same notes, and only where a click on a path acts ([CMD-30](commands.md#CMD-30)). An
address given to it is refused with a note. The command is the same endorsement as the click, the
person's typed words naming a path they were shown listed or typed whole, and nothing in the bytes of
the path.

`verified-by: bravebot_tui::link_clicks::a_ctrl_click_on_a_path_asks_for_the_editor_at_its_place`
`verified-by: bravebot_tui::link_clicks::a_click_that_is_not_a_ctrl_click_on_a_path_asks_for_no_editor`
`verified-by: bravebot_tui::link_clicks::a_ctrl_click_on_a_path_asks_for_no_editor_while_the_screen_is_something_elses`
`verified-by: bravebot_tui::link_clicks::a_ctrl_click_opens_an_address_in_the_browser_and_a_path_in_the_editor`
`verified-by: bravebot_tui::link_clicks::a_link_is_answered_with_the_files_state_open`
`verified-by: bravebot_tui::editor::each_editor_is_told_the_line_in_its_own_words`
`verified-by: bravebot_tui::editor::the_arguments_an_editor_already_has_come_first`
`verified-by: bravebot_tui::editor::a_file_is_opened_at_its_line_in_the_words_of_the_editor_that_takes_one`
`verified-by: bravebot_tui::editor::a_file_with_no_line_is_opened_at_the_top`
`verified-by: bravebot_tui::editor::what_the_editor_saves_is_the_file_and_nothing_is_read_back`
`verified-by: bravebot_tui::editor::an_editor_that_exits_badly_does_not_claim_the_file_is_unchanged`
`verified-by: bravebot_tui::editor::a_file_is_not_opened_in_an_editor_nobody_chose`
`verified-by: bravebot_tui::app::a_click_on_a_path_is_held_to_the_working_directory_and_only_the_preview_to_the_deny_rules`
`verified-by: bravebot_tui::editor::an_editor_a_click_opens_is_not_handed_this_agents_credentials_and_the_prompt_editor_is`
`verified-by: bravebot_tui::editor::the_editor_a_click_opens_is_started_without_this_agents_own_credentials`
`verified-by: bravebot_agent::file_preview::a_name_that_is_not_a_file_in_reach_shows_nothing`
`verified-by: bravebot_agent::file_preview::an_absolute_name_inside_the_working_directory_is_the_file_it_names`
`verified-by: bravebot_tui::link_clicks::edit_and_a_number_ask_for_what_a_ctrl_click_on_that_path_asks_for_in_every_shape`
`verified-by: bravebot_tui::link_clicks::edit_asks_for_no_editor_while_a_turn_is_drawing_on_the_terminal`
`verified-by: bravebot_tui::app::edit_typed_while_a_summary_runs_waits_and_asks_for_the_editor_when_it_is_over`
`verified-by: bravebot_tui::link_clicks::what_is_typed_asks_for_what_a_click_on_that_link_asks_for_in_every_shape_the_link_tests_use`

<a id="MOUSE-44"></a>
### MOUSE-44: no mouse event answers a question

A press, a release, a drag, a turn of the wheel or a click of any button, with or without a modifier,
on any cell of the screen, answers none of the questions the interface puts to a person: the
approval of a write, a run, a read, a fetch, a path, a host, a checkout, a server or a call to one,
the questions the planner and the interface ask, whether to trust a directory or the rules a
checkout proposed, the reading of a credential, and the confirmation of deleting a session's record.
Each goes on being asked, and the keys answer it. The code that reads a question's
events passes over every event that is not a key, except that a turn of the wheel moves the body of a
question that scrolls ([MOUSE-46](#MOUSE-46)), so that a click, or bytes another program wrote that
read as one, cannot approve, grant, vouch for or refuse anything.

A report of the mouse changes nothing a question draws, so none draws it again but the turn of the
wheel, which moves the body. A pointer crossing the window reports a position for each cell
([INPUT-33](terminal-input.md#INPUT-33)), and a question with a long body that was laid out and drawn
again for each would lag the pointer. The trust question draws again for a report of the mouse only
where it takes down the offer to leave that is standing ([INPUT-4](terminal-input.md#INPUT-4)).

A question added to the interface is held to this by two tests. A reply to a turn that has no kind
in the table of questions does not build, and a file that reads events from the terminal that the
table does not name fails the test that lists them. A file counts as a reader whether it calls
`input::read` and `event::poll` by path or imports `read` or `poll` by name, in a list, renamed or by
glob.

`verified-by: bravebot_tui::confirm::tests::mouse_tests::no_pointer_event_answers_any_question`
`verified-by: bravebot_tui::confirm::tests::mouse_tests::every_reader_of_events_is_one_the_table_of_questions_knows`
`verified-by: bravebot_tui::confirm::tests::mouse_tests::a_pointer_crossing_the_screen_asks_for_no_frame_of_any_question`
`verified-by: bravebot_tui::confirm::a_report_of_the_mouse_that_is_not_the_wheel_changes_nothing_a_question_draws`
`verified-by: bravebot_tui::confirm::tests::mouse_tests::a_reader_imported_by_name_is_counted_as_the_paths_to_one_are`

<a id="MOUSE-45"></a>
### MOUSE-45: a press on a panel drawn over the frame is the panel's

A picker opened while a turn runs, and the scroller's key list, are drawn over the frame, and each is
recorded with the cells it covers. A press that lands on one belongs to it, and so do the drag that
follows and the release that ends the press, wherever the pointer goes. It acts on the panel or on
nothing, and never on a target drawn beneath it: a row of the info panel, a hint under the box, a call
or a delegate in the scroller, a place in the prompt box, or any other target of the frame. The press
begins no selection, drops the one held, and leaves no click standing for the release to complete.

A picker holds the input, so while one is open every press, drag, release and turn of the wheel is
the picker's wherever the pointer is. The wheel does not scroll the transcript beneath it and a drag
does not select what the picker covers.

`verified-by: bravebot_tui::app::tests::popups::a_click_on_a_panel_reaches_no_target_beneath_it`
`verified-by: bravebot_tui::app::tests::popups::a_press_and_its_release_on_different_sides_of_a_panel_click_nothing`
`verified-by: bravebot_tui::app::tests::popups::a_press_on_the_picker_over_the_info_panel_leaves_the_context_unread`
`verified-by: bravebot_tui::app::tests::popups::an_open_picker_takes_the_wheel_and_the_drag_from_the_transcript`
`verified-by: bravebot_tui::app::tests::popups::a_press_on_the_scrollers_key_list_expands_no_call_beneath_it`
`verified-by: bravebot_tui::app::tests::popups::a_press_on_a_panel_over_the_transcript_starts_no_selection_and_no_edge_scroll`

<a id="MOUSE-46"></a>
### MOUSE-46: the wheel walks the list it is over, or the panel that holds the input

While a picker is open, a turn of the wheel anywhere on the screen moves its cursor up or down by the
number of rows `tui.wheelRows` names, the distance it moves the transcript at rest, and stops at the
first row and the last. In the model picker a row is a model, so the step is in models and the
headings and the spaces between sections are not counted. In the search over the prompts already
sent a turn of the wheel up moves the cursor to older prompts and one down to newer ones, and stops at
the oldest and the newest. In the lists of sessions it moves the cursor that many sessions, over the
headings of the sessions view, and stops at the first and the last. In each of these it takes no row,
leaves the list open, and does not scroll the transcript beneath it. Over the rows of the list
offered under the box as a line is typed the wheel moves the cursor that many items and stops at the
first and the last, and elsewhere on the screen it scrolls the transcript. While the scroller's key
list is up the wheel does nothing: the list holds the input, does not scroll, and the view behind it
stays where it was.

In an approval or other question whose body scrolls with the arrow keys, a turn of the wheel moves the
body three rows, as three presses of Down do, and stops where the keys stop. Three is the distance
`tui.wheelRows` has where it is not set, and the setting is not read there. The wheel answers nothing
([MOUSE-44](#MOUSE-44)). The questions the planner and the interface ask that move a cursor among
answers, the questions asked before a session starts, the reading of a credential and the question of
deleting a record do not read the wheel. The keys stay the way to do it without a mouse.

`verified-by: bravebot_tui::list_picker::the_wheel_moves_the_cursor_by_the_rows_asked_and_stops_at_both_ends`
`verified-by: bravebot_tui::app::tests::pickers::the_wheel_walks_a_picker_opened_mid_turn_by_the_wheel_rows`
`verified-by: bravebot_tui::model_prompt::the_wheel_moves_the_cursor_by_the_models_asked_over_the_headings`
`verified-by: bravebot_tui::app::tests::history_search::the_wheel_walks_the_prompts_by_the_wheel_rows_and_stops_at_both_ends`
`verified-by: bravebot_tui::resume::the_wheel_moves_the_cursor_by_the_sessions_asked_and_stops_at_both_ends`
`verified-by: bravebot_tui::sessions_view::the_wheel_moves_the_cursor_by_the_sessions_asked_over_the_headings`
`verified-by: bravebot_tui::app::tests::completions::the_wheel_over_the_list_walks_it_and_elsewhere_scrolls_the_transcript`
`verified-by: bravebot_tui::confirm::tests::mouse_tests::a_turn_of_the_wheel_moves_the_body_of_a_question_as_the_keys_do`
`verified-by: bravebot_tui::app::tests::popups::a_press_outside_the_scrollers_key_list_puts_it_away_and_nothing_else`

<a id="MOUSE-47"></a>
### MOUSE-47: a press outside a panel closes it only where Escape closes it with no effect

Each panel that holds the input is closed by a press outside it as Escape closes it, where Escape
closes it with no effect, and is left as it was where Escape answers something. Only a left press
closes: a press of another button, a drag and a release never do. A press that closes a panel does
nothing else, and the release that follows it is the panel's.

| Panel | What Escape does | A left press outside it |
|---|---|---|
| the pickers `/effort`, `/config`, `/theme` and `/model` open, at rest and while a turn runs | leaves what was in force alone, and `/theme` puts back the theme it opened on | closes it the same way |
| the search over the prompts already sent | closes it, leaving the box as it was | closes it the same way |
| the lists of sessions `--resume`, `/resume` and `bravebot sessions` open | leaves without resuming or attaching | there is nothing outside them: each takes the whole screen |
| the list offered under the box as a line is typed | clears the line, which discards what was typed | does nothing |
| the scroller's key list | puts it away, as does any key | puts it away |
| an approval, a question the planner or the interface asks, the question of trusting a directory or of deleting a record | refuses, declines or stops, each of which is an answer ([MOUSE-44](#MOUSE-44)) | does nothing |

`verified-by: bravebot_tui::list_picker::a_left_press_outside_the_panel_cancels_and_takes_nothing`
`verified-by: bravebot_tui::model_prompt::a_left_press_outside_the_panel_keeps_the_current_model`
`verified-by: bravebot_tui::app::tests::history_search::a_press_outside_the_search_closes_it_and_leaves_the_box_as_it_was`
`verified-by: bravebot_tui::theme_prompt::the_pointer_previews_the_row_and_an_outside_press_puts_the_old_theme_back`
`verified-by: bravebot_tui::app::tests::pickers::a_press_outside_a_picker_opened_mid_turn_closes_it_and_clicks_nothing_beneath`
`verified-by: bravebot_tui::app::tests::pickers::only_a_left_press_outside_a_picker_opened_mid_turn_closes_it`
`verified-by: bravebot_tui::app::tests::pickers::the_release_of_a_press_that_closed_a_picker_clicks_nothing_in_the_session`
`verified-by: bravebot_tui::app::tests::popups::a_press_outside_the_scrollers_key_list_puts_it_away_and_nothing_else`

<a id="MOUSE-48"></a>
### MOUSE-48: a click on a hint under the box does what the key it names does

Each hint on the row under the box that names a key is a target:

| Hint | The key it names | Where the key does it |
|---|---|---|
| `ctrl-t show trail`, `ctrl-t hide trail` | shows or hides the trail | anywhere the hint is drawn |
| `? for shortcuts` | puts the list of keys up or takes it down | on an empty line, outside shell mode and vi's NORMAL mode, where `?` is otherwise a character or vi's own key |
| `ctrl-x info` | opens the info panel | anywhere the hint is drawn |
| `ctrl-l 2 to open` | opens the view of the delegates and asides | anywhere the hint is drawn |
| `alt-l or /outputs 1 to open` | opens the view of what commands printed | anywhere the hint is drawn |
| `ctrl-b to background` | moves the command a turn waits on to the background | while a turn waits on one that can be moved |
| `held, 12 rows below  ·  ctrl-end to return` | returns a view held where it was scrolled to the end | anywhere the hint is drawn |
| `esc to cancel`, on the line of shell mode | leaves shell mode, clearing the line | at rest, in the box everybody has and with no enhancement waiting |
| `image on clipboard  ·  ctrl-v to paste` | reads the picture from the clipboard | at rest and while a turn runs |

A left click that sweeps nothing on the words of one hint does what pressing its key does at that
moment, and acts only where the key does: a hint drawn while the key is refused is not a target.
While a command the person typed is running no hint is a target, because the keys of the row are
read by nothing then, and nor is the permission mode ([MOUSE-14](#MOUSE-14)). As for [MOUSE-12](#MOUSE-12),
the press and the release are on one cell and the press is a single click, so a drag is a
selection, and the first click of a double click acts once. The indent and the separators between hints are not part of one, and a hint the width of the terminal had no room
for is not drawn and is not a target. The note at the right of the row that names the key that pastes
a picture is such a hint: it is drawn whole, with its words alone a target, or not at all.

A part of the row that describes and names no key is not a target: the context, the cache, the jobs,
the loops, the vi mode and what a copy took. Nor are the offer to press `ctrl-c` again, which any
mouse report withdraws, the sentence that says a word was sent as a prompt, which names two keys, the
line of the info panel's files state, which is one sentence that names six, the line that names a
prompt an outside link wrote, whose key sends it, and the list `?` opens. A click never sends a
prompt. The row draws no `/sandbox off`, so there is nothing of that name to click. A panel drawn
over the row takes the press first ([MOUSE-45](#MOUSE-45)). The keys stay the way to do it without a
mouse.

`verified-by: bravebot_tui::app::tests::hints::a_click_on_each_hint_does_what_its_key_does`
`verified-by: bravebot_tui::app::tests::hints::only_a_single_click_of_the_left_button_on_a_hint_acts`
`verified-by: bravebot_tui::app::tests::hints::a_click_on_a_hint_acts_while_a_turn_runs`
`verified-by: bravebot_tui::app::tests::hints::a_hint_that_is_drawn_while_its_key_is_refused_is_not_a_target`
`verified-by: bravebot_tui::app::tests::hints::what_the_row_only_describes_is_not_a_target`
`verified-by: bravebot_tui::app::tests::hints::the_cells_recorded_for_a_hint_read_as_its_words_at_every_width`
`verified-by: bravebot_tui::app::tests::hints::the_shell_line_records_its_own_hints_and_not_the_ordinary_ones`
`verified-by: bravebot_tui::app::tests::hints::the_note_a_copy_leaves_replaces_the_paste_hint_and_its_target`
`verified-by: bravebot_tui::app::tests::hints::a_note_that_names_a_key_is_drawn_whole_with_its_target_or_not_at_all`
`verified-by: bravebot_tui::app::tests::popups::a_click_on_a_panel_reaches_no_target_beneath_it`

<a id="MOUSE-49"></a>
### MOUSE-49: a press on the scrollbar puts the view there, and a drag on the thumb scrolls it

The scrollbar in the transcript pane's last column ([VIEW-39](terminal-transcript.md#VIEW-39)) takes a
left press on any cell of it. A press on the thumb takes hold of it and moves nothing. A press on the
rest of the column puts the middle of the thumb at the pointer, which is the first row of the
transcript from the top cell of the column and the tail from the bottom one, and takes hold of the
thumb there. While the thumb is held, a drag moves it by the rows the pointer moves, from wherever on
the screen the pointer is, and a pointer above or below the column is the first row or the last. The
release lets go, and so does anything that says the button is no longer held where the terminal does
not: the window losing focus, the terminal being resized and a press anywhere. The next drag of the
left button is then a selection as any other. At the tail the view follows what arrives again, and
elsewhere it stays where it was put.

The press begins no selection, drops the one held, leaves no click for the release to complete and
asks for no copy. A left press with shift is the bar's as well. A press of another button, a drag
that began elsewhere and a release that did not follow a press on the bar do nothing to it, and a
press anywhere ends the hold of a thumb whose release never came, as when the button was let go
outside the window. It
answers while a turn runs as at rest, and a panel drawn over it takes the press first
([MOUSE-45](#MOUSE-45)). The wheel, the
arrow keys and the page keys stay the way to do it without a mouse.

`verified-by: bravebot_tui::app::tests::scrollbar::a_press_in_the_track_puts_the_middle_of_the_thumb_where_it_landed`
`verified-by: bravebot_tui::app::tests::scrollbar::a_press_on_the_thumb_holds_it_and_a_drag_carries_it_with_the_pointer`
`verified-by: bravebot_tui::app::tests::scrollbar::a_press_on_the_bar_selects_nothing_copies_nothing_and_only_the_left_button_acts`
`verified-by: bravebot_tui::app::tests::scrollbar::a_press_on_the_bar_moves_the_view_while_a_turn_runs`
`verified-by: bravebot_tui::app::tests::scrollbar::the_thumb_stands_for_the_rows_shown_and_gives_the_row_back`
`verified-by: bravebot_tui::app::tests::scrollbar::a_press_on_the_text_ends_a_hold_of_the_thumb_whose_release_never_came`
`verified-by: bravebot_tui::app::tests::scrollbar::a_drag_of_the_thumb_ends_when_focus_is_lost_the_terminal_resizes_or_another_press_lands`
`verified-by: bravebot_tui::app::tests::scrollbar::a_press_on_the_track_the_scroller_has_replaced_takes_hold_of_nothing`
`verified-by: bravebot_tui::app::tests::popups::a_click_on_a_panel_reaches_no_target_beneath_it`

<a id="MOUSE-50"></a>
### MOUSE-50: what a click acts on is found in one record the frame wrote as it drew

Each frame records the targets it drew in one list, last drawn last, each as the cells it covers and
what a click on them is. The cells are those of the screen, or the columns of the screen and the rows
of the transcript counted from its first, which follow the view as it scrolls. The handler of mouse
events works out where no widget is: it asks which target is under a cell and acts on the answer. The
answer is the target drawn over the others, so a panel recorded after the frame it covers is found
before what is under it ([MOUSE-45](#MOUSE-45)), and it is a target that stands in the state the
session is in now. A cell is asked of the targets that cover its row, so that the work of a lookup
does not grow with the length of the transcript, which a report of the pointer asks at each cell.
Each kind of target says in one place in which states it stands, because the state moves on between a frame and the next: a key opens the scroller, a delegate's view or the files state,
and the targets of the frame before are then not what a click on those cells does.

A call that has more to show than it draws ([MOUSE-9](#MOUSE-9), [MOUSE-25](#MOUSE-25)) and a delegate's
block ([MOUSE-10](#MOUSE-10), [MOUSE-23](#MOUSE-23)) are in the record as the rows of the transcript
they cover, and so are the keys that act on the call at the top of the view. Neither is a target in a
delegate's view, in a view of what a command printed or of an aside, or in the files state. A link in
the assistant's words ([MOUSE-40](#MOUSE-40), [MOUSE-42](#MOUSE-42), [MOUSE-43](#MOUSE-43)) is in the
record as the cells it is drawn in on each row, with what it opens held beside them. An address
stands wherever it is drawn. A path stands only with no turn or command running and no view of a
delegate open, which is where the file can be shown and the editor can have the terminal. The rows of
the info panel a click acts on, the mark of a finished task and the controls of its files state
([MOUSE-11](#MOUSE-11) to [MOUSE-13](#MOUSE-13), [MOUSE-18](#MOUSE-18) to [MOUSE-21](#MOUSE-21),
[MOUSE-30](#MOUSE-30) to [MOUSE-32](#MOUSE-32), [MOUSE-41](#MOUSE-41)) are recorded by the panel as
it draws, as the cells of each row after the border or the cells of it the layout names, so a click
finds them and the panel is not laid out again to answer it. The two panes of the files state that
the wheel scrolls, the list and the edits ([MOUSE-19](#MOUSE-19)), are recorded under them, so the
wheel finds the pane whatever is drawn on it. They stand in the plain frame, which the scroller and
the views of a delegate, of what a command printed and of an aside replace, the rows of the ordinary
state where the files state is not open and the controls and panes of the files state where it is. A place in the prompt box's text ([MOUSE-16](#MOUSE-16)) is recorded by the box as each
row it drew, with the column its text begins at and the bytes of the line it holds, so the cell of a
click is measured against the row as it was drawn. It stands where the box has a caret to put: in the
ordinary style, outside the files state, and not for a row of a line that has been changed since the
frame was drawn. Each row of the list of what could be typed under the box ([MOUSE-15](#MOUSE-15)) is
recorded with the item it holds, and stands while that list is offered. The track of the scrollbar
beside the transcript ([MOUSE-49](#MOUSE-49)) is recorded by the frame that draws it and stands in the
plain frame, so a press on its cells after a key has opened the scroller is not a press on a bar. A
panel that holds the input, which is a picker, the scroller's list of keys, the menu a right click
opens or the search over the prompts already sent ([MOUSE-45](#MOUSE-45), [MOUSE-52](#MOUSE-52)), is
recorded as the cells it covers and, for a picker and the menu, the cells of its rows, and each match
the search lists as its row ([MOUSE-15](#MOUSE-15)). While one is up
nothing of the frame beneath it is a target, and the panel is found where it is drawn, so one lookup
says whether a cell is a panel's. The way
back out of a delegate's view ([MOUSE-24](#MOUSE-24)), the marker that leads a reply
([MOUSE-26](#MOUSE-26)), the word that copies a fenced block ([MOUSE-27](#MOUSE-27)), a prompt
waiting behind a turn ([MOUSE-28](#MOUSE-28)) and each hint on the row under the box, with the
permission mode ([MOUSE-14](#MOUSE-14), [MOUSE-48](#MOUSE-48)), are in the record. The marker and the word are not
targets in a delegate's view, whose entries are the delegate's and not the session's, or in the files
state.

A target stands in these states, which a test asks of each kind in each of them with the record the
frame before wrote. The first four columns are the state with no view of a delegate open. A view of a
delegate is open over the plain frame or the scroller, and while a turn or a command runs, and the
last column is the state it is in whichever of those it was opened over.

| Target | at rest | a turn runs | a command runs | the scroller | a panel that holds the input | the files state | a delegate's view |
|---|---|---|---|---|---|---|---|
| a row of the info panel, the mark of a finished task, the hide row | yes | yes | yes | no | no | no | no |
| a file's row, the close control, a control of the edit bar and a pane the wheel scrolls, in the files state | yes | yes | yes | no | no | yes | no |
| a hint under the box, the permission mode included | yes | yes | no | no | no | no | no |
| a place in the box's text, a prompt waiting behind a turn | yes | yes | yes | no | no | no | no |
| the track of the scrollbar, a row of the list under the box | yes | yes | yes | no | no | yes | no |
| a reply's marker, the word that copies a block, a call, a delegate's block | yes | yes | yes | yes | no | no | no |
| a web address | yes | yes | yes | yes | no | yes | yes |
| a file path | yes | no | no | yes | no | yes | no |
| the way back out of a delegate's view | no | no | no | no | no | no | yes |

The way back stands in every state that has a delegate's view open and in none that has not, which
two tests ask: the row above, of the one record the view wrote, and the same record with the view
closed and each of the other states entered. A view of what a command printed and a view of an aside
replace the frame as a delegate's view does, and stand as it does, but for the way back, which
belongs to a view of a delegate alone.

The hint that leaves shell mode stands only at rest, since a turn does not read the key, and a hint
whose key is refused in the state is not a target ([MOUSE-48](#MOUSE-48)). A panel that holds the input
stands whatever else is open, and so do the rows of its list, but only while it is itself open: one a
key has closed is no target in the cells it covered, before the frame that no longer draws it, so a
press there reaches what is beneath.

One function says whether a release ends a click, for every kind of target and in the scroller as at
the plain prompt: the release is on the cell the press was on, the pointer moved the loose end of the
selection nowhere between them, and the press was a single click, or the second press of a double
click for a row of the list under the box ([MOUSE-15](#MOUSE-15)), and the target under the release
is the one the press stood on. A frame drawn between the two can put another target under the cell, as
a reply arriving at the tail does by moving every row up, and a report can replace the plan a press
on a task was made against: the release then ends no click. A link is told by what it opens and not
by its place among the links of a frame, and a plan by how many times it has been replaced. Two tests
ask each of the first three conditions of every kind of target. A pointer that moves between a
press and its release with no report of the motion in between clicks nothing, and a drag is the
selection it makes. What the click is then depends on the target the release is over, and a release
that ends no click, finds no target or finds one that does nothing ends the selection and asks for
its copy.

A terminal reports the two clicks of a double click as a press, a release, a press and a release, so
the first release is a click and acts. The press that follows a click that acted, on the same cell
within the time that makes a double click ([MOUSE-6](#MOUSE-6)), begins no selection, and its release
and the presses of a triple click that follow answer nothing: a double click on a target acts once,
and a word of the text under the pointer is not copied, whatever the click left under it, as when it
hid the panel that the cell had been a part of. The press after a triple click is a press again. Three
kinds of click are not counted as having acted. The text of the prompt box puts the caret and is
selected by a double click as any text is ([MOUSE-16](#MOUSE-16)). A row of the list under the box
takes a second click as its own ([MOUSE-15](#MOUSE-15)). A path waits for the second press to say
whether the first was a click on it ([MOUSE-42](#MOUSE-42)). Nor is a click that did nothing: a
double click on text that is no target, or on an address without ctrl held, selects the word.

`verified-by: bravebot_tui::state::targets::the_target_recorded_last_is_the_one_drawn_on_top`
`verified-by: bravebot_tui::state::targets::a_target_in_the_transcript_follows_the_view_as_it_scrolls`
`verified-by: bravebot_tui::state::targets::a_lookup_asks_only_the_targets_that_cover_the_row_of_the_cell`
`verified-by: bravebot_tui::state::targets::a_target_the_state_no_longer_stands_behind_is_skipped`
`verified-by: bravebot_tui::state::targets::while_a_panel_holds_the_input_only_the_panel_is_a_target`
`verified-by: bravebot_tui::app::tests::standing::every_kind_of_target_stands_in_the_states_its_click_works_in_and_in_no_other`
`verified-by: bravebot_tui::app::tests::standing::while_a_panel_holds_the_input_no_cell_beside_it_is_a_target`
`verified-by: bravebot_tui::app::tests::standing::the_way_back_stands_only_while_a_delegates_view_is_open`
`verified-by: bravebot_tui::app::tests::standing::a_panel_a_key_has_closed_is_no_target_before_the_next_frame_is_drawn`
`verified-by: bravebot_tui::app::tests::clicks::a_release_on_another_cell_than_the_press_ends_no_click_on_any_target`
`verified-by: bravebot_tui::app::tests::clicks::a_release_after_a_sweep_or_a_second_press_ends_no_click_on_any_target`
`verified-by: bravebot_tui::app::tests::clicks::a_release_over_a_target_a_frame_since_the_press_put_there_ends_no_click`
`verified-by: bravebot_tui::panel::a_report_between_the_press_and_the_release_toggles_no_task`
`verified-by: bravebot_tui::app::tests::clicks::a_double_click_on_a_target_acts_once_and_selects_nothing`
`verified-by: bravebot_tui::app::tests::clicks::only_the_rest_of_a_run_that_began_with_a_click_that_acted_begins_nothing`
`verified-by: bravebot_tui::app::tests::clicks::a_double_click_in_the_text_of_the_box_or_on_a_path_still_selects_a_word`
`verified-by: bravebot_tui::app::tests::hints::the_cells_recorded_for_a_hint_read_as_its_words_at_every_width`

<a id="MOUSE-51"></a>
### MOUSE-51: the target the pointer is over is held, and a frame is asked for when it changes

A terminal that reports the pointer with no button held ([INPUT-33](terminal-input.md#INPUT-33))
tells the interface where it is. The session holds the target under it that says what a click on it
does, found as the target of a click is ([MOUSE-50](#MOUSE-50)): the one drawn over the others that
stands in the state the session is in. Every kind of target says what a click on it does but the text
of the box, which is where the caret goes wherever the pointer is over it, and the panels that hold
the input and their rows, which read no position of the pointer and draw nothing again for one.

A position asks for a frame only when the target under it is not the one that was held, or when it
is over none after one: a thousand positions within one target ask for one frame, a move to the next
target for one more, and a move off every target for the one that takes it away. Two targets of one
kind are two, as two rows of the info panel that list servers are. Every frame reads the target under
the pointer again in the layout it drew, so a target that moves or stops standing under a pointer
that did not move, as when the panel is closed, is no longer the one held, and a turn of the wheel,
which moves what is under the pointer, does not end that the pointer is over something.

The target held is drawn underlined and bold over what it already is, in the frame that is drawn
after it is held: the words on the first row it covers, from its first drawn cell to its last, and
the rows of an address or a path that was wrapped. The blanks before and after them are left as they
were, and no cell's text changes. These are the marks an address and the word that copies a block are
already drawn with, with bold to tell a hovered one from the rest, and neither is a colour, so they
show in every theme and under `NO_COLOR`. The track of the scrollbar has no text to mark and is not.

The row where the frame draws its hints, which is the row under the box, the scroller's footer and the
footer of a delegate's view, says in one line what a click on the target does, from a message of the
catalog for each kind of target, in the room to the right of the last cell the row has drawn. For an
address, a path and the pull request or issue row of the info panel the line carries the address, or
the path with the line it names, after the sentence, as the cells show it and for the panel as it
opens, cut to what is left by the columns the characters are drawn in: an address from the right,
which keeps the host, and a path from the left, which keeps the name, whatever the name is written
in. Where the room holds the sentence and too little of the address or path beside it, and where it
does not hold the sentence, the line is the address or path alone, cut in the same way, since where
a click goes is what the person cannot read elsewhere: the cells show a link only as far as its row
of the transcript held it. A line of any other target whose sentence does not fit is not drawn,
never half of it. The hints
and the permission mode are drawn first and are not moved or covered, so the room is what they leave,
and a copy notice, a note or a message that fills the row, as the line of keys the files state has
where the panel is not drawn and the banner of a prompt an outside link wrote do, and a search being
typed in the scroller, leaves none:
the line gives way to them. Nothing in the line is read from the words under the pointer but what a
link acts on, which are the text of the cells a click would open.

The pointer is over nothing after a key, after the terminal is left or resized and when a button goes
down or is held while the pointer moves, and each asks for the frame that draws without it. While a
command the person typed is running, which reads none of the hints under the box, or a picker or other
panel holds the input, the target under the pointer is not one that stands, so none is held. Moving
the pointer changes nothing else in the session, over any cell of any target: it does not select,
click, scroll, open a view, type or copy. A terminal that sends no position with no button held has
no target held and loses nothing else, and with `tui.mouse` false nothing is reported and none is held.

Hovering is not an action and has no key. It only draws what a click on the target would do, and a
person with no pointer is told what each click does by the clause of that click and by the keys and
commands those clauses name.

`verified-by: bravebot_tui::app::tests::hover::a_thousand_moves_within_one_target_ask_for_one_frame_and_a_move_to_the_next_asks_for_one_more`
`verified-by: bravebot_tui::app::tests::hover::a_move_from_the_row_of_one_server_to_the_row_of_another_asks_for_a_frame`
`verified-by: bravebot_tui::app::tests::hover::moving_across_every_target_changes_no_session_state_but_the_hover`
`verified-by: bravebot_tui::app::tests::hover::what_is_marked_ends_with_a_key_a_focus_loss_a_resize_and_a_press`
`verified-by: bravebot_tui::app::tests::hover::a_target_that_stops_standing_is_no_longer_hovered`
`verified-by: bravebot_tui::app::tests::hover::a_frame_that_moves_what_is_under_the_pointer_moves_the_hover`
`verified-by: bravebot_tui::app::tests::hover::the_target_under_the_pointer_is_drawn_underlined_and_bold_and_nothing_else_changes`
`verified-by: bravebot_tui::app::tests::hover::an_address_wrapped_over_rows_is_marked_on_every_row_of_it`
`verified-by: bravebot_tui::app::tests::hover::the_row_of_hints_says_what_a_click_on_the_target_under_the_pointer_does_in_the_room_it_leaves`
`verified-by: bravebot_tui::app::tests::hover::the_line_for_a_link_carries_the_address_or_path_cut_to_the_room_left`
`verified-by: bravebot_tui::render::a_cut_is_made_by_columns_whatever_the_characters_are`
`verified-by: bravebot_tui::app::tests::hover::the_line_gives_way_to_a_note_a_banner_the_files_line_and_a_search_being_typed`
`verified-by: bravebot_tui::app::tests::hover::the_line_for_a_link_of_the_panel_shows_the_address_that_opens_and_not_the_shorthand`
`verified-by: bravebot_tui::list_picker::a_position_of_the_pointer_asks_for_no_frame_of_the_picker_opened_at_rest`
`verified-by: bravebot_tui::theme_prompt::a_position_of_the_pointer_asks_for_no_frame_of_the_picker_opened_at_rest`

<a id="MOUSE-52"></a>
### MOUSE-52: a right click opens a small menu of what could be done with the keyboard

A right press on a reply, on a delegate's block, or inside a selection that is held opens a small menu
with its corner at the pointer, over the frame. It is moved up and left where it would reach past an
edge of the screen, wraps nothing, and is cut by its own edge where the screen is narrower than its
words. What it holds depends on which kind of thing is under the pointer, found as a click's target
is ([MOUSE-50](#MOUSE-50)), and never on the words that thing holds:

| Under the pointer | Items |
|---|---|
| the words of a reply: its rows, its marker, the word that copies a block, the blank beside them | copy, copy as text, branch this session |
| a delegate's block | open this delegate |
| a selection held, with the pointer inside it | copy |
| anything else, with the right button or another | none: no menu opens |

Each item does what a command or a click does, through the function that command or click uses, so the
menu cannot do what the command refuses and says what it says. Copy puts the reply on the clipboard
as `/copy` and the click on its marker do ([CMD-11](commands.md#CMD-11), [MOUSE-26](#MOUSE-26)):
`/copy` copies the markdown the planner wrote, so a second copy of the source would be the same item.
Copy as text puts the same reply on the clipboard as the screen shows it, drawn by the widget that
draws the transcript at a width no line reaches: none of the marks of the markdown it was written in,
as `**`, backticks and the `#` of a heading, none of the marker and margin the screen draws, and no
wrapping. Branch this session is `/branch` ([SESSION-31](sessions.md#SESSION-31)): carried out at the
plain prompt, and while a turn runs queued behind it with the note a typed `/branch` gets, since
it waits for the turn ([CMD-8](commands.md#CMD-8)). It copies the whole session and not the session
as it stood at the reply clicked, because `/branch` does and nothing cuts a copy at an earlier turn:
`/rewind` in the copy has no rewind points to go back to, a copy inheriting none, and what it does
overwrites files in the directory both sessions share, which a click on a menu is not a decision
about. Open this delegate opens the view of that delegate as a click on its block does
([MOUSE-23](#MOUSE-23)). Copy of a selection hands the clipboard what was swept as letting go of the
button did ([MOUSE-1](#MOUSE-1)). An item whose reply is no longer in the transcript asks for nothing.

The menu holds the input while it is open ([MOUSE-45](#MOUSE-45)). Up and Down, and `k` and `j` as in
the other lists, move its cursor and stop at the first item and the last, Enter takes the item the
cursor is on, and Escape or Ctrl-C closes it with no effect and does not stop a turn that is running.
Every other key does nothing and is not typed into the box, and nor is a paste. A left press on an
item takes it, and the release that follows is the menu's. A left press outside it closes it with no
other effect ([MOUSE-47](#MOUSE-47)), while a press of another button, a drag, a release and a move
never do, and a right press outside it does not open another. None of those presses drops the
selection the menu was opened for: the item that copies it needs it, and closing the menu changes
nothing else. A turn of the wheel moves the cursor by `tui.wheelRows`
and leaves the transcript beneath where it is ([MOUSE-46](#MOUSE-46)), and the frame that shows the
cursor moved is drawn at once. A resize drops it. It is recorded
with the cells it covers and those of its items, so a press on it reaches no target beneath it.

It does not open over a picker, the scroller's list of keys or the search over earlier prompts, which
hold the input, nor while a command the person typed is running, which reads no key but the one that
stops it, so that a menu open when one begins is closed. It does not open in the files state or a
delegate's view either, whose rows are not the session's replies, with a selection held in them or
without. It is never a question: it answers none of the approvals, the trust and credential questions, which read no
pointer event of any button ([MOUSE-44](#MOUSE-44)), and nothing in it grants, vouches for or refuses.
In the scroller it opens as it does at rest, over the reply the pointer is on.

The menu has no key of its own, since it is a convenience over commands and clicks that each have
one: `/copy`, `/copy text`, `/branch` and Ctrl-L are the keyboard path, and a selection, which only the
mouse makes, is copied as it is for a drag ([MOUSE-1](#MOUSE-1)). `tui.contextMenu` set to the boolean `false`,
in any settings file, opens no menu, and any other value, including the string `"false"`, leaves it on.
It is a setting apart from `tui.mouse` because terminals disagree about a right click: macOS Terminal
reports ctrl-click as one, and some terminals keep a right click for a menu of their own, while the
wheel and the drag the interface reads are wanted anyway.

`verified-by: bravebot_tui::app::tests::menu::a_right_press_opens_the_menu_of_what_is_under_it_and_nowhere_else`
`verified-by: bravebot_tui::app::tests::menu::with_a_selection_held_and_the_pointer_inside_it_the_menu_copies_the_selection`
`verified-by: bravebot_tui::app::tests::menu::the_menu_is_drawn_inside_the_screen_wherever_the_pointer_is_and_wraps_nothing`
`verified-by: bravebot_tui::app::tests::menu::the_menu_is_walked_taken_and_closed_with_the_keys`
`verified-by: bravebot_tui::app::tests::menu::each_item_does_what_the_command_or_the_click_it_stands_for_does`
`verified-by: bravebot_tui::app::tests::menu::copy_as_text_and_the_typed_command_put_the_same_words_on_the_clipboard`
`verified-by: bravebot_tui::app::tests::menu::branching_from_the_menu_while_a_turn_runs_waits_as_the_typed_command_does`
`verified-by: bravebot_tui::app::tests::menu::a_press_outside_the_menu_closes_it_and_nothing_else_does`
`verified-by: bravebot_tui::app::tests::menu::the_menu_does_not_open_where_it_must_not_and_is_dropped_on_a_resize`
`verified-by: bravebot_tui::app::tests::menu::the_menu_opens_the_delegate_of_the_block_when_two_turns_number_theirs_alike`
`verified-by: bravebot_tui::app::tests::menu::the_menu_opens_in_the_scroller_and_a_press_on_it_reaches_nothing_beneath`
`verified-by: bravebot_tui::app::tests::menu::a_key_that_closes_or_takes_from_the_menu_while_work_runs_does_not_stop_it_and_the_copy_is_handed_on`
`verified-by: bravebot_tui::app::tests::menu::a_press_on_the_item_that_copies_a_held_selection_copies_it`
`verified-by: bravebot_tui::app::tests::menu::a_turn_of_the_wheel_over_a_panel_that_holds_the_input_asks_for_the_frame_that_shows_it`
`verified-by: bravebot_tui::app::tests::menu::a_menu_left_open_when_a_command_begins_is_closed_and_the_keys_reach_the_command`
`verified-by: bravebot_tui::app::tests::menu::the_menu_of_a_held_selection_does_not_open_in_a_delegates_view_or_the_files_state`
`verified-by: bravebot_tui::app::tests::menu::a_paste_under_the_open_menu_is_not_typed_into_the_box`
`verified-by: bravebot_config::settings::only_a_boolean_turns_the_context_menu_off`

<a id="MOUSE-53"></a>
### MOUSE-53: a report past column 223 or row 223 is the cell it names, and a multiplexer has to forward it

The interface asks for coordinates in the form that has no limit of 223 (SGR, mode 1006,
[INPUT-33](terminal-input.md#INPUT-33)). A press, a drag, a release and a turn of the wheel reported
at a column or a row past 223, up to the last of the screen, is handled at that cell: it finds the
targets that cell holds, begins and ends the selections that cell does and walks the lists that cell
is over, as a report at a cell before 223 does. `ESC[<0;300;120M` is a press at column 300 and row 120,
counted from 1, which is the cell at column 299 and row 119. The reader hands such an event out as it
was reported ([INPUT-49](terminal-input.md#INPUT-49)), and nothing between it and a target keeps a
column or a row in a byte or holds it to 223.

The interface reads what its terminal sends. Inside a multiplexer the reports reach it only as far as
the multiplexer forwards them, and in the form it asked for, and nothing here can tell that they are
not arriving: the wheel and a drag then do nothing in the session, or act on the multiplexer's
own scrollback and selection. The page that documents `tui.mouse` says that the multiplexer has to
forward the reports and names the modifier that gives a drag to the terminal (MOUSE-4).

**Why.** What the tmux and GNU screen documents show, which is all that is said about them. In tmux
3.5a, `input.c` keeps one tracking mode for a pane, the last of 1000, 1002 and 1003 that the program
asked for, which is why the interface asks for 1003 after the other two; `input-keys.c` writes the
SGR form to the program only when the program asked for 1006 and the terminal tmux runs in sent the
report in that form, and otherwise writes the older form, in which a position past what a byte holds
is set to the last cell it can name; `server-client.c` forwards a mouse report to the pane while its
`mouse` option is off, and `key-bindings.c` binds a press, a drag and a turn of the wheel with it on to
`send-keys -M` when the pane's program asked for mouse reports. The GNU screen manual lists mouse
tracking in the table of private modes it understands as modes 9 and 1000, and no other. The
interface has not been run inside either multiplexer, so a click at a wide column inside one has not
been tried, and what screen does with modes 1002, 1003 and 1006 is not shown.

`verified-by: bravebot_tui::app::tests::wide::a_press_a_drag_and_a_release_on_the_scrollbar_at_a_column_past_223_move_the_view`
`verified-by: bravebot_tui::app::tests::wide::a_drag_of_the_thumb_to_a_row_past_223_goes_where_that_row_is_on_the_track`
`verified-by: bravebot_tui::app::tests::wide::a_selection_made_by_reports_past_column_223_covers_the_cells_they_name`
`verified-by: bravebot_tui::app::tests::wide::the_wheel_at_a_cell_past_223_turns_what_is_under_it`
`verified-by: bravebot_tui::app::tests::wide::a_click_on_a_row_past_223_takes_the_row_it_names`
`verified-by: bravebot_tui::input::a_report_past_column_223_is_handed_out_as_the_cell_it_names`
