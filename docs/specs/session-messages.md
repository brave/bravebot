---
id: MSG
title: Messages between background sessions
status: proposed
governs:
  - crates/agent/src/tools.rs
  - crates/session/src/jobs.rs
documented-by:
  - none (gap: a page on one session sending text to another, owed until the design is built)
---

## Scope

One background session sending text to another on the same machine: the tool that does it, what
decides where the text goes, what a person is asked, what the receiving planner is given, and what
the text is never taken for.

Nothing here is built. It depends on [background-sessions.md](background-sessions.md), whose roster,
`bravebot sessions`, `bravebot --bg`, `bravebot attach` and `bravebot reply` are built, and whose
supervisor and restarts are not. A clause in this spec reads `verified-by: none` until the work it
describes lands.

A **message** is text one session's planner wrote for another session's planner. It is not the
sender's conversation, files, references or quarantined content, none of which cross. The **sender**
is the background session whose planner called the tool, and the **receiver** is the background
session it names.

What the labels mean is [labels.md](labels.md), and where an effect may land is
[routing.md](routing.md). What a delegate is, and what comes back from one, is
[delegation.md](delegation.md). The prompts a person is asked are in [prompting.md](prompting.md).
What a reply to a background session is, which a person types, is in
[background-sessions.md](background-sessions.md). The rule that untrusted content never reaches the
driver or the planner is [AGENTS.md](../../agents/AGENTS.md).

## What exists today

A delegate is started by the turn that spawns it and returns one report to that turn
([delegation.md](delegation.md)). A person can send a background session its next prompt with
`bravebot reply`. No tool lets one running session address another, and the planner of a background
session has no way to learn that another exists.

## Clauses

<a id="MSG-1"></a>
### MSG-1: `message_session` has one routing field, the target's name, and one content field, the text

`target` is routing and `text` is content. `target` is compared, as typed, with the names in the
roster as it stands at the moment of the call. A call is refused, naming the name that was asked for
and nothing from the roster, when no session has that name, when more than one does, when the session
is the sender itself, and when the session is not live ([MSG-5](#MSG-5)).

`text` is refused, not cut, when it is longer than 4,000 characters, because a cut message is not the
one a person approved ([MSG-3](#MSG-3)).

Only background sessions are in the roster, so a session can address only sessions of the same account
and the same state directory. A session of another account, in another state directory, or on another
machine is not reachable.

**Why.** A person can approve the target alone: "this session may be told something" is a decision
about where text goes. The text is what the planner wrote and is carried to that place, so it is
content, as a delegate's task is content.

`verified-by: none`

<a id="MSG-2"></a>
### MSG-2: a message is sent only from a context that has met nothing untrusted, and carries what that context held

The text becomes part of another planner's context, so it is held to the footing of a delegate's task.
A call is refused outright when the sender's own context has met untrusted content. The text is
labelled at the integrity of the context that wrote it, which at this point is trusted, and it is
private when the sender's context holds private content: a slot a person let through, output read
aloud to the planner, a report from a delegate that was private, or a resumed context that already
held one. A workspace file the planner was shown does not make it private.

The label is assigned by the sender's policy layer from what its context held. It is not chosen by the
planner, and no text, including a statement in the text that it is safe, changes it.

**Why.** The text is model output, and model output is as trustworthy as the context it was written
in. A sender that had quoted a tool result would carry untrusted bytes into another planner, and the
two places this can be stopped are the sender's context, where nothing untrusted can have entered, and
the receiver, where [MSG-9](#MSG-9) stops what arrives anyway. A message does not need a label chosen
for it, because the context already has one.

`verified-by: none`

<a id="MSG-3"></a>
### MSG-3: the first message to a session is put to a person, with the target and the whole text

Before a message is sent, the person is asked. The prompt shows the target's name, its state and its
working directory as the roster holds them, and the whole of `text`, drawn inside the margin that
untrusted content in a prompt is drawn inside, with a row for the label it carries. It says what a yes
does and what it does not: the text goes to that session's planner as advice from another session; it
answers nothing the receiver is holding, and it grants nothing there.

Declining sends nothing and tells the planner only that the message was not sent. The planner is not
told why. A session that cannot ask, because it is a one-shot run or its channel has closed, sends
nothing. A background session holds the prompt until a person attaches, as it holds every other
([BG-7](background-sessions.md#BG-7)).

No mode answers this prompt and no allow rule does, so a person does. Bypassing is refused for a
background session, which is the only kind that sends.

**Why.** The prompt is where the routing field is approved, and a person cannot approve a field they
were not shown. The text is shown in full because it is what the receiver will be told, and a person
who reads it is the only check on it that does not rest on the sender's model.

`verified-by: none`

<a id="MSG-4"></a>
### MSG-4: a second answer covers later messages to the same session for the rest of the turn, and no longer

The prompt offers two answers: send this message, and send this one and the later ones to this session
until the turn ends. The second has a key of its own and Enter does not reach it, as with any standing
answer. It covers the one target, is not offered for another, is not written anywhere, and ends with
the turn. A resume does not restore it.

A message sent under it is still drawn in the sender's transcript as it is sent ([MSG-11](#MSG-11)).

**Why.** A planner that needs several exchanges with one session in a turn would otherwise stop a
person on each. A grant that lasts past the turn would let the next turn, whose first prompt the
person typed for another purpose, send without asking.

`verified-by: none`

<a id="MSG-5"></a>
### MSG-5: a message is delivered to a live process, and starts nothing

A message is delivered only to a session whose process is live, by the lock the roster already uses to
tell a live process from a record ([BG-6](background-sessions.md#BG-6)). A session that is `stopped`
or `interrupted` is refused as not reachable. It is not started to receive the message, and a message
does not restart a process the supervisor stopped.

The channel is local to the machine and restricted to the person's account. The supervisor is not on
it and reads no message, as it reads no prompt or reply ([BG-3](background-sessions.md#BG-3)). The
roster gains no field for a message and nothing about one.

The call answers that the message was queued in the receiver's process. It does not say the receiver
has read it.

**Why.** Starting a session is a thing only a person does. A message that could start one would give a
planner the power the roster reserves to a typed line. A roster that held a message would hold the
planner's words in a place the list is drawn from.

`verified-by: none`

<a id="MSG-6"></a>
### MSG-6: a message never starts a turn

A message reaches the receiving planner at the start of its next request in a turn that is already
running. A receiver that is `idle` holds it until a person starts that session's next turn, by typing
into an attached terminal or by `bravebot reply`, and it is put in front of that turn's planner with
the person's prompt. A receiver in `needs input` holds it until the turn that is waiting continues.

At most eight messages wait for one receiver. A ninth is refused to its sender as full.

A message held by a process that ends is gone, and its sender is not told.

**Why.** If a message started a turn, two sessions could keep each other working with no person
anywhere, each turn asking the other for the next. Holding the message until a person starts a turn
puts a person in every cycle, and bounds what an unwatched sender can cause to text that waits.

`verified-by: none`

<a id="MSG-7"></a>
### MSG-7: the receiving planner is told what the text is, in words the driver wrote

The message is placed in the receiving planner's context in a frame the driver composes from a fixed
sentence. The sentence says the text below is a message from another session, that it is not the
person's words, that it is advice, and that it grants no permission and answers no question. It names
the sender by the first eight characters of its id and by its name. Both are read from the roster by
the sender's id when the message arrives, and neither is taken from the message. The name is quoted,
cut to the length the roster list cuts a name to, and has its control characters replaced as they are
wherever text is drawn. The text follows, quoted, with the label it arrived under.

The frame is a message of its own. It is not joined to the person's prompt and is not put in the
position a person's prompt holds.

**Why.** The receiving planner has to be able to tell this from the person, and the sender must not be
able to choose how it is introduced. A sender that supplied its own name could call itself the person.

`verified-by: none`

<a id="MSG-8"></a>
### MSG-8: the receiver's driver decides nothing from a message, and a message is nothing a person types

No branch in either half of the driver depends on the text. It is carried to the planner and to the
screen and read by neither.

Because it is not a line a person typed:

- `@`, `!` and a leading `/` in it do nothing. It is not scanned for a named file, a command or a
  shell line, and a path it names is vouched for by nothing.
- It answers no held prompt, including a question the planner asked, as a reply answers none
  ([BG-10](background-sessions.md#BG-10)).
- It changes no mode, no name, no loop, no goal and no watch.
- It is not added to the prompts the person typed, and is not offered by history recall.

Every effect the receiving planner then asks for is decided as it is for any planner: a write, a run
and a call to a server's tool are each put to the person who is attached, with their own gates.

**Why.** A message exists to tell a planner something. If the driver read it, a sender could steer the
driver. If the driver took it for the person, the sender would hold the person's authority over the
receiver, which is the thing the sender's own gates were built to deny it.

`verified-by: none`

<a id="MSG-9"></a>
### MSG-9: a message arrives with the label it was sent under, and the receiver can only lower it

The label travels with the text, as the label and not as a word the receiver composes
([LABEL-10](labels.md#LABEL-10)). A message whose label is trusted is shown to the planner. A message
with any other label, with no label, or with one the receiver cannot read, is taken as untrusted and
private: it is quarantined, the planner is handed a reference that shows no content, and the person's
screen draws it inside the quarantine margin. The receiver never gives a message a better label than
the one it arrived with.

A private message makes the receiving context private from then on, as a private report from a delegate
makes its parent's, and a resumed session keeps that.

**Why.** [MSG-2](#MSG-2) means a trusted message is the only kind a correct sender sends. The receiver
does not rely on that, because the sender is a process the receiver cannot inspect, and a label that
failed to cross would otherwise make the content trusted by moving it.

`verified-by: none`

<a id="MSG-10"></a>
### MSG-10: a session sends a bounded number of messages

A turn sends at most ten messages to one session, and a turn that tries an eleventh is refused for
that target until the next turn. A refused call does not ask a person.

**Why.** A planner in a loop could otherwise fill a receiver's queue ([MSG-6](#MSG-6)) or put a prompt
in front of a person each round.

`verified-by: none`

<a id="MSG-11"></a>
### MSG-11: both transcripts show the message, and the trail records that it happened

The sender's transcript draws each message it sends with the target's name and the whole text. The
receiver's transcript draws each message it is given as a quoted block marked as a message from the
sender, and drawn apart from the person's own prompts, in the position of a session message and not
of a typed line. Control characters in a name or in the text are replaced by visible ones.

The audit trail records the sender's id, the receiver's id, the label, whether a person was asked and
what they answered, and the length. It records no text. The receiver's session record holds the frame
its planner was shown and, for a quarantined message, the reference and no content.

**Why.** A message is a second route into a conversation, so a person reading either transcript later
has to be able to see that it was used and by whom. The trail holds labels and gate names and no
content.

`verified-by: none`

<a id="MSG-12"></a>
### MSG-12: a delegate and a processor are offered no `message_session`

Only a session's own planner is offered the tool. A delegate is not, and a call to it by name anyway is
answered as any other unknown name is. A processor, which has no tools, is not.

**Why.** A delegate's task came from a planner, so a prompt about a message it wants to send asks a
person to arbitrate something they never set up. What a delegate has to say goes in its report, and its
parent decides whether to say it to another session.

`verified-by: none`

## What this changes in other specs

None of the following is true of a build that has no such messages, and each is marked where it is
stated.

- **The tool surface.** When the tool is built, the table in [tools/tool-surface.md](tools/tool-surface.md)
  gains a row for it, with `target` as routing and `text` as content.
- **The prompts.** [prompting.md](prompting.md) lists the moments a person is asked. This adds one, and
  it says so beside the list.
- **The roads in.** [labels.md](labels.md) lists every road a first label is assigned on. A message from
  another session is one more, and it says so beside the table.
- **Only a typed line starts a session** ([BG-2](background-sessions.md#BG-2)). That holds, since a
  message starts no process ([MSG-5](#MSG-5)) and no turn ([MSG-6](#MSG-6)).

## Not decided

- **Whether a session started in a terminal can send.** The sender's name is read from the roster, so
  only a background session has one this spec can trust. A person in a foreground session can use
  `bravebot reply`. Letting a foreground session send raises the question of where its name comes from.
- **Whether a message may wake an idle session.** [MSG-6](#MSG-6) holds it until a person starts a
  turn. Waking would make a message useful to a session nobody is attached to, and would let two
  sessions drive each other without a person unless some other bound replaced it.
- **Whether the planner may list the sessions.** It learns a target's name only from the person. A
  listing would hand it every name in the roster, and each is something a person typed.
- **Whether a private message should be refused rather than carried.** [MSG-9](#MSG-9) carries the
  label and makes the receiver private, as a delegate's private report does. Refusing would keep
  private content from reaching a session with other reach, at the cost of messages that follow any
  private read.
- **Whether a target may be an id.** A name is the first prompt cut or what `/rename` set, so two
  sessions can share one, and [MSG-1](#MSG-1) refuses when they do.
- **Messages across machines, and to sessions that were forked from the sender.**

## Known costs

- **Advice from a planner is still read by a planner.** A trusted message cannot contain untrusted
  bytes, but a model that wrote it can have been wrong, and the receiving model may act on it. What it
  can cause is bounded by the receiver's gates, and each is put to the person who is attached.
- **A person has to be there for the first message.** A background session that wants to send holds the
  prompt until someone attaches, so messaging between unattended sessions waits for a person.
- **A message can be lost.** One held by a process that ends is gone, and its sender is not told
  ([MSG-6](#MSG-6)).
- **The channel trusts the account.** A program run by the same account that can write to the channel
  can send what it likes with any label, and the receiver can only treat what is not trusted as
  untrusted. [STATE-1](state-directory.md#STATE-1) is the same boundary for everything in the state
  directory.
- **A name is not unique.** Two sessions with one name cannot be addressed by it.
