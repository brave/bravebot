# The interface

What the window shows and why it is shaped that way. The setup and build instructions
are in [setup](setup.md) and [development](development.md); the protocol underneath is in
[`phase-0-rpc-protocol.md`](phase-0-rpc-protocol.md).

- [What it looks like](#what-it-looks-like), [the header](#the-conversation-header) and [where notices go](#where-notices-go)
- [Turn notices, usage and audit](#turn-notices-usage-and-audit)
- [Forking, and export](#forking)
- [Undo and rewind](#undo-and-rewind)
- [Bots](#bots)
- [The name in the menu bar](#the-name-in-the-menu-bar)
- [Keys](#keys) and [tooltips](#tooltips)
- [Layout](#layout)
- [What is remembered](#what-is-remembered)
- [Appearance](#appearance)

## What it looks like

Three columns, each side one resizable and foldable:

- **Chats**: saved conversations under `~/.bravebot/sessions`, bots' conversations included,
  with buttons to start one in the project used last or in a folder you pick. A second tab beside
  it holds the **bots**: named, persistent agents with a purpose and a memory, whose conversations
  run in a project picked for each one or in none. Switching tabs opens the first item of the
  other list. See *Bots* below.
- **Transcript** — the conversation, with the turn's tool calls gathered into runs that
  fold away, and confined content shown as what it is rather than as text the model read.
  Nine kinds of question are put here and answered here: a **write** (as a diff), a
  **command** to run (as the argv, plus the binary each name resolved to), whether the
  planner may **read what a command printed** (as the bytes in full), whether to **vouch**
  for a quarantined path, whether to **fetch** an address (as the address, and the host it
  reaches on a line of its own), whether to **start a language server** (as the binary and
  the tree it would index, saying where that runs build tooling), whether to **run a plan**
  (as the task and every step), whether the model may **read a file that holds a
  credential** (as the file and what the scan found, without the value), and a **series of
  questions** the planner wants to put to you: choices to pick from, or your own words. The
  turn blocks until one is answered. Window close and app shutdown refuse outstanding
  questions. For the last of the nine that means *no answers at all* rather than a decline per question: a decline
  somebody made and a question that never reached them must not look alike.
- **Context**: an inspector with **Overview** and **Files** tabs, folded from the header's toggle.
  Overview summarises the plan, reads and confined material; Changes distinguishes
  pending decisions, approved writes and their actual execution outcomes. Files
  provides a lazily loaded tree and project-wide filename search, including folders
  not yet expanded. Text files can be previewed or opened in their default app.
  On narrow windows the inspector opens as a drawer.

The two side columns fold from controls at either end of the transcript header, and their
widths and fold states survive a relaunch. With the chat list folded, its toggle carries a
count of the background chats waiting on an answer or an approval, and opens the list. There
is no separate focus mode: folding both columns is it.

The sidebar sits flush on the window's ground. The transcript and the inspector are two raised
cards, inset by 8px from the window's edges and 8px from each other, each with a hairline border
and rounded corners; the strip of ground above the cards still drags the window. The gap between
them is the divider that resizes the inspector. With the chat list folded the transcript card gains
ground on its left too, so its corners never meet the edge. The three column heads are 44px tall
so they line up across the window, and the sidebar's head leaves room for the inset traffic
lights.

The **Agent** menu beside the model picker chooses how the next message is handled. **Agent** is
the default: an ordinary turn. **Plan** makes the next Send start a manifest run from the draft.
The agent plans the whole task before reading anything, shows you the plan, and runs it only if
you approve. The menu goes back to Agent once the run starts, so the next message is an ordinary
turn; a conversation never holds the mode. A run cannot take attached files, and a bot cannot
start one, so Plan is greyed with the reason in either case. A file named with `@` in the task
stays a word of the task and is not read into it, as with `/manifest` in the terminal: a plan is
fixed before anything is read, so the run reads that file, if it does, as one of its steps. The
run is saved as its own record and is not part of the conversation, so a later turn is not sent
what the run said.

A run's record is listed with the conversations and marked **Plan run**. Choosing one reads
it: the task, the goal, the plan, the steps that ran, and why it stopped if it did. It has
no message box, because a run has no conversation to continue. **New chat here** starts
a chat in the run's project. Runs started in the terminal are listed and read the same way.

Drafts and reading positions survive conversation switches and restarts. A running
conversation can continue in the background. Drafting during a run does not send
anything: **Queue message** explicitly queues a follow-up. Stop or an error pauses
the queue; **Resume queue** is required to continue it. Automatic bot-memory
maintenance reserves the session until it finishes.

Conversation actions include pin, archive and restore. Archiving asks for confirmation first; restoring does not. An archived conversation also offers **Delete conversation**, which asks first and then removes the record from disk; it cannot be undone. Code blocks offer copy and wrap
controls; local file references can open previews. **New activity** returns to the latest
entries when new events arrive while you are reading older ones. The header pills show only
what can be acted on (**Approval needed**, **Jump to latest**); whether a turn is running is
said by the working row and the composer, not by a status line.

### The conversation header

Beside the fold toggle, the header shows the bot's face for a bot's conversation, a chip naming
the project (the full path is its tooltip), and the conversation's title. A bot's conversation with
no project shows no chip, no vetting chip and no context panel. A **Vetting on** chip appears at
the right while auto-vetting is on. Three controls sit after it, before the context panel's
toggle:

- **Find** (`⌘F`) opens the find bar over the transcript and searches the current
  conversation. Enter steps to the next match, Shift+Enter to the previous, Esc closes it. The
  button shows as pressed while the bar is open.
- **Export** (a download icon) opens the export menu described under [Forking, and
  export](#forking). It is disabled until somebody has said something in the session.
- **More** (vertical dots) holds the two dialogs that are seldom wanted mid-conversation:
  **Permissions…** lists and revokes path and command grants after the current turn stops, and
  **File watches…** opens the [watches](#agent-09-controls) list.

A bot's own page has a header of its face and its name, with a search over its conversations and
the context panel's toggle.

### The composer's footer

Under the message box a strip names the project the conversation runs in, and its branch at the
right. Until the first message is sent the project is a menu: a chat offers the projects opened
before and **Select project…**, which opens the folder picker, and moving to one starts the chat
there with the draft carried over. A bot's conversation offers **No project**, the projects the
bot has worked in, and **Select project…**. Once something has been said the project is fixed and
shown without the menu. The branch is read from the checkout's `.git/HEAD`, and only shown: no git
command is run, so there is no branch menu and no pull request number yet.

Every icon-only control shows its name and, where it has one, its shortcut in a tooltip
(`⌘F` for Find, `⌘↩` for Send, `⌘.` for Stop); the accessible name is set separately and is never
the tooltip alone.

### Where notices go

What the window says about a session, as opposed to what the session said, does not stack up as
banners above the header. It goes where it is needed:

- **Fork, remembered trust and auto-vetting** are notes at the top of the transcript, each one
  sentence and at most one link (**View original**, **Manage**). They are not entries, so they are
  never exported. The vetting note is repeated for as long as the session is open by the header
  chip.
- **Backend not set up** docks onto the top edge of the composer as a tray of the composer's
  width, with **Setup help**, **Check again** and **Diagnostics**, and again on the welcome
  screen. Drafting stays possible; sending does not.
- **Problems** are error toasts in the corner of the conversation card: *Something went wrong*,
  the sentence saying what happened, and a dismiss button. They are announced with `role="alert"`.
- **Confirmations** (an export saved, a copy made) are success toasts in the same corner. They
  clear themselves after four seconds, name where a file went, and at most three show at once.
  A failure is never a confirmation.

The mode control in the composer, beside the model, says how much the session asks before it acts: **Ask** (every
write and command is put to you), **Accept edits** (writes go through, commands and writes that
would create a credential are still asked about) or **Plan** (nothing is written, commands are still
asked about). Ask is drawn plainly. The other two are tinted for as long as they hold. Every session
opens in Ask, including a resumed one and a fork. A change while a turn runs applies to the rest
of that turn. Bypassing every check is not offered here, because it is reached
only through the terminal's `--dangerously-skip-permissions` flag.

The sandbox control beside it says how much a program the agent runs is held to: **Standard**
(the machine can be read except credential locations, and writes stay in the session) or
**Strict** (a program reads only what its command names). Standard is drawn plainly and Strict is
tinted for as long as it holds. The menu has no way to turn the sandbox off: that is reached only
through the terminal's `--sandbox off` flag or `/sandbox off`. A change applies from the next turn,
and a turn already running keeps the mode it began with. Every session opens in the mode the
settings and the managed file give, never in another session's choice.

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

A forked session says so in a note at the top of its transcript, with a **View original** link back
to the session it came out of, which opens it at the prompt the cut was made in front of. The
session list marks a fork beside its name. All three are the same mark — the control on a prompt,
the note, and the row — because they are the same idea. None of it can live in the agent's own record — that has no field for a
parent, and it is rewritten after every turn — so lineage is stored in the `forks`
key beside `recents` in `bravebot-ui.json`. The main process writes it from the agent's answer rather than
from anything the window asked for.

### Undo and rewind

A turn that wrote files can be put back, files and conversation together, as the terminal's
`/undo` and `/rewind` do. There are three ways in: **Undo turn** on the latest reply's footer,
**Rewind to Before This…** on the right-click of a prompt whose turn can still be reached, and
Chat › **Undo Last Turn…**, which has no shortcut because `⌘Z` belongs to the text being edited.
The chat keeps the five most recent points, as the agent lists them at the end of each turn.
A fork starts with none.

Each one asks first. The dialog names every file it puts back, says in plain words what the
backups do not cover (a command the turn ran, a hook, a language server), and for a bot's
conversation says that the bot's memory is not rolled back. Afterwards the transcript is drawn
again from what the agent now holds, the prompt that began the earliest undone turn goes back in
the composer, and a queued message waits rather than overtaking it. A file that could not be put
back is named in the problem toast. Every entry point is greyed while a turn runs.

The **Export** button in the conversation header, and File › Export, offer the same three formats:
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

The left column has two lists. The **Chats** tab is everything above; the **Bots** tab is the
people who have them. A bot's row shows its face, its name and its purpose; **+** opens
**Create bot**, which asks for a face, a name, a purpose and a model.

A bot has a name, purpose, model choice, avatar, a home folder and persistent memory. The home
folder is made by the app under its own data directory, beside the briefing and never inside it.
Opening a bot shows its page. A bot nobody has talked to yet shows its face, that it is ready, and
a message box; one with conversations lists its recent ones above the box. Sending from the box
starts a new conversation.
The box's footer picks where the conversation runs: **No project** runs it in the home folder, and
a project runs it in that folder. Only the projects the bot has already worked in, and one picked
with **Select project…**, are offered, because those are the only folders the main process lets a
bot's turn run in. A conversation with no project has no context panel.

The right column of a bot's page holds its details: the face, with a button for a new one, the
name and the purpose, which are saved when a field is left, and the memory. A bot keeps one memory per folder it works in, so a
**Memory for** menu picks which folder's memory is shown once there is more than one. The column shows
the first lines of the memory as the bot wrote it. **Edit memory** opens a dialog with the whole text
for editing and an explicit save. The dialog's **History** lists earlier versions for review and
restoration, one history per folder, and its **Reset…** asks before it empties the memory. Reset
preserves history; deleting a bot removes
its app-owned histories and cached briefing. See [file retention](file-access-security.md) for what
stays in the project.

Bots' conversations are listed in the Chats tab too, with the bot's face in place of the folder
mark.

#### Archiving one

A bot leaves the list by being **archived**, from the menu on its details, after a confirmation
dialog that says where it goes and how it comes back. It drops into a folded **Archived** section
at the foot of the tab, and comes back from there with one click and no confirmation.

Nothing about it changes but a single field recording when it was put away. It keeps its slug, so
it keeps its memory file; it keeps its seed, so it keeps its face; it keeps its session, so
bringing it back resumes the same conversation rather than starting a new one. That is the whole
of the feature, and it is the reason archiving is a field rather than a second list somewhere.

It used to be **Forget**, one click and no confirmation, and the button's own tooltip explained
that the session and the memory file were left where they were. Both true, and neither much
comfort: what was dropped was the definition, and the definition is the only thing tying those
pieces together. A bot made again afterwards gets a fresh slug — so a different memory file — and
a fresh seed, so a different face. It was a different bot wearing the same name.

An archived row has one control, an actions menu (⋯) shown on hover like a conversation's, with **Restore bot**
and **Delete bot**. Restore brings the bot back with one click.

Taking a bot away for good still exists, as **Delete bot**, and it is offered only from an
archived row — the second deliberate step rather than the first one on the way past. It is the
only act in this window that cannot be taken back, and it is the only menu item that says so before
it is hovered: it carries the colour a deletion wears in a diff, where everything else in that
column earns its colour on the way past.

It also asks, in a dialog that names the bot and says what the deletion costs, with Cancel and a
red **Delete**. The dialog is the same one a conversation's deletion uses.

Deletion removes the bot definition, cached briefing and app-owned memory revision
histories. Saved conversations stay under `~/.bravebot`, the project memory files stay in
their checkouts, and the home folder stays where it is. Deletion is refused while a bot conversation is running.

Archiving changes nothing in Chats. A bot's conversations are listed there whether the bot is
archived or not, and one opened from there is still that bot's conversation, so restoring the bot
continues the same record.

An archived row is drawn like any other bot row, with the actions menu shown on hover or focus.
Its face is a still picture (`BotFace`) instead of the animated one, because an archive is the list
that can grow to forty rows nobody is looking at.

#### The face

Each bot has a pixel avatar generated from its stored seed, so it stays the same across renames
and windows. The silhouette is a mirrored Space Invaders figure: a body, a crown, arms and legs
picked from small sets, with about a third of bots breaking the mirror by a few cells. Some bodies
are legless heads. The fill is a gradient between two or three paints at least 60 degrees apart in
hue, walked through the paints between them, one colour per cell. The eyes are two cells each, a
white cell and a pupil. Avatar colours do not change with appearance. A face has no background or
rounded frame: it is drawn on the page and cropped to its own cells, so it fills whatever size it
is given.

Any stored seed draws a face, so every bot has one, new or old.

Each face is an SVG with one rect per cell. One shared clock moves the animated ones by whole
cells, writing attributes rather than re-rendering: idle glances swap the pupils, blinks close
the eyes to a line, a running turn drops the eyes a row, a failed turn looks away for a few
seconds, and finishing a turn nods the figure once. Reduced-motion preferences stop the clock, and
a face then changes only when what its bot is doing changes. See `src/renderer/avatar/` and
`src/renderer/components/BotAvatar.tsx`.

#### How a purpose reaches the model

The agent has no persona field, and it is not modified here. `Task` offers a prompt, some files and
a home directory; the system prompt belongs to the build, and `AGENTS.md` is global or per-checkout
rather than per-bot — writing one into somebody's repo would clobber theirs. Splicing a purpose
into the prompt is out too: every prompt is appended to the shared `~/.bravebot/history`, and a
charter poured into somebody's recall is not a feature.

So a bot is handed a **file to read**, and there are two of them:

- **The briefing**, `<userData>/bots/<slug>/ground.md`, composed by the main process from the bot's
  name and its purpose. It goes to a turn as `dropped`, which is the read deliberately *not*
  confined to the workspace. It lives outside the checkout precisely so the planner cannot rewrite
  what defines it — the agent may write inside the workspace and nowhere else, and this is nowhere
  else.
- **The memory**, `<folder>/.bravebot-ui/bots/<slug>.md`, inside the folder the conversation runs
  in, because that is the only place the agent *can* write. There is one in each folder the bot
  works in: its home folder for conversations with no project, and each project. That is the whole mechanism: the bot is told where
  its memory is and asked to keep it current, and it edits the file with its ordinary write tool.
  Nothing parses what a model said; the change the agent applied is the record. The folder ignores
  itself, so it never becomes a change nobody made. What that write is *gated* on is below, and is
  not what it looks like.

**Only the briefing is handed over, and the memory is neither attached nor quoted in it.** `files`
and `dropped` are both admitted as *trusted* context, which is the agent recording that a person
named the path in their own line, so the only path the app may name is one whose every byte the app
wrote. The briefing is one: the name and the purpose were typed into this window, and the memory's
path is a string composed from a slug. The memory is not one: its words are the model's own. The
briefing therefore says where the memory is and asks the bot to read it first, and what that read
comes back as is the trust map's answer about that path, the same as for any other file in the
checkout. A memory that an earlier write left untrusted comes back quarantined.

A memory the grounding walk has just created is the one case the briefing does not ask for: it
holds a template and nothing more, so the briefing says as much rather than spending a call on it.
A memory that is there is never replaced, whatever is in it: one whose bytes are not text now
costs the bot a paragraph, where under the old arrangement it would have failed the turn, so
there is nothing to be gained by a seed that overwrites what it cannot read.

Reading it costs one tool call at the top of a grounded turn, and buys back the attachment it used
to spend: every attached file becomes its own user message, and the agent's compaction keeps only
the last two of those verbatim, so an injection of the app's own is expensive in the one window
that matters.

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

Both halves are true of a bot's memory in the ordinary case. The destination is trusted because the
person vouched for the checkout it is in, and a turn that has only read its own checkout has seen
nothing untrusted. So a bot exploring its project and writing down what it found **does so without
asking**, and the record is the `Update` line in the transcript and the row in the Writes panel
rather than a card somebody pressed.

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
- **A bot whose memory was poisoned loses it rather than reading it back.** The memory arrives under
  whatever the trust map says about its path, so a write that turned the path untrusted means the
  next read is quarantined and the bot carries on without what it knew. That is the gate doing its
  job, and it is the honest failure: the app once vouched for that path on every grounded turn,
  which undid the prompt the write had asked for and let a fetched page's bytes back in as trusted
  context. Recovering the memory is a person's decision, taken by vouching for the path.
- **A memory in a checkout nobody vouched for is quarantined too.** Declining the project at
  startup means declining its files, and the bot's memory is one of them.

#### What the window cannot do

The bridge protocol accepts `files` and `dropped` paths, both admitted as trusted
context. The main process strips those raw lists from renderer requests. A person's
files reach `files` two ways: native-picker grants bound to the session, which the
main process revalidates as text files at send, and names written with `@` in the
prompt, which the bridge reads back out of the prompt at `turn.send` and surveys with
the agent's own confined read before the turn starts.
Bot briefings are composed by the main process from a bot definition, and are the only
`dropped` path a bot contributes.

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
It runs from `pnpm run dev` and from `postinstall`, because an `pnpm install` restores the
original. If the menu bar ever says "Electron" again, `pnpm run name-dev-app` puts it back.

In a release there is no hack: `scripts/package.mjs` names the bundle `Brave Bot`, and AppKit
reads that. See [packaging](development.md#packaging).

## Keys

The menu is where these are written down, which is most of why it exists — before it there
was no way to find out that ⌘↵ sent a prompt. Every accelerator below is declared once in
`src/shared/commands.ts`; the native menu (`src/main/menu.ts`) builds its items from that list,
and an item is greyed when its `requires` tag is not met. (On Windows and Linux, `⌘` is `Ctrl`.)

| Key | Menu item | What |
| --- | --- | --- |
| `⌘N` | File › New Chat | Start a chat in the project used last, or pick a folder when there is none |
| `⇧⌘W` | File › Close Chat | Close the chat, `⌘W` still closes the window |
| `⌘F` | View › Find in Conversation | Open the find bar and focus its field; needs a session |
| `⌘L` | View › Focus Composer | Move focus to the message box; needs a session |
| `⌘↩` | Chat › Send | Send the draft; greyed while a turn runs or the draft is empty |
| `⌘.` | Chat › Cancel Turn | Cancel the running turn; greyed when nothing is running |
| `⇧⌘M` | Chat › Cycle Permission Mode | Ask, then Accept edits, then Plan, then Ask again; needs a session |
| | Chat › Undo Last Turn… | [Undo](#undo-and-rewind) the latest turn; greyed while a turn runs or with nothing to undo |
| `⌥⌘←` / `⌥⌘→` | View › Hide/Show Chat List / Context Panel | Fold the chat list / the context panel |
| `Enter` | | In the message box: send, or queue the message while a turn is running. With the file list open on a half-typed `@` name, complete it instead |
| `Tab` | | In the message box, on an `@` name: complete it to the highlighted file or directory |
| `↑` / `↓` | | In the message box, with the file list open: move through it |
| `Shift+Enter` | | In the message box: insert a new line |
| `Esc` | | See below |
| right-click | | A chat row, or anything in the transcript |

The round button at the foot of the composer is **Send** (`⌘↩` in its tooltip) and, for as long as
a reply is generating, **Stop** (`⌘.`). The working row has no stop control of its own; `Esc` in
the composer also stops the turn, as described below.

Typing `@` as the start of the last word in the message box opens a list of what is in the
chat's folder, the terminal's list ([NAME-4](../../docs/specs/naming-files.md#NAME-4)):
directories first and then files, at most 40, narrowed by what follows the `@`, without
version-control, build and dependency directories. A slash lists that directory. Accepting a
file replaces only the last word and adds a space, which closes the list; accepting a
directory keeps it open on that directory. `Enter` sends a message whose last word already
names a file, and completes one that is half typed. When the message is sent, each `@` name
goes as a file the agent reads as trusted context and draws a **Read** row above the message,
as a picked file does; a name ending in `/` names nothing. A name that is not a text file
inside the project, a directory written without its slash included, stops the send, says which
name it was above the box, and leaves the message there to fix. The folder is the one the chat runs in: its project, or for
a bot's conversation with no project, the bot's home folder. A bot's page before its first
conversation has no folder yet, so it offers no list; a name typed there is still read and
checked against the folder the conversation opens in, and a refused one waits in that
conversation's queue with the reason.

`Esc` is the one that is not in a menu. As an accelerator it would fire with no session open
and would fight every other use of the key, so it stays where it was: a convenience local to
whichever surface has it, and each one gives way to the one above it:

- In the composer, with the `@` file list open: close the list and keep the text. Typing on
  into another name opens it again.
- In the composer, with a turn running: cancel it. If the find bar or a menu is open, `Esc`
  closes that instead and the turn keeps running.
- In the find bar: close it.
- In an open menu, the model picker or a dialog: close it.
- In Settings: go back to the chat view.
- In the audit inspector: close it and restore the previous context view.
- In the search box above the chat list: clear the search.

The chat list's head is one line: the search box, the filter menu that groups the chats under the
checkout each was started in and shows archived ones, a folder button that opens the picker and
starts a chat in the folder chosen, and **+**, which starts a chat in the project used last. None
has a key of its own beyond `⌘N` for **+**; `⌘F` belongs to the conversation.

Each chat row has three lines: the project (or **No project** for a bot's conversation in its home
folder) with how long ago it was active, the title, and the branch it was started on. The leading
mark is the bot's face for a bot's conversation and a folder otherwise, or the working or waiting
mark while the chat asks something of you.

Clicking a group's name folds it away and brings it back, and the **+** beside its count
starts a chat in that checkout, without the folder picker, since the heading already knows which
folder. A checkout that has since been deleted
or moved is refused by the bridge with `not_a_directory` rather than failing quietly. A live filter reaches into a
folded group regardless — a heading with nothing under it is the opposite of what somebody
who just typed a search asked for — and the fold is still there when the box is cleared.

The list draws its first 100 conversations, pinned ones and then the newest, and the archive its
newest 100, each with a **Show more** row at the foot that draws the next 100 and moves focus to
the first of them. The open conversation, and any that is working or waiting on the reader, is
drawn wherever it falls, after the page if that is where it is. The filter searches every session,
and the cap applies to what it found, so a search still finds a conversation from months ago. With
grouping on, every checkout keeps its heading, which counts all of its conversations. Each group
first draws the ones it has among the list's first 100, so grouping mounts no more rows than the
flat list, and has a **Show more** row of its own for the rest.

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

**No key answers a question.** The nine the agent can ask (a write, a command, whether the
planner may read output, whether to vouch, whether to fetch, whether to start a language
server, whether to run a plan, whether to send a file holding a credential, and a series of
questions) are answered in the transcript and nowhere else. An approval is a claim that somebody looked at
the evidence, and a keystroke can be typed from muscle memory into a window whose contents
changed a frame ago. The absence is structural: no command id names an approval, and the dispatch table in
`src/renderer/commands.ts` is not given the callbacks that answer.

## Layout

The parts worth naming, not every file:

```
crates/ui-bridge/     the Rust library and the bravebot-rpc binary
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
crates/ui-files/  descriptor-based helper for previews and bot memory
src/main/                   Electron main: one window, one child process, a narrow channel
  index.ts                  the window, and the allow-list of what the renderer may call
  bridge.ts                 the child process, and its lifetime
  menu.ts                   the application menu, built from the shared command list
  bots.ts                   the bots, and the two files each one speaks through
  state.ts                  bravebot-ui.json: one key replaced at a time, rest untouched
  files.ts                  listing, search, preview, opening and attachment grants
  project-files.ts          client for the secure-file helper
  experience.ts             drafts, scroll position, pins and archives
  memory.ts                 bot-memory editing and revision history
  recents.ts                the projects opened before, which only this side writes
  forks.ts                  which session came out of which
  export.ts                 text, Markdown and the second renderer that draws the PDF
  theme.ts                  applying System / Light / Dark to nativeTheme
src/preload/                the only thing the renderer can reach
  index.ts                  a handful of functions and one subscription
  export.ts                 the same, for the PDF renderer
src/renderer/               the React app
  App.tsx                   the three columns
  commands.ts               what a chosen menu item does — and what it deliberately cannot
  columns.ts                widths, folds and the clamps on both
  transcript.ts             gathering a turn's tool calls into runs
  styles.css styles/        the stylesheet modules and the token layer; see development.md
  highlight.ts              the syntax-colour grammars, shared by replies and diffs
  toasts.ts                 the store behind the confirmation toasts
  theme.ts                  putting System / Light / Dark on <html> data-theme
  export.tsx                the PDF entry point, using the components the window uses
  components/               SettingsView, Sidebar, Transcript, BotView, FileTree,
                            Diff, TrustPrompt and BotAvatar are the load-bearing ones
  avatar/pixels.ts          the pixel figure a seed describes, and where its eyes go per look
  avatar/clock.ts           one clock that blinks and moves every animated face
src/shared/                 types both sides agree on
  protocol.ts               the wire format, mirroring the crate's own
  state.ts                  bravebot-ui.json as a whole, each key delegated to its parser
  layout.ts view.ts         the column and list shapes, and their validators
  files.ts                  the lexical check: no `..`, nothing absolute
  commands.ts               the command list the menu and the renderer share
  bots.ts                   what a bot is, and which half of one a window may write
  recents.ts forks.ts       the two keys the renderer may read and never write
  export.ts                 the formats, and what each one leaves out
  theme.ts                  Appearance names and parseAppearance
scripts/                    the bridge build, the packager, the drivers and the demo
build/                      the app icon, and the drawing it is made from
docs/                       the protocol design, this document, testing and the demo
```

### What is remembered

`bravebot-ui.json` under `app.getPath('userData')` holds application preferences:

| Key | What it holds |
| --- | --- |
| `layout` | The column widths and which side columns are folded |
| `view` | Whether the chat list is grouped by checkout, which headings are shut, and which tab is open |
| `recents` | The projects opened before, newest first |
| `forks` | Which session came out of which |
| `bots` | The bots defined here: name, purpose, avatar seed, home folder, model, each conversation ID with the folder it ran in, and memory bookkeeping |
| `theme` | Appearance: `system`, `light`, or `dark` |

Additional state lives outside this file:

- `experience.json`: per-conversation drafts, scroll, pins, archives and bot associations;
  also recent model choices.
- `bots/<slug>/ground.md` and `memory-history-<folder hash>.json`: cached briefing and memory
  revisions, one history per folder.
- `bot-homes/<slug>/`: a bot's home folder, where its conversations with no project run.
- Project `.bravebot-ui/bots/<slug>.md`: the bot's persistent memory.
- Renderer `localStorage`: per-conversation model choices, keyed by project and session ID.

See [file access and retention](file-access-security.md) for retention and permissions.

One file, but not one judgement: `src/shared/state.ts` decides nothing itself. It delegates each
key whole to the validator that already owned that shape — `parseLayout`, `parseView`,
`parseRecents`, `parseForks`, `parseBots`, so a hand-edited grouping flag still cannot cost
somebody their column widths. Every write goes through `src/main/state.ts`, which replaces exactly
one key and leaves the rest of the file as it found it, and what lands on disk is always the parsed
state rather than the object a caller passed.

The renderer reaches four of those keys, and only through a channel of its own per shape:
`layout`, `view`, `theme` and `bots`. `recents` and `forks` are written by the main process alone, from a
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

### Appearance

**Settings ▸ General** holds the theme, chosen from a dropdown: System, Light or Dark. The choice
applies and is kept in `bravebot-ui.json` as soon as it is made. `View ▸ Appearance…` opens that
page.

System follows the OS (`prefers-color-scheme`). Light and Dark set `data-theme` on
`<html>` so Leo (Nala) tokens stay put. The PDF export window is pinned with
`data-theme="light"` on `export.html`.

Legacy palette names (`brave`, `nord`, and the rest) stored from earlier builds all
resolve to System.


## Agent 0.9 controls

**Settings** at the foot of the sidebar replaces the chat view with the settings: pages on the
left (**General** and **Agent settings**) and **Back to
BraveBot**, which `Esc` also does. The chat view stays as it was underneath. The setup button on
the backend notice opens Agent settings.

**Agent settings** shows Connection, Hooks and Run settings, one after another. Connection
shows the bundled agent build, model services, certificate/proxy details and administrator
pins, with setup instructions for gateways, AWS Bedrock and Brave. Secrets are not shown.
Run settings selects a JSON model/connection override for this app run, lists loaded files
in precedence order, and provides Clear override. Existing turns retain their configuration;
future turns and model discovery use the selected override. Terminal-only preferences are not
read. Permission rules in settings files are: see *Permission rules* below.

Hooks are shared with the terminal client. Add a lifecycle event, a program and separate
arguments, optionally limiting a tool-completion hook to a tool name. Save applies changes
to future turns. Reload resolves external-edit conflicts; malformed or unsupported existing
files are reported rather than silently rewritten. Hook failures appear in turn notices. Leaving
the page with unsaved hook changes asks first.

### Permission rules

The `permissions` block of the settings files governs a conversation, as it does in the
terminal. The rules are read when a conversation opens and kept until it closes, so a file
edited afterwards changes the next conversation.

- A `deny` rule refuses before anything is asked. No card appears, and the agent is told a
  rule refused.
- An `ask` rule puts a card that would not otherwise have appeared.
- An `allow` rule in your own `~/.bravebot/settings.json` answers a card for you, so none
  appears. It does not make what a command prints trusted.
- An `allow` rule in a project's settings file is not in force. Only your own file may write
  one. The terminal asks whether to grant a project's rules. This app has no such question
  yet, so it grants none and you are still asked.
- A settings file chosen under **Agent settings** counts as your own where it is outside the
  project, so an `allow` rule in it is in force. One inside the project is the project's
  file, and its `allow` rule is not.
- A directory named in `additionalDirectories` is not opened.
- Rules are not applied to a plan run. A plan card names the `deny` and `ask` rules the
  conversation holds, so you can check the steps against them.

A folded note at the top of the conversation says what a settings file wrote that is not in
force: an entry that is not a rule, a project's `allow` rule, or a named directory.
**Permissions…**, in the header's **More** menu, lists the rules in force. They cannot be
revoked there, since they are changed in their file.

**File watches…**, in the header's **More** menu, lists up to eight live file watches, with their
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
