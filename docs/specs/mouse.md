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
word and line selection and turning mouse capture off are proposed in
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
