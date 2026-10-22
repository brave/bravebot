---
id: MOUSE
title: Selecting text with the mouse
status: normative
governs:
  - crates/tui/src/select.rs
documented-by: docs/website/docs/using/transcript.md
---

## Scope

Dragging across the screen to copy text. The mouse is captured so the wheel can scroll, which
takes the terminal's own selection away, so a drag is answered by the interface itself. What a
drag selects is spelled out here. Clicking a call in the scroller is built. Clicking other things on the screen, a context menu, edge
scrolling and copying a selection longer than the screen are proposed in
[brave/bravebot#2685](https://github.com/brave/bravebot/issues/2685) and not built.

## Clauses

<a id="MOUSE-1"></a>
### MOUSE-1: a selection belongs to the pane it began in

The screen is recorded each frame as panes: the transcript, the status line, the prompt box, what is
offered beneath it, the footer and, when it is drawn, the info panel. A press inside a pane starts a
selection that belongs to that pane. The pointer is held inside the pane's rectangle, so a drag past
its right edge stops at its last column and a drag past its bottom edge stops at its last row, and
the copy and the highlight read only that pane's cells. A drag from the transcript across the info
panel copies no panel text, and the reverse. A view that records no panes, such as the scroller,
treats the whole screen as one pane.

`verified-by: bravebot_tui::app::a_drag_from_the_transcript_never_copies_the_panel`
`verified-by: bravebot_tui::app::a_drag_from_the_panel_never_copies_the_transcript`
`verified-by: bravebot_tui::select::a_sweep_past_the_edge_of_its_pane_stops_at_the_edge`
`verified-by: bravebot_tui::select::rows_between_the_ends_are_cut_to_the_pane`
`verified-by: bravebot_tui::select::a_highlight_stays_inside_its_pane`
`verified-by: bravebot_tui::app::a_drag_past_the_bottom_stops_at_the_last_row_of_the_pane`

<a id="MOUSE-2"></a>
### MOUSE-2: a press between panes starts nothing

A press that lands in no recorded pane begins no selection and clears the one held. A press on the
info panel's border column starts a resize instead ([PANEL-6](info-panel.md#PANEL-6)), which also
begins no selection and copies nothing on release.

`verified-by: bravebot_tui::select::a_press_between_recorded_panes_belongs_to_none`
`verified-by: bravebot_tui::app::a_press_outside_every_pane_starts_no_selection`
`verified-by: bravebot_tui::panel::a_drag_from_the_border_never_selects_or_copies`

<a id="MOUSE-3"></a>
### MOUSE-3: a selection does not outlive its pane

A selection follows its pane's rectangle as the next frame lays it out. It is dropped when the pane
is no longer drawn, as when the info panel is closed, and when the terminal changes size, because
the cells it named then hold other text.

`verified-by: bravebot_tui::app::closing_the_panel_drops_a_selection_inside_it`
`verified-by: bravebot_tui::app::a_width_change_drops_the_selection`
`verified-by: bravebot_tui::app::a_height_change_drops_the_selection`

<a id="MOUSE-4"></a>
### MOUSE-4: mouse capture can be turned off

`tui.mouse` set to the boolean `false`, in any settings file, stops the interface asking the terminal
to report the mouse: the wheel and a drag are the terminal's own, and the interface does no selecting
of its own. Any other value, including the string `"false"`, leaves capture on. The `?` list names
the terminal's bypass modifier (shift-drag; option on macOS iTerm2 and Terminal.app) for the
terminals that still report the mouse.

`verified-by: bravebot_config::settings::only_a_boolean_turns_mouse_capture_off`
`verified-by: bravebot_tui::app::a_session_with_the_mouse_off_never_asks_for_mouse_reporting`
`verified-by: bravebot_tui::app::the_mouse_is_captured_unless_the_settings_turn_it_off`
`verified-by: bravebot_tui::render::the_shortcuts_name_the_terminals_own_selection`

<a id="MOUSE-5"></a>
### MOUSE-5: a selection in the info panel takes its text and nothing around it

The info panel draws a border column and a space before each row's text. A press on the border
column resizes the panel ([MOUSE-2](#MOUSE-2)). A selection that begins on the space starts at the
first column of the text, and the highlight stops at the end of each row's
text. The copy takes the text alone, with no border, indent or trailing blanks. The session id is
drawn over two rows because it does not fit on one, and a copy that spans both rows reads it as one
line.

`verified-by: bravebot_tui::select::a_panel_selection_takes_the_text_and_not_the_border_or_indent`
`verified-by: bravebot_tui::select::a_panel_highlight_covers_the_text_and_not_the_chrome_or_padding`
`verified-by: bravebot_tui::select::a_joined_row_runs_on_into_the_next_without_a_break`
`verified-by: bravebot_tui::app::a_drag_over_the_session_id_copies_the_id_alone`

<a id="MOUSE-6"></a>
### MOUSE-6: a double click takes a word and a triple click a row

Two presses of the left button on the same cell within 400 milliseconds select the word there, and
a third the whole row, inside the pane the press belongs to. A word is a run of letters, digits and
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
end to the pointer and keeps its anchor. A shift-press anywhere else, or with no selection held, is
an ordinary press. Terminals that keep shift for their own selection never report it.

`verified-by: bravebot_tui::app::a_shift_click_extends_the_selection_held`
`verified-by: bravebot_tui::app::a_shift_click_in_another_pane_begins_a_new_selection`

<a id="MOUSE-8"></a>
### MOUSE-8: a wrapped line copies as one line

A line the transcript is wider than is drawn over several rows. A copy that spans those rows reads
them as the one line they are, so a wrapped paragraph is pasted as a paragraph and not with a line
break at every row the screen cut it at. A row the wrap broke at a word is joined to the next with
one space, even when the word ended in the last column and the blank after it was dropped, and a
row cut inside a word with none. Lines
the author broke are still separate lines. This holds in the transcript and in the scroller, and it is not applied to any other pane: the rows are screen rows, and the info
panel is drawn on the same ones.

`verified-by: bravebot_tui::select::a_wrapped_row_joins_the_row_below_it`
`verified-by: bravebot_tui::select::a_blank_dropped_at_the_last_column_is_still_a_space`
`verified-by: bravebot_tui::render::transcript::a_break_is_told_from_one_inside_a_word`
`verified-by: bravebot_tui::select::a_wrap_in_the_transcript_does_not_join_the_rows_of_another_pane`
`verified-by: bravebot_tui::app::a_wrapped_reply_copies_as_one_paragraph`

<a id="MOUSE-9"></a>
### MOUSE-9: a click on a call in the scroller expands it

In the scroller, a left click that sweeps nothing on a row of a call that has more to show than it
draws expands that call, and a second click collapses it, as `x` does ([SCROLL-10](scroller.md#SCROLL-10)).
The press and the release are on one cell and the press is a single click, so a drag, a double click
and a triple click are selections and copy as before. A click on a row that belongs to no call, with
the key list open over the transcript, or in a delegate's view, does nothing to any call. The key
stays the way to do it without a mouse.

`verified-by: bravebot_tui::app::a_click_on_a_call_in_the_scroller_expands_and_collapses_it`
`verified-by: bravebot_tui::app::a_click_off_a_call_or_a_drag_over_one_expands_nothing`
`verified-by: bravebot_tui::app::a_click_with_the_scroller_help_open_expands_nothing`
