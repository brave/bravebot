---
id: MEMORY
title: A definition's memory and its checkout
status: proposed
governs:
  - crates/agent/src/agents.rs
  - crates/agent/src/preamble.rs
  - crates/agent/src/delegate.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/memory.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/rewind.rs
  - crates/agent/src/manifest.rs
  - crates/core/src/delegate.rs
  - crates/core/src/policy.rs
  - crates/ui-bridge/src/bridge.rs
  - ui/src/shared/bots.ts
  - ui/src/main/bots.ts
  - ui/src/main/memory.ts
  - ui/src/shared/bot-history.ts
  - ui/scripts/bot-directory.test.mjs
  - ui/scripts/bot-grounding.test.mjs
  - ui/scripts/bot-model.test.mjs
documented-by:
  - docs/website/docs/customize/agents.md
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

The memory and the checkout are built, [MEMORY-2](#MEMORY-2) to [MEMORY-7](#MEMORY-7). The desktop
half is not: [MEMORY-1](#MEMORY-1), [MEMORY-8](#MEMORY-8) to [MEMORY-11](#MEMORY-11), and the
sentences in [MEMORY-4](#MEMORY-4) about the desktop's panel are a design, written to be agreed
before that work starts.

## What exists today

A definition is read for its name, description, kind, tools, model, skills, rounds, memory and
isolation, and every other key is ignored. One whose `memory:` is `project` or `local` keeps a memory in the
working directory ([MEMORY-2](#MEMORY-2)), and every other definition keeps nothing between runs: a
delegate begins with the task it was given, and a run a person addresses begins with the session's
conversation. It works in whichever directory the session is in, except that a delegate of one
whose `isolation:` asks for a checkout works in a checkout of it
([CHECKOUT-2](checkouts.md#CHECKOUT-2)).

The desktop front end's bots are a format of their own. A bot is a row in the desktop's store: a
name, a purpose, a model, a home folder the desktop makes for it under its own data directory, and
a history of conversations, each with the folder it ran in. A conversation runs in the home folder
or in a project folder a person picked for it ([MEMORY-7](#MEMORY-7)). Its purpose reaches a turn
as a briefing file the desktop composes under its own data directory and
hands over as a dropped file, on the first turn of a session, after a compaction, and after a run
of turns in which its memory did not change. Its memory is a file in the folder the conversation
runs in, `.bravebot-ui/bots/<slug>.md`, one in each folder it works in, which the briefing names
and the bot reads and writes with its ordinary tools. The desktop keeps up to thirty earlier
versions of each in its own data directory, and a person can edit the memory and restore an earlier version from a panel. The
desktop addresses no definition.

## The comparison

Claude Code reads a `memory:` key on a subagent definition with three values: `user`, kept under
`~/.claude/agent-memory/<name>/`, `project`, under `.claude/agent-memory/<name>/`, and `local`,
under `.claude/agent-memory-local/<name>/`. The first 200 lines or 25 KB of the `MEMORY.md` in that
directory go into the subagent's system prompt, and setting the key turns on its read, write and
edit tools. `isolation: worktree` runs a subagent in a temporary git worktree branched from the
default branch, removed afterwards if nothing in it changed. No key names a fixed checkout: a
subagent starts in the conversation's working directory.

Three of those cannot be taken as they are. A key that turns on a write tool is a checked-in file
choosing what a run may reach, which [DELEGATE-19](delegation.md#DELEGATE-19) refuses for `tools:`
and [MEMORY-6](#MEMORY-6) refuses here. A memory in the system prompt would put words a run wrote
where a delegate's prompt keeps its definition's, and no word a planner wrote goes there
([DELEGATE-5](delegation.md#DELEGATE-5)), so [MEMORY-4](#MEMORY-4) names the file and the run reads
it. A memory under the home directory is outside the map ([TRUST-11](trust-map.md#TRUST-11)): a
turn writes there only once a person opens that directory ([TRUST-10](trust-map.md#TRUST-10)), and
a write there could not leave the file untrusted. The rest is taken: the key's name, its `project`
and `local` values, and one memory per definition name.

## Which half is a file

<a id="MEMORY-1"></a>
### MEMORY-1: the definition file holds what a bot is for, and the desktop's store holds what happened to it

| Kept in | What | Why there |
|---|---|---|
| the definition file | its name, description, kind, tools, model, skills, rounds, `memory:` and `isolation:`, and the purpose as its body | each says what the bot is for or narrows what it may reach, and each means the same on every machine |
| the desktop's store | the name it is shown under, its home folder, its avatar, its session and every conversation with the folder it ran in, how much compaction has taken from that session, and when it was archived, made and last changed | each is a fact about one machine or a record of what happened |

The memory itself is in neither. It is a file in each folder the bot works in ([MEMORY-2](#MEMORY-2)).

**Why no folder is in the file.** A path is a fact about one machine, and
[MEMORY-7](#MEMORY-7) gives the rest of the reasons.

**Why the history is not in the file.** A session id and a list of conversations are records of
what happened, which the desktop already refuses to take from anything a window typed. A checked-in
file carrying them would change on every turn, and anyone who can commit to the checkout could
write one.

**Why the name it is shown under is not the definition's name.** The definition's name is fixed
when the bot is made ([MEMORY-3](#MEMORY-3)), since its memory is named after it. What a person
calls the bot is free text they may change, like its avatar.

Nothing builds this yet.

`verified-by: none`

## The memory

<a id="MEMORY-2"></a>
### MEMORY-2: `memory: project` or `local` keeps one file in the working directory, and no other value keeps one anywhere

A definition with `memory: project` or `memory: local` keeps its memory in one file,
`.bravebot/memory/<name>.md` under the session's working directory, named after the definition. A
definition with no `memory:` key, or an empty one, keeps none. Any other value, `user` included, is
a definition that loads and keeps no memory, and the turn says so, naming the definition's file and
the value. Where that path falls inside the person's own directory, `~/.bravebot`, as it does for a
session in the home directory, no memory is kept either, and the turn says so.

`project` and `local` are one file. In Claude Code they differ over whether the file is meant to be
committed, and whether a file is committed is decided by the person and their ignore rules rather
than by a key. Nothing here writes an ignore rule.

The working directory is read each turn, so a session moved with `/cd` reads the memory kept where
it moved to.

**Why the working directory.** It is where a run can write, and where the trust map records what a
write did. A memory there is kept with the tools a run already has, and a write its file tools make
passes the gate every such write passes.

**Why not inside the person's own directory.** The map does not govern it
([TRUST-11](trust-map.md#TRUST-11)). A file there is trusted for being the person's, so neither a
write nor the record [MEMORY-5](#MEMORY-5) keeps could leave a memory there untrusted.

**Why `user` loads.** It is Claude Code's default. A definition written for it would otherwise not
load here, which would lose the definition over where its notes go;
[DELEGATE-6](delegation.md#DELEGATE-6) holds a number past a ceiling rather than refusing the file,
for the same reason. It is said, because an author not told would believe a memory was being kept.

**A later definition of the same name takes the key over**, with the body and the model
([DELEGATE-20](delegation.md#DELEGATE-20)), because a memory changes what a run knows and never what
it may do ([MEMORY-6](#MEMORY-6)). Two definitions of one name that both keep a memory keep the same
one, since the file is named after the name.

`verified-by: bravebot_agent::agents::a_definition_keeping_its_memory_in_the_project_or_locally_keeps_one`
`verified-by: bravebot_agent::agents::a_memory_value_nothing_here_keeps_loads_the_definition_and_says_it_keeps_none`
`verified-by: bravebot_agent::agents::a_memory_that_would_sit_in_the_state_directory_is_kept_in_home`
`verified-by: bravebot_agent::agents::a_memory_that_would_sit_inside_the_state_directory_is_not_kept_and_is_said`
`verified-by: bravebot_core::delegate::a_later_definition_takes_over_whether_a_memory_is_kept`
`verified-by: bravebot_core::delegate::keeping_no_memory_clears_it_from_every_definition_and_nothing_else`

<a id="MEMORY-3"></a>
### MEMORY-3: only a definition whose name is a slug keeps a memory

The name becomes a path segment. A definition keeping a memory has a name made of lowercase letters
and digits, in runs joined by single hyphens, at most 64 characters long. A definition whose name is
not one loads and keeps no memory, and the turn says so, naming its file.

**Why this narrow.** A name that cannot traverse is the floor. Lowercase is for a filesystem that
folds case, where `Reviewer` and `reviewer` would be two definitions sharing one file. It is the
class the desktop already holds its bots' slugs to, so a bot's slug becomes a definition's name
without being rewritten.

This amends the sentence in [DELEGATE-21](delegation.md#DELEGATE-21) saying a name is never
resolved against anything. The name of a definition that keeps no memory is held to nothing beyond
that clause.

`verified-by: bravebot_agent::memory::only_a_lowercase_hyphenated_name_of_64_characters_or_fewer_is_a_slug`
`verified-by: bravebot_agent::agents::a_definition_whose_name_is_no_slug_keeps_no_memory_and_says_why`

<a id="MEMORY-4"></a>
### MEMORY-4: the run is told where its memory is and what the map says of it, and reads it itself

Before the turn, with the definitions, the driver asks the map about the memory's path. Trust is
decided first and by the path alone, so a path the map does not trust is withheld whatever is or is
not there. The run is told, in the driver's words:

| The path | What the run is told |
|---|---|
| not trusted | its path, and that the memory is withheld; a read of it is quarantined like any other |
| trusted, and a link, reached through one, or not a file | its path, and that it is not read |
| trusted, and nothing is there | its path, and that nothing is kept yet |
| trusted, and a file is there | its path, and that its notes are there to read |

That sentence goes after the definition's body and before what the run cannot do
([DELEGATE-5](delegation.md#DELEGATE-5)), alike for a run a person addressed and for a delegate a
planner spawned, since each is a run under the definition. What the memory holds reaches the run
only through a read the run makes, on the map's terms, as any file's does. Nothing puts its bytes in
the prompt or the conversation, and nothing keeps a copy of them under another path.

In the desktop front end, which nothing yet builds this for, the history of earlier versions is
retired, and restoring one with it. Its panel still shows the file to the person, and a change they
save there is written as a program they run writes a file.

**Why the path and not the bytes.** A delegate's prompt holds the driver's words and its
definition's body, and no word a planner wrote ([DELEGATE-5](delegation.md#DELEGATE-5)). A memory
is words runs wrote. A planner in a vouched checkout writes one without being asked, and in the
prompt of the next run under that definition its words would stand where that clause keeps the
definition's. Read as a file, a memory is what any file a run reads is.

**Why through the map.** A write whose body is quarantined content leaves its path untrusted
([TRUST-4](trust-map.md#TRUST-4)): content the run passed on by reference, or a processor's reading
of something nobody vouched for. A memory handed to a run whatever the map said would bring those
bytes back as trusted on the next turn, which is the round trip that table exists to close. What the
planner writes in its own words is trusted, since it was never shown what the map does not trust
([LABEL-9](labels.md#LABEL-9)). The desktop found the round trip once already. It named its bots'
memory in every briefing, and naming a path vouches for it, so a memory a write had left untrusted
came back trusted.

**Why trust is decided first.** Whether a file is at a path nobody vouched for is something that
directory decides, and the driver's words carry nothing such a directory decides.

**Why a link is not read.** The map keys a path by the name it is spelled with. A link at the
memory's path to another file in the checkout would be read on the link's terms, whatever a write
had left the file it points at, and the record [MEMORY-5](#MEMORY-5) keeps would name the link.

**Why the path is said even when nothing else is.** The path is made from the working directory and
the definition's name, which the driver already holds, and nothing in it is read out of the file. A
run told nothing would take a withheld memory for an empty one and write over it.

**Why no copy.** The map keys by path. A copy under a path it has never recorded, such as a cache, a
briefing or a list of earlier versions, is read on that path's terms, and in a vouched checkout those
are trusted. Restoring one could put back bytes a write had left untrusted, after a later write had
made the path trusted again: a copy is how untrusted bytes are laundered.

`verified-by: bravebot_agent::memory::a_trusted_memory_stands_as_what_is_at_its_path`
`verified-by: bravebot_agent::memory::a_memory_the_map_does_not_trust_is_withheld_whatever_is_there`
`verified-by: bravebot_agent::delegate::a_delegates_memory_is_said_after_its_body_and_before_what_it_cannot_do`
`verified-by: bravebot_agent::turn::an_addressed_run_is_told_where_its_memory_is_and_not_what_it_holds`
`verified-by: bravebot_agent::turn::a_delegate_under_a_definition_keeping_a_memory_is_told_where_it_is`

<a id="MEMORY-5"></a>
### MEMORY-5: a memory a write left untrusted is still untrusted in the next session

The map belongs to the session ([TRUST-6](trust-map.md#TRUST-6)), so a fresh one does not know what
an earlier session's writes marked untrusted. [trust-map.md](trust-map.md) accepts that as a cost
for files in general. For a memory it does not hold.

A write that leaves a memory's path untrusted, whether a file tool makes it or a command's
redirection does, is recorded before the write lands, naming the path in full, in
`~/.bravebot/untrusted/`, beside the remembered answers to the startup question and keyed as they
are. The path named is the one a session in the memory's directory asks about: that directory
resolved as a working directory is, through every link and in the case the host reports for it,
then `.bravebot/memory/<name>.md`. So a write through a link to `sub` is recorded under `sub`, and
on a volume that holds two spellings differing only in case as one file, a write to
`.Bravebot/memory/NOTES.md` is recorded as `.bravebot/memory/notes.md` and one to
`Sub/.bravebot/memory/notes.md` under `sub`. A write that cannot be recorded, because the session
has no state directory or the record cannot be written, does not land. A rewind putting back bytes
the map will not trust is recorded first in the same way, and is not put back where it cannot be.
The record will also hold the old notes [MEMORY-11](#MEMORY-11) puts in it, which nothing yet
builds. Before every turn, a planned one included, each path the record names is untrusted in the
session's map, as though the session's own write had marked it, so the memory is withheld
([MEMORY-4](#MEMORY-4)) and a read of it is quarantined. That holds however the session came to the
directory: started there, cleared, resumed, reopened, or moved there with `/cd`.

A path leaves the record when a session trusts it again: by a later write that leaves it trusted, by
a rewind that puts back bytes the map trusts, by a person's yes when a read of it is quarantined
([TRUST-8](trust-map.md#TRUST-8)), or by their naming it with `@`, dropping it or attaching it, which
vouches for it as the yes does. Under another spelling, that takes the memory out only where the
volume opens the spelling as the file kept there: NTFS holds `claß.md` apart from `class.md`, and
a yes for one is no yes for the other. A rewind past that grant takes it back, and the path is
recorded again, since a rewind point holds the map with the record's rules in it. A path already
distrusted by a rule of its own is left as it is when the record is read again, so a run starting
changes nothing another run's command is labelled by. A run and its delegates changing the record
at once lose no line either wrote. The record is kept in every session, incognito included.

**Why a memory and not every file.** Every other file reaches a run because something asked to
read it. A memory is read because the driver tells every run under the definition where it is, and
a fresh session is exactly the one the map's forgetting reaches. The cost the map accepts is a file
somebody might read; a memory is a file certain to be read.

**Why this is not the per-directory map the trust map refuses.** That map would be a directory that
trusts itself. This record can only distrust, one path at a time, and it is kept with the person's
own configuration rather than in the directory it is about, so no file in the checkout can take a
line out of it. It is the record trust-map.md's cost about a kept answer says would close that
cost's second half, kept for memories alone.

**Why before every turn.** A session's map is made at a start, a clear and a resume, and moved by
`/cd`. A record read only as a session opened would reach the first of those and miss the rest.

**Why a person's yes takes a path out.** The yes comes after they were shown the file, with what a
check found in all of it, which is what vouching for any file is. A record that outlived the yes
would take it back on the next turn.

**Why before the write lands.** A session killed between the two would leave the bytes and no
record, which is the case this clause is for. A write that cannot be recorded at all would leave the
same, so it is refused.

**Why incognito keeps it.** The record names a path and nothing a person typed. Without it, the next
session would read what the incognito one poisoned as trusted. It does record that a session wrote
in that directory, which [INCOG-3](incognito.md#INCOG-3) otherwise refuses, so
[INCOG-8](incognito.md#INCOG-8) lists it with the other things that mode still writes.

`verified-by: bravebot_agent::memory::a_recorded_memory_is_read_back_for_its_directory_alone`
`verified-by: bravebot_agent::memory::a_directory_sharing_a_record_file_reads_none_of_the_other_s_lines`
`verified-by: bravebot_agent::memory::an_untrusted_memory_write_with_nowhere_to_record_it_is_refused`
`verified-by: bravebot_agent::memory::a_record_that_cannot_be_written_refuses_the_write`
`verified-by: bravebot_agent::memory::a_path_trusted_again_leaves_the_record_and_the_rest_stays`
`verified-by: bravebot_agent::memory::trusting_the_last_recorded_path_again_removes_the_record`
`verified-by: bravebot_agent::memory::a_path_recorded_after_a_half_written_line_is_read_back`
`verified-by: bravebot_agent::memory::a_path_recorded_while_another_is_trusted_again_stays_recorded`
`verified-by: bravebot_agent::memory::the_map_a_rewind_point_holds_distrusts_every_recorded_memory`
`verified-by: bravebot_agent::memory::on_a_volume_that_folds_case_a_memory_in_any_case_is_that_memory`
`verified-by: bravebot_agent::memory::a_memory_written_in_another_case_is_recorded_as_a_session_asks_about_it`
`verified-by: bravebot_agent::memory::a_spelling_the_volume_does_not_open_as_the_memory_leaves_it_recorded`
`verified-by: bravebot_agent::workspace::an_untrusted_write_to_a_memory_is_recorded_in_the_state_directory`
`verified-by: bravebot_agent::workspace::an_untrusted_write_to_a_memory_in_another_case_is_recorded_in_the_state_directory`
`verified-by: bravebot_agent::workspace::an_untrusted_write_to_a_memory_under_a_directory_in_another_case_is_recorded_for_it`
`verified-by: bravebot_agent::workspace::a_write_to_a_memory_through_a_linked_directory_is_recorded_for_the_directory_it_reaches`
`verified-by: bravebot_agent::workspace::a_rewind_into_a_memory_in_another_case_is_recorded_in_the_state_directory`
`verified-by: bravebot_agent::workspace::an_untrusted_write_to_a_memory_with_nowhere_to_record_it_is_refused`
`verified-by: bravebot_agent::workspace::a_trusted_write_to_a_memory_takes_it_out_of_the_record`
`verified-by: bravebot_agent::turn::a_memory_a_write_left_untrusted_is_withheld_from_the_next_session_until_trusted_again`
`verified-by: bravebot_agent::turn::a_persons_yes_to_a_recorded_memory_takes_it_out_of_the_record`
`verified-by: bravebot_agent::turn::a_recorded_memory_named_dropped_or_attached_leaves_the_record`
`verified-by: bravebot_agent::turn::a_redirection_into_a_memory_is_recorded_before_it_opens_it`
`verified-by: bravebot_agent::turn::a_redirection_into_a_memory_in_another_case_is_recorded_before_it_opens_it`
`verified-by: bravebot_agent::rewind::a_rewind_into_a_memory_is_recorded_as_a_write_is`
`verified-by: bravebot_agent::rewind::a_rewind_into_a_memory_in_another_case_is_recorded_as_a_session_asks_about_it`
`verified-by: bravebot_agent::manifest::a_memory_an_earlier_session_left_untrusted_is_untrusted_in_a_plan`
`verified-by: bravebot_agent::manifest::a_plan_writing_untrusted_bytes_into_a_memory_records_it`
`verified-by: bravebot_agent::incognito::a_memory_left_untrusted_is_still_recorded`
`verified-by: bravebot_core::policy::a_remembered_path_already_distrusted_is_left_as_it_is_and_said_once`

<a id="MEMORY-6"></a>
### MEMORY-6: a memory is kept with the tools the run holds, and keeping one adds none

A run updates its memory by writing the file with its ordinary tools, and a write its file tools
make passes the gate every such write passes ([TRUST-4](trust-map.md#TRUST-4)). A program a run
starts writes the memory as it writes any file, which the map does not see. Keeping a memory adds
no tool and no capability.

A `reader` writes nothing and runs nothing, so it cannot change its memory. A `checker` has no tool
that writes a file ([DELEGATE-4](delegation.md#DELEGATE-4)). A delegate's write is shown to a person
first, as every delegate's write is ([DELEGATE-1](delegation.md#DELEGATE-1)). Two runs under one
definition writing its memory at once meet as any two writers of one path do: the second is refused
while the first is in progress.

**Why nothing is added.** A key that turned on a write tool would be a checked-in file choosing what
a run may reach, which [ADDRESS-7](addressing-a-definition.md#ADDRESS-7) says a file may never do. A
definition meant to keep its memory is written as a kind that writes.

**Why no tool of its own.** A tool that wrote only the memory would be a write gated on which file it
is. Every other write is gated on whether the data and the destination are trusted, and that is the
gate a memory needs.

`verified-by: bravebot_agent::turn::keeping_a_memory_offers_no_tool_the_kind_does_not_hold`

## The checkout

<a id="MEMORY-7"></a>
### MEMORY-7: no file names the directory a definition works in

A definition works in the session's working directory, as every run does, except where its
`isolation:` key asks for a checkout the driver makes of it for its delegates
([CHECKOUT-2](checkouts.md#CHECKOUT-2)). No `directory:` key is read, and no key names a directory. The desktop keeps a bot's
home folder and the folder each of its conversations ran in in its own store
([MEMORY-1](#MEMORY-1)), and each conversation is a session in its folder
([SESSION-1](sessions.md#SESSION-1)).

The desktop sends a bot's turn only in a folder the agent confirmed the session runs in, and only
when that folder is one of three: the bot's home folder, which the main process composes from the
bot's slug under its own data directory; a folder the native picker handed over in this run; or a
folder the bot's store records a conversation in. The store records a folder only when a turn sent
there under this rule ends, so a window cannot add one by opening a session somewhere
([TRUST-20](trust-map.md#TRUST-20)). The home folder is beside the directory that holds the
briefing and never inside it, so a run in the home folder cannot rewrite its own purpose.

**Why no file names one.** Each of three reasons is enough:

- A path in a checked-in file is the file choosing where a run reaches, and
  [ADDRESS-7](addressing-a-definition.md#ADDRESS-7) says a file may choose what a run is for and
  never what it may reach.
- A path is a fact about one machine. A definition carrying one works on the machine it was written
  on, so the file stops being something a team can share.
- A run cannot write outside its working directory and the directories a person opened
  ([TRUST-10](trust-map.md#TRUST-10)). A checkout named in a file is reachable only once somebody
  opens it, and that is a person's act and not a file's.

`isolation:` is read as a request for a checkout the driver makes
([CHECKOUT-2](checkouts.md#CHECKOUT-2)), so that key starts a delegate somewhere other than the
working directory. The file still names no path, a turn a person addresses to the definition still
works in the working directory, and a delegate in a checkout keeps no memory
([CHECKOUT-9](checkouts.md#CHECKOUT-9)). A definition keeping a memory and asking for a checkout
says so when it loads, so its author learns that only an addressed turn keeps one.

`verified-by: by-construction (a definition is read for its named keys alone, none of which is a directory, and every run works in the session's workspace or in a checkout the driver made of it)`
`verified-by: by-construction (the desktop is not a crate this workspace compiles, so the folder rule is pinned instead by ui/scripts/bot-directory.test.mjs, which asserts that a new bot's home is composed under the app's data directory whatever the window sent, that a bot works in its home and in a folder the picker handed over and in no other absolute path, that a cancelled picker grants nothing, that a recorded folder survives a restart, and that an edit cannot move the home; ui/scripts/bot-grounding.test.mjs asserts that each folder keeps its own memory, and ui/scripts/bot-model.test.mjs that a bot written before home folders keeps its conversations in the folder it was pinned to; make check-ui and the Front end CI job run them)`

## The desktop's bots

<a id="MEMORY-8"></a>
### MEMORY-8: making a desktop bot writes a definition in the person's own directory

Making a bot writes a definition to `~/.bravebot/agents/<name>.md`. The crate that reads the
person's definitions writes it, since a surface writes nothing into that directory itself
([STATE-3](state-directory.md#STATE-3)), and nothing writes one there today. The name is the bot's
slug. The description is the first line of its purpose that is not blank, and the body is the whole
purpose. The kind is `worker`, `memory:` is `project`, and the model is given where one was chosen.
The desktop's row names the definition and the home folder.

Nothing typed into the form becomes a key. The description and the model are each written so that
reading the file back gives exactly what was typed, and the body comes after the front matter
closes. A model that is not one line, or a purpose with no line that is not blank, is refused, and
no bot is made.

Making a bot never writes over a file. A name some file in that directory already declares, or one
of the kinds' own names ([DELEGATE-19](delegation.md#DELEGATE-19)), is taken, and the next free name
with a number after it is used.

**Why the person's own directory.** A file there is trusted for being theirs
([SKILL-3](skills.md#SKILL-3)), whatever folder a conversation runs in. And it is outside every checkout, so
a run cannot rewrite the definition it runs under, which is the reason the desktop keeps its
briefing outside the folder today.

**Why a worker that names no tools.** A bot's turn has the session's reach today, since the desktop
addresses nothing. A worker that names no tools keeps that reach, with every server the session
reached ([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)), less a later look and a watch, which an
addressed run is never offered ([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). A person
narrows it further by editing the file.

Nothing builds this yet.

`verified-by: none`

<a id="MEMORY-9"></a>
### MEMORY-9: editing a bot rewrites only the fields the form shows

Editing a bot's purpose or model rewrites the description, the body and the model, and leaves every
other line of the file as it is, so a `tools:` line somebody added by hand survives an edit made in
a form that does not show it. What is written is held to the terms [MEMORY-8](#MEMORY-8) holds a
new bot to.

**Why not the whole file.** The file is the person's as much as the desktop's. A form that wrote the
whole file back would undo whatever it does not show, such as a narrowing somebody made by hand.

Nothing builds this yet.

`verified-by: none`

<a id="MEMORY-10"></a>
### MEMORY-10: every turn in a bot's conversation addresses the bot's definition

Each turn in a bot's conversation names the bot's definition, and the driver resolves and addresses
that name as it does one typed after `/agent`
([addressing-a-definition.md](addressing-a-definition.md)). That includes the turns the desktop
composes itself, such as the one it sends after a compaction, and the turn a watch the person armed
there fires. The name comes from the row the person opened, never from the line they typed and never
from a reply. A row whose definition no longer resolves, because the file was removed or no longer
loads, runs nothing and says so, naming the definition.

A reply is drawn in the bot's window under the name the bot is shown under, which comes from that
row, as the definition's name does.

The briefing file is retired, and so are the memory's recorded modification time and the count of
quiet turns that decided when it was sent. The purpose reaches the run as the definition's body, and
the memory's path as [MEMORY-4](#MEMORY-4) puts it, which leaves the briefing nothing to carry.

**Why every turn, where `/agent` lasts one.** [ADDRESS-10](addressing-a-definition.md#ADDRESS-10)
refuses a mode because of four questions. What happens to the conversation so far? What does the
box look like? What key leaves the mode? What does a queued line mean when the mode changes? A
bot's conversation has been the bot's since its first turn, and it has a window of its own. Leaving
it is closing that window, and every line queued there is the bot's. None of the four arises.

**What stands in for the keystroke.** [ADDRESS-3](addressing-a-definition.md#ADDRESS-3) rests an
addressed run on a person's act. Here there are two: making the bot, which wrote the definition,
and opening its conversation, which chose the row. Arming a watch there is a third, for the turns
that watch fires.

**Why the composed turns too.** Addressing only narrows. A turn in a bot's conversation sent
unaddressed would hold the session's whole reach, wider than the bot's own, on a turn nobody typed.

**Why this stops short of a turn the run arranged.** A later look the run schedules and a file it
watches are still never addressed, and are still withheld from an addressed run
([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). What separates them is who decided the turn
happens. The desktop sends its own turn when the agent reports a compaction, and a person arms a
watch: the run chose neither. A scheduled turn is one the run chose.

**Why the shown name.** [ADDRESS-12](addressing-a-definition.md#ADDRESS-12) draws a reply under the
definition's name so that no reply chooses what it is drawn under. The row is the driver's fact in
the same way, and it holds the name the person gave the bot.

This amends [ADDRESS-3](addressing-a-definition.md#ADDRESS-3), which admits only a line typed into
the box, [ADDRESS-12](addressing-a-definition.md#ADDRESS-12), and the sentence in
[addressing-a-definition.md](addressing-a-definition.md) saying the desktop addresses nothing.

Nothing builds this yet.

`verified-by: none`

<a id="MEMORY-11"></a>
### MEMORY-11: a bot made before this keeps its notes where they are, untrusted until a person reads them

The first time a bot with no definition is opened, it is given one as [MEMORY-8](#MEMORY-8) makes
one, named after its old slug where that name is free. Its old memory file is left where it is, and
nothing reads its bytes on the way. Its path goes into the record [MEMORY-5](#MEMORY-5) keeps, so
from then on it is untrusted in every session, as a memory a write left untrusted is.

The first turn after that is one the desktop composes and addresses. It tells the run where the old
notes are, in the words of the turn and never as a file handed to it, and asks it to carry what
still holds into its memory. The run's read of them is quarantined, so a person is shown them, with
what a check found in all of them, and asked before they reach the run
([TRUST-8](trust-map.md#TRUST-8)).

**Why untrusted whatever the map would say.** A write in an earlier session may have left them
untrusted, and the map that recorded it is gone ([TRUST-6](trust-map.md#TRUST-6)). Until its fix,
the desktop also named them in every briefing, which vouched for them whatever a write had left.
Nothing now knows whether a given bot's notes were ever left untrusted, so a person decides.

**Why the run and not the desktop.** A copy made by the desktop would put those bytes under a path
the map has never recorded, which in a vouched checkout is trusted by the directory above it. What
the run writes from notes a person let it read is what it writes from any file it read.

**Why not handed as a file.** A file handed to a turn is one a person is recorded as vouching for,
and nobody vouched for these notes.

Nothing builds this yet.

`verified-by: none`

## Open questions

- **Whether a memory may live in the person's own directory.** `memory: user` needs a run to write
  into `~/.bravebot` without a person opening it, which no tool does. It also needs a record of what
  that write did, which the map does not keep there ([TRUST-11](trust-map.md#TRUST-11)). Both are
  new, and until they exist `user` keeps nothing ([MEMORY-2](#MEMORY-2)).

## Known costs

- **A checkout a person vouched for can hold a memory for any name.** A file at
  `.bravebot/memory/<name>.md` there is the memory of any definition of that name, including one in
  the person's own directory, as a checked-in `AGENTS.md` is that project's instructions. The run
  reads it as it reads any file there, and it decides nothing about what the run may reach
  ([MEMORY-6](#MEMORY-6)), which is the bound a definition's body has too.

- **A program writes a memory where the map cannot see.** [trust-map.md](trust-map.md) records this
  for every file: a `git pull`, an editor, or a program a run was let start, `curl -o` to the
  memory's path among them, puts bytes there that are read as trusted wherever the path is. What a
  memory adds is that every run under its definition is told to read it. A checked-in `AGENTS.md`
  carries the same cost and is put in every turn's prompt. A checkout moved or cloned afresh starts
  without the record [MEMORY-5](#MEMORY-5) keeps.

- **Keeping quarantined content in a memory costs the memory.** Passing a fetched page on by
  reference, or a processor's reading of a file nobody vouched for, asks before it is written, and
  after a yes the memory is withheld until a write that leaves it trusted replaces it, or a person
  shown it says yes or names it ([MEMORY-5](#MEMORY-5)). Deleting it empties it and does not put it back. What
  the run writes in its own words after such a read is trusted and asks nothing.

- **A memory is one read away.** Claude Code puts the head of one in the prompt. Here a run that
  does not read it works without it, and each run that does spends a round on it.

- **A memory is per checkout.** Two clones of one project keep two, and a definition in the
  person's own directory keeps one in each project it runs in. It is also a file `git status` shows
  until the person ignores it or commits it.

- **A rename is a new definition.** The memory is named after the definition's name, so a file
  whose `name:` changes starts with no memory, and the old one stays where it was.

- **A `reader` or a `checker` has no tool that keeps its memory.** It reads one something else
  wrote, and changing it takes a definition of a kind that writes ([MEMORY-6](#MEMORY-6)).

- **A bot cannot arrange a later turn of its own.** Every turn in its conversation is addressed
  ([MEMORY-10](#MEMORY-10)), and an addressed run is offered neither a later look nor a watch
  ([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). A watch the person arms still fires.

- **A bot is on offer to every planner in every project**, as any definition in the person's own
  directory is ([INSTR-1](instructions.md#INSTR-1)). A planner may spawn one as a delegate in a
  folder the person never sent it to, where its memory is whichever that checkout keeps. Whether a
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
