---
id: BG
title: Background sessions
status: proposed
governs:
  - crates/cli/src/main.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/app.rs
  - crates/agent/src/home.rs
documented-by:
  - none (gap: a page on starting a session in the background, listing, attaching to and replying to one, owed until the design is built)
---

## Scope

A session that keeps running after the terminal that started it closes: how a person starts one,
what owns its process, what is recorded about it, how it is listed, how a terminal reattaches, how
a person sends it its next prompt, and what it does when it needs an answer.

The roster, `bravebot sessions`, `bravebot sessions stop`, `bravebot --bg`, `bravebot attach` and
`bravebot reply` are built, and the session a background process runs is the session in lines. A
clause that is built whole names its tests. A clause with a part still to build reads
`verified-by: none` until all of it lands, and the parts not built are: the supervisor and restarts
([BG-12](#BG-12)), the idle stop ([BG-13](#BG-13)), starting a session again from `attach` or
`reply`, `/bg` and `/detach`, the refusal of `--resume`, `--continue` and `--fork` for a record a
running session holds, the checkout ([BG-14](#BG-14)), and holding a question from an MCP server
that has to be started. A session that needs one of those servers starts without it. The clauses
that other specs would contradict are named under
[What this changes in other specs](#what-this-changes-in-other-specs).

A **background session** is an ordinary session, as [sessions.md](sessions.md) describes one, whose
process a supervisor owns instead of a terminal. The **supervisor** is a small process that starts,
stops and restarts those processes and records their state. The **roster** is that record. A
**foreground session** is any session that is not one.

What a session record holds is [sessions.md](sessions.md). What the prompts ask and what an answer
grants is [prompting.md](prompting.md), and what a mode answers in advance is
[permission-modes.md](permission-modes.md). A checkout is [checkouts.md](checkouts.md). The rule
that untrusted content never reaches the driver or the planner is [labels.md](labels.md) and
[AGENTS.md](../../agents/AGENTS.md).

## What exists today

A session ends with the terminal it runs in. Nothing holds one once that terminal closes, and the
only way back is `--resume`, which starts a new process from the record. A delegate ends with the
turn that started it ([delegation.md](delegation.md)). A background job is a shell command inside a
turn and ends with it. [info-panel.md](info-panel.md) tells the sessions open in different terminals
apart, and no command lists sessions that are running or reattaches to one. No process holds a
record, so two processes can resume one.

## Clauses

<a id="BG-1"></a>
### BG-1: a background session is an ordinary session in a process the terminal does not own

A background session runs the same turn loop as a foreground one, reads and writes the same record
in the same place, asks the same prompts, and keeps its trust map and its standing permissions in
that record. The record is the state a restart resumes from. The supervisor's process table and the
roster add no state to the session.

A background session is refused where one cannot be kept: with `--incognito`, which adds nothing to
`~/.bravebot` while the roster and the record both need to be there, and on a platform where the
attach channel in [BG-9](#BG-9) cannot be restricted to the person's account. The refusal names the
reason and starts nothing.

**Why.** A second kind of session, with its own record or its own prompts, would be a second set of
rules to keep in step with the first, and the second set is the one nobody reads.

`verified-by: none`

<a id="BG-2"></a>
### BG-2: only a line a person typed starts one

`bravebot --bg "prompt"` starts a background session whose first turn is the prompt as typed.
`/bg` between two turns of a foreground session hands that conversation to a background session and
ends the foreground process once the record is written. `/bg` while a turn is running is refused,
because the process holding that turn cannot be handed over. Both start the session in the working
directory they ran in, and `/bg` keeps the permission mode the foreground session was in
([BG-8](#BG-8)).

Nothing else starts one. The planner has no tool for it, a delegate cannot, and neither can a hook,
a loop tick, a watch or a file the session read. A prompt that arrived on a pipe is untrusted input
and does not start one either.

**Why.** A background session does work with nobody watching. A thing that may start one is a thing
that may start unwatched work, so the only thing that may is a person, and the line they typed is
the whole of what they asked for.

`verified-by: none`

<a id="BG-3"></a>
### BG-3: the supervisor moves process state and decides nothing from content

The supervisor starts a session's process, stops it, restarts it ([BG-12](#BG-12)), stops it when it
has been idle ([BG-13](#BG-13)), and writes the roster. It reads no conversation, no record, no
prompt, no reply and no tool result. It makes no model request and holds no credential: a session's
process reads its own, as a foreground one does. It branches only on process facts (whether the
process is alive, how it exited, when it last did anything) and on the roster's own fields.

The attach channel ([BG-9](#BG-9)) is between a terminal and the session's process. The supervisor
is not on it.

**Why.** The supervisor is part of the driver, and the driver does not branch on untrusted bytes.
Keeping it ignorant of every byte of content is how that holds without an argument about each field:
a supervisor that can read a conversation can be made to act on one.

`verified-by: none`

<a id="BG-4"></a>
### BG-4: the roster is a closed set of fields in the person's own state directory

The roster is one directory per session, `~/.bravebot/jobs/<id>/`, where `<id>` is the session's id
and the one `--resume` takes. Its `state.json` holds exactly these fields: the id, the working
directory and the checkout the session works in ([BG-14](#BG-14)), the session's name, the last
prompt the person typed, the state ([BG-6](#BG-6)), the kind of the held prompt if there is one
([BG-7](#BG-7)), the process id and the time it started, the time of the last turn, and the mode the
session runs in. A field added later states where its value comes from, and it is added only if that
is something the person typed or a fact about the process.

The directory is created with mode 0700 and the files in it with mode 0600, the same as everything
else in the state directory. The roster lists sessions from every project, which the per-directory
session records cannot do.

No conversation, tool result, reply, quarantined slot or reference is written to the roster. A
session's quarantined content stays in the process that holds it and is not in the record either, so
a restarted session resumes without it, as a resumed foreground session does.

**Why.** The roster is read by `bravebot sessions` for every session on the machine, and drawn in a
terminal. A closed set whose sources are named is what lets a reader of a diff check that nothing
untrusted is among them. A field that could hold "whatever is useful" would be filled with whatever
was nearest.

`verified-by: bravebot_session::jobs::an_entry_holds_exactly_the_fields_the_spec_names`
`verified-by: bravebot_session::jobs::the_directory_and_its_files_are_private`
`verified-by: bravebot_session::jobs::an_id_that_is_a_path_names_nothing`
`verified-by: bravebot_session::jobs::a_prompt_is_bounded_when_it_is_stored`

<a id="BG-5"></a>
### BG-5: the list shows labels and states, never content

`bravebot sessions` prints one line per background session, across every project: the first eight
characters of its id, its name, its state, its working directory, how long since its last turn, and the last prompt the person typed,
cut as a title is cut. `--json` prints the roster fields for each session. Where a prompt is held,
the line says that the session needs input and names the kind of prompt it is, by one word from a fixed set with
a word for each kind a foreground session puts, and shows nothing of what the prompt is about.

Every part of a line is something the person typed, a fact about the process, or one of a fixed set
of words. No part of it comes from a reply, a tool result, a file's contents, a command's output or
the text of a question. Control characters in a name or a prompt are replaced by visible ones, as
they are wherever text is drawn.

The list is how a person finds a session, and a prompt is answered only by attaching
([BG-7](#BG-7)), so the list needs nothing from the prompt that the person could act on from it.

**Why.** Many sessions are listed at once, so a line drawn here is in front of the person for as long
as the list is. Text from the model's reply or from a tool would be read as the program's own
sentence, which is the reason the info panel draws nothing from either.

`verified-by: bravebot_cli::background::a_control_character_in_what_was_typed_cannot_write_a_row`
`verified-by: bravebot_cli::background::a_held_prompt_shows_its_kind_and_nothing_else`
`verified-by: bravebot_cli::background::a_session_with_no_process_is_not_listed_as_working`
`verified-by: bravebot_cli::running::sessions_lists_an_entry_with_no_process_as_interrupted`

<a id="BG-6"></a>
### BG-6: a session is in one of five states, and the state is a fact about the process

| State | Meaning |
|---|---|
| `working` | a turn is running |
| `needs input` | a prompt is held ([BG-7](#BG-7)) |
| `idle` | the process is alive and no turn is running |
| `stopped` | the process was stopped by a person or by [BG-13](#BG-13), and the record is intact |
| `interrupted` | the process ended while a turn was running ([BG-12](#BG-12)) |

The session's process reports `working`, `needs input` and `idle`; whoever stops it sets `stopped`.
A process is live while it holds the lock on the `live` file in its directory, which the kernel
releases when the process ends however it ends. A roster entry with no such lock held reads as
`interrupted` if its last state was `working` or `needs input`, and as `stopped` otherwise, so
neither a process that was killed nor a pid that now belongs to another program is believed.

**Why.** A state the list can show truthfully has to be one the process can state without anybody
reading content. Whether a turn's work is ready for review is a judgement about content, so it is
not a state.

`verified-by: bravebot_session::jobs::an_entry_whose_process_is_gone_is_not_believed`
`verified-by: bravebot_session::jobs::a_held_prompt_without_a_process_is_interrupted_not_needing_input`
`verified-by: bravebot_session::jobs::a_second_process_cannot_claim_a_live_entry`

<a id="BG-7"></a>
### BG-7: a question the session would ask is held until a person answers it

Every prompt a foreground session would put to a person, a question the planner asks included,
is held in the process. The session pauses on it, its state is `needs input`, and it stays
that way for as long as it takes. No timeout answers it and no default does. The process holding it
is not stopped for idleness, and a restart does not answer it.

Only a person attached to the session answers it ([BG-9](#BG-9)). The attached terminal draws the
prompt as a foreground session would, with what is at stake shown in full, and the answer is that
person's. A prompt that ends because its process ended is gone, and the session is `interrupted`.

**Why.** A one-shot run declines every prompt because nobody can be asked. A background session can
be asked later, and declining on the person's behalf while they are away would cancel work they
expect to find waiting. Answering would make the absent person's answer a guess made in their name.
Holding the prompt is the only choice that does neither.

`verified-by: none`

<a id="BG-8"></a>
### BG-8: the mode is fixed for the process, bypass is refused, and a rule that allows does not answer

A background session runs in one mode for as long as its process runs. `--bg` starts in the mode
every session opens in, which asks about everything. `/bg` starts in the mode the foreground session
was in, asking, accepting edits or planning. Attaching offers no key to change it, and the mode is
shown to the person attached.

`--dangerously-skip-permissions` with `--bg` is refused, and so is `/bg` from a session in bypass.
The refusal says that a session nobody is watching does not run in a mode that answers every
question, and names a one-shot run as the way to ask for that.

An allow rule from the settings and a command line remembered from an earlier session do not
answer a prompt in a background session. A prompt either would have answered is held, as every
other is. A rule that refuses, or that forces a prompt, holds here as it does anywhere. Only the
mode the person chose answers in their absence.

A process the supervisor starts again, after it was stopped or after it exited, opens in the mode
every session opens in, whatever it was running in before.

**Why.** A mode is an answer somebody gave while watching one piece of work, so it belongs to the
sitting it was chosen in. A restart is a new sitting with nobody present. An allow rule is a
decision about a session someone is sitting in front of; read here, one line in a file in the home
directory would let a session write unwatched, and that is what the bypass flag is named and
warned about for.

`verified-by: none`

<a id="BG-9"></a>
### BG-9: attaching draws the session as a foreground one, and detaching leaves it running

`bravebot attach <id>` connects the terminal to the running process of that session. It draws the
transcript, the held prompt if there is one, and the input box as a foreground session does, and
every prompt it puts is answered by the person there. A session that is `stopped` or `interrupted`
is started again in the mode every session opens in and attached to.

One terminal is attached at a time. A second `attach` is refused and names the session. Closing the
terminal, losing its connection or typing `/detach` ends the attachment and nothing
else: a turn that is running keeps running, and a held prompt stays held.

The channel between the terminal and the process is local to the machine. It is restricted to the
person's account and carries content with the label it was released under, so the terminal marks
quarantined bytes as it would in a foreground session. The attached terminal is handed what a
foreground terminal would be handed, and nothing the session holds beyond that.

A record a running background session holds is not resumed or continued by another process.
`--resume` and `--continue` name the session and say to attach. `--fork` copies the record as it
was last written, and the copy is a new session.

**Why.** If detaching ended the turn, closing a laptop would cancel work the person started with
`--bg` in order to keep. If a second process could resume the record, two processes would write one
conversation.

`verified-by: none`

<a id="BG-10"></a>
### BG-10: a reply is the next prompt and answers nothing that was asked

`bravebot reply <id> "text"` sends the text as the session's next prompt. The text is the person's
own typed line and it starts a turn as a typed prompt does. A session that is `stopped` is started
again first ([BG-8](#BG-8)).

A reply is refused while the session is `working` or `needs input`, and says to wait, or to attach.
It is refused for `interrupted` only after saying that the interrupted turn is not repeated
([BG-12](#BG-12)). It does not read standard input: a prompt that arrived on a pipe is untrusted
input and is not a line a person typed.

A reply never answers a held prompt, including a question the planner asked.

**Why.** An answer is to something put in front of the person who gives it. A reply is typed without
the prompt on screen, so it could be taken for an answer to a write or a run it never showed.

`verified-by: none`

<a id="BG-11"></a>
### BG-11: a person can stop a session, and there is one supervisor per account

`bravebot sessions stop <id>` stops a turn that is running, then ends the process and sets the state
to `stopped`. The record is kept and is resumable. No session is without a way to be stopped.

There is one supervisor per account. It is started by the first `--bg` or `/bg` and ends when it
owns no live process. A supervisor already running is found through a lock in `~/.bravebot/jobs/`,
and a second one is never started. `bravebot sessions` reads the roster and needs no supervisor.

**Why.** Work nobody is watching has to be stoppable by the person who started it. A supervisor that
stays after its last session is a standing process the person did not ask for.

`verified-by: none`

<a id="BG-12"></a>
### BG-12: a process that ends between turns is restarted, and one that ends in a turn is not repeated

A process that exits while the state is `idle` is restarted, at most three times in an hour, in the
mode every session opens in. After that its state is `stopped`. A process that exits while the state
is `working` or `needs input` is not restarted: its state is `interrupted`, the turn it was running
is not run again, and the prompt it held is gone.

The first turn after an `interrupted` session is a reply or an attach. The planner is told, by the
driver, that the earlier turn ended without finishing, and not what it had done.

**Why.** Running the turn again would repeat its writes and commands, and some of them are not safe
to repeat. The person who comes back sees the state and decides.

`verified-by: none`

<a id="BG-13"></a>
### BG-13: an idle session is stopped after an hour and wakes on the next reply

A process with no turn running, no held prompt, no attached terminal, no loop and no watch for an
hour is stopped, and its state is `stopped`. A reply or an attach starts it again from the record.

A loop or a watch does not survive its process, so a session holding one is not idle.

**Why.** A background session with nothing to do should not hold a model client, a language server
and memory for as long as the machine is on. Stopping a session with a loop would silently end the
loop.

`verified-by: none`

<a id="BG-14"></a>
### BG-14: a background session in a git repository works in a checkout of its own

`--bg` and `/bg`, run in a git repository, make a checkout for the session before its first turn, in
the way a delegate's checkout is made ([checkouts.md](checkouts.md)), and the session works in it:
its file tools reach it, and its programs start in it. The session keeps the checkout, the record
lists it, and a person removes it as they remove any other. A running session holds the checkout's
lock, so the opening sweep ([CHECKOUT-16](checkouts.md#CHECKOUT-16), which nothing yet builds)
does not take it.

Outside a git repository the session works in the directory it was started in. The list shows the
directory, and two background sessions started there write to the same files.

**Why.** Two sessions editing and building in one tree wait on each other's build directory and
report failures the other caused. A session that nobody is watching is the one whose edits the
person has not seen arrive in their tree.

`verified-by: none`

## What this changes in other specs

None of the following is true of a build that has no background sessions, and each is marked where
it is stated.

- **A delegate ends with the turn that started it.** That holds inside a background session
  unchanged: its turn does not answer while a delegate is working. What outlives the terminal is the
  session's process, and a turn that has not ended is not a delegate outliving one.
  [delegation.md](delegation.md) says so beside that clause.
- **A checkout is a delegate's.** A background session's checkout is made the same way and is the
  session's own. [checkouts.md](checkouts.md) says so in its scope.
- **A session ends with its terminal, and no process holds a record.** Both are statements about a
  session that is not a background one. [sessions.md](sessions.md) says so where it describes the
  record, and the state directory lists `jobs/` as proposed.
- **The info panel tells open sessions apart.** The list in [BG-5](#BG-5) tells the background ones
  apart and draws nothing the panel would not.
- **Where nobody can be asked, the answer is no** ([PROMPT-9](prompting.md#PROMPT-9)). Somebody can
  be asked of a background session, later, so it holds its prompts ([BG-7](#BG-7)) and declines none.

## Not decided

- **Whether attaching may change the mode.** [BG-8](#BG-8) fixes it for the process, so a session
  started asking stays asking, and a person who wants a session to write unasked has to start it
  from a foreground session already in that mode with `/bg`. Letting a mode change last until
  detach raises the question of what the session runs in after the person leaves.
- **Whether bypass is ever allowed.** Other tools refuse it until an interactive step accepts a
  disclaimer. Bravebot has no such step: the flag is the acceptance. [BG-8](#BG-8) refuses it.
- **Whether the checkout is made at the first edit rather than at the start.** A checkout at the
  start is made for sessions that never edit. One at the first edit moves the working directory
  during a session.
- **Whether `bravebot sessions` has an interactive view.** [BG-5](#BG-5) specifies lines. Replying
  from a row is not specified.

## Known costs

- **A standing process.** The supervisor and each session's process run for as long as a person
  leaves them. The supervisor is the only one that has no work to do, and it ends with its last
  session.
- **Asking is the only mode on the command line.** `--bg` cannot start a session that accepts edits,
  and a session stopped for idleness returns to asking. A person walking away from accept-edits work
  finds it waiting on the first write after a restart.
- **A session outside a repository shares its directory.** [BG-14](#BG-14) separates sessions only
  where git can.
- **Unix first.** The attach channel is a socket in a directory only its owner can enter. Where that
  cannot be restricted, [BG-1](#BG-1) refuses a background session.
- **The list says a prompt is held and not what it is.** A person must attach to see whether it is
  one they would approve. The alternative is drawing the prompt's subject in the list, which is the
  text [BG-5](#BG-5) keeps out.
