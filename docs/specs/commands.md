---
id: CMD
title: Slash commands
status: normative
governs:
  - crates/tui/src/app.rs
  - crates/tui/src/app/actions.rs
  - crates/tui/src/app/models.rs
  - crates/tui/src/app/session_commands.rs
  - crates/tui/src/app/keys.rs
  - crates/tui/src/command_table.rs
  - crates/tui/src/picking.rs
  - crates/tui/src/skills.rs
  - crates/tui/src/caffeinate.rs
  - crates/tui/src/away.rs
  - crates/tui/src/init_command.rs
  - crates/tui/src/review_command.rs
  - crates/tui/src/commit_message_command.rs
  - crates/tui/src/help_command.rs
  - crates/tui/src/prompt_files.rs
guards:
  - symbol: commands
documented-by:
  - docs/website/docs/reference/commands.md
  - docs/website/docs/customize/model.md
  - docs/website/docs/customize/effort.md
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
a different surface entirely and is [shell-mode.md](shell-mode.md). `/copy` has no other spec to
belong to, so what it copies is CMD-11, and neither has `/caffeinate`, so what it holds is CMD-12. `/init` starts an ordinary turn, but the words that turn is given and
when it is refused are CMD-13. `/review` is the same kind of turn, and what it is given and what
it may not do are CMD-18. `/help` is the same kind of turn, given a question and
the name of the skill that carries the documentation, which is CMD-24, and `/changelog` shows text
this program carries and starts no turn, which is CMD-25. `/request` shows the request a turn built, which is
[TRACE-9](trace.md#TRACE-9)'s, and `/context` divides what it measured, which is CMD-19.

**Skills are offered here, and are never commands.** A slash word is offered the skills a turn
starting now would advertise to the planner, beneath the commands at the start of a line and alone
after other words. Taking one writes `/name ` into the line and nothing else, so `/commit-style` is
still a prompt like any other sentence, and the planner is what fetches the skill it names.
[skills.md](skills.md) owns what a skill is and what each source is trusted for, and
[tools/load-skill.md](tools/load-skill.md) owns the fetch. CMD-7 says why no skill becomes a
command, and CMD-9 says what the box offers.

**An MCP server's prompts are offered here, and are never commands.** A prompt of a server this
session reached is a row of the list a slash opens, beside the skills. A line that starts with its
word is sent as the prompt it is, and the turn it starts fetches the prompt. CMD-26 says what a
line may hold and what the person is told, and [mcp.md](mcp.md) says what is labelled and why.

**Prompt files are looked up, and are never commands.** A slash word that names a file in the
person's own `~/.bravebot/prompts` is replaced in the box by the file's text, for the person to read
and send. CMD-17 says what the lookup reads and what it never does.

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

`verified-by: by-construction (every dispatch site reads a line that came off the input box: the box's own key handler at rest, the same box's handler mid-turn for a word that does not wait, and the queue that handler put the line in for when the turn ends. No path carries model output, file content or processor output into any of them)`


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

**`exit` and `quit` typed with no slash are not commands, and a line that is only one of them is
sent as the prompt it is, with a note under the box.** The first Enter on a trimmed line equal to
either word in any case sends it as it would any other prompt, and asks nothing of the person first.
While the turn it begins runs, the row under the box says that the line was sent as a prompt, that
Ctrl-C stops it and that `/exit` leaves. The note is drawn for a line the person typed in the box,
sent at once or queued behind a running turn and sent when that turn ends. A turn that a loop's tick,
a watch, a goal or a command begins carries no note, and neither does a line with anything else in it,
which includes two of these words sent together by Ctrl-Enter. In shell mode the line is a command
line (CMD-3) and runs at the first Enter, with no turn to say anything under. The row names Ctrl-C
first because a turn is running and the first press stops it ([INPUT-4](terminal-input.md#INPUT-4)).
`/exit` typed while the turn runs waits for the turn to end like every command in the second row of
the table in [CMD-8](#CMD-8). The row is short enough that any one permission mode name is still
drawn beside it on a terminal eighty columns wide.

**Why.** Other terminal agents leave on the bare word, so a person who types it may expect to leave,
and a turn that starts with nothing on the screen saying the line went to the model reads as an
interface that did not understand. Claiming the word would take it out of the language a person can
use to talk to the planner, and a question about leaving would stop being a question, so it stays a
prompt and the row says so. Holding the first Enter would add a step in front of a line that sends
at once, which no other prompt has. The line is the text of the person's own box, and what it
decides is only whether the note is drawn.

`verified-by: bravebot_tui::app::a_longer_word_starting_with_the_command_is_a_prompt`
`verified-by: bravebot_tui::app::an_argument_is_taken_only_after_the_whole_command_word`
`verified-by: bravebot_tui::app::a_prompt_containing_the_add_dir_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_status_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_cost_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_copy_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_clear_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_compact_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_model_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_theme_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_theme_is_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_effort_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_advisor_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_advisor_is_a_prompt`
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
`verified-by: bravebot_tui::app::a_prompt_containing_the_caffeinate_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_longer_word_starting_with_caffeinate_is_a_prompt`
`verified-by: bravebot_tui::app::init_with_words_after_it_is_a_prompt`
`verified-by: bravebot_tui::app::a_lone_leave_word_is_sent_at_the_first_enter`
`verified-by: bravebot_tui::app::a_line_that_holds_more_than_a_leave_word_is_sent_without_the_mark`
`verified-by: bravebot_tui::app::the_keys_the_note_on_a_sent_leave_word_names_stop_the_turn_and_leave`
`verified-by: bravebot_tui::app::the_exit_command_leaves_on_the_first_enter_and_marks_nothing`
`verified-by: bravebot_tui::app::a_bare_leave_word_in_shell_mode_is_run_as_the_command_line_it_is`
`verified-by: bravebot_tui::state::a_turn_begun_on_a_lone_leave_word_is_marked_and_one_on_any_other_line_is_not`
`verified-by: bravebot_tui::state::the_prompt_after_a_marked_turn_does_not_inherit_the_mark`
`verified-by: bravebot_tui::state::a_turn_that_begins_without_a_line_from_the_box_takes_down_the_mark`
`verified-by: bravebot_tui::state::a_lone_leave_word_that_waited_in_the_queue_is_marked_when_its_turn_begins`
`verified-by: bravebot_tui::state::leave_words_hurried_together_are_not_marked_and_one_hurried_alone_is`
`verified-by: bravebot_tui::render::the_row_under_the_box_says_a_sent_leave_word_went_to_the_model_as_a_prompt`
`verified-by: bravebot_tui::render::the_row_under_the_box_says_nothing_of_a_prompt_that_is_more_than_a_leave_word`
`verified-by: bravebot_tui::render::the_note_on_a_sent_leave_word_leaves_the_permission_mode_on_an_eighty_column_row`


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
`/model` reaches the network to list models. `/plan <task>` starts a turn, and what that turn is sent
is the task after the word, which [permission-modes.md](permission-modes.md) governs. None of them sends
the typed line.

`verified-by: bravebot_tui::app::typing_the_status_command_reports_rather_than_prompting`
`verified-by: bravebot_tui::app::typing_the_cost_command_reports_rather_than_prompting`
`verified-by: bravebot_tui::app::typing_the_copy_command_copies_the_latest_reply_rather_than_prompting`
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
`verified-by: bravebot_tui::app::the_checkouts_command_lists_and_removes_by_number`
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
`verified-by: bravebot_tui::app::the_add_dir_close_command_carries_the_directory_to_close`
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

One table names every command, its argument, its one-line description and whether it waits for a
turn in flight, and each name is a single constant the one place that dispatches matches on. The
completion list's command rows, Tab and the arrows all read the table, and so do the arms that
carry out or queue a command typed mid-turn (CMD-8), so typing `/` lists every command with what it
does and narrowing, queueing and dispatching all work on one set. The skill rows beneath them read the set a turn would advertise
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

A prompt of an MCP server (CMD-26) does not add to the set either. Its word is offered as a skill's
is and written into the box as a prompt, and what runs it is the turn the sent line starts, so no
dispatch in the box reads a word a server chose.

A prompt file (CMD-17) does not add to the set either. A name is looked up in one directory when
Enter is pressed on a line that starts with it, and is never listed, drawn as a row, or allowed to
claim a word the table holds.

`verified-by: by-construction (the table is an array of string literals fixed at compile time, and no directory listing, configuration value or turn output reaches it)`


<a id="CMD-8"></a>
### CMD-8: while a turn runs the word waits, unless it touches nothing the turn holds

A command typed while a turn is in flight is one of two kinds, and a column of the table says which.

| Kind | Commands | Enter mid-turn |
|---|---|---|
| touches nothing the turn holds | `/cost`; `/context`; `/limit` in every form; `/status`; `/copy`; `/export`; `/rename`, `/issue` and `/pr`; `/note`; `/forget-trust`; `/sandbox` in every form but `off`; `/model`, `/theme` and `/effort` in every form; `/config`; `/language` in every form; `/advisor` and `/style` in every form; `/watch` and `/jobs` in every form; `/outputs`; `/panel`; `/files`; `/caffeinate`; `/thinking`; `/request`; `/changelog`; `/loop` and `/goal` in every form but the one that starts a loop or sets a goal | carried out as it is typed |
| everything else | every other command, `/sandbox off`, and `/loop <interval> <prompt>` and `/goal <condition>` | waits for the turn to end |

A command that reads or ends something, or that changes only how the screen is drawn, what the
box edits in or what a later start reads (`/theme`, `/config` and `/language`), goes ahead of every line already waiting, and a line behind
it stays where it was. The exception is a line of the same command already waiting, which it waits
behind: `/loop stop` typed after a waiting `/loop 5m check the deploy` would find no loop to stop
and the loop would start after it, so two lines of one command are carried out in the order they
were typed. `/loop` waits behind a waiting `/after` as well, since that takes the slot a loop takes.
A command that changes what the session keeps, `/rename`, `/note`, `/issue`, `/pr`,
`/forget-trust`, `/sandbox`, `/model`, `/effort`, `/advisor` or `/style`, is carried out as it is
typed unless a line waiting ahead of it could change what it acts on, and otherwise waits behind
that line, so it lands where it was typed: `/rename` ahead of a waiting `/clear` would name the
session `/clear` leaves, `/forget-trust` ahead of a waiting `/cd` would forget the directory `/cd`
leaves, and `/effort` ahead of a waiting prompt would change the level that prompt was typed under.
Which lines could change what a command acts on depends on the command. A waiting `/sandbox` line in
any form sets the mode the next turn is built with and nothing else
([SANDBOX-22](sandboxing.md#SANDBOX-22)), so it holds back no command but another `/sandbox`:
`/note` typed behind a waiting `/sandbox off` is carried out as it is typed, and `/sandbox strict`
typed behind it waits, since the mode typed last is the one the person means to end in. `/issue` and
`/pr` act on the link the record holds ([PANEL-12](info-panel.md#PANEL-12)), which only a line that
starts another record, switches to one or leaves this one changes: `/clear`, `/handoff`, `/branch`,
`/resume`, `/bg` and `/exit`. Behind any other waiting line, a prompt and a command line included,
they are carried out as they are typed. `/model`, `/effort` and `/advisor` act on what a request is
built with. A prompt reads it, and so does every command that sends a request or leaves for another
session, and the three hold one another back, since the level a model reads depends on the model. A
command line, `/clear`, `/undo`, `/rewind`, `/hunks`, `/memory`, `/reach`, `/branch`, `/add-dir`,
`/cd` and `/mcp` read none of it and change none of it, nor does a waiting `/rename`, `/note`,
`/style`, `/forget-trust`, `/issue`, `/pr` or `/sandbox`, so behind any of those the three are
carried out as they are typed. `/rename` and `/note` act on the name and the notes the record holds. A
note is stamped with the turn count it is typed at, and a rename gives up every rewind point and is
undone by the commands that put one back, so a line that begins a turn changes what they act on:
every prompt, every command that sends a request or begins a turn, `/clear`, `/undo`, `/rewind`,
`/hunks`, `/cd`, `/compact`, `/handoff`, `/branch`, `/resume`, `/bg` and `/exit` hold them back, and
they name it. A command line adds to the conversation and saves the record with the notes and the name
it already holds, and `/add-dir`, `/memory`, `/reach`, `/mcp`, `/style`, `/forget-trust`, `/issue`,
`/pr`, `/sandbox`, `/model`, `/effort` and `/advisor` write neither, so behind any of those, and
behind a waiting `/note` for `/rename` and a waiting `/rename` for `/note`, they are carried out as
they are typed. A `/note` typed behind a waiting `!make` is written at once and stamped with the turn
the running turn is. `/style` acts on the pick that every turn reads when it is built and on the
person's own style files its name is looked up among, which a program cannot write. A file is read
only where its path does not land in the reach the workspace holds: the root, the directories added,
the checkouts the session kept and the paths programs were let reach. A prompt, every command that
begins a turn, and `/clear`, `/cd`, `/add-dir`, `/reach`, `/branch`, `/handoff`, `/resume`, `/bg`
and `/exit` hold it back, since each moves or ends that reach or leaves for another session, and it
names the line. A command line and `/compact`, `/btw`, `/recap`, `/memory`, `/undo`, `/rewind`,
`/hunks`, `/mcp`, `/after`, `/rename`, `/note`, `/forget-trust`, `/issue`, `/pr`, `/sandbox`,
`/model`, `/effort` and `/advisor` do not, so behind any of those it is carried out as it is typed:
`/undo`, `/rewind` and `/hunks` put back files the workspace resolves and move no reach, `/mcp`
starts or ends a server confined so that it cannot write under `~/.bravebot`, and `/after` arms a
prompt that goes out when its delay is up and sends nothing now, so a style typed after it is the
one the turn it sends is built with. The project's own style names are the list held when the line
was typed and the turn reads the pick under its own trust map, so a rewind that changes the map or
the files under the root does not change which names `/style` takes. `/forget-trust` acts on the answer kept about the working directory
and the git worktree around it ([TRUST-24](trust-map.md#TRUST-24)). A prompt and a command line can
run a program that makes the `.git` entry that moves which root is around the directory, a line that
starts a session here reads the answer, and one that moves the directory or leaves the session
changes what is withdrawn, so all of those hold it back, as do `/branch`, which replaces the
session's record and its reach and has not been read against the answer, and `/mcp`, `/undo`,
`/rewind` and `/hunks`, which have not been shown unable to make or remove the `.git` entry. A rewind
removes what a file tool wrote, which may be a `.git` entry written into an added directory that
holds the working directory, and a server `/mcp` starts may write the directory its declaration
names. Behind
`/compact`, `/btw`, `/recap`, `/memory`, `/add-dir`, `/reach`, `/after`, `/rename`, `/note`, `/style`,
`/issue`, `/pr`, `/sandbox`, `/model`, `/effort` and `/advisor` it is carried out as it is typed:
`/after` arms a prompt that goes out when its delay is up and runs nothing now, so the turn it sends
runs under the answer as `/forget-trust` left it whichever of the two is carried out first.
`/add-dir` and `/reach` start no program and move neither the working directory nor the root. The first
opens or closes a directory and a rule in the session's own trust map, and the second writes the
reach record or the workspace's list of paths programs may reach, and neither writes a file the
answer is kept in or makes a `.git` entry, so `/forget-trust` withdraws the same answers before them
as after them. A
command carried out as it is typed comes off the box and is not remembered, as at rest. What it says goes into the transcript as it is said, so it scrolls with the turn's own
entries and the rest of the history, and it stays there once the turn has ended. Ctrl-Enter on one stops nothing: the command is already done,
and stopping the turn would send the prompts waiting behind it, which nobody asked to hurry.

A command that waits is taken off the box and joins the lines waiting for the turn to end, exactly
as a prompt does: the box clears, the history remembers it, and it is drawn under the box marked as
waiting. It says why it waits, in a line held under the turn as a command's answer is: it acts on
what the turn holds or starts a turn of its own, or it waits behind a line typed before it so that it lands where
it was typed, or, for `/sandbox off`, it asks first and the turn has the screen until it ends. A command that waits behind a line
names the first line waiting that holds it back: a command as it was typed, such as `/clear`, and
`the prompt before it` or `the shell line before it` where that line is not a command. Its mark
under the box says the same in place of `carried out after this`, as `after /clear`, where the
row has the width for it, and is left as it was for a command that waits for the turn on its own
account, since the lines ahead of that one are not why. What it waits for is different. It is never offered to the turn in flight, so nothing about
it reaches the planner, and when the queue reaches it, it is carried out rather than sent. The queue
is drained in the order the lines were typed, so a command behind a prompt waits for that prompt's
turn.

Neither kind is offered to the turn in flight. Which lines are commands is CMD-2's rule and nothing
narrower, so a sentence mentioning a command is a prompt mid-turn as it is at rest, and a word the
table gives no argument is a prompt with anything after it. A compaction, an aside and the other
loops that share the working status are not a turn, and every command typed during one waits.

**The commands that rewrite the conversation or the workspace wait.** `/clear`, `/compact`,
`/undo`, `/rewind`, `/cd`, `/add-dir` and `/exit` are not carried out while the turn runs, and do
not stop it: stopping a turn costs the work it has done, and a person who typed a command did not
ask for that. Each says why it waits, and Ctrl-Enter on one stops the turn and carries the command
out, as it does for a waiting prompt, since that press is the person asking for it to be hurried. A
turn stopped that way gives its prompt back to the box when it recorded nothing.

**Why the second kind waits.** Enter mid-turn already means the line waits, and that is what a
person pressing it expects of a line they type. What the queue must not do is send a command: a line
waiting there used to be a prompt like any other, and the running turn takes those at its next round
boundary, so a queued `/clear` asked the planner what to clear. Carrying one of these out as it is
typed is no better. Each acts on the conversation, the workspace, the terminal or the network, and
the turn holds all four, so it would change what the turn is running under. Starting a loop or
setting a goal is in this kind for the same reason: `/loop 5m check the deploy` sends its first tick
at once, and a second turn may not begin while one is in flight, while a goal set mid-turn would
have the turn in flight judged against a condition it was never sent with.

**Why the first kind does not.** A loop, a goal, a watch and the spend so far are the session's own,
and the turn holds none of them, so reading or ending one changes nothing the turn is using. The
moment these are wanted is mid-turn: a person who has seen enough of a loop types `/loop stop` while
its tick runs, and an ending carried out after that tick would cost them the next one too if the
queue held a prompt. What a tick asks for once its loop is gone is
[loop.md](loop.md#LOOP-11)'s. The rest of the kind changes what the session keeps and the turn
does not read:

- **`/rename`.** The turn is handed the record's id, and the record is written after the turn
  under whatever name the session has by then. The rename gives up every rewind point
  ([SESSION-19](sessions.md#SESSION-19)), the one the running turn opened among them. `/rename`
  with no name renames nothing and gives up none.
- **`/note`.** The note is written to the record at once, on the turn that is running, and the turn
  is handed nothing ([SESSION-36](sessions.md#SESSION-36)). It gives up no rewind point, since no
  point holds a note. `/note` with no text records nothing: it lists the notes the session holds, or says what it needs when it holds none.
- **`/issue` and `/pr`.** The record is written after the turn with whatever links the session has
  by then, and no turn reads them ([PANEL-12](info-panel.md#PANEL-12)).
- **`/forget-trust`.** What it takes back is the answer kept for the next session in the directory
  ([TRUST-24](trust-map.md#TRUST-24)). The turn runs under the map this session opened with, which
  the command leaves alone.
- **`/model`.** A turn is sent with the model in force when it begins and does not read the choice
  again, so a model picked mid-turn is the next turn's. The turn is still compared with the model
  it was sent with when it ends, and not with the one picked meanwhile. What `/status` and the
  footer name, and the window the context gauge is read against, are the model picked from the
  moment it is picked, since the next request is the one that has to fit.
- **`/theme` and `/effort`.** A theme changes only how the screen is drawn. A turn is sent with the
  level in force when it begins and does not read it again, so a level set mid-turn is the next
  turn's. A ChatGPT account is sent the level its session began with, so a level set there applies
  from the next session ([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)).
- **`/language`.** It saves the language the interface is read in from the next start, and the
  catalog is chosen once, so nothing the turn holds or the screen draws changes
  ([LOCALE-8](localization.md#LOCALE-8)).
- **`/config`.** It picks the style the box edits in. The turn does not read it. The keys that stop
  the turn do, so with vi's style chosen Escape reaches the turn only once the box has no use for
  it ([INPUT-24](terminal-input.md#INPUT-24)).
- **The pickers.** The bare word of `/model`, `/theme`, `/effort` and `/config` opens a list. While a turn
  runs the list is a mode of the session and not a loop of its own, so the turn goes on being drawn
  and asked about. The list takes every key the box would have taken, is drawn over the frame with
  the turn still drawn behind it, and is left with Escape or Ctrl-C, which stop nothing. A question
  the turn asks takes the screen for as long as it is open, and the list is there again once it is
  answered. The theme under the cursor is in force while the list is open, and Escape puts back the
  one that was. A row taken is carried out as a name typed is, and what it says goes into the
  transcript as it is said. A list still open when the turn ends stays open and takes the next key. The roster `/model`
  lists is asked for on the thread that draws the turn, so a slow listing holds the drawing, and
  any question the turn asks, until it returns.
- **`/sandbox`.** It sets the mode the programs `run` starts are held to
  ([SANDBOX-22](sandboxing.md#SANDBOX-22)). A turn is built with the mode in force when it begins and
  does not read it again, so every stage of the running turn, those already started and those yet
  to start, keeps that mode, and a mode set mid-turn is the next turn's. The reply says so. The bare word reports the mode and says nothing of the turn.
  `/sandbox off` waits: it asks before programs start with no profile, and a question put while the
  turn has the screen would be answered over the turn's own. A managed floor refuses a looser mode
  as at rest, so the refusal is said under the turn. `/status` mid-turn names the mode set
  meanwhile, which is the one the next request is built with.
- **`/advisor`.** It names the model a later turn's planner may consult
  ([ADVISOR-9](tools/advisor.md#ADVISOR-9)). A turn is offered its advisor, or none, when it
  begins and does not read the choice again, so one named mid-turn is the next turn's. The bare word
  opens no picker, so it is carried out too.
- **`/style`.** It picks the words that open a later turn's system prompt
  ([INSTR-12](instructions.md#INSTR-12)). A turn reads the pick when it begins, so one made mid-turn
  is the next turn's. It opens no picker, so every form is carried out.

- **`/copy`.** It reads the transcript and writes to the clipboard, which a sweep with the mouse
  may do at any time. Mid-turn the latest reply may be what the running turn said on its way to a
  tool call, since that is in the transcript once it is drawn.

- **`/export`.** It reads the transcript, which the session keeps, and writes a file under the
  working directory by the rules it has at rest ([SESSION-17](sessions.md#SESSION-17)): a path that
  leaves the directory or names a file that is already there is refused, and the refusal goes into
  the transcript as it is said. The turn works in that directory, but a new file there changes neither the
  directory it works in nor what it may reach, and a name that is taken is never replaced, so
  nothing the turn wrote is lost to it. The file holds the transcript as it stands when the line is
  typed, so a reply the turn is still writing is not in it, and neither is a prompt waiting behind
  the turn. The prompt the turn is working on is marked as still running. A waiting `/cd` or
  `/clear` has not happened yet, so the file goes where the turn is working and holds what the
  transcript holds now, as `/copy` copies the reply it holds now.

- **`/status`.** It reads what the session keeps, the workspace and the configuration the turn was
  started with, none of which it changes. The trust map and the vouched programs are the turn's: it
  answers into both as it runs, so a copy taken when it began would state "every run is asked" about
  an earlier moment. The report says they are held by the running turn and states neither, and shows
  them once the turn has ended.

**Why `/jobs stop` is in the first kind.** A job is the turn's, and stopping one changes what the
turn is running. The command does it only by setting a token the driver made for that job, which the
turn reads at its own next step ([RUN-27](tools/run.md#RUN-27)), as the turn reads the stop key. A
job lives only as long as its turn, so a stop that waited for the turn to end would find nothing to
stop.

**What a command says mid-turn is not the turn's doing.** What a keystroke wrote into the
transcript would count as the turn having done something, which is what decides whether a stopped
prompt comes back to be edited, so the entries a command carried out mid-turn adds are not counted:
a stop after one still gives the prompt back where the turn recorded nothing. A waiting command is
held in the queue, and taking back what is waiting gives it back to the box like any other line.
What a command carried out mid-turn says is in the transcript from the moment it is said.

While work runs the commands are offered as at rest ([INPUT-9](terminal-input.md#INPUT-9)), each row
marked with the column above: `now` for a command that is carried out as it is typed, `queued` for
one that waits, and `now/queued` for one that is carried out or waits by what follows the word and
by what is already waiting; a `now` row reads `queued` while a line of the same command waits
(above). During a compaction or an aside every row reads `queued`. Tab, or Enter
on a half-typed word, takes the highlighted row and queues nothing; Enter on a whole command is
carried out or queued as above.

`verified-by: bravebot_tui::app::a_command_typed_while_a_turn_runs_is_not_sent_as_a_prompt`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`
`verified-by: bravebot_tui::app::a_command_that_reads_or_ends_what_the_session_keeps_answers_mid_turn`
`verified-by: bravebot_tui::app::a_command_that_would_start_a_loop_or_a_goal_waits_for_the_turn`
`verified-by: bravebot_tui::app::a_command_typed_behind_a_waiting_one_of_its_own_waits_with_it`
`verified-by: bravebot_tui::app::status_asked_mid_turn_answers_now_and_leaves_the_turns_rules_unstated`
`verified-by: bravebot_tui::status::a_report_whose_rules_are_the_turns_states_neither_the_trust_nor_the_programs`
`verified-by: bravebot_tui::app::a_command_typed_during_a_compaction_waits`
`verified-by: bravebot_tui::app::a_stopped_prompt_comes_back_after_a_command_answered_mid_turn`
`verified-by: bravebot_tui::app::ctrl_enter_on_a_command_answered_mid_turn_hurries_nothing`
`verified-by: bravebot_tui::app::a_session_renamed_mid_turn_is_renamed_as_it_is_typed`
`verified-by: bravebot_tui::app::a_session_renamed_mid_turn_gives_up_the_running_turns_rewind_point`
`verified-by: bravebot_tui::app::a_rename_with_no_name_mid_turn_keeps_the_running_turns_rewind_point`
`verified-by: bravebot_tui::app::a_link_set_mid_turn_is_set_as_it_is_typed`
`verified-by: bravebot_tui::app::a_command_that_changes_something_waits_behind_what_was_typed_first`
`verified-by: bravebot_tui::app::a_note_typed_behind_a_waiting_sandbox_off_is_carried_out_when_typed`
`verified-by: bravebot_tui::app::every_settings_command_but_sandbox_is_carried_out_behind_a_waiting_sandbox_off`
`verified-by: bravebot_tui::app::a_sandbox_mode_typed_behind_a_waiting_sandbox_off_waits_behind_it`
`verified-by: bravebot_tui::app::a_link_typed_behind_a_line_that_keeps_the_record_is_set_when_typed`
`verified-by: bravebot_tui::app::a_link_typed_behind_a_line_that_replaces_the_record_waits_behind_it`
`verified-by: bravebot_tui::app::a_model_level_or_advisor_typed_behind_a_line_that_builds_no_request_is_set_when_typed`
`verified-by: bravebot_tui::app::a_model_level_or_advisor_typed_behind_a_line_that_builds_a_request_waits_behind_it`
`verified-by: bravebot_tui::app::a_choice_names_the_first_waiting_line_that_could_change_it`
`verified-by: bravebot_tui::command_table::every_command_that_can_wait_is_placed_against_what_a_waiting_line_could_change`
`verified-by: bravebot_tui::app::a_style_typed_behind_a_line_that_cannot_change_it_is_carried_out_when_typed`
`verified-by: bravebot_tui::app::a_style_typed_behind_a_line_that_could_change_it_waits_behind_it`
`verified-by: bravebot_tui::app::a_forgotten_answer_typed_behind_a_line_that_cannot_change_it_is_carried_out_when_typed`
`verified-by: bravebot_tui::app::a_forgotten_answer_typed_behind_a_line_that_could_change_it_waits_behind_it`
`verified-by: bravebot_tui::app::opening_or_closing_a_directory_does_not_change_what_forgetting_trust_withdraws`
`verified-by: bravebot_agent::reach::a_reach_leaves_the_answer_kept_about_the_directory_as_it_was`
`verified-by: bravebot_tui::app::a_note_or_a_name_typed_behind_a_line_that_leaves_the_record_alone_is_carried_out_when_typed`
`verified-by: bravebot_tui::app::a_note_or_a_name_typed_behind_a_line_that_could_change_the_record_waits_behind_it`
`verified-by: bravebot_session::sessions::a_save_for_a_command_line_keeps_the_name_and_the_notes_given_before_it`
`verified-by: bravebot_tui::app::a_theme_or_config_typed_behind_a_waiting_line_is_carried_out_when_typed`
`verified-by: bravebot_tui::app::a_language_typed_behind_a_waiting_line_is_carried_out_when_typed`
`verified-by: bravebot_tui::render::a_command_waiting_behind_a_line_names_that_line_on_its_mark`
`verified-by: bravebot_tui::render::a_command_that_waits_for_the_turn_does_not_name_the_line_ahead_of_it`
`verified-by: bravebot_tui::render::a_command_does_not_name_a_line_typed_after_it`
`verified-by: bravebot_tui::render::a_mark_with_no_room_for_the_line_it_waits_behind_says_where_the_line_goes`
`verified-by: bravebot_tui::app::a_command_names_the_first_waiting_line_that_holds_it_back`
`verified-by: bravebot_tui::app::export_typed_mid_turn_writes_the_transcript_as_it_stands`
`verified-by: bravebot_tui::app::export_typed_mid_turn_refuses_a_path_outside_the_working_directory`
`verified-by: bravebot_tui::app::every_command_carried_out_mid_turn_answers_in_the_transcript`
`verified-by: bravebot_tui::app::ctrl_enter_on_a_rename_mid_turn_hurries_nothing`
`verified-by: bravebot_tui::app::trust_forgotten_mid_turn_is_forgotten_as_it_is_typed`
`verified-by: bravebot_tui::app::an_effort_named_mid_turn_is_set_as_it_is_typed`
`verified-by: bravebot_tui::app::a_language_named_mid_turn_is_saved_as_it_is_typed`
`verified-by: bravebot_tui::app::a_sandbox_mode_named_mid_turn_is_set_as_it_is_typed_and_the_turn_keeps_its_own`
`verified-by: bravebot_tui::app::the_sandbox_report_is_given_mid_turn`
`verified-by: bravebot_tui::app::moving_the_sandbox_to_off_mid_turn_waits_and_says_why`
`verified-by: bravebot_tui::app::a_sandbox_mode_named_behind_a_waiting_line_waits_behind_it`
`verified-by: bravebot_tui::app::a_stopped_prompt_comes_back_after_a_sandbox_mode_set_mid_turn`
`verified-by: bravebot_tui::app::a_command_that_waits_for_the_turn_says_why`
`verified-by: bravebot_tui::app::a_theme_or_an_effort_nobody_has_is_refused_mid_turn`
`verified-by: bravebot_tui::app::a_command_that_opens_a_picker_opens_it_as_it_is_typed_mid_turn`
`verified-by: bravebot_tui::app::a_level_picked_in_a_picker_opened_mid_turn_is_kept_and_said_in_the_transcript`
`verified-by: bravebot_tui::app::leaving_a_picker_opened_mid_turn_does_not_stop_the_turn`
`verified-by: bravebot_tui::app::a_picker_still_open_when_the_turn_ends_goes_on_at_rest`
`verified-by: bravebot_tui::app::a_row_taken_while_another_working_loop_runs_is_carried_out_afterwards`
`verified-by: bravebot_tui::app::the_model_command_opens_the_model_picker_as_it_is_typed_mid_turn`
`verified-by: bravebot_tui::app::a_model_picked_mid_turn_is_the_next_turns_and_the_running_turn_keeps_its_own`
`verified-by: bravebot_tui::app::a_paste_while_a_picker_is_open_lands_nowhere`
`verified-by: bravebot_tui::app::a_picker_left_open_takes_the_keys_while_a_command_runs`
`verified-by: bravebot_tui::render::a_picker_opened_mid_turn_is_drawn_over_the_turn`
`verified-by: bravebot_tui::theme_prompt::a_key_press_previews_the_row_and_leaving_puts_the_old_theme_back`
`verified-by: bravebot_tui::render::what_a_command_answered_mid_turn_is_drawn_under_the_turn`
`verified-by: bravebot_tui::app::no_command_is_sent_as_a_prompt_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_command_with_an_argument_is_not_sent_as_a_prompt_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_command_that_takes_no_argument_is_only_the_bare_word_mid_turn`
`verified-by: bravebot_tui::app::a_prompt_mentioning_a_command_is_queued_while_a_turn_runs`
`verified-by: bravebot_tui::app::the_command_queued_while_a_turn_ran_is_carried_out_when_the_turn_ends`
`verified-by: bravebot_tui::app::a_prompt_queued_behind_a_command_is_sent_once_the_command_has_run`
`verified-by: bravebot_tui::app::a_queued_command_is_not_what_the_turn_took`
`verified-by: bravebot_tui::app::a_queued_command_comes_back_to_the_box`
`verified-by: bravebot_tui::app::copy_typed_mid_turn_takes_the_reply_without_waiting`
`verified-by: bravebot_tui::render::each_command_row_says_whether_it_runs_now_or_waits_while_a_turn_runs`
`verified-by: bravebot_tui::render::every_row_is_queued_during_an_aside`
`verified-by: bravebot_tui::render::a_row_reads_queued_while_a_line_of_its_command_waits`
`verified-by: bravebot_tui::render::the_rows_carry_no_label_when_nothing_is_running`
`verified-by: bravebot_tui::app::enter_completes_a_half_typed_command_instead_of_queueing_it_while_a_turn_runs`
`verified-by: bravebot_tui::app::a_whole_command_is_still_carried_out_or_queued_while_a_turn_runs`

## What a slash word is offered

<a id="CMD-9"></a>
### CMD-9: the skills a turn would advertise, and taking one writes a prompt

A slash word at the start of the line is offered the commands it could still become and, beneath
them, the skills whose names start with it, including a word that is a command in full, since a
longer skill name may extend it (`/pr` lists `pr-fix`). A slash word after other words is offered
the skills alone, since there a command is a prompt (CMD-2). The skills are the ones a turn starting
now would advertise to the planner, read the same way, and those its file keeps from the planner,
which no turn advertises and which the box lists because it is where a person finds the name to type
([SKILL-20](skills.md#SKILL-20)). Each row says when to use the skill and where it was found: the
project, the person's own directory, or this program, and a skill kept from the planner says that too.
They are read when a line
first holds a slash word and let go when it holds none (CMD-10 also needs the names of a finished
word), so a skill written while the box sat idle is offered at the next line with a slash.

Tab, or Enter on a half-typed name, replaces the word with `/name ` and nothing else; Enter on a
name typed in full sends the line, unless the arrows moved onto another row first. Either way the line is a prompt, sent as it reads. The planner
is told that a prompt naming a skill as `/name` is the person asking for it, and it loads that
skill the way it loads any other. A skill its file keeps from the planner is the exception: the turn
supplies it for the line that names it and the planner makes no call ([SKILL-20](skills.md#SKILL-20)).
The arrows walk down the commands and on into the skills.

Nothing is offered in shell mode, while work runs (the commands alone are, CMD-8), or after the
word of a command line, which is CMD-3 and CMD-5 holding for skills as they hold for commands. A
skill whose name a command claims, or whose name holds a space or a control character, is never a
row.

**Why.** A person who knows which skill a task wants should be able to say so without hoping the
planner picks it from its description, and the name is easier to take from a list than to
remember. Writing the name into a prompt, rather than making it a command that loads the body,
keeps the choice where every other skill choice is made: the planner asks for the skill and
fetches it, so one named this way is loaded, recorded and bounded exactly as one it chose on its
own.

Reading the skills once a line, rather than once a session or once a key, is because resolving
reads directories: once a session would go on offering a skill that was deleted, and once a key
would read them on every letter.

`verified-by: bravebot_tui::app::a_slash_offers_the_skills_after_the_commands`
`verified-by: bravebot_tui::app::a_skill_is_completed_mid_sentence_and_sent_as_a_prompt`
`verified-by: bravebot_tui::app::enter_completes_a_half_typed_skill_and_sends_a_whole_one`
`verified-by: bravebot_tui::app::the_arrows_walk_from_the_commands_onto_the_skills`
`verified-by: bravebot_tui::app::enter_takes_a_skill_row_the_arrows_moved_onto_instead_of_the_typed_command`
`verified-by: bravebot_tui::app::a_command_word_typed_in_full_still_offers_the_skills_extending_it`
`verified-by: bravebot_tui::app::the_skills_are_resolved_once_a_line_and_let_go_after_it`
`verified-by: bravebot_tui::app::no_skill_is_offered_in_a_command_line_or_inside_a_command`
`verified-by: bravebot_tui::app::a_slash_offers_every_command_and_no_skill_while_a_turn_runs`
`verified-by: bravebot_tui::skills::the_word_being_typed_is_the_last_one_on_the_line`
`verified-by: bravebot_tui::skills::nothing_is_offered_inside_a_command_line`
`verified-by: bravebot_tui::skills::what_matches_is_every_name_starting_with_the_word_in_name_order`
`verified-by: bravebot_tui::skills::no_offered_skill_completes_to_a_command_line`
`verified-by: bravebot_tui::render::a_skill_row_says_where_it_came_from_within_the_width`
`verified-by: bravebot_agent::skills::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_agent::skills::each_skill_records_which_of_the_three_places_it_came_from`
`verified-by: bravebot_agent::preamble::a_turn_offered_skills_is_told_a_slash_name_is_the_user_asking_for_one`
`verified-by: bravebot_tui::render::a_skill_row_says_when_only_the_person_can_have_it_loaded`
`verified-by: bravebot_tui::skills::a_skill_kept_from_the_planner_is_offered_marked_beside_one_that_is_not`

<a id="CMD-10"></a>
### CMD-10: a recognised slash word is drawn in its own colour, and shows what it takes

In the box, a word that names a command or a skill in full is drawn in the colour of the prompt.
A command is recognised where the line is that command (CMD-2), so `/undo the last change` is
drawn as the prompt it is. A skill is recognised anywhere in the line, since it may be named
mid-sentence, and not inside a command line, whose argument is taken verbatim. A half-typed name,
a name nothing holds, and every word in shell mode are drawn as any other text.

When the line is a recognised word, a space is after it and the caret is at the end, what the word
takes is drawn dimly after the caret: the argument the command table names, or the skill's
`argument-hint` where its file has one ([skills.md](skills.md)). The first character typed
replaces it. It is cut to the row with an ellipsis rather than wrapped, so it never takes a row
the box was not sized for, and a control character in a skill's hint is drawn as a glyph.

**Why.** Colour says the word was understood before Enter is pressed, and the hint says what to
type next without opening a list that has already been closed by the space. Both are read off
names the program holds and the person typed, and the only thing decided from them is how a cell
is drawn. The skills are held while the line holds a slash word, and not only while one is being
typed, because a finished word's name is what is checked. A turn running holds none, so a skill
is not drawn as recognised then, while a command is.

`verified-by: bravebot_tui::render::a_word_that_names_a_command_or_skill_is_drawn_in_its_own_colour`
`verified-by: bravebot_tui::render::nothing_is_drawn_as_recognised_in_shell_mode`
`verified-by: bravebot_tui::render::a_command_shows_what_it_takes_once_it_is_typed`
`verified-by: bravebot_tui::render::a_skill_shows_the_hint_its_file_gave`
`verified-by: bravebot_tui::render::a_turn_running_draws_no_skill_as_recognised`
`verified-by: bravebot_tui::render::a_hint_is_cut_to_the_row_and_holds_no_escape`
`verified-by: bravebot_tui::app::the_skills_are_resolved_once_a_line_and_let_go_after_it`

## Copying a reply

<a id="CMD-11"></a>
### CMD-11: `/copy` puts a reply on the clipboard as the transcript holds it

`/copy` puts the latest reply on the clipboard, and `/copy <n>` the reply `n` back, so `/copy 1` is
the latest. A reply is what the planner said to the person: the answer a turn or a manifest run ends
on, and what it said on its way to a tool call, in this session or in the one it resumed. A prompt,
a note, a tool's row, a delegate's work, an aside's answer and a reply still arriving are not
replies, and a blank reply is not counted. What is copied is the markdown the planner wrote, with
none of the marker, indent or wrapping the screen draws it with. Every control character but a line
break and a tab is left out, as the screen leaves it out of a reply, so a Windows line ending is
copied as one break. How many characters went is drawn at the right of the hint row, where a
sweep's copy is reported, until the next prompt is sent. A sweep's highlight is taken down.

Anything but digits after the word, zero, a session with no reply, and a number past the oldest
reply are each refused with a note, and nothing reaches the clipboard. The note for a number past
the oldest says how many replies there are. A copy that no clipboard tool took and that could not be
written to the terminal says so. A terminal that ignores the request gives no answer, so that case
is reported as a copy, as it is after a sweep.

**Why.** A sweep with the mouse copies what was drawn: the marker before a reply's first row, the
indent before every other row, and a line break wherever the terminal wrapped a paragraph. Pasted
into an editor or a message, all of that has to be removed by hand. The transcript holds the reply
before any of it was drawn. A control character is left out because the screen showed none, and
because what is pasted into a shell is read as typed: a reply holding the sequence that ends a
bracketed paste would run what followed it. The count goes with the next prompt because no key
takes it down, and while it is drawn the hint that a picture is on the clipboard is not. A
highlight left up would say the swept text is what the clipboard holds.

Nothing labelled is copied. A tool's quarantined content is drawn in a margin of its own
([terminal-transcript.md](terminal-transcript.md#VIEW-3)) and is never part of a reply, since the
planner never read it. A reply is planner output released for display
([terminal-transcript.md](terminal-transcript.md#VIEW-6)), which a sweep could already copy off the
screen and `/export` already writes to a file.

`verified-by: bravebot_tui::app::typing_the_copy_command_copies_the_latest_reply_rather_than_prompting`
`verified-by: bravebot_tui::app::copy_takes_how_many_replies_back`
`verified-by: bravebot_tui::app::copy_counts_only_the_replies`
`verified-by: bravebot_tui::app::a_blank_reply_is_not_one_to_copy`
`verified-by: bravebot_tui::app::copy_refuses_what_it_cannot_take_and_copies_nothing`
`verified-by: bravebot_tui::app::a_copied_reply_carries_no_control_character_but_its_breaks_and_tabs`
`verified-by: bravebot_tui::app::a_copy_says_how_much_it_took_and_a_failed_one_says_so`
`verified-by: bravebot_tui::app::a_copy_takes_down_what_an_earlier_sweep_left_up`
`verified-by: bravebot_tui::app::a_copys_count_is_taken_down_by_the_next_prompt`
`verified-by: bravebot_tui::app::copy_reaches_the_replies_a_resumed_session_brought_back`
`verified-by: bravebot_tui::app::copy_typed_mid_turn_takes_the_reply_without_waiting`

## Keeping the machine awake

<a id="CMD-12"></a>
### CMD-12: `/caffeinate` holds off idle sleep while work is pending, and only then

`/caffeinate` turns keeping the computer awake on, and typed again turns it off. It is off when a
session starts. While it is on, the platform's inhibitor runs whenever the session has work pending,
and is ended once it has none. Work is pending while a turn is in flight and while a loop has a tick
to come, which includes the wait a turn asked for with `schedule_next`
([tools/schedule-next.md](tools/schedule-next.md#SCHED-1)). A background job is covered by its turn,
and a job the session keeps past its turn ([tools/run.md](tools/run.md#RUN-15)) holds nothing
between turns, since the person can see it and stop it there. A watch waiting on a file holds
nothing, and neither does an idle session.

The first `/caffeinate` in a person's state directory turns nothing on. It says what the command
does, that the display can still turn off and the screen can still lock, and that the machine keeps
running with their credentials on it while they are away, and it asks for `/caffeinate` again. That
second one turns it on and writes `confirmed` to `~/.bravebot/caffeinate`, after which it turns on at
once. An incognito session neither reads nor writes that file, so it explains every time.

The inhibitor is a fixed argument vector, and only idle sleep is held off:

| Platform | Program and arguments |
|---|---|
| macOS | `/usr/bin/caffeinate -i -w <this process's id>` |
| Windows | `powershell -NoProfile -NonInteractive -EncodedCommand <script>`, the script calling `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED)` and then reading its standard input to the end |
| every other platform | `systemd-inhibit --what=idle --who=bravebot --why=/caffeinate --mode=block <cat>`, `<cat>` being the absolute path of `cat` |

`<cat>` is found as [SANDBOX-30](sandboxing.md#SANDBOX-30) says and passed as an absolute path,
because a lookup of the bare name by `systemd-inhibit` would not judge where a link in a searched
entry leads. When no `cat` is found that way the inhibitor does not start. Whichever platform's
program it is, it is started with only the `PATH` entries that lookup searches, so a program it
looks up by name is found only among them.

Each one's standard input is a pipe this process holds, and each ends the hold when this process
ends, so a crash leaves no machine held awake. An inhibitor that will not start, and one that exits
by itself, is said in the transcript with the program's name, and `/caffeinate` is then off.

**Why.** A laptop that sleeps during a long turn, a loop or a `schedule_next` wait stops the request
in flight and the programs the turn runs, and the person comes back to a stalled session or a failed
request. Holding the machine only while work is pending keeps it from sleeping through that and lets
it sleep as usual otherwise; a watch is left out because it can wait for as long as the session is
open. The screen still locks, but the machine stays up with whatever credentials it holds, so the
command is off until a person has read that and asked for it a second time, which is where a person
working under a device policy finds out whether it is allowed. A hold that could not be kept turns
the command off, so nobody walks away from a machine they believe is held. Nothing from a model, a
file or a setting reaches the argument vector, and the one program it names is resolved from
directories no confined stage could have written, so the inhibitor is not a process running code we
did not write ([sandboxing.md](sandboxing.md)).

`verified-by: bravebot_tui::state::a_loop_waiting_for_its_next_tick_holds_the_machine_awake`
`verified-by: bravebot_tui::state::a_turn_holds_the_machine_awake_until_it_ends`
`verified-by: bravebot_tui::state::a_missing_inhibitor_is_said_and_turns_caffeinate_off`
`verified-by: bravebot_tui::state::the_first_caffeinate_says_what_it_does_and_holds_nothing`
`verified-by: bravebot_tui::app::the_caffeinate_command_explains_before_it_turns_on`
`verified-by: bravebot_tui::caffeinate::an_idle_session_holds_nothing_while_caffeinate_is_on`
`verified-by: bravebot_tui::caffeinate::pending_work_is_held_and_released_once_it_is_done`
`verified-by: bravebot_tui::caffeinate::caffeinate_off_holds_nothing_and_turning_it_off_releases`
`verified-by: bravebot_tui::caffeinate::a_missing_inhibitor_is_reported_and_turns_caffeinate_off`
`verified-by: bravebot_tui::caffeinate::the_first_caffeinate_explains_and_the_second_turns_it_on`
`verified-by: bravebot_tui::caffeinate::starting_a_program_that_does_not_exist_fails`
`verified-by: bravebot_tui::caffeinate::an_inhibitor_that_ended_by_itself_is_reported`
`verified-by: bravebot_tui::caffeinate::the_macos_inhibitor_holds_idle_sleep_for_this_process_alone`
`verified-by: bravebot_tui::caffeinate::the_inhibitor_holds_idle_sleep_and_starts_the_cat_it_was_found_at_byte_for_byte`
`verified-by: bravebot_tui::caffeinate::the_inhibitor_runs_the_installed_cat_and_never_one_a_stage_may_have_written`
`verified-by: bravebot_tui::caffeinate::an_inhibitor_with_no_cat_but_a_planted_one_is_not_started`
`verified-by: bravebot_tui::caffeinate::a_program_the_inhibitor_starts_by_name_is_never_one_a_stage_may_have_written`
`verified-by: bravebot_session::store::only_the_confirmed_word_is_an_agreement_to_caffeinate`

## Drafting the project's instructions

<a id="CMD-13"></a>
### CMD-13: `/init` asks the planner to draft `AGENTS.md`, and never where one exists

`/init` starts an ordinary turn whose prompt is a fixed text written in this program: write
`AGENTS.md` in the working directory, titled "Repository Guidelines", of 200 to 400 words, covering
structure, build and test commands, style, testing, and commit and pull request conventions. It
takes no argument, so `/init` followed by words is a prompt (CMD-2), and nothing the person typed
reaches the fixed text. The prompt is what the transcript shows as sent.

Where the working directory already holds a file named `AGENTS.md`, nothing starts: the command says
so and no turn runs. This is a check on the name, made without reading the file, so a link with no
target counts and what an existing file says cannot decide anything.

The turn reads what any turn may read, and the prompt adds no reach. In a workspace the person
vouched for, the planner reads the project's files and writes from them. In one they did not, a read
of a project file returns a reference and not the lines ([READ-1](tools/read-file.md#READ-1)), and
the prompt tells the planner to ask the person with `ask_user` and write from their answers. The
write is `write_file`, so it is shown and asked about like any other write, and a file written under
this name is read from the next turn on ([INSTR-7](instructions.md#INSTR-7)).

**Why.** A guide drafted from bytes nobody vouched for would be a standing instruction file built from
untrusted content, and the planner never has those bytes ([AGENTS.md](../../agents/AGENTS.md)), so
the only footing it has there is what the person says. Refusing on the name keeps `/init` from
offering a write over a file somebody wrote, and keeps the check from depending on content.

`verified-by: bravebot_tui::app::the_init_command_starts_a_turn_with_the_drivers_own_prompt`
`verified-by: bravebot_tui::app::the_init_command_starts_no_turn_where_agents_md_exists`
`verified-by: bravebot_tui::init_command::a_file_of_the_name_is_there_whatever_it_holds`
`verified-by: bravebot_tui::init_command::a_link_with_no_target_is_there`
`verified-by: bravebot_tui::init_command::the_prompt_names_no_path_for_the_person_to_vouch_for`
`verified-by: bravebot_tui::init_command::the_prompt_asks_for_the_title_the_length_the_five_areas_and_the_tools`
`verified-by: bravebot_agent::turn::a_file_the_planner_may_not_see_is_reserved_rather_than_opened`

<a id="CMD-18"></a>
### CMD-18: `/review` asks the planner to review a diff, and the diff reaches no model that may not see it

`/review` starts an ordinary turn whose prompt is a fixed text written in this program, about one of
five targets: the uncommitted changes (the bare command), `staged`, `since <ref>`, `commit <ref>`,
or `pr <number or address>`. Words after the target, or after the command where it names none, are
carried into the prompt as what the person wants looked at. The four target words are reserved only
as the first word, so `/review the staged changes` reviews the uncommitted changes with that focus.
The prompt is what the transcript shows as sent, and typed during a turn the line waits (CMD-8).

A `<ref>` is accepted only if it cannot be read by `git` as something else: not empty, at most 200
characters, no leading `-`, no `..`, and only letters, digits and `. _ / - ~ ^`. A pull request is
digits or a `https://github.com/<owner>/<repo>/pull/<number>` address. Anything else is a usage note
and no turn starts.

The prompt tells the planner to fetch the diff with `run`, and the turn reads what any turn may
read: `run` returns lines or a reference by the same rules as any output
([TOOL-1](tools/tool-surface.md#TOOL-1)), the prompt adds no reach, and it names no path with `@`, so sending it
vouches for nothing. Where the diff comes back as a reference, the prompt tells the planner to hand
the reference to `spawn_processor` and not to read it another way, so the findings are the
processor's remark, drawn for the person as untrusted content ([PROC-7](processors.md#PROC-7),
[PROC-11](processors.md#PROC-11)) and shown to no
model. The prompt also tells the planner to read a `REVIEW.md` in the working directory with
`read_file`, as the project's account of what to flag or leave alone. That read is an ordinary one
([READ-1](tools/read-file.md#READ-1)): the planner sees the lines where the trust map vouches for
the file, and a reference where it does not, in which case the prompt tells it to review without
the file and not to reach it another way. The prompt reads no file to build itself, and the file
guides the report without lifting any limit the prompt sets. The prompt forbids writes, commits,
pushes and comments; the tool surface is what enforces that, as for every turn, and the prompt
asks only. Nothing the driver does branches on the diff,
the findings or a verdict: no part of the turn reads them.

A `/review` is a command, so a prompt file or skill of that name is shadowed by it (CMD-17).

**Why.** A review exists to read changes the planner may not be allowed to see, and the answer to
that elsewhere is a processor whose output goes to the person. A typed record of findings that the
planner could act on would be the driver branching on processor output, which the design reserves for
the exceptions in [labels.md](labels.md) that read a processor's or a check's answer and which needs the maintainers' agreement
before it is built. This command is the part that needs none of that.

`verified-by: bravebot_tui::app::the_review_command_starts_a_turn_with_the_drivers_own_prompt`
`verified-by: bravebot_tui::app::the_review_command_starts_no_turn_for_a_target_it_cannot_use`
`verified-by: bravebot_tui::app::a_prompt_containing_the_review_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::the_review_command_waits_for_a_running_turn`
`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_tui::review_command::a_bare_command_reviews_the_uncommitted_changes`
`verified-by: bravebot_tui::review_command::each_reserved_word_names_its_target`
`verified-by: bravebot_tui::review_command::words_after_the_target_are_the_focus`
`verified-by: bravebot_tui::review_command::a_reserved_word_only_counts_as_the_first_word`
`verified-by: bravebot_tui::review_command::a_target_word_without_a_usable_name_is_refused`
`verified-by: bravebot_tui::review_command::a_ref_that_git_would_read_as_something_else_is_refused`
`verified-by: bravebot_tui::review_command::the_prompt_names_the_target_and_carries_the_focus`
`verified-by: bravebot_tui::review_command::the_prompt_names_no_path_for_the_person_to_vouch_for`
`verified-by: bravebot_tui::review_command::the_prompt_sends_a_diff_it_may_not_see_to_a_processor`
`verified-by: bravebot_tui::review_command::the_prompt_reads_review_md_through_read_file_and_goes_on_without_it_when_hidden`

## Picking up another session

<a id="CMD-14"></a>
### CMD-14: `/resume` leaves for another recorded session, restored as `--resume` restores it

`/resume` with no argument draws the list `--resume` draws ([SESSION-8](sessions.md#SESSION-8)),
leaving out the session being run, and `/resume <id>` names a record of this directory. Either way
the running session ends and the next begins from the record chosen through the path a start with
`--resume` takes, so its transcript, spend, trust map, programs and rules are the record's own and
nothing the session left granted reaches it: the directories opened and the checkouts kept are
closed, and a loop, a goal and the watches end ([SESSION-31](sessions.md#SESSION-31)). The session
left is written as its last turn left it and stays resumable. Servers started for the process are
not restarted.

Only the id is read from the line, and it is checked for the shape of a session's name before it
reaches a path, so one that climbs out of the sessions directory reads nothing. The line is never a
prompt, and none of it is sent. Escape or Ctrl-C in the list stays in the session. The command is
refused, with a line that does not repeat the id typed, for an id that names no record, for the
session already open, for a manifest run ([SESSION-10](sessions.md#SESSION-10)), and for a record
a running background session holds ([BG-9](background-sessions.md#BG-9)), and it says so where this
directory holds no other session. Typed during a turn it waits for the turn to end (CMD-8).

**Why.** The picker, the restoration and the refusals already exist for the flag, and a second
route into them would restore a different set. Choosing a record reads none of its content to
decide, so the command only says which one the person meant.

`verified-by: bravebot_tui::app::the_resume_command_carries_its_id_and_is_never_a_prompt`
`verified-by: bravebot_tui::app::the_resume_command_waits_for_the_turn_in_flight`
`verified-by: bravebot_tui::sessions::resume_by_id_reads_the_record_named_and_refuses_what_it_cannot_continue`
`verified-by: bravebot_tui::sessions::resume_refuses_a_record_a_running_background_session_holds`
`verified-by: bravebot_tui::sessions::resume_by_id_does_not_follow_an_id_out_of_the_sessions_directory`

<a id="CMD-15"></a>
### CMD-15: `/request` opens the last request built, and takes no argument

`/request` opens a read-only view of the last request a turn built for the planner, which
[TRACE-9](trace.md#TRACE-9) states, including which instruction files were loaded into it and how
many bytes each contributed. It takes no argument, so `/request` followed by words is a
prompt (CMD-2). It reads what the session already holds in memory, so it is carried out as it is
typed while a turn runs (CMD-8), and it starts no turn and sends nothing. It is a command, so only a
line a person typed reaches it (CMD-1).

**Why.** The view answers what the planner was just sent, which is most worth asking while a turn is
running.

`verified-by: bravebot_tui::app::the_request_command_opens_the_view_and_starts_no_turn`
`verified-by: bravebot_tui::app::words_after_the_request_command_make_a_prompt`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`
`verified-by: bravebot_tui::remote_confirm::the_request_a_turn_built_travels_to_the_thread_that_draws_it`

<a id="CMD-16"></a>
### CMD-16: `/recap` asks for a short account of the session, in the way `/btw` asks a question

`/recap` takes no argument, so `/recap` followed by words is a prompt (CMD-2). It sends a copy of the
conversation with a fixed question on the end, written in this program and never read from the line,
and the exchange is untouched afterwards, as for `/btw` ([watching.md](watching.md)). The request
offers no tools and is made through no processor, because the exchange is what the planner was
already shown. The answer is cut to 400 characters, ending in an ellipsis where it was cut, before
it is drawn and before it is offered to the record, so a model that ignores the length it was asked
for cannot exceed it. It is answered and recorded as an aside is: a row in the mode Ctrl-L opens, and
written down only where the exchange had met nothing untrusted ([SESSION-20](sessions.md#SESSION-20)). It is
not a turn. Typed during a turn it waits for the turn to end (CMD-8).

**Why.** A person returning to a long session needs to know where it stands, and reading the
transcript back to find out is the cost the command removes. Sending it as a prompt would put the
question and the account into the conversation the planner reads for the rest of the session.

`verified-by: bravebot_tui::app::the_recap_command_is_a_recap_and_not_a_prompt`
`verified-by: bravebot_tui::app::a_prompt_containing_the_recap_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_agent::turn::a_recap_is_cut_to_its_limit_and_leaves_the_conversation_alone`
`verified-by: bravebot_agent::aside::a_recap_longer_than_the_limit_is_cut_to_it`
`verified-by: bravebot_agent::aside::a_recap_within_the_limit_is_not_cut`
`verified-by: bravebot_agent::aside::the_recap_limit_counts_characters_rather_than_bytes`
`verified-by: bravebot_agent::aside::a_recap_asks_its_own_question_with_a_limit_and_leaves_the_exchange_alone`

<a id="CMD-17"></a>
### CMD-17: a line starting with the name of a prompt file is replaced by the file's text, and sends nothing

On Enter, a line whose first word is `/name` is looked up as `~/.bravebot/prompts/name.md`. Where the
file exists, the line is replaced in the box by the file's text after its front matter, with
`$ARGUMENTS` set to the rest of the line and `$1` to `$9` to its whitespace-separated words (a
missing word is nothing). The substitution is a single pass, so text from the line is never read for
another placeholder. Nothing is sent: the next Enter sends the text as a prompt, once the person has
seen it. Mid-turn the same press expands and does not queue.

A `name` is ASCII letters, digits, `-` and `_`, so it cannot leave the directory or name a hidden
file. A word the command table claims is never a name, so no file shadows a command or changes
`/init the project` from a prompt (CMD-2). A first word with no file is a prompt as typed. Only the
person's own directory is read, never a workspace's. Shell mode does not look anything up, nor does a
row the person moved to in the list.

A file that is not a regular file, not UTF-8, over 64 KiB, empty after its front matter, or whose
text would begin with a command word is not used: the line stays as typed, a note says why, and
nothing is sent. The last refusal keeps CMD-1: no command is carried out from text read out of a
file.

The one command a file can fill is `/agent`. An `agent: name` key of one word makes the box
`/agent name <text>`, which is only a line in the box until the person presses Enter on it, and the
name is then compared against the resolved set as for any other ([addressing-a-definition.md](addressing-a-definition.md)).

**Why.** A prompt used often should be a word, and the file is the person's own, so it is as trusted
as their typing. Putting the text in the box before it is sent keeps what reaches the planner a line
the person saw. Refusing a body that opens with a command, and not looking the name up in a
directory listing, keep CMD-1 and CMD-7 whole.

`verified-by: bravebot_tui::app::a_prompt_file_expands_into_the_box_and_is_sent_by_the_next_enter`
`verified-by: bravebot_tui::app::a_slash_word_with_no_file_is_sent_as_typed`
`verified-by: bravebot_tui::app::a_file_named_for_a_command_does_not_replace_it`
`verified-by: bravebot_tui::app::a_prompt_file_that_cannot_be_used_is_said_and_not_sent`
`verified-by: bravebot_tui::app::a_prompt_file_is_not_expanded_in_shell_mode`
`verified-by: bravebot_tui::app::enter_on_a_highlighted_row_takes_the_row_not_a_file`
`verified-by: bravebot_tui::app::a_prompt_file_expands_while_a_turn_is_running_before_it_is_queued`
`verified-by: bravebot_tui::prompt_files::arguments_and_positions_are_filled_from_what_followed_the_name`
`verified-by: bravebot_tui::prompt_files::text_that_came_from_the_line_is_not_read_for_placeholders`
`verified-by: bravebot_tui::prompt_files::front_matter_is_not_put_in_the_box`
`verified-by: bravebot_tui::prompt_files::an_agent_in_the_front_matter_fills_the_agent_form`
`verified-by: bravebot_tui::prompt_files::an_agent_that_is_not_one_word_is_refused`
`verified-by: bravebot_tui::prompt_files::only_a_plain_name_is_looked_up_and_only_inside_the_directory`
`verified-by: bravebot_tui::prompt_files::a_name_a_command_claims_is_never_a_file`
`verified-by: bravebot_tui::prompt_files::a_body_that_opens_with_a_command_is_refused`
`verified-by: bravebot_tui::prompt_files::a_file_that_cannot_be_used_is_refused_rather_than_ignored`

<a id="CMD-19"></a>
### CMD-19: `/context` divides the measured count of the last request into fixed sections

`/context` takes no argument, so `/context` followed by words is a prompt (CMD-2). It reports the
count the server gave for the last request ([INPUT-22](terminal-input.md#INPUT-22)) shared out over
eight sections written in this program: the system prompt, the instruction files, the skill list,
the tool definitions, what the person typed, what the planner wrote, tool results, and other
messages. A section is chosen from the role and the origin the driver recorded for a span, and its
size is the length in bytes of what was sent, scaled to the measured count with the remainders
given out so the rows add up to it. No name is read from a file, a result or a skill, no row is
chosen by what a span says, and a section with nothing in it has no row. It reads what the session
holds and sends nothing, so it is carried out as it is typed while a turn runs (CMD-8).

It breaks down the last request only. Where nothing has been measured, or there is no budget to
divide by, it says so as the context reading does; after a compaction it says that the conversation was
compacted, since the request on hand is of the conversation that was shortened; and where a count
is known but this session has built no request, it says that. None of these is drawn as a row of
zeros or as nothing. The figures are marked approximate, because bytes are not tokens.

**Why.** One percentage cannot tell a person whether tool definitions, instructions or a few large
results fill the window, and so whether to compact, clear or trim what is loaded. Every input here is
a length the driver already holds, so nothing branches on untrusted content.

`verified-by: bravebot_tui::app::the_context_command_divides_the_measured_total_by_section`
`verified-by: bravebot_tui::app::the_context_command_says_when_nothing_has_been_measured`
`verified-by: bravebot_tui::app::the_context_command_does_not_describe_a_conversation_it_has_no_request_for`
`verified-by: bravebot_tui::app::a_prompt_containing_the_context_command_is_still_a_prompt`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`
`verified-by: bravebot_tui::state::a_count_is_apportioned_in_whole_numbers_that_add_up_to_it`
`verified-by: bravebot_agent::request_view::the_size_of_the_tool_definitions_is_what_was_sent`

<a id="CMD-20"></a>
### CMD-20: a person who has been away is given a recap, once, without asking

When the terminal has reported being left, at least five minutes have passed since the last turn
completed (or the number of seconds an `awaySummaryIdleSeconds` setting names), and the session has
had at least three turns, the interface does what `/recap` does (CMD-16), once, with nothing queued
behind it. It does not do it again until another turn has completed, and a recap the person asked
for counts as one. A terminal that reports no focus changes is taken to be in front of the person,
so it never recaps by itself. An `awaySummaryEnabled` setting of `false` turns it off, and any other
value, or none, leaves it on. `awaySummaryIdleSeconds` is a whole positive count: zero, or anything
that is not a whole count, leaves the five minutes in force. It is the terminal interface's alone: a
`-p` run and the desktop app have no such view and never make one.

Nothing from the conversation reaches the decision. It reads the clock, the focus reports, a count of
turns and the settings, all of which the interface holds itself.

**Why.** CMD-16 only helps a person who thinks to ask. A person returning to a long session is the one
least likely to remember what it was doing, and the first thing they would do is ask.

`verified-by: bravebot_tui::away::a_recap_is_due_once_every_condition_holds`
`verified-by: bravebot_tui::away::a_recap_waits_for_the_idle_time`
`verified-by: bravebot_tui::away::a_recap_waits_for_the_idle_time_the_setting_names`
`verified-by: bravebot_tui::away::a_recap_waits_for_the_terminal_to_be_left`
`verified-by: bravebot_tui::away::a_recap_waits_for_enough_turns`
`verified-by: bravebot_tui::away::a_recap_is_not_made_twice_in_a_row`
`verified-by: bravebot_tui::away::a_recap_is_not_made_where_the_setting_turned_it_off`
`verified-by: bravebot_tui::away::a_session_with_no_completed_turn_owes_nothing`
`verified-by: bravebot_tui::app::leaving_the_terminal_after_a_completed_turn_makes_a_recap_due`
`verified-by: bravebot_config::settings::only_a_boolean_turns_the_away_recap_off`
`verified-by: bravebot_config::settings::the_away_recap_switch_is_among_the_names_reported`
`verified-by: bravebot_config::settings::a_settings_file_names_how_long_before_the_away_recap`

<a id="CMD-21"></a>
### CMD-21: `/effort` spells out its levels after the word, and Tab finishes one

Once `/effort ` is typed, the box draws the five levels it takes, `low`, `medium`, `high`, `xhigh`
and `max`, after the caret in place of `[level]`. The completion list keeps `[level]` in the table,
because every row's description starts after the longest word in the list and a spelled-out hint
would push all of them to the right. `--help` prints the spelled-out form. Both are built from the
set of levels the program accepts, so a level added there appears in both.

Tab after `/effort ` and the start of a level, with the caret at the end of the line, replaces the
start with the level it fits, so `/effort h` becomes `/effort high`. A start that fits several
levels is carried as far as they agree and no further, so `m`, which fits `medium` and `max`, is left
as typed. A finished level, a word that fits none, a bare `/effort `, another command, a prompt that
only mentions `/effort`, and a line in shell mode are left as typed. Tab never submits the line.

Nothing from the conversation reaches either. They read the typed line and the fixed set of levels.

**Why.** The words a command takes are what a person cannot guess, and `[level]` did not say them.
Completing a start from the same set the command accepts keeps the hint, the completion and the
refusal for an unknown word in step.

`verified-by: bravebot_tui::render::the_effort_command_offers_its_levels_after_its_word`
`verified-by: bravebot_tui::app::the_effort_hint_lists_every_level_and_the_list_stays_short`
`verified-by: bravebot_tui::render::a_slash_offers_every_command_and_what_it_does`
`verified-by: bravebot_tui::app::tab_finishes_a_level_started_after_the_effort_command`
`verified-by: bravebot_tui::app::tab_leaves_alone_a_start_it_cannot_finish`
`verified-by: bravebot_tui::app::tab_leaves_a_shell_line_alone`
`verified-by: bravebot_tui::app::tab_leaves_a_line_alone_while_the_caret_is_not_at_its_end`

<a id="CMD-22"></a>
### CMD-22: `/sandbox`, `/limit`, `/review` and `/reach` spell out what they take after the word

Once `/sandbox ` is typed, the box draws the four modes it takes, `strict`, `read-only`,
`workspace-write` and `off`, after the caret in place of `[<mode>]`. `/sandbox standard` is read
as `workspace-write` and is not listed. Tab after `/sandbox ` and the start of a mode, with the caret at
the end of the line, finishes it by the rule CMD-21 gives for `/effort`, so `/sandbox w` becomes
`/sandbox workspace-write`.

`/limit` draws `[<n>[k|m] | <n> credits | off]`, the figures it takes, and not the words `tokens`
and `credits` as choices. `/review` draws `[staged | since <ref> | commit <ref> | pr <n>] [focus]`,
the targets its parser reads. `/reach` draws the credential scopes, the `write` and `always`
words, `remove <n>` and `keys`. In each, the words are the ones the parser compares against, taken
from the type or constant it reads, so a mode, scope or target added there appears in the hint.
The completion list and the table keep the short form, so the description column does not move,
and `--help` prints the spelled-out form. `/style` and `/theme` list nothing after the word,
because a name there may come from a file a person wrote.

Nothing from the conversation reaches any of them. They read the typed line and fixed sets.

**Why.** A person cannot guess these words from `[<mode>]` or `[target]`, and a hint written
apart from the parser drifts from it.

`verified-by: bravebot_tui::render::the_sandbox_command_offers_its_modes_after_its_word`
`verified-by: bravebot_tui::app::the_sandbox_hint_lists_every_mode_and_the_list_stays_short`
`verified-by: bravebot_tui::app::the_limit_review_and_reach_hints_show_forms_their_parsers_take`
`verified-by: bravebot_tui::app::tab_finishes_a_mode_started_after_the_sandbox_command`

<a id="CMD-23"></a>
### CMD-23: `/commit-message` puts a draft in the box, and the diff and the draft reach no planner

`/commit-message [--staged | --range <a>..<b>] [format]` reads the changes named and has a model
write one commit message for them. No argument and `--staged` are what is in the index and not in
`HEAD`, and `--range` is the changes between two commits, written `a..b` or `a...b`
([GIT-15](tools/read-git.md#GIT-15)). Each name is a branch, tag, hash or `HEAD~2`, with no leading
`-` and no `..` inside it. The two flags count only as the first word, so `/commit-message use
Conventional Commits` is the staged changes in that format. Words after the source are the person's
own and go to the processor as the format they want. A range that is not two such names is a usage
note and nothing is read or sent.

The driver reads the diff with the reader `read_git` uses, under a policy of its own that holds
`FileRead` and the network and no conversation, and hands it to a processor
([PROC-14](processors.md#PROC-14)). It is not a turn: nothing joins the transcript, the
conversation is neither read nor written, and the request carries no tool.

The draft goes to one place, the input box, as text the person reads, edits and sends. What is put
in the box is a fixed line, a blank line and the draft, so the line the person sends starts with a
word the driver wrote and a draft that spells `/clear` is a prompt like any other sentence
([CMD-1](#CMD-1)). Characters a terminal draws as nothing are taken out of the draft first, and the
number is said ([PASTE-10](pasting.md#PASTE-10)). Whatever was in the box is kept as the draft that
Up brings back. Nothing runs on the draft: the next Enter sends it as an ordinary prompt, so the
planner reads it only as words the person chose to send, and a commit follows only from the planner
being asked for one, through `run`, with the person asked about it as for any program.

Nothing is drafted, and the line says which of these held, where the repository is not one
`read_git` opens ([GIT-2](tools/read-git.md#GIT-2)), where the reader shows no file, where the diff
holds what looks like a credential ([CRED-15](credential-protection.md#CRED-15)), where the model
could not be used ([PROC-13](processors.md#PROC-13)) or where the answer marks no message. A diff cut
at the reader's cap is drafted from what was read and the line says so. Typed during a turn the
line waits for the turn to end (CMD-8).

**Why.** A message is written from a diff the planner may not be allowed to read, and the answer to
that elsewhere is a processor. The draft is derived from that diff, so its integrity is the diff's,
and the one place it is released is a surface the person edits and a key the person presses. A
credential in a diff is refused where the planner's own read would ask, because nobody is at a
prompt to agree.

`verified-by: bravebot_agent::commit_message::a_range_is_two_written_out_revisions`
`verified-by: bravebot_tui::app::the_commit_message_command_carries_its_source_and_is_not_sent_as_a_prompt`
`verified-by: bravebot_tui::app::the_commit_message_command_starts_no_request_for_a_range_it_cannot_use`
`verified-by: bravebot_tui::app::a_prompt_containing_the_commit_message_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::the_commit_message_command_waits_for_a_running_turn`
`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_tui::app::a_draft_that_spells_a_command_is_sent_as_a_prompt_and_not_carried_out`
`verified-by: bravebot_tui::app::a_draft_has_the_characters_a_terminal_draws_as_nothing_taken_out`
`verified-by: bravebot_tui::app::a_draft_arriving_over_typed_text_keeps_the_text_as_a_draft`
`verified-by: bravebot_tui::app::a_stopped_or_failed_draft_leaves_the_box_alone`
`verified-by: bravebot_tui::commit_message_command::a_bare_command_drafts_from_the_staged_changes`
`verified-by: bravebot_tui::commit_message_command::a_range_names_its_source_and_the_rest_is_the_format`
`verified-by: bravebot_tui::commit_message_command::a_flag_only_counts_as_the_first_word`
`verified-by: bravebot_tui::commit_message_command::a_range_that_is_not_two_plain_revisions_is_refused`
`verified-by: bravebot_tui::commit_message_command::the_box_begins_with_the_leading_words_whatever_the_draft_says`
`verified-by: bravebot_agent::commit_message::a_range_is_drafted_from_its_diff_and_the_request_holds_no_tool`
`verified-by: bravebot_agent::commit_message::an_index_that_matches_head_sends_nothing`
`verified-by: bravebot_agent::commit_message::a_repository_the_map_does_not_trust_is_not_read_or_sent`
`verified-by: bravebot_agent::commit_message::a_diff_holding_a_credential_is_not_sent`
`verified-by: bravebot_agent::commit_message::an_answer_with_no_marker_is_no_draft`

## Answering from what this program carries

<a id="CMD-24"></a>
### CMD-24: `/help` asks the planner to answer a question from the documentation this program carries

`/help <question>` starts an ordinary turn whose prompt is a fixed text written in this program
around the question: load the built-in skill `bravebot-help` with `load_skill` and answer from the
pages it lists, not from memory ([SKILL-17](skills.md#SKILL-17)). The question is the argument as
CMD-5 takes it and is carried into the prompt as typed. The prompt is what the transcript shows as
sent, and typed during a turn the line waits (CMD-8).

With no question the command says what it takes and no turn starts. `/help` followed by words is
the command, a longer word that starts with it is a prompt, and so is a sentence that mentions it
(CMD-2).

The fixed words of the prompt name no path with `@`, so sending them vouches for nothing. The
question is the person's own line, so an `@` in it is read as it is in any line they type
([NAME-1](naming-files.md#NAME-1)). The turn is marked as one the driver wrote the prompt of, so it is
not offered a later look at its own line ([SCHED-6](tools/schedule-next.md#SCHED-6)). The pages are
this program's own text and are trusted for that reason ([SKILL-12](skills.md#SKILL-12)). The turn
reads what any turn may read and the prompt adds no reach. Nothing the driver does branches on the
question or on the answer. A skill named `bravebot-help` that a turn would offer from another source
shadows the built-in ([SKILL-12](skills.md#SKILL-12)), and the turn then loads that one. A prompt file
or skill named `help` is shadowed by the command (CMD-17).

**Why.** The planner loads the skill on its own when the description fits the question
([SKILL-13](skills.md#SKILL-13)), and a person's wording may not fit it. The command names the
skill in the prompt, so the question is answered from the documentation of the running version
whatever the wording. The question goes into the driver's sentence and not the other way round, so
the typed line itself is never sent (CMD-4).

`verified-by: bravebot_tui::app::the_help_command_starts_a_turn_with_the_drivers_own_prompt_around_the_question`
`verified-by: bravebot_tui::app::the_help_command_with_no_question_says_what_it_takes_and_starts_no_turn`
`verified-by: bravebot_tui::app::a_help_question_typed_while_a_turn_runs_waits_and_then_starts_its_own_turn`
`verified-by: bravebot_tui::app::a_prompt_containing_the_help_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::a_help_turn_is_the_drivers_prompt_and_is_offered_no_later_look`
`verified-by: bravebot_tui::app::only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue`
`verified-by: bravebot_tui::app::every_command_in_the_table_dispatches`
`verified-by: bravebot_tui::help_command::the_prompt_names_the_help_skill_and_carries_the_question_as_typed`
`verified-by: bravebot_tui::help_command::the_prompt_names_no_path_for_the_person_to_vouch_for`
`verified-by: by-construction (the command returns Action::Submit like any typed line, and the one place a submitted line is run reads it as the person's; nothing in the command marks the question as anything else)`

<a id="CMD-25"></a>
### CMD-25: `/changelog` shows the release notes this program carries, and starts no turn

`/changelog` puts the notes of the version that is running in the transcript, from the changelog
compiled into this program. Nothing is read, fetched or sent, and no turn starts. The notes are the
lines under that version's heading, each markdown link drawn as its text, under a heading that names
the version.

Where the changelog has no heading for the running version, the notes shown are the newest
release's and the heading says whose they are and which version runs, so they are not taken for the
running version's. A version matches only as a whole, so `0.1` is not the release `0.17.0`. A program
that carries no release says so.

The command takes no argument, so `/changelog` followed by words is a prompt (CMD-2). It touches
nothing a running turn holds, so typed during one it is carried out as it is typed (CMD-8). A prompt
file or skill named `changelog` is shadowed by the command (CMD-17).

**Why.** The release notes are written for the people who run a version, and the version that
runs is what a person asking has in mind. The text is carried in the program, so the notes match
the build with no network and no checkout, and a model is not needed to read out a file.

`verified-by: bravebot_tui::app::the_changelog_command_shows_notes_and_starts_no_turn`
`verified-by: bravebot_tui::app::a_prompt_containing_the_changelog_command_or_a_longer_word_is_still_a_prompt`
`verified-by: bravebot_tui::app::the_changelog_command_is_carried_out_while_a_turn_runs`
`verified-by: bravebot_tui::state::the_changelog_is_headed_by_the_running_version_and_has_its_links_written_as_text`
`verified-by: bravebot_tui::state::the_changelog_of_a_build_with_no_notes_of_its_own_says_whose_notes_it_shows`
`verified-by: bravebot_tui::state::a_changelog_with_no_release_says_so`
`verified-by: bravebot_tui::help_command::a_link_is_drawn_as_its_text_and_the_rest_of_the_line_is_untouched`
`verified-by: bravebot_tui::help_command::brackets_that_are_not_a_link_are_kept`
`verified-by: bravebot_tui::help_command::an_address_that_is_not_closed_takes_nothing_after_it`
`verified-by: bravebot_tui::help_command::a_backtick_left_open_does_not_hide_the_links_on_the_next_line`
`verified-by: bravebot_tui::help_command::an_address_with_parentheses_of_its_own_is_one_address`
`verified-by: bravebot_tui::help_command::an_image_and_a_link_in_a_code_span_are_kept_and_a_link_after_them_is_not`
`verified-by: bravebot_tui::help_command::a_release_is_the_lines_under_its_heading_up_to_the_next_heading`
`verified-by: bravebot_tui::help_command::only_a_line_that_begins_with_the_heading_mark_starts_a_release`
`verified-by: bravebot_tui::help_command::a_release_is_found_by_its_whole_version_and_else_the_newest_is_given`
`verified-by: bravebot_tui::help_command::the_carried_changelog_lists_releases_newest_first_and_the_newest_has_notes`

## Running a prompt of an MCP server

<a id="CMD-26"></a>
### CMD-26: a line that starts with the word of an MCP server's prompt runs it, and is sent as the line it is

The prompts of the servers a session reached are rows of the list a slash opens, among the skills
(CMD-9) and in name order with them. A row is `/mcp__<alias>__<word>`, says which server it came
from and what the server said about the prompt, and once the word is typed in full the box draws
what it takes after the caret, each required argument as `name=<value>` and each optional one in
brackets. A row is offered, and the word drawn as recognised (CMD-10), only where it begins the
line, since that is the one place a line runs it. Taking a row writes the word and each argument
the prompt cannot run without as a space and `name=`, so the person gives values and retypes no
name the row drew, and nothing else: an optional argument is left to be added, and a prompt that
needs none gets the word and a space. No row is offered while work runs, in shell mode, or for a
server turned off.

A line whose first word is `/mcp__<alias>__<word>`, where the alias is one of a server this session
reached and the word is one a tool may be named, runs that prompt. What follows the word is its
arguments: `name=value` pairs separated by spaces, with a value that holds spaces in double
quotes, where `\"` and `\\` stand for a quote and a backslash. A word that is not a pair, an
argument given twice, a name an argument may not have and a quote that does not close are said, and
the line stays in the box and is not sent, at rest and mid-turn alike. A word that two reached
servers' aliases both begin names neither, since either could be the one meant and the arguments go
to one of them: it is said and the line is not sent. A line that begins this way and names no
server this session reached, or whose word is not one a tool may be named, is a prompt like any
other sentence (CMD-2).

The line is sent as the prompt it is, and the turn it starts fetches the prompt, which
[mcp.md](mcp.md) says how to label. Nothing in the line is compared with what a server listed: the alias is
compared with the servers the person declared, and the word and the arguments are the person's own
and are the whole of the request. A name a taken row wrote is a word of the line like any other,
so a line reads the same whether it was typed or taken. Only a line the person wrote runs a prompt. A line this program
composed to carry a goal on does not, and a prompt file whose text would run a prompt of a server
this session reached is refused as one opening with a command is (CMD-17). A line that reaches a
turn without parsing, as a `/loop`'s prompt can, is sent as the sentence it is and the person is
told the prompt was not fetched.

**Why.** A server's words are its own, so the rows follow the rule skills do (CMD-7): the word is
one a tool may be named, so it cannot read as a sentence, and the row says where it came from. The
line is sent rather than dispatched because the keystroke is the person's endorsement of one fetch
and nothing else. It does not vouch for what the server says, which reaches the planner as a
reference and never as text. A prompt that was a command would have a path that carries a server's
word into a dispatch that decides something.

`verified-by: bravebot_tui::app::a_servers_prompt_is_offered_at_the_start_of_a_line`
`verified-by: bravebot_tui::app::a_servers_prompt_is_not_offered_after_other_words`
`verified-by: bravebot_tui::app::taking_a_prompt_row_writes_its_word_and_required_argument_names_and_sends_nothing`
`verified-by: bravebot_tui::skills::taking_a_row_writes_the_names_of_the_arguments_a_prompt_cannot_run_without`
`verified-by: bravebot_tui::app::a_line_running_a_prompt_with_words_that_are_not_arguments_is_said_and_not_sent`
`verified-by: bravebot_tui::app::a_line_running_a_prompt_with_words_that_are_not_arguments_is_not_queued`
`verified-by: bravebot_tui::app::a_line_the_person_wrote_carries_its_prompt_to_the_turn`
`verified-by: bravebot_tui::render::a_servers_prompt_row_names_its_server_and_what_it_takes`
`verified-by: bravebot_tui::render::a_servers_prompt_is_drawn_as_recognised_only_from_the_first_word`
`verified-by: bravebot_tui::app::a_line_naming_two_servers_or_no_argument_name_is_said_and_not_sent`
`verified-by: bravebot_tui::app::a_prompt_file_whose_text_would_run_a_servers_prompt_is_refused`
`verified-by: bravebot_agent::mcp::a_typed_line_names_a_prompt_only_of_a_server_this_session_reached`
`verified-by: bravebot_agent::mcp::arguments_are_pairs_and_a_quoted_value_may_hold_spaces`
`verified-by: bravebot_agent::mcp::a_quoted_value_unescapes_a_quote_and_a_backslash_and_nothing_else`
`verified-by: bravebot_agent::mcp::words_that_are_not_arguments_are_refused_naming_the_word`

<a id="CMD-27"></a>
### CMD-27: `/mode <skill>` keeps a skill applied to every turn, and `/mode off` leaves it

`/mode` alone says which skill is the session's mode, or that there is none, and lists the names it
could take. `/mode <skill>` takes one by its whole name, among the skills a turn starting now would
resolve, read the way the box offers them ([CMD-9](#CMD-9)): through the trust map and the rules, so
a skill from a project nobody vouched for is not named, offered or taken. A name that matches none,
a prefix, another case and a name with a space after it, keeps the mode that was there and says so.
`/mode off` leaves the mode. The name is stored in the session's record ([SKILL-21](skills.md#SKILL-21))
and shown in the border of the box while the mode stands, so a person who has forgotten it is told by
the place they type in.

The command's word is written in this program like every other ([CMD-7](#CMD-7)): no skill becomes a
command, and a skill called `mode` is not offered as a row. A command typed in full is offered first, so Enter on
`/mode` runs it although `/model` shares its letters. Only a line a person typed sets or clears
a mode. A mid-turn `/mode` waits for the turn to end, which is the kind CMD-8 gives every command it
does not name: the turn in flight was built without the change.

**Why.** The same words the skill rows carry are the ones a person is choosing between, so a
mode needs no second list. Taking only a name the driver enumerated keeps the mode a choice among
sources already trusted for skills, and not a way to name a file.

`verified-by: bravebot_tui::app::the_mode_command_puts_a_skill_in_force_by_its_whole_name_and_off_leaves_it`
`verified-by: bravebot_tui::app::the_mode_command_takes_a_name_or_nothing`
`verified-by: bravebot_tui::app::enter_on_mode_typed_in_full_runs_mode_and_not_model`
`verified-by: bravebot_tui::command_table::every_command_that_can_wait_is_placed_against_what_a_waiting_line_could_change`
`verified-by: bravebot_tui::render::a_slash_offers_every_command_and_what_it_does`

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
- **A lone `exit` or `quit` typed while a turn runs is queued as a prompt, and the note under the
  box is drawn only when a turn begins on it (CMD-2).** Where Enter queues the line behind a running
  turn, nothing starts on it yet, the line is drawn above the box as waiting, and the person can stop
  the turn with Ctrl-C. A line the running turn takes in as an interjection begins no turn of its own,
  so it has no note.
- **The row for a sent `exit` or `quit` is 47 columns, and a terminal narrower than that cuts it at
  the last column, which loses the way out (CMD-2).** It is the row's right-hand note, and the parts
  of the line are fitted against the width it leaves. On a row eighty columns wide that is 31
  columns, which holds the longest permission mode name alone and a shorter one beside a vi mode.
  Where the parts do not fit, the whole left side is cleared while the note is up.
- **The row for a sent `exit` or `quit` is up only while the turn runs (CMD-2).** A turn that
  answers at once takes the note down with it, and a person who looked away has nothing under the
  box to say the word was a prompt. The transcript still shows the line and the reply.
- **`/changelog` shows one release, and a long release scrolls its heading off the screen
  (CMD-25).** The release is the running version's, or the newest where this build has none of its
  own. Between releases the top section holds the notes of changes not yet released, headed with the
  last release's version, so a build made from the commits after a release shows them under that
  version's number. The notes are rows of the transcript, which shows its end, so a
  release with more rows than the terminal has leaves the heading above the rows drawn and a person
  scrolls up to it. The program
  carries every earlier release's notes and no command shows one, so a person upgrading across several
  versions reads the earlier ones on the releases page.
- **A server's prompts are the ones it offered when it started (CMD-26).** A prompt it adds later
  is not a row until the server is started again, since the client does not read the server's
  notice that its lists changed, which #1561 tracks. A server that moved after a redirect keeps the
  list it started with, so a row can name a prompt the server at its new address does not have, and
  running it reports that.
- **Where two aliases both begin a prompt's word, no spelling names either server's (CMD-26).**
  With `a` and `a__b` both reached, `/mcp__a__b__c` is the prompt `c` of `a__b` or the prompt
  `b__c` of `a`, and the line is refused rather than sent to one of them. Aliases a person declares
  are rarely nested this way.
- **A prompt takes named arguments only (CMD-26).** `/mcp__weather__forecast Paris` is refused
  rather than read as the first argument, because reading a value as an argument's means taking
  that argument's name from the server's listing when the line is read, and nothing here decides
  from what a server listed. Taking the row writes each required name as `name=`, so only values
  are typed, and a line typed out by hand names its arguments. Once a name is written the line
  holds more than the word, so the box draws no hint for the optional arguments; the row showed
  them.
