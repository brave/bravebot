# Plan

The queue of intended work. What happened is in the pull requests, the commits and the live-test
evidence, and this file is updated when a change materially moves the status below.

## Bots layer

### Status

The design is proposed and waits for a design pull request. Slice 1 is built on the branch
`bots/slice-1-rows-module`, stacked on the design branch, and waits for a pull request. No other
slice exists. The design is [design/bots-layer.md](design/bots-layer.md) and the clauses are
[specs/bots.md](specs/bots.md), which is `proposed`. The clauses and the slices below agree with the
design.

Slice 1 reads and writes the rows through the `bots` module of `bravebot-agent` and the bridge
methods `bot.list` and `bot.replace`, which the graphical user interface calls with the folder
Electron gives it as `dataFolder` on each request. The bridge takes the folder on each request
because starting the bridge with a flag would need the same flag pinned in both programs that start
it. A write of more than 100 rows through `bot.replace` is refused and writes nothing. The
interface loads its page after its first read of the rows, so the move from `bravebot-ui.json`
happens before the interface writes that file.

The order of the remaining work:

1. Review the design against the overriding rule of `.claude/CLAUDE.md`, using
   [development/reviewing-for-the-rule.md](development/reviewing-for-the-rule.md). This includes the
   security review of the terminal's own form. The graphical user interface (GUI) shows no dialog, so there is none to
   review.
2. Build the slices below.

### Accepted conclusions

The design as a whole is proposed and not accepted. These decisions are accepted:

- The history of memory versions is ported as the GUI has it, with no mark on a version. A
  version stored while the memory was untrusted can be drawn without the margin and saved to a
  trusted path, as the GUI allows today, and it is a known cost
  ([BOT-13](specs/bots.md#BOT-13)). Existing requirements are not changed:
  [MEMORY-4](specs/definition-memory.md#MEMORY-4) still says the GUI's history is retired, so it
  disagrees with BOT-13. Decided 2026-10-13.
- The comparisons on bytes a run wrote are not in the module. The history's skip of a version equal
  to the last runs in `bravebot-ui-files`, which becomes a library as well as the helper binary, and
  the save's check against the person's starting text already runs there. The terminal calls the
  library and the GUI calls the same operations through the helper. `bravebot-tui` gains a
  dependency on `bravebot-ui-files`, and the rows of [layering.md](specs/layering.md) change in the slice
  that adds it, since a row ahead of its manifest fails the layering test. `tidyMemory` stays in the
  GUI for a bot with no definition and the terminal does not tidy. Decided 2026-10-15. A history
  request names the data folder's `bots/<slug>` folder as its one root, and slice 8 amends
  [TRUST-20](specs/trust-map.md#TRUST-20) to admit it.
- `--agent` is unchanged and does not open the bots layout. `--layout bots` with `--agent` is
  refused. Decided 2026-10-16.
- The bots layout mimics the GUI's handling of a bot's definition. Every turn of a bot's session
  is addressed to the row's definition, and the layout writes no definition into the session record,
  as the GUI writes none ([BOT-10](specs/bots.md#BOT-10)). A name the record holds is ignored in
  the layout. A session resumed outside the layout runs as the session's own, and `/bg` is refused in
  a bot's session. [CLI-17](specs/cli.md#CLI-17) and the rules of
  [ADDRESS-3](specs/addressing-a-definition.md#ADDRESS-3) about what the record carries are not
  amended. This replaces the decision of 2026-10-16, which assumed the record kept no name. Decided
  2026-10-22.
- `/layout <name>` works in either layout, ends the current session (already recorded) and opens the
  named layout with no session, and is refused while a turn runs, a question waits, lines are queued
  or the run is incognito. `--layout` goes only with a start of the interactive interface. A layout
  decides the panes around the chat and does not own the session. Decided 2026-10-20
  ([BOT-17](specs/bots.md#BOT-17)).
- A session started as a bot is asked the startup question about the folder it runs in, with the
  answers and honouring every session has. The bots layout opens with no session and asks nothing.
  An answer of no withholds the memory, and the layout does not mark it, which is a known cost.
  Decided 2026-10-20 ([BOT-9](specs/bots.md#BOT-9)).
- The reading of the rows ([BOT-3](specs/bots.md#BOT-3)) mirrors the GUI's `parseBots`, because the extraction keeps the GUI's behavior: the reader drops an entry
  it cannot read, and the next write removes it, as the owner confirmed again on 2026-10-22. The move
  of the rows from `bravebot-ui.json` ([BOT-4](specs/bots.md#BOT-4)) is new code and meets
  [COMPAT-001](best-practices/compatibility.md#COMPAT-001): it is a named function that runs once,
  carries every entry of the old key unchanged, writes nothing when it cannot convert, and is tested
  against a fixture written in the old format. Decided 2026-10-20.
- The rows in `bots.json` are used as the authority for the definition, the model and the folders a
  bot's turns use. A run in a session whose workspace is, or holds, the state directory can write the
  file, and the write is put to the person except in bypass mode. This is accepted as a known cost and
  is not one of the places `labels.md` admits, and no guard like the one in
  [SKILL-3](specs/skills.md#SKILL-3) is added. Decided 2026-10-22.
- The bots list draws each bot's face beside its name, at rest, from the row's avatar seed
  ([BOT-18](specs/bots.md#BOT-18)). The face can be taken out of the list later if the list looks too
  packed. The GUI's rewind confirmation, which says a bot's memory is not rolled back, is not
  changed. Decided 2026-10-22.
- The navigation of the bots layout follows what the terminal does today ([BOT-17](specs/bots.md#BOT-17)):
  a mode over the session as the delegate view is, a chord and Esc to move focus, the `--resume`
  picker's lists and delete confirmation, the prompt box's editor for the purpose field, the person's
  own editor for memory, panes drawn at 100 columns or wider, and one line when there are no bots.
  Decided 2026-10-22.
- These GUI behaviour changes stay as the design has them. Decided 2026-10-15:
  - A make at 100 rows, archived ones included, is refused with an error and every row is kept.
  - A name or purpose with a null character is refused.
  - A failed write of the rows is shown with the path and the reason.
  - A bots file that cannot be read or parsed gives no rows, is reported with its path and is never
    overwritten.
  - The file is written through a temporary file and a rename, under a lock on `bots.json.lock`, and a
    read reports each row it dropped. Decided 2026-10-20.
  - The rows move to `~/.bravebot/bots.json`.
  - Rows are written in the byte order of the slug, with no collation library. The GUI keeps sorting
    the list it draws with `localeCompare`, and the terminal draws the rows in the order the module
    returns them, so the two lists can differ in a locale that sorts a pair of letters as one letter.
    Decided 2026-10-22.

### Slices

Each slice is reviewable on its own and runs end to end on the real production path. They are in
order, smallest first. A slice either moves one piece of behaviour into the module of
[BOT-5](specs/bots.md#BOT-5) and its bridge and switches the GUI to it, or adds one terminal
function that works when the slice lands. A slice that moves behaviour removes the GUI's copy, so
no operation has two implementations after its slice
([SHARE-001](best-practices/shared-implementation.md#SHARE-001)), and it is verified by running the
GUI built from the branch. No slice draws a pane that does nothing: the chat pane and the prompt
input pane of the layout are first drawn in slice 7, when a session can be started as a bot. The prompt
box of the phase with no session, from slice 2b, is the interface's own and takes `/layout`, the key help
and quitting. There are no commands
for bots, so each terminal function is in the bots layout.

1. **The module reads and writes the rows, and the GUI uses them through the bridge.**
   - Add the module `bots` to `bravebot-agent` with the row, the validation of
     [BOT-2](specs/bots.md#BOT-2) and [BOT-3](specs/bots.md#BOT-3), the folder function of
     [BOT-4](specs/bots.md#BOT-4) that takes the operating system and the environment, and the
     reading of `~/.bravebot/bots.json` and the move of the rows already in `bravebot-ui.json`.
   - Each bridge method this design adds is a match arm of `crates/ui-bridge/src/bridge.rs` and a module
     of that crate, as `definitions.rs` is, with a caller in the GUI's main process through
     `bridge.request`, and none is added to `ALLOWED`.
   - Add the module's write of the bots file ([BOT-4](specs/bots.md#BOT-4)) with the lock and the
     rename. Add the bridge methods that list the rows and that replace them with the list the GUI's
     TypeScript computed, and switch the GUI's reading and writing of the rows to them, so the GUI no
     longer reads or writes the `bots` key of `bravebot-ui.json` ([BOT-4](specs/bots.md#BOT-4),
     [BOT-5](specs/bots.md#BOT-5)). Reads and writes change in the same slice, so the GUI and the
     terminal never hold different stores. Slice 3 replaces the replace method with the module's
     operations. The module moves the rows already in that key to the new file when
     it does not exist. The GUI sorts the rows it draws with `localeCompare`, as it does today. The
     slice edits the `bravebot-agent` row of [layering.md](specs/layering.md) to name the module.
   - Done when a bot made in the GUI after this slice is in `bots.json`, is listed after a restart and
     is never written to the `bots` key of `bravebot-ui.json`, and the GUI lists a bots file with several bots, one archived, one with a bad slug,
     one with no home and one that holds a bare conversation identifier and a `directory`, as
     [BOT-3](specs/bots.md#BOT-3) reads it: the bad one dropped, the one with no home given
     `<data folder>/bot-homes/<slug>`, the bare identifier paired with the `directory`, the bots in use
     in file order and the archived ones most recently archived first. A hand-edited file that is not
     in slug order is listed in file order, a row with a field that is repaired is listed repaired, and
     a path in the Windows spelling is accepted on Linux. A file that does not parse gives no rows,
     names its path and is not overwritten by the module. A file with a row that is dropped and a row
     past the 100th gives a report naming each with its reason, and the GUI shows it. A
     `bravebot-ui.json` written as the GUI writes it
     today, with bots in the old forms (a bare conversation identifier, a `directory`, no `home`, no
     `avatar`, no `retired`), a row [BOT-3](specs/bots.md#BOT-3) drops and a row past the 100th, and
     no `bots.json`, gives the same list whichever program starts first. `bots.json` exists afterwards
     holding every entry of the old key unchanged and in order, the dropped entries are reported when
     it is read, and `bravebot-ui.json` is unchanged. A second start does not read the old key again.
     A `bravebot-ui.json` whose `bots` key is absent or an empty array leaves no `bots.json`, and one
     that does not parse, or whose `bots` key is not an array, gives no rows, names its path and
     leaves no `bots.json`.
     Tests that read or write the state directory set `BRAVEBOT_HOME` to a scratch directory, and tests that
     make a bot home or the history pass a scratch environment to the data-folder function, so none writes
     into the developer's real GUI data folder. Verified with `cargo test -p bravebot-agent`, `cargo test -p bravebot-ui-bridge`,
     `make check-spec`, and the GUI's test and type checks. The folder test names Linux with and
     without `XDG_CONFIG_HOME`, macOS and Windows.
2a. **The terminal has layouts and a phase with no session.**
   - Add `Layout` to `bravebot-tui`, with `default` the only one, and a phase of `run` with no session,
     which the `--resume` picker becomes a screen of, so the seam carries real behaviour. The info panel's
     column is chosen through the layout. There is no visible change. [BOT-17](specs/bots.md#BOT-17)
     states the phases and the design has the transitions.
   - Done when `cargo test -p bravebot-tui` passes unchanged and, in a real terminal, the first frame at
     80, 100 and 140 columns equals main's, the panel is drawn at 100 columns and not below, `--resume`
     with Esc begins a fresh session and with Ctrl-C begins none and fires no hooks, and `/resume` and
     `/clear` behave as before. The
     pull request describes where the seam falls in `crates/tui/src/app.rs`, `render.rs` and `keys.rs`.
2b. **The bots layout lists the bots in the terminal.**
   - Add the second layout of `bravebot-tui` ([BOT-17](specs/bots.md#BOT-17)) with a pane that lists
     the bots with their faces ([BOT-18](specs/bots.md#BOT-18)), reading them through the module, with the keys, focus, width rule and empty line of
     [BOT-17](specs/bots.md#BOT-17). A person opens it with `--layout <name>` and
     `/layout <name>`, which is added to the table of commands. A name no layout answers to is
     refused, and `--layout bots` given with `--agent` is refused.
   - Done when, in a real terminal, the layout lists the bots as [BOT-3](specs/bots.md#BOT-3) reads
     them, the same list the GUI shows. It opens by the flag and by the command, an unknown layout
     name is refused, the default layout draws as before, a file that does not parse gives no rows
     and names its path, and a name with a control character is drawn with it replaced. Each bot is drawn with the
     face the GUI draws from the same seed, at rest, and a seed with a control character draws a
     face and no text.
     `/layout bots` ends the default session and `/layout default` returns to a fresh one, the ended
     session is found by `--resume`, and nothing the ended session read is in the new one. `/layout`
     is refused while a turn runs, while a question waits, while lines are queued and in an
     `--incognito` run (and, from slice 4, with a form or a confirmation open), and ends nothing when refused. `--layout` is refused, naming both flags, with
     each flag and command [BOT-17](specs/bots.md#BOT-17) lists, in a test that reads the command
     line's own lists of one-shot flags and of commands that start no session, and `--layout default`
     is the same as no flag. In the bots layout `/branch`, `/resume`, `/manifest` and `/panel` are
     refused, the layout draws no info panel, and each pane it draws is a pane for the selection of
     [MOUSE-1](specs/mouse.md#MOUSE-1), whose list gains them. The no-session phase draws the prompt box,
     which takes `/layout`, the key help and quitting and follows [INPUT-4](specs/terminal-input.md#INPUT-4):
     Esc on a half-typed line discards it and on an empty line does nothing, one Ctrl-C offers the way out
     and a second press leaves, and Esc at the top level of a pane returns focus to the prompt. The phase
     starts with focus in the bots list. The tests of INPUT-4's ladder run on this box. At 80 columns `--layout bots` draws the prompt box and the
     line about width with focus in the prompt box, `/layout default` and the quit ladder work, and at 100
     columns the list is drawn. Verified with `cargo test -p bravebot-tui` and
     `make check-spec`.
3. **The module holds the row operations, and the GUI calls them through the bridge.**
   - The GUI's TypeScript stops computing the rows. Each operation below becomes the module's, and the
     replace method of slice 1 is removed.
   - Add the writes to the module: the slug
     composition, the ceiling, and the operations of [BOT-7](specs/bots.md#BOT-7) and
     [BOT-8](specs/bots.md#BOT-8) (make, edit, set the model, archive, restore, delete, forget a
     deleted conversation, record a conversation, record the archive length, release a session), with
     the definition written first through `make_definition` and
     `redefine`, `updated` stamped on every write, `retired` stamped on each archive, and an edit of a
     bot with no definition writing the row only.
   - Add the making of a bot's home folder to the module, with mode 0700
     ([BOT-4](specs/bots.md#BOT-4)), made on a make, on a save from the form and when a conversation
     is started or reopened in it, and the first-run rules of that clause: the folders and the file
     are made by the first bot, and a write is refused where `home::writable()` names no folder.
   - Add the bridge methods and keep them out of the window's list
     ([BOT-6](specs/bots.md#BOT-6)). Switch the GUI's make, edit, set the model
     (`bravebot:bots:model`), archive, restore and delete, its `noteBotSession`, `noteBotArchived` and
     `releaseBotSession`, and its making of the home folder, to the bridge through its existing handlers, with no dialog. The GUI writes no `bots` key of
     `bravebot-ui.json` after this.
   - Done when, in the GUI, a bot made with a name and a purpose is listed after a restart, the
     definition file exists first with the purpose as its body, an edit rewrites the definition, a
     refused definition writes no row, a bot made with a taken name has the slug `<slug>-2` and its
     own definition, and a delete leaves the definition, the sessions and the memory files and removes
     the row and the folder `<data folder>/bots/<slug>` and no other folder. The hundred and first
     bot is refused with every row kept, rows are written in the byte order of the slug, and a
     name or purpose with a null character and a model over its bound or with a line break are refused. An edit of a bot
     with no definition writes no definition, a failed write of the file is shown with its path, an
     edit remakes a home removed by hand, every write stamps `updated`, and archiving an archived bot
     stamps `retired` again. A delete of a bot with a turn in progress in the GUI is refused. With
     no `~/.bravebot` and no data folder, the first bot made makes both with mode 0700, the file and
     the home folder. A make is refused, naming the path, while a `bravebot-ui.json` that does not
     parse exists and no `bots.json` does. A write interrupted before the rename leaves the file as it
     was, two writers at once lose no row, a lock held by a program that stopped is released, and a
     write that waits past the limit for a held lock is refused naming the lock file. The form refuses a name or purpose with a null character,
     the form's Save button and the details pane's save on leaving a field are unchanged, a turn that
     ends in the GUI records its conversation and the archive's length through the bridge, a
     search of `ui/src/main` finds no write of the rows outside the bridge, and a test pins that the
     methods are absent from the window's list.
4. **The terminal makes, edits, archives, restores and deletes a bot.**
   - Add the form that makes and edits a bot to the layout, with a model field in the edit form and no
     folder field, and the actions that archive, restore and delete. Archive and delete ask for a
     confirmation that names the bot, answered with `y`, and delete is offered only for an archived bot.
     Tab moves between the form's fields, Enter saves and Esc cancels.
   - Done when, in a real terminal, a bot is made with a name and a purpose and is listed in the
     GUI after it is restarted, an edit rewrites the definition, a bot made with a taken name has
     the slug `<slug>-2`, the hundred and first bot is refused with every row kept, a name or purpose
     with a null character and a model over its bound or with a line break are refused, and a failed write of the file is
     shown with its path. Archive and delete are refused until confirmed, delete is not offered for a
     bot in use, and a make in a run with `--incognito` is refused with a message saying so and
     writes nothing. A long text pasted into the purpose field lands whole, is saved as pasted and is
     not folded to a marker. Narrowing the terminal below 100 columns with the form open keeps its text and
     draws it again when the terminal is widened, and cancels an open archive or delete confirmation with
     nothing done. `/layout` is refused with a form or a confirmation open, drawn or not, and the first
     Ctrl-C closes a confirmation with nothing done, leaves a form that holds text open and offers the way
     out, and closes an empty form, and only Esc closes a form that holds text.
5. **The module holds where a turn runs and the migration of a bot, and the GUI calls them
   through the bridge.**
   - Add the rule for where a turn runs ([BOT-9](specs/bots.md#BOT-9)) to the module, checked before
     each turn of a bot is sent, with a bridge method the GUI's main process calls before it sends
     a bot's turn, and switch the GUI's copy of the rule to it. The GUI's separate check of
     which folders its window may open a session in is not changed. The method decides where a turn
     runs and sends nothing, so the GUI still sends the turn with the files and pastes staged for
     it (`sendBotTurn`).
   - Add `migrate` and `owes_carry_over` to the module ([BOT-7](specs/bots.md#BOT-7)) with a bridge
     method for each. Switch the GUI's `migrateBot` and `owesCarryOver` to them and remove its
     copies.
   - Done when the GUI's turn of a bot is shown unchanged, a turn requested for a folder outside
     the three is refused before it is sent, a bot made before definitions is migrated and its first
     turn is addressed to the new definition, and a search of `ui/src` finds no `migrateBot`,
     `owesCarryOver` or `oldMemoryFolder`.
6. **The GUI's briefing code is removed.** Remove the code that no turn reaches: `ground`,
   `groundText`, `seedProjectMemory`, `keepingMemoryMark` (`tidyMemory` then calls the replace and the
   recording directly), the branch of `sendBotTurn` that attaches the briefing, the `grounded` and
   `nudge` arguments, the counting of quiet turns and the `remembered` and `quiet` fields of the
   GUI's row, after a search for callers. `nudgeDue` and `noteBotNudged` run before a legacy bot is
   migrated and change only the counters in its row, which this slice removes with the fields. Done when a search of `ui/src` finds no `ground(`,
   `groundText`, `seedProjectMemory`, `nudgeDue`, `noteBotNudged` or `keepingMemoryMark`, the GUI's
   turn of a bot is shown unchanged and a legacy bot's first turn is migrated and answered as before.
7. **The terminal talks to a bot.**
   - Draw the layout's chat and prompt input panes. A session started as a bot addresses every turn to
     the row's definition ([BOT-10](specs/bots.md#BOT-10)), runs in the home folder, a folder the
     person moved to with `/cd` in this run or a folder the row records, and shows a line naming the
     folder when it is not the current directory. A conversation started in the home folder makes it
     again first when it was removed by hand ([BOT-4](specs/bots.md#BOT-4)). A bot made before
     definitions is migrated before its first turn.
   - Record the conversation in the row at the end of each turn, and of a turn that failed when the
     agent's answer carries an identifier ([BOT-11](specs/bots.md#BOT-11)), list the row's
     conversations in the layout and continue one. A fork of a bot's conversation is a session with no
     bot.
   - Add to the form of slice 4 what needs a session: a model field that is absent while a turn of the
     bot runs, a delete refused while a turn of the bot is in progress in the terminal, and archiving
     the bot whose session is open closes that session.
   - Refuse a bot session with `--incognito` ([BOT-12](specs/bots.md#BOT-12)).
   - Write no definition into the record of a bot's session, ignore a name the record holds, and refuse
     `/bg` in a bot's session ([BOT-10](specs/bots.md#BOT-10)). The `by-construction` line of
     [ADDRESS-3](specs/addressing-a-definition.md#ADDRESS-3) names each caller that gives a turn a
     definition, so it gains the layout's.
   - Done when a session started as a bot answers under its definition on a live model, a conversation
     started in the terminal opens in the GUI and the other way, a `/cd` typed in the bots layout
     moves the session and the next turn runs there, a folder named by a line or a window and not by a
     typed `/cd` is refused, a turn that fails records its conversation when the answer carries an
     identifier, a fork is a session with no bot, listed in the default layout and in no row, a row
     whose definition does not resolve runs nothing and names it, a turn in a folder that holds the
     home directory is refused the same way, a turn in the home directory itself runs after a yes at
     the startup question and keeps no memory, a `--resume` outside the layout
     answers as the session's own, a `/bg` in a bot's session is refused, a conversation whose record
     names a definition is addressed to the row's and its next save names none, and a bot session in a terminal started with `--incognito` is
     refused with a message saying so. A model set from the edit form between two turns is used by the
     second turn and shown by the GUI's picker, archiving the bot whose session is open closes the
     session, and a legacy bot's first turn is migrated and answered as before, in the GUI and in
     the terminal. A session started as a bot asks the startup question about its folder, a no
     withholds the memory, leaving the question returns to the list, a continued conversation whose
     record holds a map is not asked, and `--layout bots` asks nothing.
8. **The history of memory versions is kept by `bravebot-ui-files`, and the GUI calls it.** Add the
   library and the operations that record, list and review versions
   ([BOT-13](specs/bots.md#BOT-13)) to `bravebot-ui-files`, and switch the GUI's recording and
   listing to the same operations through the helper, and edit the `bravebot-ui-files` and `bravebot-tui`
   rows of [layering.md](specs/layering.md) with the library and the dependency. Done when the GUI's panel is unchanged, a
   version equal to the last is not recorded, a delete removes the history, and the root of a
   history operation is `<data folder>/bots/<slug>`, composed by the program and never named by a window.
   The crate has no library target today, and its request names one root, which a history request fills
   with that folder, so the slice adds the library and amends [TRUST-20](specs/trust-map.md#TRUST-20) to
   admit the folder.
9. **The memory view.** Add the view and edit of [BOT-13](specs/bots.md#BOT-13) to the layout for the
   selected bot in the selected conversation's folder, drawn with control characters replaced inside
   the untrusted margin unless the memory's standing is `Kept`, with no
   notice, deciding the margin from the standing. A save is refused over 64 kilobytes (KB), with a null
   character, when the file is not the text the person started from, and while a turn of the bot is
   in progress in the terminal, as the GUI's edit is for its own turns. A person's save does not change the
   record. The view lists the history of memory versions with a way to review one and load it into
   the edit, with a read-only preview of the highlighted version, memory is edited in the person's own
   editor, and the terminal records a version after each turn of a bot and on a save. A write by the
   run is asked first in a session that has read untrusted content. Done when a memory left untrusted
   by a run is drawn inside the margin, a memory whose standing is `Kept` is drawn without it, the layout with no session draws every memory inside it, a save
   from the view leaves the record as it was, a yes at a quarantined read takes the memory out of
   quarantine, a `/undo` over the memory file does not put back bytes the map would not trust
   ([BOT-14](specs/bots.md#BOT-14)), and the GUI's panel is unchanged. A turn in the terminal
   leaves a version that the GUI's panel lists.
10. **The consolidation turns are built in Rust, and the GUI uses them.** Add the constant and the
    bridge method that builds the consolidation and carry-over turns
    ([BOT-15](specs/bots.md#BOT-15)), and remove the GUI's TypeScript text and its copy of the
    test of the archive's length. Done when a window's request that carries the tag is handled as
    before, the GUI's consolidation after a compaction is unchanged, and a test pins the constant.
11. **The terminal consolidates after a compaction.** Add the terminal's hook
    ([BOT-16](specs/bots.md#BOT-16)): compare the archive's length across each turn and after
    `/compact`, record it in the row, run the turn before queued lines, draw it as the GUI's line
    "Asked to bring its memory up to date", and send the carry-over turn after the migrating turn, or
    after the next turn that ends without a compaction when a consolidation is owed first. Done when
    `/compact` and an automatic compaction in a bot's session each send one consolidation turn in a
    real terminal, the turn's own end sends no second one, a line typed during the turn runs after it,
    a failed consolidation leaves the memory unchanged with no error of its own, the turn is drawn as a
    consolidation and not as typed words, a carry-over owed with a consolidation goes after the next
    turn that ends without a compaction, and the GUI does not send a second turn for a compaction
    the terminal saw.

### Unresolved questions

Three cases are not specified. They are listed under "Known costs" in
[specs/bots.md](specs/bots.md#known-costs).
