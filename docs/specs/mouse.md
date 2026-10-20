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
drag selects is spelled out here. Clicking things on the screen, a context menu, edge scrolling,
word and line selection are proposed in
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
