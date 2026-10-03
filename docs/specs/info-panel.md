---
id: PANEL
title: Telling sessions apart
status: normative
governs:
  - crates/tui/src/title.rs
documented-by: docs/website/docs/using/sessions.md
---

## Scope

What lets somebody running many sessions at once tell which terminal holds which. The terminal's
title is built. The info panel beside the transcript, holding the session's name, its issue and pull
request, its context and cache, its language servers and its plan, is proposed in
[brave/bravebot#1220](https://github.com/brave/bravebot/issues/1220) and not built, and its clauses
take the ids after the last one here. How a session gets its name is [sessions.md](sessions.md).

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
