# The interface

What the window shows and why it is shaped that way. The setup and build instructions
are in [setup](setup.md) and [development](development.md); the protocol underneath is in
[`phase-0-rpc-protocol.md`](phase-0-rpc-protocol.md).

- [What it looks like](#what-it-looks-like)
- [Turn notices, usage and audit](#turn-notices-usage-and-audit)
- [Forking, and export](#forking)
- [Bots](#bots)
- [The name in the menu bar](#the-name-in-the-menu-bar)
- [Keys](#keys) and [tooltips](#tooltips)
- [Layout](#layout)
- [What is remembered](#what-is-remembered)
- [Themes](#themes)

## What it looks like

Three columns, each side one resizable and foldable:

- **Sessions** — saved conversations under `~/.bravebot/sessions`, with bot histories
  accessed through the Bots tab, and a button to start a
  new one against any directory. A second tab beside it holds the **bots**: named, persistent
  agents with a purpose and a memory, each pinned to one checkout. See *Bots* below.
- **Transcript** — the conversation, with the turn's tool calls gathered into runs that
  fold away, and confined content shown as what it is rather than as text the model read.
  Five kinds of question are put here and answered here: a **write** (as a diff), a
  **command** to run (as the argv, plus the binary each name resolved to), whether the
  planner may **read what a command printed** (as the bytes in full), whether to **vouch**
  for a quarantined path, and a **series of questions** the planner wants to put to you —
  choices to pick from, or your own words. The turn blocks until one is answered.
  Window close and app shutdown refuse outstanding questions. For the last
  of the five that means *no answers at all* rather than a decline per question: a decline
  somebody made and a question that never reached them must not look alike.
- **Context** — an inspector with **Overview**, **Changes** and **Files** tabs.
  Overview summarises the plan, reads and confined material; Changes distinguishes
  pending decisions, approved writes and their actual execution outcomes. Files
  provides a lazily loaded tree and project-wide filename search, including folders
  not yet expanded. Text files can be previewed or opened in their default app.
  On narrow windows the inspector opens as a drawer.

The two side columns fold from controls in the transcript header, and their widths
and fold states survive a relaunch. Focus mode hides the sidebars; density can be
comfortable or compact.

Drafts and reading positions survive conversation switches and restarts. A running
conversation can continue in the background. Drafting during a run does not send
anything: **Queue message** explicitly queues a follow-up. Stop or an error pauses
the queue; **Resume queue** is required to continue it. Automatic bot-memory
maintenance reserves the session until it finishes.

Conversation actions include pin, archive and restore. **Find** searches the current
conversation. Code blocks offer copy and wrap controls; local file references can
open previews. **New activity** returns to the latest entries when new events arrive
while you are reading older ones. **Permissions** lists and revokes path and command
grants after the current turn stops.

The model control in the composer opens the conversation's model picker.
Search by name, provider, or reported capability (for example, `text` or `tools`),
then click a model or use the arrow keys and Enter. Escape
closes the picker. New conversations use the agent's configured default when available;
the default is also marked in the list. A choice applies to subsequent messages and is
remembered locally for that conversation across app restarts. Forks inherit the current
choice. The picker is disabled while a turn is running.

Available models come from the agent's configured backends, including OpenRouter.
Badges show the capabilities the app exposes: **Text** and **Tools**. The catalogue
may report other capabilities, but selecting such a model does not enable image,
audio or video input/output here. Context-window sizes are shown when available,
and recently chosen models appear near the top. Pricing is not supplied by the catalogue.

Refresh retries discovery if a provider is unavailable; the configured default remains
selectable. Choosing a model does not change the agent's global default.

### Turn notices, usage and audit

**Turn notices** appear beneath the prompt when the turn finishes. New or changed
groups start expanded; an identical group on the next turn starts collapsed. The
agent's messages are shown verbatim, without guessing their severity from the text.

The footer beneath a completed reply shows the model actually used and total tokens.
Expand it for exact total and output tokens and the number of tool-calling rounds.
Total usage includes every request in the turn; it is not context-window occupancy.
Cancelled turns have no final usage report. The live tokens-written counter remains
available while work is running.

**Audit** opens that turn's policy decisions in the context column (or the drawer in
a narrow window). **Policy blocked an action** indicates that a gate refused something;
it does not mean the whole task failed. Refusals appear first, with expandable recorded
evidence. **All captured events** reveals the ordered stream, including unfamiliar event
types. This inspector cannot grant permissions; approval cards remain in the transcript.
Close or press Escape inside the inspector to restore the previous context view and focus.

Notices, usage and captured audit events survive conversation switches in the current
window. The bridge does not yet return these per-turn details when reopening saved
conversations after a restart, so older replies show **Audit unavailable**. Session-wide
token totals are never presented as the usage of one reply. Failed captures and retention
limits are labelled explicitly: the UI retains whole records up to 1,000 events or
256 KiB per turn, and 2 MiB per session, releasing older turns' evidence as necessary.

### Forking

Hover a prompt you wrote and a fork appears beside it; right-clicking one offers **Fork From
Here** as well, and both do the same thing. That begins a session holding everything said
*before* that prompt, and puts the prompt itself in the composer to be edited and asked
differently — which is the usual reason to want one: a conversation that went somewhere
unhelpful, and a wish to go back rather than to start again from nothing.

The session it came from is not touched. A fork is a session of its own from the first moment,
with its own id, and — like a new session — it writes no record until it has something to say.
It inherits what the parent had answered about trusting the directory, and the commands vouched
for there, since it is the same person in the same checkout.

The cut is made by the agent, on its own conversation, in front of a message rather than in
front of a row on screen. A transcript is a projection of the exchange, so what crosses is
where the prompt falls among the prompts and what it said; the two are checked against each
other, and a fork that cannot be placed exactly is refused rather than made in roughly the
right place. `docs/phase-0-rpc-protocol.md` §7.1 has the argument in full.

A forked session says so in its header, with a link back to the session it came out of, which
opens it at the prompt the cut was made in front of. The session list marks a fork beside its
name. All three are the same mark — the control on a prompt, the banner, and the row — because
they are the same idea. None of it can live in the agent's own record — that has no field for a
parent, and it is rewritten after every turn — so lineage is stored in the `forks`
key beside `recents` in `bravebot-ui.json`. The main process writes it from the agent's answer rather than
from anything the window asked for.

An **Export** button sits in the conversation header, and File › Export offers the same three formats:
plain text, Markdown, or a PDF that keeps the window's own bubbles. What it writes by default
is the *conversation* — what was asked and what came back — and never the diffs, approval
cards or confined blobs. That is the same argument the per-entry Copy makes: those things are
evidence laid out to be read in place, and a document made out of one reads like a record of
the exchange without being one.

**Include Tool Calls**, above the formats in both menus, adds the steps between: the same
verb, target and outcome the transcript draws on a line, and nothing more. It is off to begin
with, because the usual reason to export a session is to show somebody the exchange, and it
is not remembered across launches — it is answered beside the format, by whoever knows who
the file is for. (It would belong in the save sheet itself, next to the filename; a native
save panel takes no controls of ours.) Either way the file ends with a line saying what it
left out, and that line says what the file actually carried.

The PDF is drawn by a second renderer entry point using the same React components the window
uses, rather than by assembling a string of HTML — so a reply's markdown is gated on the way
to paper by exactly what gates it on screen. See `src/main/export.ts` for why that is worth a
whole extra window.

### Bots

The left column has two lists. The **Sessions** tab is everything above; the **Bots** tab is the
people who have one.

A bot has a name, purpose, model choice, avatar, project and persistent memory.
Its overview lists its conversations and offers **Continue** and **New conversation**.
Starting a new conversation preserves earlier history; the bot's purpose and project
memory carry across conversations. Bot-associated conversations are accessed through
the bot rather than mixed into the ordinary conversation list.

The editor keeps the project fixed. **Duplicate into another project** creates a
separate bot. The memory editor offers readable and raw views, explicit saves,
reset with confirmation, and revision history for review and restoration. Reset
preserves history; deleting a bot removes its app-owned history and cached briefing.
See [file retention](file-access-security.md) for what stays in the project.

#### Archiving one

A bot leaves the list by being **archived**, from the same form that renames it. It drops into a
folded **Archived** section at the foot of the tab, and comes back from there with one click.

Nothing about it changes but a single field recording when it was put away. It keeps its slug, so
it keeps its memory file; it keeps its seed, so it keeps its face; it keeps its session, so
bringing it back resumes the same conversation rather than starting a new one. That is the whole
of the feature, and it is the reason archiving is a field rather than a second list somewhere.

It used to be **Forget**, one click and no confirmation, and the button's own tooltip explained
that the session and the memory file were left where they were. Both true, and neither much
comfort: what was dropped was the definition, and the definition is the only thing tying those
pieces together. A bot made again afterwards gets a fresh slug — so a different memory file — and
a fresh seed, so a different face. It was a different bot wearing the same name.

Taking a bot away for good still exists, as **Delete**, and it is offered only from an archived
row — the second deliberate step rather than the first one on the way past. It is the only act in
this window that cannot be taken back, and it is the only control that says so before it is
hovered: it carries the colour a deletion wears in a diff, where everything else in that column
earns its colour on the way past.

It also asks. The row turns, the checkout name is replaced by what the deletion costs, and the
answer is a different button in a different place, so nobody arrives at it by pressing twice. The
question is asked *in the row* rather than in a dialog, for the reason the agent's own questions
are asked in the transcript: a modal takes the thing being decided off the screen and replaces it
with a sentence about it, and here the sentence needs the bot's name still beside it.

Deletion removes the bot definition, cached briefing and app-owned memory revision
history. Saved conversations stay under `~/.bravebot`, and the project memory file
stays in the checkout. Deletion is refused while a bot conversation is running.

Archiving changes nothing in the sessions tab. An archived bot still owns its session — that is
what makes restoring it a restoration — so the conversation does not surface there while the bot
is away, which would make it a record openable twice by another route.

An archived row has no face, and that is deliberate twice over. A page gets a limited number of
WebGL contexts, as the avatar section below explains at some length, and an archive is exactly the
list that can grow to forty rows nobody is looking at. And a posture is a claim about what a bot
is doing: there is no word in that vocabulary for "not here", and a figure looking about beside a
Restore button would be saying something untrue.

#### The face

Each bot has a seed-based three-dimensional avatar rendered with three.js. Its stored
seed keeps the identity stable across renames. New avatars use versioned traits;
older seeds retain their original appearance. Avatar colours do not change with themes.

The avatars share a WebGL renderer rather than allocating a context per bot.
A flat canvas fallback uses the same traits when WebGL is unavailable. Motion is
clock-based and deterministic: quiet pauses, brief glances and irregular blinks,
with posture changes for running and failed turns and a single completion nod.
Reduced-motion preferences suppress continuous movement. See
`src/renderer/avatar/` and `src/renderer/components/BotAvatar.tsx`.

#### How a purpose reaches the model

The agent has no persona field, and it is not modified here. `Task` offers a prompt, some files and
a home directory; the system prompt belongs to the build, and `AGENTS.md` is global or per-checkout
rather than per-bot — writing one into somebody's repo would clobber theirs. Splicing a purpose
into the prompt is out too: every prompt is appended to the shared `~/.bravebot/history`, and a
charter poured into somebody's recall is not a feature.

So a bot is handed a **file to read**, and there are two of them:

- **The briefing**, `<userData>/bots/<slug>/ground.md`, composed by the main process from the bot's
  name, its purpose, and whatever its memory currently says. It goes to a turn as `dropped`, which
  is the read deliberately *not* confined to the workspace. It lives outside the checkout precisely
  so the planner cannot rewrite what defines it — the agent may write inside the workspace and
  nowhere else, and this is nowhere else.
- **The memory**, `<checkout>/.bravebot-ui/bots/<slug>.md`, which is inside the checkout because
  that is the only place the agent *can* write. That is the whole mechanism: the bot is told where
  its memory is and asked to keep it current, and it edits the file with its ordinary write tool.
  Nothing parses what a model said; the change the agent applied is the record. The folder ignores
  itself, so it never becomes a change nobody made. What that write is *gated* on is below, and is
  not what it looks like.

Only one file is attached, and the memory is copied *into* the briefing rather than sent beside it.
Every attached file becomes its own user message, and the agent's compaction keeps only the last
two of those verbatim — two attachments would mean the window a compaction preserves is spent
entirely on this app's own injections.

#### When it is said again

Compaction always cuts from the front, so a briefing at the top of a session is the first thing it
takes. The signal that it has is **not** the `compacting` phase: that is emitted before compaction
is attempted, so it also fires when there was nothing worth compacting, and then on every round of
a conversation that is over budget and cannot get under it. A bot re-grounded off that would be
re-grounded on every turn forever, which — given that each attachment stays in the conversation —
makes the problem worse.

What is watched instead is the size of the conversation's **archive**, which `turn.done` and
`session.open` both report. It only rises, it rises exactly once per compaction that actually
happened, and it is written into the record, so a session resumed in a new process knows it without
having watched it happen. A bot is re-grounded when that figure has gone up, and when its session
has just been opened.

#### When it is asked to write

None of the above makes a bot *remember*. It only puts the instruction in front of it, and the
instruction is in the briefing — so for as long as a session ran without being re-grounded, nothing
was asking. In practice a memory changed when somebody said "remember that", and not otherwise.

Two things close that, and neither of them attaches anything to an ordinary turn:

- **A compaction is answered with a turn of the app's own.** A rise in the archive is the one moment
  memory is unambiguously *for*, since it is the only thing that survived. Instead of waiting for
  the next prompt to carry the briefing, the main process sends a turn saying so, grounded. This is an additional model request and can incur provider usage and cost.
- **A bot that has stopped writing is grounded early.** A count on the bot rises each time one of
  its turns ends without its memory file's mtime moving, and at six the next turn carries the
  briefing whether the window thought it was due or not, with one extra paragraph asking whether
  anything since is worth keeping. It resets on the nudge as well as on a write, so a bot that
  ignores it gets six more turns of quiet rather than a briefing stapled to everything it is asked.

Both figures are main-written, like the session id and the archive watermark beside them: the editor does not control when a bot is reminded to remember.

Neither checks that the model wrote anything, because checking would mean parsing what it said, and
the rule this feature is built on is that the change the agent applied is the record. The mtime is
the only claim involved that nothing can be talked into.

A turn the app sends is also kept out of `~/.bravebot/history`, which is recall and is shared with
the terminal front-end. That is what `recall: false` on `turn.send` is for: what belongs under the
up-arrow is what somebody typed, and boilerplate this window wrote turning up in the terminal's
history would be this app spending somebody else's furniture. The same flag holds the prompt back
from naming the session, by the same argument.

A turn the app sent is **not drawn as one somebody typed**. It opens with a mark this app composes,
the transcript draws a line for it rather than a prompt bubble, and a reopened session recognises it
by that mark — the same judgement, for the same reason, that a handed-over file gets. The cost of
recognising a turn by its first line is that typing that line oneself gets the same treatment; the
mark is long and bracketed, so doing it is a thing somebody does on purpose.

#### What a memory write is actually gated on

Not on it being the memory. A memory write goes through the agent's ordinary write gate
(`Policy::write_needs_approval`), whose rule is about **integrity** rather than about which file it
is: *trusted data to a trusted path is written without a prompt*, because for data to be trusted the
turn must have observed nothing untrusted, and the destination only gains trust by it.

Both halves are true of a bot's memory in the ordinary case. The destination is trusted because this
app *names* the file — naming is what vouches for it — and a turn that has only read its own
checkout has seen nothing untrusted. So a bot exploring its project and writing down what it found
**does so without asking**, and the record is the `Update` line in the transcript and the row in the
Writes panel rather than a card somebody pressed.

The prompt appears exactly where it matters. A turn that *has* touched untrusted content — a fetched
page, a command's output, a quarantined file — is asked before it may write to the memory, because
that write would turn a trusted path untrusted. The gate is on prompt injection reaching the memory,
not on the memory changing.

The briefing handed to the model once promised more than that — that every edit would be shown as
a diff before it happened — and it was false, found by filming it and watching the Writes panel say
`APPLIED` with no card in the transcript. A false promise in a briefing is worse than none, since it
is the model telling somebody something the app does not do, so the briefing now says what is true:
the edit is on the record rather than in front of a card. Tightening the behaviour instead is not
available from here — there is no "always ask about this path" upstream, and adding one would be a
change to a repository this app does not modify.

Two more things are honestly imperfect and worth knowing:

- **The turn compaction happens in runs without the briefing.** It can fire on the first round.
  Nothing can inject mid-turn, so the summary the agent writes is what carries the gist through;
  the mitigation is keeping a purpose short enough that re-reading it is cheap.
- **Memory the model wrote is re-admitted as trusted context.** Naming a file vouches for it, so
  what the bot wrote about itself last week is trusted this week. Combined with the gate above, a
  bot that has only ever read its own checkout accumulates memory nobody was asked about — visible
  in the transcript every time, but not consented to each time.

#### What the window cannot do

The bridge protocol accepts `files` and `dropped` paths, both admitted as trusted
context. The main process strips those raw lists from renderer requests. User
attachments instead use native-picker grants bound to the session and revalidated
when sending; bot briefings are composed by the main process from a bot definition.

The preload does carry file contents for previews and memory editing. These are
bounded, explicit operations rather than unrestricted filesystem access, and previews
do not send contents to a model. See [security](security.md) and
[file access](file-access-security.md).

### The name in the menu bar

The bold word beside the Apple menu is the one part of the menu a template cannot set: AppKit
reads it from the running bundle's `CFBundleName` before any JavaScript runs, and
`app.setName` does not touch it — that renames `app.name`, which `app.getPath('userData')` is
built from, so using it would move `bravebot-ui.json` and orphan every remembered column.

Unpackaged, the running bundle is Electron's own, so `scripts/name-dev-app.mjs` renames it.
It runs from `npm run dev` and from `postinstall`, because an `npm install` restores the
original. If the menu bar ever says "Electron" again, `npm run name-dev-app` puts it back.

In a release there is no hack: `scripts/package.mjs` names the bundle `Brave Bot`, and AppKit
reads that. See [packaging](development.md#packaging).

## Keys

The menu is where these are written down, which is most of why it exists — before it there
was no way to find out that ⌘↵ sent a prompt.

| Key | What |
| --- | --- |
| `⌘N` | New session |
| `⇧⌘W` | Close the session — `⌘W` still closes the window |
| `⌘↵` | Send |
| `Enter` | Send from the message box |
| `Shift+Enter` | Insert a new line in the message box |
| `⌘.` | Cancel the running turn |
| `⌥⌘←` / `⌥⌘→` | Fold the session list / the context panel |
| right-click | A session row, or anything in the transcript |
| `↑` `↓` `⏎` `Esc` | In the theme picker: preview, keep, and put back what was there |
| `Esc` | Cancel, from the composer — or clear the session filter, from the filter box |

`Esc` is the one that is not in a menu. As an accelerator it would fire with no session open
and would fight every other use of the key, so it stays where it was: a convenience local to
whichever field has it, meaning the composer and the filter box above the session list.

The filter box itself has no key of its own, nor does the toggle beside it that groups the
sessions under the checkout each was started in. Both are always on screen under **New
session**, so there is nothing to reveal, and a ⌘F that only ever moved focus one field would
be a shortcut for something already in view.

Clicking a group's name folds it away and brings it back, and the **+** beside its count
starts a session in that checkout — the same thing **New session** does, minus the folder
picker, since the heading already knows which folder. A checkout that has since been deleted
or moved is refused by the bridge with `not_a_directory` rather than failing quietly. A live filter reaches into a
folded group regardless — a heading with nothing under it is the opposite of what somebody
who just typed a search asked for — and the fold is still there when the box is cleared.

Grouping and collapsed groups are remembered in the `view` key of `bravebot-ui.json`,
separate from the `layout` key holding column widths. The *folded* ones are
what is written down rather than the open ones, so a checkout started since last launch
arrives open instead of hidden behind a heading nobody has ever collapsed.

### Tooltips

A `title` is added where hovering says something the screen does not, and nowhere else. That
is three cases: a control with no room for a label (the column divider, whose double-click
reset is otherwise invisible), text the layout clipped (paths in the context panel, the
checkout in the header, a session's title in a narrow column), and a fold's verb — the name
stays put and `aria-expanded` carries the state, so *show* or *hide* goes in the tooltip.

A link a reply wrote carries its destination, for the reason a browser puts one in the
status bar: the link text was written by the model and need not describe where it goes.

The approval buttons deliberately have none. Their labels are already whole sentences —
`Don't run`, `Let the planner read it` — so a tooltip could only repeat them, and a popup
over an approval card covers the diff or the argv the decision rests on. The exception is
**Run and don't ask again**, whose tooltip lists the programs the vouch would cover, which
is the one thing its label cannot say.

**No key answers a question.** The five the agent can ask — a write, a command, whether the
planner may read output, whether to vouch, and a series of questions — are answered in the
transcript and nowhere else. An approval is a claim that somebody looked at the evidence,
and a keystroke can be typed from muscle memory into a window whose contents changed a frame
ago. The absence is structural: no command id names an approval, and the dispatch table in
`src/renderer/commands.ts` is not given the callbacks that answer.

## Layout

The parts worth naming, not every file:

```
crates/bravebot-bridge/     the Rust library and the bravebot-rpc binary
  src/lib.rs                the crate root, and the layering rules the tests assert
  src/bridge.rs             dispatch and session/turn lifecycle
  src/protocol.rs           the request and event types
  src/wire.rs               the JSON projections of the protocol
  src/store.rs              reading and writing the records under ~/.bravebot
  src/turn.rs               one turn, and everything that can block it
  src/fork.rs               cutting a conversation in front of a message
  src/running.rs            what is in flight, and what may answer it
  src/emit.rs               events delivered through the listener
  src/bin/bravebot-rpc.rs   read stdin, frame stdout, nothing else
  tests/                    the integration suites, including the refusal guarantees
crates/bravebot-ui-files/  descriptor-based helper for previews and bot memory
src/main/                   Electron main: one window, one child process, a narrow channel
  index.ts                  the window, and the allow-list of what the renderer may call
  bridge.ts                 the child process, and its lifetime
  menu.ts                   the application menu, built from the shared command list
  bots.ts                   the bots, and the two files each one speaks through
  state.ts                  bravebot-ui.json: one key replaced at a time, rest untouched
  files.ts                  listing, search, preview, opening and attachment grants
  project-files.ts          client for the secure-file helper
  experience.ts             drafts, scroll position, pins, archives and density
  memory.ts                 bot-memory editing and revision history
  recents.ts                the projects opened before, which only this side writes
  forks.ts                  which session came out of which
  export.ts                 text, Markdown and the second renderer that draws the PDF
  theme.ts                  the palettes on offer: the built-ins, plus JSON in themes/
src/preload/                the only thing the renderer can reach
  index.ts                  a handful of functions and one subscription
  export.ts                 the same, for the PDF renderer
src/renderer/               the React app
  App.tsx                   the three columns
  commands.ts               what a chosen menu item does — and what it deliberately cannot
  columns.ts                widths, folds and the clamps on both
  transcript.ts             gathering a turn's tool calls into runs
  theme.ts                  putting a palette on the window, as DOM rather than as a render
  export.tsx                the PDF entry point, using the components the window uses
  components/               ThemePicker, Sidebar, Transcript, FileTree,
                            Diff, TrustPrompt and BotAvatar are the load-bearing ones
  avatar/stage.ts           one WebGL context, however many avatars, and their clock
  avatar/figure.ts          what a friendly figure is made of, and what a seed varies
src/shared/                 types both sides agree on
  protocol.ts               the wire format, mirroring the crate's own
  state.ts                  bravebot-ui.json as a whole, each key delegated to its parser
  layout.ts view.ts         the column and list shapes, and their validators
  files.ts                  the lexical check: no `..`, nothing absolute
  commands.ts               the command list the menu and the renderer share
  bots.ts                   what a bot is, and which half of one a window may write
  recents.ts forks.ts       the two keys the renderer may read and never write
  export.ts                 the formats, and what each one leaves out
  theme.ts                  the palette format, ported from the agent's own theme.rs
scripts/                    the bridge build, the packager, the drivers and the demo
docs/                       the protocol design, this document, testing and the demo
```

### What is remembered

`bravebot-ui.json` under `app.getPath('userData')` holds application preferences:

| Key | What it holds |
| --- | --- |
| `layout` | The column widths and which side columns are folded |
| `view` | Whether the session list is grouped by checkout, and which headings are shut |
| `panels` | Which panels in the context column are turned **off** |
| `recents` | The projects opened before, newest first |
| `forks` | Which session came out of which |
| `bots` | The bots defined here: name, purpose, avatar seed, checkout, model, conversation IDs and memory bookkeeping |
| `theme` | Which palette the window is painted in, by name |

Additional state lives outside this file:

- `experience.json`: per-conversation drafts, scroll, pins, archives and bot associations;
  also density and recent model choices.
- `bots/<slug>/ground.md` and `memory-history.json`: cached briefing and memory revisions.
- Project `.bravebot-ui/bots/<slug>.md`: the bot's persistent memory.
- Renderer `localStorage`: per-conversation model choices, keyed by project and session ID.

See [file access and retention](file-access-security.md) for retention and permissions.

One file, but not one judgement: `src/shared/state.ts` decides nothing itself. It delegates each
key whole to the validator that already owned that shape — `parseLayout`, `parseView`,
`parsePanels`, `parseRecents`, `parseForks`, `parseBots` — so a hand-edited grouping flag still cannot cost
somebody their column widths. Every write goes through `src/main/state.ts`, which replaces exactly
one key and leaves the rest of the file as it found it, and what lands on disk is always the parsed
state rather than the object a caller passed.

The renderer reaches five of those keys, and only through a channel of its own per shape:
`layout`, `view`, `panels`, `theme` and `bots`. `recents` and `forks` are written by the main process alone, from a
native picker and from what the *agent* answered — the window can read them and has no way to
write a line into either.

Bot records split user preferences from main-process bookkeeping. The editor supplies
name, purpose and model; a native picker supplies the project at creation. The project
cannot change on an existing bot. Main-process code validates the avatar seed, creates
the slug, and records conversation IDs and memory bookkeeping from agent events.
Renderer input cannot replace those bookkeeping fields through the bot editor.

This replaces `layout.json`, `view.json`, `recents.json` and `forks.json`. Those are read once, on
the first launch after the change, so nobody loses their columns to a rename; they are then left
where they are and never read again.

### Themes

`View ▸ Theme…` opens a picker over the transcript. Moving the cursor repaints the window behind
it, Enter keeps the choice, Escape puts back what was there.

`brave` is the default and means what this window has always looked like: the macOS palette in
`styles.css`, following the system between light and dark. It is not a theme that happens to match
— under `brave` no theme is applied at all, which is why it costs nothing, why the native sidebar
blur survives it, and why an exported PDF stays white however dark the window is.

Twenty-one named schemes are compiled in beside it. A palette somebody writes goes in `themes/`,
beside `bravebot-ui.json` under `userData`; the picker prints the path, and the window follows the
file as it is edited rather than needing a relaunch. A file taking the name of a built-in replaces
it. A broken one is not a theme, and does not appear.

A palette names nine things — a ground, an ink, a quieter ink, and one each for finished, failed,
running, a confinement, the session's own voice and the person at the keyboard:

```json
{ "defs": { "ground": "#2e3440" },
  "background": "ground", "text": "#d8dee9", "muted": "#616e88",
  "ok": "#a3be8c", "fail": "#bf616a", "running": "#ebcb8b",
  "accent": "#b48ead", "note": "#d08770", "primary": "#88c0d0" }
```

Nine and not nineteen: `styles.css` mixes the window's other tokens from these in a
`:root[data-theme]` block, so writing a palette is choosing colours rather than computing a rule at
fourteen percent of your own ink. Any key left out, or set to `"none"`, is inherited — a palette
that only changes the accent is two lines long, and one that inherits its background keeps the
window blur that an opaque ground would cover.

The format is a port of `crates/tui/src/theme.rs` in the agent's repository, kept faithful so that
a palette written for one is recognisable in the other and `nord` means the same thing in both. It
is a port and not a link: nothing here reads anything the agent owns. The agent is a subprocess
this window drives, not something it is installed alongside, and a window that could not paint
itself until the terminal had been run once would be depending on something it was never promised.


## Agent 0.9 controls

**Agent settings** in the sidebar opens Connection, Hooks and Run settings. Connection
shows the bundled agent build, model services, certificate/proxy details and administrator
pins, with setup instructions for gateways, AWS Bedrock and Brave. Secrets are not shown.
Run settings selects a JSON model/connection override for this app run, lists loaded files
in precedence order, and provides Clear override. Existing turns retain their configuration;
future turns and model discovery use the selected override. Terminal-only preferences and
settings-file permission grants do not replace the desktop's approval controls.

Hooks are shared with the terminal client. Add a lifecycle event, a program and separate
arguments, optionally limiting a tool-completion hook to a tool name. Save applies changes
to future turns. Reload resolves external-edit conflicts; malformed or unsupported existing
files are reported rather than silently rewritten. Hook failures appear in turn notices.

**Watches** in the conversation toolbar lists up to eight live file watches, with their
remaining lifetime and Stop controls. Add a project file or ask the agent to watch one.
A change can start a model turn, so the dialog states that it may spend credits. Automatic
turns have their own transcript marker and retain the ordinary approval rules. Watches run
only while the conversation remains open, expire after seven days, and stop when the
conversation or app closes. Stopping an automatic turn also stops its originating watch.

Approval cards now show checker advice for quarantined content. A one-time read approval
shows the complete content and grants no standing trust. The checker has already received
the content at the backend; its verdict is advice, never permission. Write approvals show
processor remarks directly above the diff, labelled untrusted, with any omitted-line count.

The context line reports an unmeasured, measured or unavailable last-request size and
whether earlier messages were summarised. This is distinct from total billed usage.
Turn errors use stable categories with specific recovery guidance. Cancellation remains a
separate outcome, and request-attempt counts and HTTP status appear only when known.
