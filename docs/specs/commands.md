---
id: CMD
title: Slash commands
status: normative
governs:
  - crates/tui/src/app.rs
guards:
  - symbol: commands
documented-by: docs/website/docs/reference/commands.md
---

## Scope

A line beginning with `/` that this program acts on itself, in place of sending it anywhere. What
every one of them shares: where such a line may come from, when a line is one, and what happens to
the line once it is.

Not what any particular command then does. `/add-dir`, `/cd` and `/status` are the trust map's, in
[trust-map.md](trust-map.md); `/compact` is [compaction.md](compaction.md)'s; `/clear` begins a
session, which is [sessions.md](sessions.md)'s; `/btw` asks something the conversation never sees,
and where its answer is drawn is [watching.md](watching.md)'s; `/manifest` starts the other kind of
run, which is [manifest.md](manifest.md)'s. The `!` prompt is a different surface entirely and is
[shell-mode.md](shell-mode.md).

**Skills are not on this surface.** Other agents let a person type a skill's name after a slash,
and this one does not: a skill is advertised to the planner by name and description, and its body
is fetched by the planner asking for it. Nothing in the input box knows skills exist, so
`/commit-style` is a prompt like any other sentence. [skills.md](skills.md) owns what a skill is
and what each source is trusted for, and [tools/load-skill.md](tools/load-skill.md) owns the
fetch. CMD-7 says why the two surfaces stay apart.

## Where a command may come from

<a id="CMD-1"></a>
### CMD-1: only a line a person typed into the box

A command is dispatched from a line a person typed into the input box and from nowhere else. Never a
line the planner produced, never text read out of a file, never anything a processor returned, never
a line reconstructed from a transcript. A model that writes `/clear` has written four characters, and
they reach a person's screen as four characters.

**Why.** Every command here decides something a turn is not allowed to decide on its own: which
directories are reachable, what the conversation consists of, which model thinks. The endorsement
is the keystroke, so the keystroke is the only thing that may produce one. Recalling an earlier
prompt is still a person's own line, so a recalled `/status` is a command again.

**A command that waited for a turn to end is carried out with no press behind it** (CMD-8), and it is
still this rule: what waited is the line the box held when somebody pressed Enter on it, and nothing
but that press puts anything in the queue.

`verified-by: by-construction (both dispatch sites read a line that came off the input box: the box's own key handler, and the queue that handler put the line in for when the turn ends. No path carries model output, file content or processor output into either)`


<a id="CMD-2"></a>
### CMD-2: the whole word, and an argument only after a space

`/rename` is the command. `/renamed the parser` is a prompt, because the word is longer.
`what does /add-dir do` is a prompt, because the word is not the line. The bare word with nothing
after it is the command with an empty argument, answered by saying what it needs, or by doing what
the bare word means where it means something of its own, rather than by doing nothing quietly.
`/loop` on its own says what is repeating, which is [loop.md](loop.md)'s.

**Why.** The set of words this program claims is taken out of the language a person can use to
talk to the planner, so it is claimed as narrowly as possible: asking how a command works must
stay a question. Prefix matching would have made `/add-dirs are useful` open a directory called
`s are useful`.

`verified-by: bravebot_tui::app::a_longer_word_starting_with_the_command_is_a_prompt`
`verified-by: bravebot_tui::app::an_argument_is_taken_only_after_the_whole_command_word`
`verified-by: bravebot_tui::app::a_prompt_containing_the_add_dir_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_status_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_cost_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_clear_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_compact_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_model_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_theme_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_theme_is_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_effort_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_config_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_effort_is_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_rename_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_exit_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::the_bare_add_dir_command_is_still_the_command`
`verified-by: bravebot_tui::app::the_bare_rename_command_is_still_the_command`
`verified-by: bravebot_tui::app::a_prompt_containing_the_loop_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_loop_is_a_prompt`
`verified-by: bravebot_tui::app::the_bare_loop_command_says_what_is_repeating`
`verified-by: bravebot_tui::app::a_prompt_containing_the_cd_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_cd_is_a_prompt`
`verified-by: bravebot_tui::app::the_bare_cd_command_is_still_the_command`
`verified-by: bravebot_tui::app::a_prompt_containing_the_btw_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_btw_is_a_prompt`
`verified-by: bravebot_tui::app::the_bare_btw_command_is_still_the_command`


<a id="CMD-3"></a>
### CMD-3: in shell mode the line is a command line, not a command

With the `!` mode armed, `/status` is a program somebody may have and is run as one. Nothing is
offered for completion there either, since `/usr/bin/env` is a path. A turn running changes none of
that: the line is still a command line, so nothing about it is answered as a command and nothing
about it is sent as a prompt. It waits for the turn the way a command waits (CMD-8), drawn under the
box behind the `!` the scrollback echoes one behind, and when the queue reaches it, it is run. The
mode is left behind as the line is taken, since the mode lasts one command line whether that line
ran at once or waited.

**Why.** The mode is how a person says which of the two they meant, and it is the more specific
statement of the two. Completing in it would rewrite the line under somebody typing a path.

A turn begins with no press behind it, from a loop's tick or a watch firing, so the mode is armed
mid-turn over a line that was typed at rest to be run. What the press may never do there is hand
that line to the turn: `echo pwned` reaching the planner as a sentence somebody said is the one
reading of the keystroke nobody asked for.

`verified-by: bravebot_tui::app::a_slash_command_in_shell_mode_is_a_command_line`
`verified-by: bravebot_tui::app::shell_mode_offers_no_completions`
`verified-by: bravebot_tui::app::a_shell_line_is_not_taken_for_a_command_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_command_line_is_not_sent_to_the_running_turn`
`verified-by: bravebot_tui::app::the_command_line_queued_while_a_turn_ran_is_run_when_the_turn_ends`
`verified-by: bravebot_tui::app::queueing_a_command_line_leaves_shell_mode`
`verified-by: bravebot_tui::app::a_queued_command_line_is_not_what_the_turn_took`
`verified-by: bravebot_tui::app::a_prompt_queued_behind_a_command_line_is_sent_once_it_has_run`
`verified-by: bravebot_tui::app::a_queued_command_line_comes_back_to_the_box`
`verified-by: bravebot_tui::shell_mode::a_waiting_command_line_is_drawn_behind_the_marker`

## What a command does to the line

<a id="CMD-4"></a>
### CMD-4: a command is never sent as a prompt

The line comes off the box and nothing enters the conversation. A session asked to shorten itself
must not answer by talking about shortening itself, and a session asked to clear must not answer
by asking what to clear.

That a command may go on to start a request of its own is a separate thing: `/compact` sends a
conversation to be summarised, which [compaction.md](compaction.md) governs, `/btw` sends a copy of
the conversation with a question on the end of it, which [watching.md](watching.md) governs, and
`/model` reaches the network to list models. None of them sends the typed line.

`verified-by: bravebot_tui::app::typing_the_status_command_reports_rather_than_prompting`
`verified-by: bravebot_tui::app::typing_the_cost_command_reports_rather_than_prompting`
`verified-by: bravebot_tui::app::typing_the_clear_command_starts_a_new_session`
`verified-by: bravebot_tui::app::the_compact_command_asks_for_a_summary_rather_than_being_sent`
`verified-by: bravebot_tui::app::the_add_dir_command_carries_its_directory`
`verified-by: bravebot_tui::app::typing_the_model_command_opens_the_picker`
`verified-by: bravebot_tui::app::typing_the_theme_command_opens_the_picker`
`verified-by: bravebot_tui::app::the_theme_command_carries_its_name`
`verified-by: bravebot_tui::app::typing_the_effort_command_opens_the_picker`
`verified-by: bravebot_tui::app::typing_the_config_command_opens_the_panel`
`verified-by: bravebot_tui::app::the_effort_command_carries_its_level`
`verified-by: bravebot_tui::app::typing_the_exit_command_quits`
`verified-by: bravebot_tui::app::the_loop_command_sends_what_is_left_after_the_interval`
`verified-by: bravebot_tui::app::the_cd_command_carries_its_directory`
`verified-by: bravebot_tui::app::the_btw_command_carries_its_question`
`verified-by: bravebot_tui::app::a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt`


<a id="CMD-5"></a>
### CMD-5: the argument is taken verbatim

Whatever followed the space, spaces and all, with the surrounding whitespace trimmed and nothing
else done to it. A leading `~` is expanded only as a whole first segment, so a directory whose own
name begins with a tilde is not a home-relative path. Nothing shortens it, splits it, or asks the
planner what it meant.

A marker standing for something staged beside the line is the exception, and only where the command
cannot carry what it stands for: it is put back before the argument is taken, a picture to words
([pasting.md](pasting.md#PASTE-6)) and a dropped file to its name
([dropping.md](dropping.md#DROP-4)). Where the argument is sent, which is `/btw`, `/manifest` and
`/loop`, a marker the request can carry stays in it as it was typed and what it stands for goes with
it, which is this rule rather than an exception to it.

**Why.** The argument is what the command acts on: a directory that becomes trusted, a name a
session is stored under. A person is taken to have endorsed exactly the characters they typed, so
exactly those characters have to arrive.

`verified-by: bravebot_tui::app::the_rename_command_carries_the_whole_name`
`verified-by: bravebot_tui::app::the_add_dir_command_carries_its_directory`
`verified-by: bravebot_tui::app::the_cd_command_carries_its_directory`
`verified-by: bravebot_tui::app::the_btw_command_carries_its_question`
`verified-by: bravebot_tui::app::a_session_can_ask_for_a_manifest_run`
`verified-by: bravebot_tui::app::a_tilde_is_expanded_only_as_a_whole_first_segment`
`verified-by: bravebot_tui::app::a_picture_pasted_into_a_question_goes_with_it`
`verified-by: bravebot_tui::app::a_picture_a_command_line_named_is_carried_out_as_words_rather_than_as_its_marker`
`verified-by: bravebot_tui::app::a_picture_dropped_onto_a_question_goes_with_it`
`verified-by: bravebot_tui::app::a_file_dropped_onto_a_command_line_is_carried_out_as_its_name`


<a id="CMD-6"></a>
### CMD-6: the set is written down once

One table names every command, its argument and its one-line description, and each name is a
single constant the one place that dispatches matches on. The completion list, Tab and the arrows
all read the table, and so does the arm that queues a command typed mid-turn (CMD-8), so typing `/`
lists every command with what it does and narrowing, queueing and dispatching all work on one set.

**Why.** A word written down in more than one place is a word that is renamed in one of them,
leaving the rest advertising something that no longer works, which a person discovers by typing
it.

`verified-by: bravebot_tui::app::compacting_is_offered_while_a_command_is_being_typed`
`verified-by: bravebot_tui::app::the_config_command_is_offered_like_every_other`
`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_tui::render::a_slash_offers_every_command_and_what_it_does`


<a id="CMD-7"></a>
### CMD-7: a command name is written in this program, never read from a directory

The set is fixed when this program is built. No name is enumerated from disk, from a project, or
from anything a person installed, and nothing a turn produced can add to it, remove from it, or
change what one of them does.

**Why.** This is what makes the surface small enough to reason about, and it is the reason skills
are kept off it. A skill's name is content: it comes from a directory that may not be trusted, it
is written by whoever wrote the skill, and the rule that an untrusted skill is counted rather than
named exists because such a name can be composed to read like an instruction on a person's screen.
Offering those names in a completion list would put exactly that text in front of the user as
though this program had written it, one keystroke from a line that decides something. If skills
are ever wanted here, the name still may not come from the directory: this clause is what the
change has to answer to.

`/loop` is what that answer looks like. The word is a string literal in this table like every
other command, and the skill it shares a name with is one written into this program too, so
nothing about either came from a directory. A skill somebody installs still has no line here,
whatever it is called, and installing one called `loop` shadows the built-in body without
touching this table.

`/agent` is the same answer for definitions. The word is a literal in this table, and the
definition it runs is an argument on the line, compared against the set the session resolved and
never added here, however many definitions a machine holds
([addressing-a-definition.md](addressing-a-definition.md)).

`verified-by: by-construction (the table is an array of string literals fixed at compile time, and no directory listing, configuration value or turn output reaches it)`


<a id="CMD-8"></a>
### CMD-8: while a turn runs the word waits, and is carried out when the queue reaches it

A command typed while a turn is in flight is taken off the box and joins the lines waiting for the
turn to end, exactly as a prompt does: the box clears, the history remembers it, and it is drawn
under the box marked as waiting. What it waits for is different. It is never offered to the turn in
flight, so nothing about it reaches the planner, and when the queue reaches it, it is carried out
rather than sent. The queue is drained in the order the lines were typed, so a command behind a
prompt waits for that prompt's turn. Which lines are commands there is CMD-2's rule and nothing
narrower, so a sentence mentioning a command is a prompt mid-turn as it is at rest, and a word the
table gives no argument is a prompt with anything after it.

**Why.** Enter mid-turn already means the line waits, and that is what a person pressing it expects
of every line they type. What the queue must not do is send a command: a line waiting there used to
be a prompt like any other, and the running turn takes those at its next round boundary, so a queued
`/clear` asked the planner what to clear. Carrying it out as it is typed is no better, because every
command acts on the conversation, the terminal or the network and the turn holds all three, so it
would change what the turn is running under. Waiting costs a person nothing and asks nothing of
them: they typed the command once, and it happens.

**Nothing enters the transcript while it waits**, and taking back what is waiting gives the command
back to the box like any other line. What a keystroke wrote into the transcript would count as the
turn having done something, which is what decides whether a stopped prompt comes back to be edited,
so a command recorded there would cost a person the prompt they stopped.

`verified-by: bravebot_tui::app::a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt`
`verified-by: bravebot_tui::app::no_command_is_sent_as_a_prompt_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_command_with_an_argument_is_not_sent_as_a_prompt_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_command_that_takes_no_argument_is_only_the_bare_word_mid_turn`
`verified-by: bravebot_tui::app::a_prompt_mentioning_a_command_is_queued_while_a_turn_runs`
`verified-by: bravebot_tui::app::the_command_queued_while_a_turn_ran_is_carried_out_when_the_turn_ends`
`verified-by: bravebot_tui::app::a_prompt_queued_behind_a_command_is_sent_once_the_command_has_run`
`verified-by: bravebot_tui::app::a_queued_command_is_not_what_the_turn_took`
`verified-by: bravebot_tui::app::a_queued_command_comes_back_to_the_box`

## Known costs

- **The list is one row per command, and a screen with no room for it loses the last of them.**
  Nothing bounds it and nothing scrolls it: the rows are handed to the layout and whatever does
  not fit is dropped from the bottom, so a terminal a few rows short of the whole table offers
  the commands at the top of it and silently offers none of the ones below. The arrows still walk
  onto a row that was not drawn, which puts the highlight somewhere the person cannot see. Every
  command is still typeable in full, and the table is still the one place they are written down;
  what a short terminal costs is the discovery the list exists for. Bounding it would mean a
  window that scrolls with the cursor, which is a second scroller beside the transcript's.
