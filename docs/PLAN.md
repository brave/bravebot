# Plan

The queue of intended work. What happened is in the pull requests, the commits and the live-test
evidence, and this file is updated when a change materially moves the status below.

## Bots layer

### Status

The design is proposed and waits for a design pull request. Slice 1 is built on the branch
`bots/slice-1-rows-module`, stacked on the design branch, and waits for a pull request. Slice 2a is
built on the branch `bots/slice-2a-layouts`, stacked on `bots/slice-1-rows-module`, and waits for a
pull request together with slice 1. Slice 2b is built on the branch `bots/slice-2b-layout`, stacked on
`bots/slice-2a-layouts`, and waits for a pull request together with them. Slice 3a is built on the
branch `bots/slice-3-operations`, stacked on `bots/slice-2b-layout`, and waits for a pull request
together with them. Slice 3b is built on the branch `bots/slice-3b-archive`, stacked on
`bots/slice-3-operations`, and waits for a pull request together with them. Slice 3c is built on the branch `bots/slice-3c-record`, stacked on
`bots/slice-3b-archive`. There is no pull request. The branches of the design, slice 1, slice 2a,
slice 2b, slice 3a and slice 3b are pushed and equal the local ones. Slice 4 and the later slices
are not built. The design is
[design/bots-layer.md](design/bots-layer.md) and the clauses are [specs/bots.md](specs/bots.md),
which is `proposed`. The clauses and the slices below agree with the design.

Slice 1 reads and writes the rows through the `bots` module of `bravebot-agent` and the bridge
methods `bot.list` and `bot.replace`, which the graphical user interface (GUI) calls with the folder
Electron gives it as `dataFolder` on each request. The bridge takes the folder on each request
because starting the bridge with a flag would need the same flag pinned in both programs that start
it. A write of more than 100 rows through `bot.replace` is refused and writes nothing. The
interface loads its page after its first read of the rows, waiting at most five seconds, and holds
every write of `bravebot-ui.json` until a read answers that no rows are still waiting in that file
(`oldRowsWaiting` in the `bot.list` answer), so the rows in that file are moved or absent before the
interface writes it. A `bots.json` that does not parse leaves nothing waiting, so preferences are
still written while the problem is shown.

Slice 1 does not change what the interface does at the ceiling. Its make still cuts the list to 100
rows, dropping the slug that sorts last, before it calls `bot.replace`, so the refusal at 100 rows
that the module makes is reached from the interface in slice 3a. The rows the bridge returns carry no
`remembered` or `quiet` field, so slice 1 already stops persisting them. `noteBotMemory`,
`keepingMemoryMark`, `nudgeDue`, `noteBotNudged` and `QUIET_MAX` in `ui/src/main/bots.ts` run on a
count of zero until slice 6 removes them.

Slice 2a adds `crates/tui/src/layout.rs`. `Layout` has one value, `default`, and answers what the
interface draws to the right of the chat (`right_column`) and which keys it claims before the prompt
reads them (`claims_key`, which claims none). The info panel is that right column: `draw_frame` in
`render.rs` draws it where the layout says, and `panel::shows_context` asks the layout. `run` in
`crates/tui/src/app.rs` is a loop over the phases in `crates/tui/src/app/phase.rs`. The `--resume`
picker is a phase with no session, and what follows the picker and what follows a session are pure
functions that tests pin. Leaving the picker with Ctrl-C ends the interface and fires no hooks,
because the session-started moment fires only on the session path.

The slice differs from its text in five places. The getter on `Session` is `interface_layout`,
because `Session` already has a `layout` field for the transcript. The phase types are in
`app/phase.rs`. The layout's claim is consulted last in `claimed_by_a_mode`, so every existing mode
keeps its precedence, and slice 2b pins whichever order it chooses. `run` creates `Layout::default()`
and passes it to `Session::in_layout`, while `Phase`, the value `event_loop` returns and
`resume::choose` carry no layout, because the shape of that return is slice 2b's design and nothing
in 2a would read it. Three places still decide from the terminal width and the session's panel flag
and are not behind the layout: `Session::toggle_panel`, `Session::toggle_files` and the panel hint in
`render/hint.rs`. Slice 2b's entry below says what it does about them.

In a real terminal the first frame, which is the question whether to trust the directory, is
byte-identical to the base's at 80, 100 and 140 columns. The opening screen draws an animated mascot
whose face differs on each run, including between two runs of the base, so the screen after the
answer is not byte-identical. Every cell that differs from the base's lies in the mascot's rows and
columns, and no other cell differs. The `run` loop has no unit test. The real-terminal run and the
tests of the pure transitions cover it.

Slice 2b adds the second layout, `Layout::Bots`, and the pane of `crates/tui/src/bots_pane.rs` that
lists the bots. The pane reads them with `bravebot_agent::bots::list` and never through the bridge. It
draws each bot's face with `avatar::rows`, as the opening screen does, and draws the bot's name and
purpose after `render::printable` replaces control characters. A seed with a control character draws a
face and no text. The empty list, a file that does not parse and a file with dropped entries each draw
one line under the list. `--layout <name>` is `Start::Layout` and `/layout <name>` is
`Action::Layout`. A session ends into the phase with no session through `Held::leave_for_layout`, which
hands the workspace and sandbox on in `LayoutChange`, and `Phase::after_a_session_chose_a_layout`
returns `Screen::Bots` carrying the session left behind, so the interface's exit line and `--resume`
find it. `/layout default` from that screen begins a fresh session. The screen is a `Session` with
`without_a_session` set, drawn by `render::draw_without_a_session` and driven by
`app/bots_screen.rs`, so the prompt box, its editor and the Ctrl-C ladder of
[INPUT-4](specs/terminal-input.md#INPUT-4) are the ones a session has. The cases of that ladder that
concern an idle prompt box run on this box: Esc on a typed line and on an empty one, the first Ctrl-C
with and without a line, the second press, a key or other input between the two presses, two interrupts
or two end-of-transmission bytes in one read, a refused way out, a release of the keys, and Ctrl-D. The
cases about a turn, a summary, an aside, a goal check or a running command do not apply, because the
screen has none of them. The hooks `session-started` and `session-ended` fire on the session path only,
so a bots layout opened and left with no session fires neither.

The slice differs from its text in these places:

- `Phase` carries no layout and `event_loop` does not return one. `event_loop` takes a second
  out-parameter, `layout_change`, beside `switch`, because `Switch` and `Phase::Session` are matched
  by existing tests whose shape does not change. The layout is on `Screen::Bots`, the only screen of
  the bots layout, and `Phase::Session` always begins the default layout. A session in the bots
  layout begins in slice 7.
- The slice is one commit and ships alone, with `/layout` in the slash menu and the line about a later
  release on Enter. The owner decided this on 2026-10-29.
- In the phase with no session the `?` list and the slash menu offer only what the phase takes:
  `/layout`, leaving, the help and, at 100 columns or wider, the panes chord. The owner decided this on
  2026-10-29.
- `/layout` is refused in a session addressed to a definition the next session would not keep, as
  `--layout` is refused with `--agent`. The owner decided on 2026-10-29 to refuse it in an `--agent`
  session. The check is the session's standing definition, which a recorded definition also sets, and the
  session does not tell those two apart, so `/layout` is refused for both. A definition chosen by the
  `agent` setting is kept by the next session, so `/layout` is carried out there.
- `/layout` is carried out as it is typed so that it can refuse mid-turn, and is never queued. That
  adds it to the list of commands that skip the queue, so the one existing test
  `only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue` gained one entry,
  with the owner's approval on 2026-10-27. No other existing test changed.
- `/layout` keeps the sandbox mode and the Model Context Protocol (MCP) servers the earlier session started, as `/resume` and
  `/clear` keep them, and drops the transcript, what the session read and the workspace's session
  grants and answers. [BOT-17](specs/bots.md#BOT-17) is reworded to say so. The owner decided this on
  2026-10-27.
- `--layout bots` runs the same startup as a bare start before the screen opens: it reads the
  settings of the directory, builds the workspace, asks about the MCP servers that checkout requests
  and starts the approved ones. [BOT-9](specs/bots.md#BOT-9) says opening the bots layout asks nothing
  about the current directory, which holds for the trust question and not for this one. The owner
  decided on 2026-10-27 to accept it and record it. Starting no server for `--layout bots` needs
  startup to move to the point a session begins, and the design pull request settles it.
- The plan's text listed four conditions that refuse `/layout`. [BOT-17](specs/bots.md#BOT-17) lists
  six. The slice refuses on a turn running, a question waiting, lines queued, a `/loop`, `/goal` or
  `/after` pending, and an incognito run. The two that need slice 4, an open form and an open
  confirmation, are added in that slice.
- `--layout` is refused with every flag and command BOT-17 lists, including `--layout default`, which
  is accepted only on a bare start of the interface. The shell completion scripts name `--layout`.
- Enter on a bot says "Starting a session as a bot arrives in a later release." under the list and
  begins no session. The pane moves a selection through the list, and Enter confirms it with that
  line. Tab moves nothing while one pane is drawn.
- The action `panes` moves focus between the prompt and the panes. Its default chord is `alt-p` and a
  settings file can move it ([INPUT-32](specs/terminal-input.md#INPUT-32), which now says fourteen
  chords). The panel chord is taken and does nothing in the bots layout: it opens and closes nothing
  and saves nothing. `/panel` and `/files` are refused with a line, and the hint line does not name
  the panel chord. The owner decided both on 2026-10-27, after `alt-l` was found to contest an
  existing test that moves the `watch` action to it. Any person whose settings file already gives
  `alt-p` to another action gets that action back on its default, as every contested chord does.
- A layout's claim is consulted last in `claimed_by_a_mode`, so a picker, a delegate view, the
  scroller or a history search that is open keeps its keys. Tests pin this through `handle_key` and
  `handle_key_while_working`. Esc no longer stops a turn while the panes hold focus.
- The pane draws the rows in the order the module returns them, which is file order, and marks no
  archived row. [BOT-3](specs/bots.md#BOT-3) and [BOT-17](specs/bots.md#BOT-17) say the layout lists
  archived bots after the bots in use, most recently archived first. That order depends on each
  row's `retired` field in a file any process can write, so the slice does not apply it. The owner
  decided on 2026-10-27 to draw as the module returns and to record the gap. The design pull request
  settles the sentence or the pane changes.
- The text of the empty line, "No bots yet. Make one in the desktop app, and it is listed here.", and
  of the line about width are the slice's own wording, since BOT-17 fixes only what each says.

What slice 2a left for 2b is done. The layout carries a phase through `Screen::Bots`,
`Session::toggle_panel`, `Session::toggle_files` and the panel hint ask the layout, and the order of
the claim is pinned.

What later slices do. Slices 3a to 3c change nothing in the terminal. Slice 4 inherits a pane that lists rows
read once when the screen opens, so it has to reread them after a write. The `y` that agrees to an
archive and the line under the list for a failed write are slice 4. Tab between panes needs a second
pane. The session that Enter on a bot begins is slice 7. The `bots-none` line names the desktop app and
changes when the terminal can make a bot.

The owner cut slice 3 into three on 2026-11-02. Slice 3a adds `crates/agent/src/bots/operations.rs` to
the module `bots` of `bravebot-agent`, with `make`, `edit`, `set_model` and `ensure_home`, and the
bridge methods `bot.make`, `bot.edit`, `bot.set_model` and `bot.ensure_home`. The methods are in
`DENIED`, so the window cannot send them. Each of the first three is one read-modify-write under the
lock on `bots.json.lock`. A make composes the slug from the name ([BOT-2](specs/bots.md#BOT-2)),
refuses at 100 rows before it writes anything, writes the definition with `make_definition` and then
the row. An edit and a change of model rewrite the definition with `redefine` and then write the
row, and an edit of a bot with no definition writes the row only. Each stamps `updated`. A make and
an edit make the home folder with mode 0700 after the row is saved, and `ensure_home` makes the folder
the row names, for a conversation started or reopened in it. In the GUI, `bravebot:bots:write`,
`bravebot:bots:model` and the home folder made for `session.new` and `session.open` call the
bridge methods. `botFromForm`, `saveFormBot`, `ensureHome`, `botHome`, `slugFor` and `isBotModel` are
deleted, with the two test cases in `bot-model.test.mjs` that pinned `isBotModel` and `slugFor`. The
methods `bot.replace`, `bot.define`, `bot.redefine` and `bot.migrate` stay. The tests that pinned
the deleted TypeScript (`bot-model.test.mjs`, `bot-directory.test.mjs`, `bot-store.test.mjs` and
`session-directory.test.mjs`) are ported to the methods that replace it, and the shared fake
`ui/scripts/fake-bots.mjs` answers the four methods. `ui/scripts/bot-operations-bridge.test.mjs` runs
the GUI's calls through the real `bravebot-rpc`.

The evidence for slice 3a is in the pull request description: the scenarios run in the desktop app and the terminal, the test counts, the mutation results, the critiques and their answers, and what was not run. The plan does not repeat them.

Slice 3a differs from its text in these places:

- The plan's text puts the module code in `bots.rs`. It is in `bots/operations.rs`.
- `bravebot-agent` has no logging dependency and none was added, so the operations log nothing and
  report through their return values.
- `make` checks for an incognito run and for a state folder before it validates the form.
- `MakeRefused::Io` carries no path, so a failed write of a definition at make names
  `<state directory>/agents`.
- An edit that changes only the name is refused when the row's definition file was removed by hand,
  as the GUI's edit was.
- The GUI's `botHome` export in `ui/src/main/state.ts` is deleted, since nothing else uses it.
- `slug_for` numbers upward without the GUI's fallback to `Date.now()` after 999. With at most 100
  rows the fallback cannot be reached.
- The GUI's make, edit and change of model share one queue with `updateBots`, so a make or an edit
  cannot interleave with a read followed by `bot.replace`.
- A read made outside the lock, then a write of the whole list, remains for the row write of
  `migrateBot` and the counter writes. Slice 3c records a conversation, an archive length, a
  released session and a forgotten conversation under the lock. The owner left the other writes on
  `bot.replace` on 2026-11-06. A program that writes between that read and the replace can lose its
  row.
- [BOT-6](specs/bots.md#BOT-6) and [BOT-7](specs/bots.md#BOT-7) name tests for the part 3a builds,
  as [BOT-2](specs/bots.md#BOT-2) to [BOT-5](specs/bots.md#BOT-5) do under the owner's answer of
  2026-10-29, so they are not in `agents/unverified-clauses.txt`. The owner confirmed on 2026-11-05
  that this applies to BOT-6 and BOT-7, with each later slice adding its tests to the same clauses.
  The parts of their text for the terminal's form, archive, restore, delete and migrate stay
  untested until their slices.
- A bot made after the read dropped a row takes that row's slug and home folder, because the set of
  slugs taken holds only the rows kept. The new bot shares the memory files in that home folder
  with the dropped row's bot, as it does in the GUI today. The next write removes the dropped row, as
  [BOT-3](specs/bots.md#BOT-3) says.
- `edit`, `set_model` and `ensure_home` refuse a data folder that cannot be text, as `make` does.
- `bot.make` and `bot.edit` refuse a null for the name, the purpose or the avatar with `bad_request`.
- The GUI's model handler no longer checks the model in TypeScript. `isBotModel` is deleted, with its
  test case ("IPC model validation accepts opaque IDs and rejects malformed inputs"). The rule of the
  module decides, and the agent's refusal reaches the person with its reason.
- A refusal with the code `bad_request` no longer sets the persistent bots problem, since it is about
  what was asked for and not about the file.
- `make_home` makes nothing for a home that is not absolute for the current system, such as
  `C:\Users\x\bot-homes\dev` on Linux, and reports that the home was not made. `mkdirSync` in the
  GUI's `ensureHome` made a relative folder for it.
- `onBotModel` in `ui/src/renderer/App.tsx` shows a refusal of the model with `setProblem`, as
  `chooseModel` does, and reads the bots again. A later success from this handler clears the banner
  before the read, and a resolved null is shown the same way `chooseModel` shows a missing save. A
  refusal reaches it now because the module's rule decides the model.

Slice 3b adds `archive`, `restore` and `delete` to the module in `crates/agent/src/bots/operations.rs`.
Archive sets `retired` to the time of that archive, including when the bot is already archived.
Restore sets `retired` to zero, including when it is already zero. Each of those writes sets
`updated` to the time of the write and leaves every other field as it was. The definition file is
not opened for write. Delete checks the slug with `is_slug` before any path is composed from it. A
value that is not a slug removes nothing, and a slug that has no row removes nothing, including no
folder. When the row exists, delete removes the folder `<data folder>/bots/<slug>` and then the row,
and the lock is held across both. The row is written only after the folder removal succeeds, so a
list cannot observe a deletion that is then reverted. A list during the removal still sees the row.
A replace of that list after the delete puts the row back, and the folder is already gone. That is
the replace race for a read made outside the lock, then a write of the whole list. Slice 3c leaves
the writes that still go through `bot.replace` on that path.
A list taken before the delete starts ends the same way. One desktop process does not read during
its own delete, because that read shares the queue with the delete and runs after the delete returns.
A missing folder is success. If the folder
cannot be removed, the bots file is left as it was and the error names that folder. The removal is recursive and does not read the folder to decide what to
remove. The module's delete does
not check `retired` and does not know about turns. The bridge methods are `bot.archive`,
`bot.restore` and `bot.delete`. They are in `DENIED`, so the window cannot send them. Each request
carries `dataFolder` from the folder Electron names, as `bot.make` does. In the GUI,
`bravebot:bots:retire` is still `(slug, retired)`. `retired === true` calls archive and
`retired === false` calls restore. A `retired` that is not a boolean returns null and calls nothing.
`bravebot:bots:remove` is still `(slug)`. A delete while a turn of that bot is in progress in this
program is refused from the desktop's turn table, before the list is read, with the error text the
handler already uses, and on that refusal nothing is removed. That check stays in the desktop,
before any bridge delete. Archive, restore and delete run
on the same queue as `updateBots`. `retireBot` and `removeBot` no longer write the row through
`saveBot` or `updateBots`. `removeMemoryHistory` stays, because other callers use it, and the delete
path does not call it. `saveBot`, `updateBots`, `bot.replace`, `bot.define` and `bot.redefine` stay.
Slice 3b adds no dialog, and the renderer is unchanged. The terminal's confirmation of an archive or
a delete, and its offer of delete only for an archived bot, are not built.

Slice 3b is built on `bots/slice-3b-archive`. There is no pull request yet, so the evidence is the
tests on that branch. The plan does not repeat them.

What slice 3c inherits. The owner left the row write of `migrateBot` and the counter writes of
`noteBotMemory`, `keepingMemoryMark` and `noteBotNudged` on `bot.replace`, and left `bot.define`
and `bot.redefine` in place. Slice 5 still owns the migration's row write. Slice 6 still removes
the counters.

Slice 3c adds `record_conversation`, `record_archived`, `release_session` and `forget_conversation`
to the module in `crates/agent/src/bots/operations.rs`. Each is one read-modify-write under the lock
on `bots.json.lock`, on the same path as `archive`, and none of them rewrites the definition.
Recording a conversation sets `session` to the given identifier and adds the id and directory to
`conversations` when that pair is absent. A second note of the same pair still writes and stamps
`updated`. Recording an archive length raises `archived` only when the value is a finite integer in
the range the row parser accepts and is strictly greater than the stored length. The comparison is
under the lock. Releasing a session sets `session` to none and `archived` to zero and keeps the
conversations entry. A second release writes nothing. Forgetting a conversation removes that id and
directory from every row that lists the pair, and sets `session` to none and `archived` to zero on
a row whose `session` equals that id. Directory is not part of that comparison. A row that does not
change keeps its `updated`. When an operation changes nothing, the file is not written, and that answer does not clear a
failure already shown. When the row exists, a session that is not an identifier, or a directory
that is not absolute, is refused and the file is left as it was. A missing row is not an error.
The bridge
methods are `bot.record_conversation`, `bot.record_archived`, `bot.release_session` and
`bot.forget_conversation`. They are in `DENIED`, so the window cannot send them. Each request
carries `dataFolder` from the folder Electron names. In the GUI, `noteBotSession`,
`noteBotArchived`, `releaseBotSession` and `forgetBotConversation` call those methods through
`writeThroughBridge`. `releaseBotSession` still returns before any bridge call when the slug is not
a slug. The GUI does not read the list to compute the new row. `migrateBot`, `noteBotMemory`,
`keepingMemoryMark`, `noteBotNudged`, `saveBot`, `updateBots` and `withBot` stay, and so do
`bot.replace`, `bot.define` and `bot.redefine`. A search of `ui/src/main` still finds row writes
outside the bridge, in `saveBot`, `migrateBot` and the counter writes. The terminal does not record
a turn, does not continue a conversation from the layout, and has no delete of a conversation.

The owner decided on 2026-11-06 that the row write of `migrateBot` and the counter writes of
`noteBotMemory`, `keepingMemoryMark` and `noteBotNudged` stay on `bot.replace` until slice 5 and
slice 6, and that `bot.define` and `bot.redefine` stay. `bot.replace`, `updateBots`, `saveBot` and
`withBot` stay with them. The done-when that required `bot.replace` to be removed, and a search of
`ui/src/main` to find no write of the rows outside the bridge, is not the done-when of this slice.

Slice 3c is built on `bots/slice-3c-record`. There is no pull request, so the evidence is the tests
on that branch. The plan does not repeat them.

Before a pull request. The stack is 141 commits behind `origin/main` and conflicts with it in most
files slice 2b touches, so the whole stack is rebased and the gates run again. The rebase changes
these things beyond text: `MOVABLE` in `keybindings.rs` is already 15 and 14 upstream (the actions
`todos` on `alt-a` and `outputs` on `alt-l` exist there), so adding `panes` makes it 16 and 15 and every
document and test that says fourteen changes; `Start` has a `Prefilling` variant that `begin_session` and
`beginning_of` match beside `Start::Layout`; the view over what commands printed may claim keys, so
the order of the claim and the test that lists the open modes are checked again; and the array lengths
in `commands()` (56 here, 58 upstream) and in the `FLAGS` of `completion.rs` (53 here) change.

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
- Bots need a `~/.bravebot` that can be written. When the move of the rows out of `bravebot-ui.json`
  cannot happen (no state directory is named, a private run, a folder that cannot be made or written,
  a lock that fails or is held past the wait, a write that fails, or an old file that cannot be
  used), the read lists no rows and reports the cause. The interface does not list the old rows from
  `bravebot-ui.json`, since that would be a second reader of the key with its own reading rules, and
  the interface could not save a change to them. The rows stay in `bravebot-ui.json`, and the interface
  writes nothing to that file while they wait (`oldRowsWaiting` in the `bot.list` answer), so theme,
  layout, recents and fork lineage are not saved until the rows are moved or the old file is repaired
  or removed. That includes a damaged `bravebot-ui.json`, which the interface used to replace with
  defaults on its next write and now reports with its path. Decided 2026-10-26.

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
     terminal never hold different stores. Slices 3a to 3c replace the replace method with the
     module's operations. The module moves the rows already in that key to the new file when
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
     is refused while a turn runs, while a question waits, while lines are queued, while a `/loop`,
     `/goal` or `/after` is pending, in a session addressed to a definition from `--agent` or a record and in an `--incognito` run (and, from slice 4, with a form or a
     confirmation open), and ends nothing when refused. `--layout` is refused, naming both flags, with
     each flag and command [BOT-17](specs/bots.md#BOT-17) lists, in a test that reads the command
     line's own lists of one-shot flags and of commands that start no session, and `--layout default`
     is the same as no flag. In the bots layout `/branch`, `/resume`, `/manifest`, `/panel` and `/files` are
     refused, the layout draws no info panel, and each pane it draws is a pane for the selection of
     [MOUSE-1](specs/mouse.md#MOUSE-1), whose list gains them. The no-session phase draws the prompt box,
     which takes `/layout`, the key help and quitting and follows [INPUT-4](specs/terminal-input.md#INPUT-4):
     Esc on a half-typed line discards it and on an empty line does nothing, one Ctrl-C offers the way out
     and a second press leaves, and Esc at the top level of a pane returns focus to the prompt. The phase
     starts with focus in the bots list. The tests of INPUT-4's ladder run on this box. At 80 columns `--layout bots` draws the prompt box and the
     line about width with focus in the prompt box, `/layout default` and the quit ladder work, and at 100
     columns the list is drawn. A mode open over the session (the picker, a delegate view, the
     scroller, a history search) is tested against the bots layout's claim through both `handle_key`
     and `handle_key_while_working`, pinning whichever order 2b chooses, since 2a leaves the claim
     last and no test can see the order with one layout. A refused `/panel` or `/files`, and the
     panel chord where it is refused, leave the panel flag and the saved panel preference unchanged
     (`Session::toggle_panel` writes it with `bravebot_session::store::save_panel`), and the hint in
     `render/hint.rs` does not offer a chord the layout refuses. [BOT-17](specs/bots.md#BOT-17) does
     not say what the panel chord (`bindings().is_panel`, reached from `navigate` and `files_key`)
     does in the bots layout, so 2b decides it and writes it into BOT-17 in the same change. 2b adds
     the layout to `Phase` and to what `event_loop` returns, so `/layout` can end a session into the
     no-session phase of another layout. Verified with `cargo test -p bravebot-tui` and
     `make check-spec`.
3a. **The module makes, edits and sets the model of a bot and makes its home folder, and the GUI calls
   them through the bridge.** Built. The Status above describes it.
   - Add the writes to the module: the slug composition, the ceiling, and the operations of
     [BOT-7](specs/bots.md#BOT-7) that make a bot, edit it and set its model, each one read-modify-write
     under the lock, with the definition written first through `make_definition` and `redefine`,
     `updated` stamped on every write, and an edit of a bot with no definition writing the row only.
   - Add the making of a bot's home folder to the module, with mode 0700
     ([BOT-4](specs/bots.md#BOT-4)), made on a make, on a save from the form and when a conversation
     is started or reopened in it, and the first-run rules of that clause: the folders and the file
     are made by the first bot, and a write is refused where `home::writable()` names no folder.
   - Add the bridge methods `bot.make`, `bot.edit`, `bot.set_model` and `bot.ensure_home` and keep them
     out of the window's list ([BOT-6](specs/bots.md#BOT-6)). Switch the GUI's make, edit, set the model
     (`bravebot:bots:model`) and its making of the home folder to the bridge through its existing
     handlers, with no dialog, and delete the TypeScript they used (the slug composition, the form's
     validation, the calls that write a definition and the making of the home folder). The methods
     `bot.replace`, `bot.define`, `bot.redefine` and `bot.migrate` stay.
   - Done when, in the GUI, a bot made with a name and a purpose is listed after a restart, the
     definition file exists first with the purpose as its body, an edit rewrites the definition, a
     refused definition writes no row, and a bot made with a taken name has the slug `<slug>-2` and its
     own definition. The hundred and first bot is refused with every row kept, rows are written in the
     byte order of the slug, and a name or purpose with a null character and a model over its bound or
     with a line break are refused. An edit of a bot with no definition writes no definition, a failed
     write of the file is shown with its path, an edit remakes a home removed by hand, and every write
     stamps `updated`. With no `~/.bravebot` and no data folder, the first bot made makes both with mode
     0700, the file and the home folder. A make is refused, naming the path, while a `bravebot-ui.json`
     that does not parse exists and no `bots.json` does. A write interrupted before the rename leaves
     the file as it was, two writers at once lose no row, a lock held by a program that stopped is
     released, and a write that waits past the limit for a held lock is refused naming the lock file.
     The form refuses a name or purpose with a null character, the form's Save button and the details
     pane's save on leaving a field are unchanged, a search of `ui/src/main` finds no write of a bot
     made, edited or given a model outside the bridge, and a test pins that the methods are absent from
     the window's list. Verified with `cargo test -p bravebot-agent bots`,
     `cargo test -p bravebot-ui-bridge`, `cd ui && node --test scripts/*.test.mjs` and
     `make check-spec`.
3b. **The module archives, restores and deletes a bot, and the GUI calls them through the bridge.** Built. The Status above describes it.
   - Add the operations of [BOT-7](specs/bots.md#BOT-7) and [BOT-8](specs/bots.md#BOT-8) that archive
     a bot, restore it and delete it, with `retired` stamped on each archive and `updated` on each
     write. A delete removes the row and the folder `<data folder>/bots/<slug>`, composed from the slug
     the person gave and checked first, and no other folder.
   - Add the bridge methods for them and keep them out of the window's list
     ([BOT-6](specs/bots.md#BOT-6)). Switch the GUI's archive, restore and delete to the bridge through
     its existing handlers, with no dialog. The GUI keeps its refusal of a delete while a turn of the
     bot is in progress.
   - Done when, in the GUI, a delete leaves the definition, the sessions and the memory files and
     removes the row and the folder `<data folder>/bots/<slug>` and no other folder, archiving an
     archived bot stamps `retired` again, and a delete of a bot with a turn in progress in the GUI is
     refused.
3c. **The module records what happened to a row, and the GUI calls those operations through the bridge.** Built. The Status above describes it.
   - Add the operations of [BOT-7](specs/bots.md#BOT-7) and [BOT-11](specs/bots.md#BOT-11) that record
     a conversation, record the archive length, release a session and forget a deleted conversation,
     with the bridge methods for them, kept out of the window's list
     ([BOT-6](specs/bots.md#BOT-6)). Switch the GUI's `noteBotSession`, `noteBotArchived`,
     `releaseBotSession` and `forgetBotConversation` to the bridge through its existing handlers.
   - The owner decided on 2026-11-06 that the row write of `migrateBot` and the counter writes of
     `noteBotMemory`, `keepingMemoryMark` and `noteBotNudged` stay on `bot.replace` until slice 5 and
     slice 6, and that `bot.define` and `bot.redefine` stay. `bot.replace`, `updateBots`, `saveBot`
     and `withBot` stay. The done-when that required `bot.replace` to be removed, and a search of
     `ui/src/main` to find no write of the rows outside the bridge, is not the done-when of this
     slice. A search of `ui/src/main` still finds the row writes of `saveBot`, `migrateBot` and the
     counters.
   - Done when a turn that ends in the GUI records its conversation and the archive length through the
     bridge, a release sets `session` to none and `archived` to zero and keeps the conversation, a
     forgotten conversation is removed from every row that lists it, and each of those writes stamps
     `updated` and leaves the definition file unchanged.
4. **The terminal makes, edits, archives, restores and deletes a bot.**
   - Slice 2b's pane reads the rows once, when the phase with no session opens
     (`bots_pane::State::read`). Reread them after each write of this slice. Slice 2b leaves out the
     refusal of `/layout` with a form or a confirmation open, so add it here.
   - `State::press` in `bots_pane.rs` swallows every key that carries Ctrl or Alt, so the list never types
     one. Revisit that when this slice binds a chord in the list.
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
   The checks of `ui/scripts/drive-bot-turn.mjs` and `ui/scripts/drive-bot-memory.mjs` that assert the
   briefing file, the memory seeded in the checkout and the briefing drawn on a reopened session go
   with the code, since a live turn no longer reaches it. Three checks of `drive-bot-turn.mjs` fail in a live
   run today, and `drive-bot-memory.mjs` stops at its first check of the briefing file, so its later checks
   do not run.
7. **The terminal talks to a bot.**
   - Enter on a bot in slice 2b's pane says that starting a session as a bot comes in a later release,
     and begins none (`bots-pick-later` in `en-US.ftl`). Replace that line with the session this slice
     begins, and give `Phase::Session` the bots layout, which slice 2b leaves as the default.
   - Slice 2b draws the line about width only in the phase with no session. A session in the bots
     layout below 100 columns draws the chat alone with that line ([BOT-17](specs/bots.md#BOT-17)),
     which nothing draws yet. The in-session paths of the layout (`Layout::refuses`, the panes column
     of `RightColumn`, the refusals of `/panel` and `/files`, the hint and the claim) are reached only
     through tests in slice 2b, since no production path begins a session in the bots layout. Run them
     in a real terminal here.
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

Questions slice 1 raised for the owner:

- The order of the list the interface draws. The accepted conclusion of 2026-10-22 says the interface
  keeps sorting what it draws with `localeCompare`, and slice 1's "Done when" says a hand-edited file
  not in slug order is listed in file order. Both cannot hold for the interface. The interface does
  not sort when it draws, so after slice 1 its list is in the order the module returns, which is the
  byte order of the slug as written, and in a locale that sorts a pair of letters as one letter the
  list differs from before.
- Whether a clause that a later slice completes may name tests for the part an earlier slice
  builds. [BOT-2](specs/bots.md#BOT-2) to [BOT-5](specs/bots.md#BOT-5) do, and neither
  [specs/README.md](specs/README.md) nor
  [development/spec-enforced-development.md](development/spec-enforced-development.md) says. Answered
  yes by the owner on 2026-10-29: [BOT-17](specs/bots.md#BOT-17) and [BOT-18](specs/bots.md#BOT-18)
  name the tests of slice 2b, and the files it adds are governed by `docs/specs/bots.md`.

Questions slice 2b raised for the owner:

- The order and mark of archived rows in the bots list, described above under the differences of slice
  2b. The pane draws file order with no mark until the design says how archived rows are placed
  without an order that depends on the file's `retired` values.
- MCP server startup and its approval question happen at launch for `--layout bots`, as the previous
  deviation says. Whether BOT-9 or the startup changes is not decided.
- `/layout bots` then `/layout default` drops the record of the session left before the new one, so the
  exit line of the new session names only its own record and says nothing when it recorded nothing.
  The earlier session stays findable with `--resume`. Whether the exit line should name both is not
  decided.
- With focus in the list the prompt box shows its cursor although it drops typing. A cue for which
  of the two has focus is not specified.
- With focus in the list, which is where the phase starts, typed text is dropped until Esc or the
  panes chord returns focus to the prompt, as [BOT-17](specs/bots.md#BOT-17) says.

Questions slice 3a raised for the owner:

- What replaces the row write of `migrateBot` and the counter writes of `noteBotMemory`,
  `keepingMemoryMark` and `noteBotNudged` in `ui/src/main/bots.ts`. They go through `bot.replace`, the
  entry for slice 3c lists no operation for them, and slice 6 removes the counters. Answered by the
  owner on 2026-11-06: they stay on `bot.replace`. Slice 5 owns the migration's row write and slice 6
  removes the counters. `bot.replace`, `updateBots`, `saveBot` and `withBot` stay until those slices.
- Whether slice 3c removes `bot.define` and `bot.redefine`. The GUI has no caller of either after
  slice 3a. Four unit tests in `crates/ui-bridge/src/definitions.rs` cover them
  (`a_bot_is_answered_with_the_name_its_definition_was_given`,
  `an_edit_the_agent_refuses_or_cannot_find_is_a_bad_request`,
  `an_edit_rewrites_the_file_made_for_the_bot` and `a_refused_bot_writes_nothing`), and removing the
  methods removes those tests. Answered by the owner on 2026-11-06: `bot.define` and `bot.redefine`
  stay, with those tests.
- Whether the operations of the module should log a refused write or a home folder that could not be
  made. `bravebot-agent` has no logging dependency, so they log nothing and report through their
  return values, and logging needs a dependency added to the crate.
- An edit of a bot whose definition file was removed by hand is refused, including an edit that
  changes only the name, as the GUI's edit was. Whether such an edit should write the row is not
  decided.
- Whether a bot made after the read dropped a row should avoid that row's slug. Until that is decided
  it takes the slug and the home folder of the dropped row.
- What the desktop does when a make or an edit saves the row and the home folder cannot be made.
  [BOT-4](specs/bots.md#BOT-4) says in its paragraph on a bot's home folder that the module "reports
  whether it could be made", and its test
  `a_row_saved_while_its_home_cannot_be_made_is_kept_and_says_so` pins that the row is kept and
  `home_made` is false. The bridge answers `"homeMade": false` for `bot.make`, `bot.edit` and
  `bot.ensure_home`. The main process reads only `answer.bot` from a make or an edit, so the window shows
  nothing, and a conversation in that home is refused later with "could not make the folder for
  <name>". Seen with `ud/bot-homes` replaced by a regular file. The base ignored the result of
  `ensureHome` after a make in the same way. Whether the desktop should show that the home was not
  made when it saves is not decided.
- What the model picker's "Automatic (Default)" entry should do for a bot. It writes the model string
  `automatic-bravebot` (`DEFAULT_MODEL` in `crates/config/src/lib.rs`) and does not clear the model, so
  the row and the definition carry that string. Nothing in the renderer calls `writeBotModel(slug, null)`,
  and the preload types the model as a `string`. Clearing works through the bridge and the module with
  `model: null`. The base writes the string in the same way. Whether the entry should clear the model is
  not decided.
- `make check-ui-walkthrough` already fails on the unchanged base `16171073`, in a build with the
  builder's service credentials and in a build without them, and slice 3a neither causes nor fixes it,
  so the check is not a gate for this slice and whether to fix it is not decided.
- `chooseModel` and `saveBot` do not clear an error toast already on screen. A model refusal from the
  details pane can stay up after a successful header model change or a successful form save. The toast
  does not block input. Both handlers already left their own errors up, and this slice does not change
  them. To be filed, not fixed here.

What slices 3b and 3c inherit from the GUI run. A failed write is drawn in a toast that truncates the
path with an ellipsis on screen, and the full text is in the page. The toast text starts with Electron's
`Error invoking remote method 'bravebot:bots:write': BridgeError:` before the agent's reason, so 3b and
3c refusals will read the same way until the handlers change.
