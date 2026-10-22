# The bots layer

Status: proposed. Nothing in this document is built. The clauses are in
[../specs/bots.md](../specs/bots.md), and the order of work is in [../PLAN.md](../PLAN.md).

This design adds a layout to the terminal user interface (TUI) that mimics the graphical
user interface (GUI) for bots. A bot is a definition plus a row in a file the
terminal and the GUI share. This design does not change what a bot is. It lets the terminal use
the same bots, and moves the code that reads and writes the rows into Rust that both programs call.

## What a bot is

A bot is two things ([MEMORY-1](../specs/definition-memory.md#MEMORY-1)):

- A definition, `~/.bravebot/agents/<name>.md`, of type `worker` with `memory: project`. Its body is the
  purpose and its `model` line is the model. A definition is read on every turn as the person's own
  text ([MEMORY-8](../specs/definition-memory.md#MEMORY-8)).
- A row in `~/.bravebot/bots.json`, which holds what happened to the bot: its name, avatar seed, home
  folder, session, conversations, and whether it is archived. Today the GUI keeps the rows in the
  `bots` key of its own `bravebot-ui.json`, and the design moves them to the shared file.

Every turn of a bot's conversation is addressed to its definition
([MEMORY-10](../specs/definition-memory.md#MEMORY-10)). Its memory is
`<folder>/.bravebot/memory/<definition>.md` in each folder it works in, edited with the ordinary file
tools ([MEMORY-2](../specs/definition-memory.md#MEMORY-2)).

## Where things are kept

| What | Where | Written by |
|---|---|---|
| The rows | `~/.bravebot/bots.json` | the bots module, for both programs |
| A bot's definition | `~/.bravebot/agents/<name>.md` | `make_definition` and `redefine`, called by the module |
| A bot's memory | `<folder>/.bravebot/memory/<definition>.md` | the bot's own turns and a person's save from the memory view |
| The record of untrusted writes | `~/.bravebot/untrusted/` | the agent, for every session |
| A bot's home folder | `<data folder>/bot-homes/<slug>` | the module, when a bot is made or a conversation starts in it |
| The history of memory versions | `<data folder>/bots/<slug>/memory-history-<hash>.json` | `bravebot-ui-files`, called by the program that ran the turn and on a save |

The data folder is the GUI's own: `$XDG_CONFIG_HOME/bravebot-ui` or `$HOME/.config/bravebot-ui` on
Linux, `$HOME/Library/Application Support/bravebot-ui` on macOS and `%APPDATA%\bravebot-ui` on Windows
([BOT-4](../specs/bots.md#BOT-4)). Bots are kept out of `bravebot-ui.json` because that file is
written as disposable preferences: a failed write is swallowed, a damaged file falls back to defaults,
and every write rebuilds the file from a fixed list of keys (`ui/src/main/state.ts:48-56`, `:97-105`,
`ui/src/shared/state.ts:53-68`). Bots are the person's own record.

A terminal run before any GUI reads a missing bots file as no rows. The first bot made, where
`~/.bravebot` or the data folder does not exist, makes the folder with mode 0700. When
`bravebot-ui.json` cannot be read, the GUI reads four older files, `layout.json`, `view.json`,
`recents.json` and `forks.json`, for its other keys. The module does not read them. The terminal needs
none of their values, and they never held bots. The module writes a bots file that holds the rows and
nothing else, because no other program writes it. The history of memory versions stays in the data
folder as the GUI keeps it, and moving it is not part of this design.

## Why a module of `bravebot-agent` owns the rows

The GUI keeps the rows in TypeScript (`ui/src/shared/bots.ts`, `ui/src/main/bots.ts` and
`ui/src/main/state.ts`). The terminal has none. A person who goes between the two programs needs one
list of bots and one conversation. [SHARE-001](../best-practices/shared-implementation.md#SHARE-001)
says that when a second client needs behavior one client holds, both call one Rust implementation, so
the code is extracted and not written a second time. Extracting keeps the GUI's functionality and
design and changes only where the code is.

The code is a module of `bravebot-agent`. The terminal and the GUI's bridge already depend on that
crate, and it already holds `make_definition`, `redefine` and `migrate_definition`, which a bot's row
has to call in step with the definition. A new crate would need its own row in the layering table and
would call those functions through this crate anyway. `bravebot-session` was weighed. It depends on `bravebot-agent`, so it could call `make_definition`, and the terminal, the bridge and the command line already depend on it. Its row says it decides none of what it serialises, and the module decides: a slug, the ceiling, the folder rule. Moving the module there would relocate those decisions out of the driver without removing them, which the rule document of this repository says is not a fix, so the argument is the person's own file ("Where the rows come from"), whichever crate holds the code. The module does not depend on a front end.

```
  terminal (Ratatui)        GUI window (Electron, TypeScript)
          |                             |
          |                  main process (calls the bridge)
          |                             |
          |                      bridge methods (bravebot-ui-bridge)
          |                             |
          +--------------+--------------+
                         |
              bravebot-agent: bots module
              |                       |
   ~/.bravebot/bots.json          agents::{make_definition, redefine, migrate_definition}
```

The module reads two files, both the person's own: the bots file and the definitions. It carries what
a row holds to the display or to a definition and decides nothing from it. The GUI keeps what only
the GUI draws or decides: the form and the choice of an avatar seed.

## Why a layout of the existing interface

The bots layout is a layout of `bravebot-tui` and not a second terminal application. The chat is most
of `bravebot-tui`: streaming, approvals, trust prompts and the margin drawn around untrusted content.
A second application would rebuild those or need them extracted first, and a copy that skipped the
margin would display untrusted bytes unmarked.

## Layout and session states

`bravebot-tui` has no state with no session in its main flow today. `run` starts one session in
`event_loop`, and the screen state is `state::Session`, which is not the agent's conversation. The
`--resume` picker (`resume::choose`) is a screen shown before any session exists, with a loop of its own,
no session and no hooks. The seam adds two things: a `Layout`, with `default` the only one at first, and
a phase of `run` with no session, which the picker becomes a screen of. Drawing already chooses a right
column through the info panel's width rule, and key routing already lets a mode claim keys before the
prompt, so a layout chooses that column and adds one claim, and `default` does nothing different.

| From | Event | To |
|---|---|---|
| start | no flag or `--layout default` | session in the default layout, as today |
| start | `--layout bots` | no session, bots layout (refused under `--incognito`) |
| no session, bots | choose a bot or a conversation | session in the bots layout, after the folder startup question |
| session in default | `/layout bots` | the session ends and is already recorded, then no session, bots layout |
| bots layout | `/layout default` | a fresh session in the default layout |
| bot session | `/clear` or `/handoff` | the same bot, a new record |
| bot session | archive the open bot | no session, bots layout |
| bots layout | `/branch`, `/resume`, `/manifest`, `/panel`, `/files` | refused |
| either | `/layout` while a turn runs, a question waits, lines are queued, a form or a confirmation is open or a `/loop`, `/goal` or `/after` is pending, or under `--incognito` | refused, nothing ended |
| no session, bots | Esc at the top level of a pane | focus returns to the prompt |
| either | leave, by the ladder of [INPUT-4](../specs/terminal-input.md#INPUT-4) | the interface is left |

`session-started` fires at the first session the interface begins and `session-ended` when it is left
([HOOK-10](../specs/hooks.md#HOOK-10)). From slice 2b the phase with no session draws the prompt box, which takes
`/layout`, the key help and quitting. In slice 2a the phase is the `--resume` picker's screen, which
draws none. Slice 2a adds the seam with no visible change, and slice 2b adds the bots list.

## What the terminal does

Each item is specified in [../specs/bots.md](../specs/bots.md).

- **A second layout.** The bots layout is a second layout of the terminal interface, in the same crate
  and binary, with the chat and the prompt input on the left and panes for the bots, the selected bot's
  conversations and its memory on the right. The current layout stays the default. Navigation follows what the terminal already does: the layout is a mode over the session as the delegate view is, a chord and Esc move focus between the prompt and the panes, lists and the delete confirmation are drawn and answered as the `--resume` picker's are, and memory is edited in the person's own editor ([BOT-17](../specs/bots.md#BOT-17), [BOT-13](../specs/bots.md#BOT-13)). A layout is chosen
  by name, with `--layout <name>` at startup and `/layout <name>` in either layout, and a name no
  layout answers to is refused. The bots layout holds only sessions of a bot and the default layout
  holds the session with no bot. `/layout` ends the current session, which is already recorded, and
  opens the named layout with no session, so nothing a session read carries into the next. It is
  refused while a turn runs, a question waits, lines are queued or the run is incognito. `--layout`
  goes only with a start of the interactive interface and is refused with a recorded session, a
  one-shot run, lines mode, a background session or a command
  ([BOT-17](../specs/bots.md#BOT-17)). A layout decides the panes around the chat. The session, the
  transcript, the approvals and the margin are one implementation both layouts use, and slice 2a
  adds the seam, a layout type and a phase with no session ("Layout and session states").
- **No new commands.** There is no `--bot` flag and no command that makes, lists, edits or deletes
  bots. The layout does each of them. `--agent <name>` is unchanged and does not open the layout
  ([BOT-17](../specs/bots.md#BOT-17)).
- **The form.** A person types or pastes a name and a purpose into the terminal's own form. A paste is
  the person's own words, is not labelled and is put in whole, and a purpose written from it becomes
  the definition's description and body as typed text does
  ([MEMORY-8](../specs/definition-memory.md#MEMORY-8)). The form's bounds apply to a paste as to typed
  text: a name and a purpose are not blank and have no null character, and have no length bound. The
  terminal cuts a name to the width of its pane only when it draws it. A length ceiling could be added
  later. It would be checked when a bot is made or edited and never when rows are read, so it would
  not invalidate a row. The terminal cannot tell a paste from typing where it does not send bracketed
  paste ([terminal-input.md](../specs/terminal-input.md)), and a form that treats both the same does
  not need to. The make form has no model field and no folder field.
  The edit form has a model field, as the GUI's model picker lets a person change a bot's model
  between turns (`ui/src/renderer/components/Composer.tsx:423`, `ui/src/renderer/App.tsx:623`, `:1234`).
  The field is not offered while a turn of the bot is running, as the GUI's picker is disabled
  then. A change applies to every conversation of the bot and to the next turn of an open session
  ([BOT-6](../specs/bots.md#BOT-6), [BOT-7](../specs/bots.md#BOT-7)).
- **Folders.** The terminal has no folder chooser and keeps no list of recent folders. It does not
  browse folders or make a new one, which the GUI's dialog does, and the GUI keeps its dialog
  and its recent folders. A person moves a bot's session with `/cd`, which works in the bots layout as
  in the default layout. A folder counts as moved to in this run after the agent confirms that a
  `/cd` the person typed moved the session, and a folder a line or a window names does not count. A
  turn runs in the bot's home folder, a folder moved to with `/cd` in this run, or a folder the row
  records ([BOT-9](../specs/bots.md#BOT-9)). The row records a folder when a turn sent there ends
  (`ui/src/main/index.ts:376-379`).
- **A turn.** Every turn of a bot's session is addressed to the row's definition, and the conversation
  is recorded in the row so either program can continue it. The terminal and the GUI read the same
  session records and the same row, so a person can switch a bot from one program to the other. The
  session record is unchanged, including its `front` field, and no origin stamp is added. The layout writes no definition into it, as the GUI writes none
  ([BOT-10](../specs/bots.md#BOT-10)). A fork of a
  bot's conversation is a session with no bot. A bot session is refused with `--incognito`. The
  GUI has no incognito mode, so this changes nothing there. `/undo` works as in any session
  ([BOT-10](../specs/bots.md#BOT-10), [BOT-11](../specs/bots.md#BOT-11),
  [BOT-12](../specs/bots.md#BOT-12), [BOT-14](../specs/bots.md#BOT-14)).
- **The memory view.** The view shows the memory of the selected bot in the selected conversation's
  folder, lets a person edit it, and lists the history of earlier versions with a way to review one and
  load it into the edit. It draws the bytes with control characters replaced, inside the untrusted
  margin unless the memory's standing is `Kept`, the standing `/memory` and the run use
  ([MEMORY-12](../specs/definition-memory.md#MEMORY-12)). The margin is a bar the renderer draws before
  the content, which the content cannot forge, and the decision to draw it comes from the standing and
  never from the bytes. The view adds no wording about
  the model, because the GUI's panel has none. The GUI's panel draws no margin, and its
  quarantine card is a separate display. Reading and saving do not change the record, and the one way a
  person grants trust to a memory is the yes at a quarantined read of it
  ([BOT-13](../specs/bots.md#BOT-13)). A write by the run is asked first in a session that has read
  untrusted content, and a yes leaves the memory untrusted. A session that has read nothing untrusted
  writes without a question and leaves it trusted.
- **Consolidation.** After each compaction the terminal sends the consolidation turn, found by
  comparing the length of the conversation's archive with the row's `archived` figure after each turn
  and after `/compact`, as the GUI does (`ui/src/main/index.ts:382-410`). The turn runs before
  queued lines, and a flag stops its own end from sending another. The turn is drawn as one line
  reading "Asked to bring its memory up to date", as the GUI draws it
  (`ui/src/renderer/components/Transcript.tsx:1911`), and not as words typed. The text, the tag and the
  test of the archive are written once in Rust and called by the bridge and the terminal
  ([BOT-15](../specs/bots.md#BOT-15), [BOT-16](../specs/bots.md#BOT-16)).
- **Archive, restore and delete.** Archiving and deleting ask for a confirmation that names the bot,
  as the GUI's do (`ui/src/renderer/components/ConfirmArchive.tsx` and `ConfirmDelete.tsx`), once
  and keeping nothing. A delete removes the row and the memory history and cannot be undone, and an
  archive closes the bot's open session. Delete is offered only for an archived bot. A delete removes the row and the whole folder
  `<data folder>/bots/<slug>`, which holds the history of memory versions and, on a machine that ran an
  earlier build, an old `ground.md` and any half-written `ground.md.<uuid>.tmp`. The slug is checked
  before the path is composed. A delete does not remove the home folder, the definition, the sessions
  or the memory files in the folders the bot worked in. A delete and a save of the memory are refused
  while a turn of the bot is in progress in the same program. The GUI refuses them today with its
  own record of turns (`botHandles` and `runningHandles`, `ui/src/main/index.ts:80-81`, `:361-362`, `:784`,
  `:802-809`), and the terminal keeps the same record of its own turns. A turn in the other program is not
  seen, because using both programs on one bot at once is not supported. A delete during the other
  program's turn leaves that conversation unrecorded, since `noteBotSession` does nothing for a missing
  row (`ui/src/main/bots.ts:373-376`), and a memory save is refused when the file is not the text the
  person started from (`ui/src/main/memory.ts:58`)
  ([BOT-7](../specs/bots.md#BOT-7), [BOT-8](../specs/bots.md#BOT-8),
  [BOT-11](../specs/bots.md#BOT-11)).
- **The avatar.** The bots list draws each bot's face beside its name, from the row's seed, with the
  function the opening screen draws its own face with (`crates/tui/src/avatar.rs`). The face is drawn
  at rest, and the GUI's blinking and glancing are not drawn. A bot the terminal makes stores the
  slug as the avatar seed, and the seed of every bot is left as it is
  ([BOT-18](../specs/bots.md#BOT-18)).

## What changes in the GUI

The GUI's functionality and the row's design are kept. The code moves, and a few behaviors change
because one implementation serves both programs.

| The GUI today | This design |
|---|---|
| The rows are in the `bots` key of `bravebot-ui.json`, read and written by `ui/src/main/state.ts`. | The rows are in `~/.bravebot/bots.json`, read and written by the module through bridge methods. A named function of the module moves every entry of the old key unchanged, in whichever program runs first, and never writes `bravebot-ui.json` ([COMPAT-001](../best-practices/compatibility.md#COMPAT-001)). The file is then read by the rules of [BOT-3](../specs/bots.md#BOT-3). |
| The row's shape, validation, folder rule, home folder and reading and writing are TypeScript. | They are in the module. The GUI keeps the form and the choice of an avatar seed. |
| A save cuts the list to 100 rows by dropping the slug that sorts last, and sorts with `localeCompare`. | A make at 100 rows, archived ones included, is refused with every row kept. Rows are written in the byte order of the slug, with no collation library, and the GUI keeps sorting the list it draws with `localeCompare`. |
| A name or purpose with a null character is accepted, and the row is dropped on the next read. | The form refuses it. |
| A failed write of the rows is swallowed and the bot is shown as saved. | The failure is shown with the path and the reason. |
| A damaged `bravebot-ui.json` is treated as missing and the next write replaces it, losing the bots. | A bots file that cannot be read or parsed gives no rows, is reported with its path and is never overwritten. |
| The rows are written in place with `writeFileSync`, a read drops invalid rows without saying so, and two programs writing at once lose one write. | The file is written through a temporary file and a rename under a lock. A read reports each row it dropped and the next write still removes it. |
| A delete removes the row and the folder `<data folder>/bots/<slug>` with a recursive removal (`removeMemoryHistory`, `ui/src/main/memory.ts:45-49`). | Unchanged, done by the module, which compares nothing. |
| `migrateBot` (`ui/src/main/bots.ts:296-312`), `owesCarryOver` (`:282`), `oldMemoryFolder` (`:271`) and the consolidation text are TypeScript. | They are in the module and the GUI calls them through the bridge. `migrate` calls `migrate_definition` with the slug, the purpose, the model and the old memory folder, stores the name the agent answers with as the row's definition, and leaves the row as it was when the agent refuses. `owes_carry_over` is true for a bot with no definition in the old memory folder. The terminal migrates a bot before its first turn, and it does not refuse a bot with no definition. The GUI's handling of the `consolidation` tag on a window's request is unchanged (`sanitised`, `ui/src/main/sanitise.ts:43-78`, and `wire::composed`, `crates/ui-bridge/src/wire.rs:411-418`). |
| The history of memory versions is kept by the GUI alone, in TypeScript. | It is kept by `bravebot-ui-files`, which becomes a library as well as the helper binary. The terminal calls the library and the GUI calls the same operations through the helper. The skip of a version equal to the last is a comparison on bytes a run wrote, so it is not in the module, which is driver code. |
| The briefing file `ground.md` and the counters that decide when it is attached are code no turn reaches, since `sendBotTurn` runs `migrateBot` before it sends, and that returns a bot with a definition or throws (`ui/src/main/index.ts:196-227`, `ui/src/main/bots.ts:296-312`). | The code is removed, and the row has no `remembered` or `quiet` field. The code is `ground`, `groundText`, the `Grounding` type, `GITIGNORE`, `seedProjectMemory`, the branch of `sendBotTurn` that attaches the briefing (`ui/src/main/index.ts:228-245`), the `grounded` and `nudge` arguments, the counting of quiet turns (`noteBotMemory`, `nudgeDue`, `noteBotNudged`, `QUIET_MAX`) and the `grounded` flag of the renderer and the preload where nothing else uses it. The two calls that run before the migration, `nudgeDue` and `noteBotNudged` (`ui/src/main/index.ts:850-851`), change only the counters of a legacy bot's row. A search for callers confirms the list when the slice is written. |
| The GUI's model handler is TypeScript (`saveBotModel`) that asks the agent to rewrite the definition and then writes the row. | It calls the module's set-model operation, which does both writes. |
| The GUI shows no dialog before a bot is made or edited, and the methods that write a bot are not in the window's list. | Unchanged. The methods this design adds are kept out of the window's list the same way. |

## What the code does today

**The bridge.** `bot.define`, `bot.redefine` and `bot.migrate` are bridge methods
(`crates/ui-bridge/src/bridge.rs:316-318`). They call `crate::definitions::make`, `remake` and
`migrate` (`crates/ui-bridge/src/definitions.rs`), which call
`bravebot_agent::agents::make_definition`, `redefine` and `migrate_definition`. They are not in the `ALLOWED` set of `ui/src/main/index.ts:470`,
which is the one list of methods the window may send, so only the main process calls them. The main
process calls them today with no dialog.

**The GUI's rows.** `ui/src/main/state.ts` reads the file on every call, with no cache, and writes
the whole file non-atomically with `writeFileSync`. `parseState` keeps the fields it knows and drops
the rest, so a field added by another writer is erased the next time the GUI writes. The `Bot` row
has the fields slug, name, purpose, avatar, model, home, definition, session, conversations (each an
identifier and a directory), archived, remembered, quiet, retired, created and updated. The limits are
`BOTS_MAX = 100` with archived rows counted (`ui/src/shared/bots.ts:168`), a slug matching
`/^[a-z0-9]+(?:-[a-z0-9]+)*$/` of at most 64 characters (`:182`), `slugFor` cutting to 48 characters and
appending `-2`, `-3`, a model of at most 512 characters with no null character (`:134`), and an avatar
of at most 128 (`ui/src/main/bots.ts:180`). `noteBotSession` appends the
conversation and sets `session`, `noteBotArchived` only raises `archived`, and `releaseBotSession`
sets `session` to none and `archived` to zero.

**What a turn does.** Every turn of a bot's conversation names the row's definition (MEMORY-10). A turn
is sent only in the home folder, a folder chosen with the picker in this run, or a folder recorded for the
bot, and a folder is recorded when a turn sent there ends (MEMORY-7). After a compaction the GUI
sends a consolidation turn tagged `consolidation`, with TypeScript text, and decides it from the
length of the conversation's archive against `archived`.

**The terminal.** It has `--agent <name>`, which addresses every turn to a definition
([CLI-17](../specs/cli.md#CLI-17)), and no bots, no layout flag, no `/layout` command, no folder
chooser and no session `format` field. It has `/undo` and `/cd`.

**Forks.** `crates/ui-bridge/src/fork.rs` only cuts a conversation and knows nothing of bots. The
GUI's `forkFrom` makes the fork a session with no bot and records its lineage in the forks list
and in no bot's row, because a second conversation carrying the bot's name would be a second bot with
the same memory file.

**The session view.** `crates/ui-bridge/src/view.rs` is an opt-in display view that a client starts
for a fresh session. It refuses a saved resume and a fork, and it does not mention bots or
definitions. [BOT-11](../specs/bots.md#BOT-11) concerns the row and a session that is continued, so
the view does not affect it. Whether the bots layout's chat pane uses the view is an implementation
choice for slice 7.

## Trust analysis

The overriding rule of this repository is that the driver and the planner never hold untrusted content.
This design adds no way for untrusted bytes to reach either. The module carries rows and
text a person typed, and nothing it reads from a folder, a memory or a model reply decides what it does.

### What reaches the system prompt

A bot's purpose reaches a run as the body of a definition under `~/.bravebot/agents`, which the agent
already reads as the person's own text. The design adds no source for it. The text is typed or pasted
into a form by the person making or editing the bot.

### The window and the definition write

The bridge methods that write a definition are not in `ALLOWED`, so the window cannot call them
directly. The form in the window reaches them through a handler in the main process, and that handler
runs with no question today. The design keeps it so: the GUI shows no dialog, and the methods this
design adds are not in `ALLOWED` either. A compromised window can therefore call the handler with text
of its choosing, and the text becomes a definition body that is read as the person's own on every turn
of that bot. This is a known cost. Nothing in this design shows that the renderer displays fetched
pages, which is what would let a window be taken over. The terminal's own form takes the person's keys.

### Row fields a window can change

A window sends four fields: a name, a purpose, a model and an avatar seed (`botFromForm`,
`ui/src/main/bots.ts`). The main process composes the slug and the home folder, and the `definition`
is the name the agent answers with, so a window names no path and no definition. It can still ask the
main process to write any of the four with no question, as the GUI does today. The purpose becomes
the body of the definition every turn of the bot reads, and the model identifier selects the backend a
bot's turns go to. `sanitised` removes a `definition` from a window's `turn.send`
(`ui/src/main/sanitise.ts`), so a window cannot address a turn to a definition of its choosing.

### Where the rows come from

The rows are read from `~/.bravebot/bots.json`, which [TRUST-11](../specs/trust-map.md#TRUST-11) treats
as the person's own by provenance. A session whose workspace is, or holds, the state directory can
write that file with the file tools, and [SKILL-3](../specs/skills.md#SKILL-3) leaves a definition,
`AGENTS.md` or a style there unread from provenance for that reason. The rows have no such guard. The
reader runs before any session exists, in the terminal's layout and in the GUI's bridge, so it cannot
know which session wrote the file. A write to the file is a write to a protected path, since
`.bravebot` is a protected component ([PERM-9](../specs/permissions.md#PERM-9)): it is put to the person
in the default and accepting-edits modes and is not asked in bypass mode. A row decides which definition
and model a bot's turns use and which folders they may run in, and a session started as a bot is asked
the startup question about its folder ([BOT-9](../specs/bots.md#BOT-9)). The GUI's rows in
`bravebot-ui.json` have no protection today, since that file is outside `~/.bravebot`. A
`BRAVEBOT_HOME` that does not end in a `.bravebot` component is not protected by name. This is a known
cost.

### The four shapes applied

[reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md) lists four shapes a violation
takes in a diff.

| Shape | Result |
|---|---|
| A branch on untrusted bytes | None. The module validates a slug, a name, a purpose and a model, which are text a person typed and rows from the person's own file, with one known cost, under "Where the rows come from", which is not one of the seven places [labels.md](../specs/labels.md) admits, and carries them. The memory view decides its margin from the memory's standing and never from the bytes. The history's skip of a version equal to the last and the save's check against the text the person started from run in `bravebot-ui-files`, which is not the driver, and the driver receives no bytes back to decide from. The test for a compaction compares two numbers the agent computes. |
| The same branch moved into the kernel | None. No decision moves. |
| A `declassify` outside the gates | None. Nothing is declassified. |
| A `Labelled` built by hand | None. The module holds plain driver data and adds no construction site, so the count pinned in [labels.md](../specs/labels.md) does not change. |

The test for a compaction compares the length of the conversation's archive (`snapshot.archive.len()`,
`crates/ui-bridge/src/bridge.rs:2952`), a count of messages the driver's own conversation structure
holds, with the row's `archived` figure. The count is not a byte of
untrusted content, and the turn it sends is fixed text that carries no byte of the conversation. The
compaction that moves the figure is decided from token counts, as `reviewing-for-the-rule.md`
describes for `adopt_window`.

The consolidation turn ([BOT-15](../specs/bots.md#BOT-15)) is composed by the bridge from a
constant and names a path, never bytes. A window can still send the `consolidation` tag on its own
request, as it can today. The tag changes how a transcript draws the turn and nothing a run decides.

The history of memory versions is a copy of memory text under another path, kept in the GUI's data
folder as the GUI keeps it today. That folder is not `~/.bravebot`, so
[TRUST-11](../specs/trust-map.md#TRUST-11) does not make it trusted. Nothing is trusted until a person
grants it ([TRUST-1](../specs/trust-map.md#TRUST-1)), and reading is confined to the working directory
and the directories opened beside it ([TRUST-10](../specs/trust-map.md#TRUST-10)), which the bot's home
folder `<data folder>/bot-homes/<slug>` does not include.

A bot's home folder is under the same data folder and not under `~/.bravebot` for two reasons. The
GUI composes it there today, and every row and memory file carries that path. A home under
`~/.bravebot` would also be read as trusted by provenance and outside the trust map
([TRUST-11](../specs/trust-map.md#TRUST-11)), so a memory a run left untrusted would be read as
trusted ([BOT-4](../specs/bots.md#BOT-4)). [MEMORY-2](../specs/definition-memory.md#MEMORY-2) also keeps no
memory for a session whose working directory is inside `~/.bravebot`, so a bot with its home there
would keep none.

The inverse mistake is to invent a violation where none exists. The memory is bytes a run wrote. The
view draws it and does not send it anywhere, and the person's save is a write the person made.

### Backends and the system message

A definition's body reaches a run in the system message, so the question is whether any backend drops
or rewrites that message. Three backends exist (`crates/agent/src/backend.rs:3`): the aichat endpoint,
Amazon Web Services (AWS) Bedrock and a configured OpenAI-compatible gateway, and a local Ollama is a
gateway provider. No code rewrites or drops the system message for a model. Bedrock moves it into its
`system` field without changing the text (`crates/bedrock/src/protocol.rs:556-601`), and the aichat
protocol sends a system message. This program already sends a system message on every request, so a
definition's body inside it behaves as the rest of that message does.

For the gateway in front of Leo, Brave's assistant, the `ai-gateway` checkout at commit `c4d44fb`
passes system and developer messages from a client unchanged, and adds its own system prompt only when
the client sent none, according to its tests `test_existing_system_prompt` and
`test_existing_developer_prompt`. That is a reading of the checkout and not a probe of the deployed
endpoint, so whether the deployed service matches that commit is not known. A deployed gateway that
dropped the system message would already break every instruction the program sends in it. A probe sends
a live request with the person's credentials, so none is made. This is a known cost.

## Known costs

The known costs are listed once, under "Known costs" in [bots.md](../specs/bots.md#known-costs), which
governs them. The decisions about the memory history ([BOT-13](../specs/bots.md#BOT-13)) and about
where the rows come from ("Where the rows come from", above) are recorded there as costs and as the
alternatives that were not adopted.
