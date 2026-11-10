---
id: BOT
title: Bots
status: proposed
governs:
  - crates/agent/src/agents.rs
  - crates/agent/src/bots.rs
  - crates/agent/src/bots/operations.rs
  - crates/agent/src/memory.rs
  - crates/cli/src/flags.rs
  - crates/cli/src/main.rs
  - crates/tui/src/app.rs
  - crates/tui/src/app/bots_screen.rs
  - crates/tui/src/app/phase.rs
  - crates/tui/src/bots_pane.rs
  - crates/tui/src/layout.rs
  - crates/tui/src/state/bots.rs
  - crates/ui-files/src/main.rs
  - crates/ui-bridge/src/bots.rs
  - crates/ui-bridge/src/bridge.rs
  - ui/src/main/bot-store.ts
  - ui/src/main/bots.ts
  - ui/src/main/index.ts
  - ui/src/main/state.ts
  - ui/src/shared/bots.ts
documented-by:
  - none (gap: a page on making a bot and continuing its conversation in the terminal or the GUI, owed until the design is built)
---

## Scope

A **bot** is a definition a person made and a row kept for it in `~/.bravebot/bots.json`. The definition
says what the bot is for and the row says what happened to it ([MEMORY-1](definition-memory.md#MEMORY-1),
which splits a bot between the definition file and the store of the graphical user interface
(GUI), where the store is the GUI's `bots` key until the design is built). Every turn of a bot's
conversation is addressed to its definition ([MEMORY-10](definition-memory.md#MEMORY-10), which
says each turn names the bot's definition and runs under it).

This file says how the terminal gets the same bots the GUI has: one file of rows that both read
and write, one module that holds the code for them, and a second layout of the terminal that
presents them. It does not change what a bot is. [definition-memory.md](definition-memory.md) is
what a bot is, how its memory is kept and what the GUI does with it today, and this file takes
that as given.

Built so far are the module that reads and writes the rows and the bridge methods the GUI calls
it through (slice 1), the layout seam and the phase with no session (slice 2a), the bots layout
that lists the bots, opened by `--layout bots` and `/layout bots` (slice 2b), and the module's make,
edit, set the model and home folder operations with the bridge methods the GUI calls them through
(slice 3a), and archive, restore and delete with the bridge methods the GUI calls them through
(slice 3b), and the operations that record a conversation, an archive length, a released session
and a forgotten conversation, with the bridge methods the GUI calls them through (slice 3c). The
terminal does not record a turn, does not continue a conversation from the layout, and has no
delete of a conversation. Still not built are a session as a bot, the forms and the memory. The reasoning and the trust analysis are in
[../design/bots-layer.md](../design/bots-layer.md). Where each clause is built, and in what order,
is in [../PLAN.md](../PLAN.md).

The GUI's code for the rows is in TypeScript. [SHARE-001](../best-practices/shared-implementation.md#SHARE-001)
(when a second client needs behavior that one client holds, both call one Rust implementation)
applies, so the code that reads, validates and writes the rows moves to Rust and both clients
call it. The functionality and the design of the rows are the GUI's today and are kept. Only
where the code is written changes.

## The rows

<a id="BOT-1"></a>
### BOT-1: a bot's row has the fields the GUI's row has, and keeps them

A row has:

| Field | Meaning |
|---|---|
| slug | the name the bot's files are known by, fixed when the row is made |
| name | what the bot is called, free text, which may change |
| purpose | what it is for, free text, which may change. The row holds the text the form shows, and the definition's body is what reaches a run ([MEMORY-8](definition-memory.md#MEMORY-8), which writes the definition) |
| avatar | a stored seed the picture is drawn from, which does not change when the name does |
| model | the model the bot requests, or none for the configured default |
| home | the folder its conversations with no project run in, made under the GUI's data folder ([BOT-4](#BOT-4)) |
| definition | the name of the definition written for it, or none for a bot made before definitions |
| session | the identifier its next turn continues, or none until it has spoken |
| conversations | each conversation it has had, with the folder it ran in |
| archived | the length of the conversation's compaction archive when a consolidation was last decided ([BOT-16](#BOT-16)) |
| retired | when the bot was archived, or zero for a bot in use |
| created, updated | when it was made and when anything about it last changed |

A row is read and written by one module ([BOT-5](#BOT-5)) and holds no field beyond these. A row
written before homes existed also has a `directory` field, and a row written by the GUI
before this design also has `remembered` and `quiet` fields. `directory` is read as [BOT-3](#BOT-3)
says. `remembered` and `quiet` are ignored when the rows are read and are not written back.

An archived bot keeps its slug, its avatar, its definition, its session and its conversations.
Bringing it back resumes its session. Archived bots count toward the ceiling in
[BOT-3](#BOT-3).

`verified-by: bravebot_agent::bots::a_written_row_reads_back_without_the_fields_the_row_no_longer_has`
`verified-by: bravebot_agent::bots::a_bots_file_lists_each_bot_the_way_the_gui_read_it`

<a id="BOT-2"></a>
### BOT-2: a slug is lowercase letters and digits in runs joined by single hyphens

A slug is 1 to 64 characters and matches `^[a-z0-9]+(?:-[a-z0-9]+)*$`. It is composed from the name
once, when the row is made: the name is lowercased, every run of other characters becomes one
hyphen, leading hyphens are dropped, the result is cut to 48 characters, and trailing hyphens are
dropped from the cut. A name with nothing left becomes `bot`. The cut comes before the last trim, so
a name whose 48th character falls on a word break does not leave a slug ending in a hyphen, which
the agent refuses as a definition name. A slug that another row already holds, archived or not, takes `-2`,
then `-3`, and so on. A stored row whose slug fails the pattern, or repeats the slug of an earlier
row, is dropped when the rows are read, and the other rows are kept.

The definition's name is chosen by the agent when it writes the definition
([MEMORY-8](definition-memory.md#MEMORY-8), which takes the next free name when one is taken), so it
may differ from the slug. The row keeps the name the agent answered with, and only a name that is a
slug ([MEMORY-3](definition-memory.md#MEMORY-3), which limits a memory-keeping definition's name to
that class).

**Why this narrow.** A slug is a path segment in the home folder and the name of the row. With no
dot, `..` cannot be written, and lowercase keeps two names from being one file on a volume that
folds case.

`verified-by: bravebot_agent::bots::a_slug_is_runs_of_lowercase_letters_and_digits_joined_by_single_hyphens`
`verified-by: bravebot_agent::bots::every_entry_the_rules_drop_is_reported_with_its_reason`
`verified-by: bravebot_agent::bots::operations::a_slug_is_composed_from_the_name_as_the_spec_says`
`verified-by: bravebot_agent::bots::operations::a_slug_another_row_holds_takes_a_number`
`verified-by: bravebot_agent::bots::operations::a_composed_slug_is_always_a_slug`
`verified-by: bravebot_agent::bots::operations::a_taken_name_is_numbered_and_each_bot_has_a_definition_of_its_own`
`verified-by: by-construction (the GUI's slug composition, slugFor, is deleted, so ui/scripts/bot-operations-bridge.test.mjs makes two bots with one name through the real bravebot-rpc and asserts that the slugs are custodian and custodian-2 and that each has a definition of its own, and ui/scripts/bot-store.test.mjs asserts that ui/src/main/bots.ts and ui/src/main/index.ts hold no slugFor; make check-ui runs both)`

<a id="BOT-3"></a>
### BOT-3: a name, a purpose, a model and an avatar are bounded text, and 100 bots is the ceiling

| Field | Bound |
|---|---|
| name, purpose | not blank, no null character (byte value zero) |
| model | absent, or one line that is not blank, at most 512 characters, no null character, no leading or trailing space, and not `inherit` |
| avatar | not blank when given, at most 128 characters. A stored avatar that is blank is replaced with the slug and the bot is never dropped for it |

A name and a purpose have no length bound, as the GUI has none, and the terminal cuts a name to
the width of the pane only when it draws it. A model is an identifier a backend is asked for, never a
path or a command. A form that breaks a bound is refused and the text is not cut. The GUI accepts a model with a line
break, edge spaces or the word `inherit`, and `make_definition` refuses each ([MEMORY-8](definition-memory.md#MEMORY-8)),
so every redefine of such a row fails. The module refuses them in the form and reads a stored one as none. The GUI's form is refused the same
way once it calls the module, where it accepts a null character today and drops the row on the next read.
At most 100 rows are kept, archived ones included, and a
bot made at the ceiling is refused. The GUI's save drops the slug that sorts last at the ceiling
and never refuses, so it changes the same way once it calls the module.

**Reading the rows.** The module applies the rules of the GUI's `parseBots`
(`ui/src/shared/bots.ts`) to each entry of the `bots` array of the file, in file order. An entry is dropped, and
the other entries are kept, when:

- it is not an object, its slug fails [BOT-2](#BOT-2) or repeats the slug of an earlier entry, or its
  name or purpose is blank or holds a null character;
- its `session` is present and is not an identifier (non-empty, no `/`, no null character), where
  `null` and absent both mean none;
- its `created` or `updated` is present and is not a finite number.

An entry is kept with a field repaired, and not dropped, when:

- `home` is missing or is not an absolute path. The row takes `<data folder>/bot-homes/<slug>`
  ([BOT-4](#BOT-4)).
- `avatar` is blank. It becomes the slug.
- `model` is not valid under the table above. It becomes none.
- `definition` is not a slug. It becomes none.
- `archived` is not a non-negative whole number, or `retired` is not a non-negative finite number.
  The field becomes zero.
- `created` is absent. It becomes the time of the read, and an absent `updated` becomes `created`.
- A `conversations` entry is a bare identifier, which a row written before pairs existed holds. It is
  paired with the row's `directory` when that is an absolute path, otherwise with its home. An entry
  that is neither that nor a pair of an identifier and an absolute path is dropped, and so is a
  second entry with the same folder and identifier. A `session` that no entry names is added as an
  entry, paired the same way.

Reading stops after the 100th row kept. The rows are returned in file order and are not sorted. The
read also returns what it dropped, which is each entry dropped by the rules above with its slug when
it has one and the reason, and the entries after the 100th row kept with their slugs. The layout draws
that as a line naming the file, and the bridge returns it to the GUI beside the rows, in the
channel that reports a file that cannot be parsed. The next write still removes those entries, as the
GUI's does today, so a hand-edited mistake does not stop a person using their other bots. A
write places the rows in the byte order of the slug and keeps at most 100. A slug holds only
lowercase letters, digits and hyphens, and for those the byte order is the order `localeCompare`
gives in English, German, French, Swedish, Turkish, Spanish, Polish, Finnish, Japanese and Chinese.
It differs in a locale that sorts a pair of letters as one letter, such as Czech, Slovak, Danish,
Norwegian, Hungarian, Lithuanian, Estonian, Welsh and traditional Spanish. The GUI keeps sorting the
list it draws with `localeCompare`, as it does today, so its list does not change, and the module
needs no locale and no collation library. The layout lists the bots in use in the order the rows are
returned and the archived ones most recently archived first. In a locale of the second kind the
terminal's order can differ from the GUI's for a slug that holds such a pair. A test pins the byte
order, and compares it with `localeCompare`'s for slugs with digits and hyphens in the English locale.

**An absolute path** is a string with no null character that starts with `/`, or is a drive path
(`C:\` or `C:/`) or a share path (`\\server\share`, with a server name that does not start with `?` or
`.`) that has no colon after its first two characters. Both spellings are accepted on every system,
as the GUI accepts them, so a row written on one system reads the same on another. The module
does not use the standard library's test for the current system. A folder that fails the test is
never used for a turn ([BOT-9](#BOT-9)).

**Why the archive counts.** An archive outside the ceiling would be a place to put bots to get past
it.

`verified-by: bravebot_agent::bots::a_model_is_one_bounded_line_that_is_not_inherit`
`verified-by: bravebot_agent::bots::an_absolute_path_is_either_spelling_and_never_a_device_or_stream`
`verified-by: bravebot_agent::bots::a_bots_file_lists_each_bot_the_way_the_gui_read_it`
`verified-by: bravebot_agent::bots::a_row_with_a_repairable_field_is_listed_repaired`
`verified-by: bravebot_agent::bots::a_home_in_the_windows_spelling_is_kept_and_a_relative_one_is_replaced`
`verified-by: bravebot_agent::bots::every_entry_the_rules_drop_is_reported_with_its_reason`
`verified-by: bravebot_agent::bots::rows_past_the_hundredth_are_reported_with_their_slugs`
`verified-by: bravebot_agent::bots::rows_are_written_in_the_byte_order_of_the_slug`
`verified-by: bravebot_agent::bots::a_hundred_and_first_row_is_refused_and_every_row_is_kept`
`verified-by: bravebot_agent::bots::operations::a_form_that_breaks_a_bound_is_refused_and_nothing_is_written`
`verified-by: bravebot_agent::bots::operations::a_model_and_an_avatar_at_their_bounds_are_accepted`
`verified-by: bravebot_agent::bots::operations::an_edit_that_breaks_a_bound_or_names_no_bot_is_refused`
`verified-by: bravebot_agent::bots::operations::a_model_that_breaks_the_bound_or_a_missing_bot_is_refused_when_setting_it`
`verified-by: bravebot_agent::bots::operations::the_hundred_and_first_bot_is_refused_and_no_definition_is_written`
`verified-by: bravebot_agent::bots::operations::the_hundredth_bot_is_made`
`verified-by: bravebot_agent::bots::operations::a_row_the_read_drops_is_removed_by_the_write`
`verified-by: bravebot_agent::bots::operations::rows_come_out_in_the_byte_order_of_the_slug`
`verified-by: bravebot_ui_bridge::bots::a_refused_make_edit_or_home_is_answered_with_the_reason_and_the_code_for_it`
`verified-by: bravebot_ui_bridge::bots::a_null_name_purpose_or_avatar_is_a_bad_request_and_a_null_model_is_none`
`verified-by: bravebot_ui_bridge::bots::what_a_read_dropped_and_a_file_it_could_not_use_are_answered_beside_the_rows`
`verified-by: by-construction (localeCompare is JavaScript's and a Rust test cannot run it, so ui/scripts/bot-rows-bridge.test.mjs writes slugs of digits and hyphens through the real bravebot-rpc and asserts that the file holds them in byte order and that this is the order localeCompare gives in English, that a hand-edited file is listed in file order, a repaired row is listed repaired and a Windows path is accepted, and that a dropped row and a row past the 100th are each reported; ui/scripts/bot-store.test.mjs asserts the report reaches the window as one text naming each entry with its reason, and the path of a file that could not be read; make check-ui runs both)`
`verified-by: by-construction (the GUI's validation of a form, botFromForm, is deleted, so ui/scripts/bot-operations-bridge.test.mjs runs the real bravebot-rpc and asserts that a make with a null character in the name or in the purpose is refused and writes no bots file and no definition, that the 101st make is refused with every row kept and no definition written for it, and that the refusal is shown with its reason; make check-ui runs it)`

<a id="BOT-4"></a>
### BOT-4: the rows are the file `~/.bravebot/bots.json`, which the terminal and the GUI read and write

The rows are the `bots` array of the file `bots.json` in `~/.bravebot`, the folder
`bravebot_agent::home` resolves for every file that is the person's own, which is the directory
`BRAVEBOT_HOME` names when that is set ([state-directory.md](state-directory.md)). Where this file and
the design write `~/.bravebot`, they mean that folder. The
file holds that array and no other key. The terminal and the GUI read and write it through the
module ([BOT-5](#BOT-5)) and neither keeps a record of bots of its own. The GUI no longer reads
or writes the `bots` key of `bravebot-ui.json`. Rows already in that key reach the file as the next
paragraphs say.

The module reads the file from the folder `home::directory()` names and writes it only where
`home::writable()` names one. A run with `--incognito`, and a platform that names no profile folder,
therefore write nothing, and the write is refused with a message saying why. The folder is made with
mode 0700 and the file is written readable by the person only, through the functions of
`bravebot_agent::home`. A write takes an exclusive lock on the file `bots.json.lock` beside it, reads the whole file,
replaces the rows and writes the file through a temporary file in the same folder that is renamed over
`bots.json`, with the function the agent already uses for that (`bravebot_agent::mcp::replace`), and
then releases the lock. The lock is on a file of its own because the rename replaces `bots.json`, and
the operating system releases it when a program stops. The module uses `std::fs::File::lock`, so the
terminal, the GUI through the bridge, a second terminal and a background session take turns on
every system. It waits a few seconds for the lock and then refuses the write with a message that
names the lock file, so a program that holds it and hangs does not stop the others. A program stopped
during a write leaves the file as it was, and a reader never sees part of one. A file that cannot be
read or parsed gives no rows and is reported with its path, and a write is refused rather than made
over a file that did not parse. A missing file gives no rows, except as the next paragraph says.

**Moving the rows already in `bravebot-ui.json`.** One function of the module, separate from the
reading of [BOT-3](#BOT-3), does the move, as [COMPAT-001](../best-practices/compatibility.md#COMPAT-001)
asks of a migration. It runs when `bots.json` does not exist and `home::writable()` names a folder, in
whichever program reads the rows first, and it is the same code in both. It takes the lock on
`bots.json.lock` ([BOT-4](#BOT-4)) first, checks again under the lock that `bots.json` does not exist,
writes through a temporary file renamed into place, and releases the lock, so a program that moved the
rows and then made a bot is not overwritten by a program that checked earlier. It reads the `bots` key of
`bravebot-ui.json` in the data folder. When that key is an array with at least one entry, the function
writes `bots.json` holding every entry of the array, in its order, with every field the entry has.
That includes an entry BOT-3 would drop, an entry after the 100th and a field the module does not
know. The move repairs and drops nothing. The rules of BOT-3 apply to those entries when `bots.json`
is read, as they apply to any entry of that file. The module never writes `bravebot-ui.json`, so the
old key stays there until the GUI's next write of its own file, which no longer holds the key,
drops it. A `bravebot-ui.json` that is missing, has no `bots` key or holds an empty array moves
nothing. A `bravebot-ui.json` that exists and cannot be parsed, or whose `bots` key is present and is
not an array, cannot be converted. The move writes nothing then and reports the path of
`bravebot-ui.json`, and a make is refused until it is repaired or removed, since a bots file written
first would leave the rows in it unread. The function is removed in a later release.

**A bot's home folder** is `<data folder>/bot-homes/<slug>`, where the data folder is the one the
GUI keeps its own files in, beside the `bots/<slug>` folder it keeps its ground files in and never
inside it. The data folder follows the operating system and the name of the application,
`bravebot-ui`:

| System | Folder |
|---|---|
| Linux | `$XDG_CONFIG_HOME/bravebot-ui` when that variable is set to an absolute path, otherwise `$HOME/.config/bravebot-ui` |
| macOS | `$HOME/Library/Application Support/bravebot-ui` |
| Windows | `%APPDATA%\bravebot-ui` |

The function that computes the data folder takes the operating system and the environment as
arguments, and a test pins its result for each system in the table. The GUI does not use it: the GUI passes
the path Electron gives it (`app.getPath('userData')`) to the bridge as `dataFolder` on each request that reads
or writes the rows, because `launch()` in `crates/ui-bridge/src/arguments.rs` is shared with `bravebot-acp`. A renamed app or
a `--user-data-dir` therefore does not make the two differ, and the terminal uses the function. The module makes the home folder with mode 0700 when a bot is made, when a form's save
writes a bot, so an edit makes a home that was removed by hand, and when a conversation is started or
reopened in it, and reports whether it could be made. The first bot made when the data folder does
not exist makes the folder with mode 0700. A change of the model, an archive and a restore do not
make the home folder. The terminal does not change the avatar of a bot ([BOT-18](#BOT-18)). A data folder whose name is not
valid Unicode Transformation Format 8-bit (UTF-8) cannot be written into a row as text, and a make is refused with a message saying so
([PATH-001](../best-practices/paths.md#PATH-001)).

**Why one file in the agent's folder.** A person who goes between the two programs should find one set
of bots and one history of conversations, and a second record would have to be kept equal to the
first. The rows sit beside the definitions and sessions they point to, and the terminal finds them
without the GUI's application name. `bravebot-ui.json` is written as a file of disposable
preferences: a failed write is swallowed, a damaged file falls back to defaults, and every write
rebuilds the file from a fixed list of keys. Bots are the person's own record and are kept out of it.

**Why a home folder is under the GUI's data folder and not under `~/.bravebot`.** Each of two
reasons is enough:

- The GUI composes every bot's home there today (`botHome` in `ui/src/main/state.ts`), and each
  row, each conversation's folder and each memory file in a home carries that path. Moving homes would
  orphan them and is a redesign, which this extraction does not make.
- [TRUST-11](trust-map.md#TRUST-11) reads `~/.bravebot` as trusted by provenance and puts it outside
  the trust map. A bot's memory is a file in its home folder that runs write
  ([MEMORY-2](definition-memory.md#MEMORY-2)), and a home under `~/.bravebot` would make it read as
  trusted whatever the record of untrusted writes says ([MEMORY-5](definition-memory.md#MEMORY-5)),
  which is the round trip [MEMORY-4](definition-memory.md#MEMORY-4) closes. [MEMORY-2](definition-memory.md#MEMORY-2)
  also keeps no memory for a session whose working directory is inside `~/.bravebot`, so a bot whose
  home were there would keep none. A home under the data folder is read through the map like any
  project folder.

**Why the terminal does not change the avatar.** The seed in a row is what both programs draw the
face from. A bot the terminal makes takes its slug as the seed, so both programs draw a face for it.

`verified-by: bravebot_agent::bots::linux_uses_the_xdg_config_home_when_it_is_absolute`
`verified-by: bravebot_agent::bots::linux_falls_back_to_dot_config_under_home`
`verified-by: bravebot_agent::bots::macos_uses_application_support_under_home`
`verified-by: bravebot_agent::bots::windows_uses_appdata`
`verified-by: bravebot_agent::bots::a_missing_variable_names_no_data_folder`
`verified-by: bravebot_agent::bots::a_missing_bots_file_gives_no_rows_and_makes_no_file`
`verified-by: bravebot_agent::bots::a_file_that_does_not_parse_is_reported_and_never_overwritten`
`verified-by: bravebot_agent::bots::the_first_write_makes_a_private_directory_and_file`
`verified-by: bravebot_agent::bots::a_write_that_waits_past_the_limit_for_the_lock_is_refused_naming_it`
`verified-by: bravebot_agent::bots::a_write_that_fails_before_the_rename_leaves_the_file_as_it_was`
`verified-by: bravebot_agent::bots::the_rows_in_the_old_file_move_unchanged_and_the_old_file_is_untouched`
`verified-by: bravebot_agent::bots::a_second_start_does_not_read_the_old_key_again`
`verified-by: bravebot_agent::bots::an_existing_bots_file_is_not_moved_over`
`verified-by: bravebot_agent::bots::a_move_that_waited_for_the_lock_finds_the_file_another_program_wrote`
`verified-by: bravebot_agent::bots::nothing_to_move_leaves_no_bots_file`
`verified-by: bravebot_agent::bots::an_old_file_that_cannot_be_converted_is_named_and_nothing_is_written`
`verified-by: bravebot_agent::bots::nothing_is_moved_where_nothing_may_be_written`
`verified-by: bravebot_agent::bots::operations::the_first_make_creates_the_state_directory_the_file_the_data_folder_and_the_home_privately`
`verified-by: bravebot_agent::bots::operations::a_row_saved_while_its_home_cannot_be_made_is_kept_and_says_so`
`verified-by: bravebot_agent::bots::operations::a_make_is_refused_naming_the_old_file_while_it_cannot_be_converted`
`verified-by: bravebot_agent::bots::operations::a_make_after_the_rows_moved_from_the_old_file_keeps_them`
`verified-by: bravebot_agent::bots::operations::a_file_that_does_not_parse_is_never_overwritten`
`verified-by: bravebot_agent::bots::operations::a_data_folder_that_cannot_be_written_into_a_row_refuses_the_make`
`verified-by: bravebot_agent::bots::operations::a_write_that_fails_before_the_rename_leaves_the_file_as_it_was`
`verified-by: bravebot_agent::bots::operations::two_writers_at_once_lose_no_row`
`verified-by: bravebot_agent::bots::operations::a_lock_a_stopped_program_left_behind_is_not_held`
`verified-by: bravebot_agent::bots::operations::a_lock_held_past_the_wait_refuses_every_write_naming_the_lock_and_writes_nothing`
`verified-by: bravebot_agent::bots::operations::a_lock_held_past_the_wait_leaves_a_definition_as_it_was_for_an_edit_and_a_model_change`
`verified-by: bravebot_agent::bots::operations::a_data_folder_that_cannot_be_text_refuses_an_edit_a_model_change_and_an_ensure_before_any_row_is_dropped`
`verified-by: bravebot_agent::bots::operations::an_edit_makes_a_home_that_was_removed_by_hand`
`verified-by: bravebot_agent::bots::operations::setting_the_model_does_not_make_a_home_that_is_missing`
`verified-by: bravebot_agent::bots::operations::an_archive_and_a_restore_do_not_make_a_home_that_is_missing`
`verified-by: bravebot_agent::bots::operations::ensuring_a_home_makes_a_missing_one_privately`
`verified-by: bravebot_agent::bots::operations::ensuring_a_home_changes_no_row_and_says_when_it_could_not_be_made`
`verified-by: bravebot_agent::bots::operations::ensuring_the_home_of_a_bot_nobody_made_or_from_a_file_that_does_not_parse_is_refused`
`verified-by: bravebot_agent::bots::operations::a_home_the_row_names_is_the_one_made_whatever_the_data_folder`
`verified-by: bravebot_agent::incognito::a_private_session_refuses_to_make_edit_or_ensure_a_bot_and_writes_nothing`
`verified-by: bravebot_agent::incognito::a_private_session_refuses_to_archive_restore_or_delete_a_bot_and_writes_nothing`
`verified-by: bravebot_agent::home::a_machine_that_names_no_state_directory_refuses_to_make_edit_or_ensure_a_bot`
`verified-by: bravebot_ui_bridge::bots::a_bot_replaced_through_the_bridge_is_in_the_bots_file_and_listed_after_a_restart`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-rows-bridge.test.mjs runs the real bravebot-rpc against a scratch state directory and data folder and asserts that a bot the desktop makes is in bots.json and not in bravebot-ui.json, that a bot is listed again by a bridge started afterwards, that the rows of an old bravebot-ui.json written in the old forms move unchanged and in order and the old file is left as it was, that a second start does not read the old key again, that an absent or empty key leaves no bots file, and that an old file that does not parse, or whose bots key is not an array, gives no rows, names its path and refuses a make; ui/scripts/bot-store.test.mjs asserts that the desktop sends the data folder Electron names and never reads or writes the bots key of bravebot-ui.json, that a write the agent refuses is shown with its reason and the bot is not shown as saved, and that a file that could not be read is never written over; make check-ui runs both)`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-operations-bridge.test.mjs runs the real bravebot-rpc against a scratch state directory and data folder and asserts that a bot the desktop makes has its home folder, that an edit makes a home that was removed by hand and a change of model does not, that ensuring the home makes it again and answers false for a bot that does not exist, and that bravebot-ui.json never holds a bots key; ui/scripts/bot-store.test.mjs asserts that bot.make, bot.edit, bot.set_model, bot.ensure_home, bot.archive, bot.restore and bot.delete each carry the data folder Electron names; ui/scripts/session-directory.test.mjs asserts that session.new and session.open make the bot's home folder through bot.ensure_home before the request is forwarded; make check-ui runs all three)`

<a id="BOT-5"></a>
### BOT-5: one module of `bravebot-agent` reads and writes the rows, and both clients call it

The module `bots` of `bravebot-agent` holds the row, the validation of [BOT-2](#BOT-2) and
[BOT-3](#BOT-3), the slug composition, the ceiling, the reading and writing of the bots file
([BOT-4](#BOT-4)), and the operations that change a row: make, edit, set the model, archive, restore,
delete, migrate a bot made before definitions, record a conversation, forget a deleted conversation,
record the archive length and release a session. It also holds the rule that says whether a
carry-over turn is owed ([BOT-7](#BOT-7)), the
making of a bot's home folder ([BOT-4](#BOT-4)), the text of the consolidation and carry-over turns
([BOT-15](#BOT-15)) and the rule for which folders a bot's turn may run in ([BOT-9](#BOT-9)).

The terminal calls the module directly. The GUI's main process calls it through methods of the
bridge, and no longer keeps its own copy of the row's shape, its validation, the folder rule, the
making of the home folder or the reading and writing of the rows. `bot.define`, `bot.redefine` and `bot.migrate` already exist as
bridge methods that the window cannot call, and the methods this adds are not in the list of methods
the window may send either ([BOT-6](#BOT-6)).

The GUI's writes of its own file, `bravebot-ui.json`, no longer include the rows, so only the
module changes them.

What stays in the GUI is what only the GUI draws: the form. The briefing and the counters a bot made before definitions uses are not moved, because no turn reaches the briefing, since a bot is migrated before it is sent. The GUI's code for them is removed. The history of earlier memory versions is in `bravebot-ui-files`, and the module has no operation on it ([BOT-13](#BOT-13)). The code that
writes those counters into the rows (`noteBotMemory`, `keepingMemoryMark` and `noteBotNudged` in
`ui/src/main/bots.ts`) is part of that removal, and the rows have no `remembered` or `quiet` field.
`tidyMemory` stays in the GUI, since it acts only on a bot with no definition, which a first
turn migrates. It calls the replace and the recording directly where it called `keepingMemoryMark`,
and it is removed with the code for a bot made before definitions. The terminal does not tidy.

The module does not depend on a front end. It reads two things, the bots file and the
definitions the agent crate already reads. Both are the person's own, so it reads no untrusted
content, and it carries what the rows hold to the display or the definition without deciding
anything from it. [LAYER-1](layering.md#LAYER-1) (which crate may do what) changes when the module is built: the
`bravebot-agent` row then says the crate also holds the bots module, which reads and writes the bots
file in `~/.bravebot` and nothing else outside the person's own configuration. No crate is added. The
slice that builds the module edits the row, because the table states what the crates are and a row
ahead of its manifest fails the layering test. The rows of `bravebot-ui-files` and `bravebot-tui` change
the same way, in the slice that adds the library and the dependency ([BOT-13](#BOT-13)).

**Why a module of this crate.** The terminal and the GUI's bridge already depend on
`bravebot-agent`, and the definitions a bot is written as are written there
(`bravebot_agent::agents::make_definition`). A new crate would need a row of its own and could not
call the definition code without depending on this crate anyway. `bravebot-session` was weighed. It depends on `bravebot-agent`, so it could call `make_definition`,
and the terminal, the bridge and the command line already depend on it. Its row says it decides none of
what it serialises, and the module decides: a slug, the ceiling, the folder rule. Moving the module
there would relocate those decisions out of the driver without removing them, which the rule document
of this repository says is not a fix, so the argument is the person's own file ("Where the rows
come from"), whichever crate holds the code.

`verified-by: bravebot_ui_bridge::bots::a_bot_replaced_through_the_bridge_is_in_the_bots_file_and_listed_after_a_restart`
`verified-by: bravebot_ui_bridge::bots::the_methods_are_denied_to_the_host_and_offered_to_no_client`
`verified-by: bravebot_ui_bridge::bots::the_methods_that_make_and_change_a_bot_are_denied_to_the_host_and_offered_to_no_client`
`verified-by: bravebot_ui_bridge::bots::a_bot_made_edited_and_given_a_model_through_the_bridge_is_listed_after_a_restart`
`verified-by: bravebot_ui_bridge::bots::a_refused_make_edit_or_home_is_answered_with_the_reason_and_the_code_for_it`
`verified-by: bravebot_ui_bridge::bots::each_refusal_of_a_make_or_an_edit_is_answered_with_its_own_message_and_code`
`verified-by: bravebot_ui_bridge::bots::a_wrongly_typed_or_missing_field_is_a_bad_request_before_anything_is_written`
`verified-by: bravebot_agent::bots::operations::a_bot_is_made_with_its_definition_its_row_and_its_home`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-store.test.mjs asserts that it reads the rows through bot.list and replaces them through bot.replace, that it makes, edits and changes the model of a bot through bot.make, bot.edit and bot.set_model and makes a home folder through bot.ensure_home, each with the data folder Electron names, that changes made at the same moment, a make, an edit and a change of model among them, each reach the file, that the agent not running gives no rows and writes nothing, that ui/src/main/bots.ts and ui/src/main/index.ts no longer hold botFromForm, saveFormBot, ensureHome, bot.define or bot.redefine, and that none of bot.list, bot.replace, bot.make, bot.edit, bot.set_model, bot.ensure_home, bot.archive, bot.restore and bot.delete is among those a window may send; ui/scripts/bot-operations-bridge.test.mjs runs a make, an edit, a change of model and the making of a home folder through the real bravebot-rpc and asserts the rows, the definition and the folder they leave; make check-ui runs both)`

## Making and changing a bot

<a id="BOT-6"></a>
### BOT-6: the window reaches the writes of a bot only through the main process

A bot's name and purpose reach the system prompt as a definition's description and body, which the
person's own directory is trusted for and a renderer cannot write. The GUI's window therefore does
not call the bridge methods that write a definition or a row. The GUI shows
no confirmation dialog.

In the GUI:

- The methods that make or edit a bot, `bot.define`, `bot.redefine` and `bot.migrate` and the
  methods this design adds, are not in the list of methods the window may send to the bridge. The
  window reaches them only through a handler in the main process.
- The handler calls the bridge when the window asks, as it does today. It shows no dialog, and the
  form's Save button and the details pane's save on leaving a field are unchanged.
- A test pins that the methods are absent from the window's list.

In the terminal, the person types or pastes the name and the purpose into the terminal's own form. A
paste is the person's own words and is not labelled, as in the GUI's text area. The
form puts it in the field whole and never folds it to a marker, and draws control characters as
pictures without changing the text that is saved. A form field is not the prompt box, so
[PASTE-1](pasting.md#PASTE-1) does not apply to it, and a marker saved into a definition would stand
for words nothing could put back. The prompt input pane is the prompt box and folds a long paste as
PASTE-1 says. The make and edit form has one field to a row, and Tab moves between them. The purpose
field is edited as the prompt box is edited, with Shift-Enter for a new line
([INPUT-2](terminal-input.md#INPUT-2)). Enter saves the form and Esc cancels it.

**Why.** The definition is read on every turn as the person's own, so a write to it is the write
that matters most. Keeping the methods out of the window's list is what the GUI does today, and
the methods this design adds follow it.

**Limit.** A compromised window can ask the main process to write a definition, and nothing asks the
person first. Nothing here shows that the renderer displays fetched pages, which is what would let a
window be taken over.

`verified-by: bravebot_ui_bridge::bots::the_methods_that_make_and_change_a_bot_are_denied_to_the_host_and_offered_to_no_client`
`verified-by: bravebot_tui::app::layout_tests::n_opens_a_make_form_and_e_opens_the_selected_row`
`verified-by: bravebot_tui::app::layout_tests::shift_enter_and_ctrl_j_break_the_purpose_only_and_do_not_save`
`verified-by: bravebot_tui::app::layout_tests::escape_drops_a_form_and_the_next_one_starts_empty`
`verified-by: bravebot_tui::app::layout_tests::a_long_paste_lands_in_the_purpose_whole_and_a_picture_does_not`
`verified-by: bravebot_tui::app::layout_tests::a_paste_follows_focus_and_an_empty_paste_into_the_form_says_why`
`verified-by: bravebot_tui::app::layout_tests::an_inactive_field_shows_the_rest_of_the_text`
`verified-by: bravebot_tui::app::layout_tests::a_long_purpose_keeps_the_model_field_the_keys_and_the_refusal_on_screen`
`verified-by: bravebot_tui::app::layout_tests::a_control_character_is_drawn_as_a_picture_and_kept_in_the_field`
`verified-by: bravebot_tui::bots_form::a_long_paste_is_stored_whole_and_a_control_character_is_saved`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-store.test.mjs reads the list of methods the window may send out of ui/src/main/index.ts and asserts that none of bot.make, bot.edit, bot.set_model, bot.ensure_home, bot.archive, bot.restore, bot.delete, bot.record_conversation, bot.record_archived, bot.release_session, bot.forget_conversation, bot.list, bot.replace, bot.define, bot.redefine, bot.migrate, bot.owes_carry_over and bot.works_in is in it, so the window reaches the methods named here only through the main process; ui/scripts/session-directory.test.mjs asserts that session.new and session.open make the bot's home folder through bot.ensure_home before the request is forwarded; make check-ui runs both. That the handlers keep the channel names and arguments the window sends, and that no dialog is shown, is not asserted by a test)`

<a id="BOT-7"></a>
### BOT-7: the definition is written first, and the row only after it

Making a bot in the terminal writes the definition with `make_definition`
([MEMORY-8](definition-memory.md#MEMORY-8)), keeps the name the agent answers with as the row's
definition, and then writes the row. Editing a bot's purpose or model rewrites the definition with
`redefine` ([MEMORY-9](definition-memory.md#MEMORY-9)) and then writes the row. A refusal, a name
that is not a slug, or a file that cannot be written saves nothing, so the row and the file never
disagree about what the bot is for. Setting the model is the same two writes, and the terminal's
edit form offers it as a model field that takes an identifier [BOT-3](#BOT-3) accepts. It is not
offered while a turn of the bot is running, as the GUI's model picker is disabled then. The model
belongs to the bot, so the change applies to every conversation of the bot, and the next turn of an
open session runs under it. The make form has no model field. An empty model field in the terminal's edit form clears the stored model, so a later turn uses the configured default. Any other text is stored as typed, and a model the module refuses is left unchanged. An edit changes the row's name, purpose, model and avatar only,
and not the fields that record what happened. A bot that has no definition has its row written and no
definition made. The module reads the row once, under the lock of [BOT-4](#BOT-4), and writes the
definition and then the row before it releases the lock, so a conversation another program records
meanwhile is kept: that program waits for the lock, or finished before the read. Saves run one at a time,
and each reads the row as the save before left it. A write of the file that fails is reported with the
path and the reason, and the save is shown as failed. The definition is written before the row, so a
make whose row fails leaves a definition with no row, and an edit whose row fails leaves the new
purpose in the definition and the old one in the row. A second save of the same form writes both
again. The GUI swallows a failed write and shows the bot as saved, so once it calls the module it
shows the failure.

Every write of a row sets its `updated` to the time of the write, including the recording of a
conversation, of the archive length and the release of a session. Archiving sets `retired` to the
time of the archive, and archiving a bot that is archived sets it again. Restoring sets it to zero.
Neither touches the definition. A bot with a running conversation can be archived. Archiving the bot
whose session is open closes that session first, as the GUI closes the session on its screen, so
no header names a bot that is no longer in use.

Archiving a bot asks the person to confirm it, naming the bot, and so does deleting one
([BOT-8](#BOT-8)), as the GUI's confirmations do. Restoring is not confirmed.

A bot made before definitions has no definition, and is imported as it is ([BOT-3](#BOT-3)). The
module's `migrate` gives it one before its first turn, in either program, as the GUI's
`migrateBot` does today. It calls `migrate_definition` with the slug, the purpose, the
model and the old memory folder, which is the folder of the bot's first conversation or its home,
and stores the name the agent answers with as the row's definition
([MEMORY-11](definition-memory.md#MEMORY-11), which migrates such a bot and records its old notes as
untrusted). A bot that has a definition is returned as it is and the agent is not asked. The turn that
opened the bot runs addressed to the new definition. A migration that fails, or an answer that is
not a slug, sends nothing, leaves the row as it was and is tried again on the next turn. The module's
rule `owes_carry_over` is true for a bot with no definition in the old memory folder, and is
evaluated before the migration. The GUI calls both through the bridge and no longer holds
`migrateBot`, `owesCarryOver` or `oldMemoryFolder`. The terminal does not refuse a bot that has no
definition.

`verified-by: bravebot_agent::bots::operations::a_bot_is_made_with_its_definition_its_row_and_its_home`
`verified-by: bravebot_agent::bots::operations::a_definition_the_agent_refuses_saves_no_row`
`verified-by: bravebot_agent::bots::operations::a_definition_name_that_is_not_a_slug_saves_no_row`
`verified-by: bravebot_agent::bots::operations::a_definition_that_cannot_be_written_saves_no_row`
`verified-by: bravebot_agent::bots::operations::the_definition_is_written_first_and_stays_when_the_row_cannot_be_saved`
`verified-by: bravebot_agent::bots::operations::an_edit_rewrites_the_definition_and_keeps_what_happened`
`verified-by: bravebot_agent::bots::operations::an_edit_that_names_no_model_leaves_it_and_one_that_clears_it_removes_it`
`verified-by: bravebot_agent::bots::operations::an_edit_whose_definition_is_refused_saves_no_row_change`
`verified-by: bravebot_agent::bots::operations::an_edit_of_a_bot_whose_definition_is_gone_saves_nothing`
`verified-by: bravebot_agent::bots::operations::an_edit_of_a_bot_with_no_definition_writes_the_row_and_makes_none`
`verified-by: bravebot_agent::bots::operations::setting_the_model_rewrites_the_definition_then_the_row_and_clears_with_none`
`verified-by: bravebot_ui_bridge::bots::a_bot_made_edited_and_given_a_model_through_the_bridge_is_listed_after_a_restart`
`verified-by: bravebot_ui_bridge::bots::each_refusal_of_a_make_or_an_edit_is_answered_with_its_own_message_and_code`
`verified-by: bravebot_ui_bridge::bots::a_set_model_request_without_a_model_is_refused_and_leaves_the_bots_model_alone`
`verified-by: bravebot_agent::bots::operations::a_lock_held_past_the_wait_leaves_a_definition_as_it_was_for_an_edit_and_a_model_change`
`verified-by: bravebot_agent::bots::operations::archiving_a_bot_that_is_archived_stamps_retired_again_and_leaves_the_definition`
`verified-by: bravebot_agent::bots::operations::restoring_sets_retired_to_zero_and_each_write_stamps_updated`
`verified-by: bravebot_agent::bots::operations::an_archive_or_a_restore_of_a_missing_bot_writes_nothing`
`verified-by: bravebot_tui::bots_form::a_make_writes_the_definition_body_and_the_reread_list_holds_the_bot`
`verified-by: bravebot_tui::bots_form::an_edit_rewrites_the_definition_body`
`verified-by: bravebot_tui::bots_form::a_second_bot_with_the_same_name_takes_the_slug_with_a_number`
`verified-by: bravebot_tui::bots_form::the_hundred_and_first_bot_is_refused_and_every_row_is_kept`
`verified-by: bravebot_tui::bots_form::a_name_or_a_purpose_with_a_null_is_refused_and_the_file_is_unchanged`
`verified-by: bravebot_tui::bots_form::a_model_over_the_bound_or_with_a_newline_is_refused_and_the_stored_model_is_unchanged`
`verified-by: bravebot_tui::bots_form::an_empty_model_field_clears_the_model_and_any_other_text_is_stored`
`verified-by: bravebot_tui::bots_form::a_failed_write_names_the_path_and_keeps_the_list`
`verified-by: bravebot_tui::bots_form::archive_and_delete_wait_for_y_and_restore_does_not`
`verified-by: bravebot_tui::bots_form_incognito::an_incognito_make_says_so_and_writes_nothing`
`verified-by: bravebot_agent::bots::operations::a_legacy_bot_is_migrated_with_the_model_and_the_old_memory_folder_from_its_row`
`verified-by: bravebot_agent::bots::operations::a_bot_that_already_has_a_definition_is_not_migrated_again`
`verified-by: bravebot_agent::bots::operations::a_failed_migration_or_a_name_that_is_not_a_slug_writes_no_row`
`verified-by: bravebot_agent::bots::operations::a_migration_whose_row_cannot_be_saved_removes_the_definition_and_the_retry_uses_the_same_name`
`verified-by: bravebot_agent::bots::operations::owes_carry_over_is_true_only_for_a_bot_with_no_definition_in_its_old_memory_folder`
`verified-by: bravebot_ui_bridge::bots::a_legacy_bot_migrated_through_the_bridge_keeps_the_definition_and_a_failure_leaves_the_row`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-operations-bridge.test.mjs runs the real bravebot-rpc and asserts that the definition file holds the purpose as its body after a make, is rewritten by an edit, and has its model line set and removed by a change of model, with the row and the definition agreeing after each, and that archiving a bot that is already archived stamps retired again, leaves the definition bytes unchanged, sets retired to zero on a restore, and changes updated on each of those writes; ui/scripts/bot-store.test.mjs asserts that a make, an edit and a change of model the agent refuses are not shown as saved and are reported with the path and the reason the agent gave; make check-ui runs both. The recording operations are built, and ui/scripts/bot-operations-bridge.test.mjs asserts them through the real bravebot-rpc. The migration of a bot made before definitions is built: ui/scripts/bot-operations-bridge.test.mjs asserts that a legacy bot migrated through the bridge has the definition name on the row and the definition file written, and that a failed migration leaves the row; ui/scripts/bot-model.test.mjs asserts that the main process calls bot.migrate with the data folder and the slug and does not send a directory, a purpose or a model, and that a refusal of the folder returns before turn.send)`

<a id="BOT-8"></a>
### BOT-8: deleting a bot removes its row and its history of memory versions, and nothing else

Deleting a bot removes its row and the whole folder `<data folder>/bots/<slug>`, which holds the
history of the bot's earlier memory versions ([BOT-13](#BOT-13)) and any `ground.md` an earlier
GUI build left. The slug is checked before the path is composed. A delete from the terminal and
one from the GUI remove the same things. It
does not remove the definition, the sessions, the memory files in the folders the bot worked in, or
the home folder. A bot made afterwards with the same name has the same slug only if no row holds it,
and its definition takes the next free name ([MEMORY-8](definition-memory.md#MEMORY-8)), so it does
not share the earlier bot's memory, which is named after the earlier definition
([MEMORY-2](definition-memory.md#MEMORY-2), which names the memory after the definition).

A delete is refused while a turn of the bot is in progress in the program that is asked, and the person is told to stop it or
to archive the bot. The terminal keeps a record of its own turns for this, as the GUI does for
its own. A turn in the other program is not seen, because using both programs on one bot at once is not supported ([BOT-11](#BOT-11)). The terminal offers a delete only for an archived bot, and asks the person to
confirm it, naming the bot, as the GUI does. The module's delete does not check that the bot is
archived, as the GUI's delete channel does not.

There is no list of deleted slugs. A slug can return, and its home folder is then the folder the
earlier bot had.

**Why nothing else.** The definition is a file in the person's own directory and may have been
edited by hand. Sessions and memories are records of what happened. What a delete removes is what
the GUI's delete removes today, and the module does the removal for both programs.

`verified-by: bravebot_agent::bots::operations::deleting_a_bot_removes_its_row_and_its_history_folder_and_nothing_else`
`verified-by: bravebot_agent::bots::operations::a_delete_whose_history_folder_is_already_gone_removes_the_row`
`verified-by: bravebot_agent::bots::operations::a_history_folder_that_cannot_be_removed_leaves_the_row`
`verified-by: bravebot_agent::bots::operations::a_removal_that_fails_does_not_publish_the_row_deletion`
`verified-by: bravebot_agent::bots::operations::a_slug_that_is_not_one_removes_nothing_and_a_missing_bot_removes_no_folder`
`verified-by: bravebot_ui_bridge::bots::a_bot_archived_restored_and_deleted_through_the_bridge`
`verified-by: bravebot_ui_bridge::bots::a_bad_slug_or_a_missing_row_refuses_before_a_folder_is_removed`
`verified-by: bravebot_tui::app::layout_tests::delete_is_not_offered_for_a_bot_that_is_not_archived`
`verified-by: bravebot_tui::bots_form::delete_is_not_offered_for_a_bot_that_is_not_archived_and_removes_nothing`
`verified-by: bravebot_tui::bots_form::archive_and_delete_wait_for_y_and_restore_does_not`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-operations-bridge.test.mjs runs the real bravebot-rpc and asserts that a delete removes the row and the folder under the data folder at bots/<slug> and no other folder, and that the definition, a session file in the state directory's sessions folder, a memory file in a folder the bot worked in, the home folder and a sibling folder under bots all remain; ui/scripts/bot-store.test.mjs invokes the function the remove handler calls, with the running flag set, through the fake bridge and a real folder, and asserts the refusal is thrown without a list read, including when the bridge cannot list the rows, that bot.delete was not sent, and that the row and the folder remain; make check-ui runs both)`

## Talking to a bot

<a id="BOT-9"></a>
### BOT-9: a bot's turn runs in its home folder, a folder a person moved to in this run, or a folder recorded for the bot

The terminal sends a bot's turn only in a folder the session runs in and that is one of three: the
bot's home folder, composed from its slug under the GUI's data folder; a folder a person moved
the session to with a `/cd` they typed in this run; or a folder the bot's row records a
conversation in. The row records a folder only when a turn sent there ends, so no request adds one by
naming a path ([MEMORY-7](definition-memory.md#MEMORY-7), which gives the same rule to the GUI).
The terminal has no folder chooser. `/cd` is available in the bots layout as it is in the default
layout, takes a path the person types, and does what it does today ([TRUST-13](trust-map.md#TRUST-13)
and [SESSION-13](sessions.md#SESSION-13)), with everything `/cd` does and nothing more. The folder
counts as moved to in this run only after the agent confirms that `/cd` moved the session, so a
folder a line or a window names does not count. A bot's home is refused when it is not a directory. A new conversation starts in the bot's home folder,
and a person works in a project folder by typing `/cd` in it. The folders moved to in this run belong to
the run, as the GUI's set of folders chosen does, so a folder moved to in one bot's session is allowed
for another bot's in the same run and survives `/layout`.

A session started as a bot is asked the startup question ([TRUST-7](trust-map.md#TRUST-7)) about the
folder it runs in, with the same answers, the same answer from the bypass mode and the same honouring
of a remembered answer ([TRUST-23](trust-map.md#TRUST-23)) as any other session. The bots layout
opens with no session, so opening it asks nothing about the current directory. Leaving the question
starts no session and returns to the list. A recorded conversation that is continued takes its map
from its record and is not asked, and a record that holds no map is asked. An answer of no runs the
session with nothing trusted: every write is shown, the memory is withheld
([MEMORY-4](definition-memory.md#MEMORY-4)), and the layout draws the line the default layout draws
for a directory that is not trusted. The module and the layout read no trust map and decide nothing
about trust.

The check runs before each turn is sent, against the folder the agent confirmed the session runs in
and never against one the window or the line named, as the GUI's `sendBotTurn` runs it
(`worksIn`). It is a separate check from the one that decides which folders the GUI's window may
open a session in (`mayOpenSessionIn`), which also accepts every folder the main process offered to
the window, such as the folders in each row, and that check is not changed. The GUI's dialog that picks a folder is the GUI's way of
moving to one, and the terminal's is `/cd`. The helper that
reads the files in a folder is pinned only to a directory a person opened
([TRUST-20](trust-map.md#TRUST-20)), and that is not changed. A folder is accepted only as an
absolute path under the test of [BOT-3](#BOT-3).

A folder is compared with the recorded folders as the system gives it, never through a lossy
rendering ([PATH-001](../best-practices/paths.md#PATH-001)). A bot's turn is refused, with a line
saying the folder's name is not text, when the folder the session runs in is not valid UTF-8,
because the row holds text and the conversation could not be recorded ([BOT-11](#BOT-11)).

A bot's definition is read from `~/.bravebot/agents`, and [SKILL-3](skills.md#SKILL-3) leaves a file
there unread from provenance whenever a directory the session may write to is, or holds, the home
directory: the working directory, a directory opened beside it, or the session's own. In the home
directory itself the definition loads only as the project's own, through the trust map, so a no at
the startup question leaves it unread, and the bot keeps no memory there
([MEMORY-2](definition-memory.md#MEMORY-2)). In a folder that holds the home directory, and with the
home directory opened beside the session, it does not load at all. A bot's turn in any of these is
refused as a row whose definition does not resolve is ([BOT-10](#BOT-10)), naming the definition.

**Why not only the bot's folder.** A bot works in the folders a person sends it into. The GUI
already lets a bot take a project folder, and the terminal does the same.

`verified-by: bravebot_agent::bots::operations::a_turn_runs_in_its_home_a_recorded_folder_or_a_folder_opened_in_this_run`
`verified-by: bravebot_agent::bots::operations::a_folder_outside_those_three_is_refused`
`verified-by: bravebot_agent::bots::operations::a_path_that_differs_by_a_trailing_slash_or_a_dot_segment_is_refused`
`verified-by: bravebot_agent::bots::operations::a_home_that_is_not_a_directory_is_refused`
`verified-by: bravebot_agent::bots::operations::a_folder_whose_name_is_not_text_is_refused`
`verified-by: bravebot_ui_bridge::bots::a_turn_folder_checked_through_the_bridge_allows_only_the_three`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-directory.test.mjs asserts that worksIn calls the bridge, that the home is allowed, a not-yet-chosen absolute path is refused, an undefined folder is refused, a folder chosen in this run is allowed and the one beside it is not, a recorded conversation folder is allowed after a fresh load, a cancelled picker leaves the path refused, an offered directory that mayOpenSessionIn accepts is refused, a path that differs by a trailing slash or a dot segment is refused, and a home that is not a directory is refused while another folder is not; ui/scripts/session-directory.test.mjs keeps its assertions on mayOpenSessionIn; make check-ui runs both. The terminal's check of a folder moved to with /cd is not built. The module refuses a folder whose name is not text with the same answer as a folder outside the three, and the bridge only receives text, so the GUI does not show a separate line for it. A turn in the home directory, in a folder that holds it, or with that directory opened beside the session, is not refused here. Slice 7 still has that refusal)`

<a id="BOT-10"></a>
### BOT-10: every turn of a bot's session in the terminal addresses the bot's definition

In the bots layout every turn of a bot's session names the row's definition and runs under it, as a
turn after `--agent` does ([CLI-17](cli.md#CLI-17), which addresses every turn of a session to one
definition). That includes typed lines, `/loop` ticks, `/goal` rounds and the turns the terminal
composes. The name comes from the row the person chose, never from the line typed and never from a
reply. A row whose definition does not resolve runs nothing and says so, naming the definition. A
definition whose model needs a sign-in the machine has not made is refused
([ADDRESS-11](addressing-a-definition.md#ADDRESS-11), which refuses such a model).

A reply is drawn under the name the row gives, as the GUI draws it. A `/agent` line naming
another definition addresses that one for one turn. Scheduling a next turn and watching a file
are not offered to the run, as for any addressed run
([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). The model the definition
names is the session's model, and `/model` is refused in the session and says why, as [CLI-17](cli.md#CLI-17) says.

The layout writes no definition into the session record, as the GUI writes none, so the name
that [ADDRESS-3](addressing-a-definition.md#ADDRESS-3) carries into a resume is never a bot's. A
session opened in the layout takes its definition from the row. A name the record holds, because the
session was once started with `--agent`, is ignored, and the next save of the record names none. A
session resumed outside the layout, with `--resume`, `--continue`, `--fork`, `--from-pr` or `/resume`,
therefore has no recorded name and runs as the session's own. `/bg` is refused in a bot's session,
since a background process takes its definition from the record alone. A session is not recorded in
a bot's row unless it ran in the layout.

**Why.** The bot's reach is the definition's. A turn sent unaddressed would hold the session's whole
reach, wider than the bot's, on a turn the person did not type. A conversation continued outside the
layout is not narrowed, as the GUI's is not, and the layout is where a person continues one as
the bot.

`verified-by: none`

<a id="BOT-11"></a>
### BOT-11: a conversation is recorded in the row, and either surface can continue it

When a turn of a bot's session ends, the terminal records the session's identifier and folder in the
row, as the GUI does: the row's `session` becomes that identifier and the conversation is added
to `conversations` once. When the row exists, a session that is not an identifier, or a directory
that is not absolute, is refused and the file is left as it was. A missing row is left as it was
and is not an error, including when that session or directory would have been refused. A note that
does not write the file leaves a failure already shown. A turn that fails records the identifier too, when the agent's answer to the
failure carries one, as the GUI records it on `turn.error` as well as `turn.done`. The identifier
is read off what the agent answered, never off anything the person typed. A session continued in the bots layout is the row's `session` or one chosen from the
row's `conversations`, each shown with its folder.

A fork of a bot's conversation is a session with no bot, as the GUI makes it. It is not recorded
in any row, it is not addressed to a definition, since the record names none ([BOT-10](#BOT-10)), and it asks for no consolidation. It is listed in
the default layout and not in the bots layout.

A session started in one surface and opened in the other runs with the same history and trust map,
since both read the same session record. The row is the same row. A bot is used through one surface
at a time. Switching from one surface to the other is supported. Using the terminal and the GUI
on one bot at once is not supported, and nothing checks for it: each surface writes the session
record from its own state after every turn, so the later write replaces the earlier one.

Releasing a session sets the row's `session` to none and its `archived` to zero and keeps the
`conversations` entry, so the next turn starts a session. The GUI's main process has a channel
that releases a session, and nothing in the window calls it today. The terminal adds no trigger for
it either. What triggers a release today is a deleted conversation: after the agent confirms a
delete, the GUI removes that conversation from every row that lists it, and releases the
`session` of a row that pointed at it. The module holds that operation for the GUI's call. The
terminal has no way to delete a conversation and does not call it.

**Why the row and not the sessions.** The row is the list the GUI already keeps, so a bot shows
the same conversations in both surfaces. Reading session records for a slug would need a field the
record does not have, and a conversation can run in any folder, so a slug in the record would leave two
lists to keep equal. A slug in the record is a product alternative this design leaves open.

`verified-by: bravebot_agent::bots::operations::a_recorded_conversation_sets_the_session_and_is_added_once`
`verified-by: bravebot_agent::bots::operations::a_conversation_the_parser_would_not_keep_is_refused_and_writes_nothing`
`verified-by: bravebot_agent::bots::operations::an_archive_length_only_rises`
`verified-by: bravebot_agent::bots::operations::releasing_a_session_clears_it_and_keeps_the_conversation`
`verified-by: bravebot_agent::bots::operations::forgetting_a_deleted_conversation_removes_it_from_every_row_that_lists_it`
`verified-by: by-construction (the desktop's main process is not a crate this workspace compiles, so ui/scripts/bot-operations-bridge.test.mjs runs the real bravebot-rpc and asserts that a recorded conversation sets the session and is added once, that a second note of the same pair does not add it again, that an archive length only rises, that a release sets session to none and archived to zero and keeps the conversation, and that a forgotten conversation is removed from every row that lists it and releases a row whose session equals that id, with updated stamped on each write and the definition bytes unchanged; ui/scripts/bot-store.test.mjs asserts that a note which does not write the file leaves a failure already shown; make check-ui runs both. The terminal does not record a turn, does not continue a conversation from the layout, and has no delete of a conversation)`

<a id="BOT-12"></a>
### BOT-12: a bot's session is refused with incognito

A bot's conversation is recorded in the row ([BOT-11](#BOT-11)), and an incognito session writes
nothing ([INCOG-3](incognito.md#INCOG-3), which says it leaves no record). Starting a session as a
bot in a terminal started with `--incognito` is refused with a message saying so. The GUI has no
incognito mode, so this clause adds nothing to it.

**Why.** An incognito conversation could not be continued, and a bot's conversation is made to be.

`verified-by: none`

## The memory

<a id="BOT-13"></a>
### BOT-13: a bot's memory is the definition's, and the terminal shows it without trusting it

A bot's memory is `<folder>/.bravebot/memory/<definition>.md` in the folder the conversation runs in,
kept with the run's ordinary tools and told to the run by path and never by its bytes
([MEMORY-2](definition-memory.md#MEMORY-2), [MEMORY-4](definition-memory.md#MEMORY-4) and
[MEMORY-6](definition-memory.md#MEMORY-6), which keep and name the memory and add no tool). A write
that leaves it untrusted is recorded by directory under `~/.bravebot/untrusted/`
([MEMORY-5](definition-memory.md#MEMORY-5), which records it in every session, incognito included).
This design adds no memory tool and no memory file under the state directory. It keeps the history of earlier versions that the GUI keeps today, described below, which is a copy of a memory.

A write the run makes to its memory passes the gate of every file write
([TRUST-4](trust-map.md#TRUST-4), which asks before untrusted data is written to a path that is trusted): a session
that has read untrusted content is asked, and a session that has not writes without a question.

The terminal's bots layout shows the memory of the selected bot in the selected conversation's
folder, and a person edits it there, in their own editor: the hand-off Ctrl-G makes for the prompt
([INPUT-14](terminal-input.md#INPUT-14)), with the file's text as the starting text. What the editor
saves comes back as the save, the refusals below apply to it, and a refusal names the file. The view draws the file's bytes with control characters
replaced. The rows are inside the margin the terminal draws for untrusted text unless the memory's
standing is `Kept`. The standing is the one [MEMORY-12](definition-memory.md#MEMORY-12) gives `/memory`
and the run, from `agents::memories`, with the live trust map of the conversation's session when it is open, which
is the map the run uses, otherwise the conversation's saved map, or a map that trusts nothing when
there is none, as in the layout before a session. The pane builds a `Workspace` for the row's folder
to call it, since it takes a workspace, the map, permissions and a sink, and it needs no running session. A memory the map withholds, or
that the record of untrusted writes names, is therefore inside the margin. The view adds no wording
about the model, as the GUI's panel has none. The bytes come from `bravebot-ui-files` with no label and
the standing is their label, so the margin is decided from the standing and never from the bytes. The
pane shows a withheld memory to the person and sends it to no turn, so it does not take the yes that
MEMORY-12 asks for a run's read of one.
The memory is read for the view and the editor by `bravebot-ui-files`, as `preview.rs` reads a file for
display, and nothing read this way reaches the driver or a turn.
`/memory` ([MEMORY-12](definition-memory.md#MEMORY-12)) works in a bot's session as in any session and
lists paths and standings only. The memory pane reads and shows the file.

Reading the memory in the view and saving from it do not change the record: a save is written as a
program the person runs writes a file, as the GUI's panel does.

The GUI refuses a memory edit when the text is over 64 kilobytes (KB), 65,536 bytes of UTF-8,
or holds a null character, when the file's current text is not the text the person started from,
and while a turn of the bot is in progress in the same program. It reads and edits only a folder
the row records, which is the home or a conversation's folder. The terminal's view keeps the same four rules and offers the same
folders, which the layout shows as the selected conversation's folder. The GUI's panel also
lists them in a dropdown, which the layout does not draw.

A memory left untrusted stays
withheld after either. The one way a person grants trust to it is the yes to a quarantined read of
it ([TRUST-8](trust-map.md#TRUST-8), which asks whether to trust a file a turn reads that nobody vouched for). There
is no `@` form for the memory and the GUI gets no control for it.

The history of earlier versions of a memory that the GUI's panel shows today is kept by
`bravebot-ui-files`, which becomes a library as well as the helper binary it is, and not by the
module of [BOT-5](#BOT-5). Recording a version skips it when it equals the last one, and that
comparison is on bytes a run wrote, so it runs in a crate that is not the driver
([LAYER-2](layering.md#LAYER-2)). The history keeps up to 30 versions for each bot and folder, each
with its time, its source (the bot's turn or the person's edit) and its text, in
`<data folder>/bots/<slug>/memory-history-<hash>.json`. The module supplies the data folder and the
slug ([BOT-4](#BOT-4)) and no operation on the history. The root of a history operation is
`<data folder>/bots/<slug>`, composed by the program from the data folder and a validated slug and never
named by a window, as the state directory is, and the helper's request names that one root. The slice
that adds the library amends [TRUST-20](trust-map.md#TRUST-20) to admit it. The terminal calls the library directly
and the GUI's main process calls the same operations through the helper binary it already
uses for the memory file. The program that ran a turn records a version after it, and a person's
save records one, so the terminal and the GUI record the same way
([SHARE-001](../best-practices/shared-implementation.md#SHARE-001)). A delete removes the history
with the bot's folder ([BOT-8](#BOT-8)), which compares nothing. The memory view lists the
versions, newest first, with a read-only preview of the highlighted one and a key that opens it in the
editor as the text to save, which the person then saves or not. The save is checked against the file's
current text.
[MEMORY-4](definition-memory.md#MEMORY-4) describes the history as retired in the GUI. Nothing builds that, and this clause keeps the GUI's behavior, and the memory spec's no-copy clause ([MEMORY-4](definition-memory.md#MEMORY-4)) is not amended here, so the two disagree (see "Known costs" below). A version is drawn by the rule above, inside the margin unless the memory's standing is `Kept` now, and the history stays where the GUI keeps it, outside every folder a run reaches by default.

A version carries no mark of its own, as the GUI's history carries none. A version stored while the memory was untrusted is drawn without the margin once the memory's standing is `Kept` again, and a person who loads it and saves it writes those bytes to a trusted path. The GUI does this today, and this design keeps it as it is and does not close it (see "Known costs" below). A one-bit mark per version, taken from the record and not from the bytes, would close it and let this clause amend [MEMORY-4](definition-memory.md#MEMORY-4), and leaving the history out of the terminal's view would avoid it. Neither is adopted.

**Why the standing decides the margin.** The standing comes from the map and the record of
untrusted writes, through the one function the run uses, so the pane and the run cannot disagree. The
bytes decide nothing, so the driver does not branch on them. The comparisons on
bytes a run wrote are the history's skip of a version equal to the last and the save's check that
the file is the text the person started from. Both run in `bravebot-ui-files`, which is not the
driver, and the driver hands it paths and the person's text and receives no bytes back to decide
from.

`verified-by: none`

<a id="BOT-14"></a>
### BOT-14: `/undo` in a bot's session is the terminal's `/undo`

`/undo` is available in a bot's session as in any session
([SESSION-19](sessions.md#SESSION-19), which puts the conversation and the files its turns wrote back to before an earlier turn). A rewind into
a bot's memory file is recorded as a write is, so bytes the map will not trust are not put back
where they cannot be ([MEMORY-5](definition-memory.md#MEMORY-5)). The GUI's rewind calls the
same restore as `/undo` (`bravebot_agent::rewind::restore`), so both put a bot's memory file back and
record it as a write. This clause does not change the GUI.

`verified-by: none`

<a id="BOT-15"></a>
### BOT-15: the carry-over turn and the consolidation turn are one text in Rust

The text of the turn that asks a bot to bring its memory up to date after a compaction, and of the
turn that asks it to carry old notes into its memory ([MEMORY-11](definition-memory.md#MEMORY-11)),
is a constant in the module of [BOT-5](#BOT-5). The GUI's TypeScript text of both is removed.
A bridge method makes the turn from the constant and sends it, naming the session only, and the
terminal makes the same turn the same way. The window sends no text for it.

The turn is tagged `consolidation`, the one composed tag a front end may write today
(`wire::composed` in `crates/ui-bridge/src/wire.rs`). The GUI's handling of the tag on a request
from its window is unchanged: a window can still send it, and the tag changes how a
transcript draws the turn and nothing a run decides. The turn joins no recall history, holds no bytes from the conversation or the memory, and
names the definition's memory by its path. It is addressed to the bot's definition like every turn
of the conversation ([BOT-10](#BOT-10)).

**Why the text is in Rust.** The terminal needs it, and a second copy in TypeScript would be a second
implementation of one behavior ([SHARE-001](../best-practices/shared-implementation.md#SHARE-001)).

`verified-by: none`

<a id="BOT-16"></a>
### BOT-16: the terminal sends the consolidation turn after each compaction and after the migrating turn

The terminal sends the turn of [BOT-15](#BOT-15) on its own, in a bot's session, after each
compaction, whether `/compact` or the automatic one made it. It finds the compaction by the length
of the conversation's archive, a number the agent computes: the length after a turn, or after
`/compact`, is compared with the row's `archived`, and the row's figure is raised to it. The row's
figure only rises, as the GUI treats it, and is set to zero when the session is released. The
terminal records it in the row after each turn, so a compaction the terminal saw is not consolidated
again by the GUI.

The turn runs before any line the person queued during the turn. A flag set while it runs stops its
own end from sending another, and a turn that fails clears the flag. The terminal draws the turn as the GUI does, as one line that
reads "Asked to bring its memory up to date" (`ui/src/renderer/components/Transcript.tsx:1911`), never
as words the person typed. A failure leaves the memory as it was
and adds no error of its own.

The turn that migrated a bot ([BOT-7](#BOT-7)) makes the carry-over turn owed. It is sent after a
turn ends that is not itself a consolidation and after which no consolidation is owed. A turn that
ends after a compaction sends the consolidation first and leaves the carry-over owed. The end of a
consolidation sends nothing further, so the carry-over goes after the next turn that ends without a
compaction. It is sent only in the folder the old notes were recorded under, and is not sent when
the window or session closes first. The old notes stay recorded as untrusted, and a person can ask
the bot for them. The GUI marks the end event of each of these turns `consolidating` to hold its
window's queue until the turn's own announcement has been drawn, and the terminal holds the queued
lines for the same span.

`verified-by: none`

## The bots layout

<a id="BOT-17"></a>
### BOT-17: the terminal has a second layout for bots, chosen by name

The terminal's bot management is a second layout of the terminal interface, in the same crate and
binary as the current one. The current layout stays the default and does not change. The bots layout
shows the chat and the prompt input on the left and, on the right, panes that list the bots, the
selected bot's conversations and the memory view of [BOT-13](#BOT-13). The panes arrive with the
functions they present ([../PLAN.md](../PLAN.md)). At least one more layout is planned, so a layout
is chosen by name and nothing assumes there are two.

A person opens a layout with a startup flag, `--layout <name>`, and with a slash command typed in
either layout, `/layout <name>`. The default layout is named `default`, so `/layout default` is the
way back. A name that no layout answers to is refused by both. The bots layout holds only sessions
that are bots, and the default layout holds the session with no bot.

A layout decides which panes are drawn around the chat and which keys reach them. It does not own the
session, the transcript, the approvals or the margin drawn around untrusted content, which are one
implementation shared by every layout.

`/layout <name>` ends the current session and opens the named layout with no carried session. The
transcript, what the session read, the directories it added, the paths and hosts it was let reach and
the answers it was given, and the file watches its hooks began do not carry into the next one. The sandbox mode
and the Model Context Protocol servers the session started do carry, as they do across `/resume` and
`/clear`, so a mode typed with `/sandbox` and a server the person approved last until the process
ends. The ended session is already recorded
and is found by `--resume`, and a bot's conversation is already recorded in its row
([BOT-11](#BOT-11)). The command is refused, with a line saying why and nothing ended, while a turn is
running, while a question is waiting for an answer, while lines are queued, while a form or a
confirmation is open, drawn or not, with a line saying to save or cancel it first, while a `/loop`,
`/goal` or `/after` is pending, and in a run started with
`--incognito`, whose session writes no record to find again, and in a session that works under an
agent definition that `--agent` or a record gave, since the next session would not keep it. A
definition the `agent` setting chose does not refuse it, since the next session is addressed to the
same definition. It does not wait for the turn to end, as
`/branch` does ([SESSION-31](sessions.md#SESSION-31)), because the screen would change later with
nothing typed.

The interface is in a phase with a session or a phase with none, and a layout is drawn in both, as the
`--resume` picker is a screen before any session exists. `--layout bots` starts in the phase with no
session. `/layout bots` ends the current session and enters that phase. Choosing a bot or one of its
conversations begins a session in the bots layout, after the folder startup question
([BOT-9](#BOT-9)). `/layout default` begins a fresh session in the default layout. Archiving the bot
whose session is open ends the session and returns to the phase with none. The phase with no session
draws the prompt box, which takes only `/layout`, the key help and quitting, and any other line is
refused with a line saying to choose a bot first. The key list and the slash menu offer only what the
box takes. The prompt box follows
[INPUT-4](terminal-input.md#INPUT-4): Esc on it discards a half-typed line and does nothing on an empty
one, and Ctrl-C leaves only on the last rung of its ladder, so one press never ends the interface. In the
panes Esc goes back one level, and at the top level of a pane it returns focus to the prompt, as it does
in a session. The phase starts with focus in the bots list, so a person can choose a bot without a
chord. A person leaves the phase for a default session with `/layout default`, and leaves the interface
by the ladder, which Ctrl-C follows in the panes as it does in the prompt box.

The first Ctrl-C of the ladder says on screen that a second one leaves, at the right of the hint line
and at every width, as the default layout does. The hint line names the chord that moves between the
list and the prompt only where the list is drawn, since the chord does nothing below 100 columns.

Below 100 columns the phase always draws the line about width, and draws a refusal or any other
message on the rows under it, so a message never replaces that line. A message goes at the next key
that reaches the screen, whichever pane has the focus. While the list has the focus a paste and a drop
reach no prompt box, and Ctrl-C, Ctrl-D and the suspend chord ([INPUT-47](terminal-input.md#INPUT-47))
reach their handlers from the list. When rows still sit in `bravebot-ui.json` because they could not
be moved ([BOT-4](#BOT-4)), the list does not say there are no bots and draws a line naming that file.

`--layout <name>` goes only with a bare start of the interactive interface. It is refused, naming
both flags, with `--resume`/`-r`, `--continue`/`-c`, `--fork`/`-f` and `--from-pr`; with
`-i`/`--interactive`; with `--incognito`, since a bot's session is refused there
([BOT-12](#BOT-12)); with `--issue`, `--pr`, `--system-prompt` and `--append-system-prompt`; with
`-p`/`--print`, `--mode` (a manifest run included), each other flag that begins a one-shot run
(`--model`, `--advisor`, `--processor-model`, `--effort`, `--file`, `--session-ref`, `--add-dir`,
`--trust-workspace`, `--trace`, `--json`, `--json-stream`, `--input-format`, `--trust-input-lines`,
`--output-schema`, `--goal`, `--max-rounds` and `--max-tokens`) and a bare task; with `--plain` and
`--bg`; and with each command that starts neither a session nor a task (`doctor`, `update`,
`bug-report`, `auth`, `mcp`, `permissions`, `sessions`, `stats`, `attach`, `reply`,
`import-leo-creds`, `import-providers`, `completion` and `shell-init`). A test reads the command line's own lists of
the flags that begin a one-shot run and of the commands that start no session, so an entry added to
either is refused as well. A recorded session is not a bot's session, and a one-shot run or a
session in lines draws no panes. A bot's
conversation is continued from the layout's own list ([BOT-11](#BOT-11)). A name nothing would use is
refused and not ignored, for the reason [CLI-13](cli.md#CLI-13) gives about a settings file.
`--layout default` is accepted and is the same as no flag.

**Keys and focus.** The bots layout is a mode over the session, as the delegate view is and as the
info panel's files state is ([FILES-3](files-mode.md#FILES-3)). The prompt
and the chat keep the keys they have in any session. The action `panes` moves focus from the prompt to
the panes, and pressed again it moves focus back. Its default chord is `alt-p`, and a settings file
can move it as it moves the other actions ([INPUT-32](terminal-input.md#INPUT-32)). Esc at the top
level of a pane also returns focus to the prompt. In the default layout the key does what it did
before the action existed. The panel chord ([PANEL-5](info-panel.md#PANEL-5)) is taken and does
nothing in the bots layout: it does not open or close the info panel, which the layout does not draw,
and it does not write the saved choice of whether the panel is open. In a pane, Up and Down or `j` and `k` move through a list, Enter picks,
Tab moves to the next pane, and Esc goes back one level. A list is drawn as the `--resume` picker is
drawn, and a refusal or a failed write that names its path is a line under the list, as that picker
draws one. Archive and delete ask as that picker's delete asks, with a question naming the bot: `y`
agrees and any other key, Esc included, keeps it. The panes are drawn at 100 columns or wider, as the
info panel is ([PANEL-6](info-panel.md#PANEL-6)). Below that the chat is drawn alone with a line saying
the panes need a wider terminal. With no bots the list shows one line saying how to make one.

In the phase with no session there is no chat, so below 100 columns the phase draws the prompt box and
the line about width, with focus in the prompt box, since no list is drawn. `/layout default` and the
quit ladder work there, so a person can leave a layout that is unusable at that width. A form open when
the terminal narrows below 100 columns is not drawn, keeps its text, and is drawn again when the
terminal is wide enough again, and no key reaches it while it is not drawn. A confirmation of an
archive or a delete open at that moment is cancelled with nothing done, because a question that is not
on the screen is not answered. The person's own editor draws itself, and its width is not the layout's. An open
confirmation, drawn or not, is the nearest thing for the first rung of the Ctrl-C ladder of
[INPUT-4](terminal-input.md#INPUT-4) to stop, and the first press closes it with nothing done. An open
form that holds text does not close on Ctrl-C, so a stray byte cannot lose what was typed: the first
press offers the way out and leaves the form where it is, and only Esc closes it
([BOT-6](#BOT-6)). An empty form closes on the first press. The second press leaves, and the ladder goes
on as that clause says. No draft of a form is kept.

In the bots layout, `/branch` and `/resume` are refused with a line naming `--fork` and `--resume`,
since either would leave a session with no bot in a layout that holds only bots
([BOT-11](#BOT-11)). `/manifest` is refused in a bot's session, as `--mode manifest` is refused with
an addressed run ([CLI-17](cli.md#CLI-17)). `/panel` and `/files` are refused, since the layout draws no info
panel, and they leave the panel's open state and its saved choice as they were. The hint line does
not name the panel chord in the bots layout. `/rewind` follows the rule `/undo` does ([BOT-14](#BOT-14)). `/clear` and `/handoff` open a new
session of the same bot, which is added to the row after its first turn and whose record names no
definition ([BOT-10](#BOT-10), [BOT-11](#BOT-11)). `/memory`, `/mcp` and every other command work as
in any session.

`session-started` fires when the interface first begins a session, in either layout
([HOOK-10](hooks.md#HOOK-10)). A session begun later by `/layout` or by choosing another bot fires
neither moment, as `/resume` and `/clear` fire neither, and `session-ended` fires once when the
interface is left. A bots layout opened and left with no session begun fires neither.

The bots layout draws no info panel ([PANEL-6](info-panel.md#PANEL-6)), because its right-hand column
is the panes above. Each pane it draws is a pane for [MOUSE-1](mouse.md#MOUSE-1) to [MOUSE-3](mouse.md#MOUSE-3), so a
drag in one never copies another's text, and a selection is dropped when `/layout` stops drawing the
pane.

A bot's name, purpose and conversation folders are read from a file that any process of the account
can write. Each goes through the replacement of control characters that the terminal applies to
other drawn text before it is drawn, as [BOT-13](#BOT-13) says for the memory. Nothing branches on them, and drawing them reaches the display only.

There is no `--bot` flag and no command that makes, lists, edits or deletes bots. The layout does
each of them. The chat pane and the prompt input pane are drawn once a session can be started as a
bot, and no pane is drawn that does nothing. The prompt box of the phase with no session is the
interface's own prompt box, and it does something from the slice that draws it, slice 2b: it takes
`/layout`, the key help and quitting. Slice 2a has no prompt box in that phase, because the `--resume`
picker, which is the phase's only screen then, draws none.

`--agent <name>` ([CLI-17](cli.md#CLI-17), which addresses every turn of a session to one
definition) is unchanged. It does not open the bots layout and does not read the rows, and a
session it starts is not recorded in a row. `--layout bots` given with `--agent` is refused, since a
bot's session is addressed to the definition of its row and the flag would name a second one.

`verified-by: bravebot_tui::app::layout_tests::no_bots_draws_one_line_saying_how_to_make_one`
`verified-by: bravebot_tui::app::layout_tests::the_list_names_the_keys_that_apply_to_the_selected_row`
`verified-by: bravebot_tui::app::layout_tests::narrowing_keeps_a_forms_text_and_cancels_a_confirmation`
`verified-by: bravebot_tui::app::layout_tests::layout_is_refused_while_a_form_or_a_confirmation_is_open`
`verified-by: bravebot_tui::app::layout_tests::the_first_ctrl_c_on_a_form_or_a_confirmation_follows_the_ladder`
`verified-by: bravebot_tui::app::layout_tests::a_short_pane_still_draws_the_confirmation`
`verified-by: bravebot_tui::app::layout_tests::a_one_row_pane_names_the_bot_and_says_how_to_agree`
`verified-by: bravebot_tui::bots_form::a_make_selects_the_new_bot`
`verified-by: bravebot_tui::app::layout_tests::a_paste_while_the_form_is_not_drawn_or_a_confirmation_is_open_changes_nothing`
`verified-by: bravebot_tui::app::layout_tests::a_file_that_does_not_parse_shows_no_rows_and_names_its_path`
`verified-by: bravebot_tui::app::layout_tests::a_row_reads_a_bounded_prefix_of_a_purpose_with_no_length_bound`
`verified-by: bravebot_tui::app::layout_tests::a_row_with_a_multi_megabyte_purpose_draws_the_same_row_as_its_first_screenful`
`verified-by: bravebot_tui::app::layout_tests::rows_still_in_the_desktops_file_are_not_called_no_bots`
`verified-by: bravebot_tui::app::layout_tests::an_unreadable_file_is_named_and_the_old_rows_line_is_not_drawn_beside_it`
`verified-by: bravebot_tui::app::layout_tests::dropped_entries_are_a_line_naming_the_file`
`verified-by: bravebot_tui::app::layout_tests::the_panes_need_100_columns_and_the_prompt_box_is_drawn_at_any_width`
`verified-by: bravebot_tui::app::layout_tests::a_session_in_the_bots_layout_draws_the_panes_and_no_info_panel`
`verified-by: bravebot_tui::app::layout_tests::a_drag_in_one_pane_never_copies_the_other`
`verified-by: bravebot_tui::app::layout_tests::a_selection_in_the_pane_is_dropped_when_the_pane_stops_being_drawn`
`verified-by: bravebot_tui::app::layout_tests::the_arrows_and_j_and_k_move_through_the_list_and_stop_at_its_ends`
`verified-by: bravebot_tui::app::layout_tests::the_marker_is_drawn_on_the_selected_bot`
`verified-by: bravebot_tui::app::layout_tests::enter_on_a_bot_says_so_and_begins_no_session`
`verified-by: bravebot_tui::app::layout_tests::tab_with_one_pane_drawn_moves_nothing`
`verified-by: bravebot_tui::app::layout_tests::escape_returns_focus_to_the_prompt`
`verified-by: bravebot_tui::app::layout_tests::the_chord_moves_focus_both_ways_and_nothing_where_no_pane_is_drawn`
`verified-by: bravebot_tui::app::layout_tests::narrowing_the_terminal_returns_focus_to_the_prompt`
`verified-by: bravebot_tui::app::layout_tests::the_shortcut_list_names_the_panes_chord_in_the_bots_layout`
`verified-by: bravebot_tui::app::layout_tests::the_shortcut_list_of_the_default_layout_does_not_name_the_panes_chord`
`verified-by: bravebot_tui::app::layout_tests::in_the_default_layout_the_chord_types_its_letter_as_before`
`verified-by: bravebot_tui::app::layout_tests::ctrl_c_in_the_panes_follows_the_ladder`
`verified-by: bravebot_tui::app::layout_tests::the_panel_chord_does_nothing_in_the_bots_layout`
`verified-by: bravebot_tui::app::layout_tests::the_prompt_box_follows_the_ladder_of_the_input_spec`
`verified-by: bravebot_tui::app::layout_tests::a_line_other_than_a_layout_help_or_quitting_is_refused`
`verified-by: bravebot_tui::app::layout_tests::layout_quitting_and_the_key_list_are_taken_at_any_width`
`verified-by: bravebot_tui::app::layout_tests::layout_names_the_layouts_that_exist_when_it_is_refused`
`verified-by: bravebot_tui::app::layout_tests::the_chords_of_actions_that_need_a_session_do_nothing`
`verified-by: bravebot_tui::app::layout_tests::no_key_and_no_enter_hands_the_loop_an_action_the_screen_does_not_carry_out`
`verified-by: bravebot_tui::app::layout_tests::a_frame_is_drawn_after_an_event_that_changes_the_screen_and_not_after_a_timeout`
`verified-by: bravebot_tui::app::layout_tests::a_refusal_below_100_columns_is_drawn_under_the_line_about_width_and_the_next_key_clears_it`
`verified-by: bravebot_tui::app::layout_tests::the_message_of_a_refused_line_goes_with_the_next_key_wherever_the_focus_is`
`verified-by: bravebot_tui::app::layout_tests::a_key_that_takes_a_message_away_in_the_loop_owes_the_frame_that_removes_it`
`verified-by: bravebot_tui::app::layout_tests::a_paste_or_a_drop_while_the_list_has_the_keys_reaches_no_box`
`verified-by: bravebot_tui::app::layout_tests::a_picture_pasted_while_the_list_has_the_keys_names_no_chord`
`verified-by: bravebot_tui::app::layout_tests::ctrl_c_and_ctrl_d_reach_the_ladder_from_the_list`
`verified-by: bravebot_tui::app::layout_tests::the_suspend_chord_reaches_its_handler_from_the_list`
`verified-by: bravebot_tui::app::layout_tests::a_chord_moved_onto_ctrl_alt_c_or_d_does_not_reach_the_prompt_from_the_list`
`verified-by: bravebot_tui::app::layout_tests::a_drag_in_the_list_never_copies_the_prompt_box_and_the_reverse`
`verified-by: bravebot_tui::app::layout_tests::a_selection_in_the_list_is_dropped_when_a_resize_below_100_columns_stops_drawing_it`
`verified-by: bravebot_tui::app::layout_tests::the_first_ctrl_c_says_the_next_one_leaves_at_every_width`
`verified-by: bravebot_tui::app::layout_tests::a_key_that_takes_the_offer_back_takes_its_line_off_the_screen`
`verified-by: bravebot_tui::app::layout_tests::the_panel_chord_and_the_chords_that_need_a_session_take_the_offer_back_and_say_so`
`verified-by: bravebot_tui::app::layout_tests::the_panes_chord_takes_the_offer_back_and_says_so_where_no_list_is_drawn`
`verified-by: bravebot_tui::app::layout_tests::a_chord_takes_back_the_note_of_a_cleared_line_that_offered_no_exit`
`verified-by: bravebot_tui::app::layout_tests::the_panes_chord_takes_the_offer_back_and_says_so_where_the_list_is_drawn`
`verified-by: bravebot_tui::app::layout_tests::a_chord_with_no_offer_to_take_back_owes_no_frame`
`verified-by: bravebot_tui::app::layout_tests::a_chord_that_takes_the_offer_back_in_the_loop_owes_the_frame_that_removes_it`
`verified-by: bravebot_tui::app::layout_tests::a_chord_with_no_offer_standing_owes_no_frame_in_the_loop`
`verified-by: bravebot_tui::app::layout_tests::the_hint_names_the_panes_chord_only_where_the_panes_are_drawn`
`verified-by: bravebot_tui::app::layout_tests::a_step_quits_changes_layout_or_stays`
`verified-by: bravebot_tui::app::layout_tests::the_key_list_holds_only_what_the_phase_takes`
`verified-by: bravebot_tui::app::layout_tests::the_key_list_names_the_suspend_chord_the_phase_takes`
`verified-by: bravebot_tui::app::layout_tests::the_slash_menu_holds_only_what_the_phase_takes`
`verified-by: bravebot_tui::app::layout_tests::a_session_in_the_default_layout_still_offers_everything`
`verified-by: bravebot_tui::app::layout_tests::the_screen_takes_the_name_off_the_title_of_the_session_it_follows`
`verified-by: bravebot_tui::app::layout_tests::branch_resume_and_manifest_are_refused`
`verified-by: bravebot_tui::app::layout_tests::the_panel_commands_and_chord_change_neither_the_flag_nor_the_saved_choice`
`verified-by: bravebot_tui::app::layout_tests::files_is_refused_with_its_own_line_where_a_file_was_written`
`verified-by: bravebot_tui::app::layout_tests::the_default_layout_given_the_same_keys_opens_the_panel_and_saves_the_choice`
`verified-by: bravebot_tui::app::layout_tests::the_pane_reads_the_bots_file_through_the_module`
`verified-by: bravebot_tui::app::layout_tests::an_idle_session_ends_into_the_layout_named`
`verified-by: bravebot_tui::app::layout_tests::it_is_refused_with_a_line_while_a_turn_runs_lines_wait_or_a_loop_goal_or_after_is_pending`
`verified-by: bravebot_tui::app::layout_tests::typed_mid_turn_it_is_refused_at_once_and_never_waits`
`verified-by: bravebot_tui::app::layout_tests::the_layout_already_open_is_said_and_ends_nothing`
`verified-by: bravebot_tui::app::layout_tests::a_session_working_under_an_agent_definition_is_refused_and_ends_nothing`
`verified-by: bravebot_tui::app::layout_tests::a_definition_the_agent_setting_chose_does_not_refuse_the_layout`
`verified-by: bravebot_tui::app::layout_tests::the_command_is_in_the_table_and_spells_the_layouts`
`verified-by: bravebot_tui::app::layout_tests::an_open_mode_keeps_its_keys_through_both_handlers`
`verified-by: bravebot_tui::app::layout_tests::escape_in_the_panes_does_not_stop_a_turn_and_ctrl_c_does`
`verified-by: bravebot_tui::app::layout_tests::the_bots_layout_starts_with_no_session_and_the_default_begins_one`
`verified-by: bravebot_tui::app::layout_tests::a_session_that_chose_the_bots_layout_ends_into_its_screen_with_what_it_left`
`verified-by: bravebot_tui::app::layout_tests::leaving_the_screen_begins_a_fresh_session_or_ends_the_interface`
`verified-by: bravebot_tui::app::layout_tests::the_first_session_fires_the_moment_once_and_a_session_begun_by_layout_does_not_fire_it_again`
`verified-by: bravebot_tui::app::layout_tests::leaving_ends_what_the_session_was_granted_and_carries_what_resume_carries`
`verified-by: bravebot_tui::app::layout_tests::the_window_moves_only_when_the_selection_leaves_it`
`verified-by: bravebot_tui::app::layout_tests::the_last_page_keeps_the_marker_moving_up_the_screen`
`verified-by: bravebot_tui::app::layout_tests::a_window_that_changes_height_keeps_the_selection_in_view`
`verified-by: bravebot_tui::app::layout_tests::the_list_is_drawn_before_the_notes_under_it_where_both_cannot_fit`
`verified-by: bravebot_tui::app::layout_tests::escape_clears_a_typed_line_without_quitting`
`verified-by: bravebot_tui::app::layout_tests::escape_on_an_empty_line_does_nothing_and_twice_clears_and_stays`
`verified-by: bravebot_tui::app::layout_tests::ctrl_c_on_a_line_takes_it_and_offers_and_the_second_press_leaves`
`verified-by: bravebot_tui::app::layout_tests::ctrl_c_on_an_empty_box_offers_without_claiming_a_line_went`
`verified-by: bravebot_tui::app::layout_tests::a_key_between_the_two_presses_withdraws_the_offer`
`verified-by: bravebot_tui::app::layout_tests::input_that_is_not_a_key_withdraws_the_offer`
`verified-by: bravebot_tui::app::layout_tests::two_interrupts_in_one_read_do_not_leave_and_arm_nothing`
`verified-by: bravebot_tui::app::layout_tests::two_end_of_transmissions_in_one_read_do_not_leave`
`verified-by: bravebot_tui::app::layout_tests::a_refused_way_out_says_so_in_the_message_of_the_screen`
`verified-by: bravebot_tui::app::layout_tests::ctrl_d_leaves_only_from_an_empty_box_and_on_the_second_press`
`verified-by: bravebot_tui::app::layout_tests::a_press_on_its_own_after_a_run_still_leaves`
`verified-by: bravebot_tui::app::layout_tests::letting_go_of_the_keys_in_either_order_still_leaves`
`verified-by: bravebot_tui::layout_incognito::layout_is_refused_with_a_line_and_ends_nothing`
`verified-by: bravebot_cli::flags::layout_tests::every_entry_of_the_commands_own_lists_is_refused`
`verified-by: bravebot_cli::flags::layout_tests::every_flag_and_command_the_spec_lists_is_refused`
`verified-by: bravebot_cli::flags::layout_tests::a_flag_taken_out_earlier_and_a_bare_task_are_refused_by_name`
`verified-by: bravebot_cli::flags::layout_tests::a_bare_start_is_accepted`
`verified-by: bravebot_cli::layout_flag_tests::a_name_no_layout_answers_to_is_refused_naming_the_layouts`
`verified-by: bravebot_cli::layout_flag_tests::an_empty_name_is_refused_as_a_missing_one_is`
`verified-by: bravebot_cli::layout_flag_tests::a_parsed_layout_opens_that_layout_and_default_is_a_fresh_session`
`verified-by: bravebot_cli::layout_flag_tests::a_missing_name_is_refused`
`verified-by: bravebot_cli::layout_flag_tests::default_is_accepted_and_the_last_name_wins`
`verified-by: bravebot_cli::layout_flag::a_name_no_layout_answers_to_is_refused_with_the_names_that_exist`
`verified-by: bravebot_cli::layout_flag::a_line_that_is_not_a_bare_start_is_refused_naming_both_flags`
`verified-by: bravebot_cli::layout_flag::the_order_of_the_arguments_does_not_change_the_refusal`
`verified-by: bravebot_cli::layout_flag::the_version_and_the_help_win_over_the_layout_wherever_it_is_given`
`verified-by: bravebot_cli::layout_flag::the_usage_text_lists_every_layout_name`
`verified-by: bravebot_tui::keybindings::the_panes_chord_is_alt_p_and_can_be_moved`
`verified-by: bravebot_tui::keybindings::the_panes_chord_is_contested_like_every_other`

<a id="BOT-18"></a>
### BOT-18: the terminal draws a bot's face beside its name and leaves its avatar as it is

The bots list draws each bot's face beside its name, from the avatar seed in the bot's row, with the
function the opening screen draws its own face with (`crates/tui/src/avatar.rs`), which draws the same
shape from the same seed as the GUI. The face is ten columns wide and five terminal rows tall, so
each entry of the list is as tall as the face. It is drawn at rest. The GUI's blinking and
glancing are not drawn. The seed is bounded text ([BOT-3](#BOT-3)) from a file any process of the
account can write. It is the input of a drawing that decides nothing from it, and it is never drawn as
text. The layout does not change an avatar. A bot the terminal makes stores its slug as the avatar
seed, and the seed of every other bot is left as it is.

**Why.** A person who goes between the two programs recognises a bot by its face, and the terminal
already has the drawing. Five rows to a bot makes a long list, and the face can be taken out of the
list later if the list looks too packed.

`verified-by: bravebot_tui::app::layout_tests::each_bot_is_drawn_with_the_face_its_seed_describes_and_is_as_tall_as_the_face`
`verified-by: bravebot_tui::app::layout_tests::the_bots_are_drawn_in_the_order_the_module_returned_them`
`verified-by: bravebot_tui::app::layout_tests::control_characters_are_replaced_and_a_seed_is_never_drawn_as_text`

## Known costs

- **A bot in a folder that was not trusted has no memory, and the layout does not say so.** The person
  learns it from the session's line that the directory is not trusted, from the bot, which is told
  its memory is withheld, and from `/memory`, which lists the memory as withheld
  ([MEMORY-12](definition-memory.md#MEMORY-12)). The memory pane does not mark the folder, so a bot that does not mention it
  looks as if it keeps nothing. The GUI behaves the same today.
- **A write removes the entries the read dropped.** The read drops a row that fails the rules of
  [BOT-3](#BOT-3) and every row past the 100th, and the next write does not put them back. A hand-edited
  mistake in a row loses that row on the next write. [COMPAT-001](../best-practices/compatibility.md#COMPAT-001)
  says a reader must not discard data it cannot parse, and this clause does. The owner accepted it on
  2026-10-22. The read reports each dropped entry with its reason, and the GUI shows the report before
  the write removes the entry. The GUI's own save did the same before the module existed.
- **A purpose has no length bound.** It is the body of the definition read into every turn, so a long
  one costs tokens on every turn and can exceed the model's context. It is also stored in the shared
  file, which is read whole ([BOT-3](#BOT-3)).
- **Two programs at once on one bot are not supported, though the file is safe.** The writes take
  turns under a lock and are atomic ([BOT-4](#BOT-4)), so no row is lost to another program's write
  and no reader sees a partial file. The session record a bot's conversation uses is still replaced
  by the later write, so using both programs on one bot at the same time is not supported
  ([BOT-11](#BOT-11)). A write that waits longer than a few seconds for the lock is refused.
- **The refusal while a turn is in progress is written twice.** A delete ([BOT-8](#BOT-8)) and a
  memory save ([BOT-13](#BOT-13)) each check the turns of the program that is asked. The module reads
  rows and cannot see turns, so the terminal and the GUI each keep their own record of turns in
  progress. A turn in the other program is not seen, which is the case [BOT-11](#BOT-11) does not
  support.
- **A bot's session resumed in the terminal outside the layout runs as the session's own.** The
  layout writes no definition into the record, as the GUI writes none, and `/bg` is refused in a
  bot's session for the same reason ([BOT-10](#BOT-10)).
- **A process of the same account can rewrite the bots file, a definition or a session record.** A
  resume already believes more than that.
- **The design has no rule about adding a row field and does not protect a field from an older
  GUI.** [COMPAT-001](../best-practices/compatibility.md#COMPAT-001) covers new code reading what
  earlier versions wrote, and the move in [BOT-4](#BOT-4) follows it. No rule requires an older
  version to read a newer file.
- **A bot's row records folders as paths, and a directory put at a recorded path is one the bot may
  run in without a `/cd`.** A continued conversation there takes its map from its record, as any
  resume does ([BOT-9](#BOT-9), [KEPT-001](../best-practices/carried-grants.md#KEPT-001)).
- **The history of memory versions can launder untrusted bytes, and [MEMORY-4](definition-memory.md#MEMORY-4)
  plans to retire it.** [BOT-13](#BOT-13) ports it as the GUI has it, and that clause is not amended.
  [MEMORY-10](definition-memory.md#MEMORY-10) records the retirement as not built.
- **The GUI's rewind confirmation says a bot's memory is not rolled back, and the restore it
  calls puts the memory file back.** The text in `ui/src/renderer/components/Rewind.tsx` dates from
  when a bot's memory was a file the app owned. This design does not change it.
- **A bot cannot run in a folder that holds the home directory, or with the home directory opened
  beside the session, and the refusal does not say why.** The agent leaves its definition unread
  there ([SKILL-3](skills.md#SKILL-3)), and trusting the folder does not change that. In the home
  directory itself the bot runs after a yes at the startup question and keeps no memory
  ([BOT-9](#BOT-9)).
- **A directory the run may read that is, or holds, the GUI's data folder makes the history of memory
  versions readable by the run, and writable under the session's write rules.** That includes a
  session in the home directory, a folder opened with `/add-dir`, and a folder chosen in the GUI's
  "Open a project" dialog, which does not exclude the data folder. A version a run planted can be
  loaded and saved by a person ([BOT-13](#BOT-13)). The GUI behaves the same today.
- **A run in a session whose workspace is, or holds, the state directory can write `bots.json`, and the
  module reads the rows as the person's own.** The write is put to the person in the default and
  accepting-edits modes ([PERM-9](permissions.md#PERM-9)) and is not asked in bypass mode. A row
  names the definition and model a bot's turns use and the folders they may run in, and a session
  started as a bot is asked the startup question about its folder ([BOT-9](#BOT-9)). The GUI's rows
  have no such protection today. A `BRAVEBOT_HOME` that does not end in a `.bravebot` component is
  not protected by name.
- **A GUI started with `--user-data-dir` keeps its data folder elsewhere, and the terminal computes the
  default one.** Bot homes are made under each program's own folder, and each row stores an absolute
  `home`, so both programs run either. The move from `bravebot-ui.json` reads the default folder when
  the terminal runs first, so rows kept in the custom folder are not moved and stay hidden from the
  terminal. The history of memory versions and a delete are keyed on the data folder, so a version
  recorded by one program is not listed by the other and a delete in one leaves the other's folder
  ([BOT-4](#BOT-4), [BOT-13](#BOT-13)).
- **A `BRAVEBOT_HOME` that names the GUI's data folder, or a folder holding it or a bot's home, puts a
  bot's working folder inside the state directory.** The bot keeps no memory there
  ([MEMORY-2](definition-memory.md#MEMORY-2)), and a folder holding the state directory leaves its
  definition unread ([SKILL-3](skills.md#SKILL-3)). Both fail safe.
- **An empty model field in the terminal's edit form clears the stored model.** A field that is empty
  after its blank characters are trimmed is removed, and a later turn uses the configured default. Any
  other text is stored as typed, and a model the module refuses is left unchanged.
- **What `/memory` does in the bots layout before a session exists is not specified.** The command
  works in a bot's session ([BOT-13](#BOT-13)).
- **Each bot in the list is five rows tall because of its face, so a long list is tall.** The face can be
  taken out of the list later if the list looks too packed ([BOT-18](#BOT-18)).
- **The terminal draws the bots in the module's byte order and the GUI sorts with `localeCompare`.** In
  a locale that sorts a pair of letters as one letter, such as Czech or Danish, the two lists can differ
  for a slug that holds such a pair ([BOT-3](#BOT-3)).
- **A bot's conversation continued in the layout is addressed from its row, whatever the record
  names.** A name the record holds, because its session was once started with `--agent`, is ignored, and
  the next save of the record names none ([BOT-10](#BOT-10)). A conversation resumed from the default
  layout runs as the planner, and continuing it from the bots layout afterwards puts turns from both in
  one transcript.
- **One Esc discards the text of an open form, and nothing keeps it.** Esc is one byte, so a program
  able to write into the terminal can discard a long purpose. The prompt box keeps what Esc clears as a
  draft for Up ([INPUT-39](terminal-input.md#INPUT-39)), and the form keeps none ([BOT-17](#BOT-17)).
  The form is a deliberate screen and Esc is the person's own key for leaving it.
- **A bot's turn has the definition's reach, which a person narrows by editing the file.** A bot made
  to be careful is told to be, and nothing stops a definition with no `tools:` line from holding the
  session's reach ([MEMORY-8](definition-memory.md#MEMORY-8)).
- **Nothing asks the person before the GUI writes a definition** ([BOT-6](#BOT-6)).
- **A deleted bot's slug can return** ([BOT-8](#BOT-8)). Its definition and memory are not shared
  with the earlier bot, but its home folder is.
- **A service that drops the system message drops the definition's body.** Three backends exist and
  none drops it. The deployed gateway was not probed, and a service that dropped the system message
  would already break every instruction the program sends in it.
- **Accepting edits and bypassing answer the question about a memory write.** A session that has
  seen untrusted content then writes the memory without a person being asked. The memory is
  recorded untrusted, so a read of it is quarantined until a person says yes
  ([MEMORY-5](definition-memory.md#MEMORY-5)).
