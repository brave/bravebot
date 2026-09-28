---
id: MEMORY
title: A definition's memory and its checkout
status: proposed
governs:
  - crates/agent/src/agents.rs
  - crates/agent/src/preamble.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/turn.rs
  - crates/core/src/delegate.rs
  - crates/ui-bridge/src/bridge.rs
  - ui/src/shared/bots.ts
  - ui/src/main/bots.ts
  - ui/src/main/memory.ts
documented-by: none (gap: nothing here is built yet, so there is no behaviour for a page to describe)
---

## Scope

What a definition keeps from one conversation to the next, where it is kept, and which checkout a
definition works in. Then what the desktop front end's bots become once a definition can keep
both: a definition file, and a row the desktop keeps for itself.

What a definition is, where one is read from and what it is trusted for is
[delegation.md](delegation.md). Running one yourself is
[addressing-a-definition.md](addressing-a-definition.md). What a write does to the record of which
paths are trusted is [trust-map.md](trust-map.md). Where a clause here changes one of those, that
file says so at the place it changes.

Nothing in this file is built. It is a design, written to be agreed before the work starts.

## What exists today

A definition is read for its name, description, kind, tools, model, skills and rounds, and every
other key is ignored. It keeps nothing between runs: a delegate begins with the task it was given,
and a run a person addresses begins with the session's conversation. It works in whichever
directory the session is in.

The desktop front end's bots are a format of their own. A bot is a row in the desktop's store: a
name, a purpose, a model, a folder chosen when it was made, and a history of conversations. Its
purpose reaches a turn as a briefing file the desktop composes under its own data directory and
hands over as a dropped file, on the first turn of a session, after a compaction, and after a run
of turns in which its memory did not change. Its memory is a file in its folder,
`.bravebot-ui/bots/<slug>.md`, which the briefing names and the bot reads and writes with its
ordinary tools. The desktop addresses no definition.

## The comparison

Claude Code reads a `memory:` key on a subagent definition with three values: `user`, kept under
`~/.claude/agent-memory/<name>/`, `project`, under `.claude/agent-memory/<name>/`, and `local`,
under `.claude/agent-memory-local/<name>/`. The first 200 lines or 25 KB of the `MEMORY.md` in that
directory go into the subagent's system prompt, and setting the key turns on its read, write and
edit tools. `isolation: worktree` runs a subagent in a temporary git worktree branched from the
default branch, removed afterwards if nothing in it changed. No key names a fixed checkout: a
subagent starts in the conversation's working directory.

Two of those cannot be taken as they are. A key that turns on a write tool is a checked-in file
choosing what a run may reach, which [DELEGATE-19](delegation.md#DELEGATE-19) refuses for `tools:`
and [MEMORY-6](#MEMORY-6) refuses here. A memory under the home directory is a file no turn can
write, since a turn's writes stay in its working directory and the directories a person opened
([TRUST-10](trust-map.md#TRUST-10)), and one the map could not mark untrusted if a turn could,
since the map does not govern that directory ([TRUST-11](trust-map.md#TRUST-11)). The rest is
taken: the key's name, its `project` and `local` values, one memory per definition name, and a
bounded head of it in the prompt.

## Which half is a file

<a id="MEMORY-1"></a>
### MEMORY-1: the definition file holds what a bot is for, and the desktop's store holds what happened to it

| Kept in | What | Why there |
|---|---|---|
| the definition file | its name, description, kind, tools, model, skills, rounds and `memory:`, and the purpose as its body | each says what the bot is for or narrows what it may reach, and each means the same on every machine |
| the desktop's store | the name it is shown under, its folder, its avatar, its session and every conversation, the compaction and memory watermarks and the count of quiet turns, and when it was archived, made and last changed | each is a fact about one machine or a record of what happened |

The memory itself is in neither. It is a file in the checkout ([MEMORY-2](#MEMORY-2)).

**Why the folder is not in the file.** A path is a fact about one machine, and
[MEMORY-7](#MEMORY-7) gives the rest of the reasons.

**Why the history is not in the file.** A session id and a list of conversations are records of
what happened, which the desktop already refuses to take from anything a window typed. A checked-in
file carrying them would change on every turn, and anyone who can commit to the checkout could
write one.

**Why the name it is shown under is not the definition's name.** The definition's name is fixed
when the bot is made ([MEMORY-3](#MEMORY-3)), since its memory is named after it. What a person
calls the bot is free text they may change, like its avatar.

`verified-by: none`

## The memory

<a id="MEMORY-2"></a>
### MEMORY-2: `memory: project` keeps one file in the working directory, and no other value keeps one anywhere

A definition with `memory: project` or `memory: local` keeps its memory in one file,
`.bravebot/memory/<name>.md` under the session's working directory, named after the definition. A
definition with no `memory:` key keeps none, as today. Any other value, `user` included, is a
definition that loads and keeps no memory, and the turn says so, naming the definition's file and
the value.

`project` and `local` are one file. In Claude Code they differ over whether the file is meant to be
committed, and whether a file is committed is decided by the person and their ignore rules rather
than by a key. Nothing here writes an ignore rule.

The working directory is read each turn, so a session moved with `/cd` reads the memory kept where
it moved to.

**Why the working directory.** It is where a run can write, and where the trust map records what a
write did. A memory there is kept with the tools a run already has, through the gate every write
already passes.

**Why `user` loads.** It is Claude Code's default. A definition written for it would otherwise not
load here, which would lose the definition over where its notes go;
[DELEGATE-6](delegation.md#DELEGATE-6) holds a number past a ceiling rather than refusing the file,
for the same reason. It is said, because an author not told would believe a memory was being kept.

**A later definition of the same name takes the key over**, with the body and the model
([DELEGATE-20](delegation.md#DELEGATE-20)), because a memory changes what a run is told and not what
it may do ([MEMORY-6](#MEMORY-6)). Two definitions of one name that both keep a memory keep the same
one, since the file is named after the name.

`verified-by: none`

<a id="MEMORY-3"></a>
### MEMORY-3: only a definition whose name is a slug keeps a memory

The name becomes a path segment. A definition keeping a memory has a name made of lowercase letters
and digits, in runs joined by single hyphens, at most 64 characters long. A definition whose name is
not one loads and keeps no memory, and the turn says so, naming its file.

**Why this narrow.** A name that cannot traverse is the floor. Lowercase is for a filesystem that
folds case, where `Reviewer` and `reviewer` would be two definitions sharing one file. It is the
class the desktop already holds its bots' names to, so a bot's name becomes a definition's name
without being rewritten.

This amends the sentence in [DELEGATE-21](delegation.md#DELEGATE-21) saying a name is never
resolved against anything. The name of a definition that keeps no memory is held to nothing beyond
that clause.

`verified-by: none`

<a id="MEMORY-4"></a>
### MEMORY-4: the memory reaches the run through the trust map, and only as far as the map trusts it

The memory is read before the turn, with the definitions, through the gate a project's `AGENTS.md`
passes ([INSTR-5](instructions.md#INSTR-5)). What the run is told depends on what that read found:

| The file | What the run is told |
|---|---|
| trusted | its path, and what it holds, up to 32 KiB cut at a line |
| trusted, and longer than that | the same, and that the file goes on past what it was given |
| not trusted | its path, and that it was withheld; a read of it is quarantined like any other |
| not there | its path, and that nothing is kept yet |

What it holds goes into the prompt the definition's body is in, after the body and before what the
run cannot do, so that it displaces neither the guidance before it nor the limits after it
([DELEGATE-5](delegation.md#DELEGATE-5)). It is never put in the conversation. This holds alike for
a run a person addressed and for a delegate a planner spawned, since each is a run under the
definition.

Nothing else reads the memory's bytes, and nothing copies them to another path.

**Why through the map and not around it.** A model writes the memory, and a turn that has met
untrusted content writes untrusted data: the write asks the person, and the path stops being trusted
([TRUST-4](trust-map.md#TRUST-4)). A memory put in the prompt whatever the map said would bring those
bytes back as trusted context on the next turn, which is the round trip that table exists to close.
The desktop found this once already. It named its bots' memory among the files it handed a turn,
naming a path vouches for it, and a memory a write had left untrusted came back trusted on the next
briefing.

**Why no copy.** The map keys by path. A copy under a path it has never recorded, such as a cache or
a briefing, is read on that path's terms, and in a vouched checkout those are trusted: a copy is how
untrusted bytes are laundered.

**Why a bound.** The memory is sent with every request of every turn under the definition. 32 KiB is
about eight thousand tokens, which is where notes turn into a transcript, and the rest is one read
away.

**Why the path is said even when nothing else is.** The path is made from the working directory and
the definition's name, which the driver already holds, and nothing in it is read out of the file. A
run told nothing would take a withheld memory for an empty one and write over it.

`verified-by: none`

<a id="MEMORY-5"></a>
### MEMORY-5: a memory a write left untrusted is still untrusted in the next session

The map belongs to the session ([TRUST-6](trust-map.md#TRUST-6)), so a fresh one does not know what
an earlier session's writes marked untrusted. [trust-map.md](trust-map.md) accepts that as a cost
for files in general. For a memory it does not hold.

A write that leaves a memory's path untrusted is recorded under `~/.bravebot`, through the crate
that owns the record, before the write lands, naming the path in full. A session opening in that
directory starts with the path untrusted in its map, as though its own write had marked it, so the
memory is withheld ([MEMORY-4](#MEMORY-4)) and a read of it is quarantined. A later write that leaves
the path trusted takes the record away. An incognito session keeps the record too.

**Why a memory and not every file.** Every other file reaches a run because something asked to
read it. A memory reaches the next session's prompt because that is what it is for, with nobody
reading it first, and a fresh session is exactly the one the map's forgetting reaches. The cost the
map accepts is a file somebody might read; a memory is a file certain to be read.

**Why this is not the per-directory map the trust map refuses.** That map would be a directory that
trusts itself. This record can only distrust, one path at a time, and it is kept with the person's
own configuration rather than in the directory it is about, so no file in the checkout can take a
line out of it.

**Why before the write lands.** A session killed between the two would leave the bytes and no
record, which is the case this clause is for.

**Why incognito keeps it.** The record names a path and nothing a person typed. Without it, the next
session would read what the incognito one poisoned as trusted. It does record that a session wrote
in that directory, which [INCOG-3](incognito.md#INCOG-3) otherwise refuses, so
[INCOG-8](incognito.md#INCOG-8) lists it with the other things that mode still writes.

`verified-by: none`

<a id="MEMORY-6"></a>
### MEMORY-6: a memory is kept with the tools the run holds, and keeping one adds none

A run updates its memory by writing the file with its ordinary tools, and the write passes the gate
every write passes ([TRUST-4](trust-map.md#TRUST-4)). Keeping a memory adds no tool and no
capability.

A `reader` or a `checker` holds no write ([DELEGATE-4](delegation.md#DELEGATE-4)), so a definition
of either kind reads its memory and cannot change it, and it is told so with what else it cannot do.
A delegate's write is shown to a person first, as every delegate's write is
([DELEGATE-1](delegation.md#DELEGATE-1)). Two runs under one definition writing its memory at once
meet as any two writers of one path do: the second is refused while the first is in progress.

**Why nothing is added.** A key that turned on a write tool would be a checked-in file choosing what
a run may reach, which [ADDRESS-7](addressing-a-definition.md#ADDRESS-7) says a file may never do. A
definition meant to keep its memory is written as a kind that writes.

**Why no tool of its own.** A tool that wrote only the memory would be a write gated on which file it
is. Every other write is gated on whether the data and the destination are trusted, and that is the
gate a memory needs.

`verified-by: none`

## The checkout

<a id="MEMORY-7"></a>
### MEMORY-7: a definition works in the checkout its session is in, and no file names one

A definition works in the session's working directory, as every run does. No `directory:` key is
read, and no key starts a run anywhere else. The desktop keeps the folder a bot was made for in its
own store ([MEMORY-1](#MEMORY-1)), and a bot's conversation is a session in that folder
([SESSION-1](sessions.md#SESSION-1)).

**Why no file names one.** Each of three reasons is enough:

- A path in a checked-in file is the file choosing where a run reaches, and
  [ADDRESS-7](addressing-a-definition.md#ADDRESS-7) says a file may choose what a run is for and
  never what it may reach.
- A path is a fact about one machine. A definition carrying one works on the machine it was written
  on, so the file stops being something a team can share.
- A run cannot write outside its working directory and the directories a person opened
  ([TRUST-10](trust-map.md#TRUST-10)). A checkout named in a file is reachable only once somebody
  opens it, and that is a person's act and not a file's.

`isolation:` is not read either, and is an open question below rather than a refusal.

`verified-by: none`

## The desktop's bots

<a id="MEMORY-8"></a>
### MEMORY-8: a desktop bot is a definition in the person's own directory, plus a row the desktop keeps

Making a bot writes a definition to `~/.bravebot/agents/<name>.md`, through the crate that owns that
directory ([STATE-3](state-directory.md#STATE-3)). The name is the bot's slug. The description is
the first line of its purpose, and the body is the whole purpose. The kind is `worker`, `memory:` is
`project`, and the model is given where one was chosen. Nothing typed into the form becomes a key:
the description is one line, and the body comes after the front matter closes.

The file is created and never written over. A name some file in that directory already declares, or
one of the kinds' own names ([DELEGATE-19](delegation.md#DELEGATE-19)), is taken, and the next free
name with a number after it is used.

Editing a bot's purpose or model rewrites those fields and leaves every other line of the file as it
is, so a `tools:` line somebody added by hand survives an edit made in a form that does not show it.
The desktop's row names the definition and the folder. A row whose definition no longer resolves,
because the file was removed or no longer loads, runs nothing and says so, naming the definition.

**Why the person's own directory.** A file there is trusted for being theirs
([SKILL-3](skills.md#SKILL-3)), whatever the bot's folder is. And it is outside every checkout, so
a run cannot rewrite the definition it runs under, which is the reason the desktop keeps its
briefing outside the folder today.

**Why a worker that names no tools.** A bot's turn has that reach today, since the desktop
addresses nothing: the session's own, with every server it reached
([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)). A person narrows it by editing the file.

`verified-by: none`

<a id="MEMORY-9"></a>
### MEMORY-9: every turn in a bot's conversation addresses the bot's definition

Each turn the desktop sends in a bot's conversation names the bot's definition. The driver resolves
and addresses that name as it does one typed after `/agent`
([addressing-a-definition.md](addressing-a-definition.md)). That includes the turns the desktop
composes itself, such as the one it sends after a compaction. The name comes from the row the person
opened, never from the line they typed and never from a reply.

The briefing file is retired. The purpose reaches the run as the definition's body and the memory
as [MEMORY-4](#MEMORY-4) puts it, which leaves the briefing nothing to carry.

**Why every turn, where `/agent` lasts one.** [ADDRESS-10](addressing-a-definition.md#ADDRESS-10)
refuses a mode because of four questions. What happens to the conversation so far? What does the
box look like? What key leaves the mode? What does a queued line mean when the mode changes? A
bot's conversation has been the bot's since its first turn, and it has a window of its own. Leaving
it is closing that window, and every line queued there is the bot's. None of the four arises.

**What stands in for the keystroke.** [ADDRESS-3](addressing-a-definition.md#ADDRESS-3) rests an
addressed run on a person's act. Here there are two: making the bot, which wrote the definition,
and opening its conversation, which chose the row.

**Why the composed turns too.** Addressing only narrows. A turn the desktop composes in a bot's
conversation and sends unaddressed would hold the session's whole reach, wider than the bot's own,
on a turn nobody typed.

**Why this stops short of a turn the run arranged.** A later look the run schedules and a file it
watches are still never addressed, and are still withheld from an addressed run
([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). What separates the two is who decided the turn
happens. The desktop sends its own turn when the agent reports a compaction, and the run chose
neither the compaction nor the turn. A scheduled turn is one the run chose.

This amends [ADDRESS-3](addressing-a-definition.md#ADDRESS-3), which admits only a line typed into
the box, and the sentence in [addressing-a-definition.md](addressing-a-definition.md) saying the
desktop addresses nothing.

`verified-by: none`

<a id="MEMORY-10"></a>
### MEMORY-10: a bot made before this keeps its notes where they are, and they are read rather than copied

The first time a bot with no definition is opened, it is given one as
[MEMORY-8](#MEMORY-8) makes one, named after its old slug where that name is free. Its old memory
file is left where it is, and nothing reads its bytes on the way. The first turn after that is one
the desktop composes and addresses. It tells the run where the old notes are, in the words of the
turn and never as a file handed to it, and asks it to carry what still holds into its memory.

**Why the run and not the desktop.** Whether the old notes are trusted is the map's answer, and only
a read through the map gets it. Notes the map trusts are read as they are. Notes it does not trust
come back quarantined, so what the run writes from them is untrusted and the write asks. A copy made
by the desktop would put those bytes under a path the map has never recorded, which in a vouched
checkout is trusted by the directory above it.

**Why not handed as a file.** A file handed to a turn is one a person is recorded as vouching for,
and nobody vouched for these notes.

`verified-by: none`

## Open questions

- **Whether a definition may ask for a checkout of its own.** Claude Code's `isolation: worktree`
  gives a subagent a fresh worktree. Here that would be a run writing outside its working directory
  ([TRUST-10](trust-map.md#TRUST-10)), into a path nobody vouched for whose first write asks, after
  a `git` command no person chose to run. The directory would outlive the turn that made it, so
  removing it is either a question for a person or a deletion nobody approved. Each of those has an
  answer, and together they are a spec of their own.

- **Whether a memory may live in the person's own directory.** `memory: user` needs a write a run
  can make into `~/.bravebot`, which no tool has. It also needs a record of what that write did,
  which the map does not keep there ([TRUST-11](trust-map.md#TRUST-11)). Both are new, and until
  they exist `user` keeps nothing ([MEMORY-2](#MEMORY-2)).

## Known costs

- **A checkout a person vouched for can hold a memory for any name.** A file at
  `.bravebot/memory/<name>.md` there is read as the memory of any definition of that name,
  including one in the person's own directory, as a checked-in `AGENTS.md` is read as that
  project's instructions. It decides what the run is told and never what it may reach
  ([MEMORY-6](#MEMORY-6)), which is the bound a definition's body has too.

- **A file another program writes over a memory is read as trusted wherever its path is.**
  [trust-map.md](trust-map.md) records this for every file. A `git pull` replacing a memory in a
  vouched checkout, or in one where a clean turn's write left the memory's path trusted, puts the
  new bytes in the next prompt. What a memory adds is that nobody has to read the file for that to
  happen. A checkout moved or cloned afresh starts without the record [MEMORY-5](#MEMORY-5) keeps.

- **A bot that reads untrusted content asks before it writes its memory, and a yes costs the
  memory.** A fetched page and a command's output nobody vouched for both leave a turn holding
  untrusted content, so its memory write asks, and after a yes the memory is withheld until a turn
  that met nothing untrusted replaces it. That is the gate working, and it costs such a bot the
  memory it can see. No answer puts a withheld memory back: deleting it or replacing it does.

- **A memory is per checkout.** Two clones of one project keep two, and a definition in the
  person's own directory keeps one in each project it runs in. It is also a file `git status` shows
  until the person ignores it or commits it.

- **A rename is a new definition.** The memory is named after the definition's name, so a file
  whose `name:` changes starts with no memory, and the old one stays where it was.

- **A `reader` or a `checker` cannot keep its memory.** It reads one that something else wrote, and
  changing it takes a definition of a kind that writes ([MEMORY-6](#MEMORY-6)).

- **A bot is on offer to every planner in every project**, as any definition in the person's own
  directory is ([INSTR-1](instructions.md#INSTR-1)). A planner may spawn one as a delegate far from
  the folder it was made for, where its memory is whichever that checkout keeps. Whether a
  definition may say it is meant only to be addressed is an open question in
  [addressing-a-definition.md](addressing-a-definition.md).

- **A vouched checkout can take over what a bot is for.** A definition of the bot's name there
  replaces its body and its `memory:` key, as [DELEGATE-20](delegation.md#DELEGATE-20) lets any
  project do, and cannot widen what it may reach.

- **A bot's session resumed in the terminal runs as the session's own.** The record does not keep
  which definition answered ([addressing-a-definition.md](addressing-a-definition.md)), so the
  terminal does not address it, and the turns there hold the session's reach.

- **Using Claude Code alongside keeps two memories.** Its `.claude/agent-memory/` is not read here,
  and this program's memory is not read there.
