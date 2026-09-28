---
id: CMD
title: Slash commands
status: normative
governs:
  - crates/tui/src/app.rs
  - crates/tui/src/skills.rs
guards:
  - symbol: commands
documented-by: docs/website/docs/reference/commands.md
---

## Scope

A line beginning with `/` that this program acts on itself, in place of sending it anywhere. What
every one of them shares: where such a line may come from, when a line is one, and what happens to
the line once it is.

Not what any particular command then does. `/add-dir`, `/cd`, `/forget-trust` and `/status` are the
trust map's, in [trust-map.md](trust-map.md); `/compact` is [compaction.md](compaction.md)'s;
`/clear` begins a session, which is [sessions.md](sessions.md)'s; `/btw` asks something the
conversation never sees, and where its answer is drawn is [watching.md](watching.md)'s;
`/manifest` starts the other kind of run, which is [manifest.md](manifest.md)'s. The `!` prompt is
a different surface entirely and is [shell-mode.md](shell-mode.md).

**Skills are offered here, and are never commands.** A slash word is offered the skills a turn
starting now would advertise to the planner, beneath the commands at the start of a line and alone
after other words. Taking one writes `/name ` into the line and nothing else, so `/commit-style` is
still a prompt like any other sentence, and the planner is what fetches the skill it names.
[skills.md](skills.md) owns what a skill is and what each source is trusted for, and
[tools/load-skill.md](tools/load-skill.md) owns the fetch. CMD-7 says why no skill becomes a
command, and CMD-9 says what the box offers.

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
single constant the one place that dispatches matches on. The completion list's command rows, Tab
and the arrows all read the table, and so does the arm that queues a command typed mid-turn
(CMD-8), so typing `/` lists every command with what it does and narrowing, queueing and
dispatching all work on one set. The skill rows beneath them read the set a turn would advertise
(CMD-9), and no key on one dispatches anything.

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

**Why.** This is what makes the surface small enough to reason about, and it is why no skill is a
command. A skill's name is content: it is written by whoever wrote the skill, and the rule that an
untrusted skill is counted rather than named exists because such a name can be composed to read
like an instruction on a person's screen. The skill rows CMD-9 draws beside the commands answer
each part of that without adding a word here:

- Only a skill a turn would advertise is offered, read through the same trust gate, so a skill
  from a project nobody trusts is never drawn, and nothing reaches the screen that the planner is
  not already shown.
- Each row says whether it came from the project, the person's own directory or this program, so
  none is drawn as though this program had written it.
- Taking one writes a prompt naming it, never a command line. A name a command claims, or one
  holding a space or a control character, is never a row, so no key on a skill row reaches a line
  that decides something.

`/loop` is the one name a command and a skill share. The word is a string literal in this table
like every other command, and the skill it shares a name with is one written into this program
too, so nothing about either came from a directory. A skill somebody installs still has no line
here, whatever it is called, and installing one called `loop` shadows the built-in body without
touching this table. Neither is a skill row, since the command claims the name.

`/agent` is how definitions are reached, and they get no rows at all. The word is a literal in
this table, and the definition it runs is an argument on the line, compared against the set the
session resolved and never added here, however many definitions a machine holds
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

## What a slash word is offered

<a id="CMD-9"></a>
### CMD-9: the skills a turn would advertise, and taking one writes a prompt

A slash word at the start of the line is offered the commands it could still become and, beneath
them, the skills whose names start with it. A slash word after other words is offered the skills
alone, since there a command is a prompt (CMD-2). The skills are the ones a turn starting now would
advertise to the planner, read the same way, and each row says when to use the skill and where it
was found: the project, the person's own directory, or this program. They are read as a slash word
begins and let go as it ends, so a skill written while the box sat idle is offered at the next
slash.

Tab, or Enter on a half-typed name, replaces the word with `/name ` and nothing else; Enter on a
name typed in full sends the line, unless the arrows moved onto another row first. Either way the line is a prompt, sent as it reads. The planner
is told that a prompt naming a skill as `/name` is the person asking for it, and it loads that
skill the way it loads any other. The arrows walk down the commands and on into the skills.

Nothing is offered in shell mode, while a turn runs, or after the word of a command line, which is
CMD-3 and CMD-5 holding for skills as they hold for commands. A skill whose name a command claims,
or whose name holds a space or a control character, is never a row.

**Why.** A person who knows which skill a task wants should be able to say so without hoping the
planner picks it from its description, and the name is easier to take from a list than to
remember. Writing the name into a prompt, rather than making it a command that loads the body,
keeps the choice where every other skill choice is made: the planner asks for the skill and
fetches it, so one named this way is loaded, recorded and bounded exactly as one it chose on its
own.

Reading the skills once a word, rather than once a session or once a key, is because resolving
reads directories: once a session would go on offering a skill that was deleted, and once a key
would read them on every letter.

`verified-by: bravebot_tui::app::a_slash_offers_the_skills_after_the_commands`
`verified-by: bravebot_tui::app::a_skill_is_completed_mid_sentence_and_sent_as_a_prompt`
`verified-by: bravebot_tui::app::enter_completes_a_half_typed_skill_and_sends_a_whole_one`
`verified-by: bravebot_tui::app::the_arrows_walk_from_the_commands_onto_the_skills`
`verified-by: bravebot_tui::app::the_skills_are_resolved_once_a_word_and_let_go_after_it`
`verified-by: bravebot_tui::app::no_skill_is_offered_in_a_command_line_or_inside_a_command`
`verified-by: bravebot_tui::app::nothing_is_offered_for_completion_while_a_turn_runs`
`verified-by: bravebot_tui::skills::the_word_being_typed_is_the_last_one_on_the_line`
`verified-by: bravebot_tui::skills::nothing_is_offered_inside_a_command_line`
`verified-by: bravebot_tui::skills::what_matches_is_every_name_starting_with_the_word_in_name_order`
`verified-by: bravebot_tui::skills::no_offered_skill_completes_to_a_command_line`
`verified-by: bravebot_tui::render::a_skill_row_says_where_it_came_from_within_the_width`
`verified-by: bravebot_agent::skills::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_agent::skills::each_skill_records_which_of_the_three_places_it_came_from`
`verified-by: bravebot_agent::preamble::a_turn_offered_skills_is_told_a_slash_name_is_the_user_asking_for_one`

## Known costs

- **The list is one row per command and per skill, and a screen with no room for it loses the
  last of them.** Nothing bounds it and nothing scrolls it: the rows are handed to the layout and
  whatever does not fit is dropped from the bottom, so a terminal a few rows short of the whole
  list offers the rows at the top of it and silently offers none of the ones below. The skills sit
  beneath the commands, so they are the first to go, and a bare `/` in a session holding many
  skills is the list most likely to be cut. The arrows still walk onto a row that was not drawn,
  which puts the highlight somewhere the person cannot see. Every command and skill is still
  typeable in full, and typing a letter or two narrows the list back onto the screen; what a
  short terminal costs is the discovery the list exists for. Bounding it would mean a window that
  scrolls with the cursor, which is a second scroller beside the transcript's.
