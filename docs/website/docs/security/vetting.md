---
sidebar_position: 5
title: Vetting quarantined content
description: A second model reads quarantined content and reports what it found, so you have an opinion in front of you before you decide.
---

# Vetting quarantined content

Content nobody vouched for is **quarantined**: it is held in a slot, and the planner is handed a
reference to that slot rather than the bytes. A fetched page, what a program printed, a file outside
any [trusted directory](trust.md): the planner can carry these and write them somewhere, and it cannot
read them.

Every way out of quarantine ends at you answering a prompt. A **check** is what puts a second opinion
beside the bytes before you answer, so the question is not the first time anything has looked at what
the content holds.

## The three ways out

| The prompt | What a yes does |
|---|---|
| `vet_content` | promotes one slot's bytes once |
| `read_output` | releases what a program printed to the planner |
| the vouch offer on a read | writes a [trust rule](trust.md) about the path |

Everything else stays quarantined for the life of the session.

The single-use promotion exists partly for the friction. Wanting the agent to use a fetched page
otherwise leaves two moves: read every byte of it yourself, or vouch for a whole path, which is very
much larger. This covers one slot's bytes, once, and leaves nothing behind.

## What a check is

A second model call that holds nothing:

| | |
|---|---|
| Tools | none, and the request carries no tool list at all |
| Memory | none: the messages are built from nothing each time |
| Conversation | one request, one reply, no loop to steer |
| Reads | one piece of content, fixed before the call |
| Writes | **nothing**: no slot, no reference, no destination of any kind |

A [processor](../how-it-works.md) can mint one slot. A check mints none, ever, and holds no field that
could name a place for a result to go. The only things that come back are one word from a fixed set and
free text for you to read.

The content is enclosed as a single JSON string literal, so it occupies one physical line whatever it
holds and a newline inside it is written as an escape. That encoding is the containment: the content
may spell the block markers, but it cannot put one at the start of a line, because it cannot produce a
line.

Unlike a processor, which is asked to leave an injection attempt out of its output, a check exists to
say so. Where it cannot tell, it warns.

## The planner can ask for one

`vet_content` names one reference and says in a sentence what the planner expects it to hold. The
reference is the only field that decides anything, and you are shown the bytes behind it, so the field
you approve is the field the call turns on.

That sentence is drawn on the prompt so you can see why the planner wants this, and it is sent to the
check so a page can be judged against what it was supposed to be. It must not be private: your own data
must not become another model's prompt. Where the content came from is said in Brave Bot's words, as a
path, a URL or a command, because a reference name means something to the planner and nothing to you.

It refuses a reference to nothing, a picture (a check reads text), a private sentence, and a call from a
delegate. A reference to a file nothing has read yet is opened rather than refused, since naming one is
the ordinary way for the planner to ask about a file it may not read.

A delegate is not offered it because the prompt would belong to you, about content you never asked to
see, in the middle of work you are not reading.

## Every promoting prompt runs a check first

A check runs before the prompt is drawn, not after it is answered, on all three routes:

| The prompt | What the check reads |
|---|---|
| `vet_content` | the slot the planner named |
| `read_output` | the slot the planner named |
| the vouch offer on a read | **the whole file**, not the preview |

The vouch offer reads the whole file because it shows you the head of one and a yes grants all of it, so
a check over the preview would report on the part an injection attempt has the least reason to be in. The
check reads what the answer covers.

All three, because otherwise the quickest way past a check is to ask for the prompt that does not run
one. The one exemption is [bypassing mode](permissions.md), which draws no prompt at all: there a check
would be a model call whose word nobody reads, so the verdict is filled in as inconclusive, which claims
nothing.

## What comes back

A reply states `safe` or `unsafe`. Any other word, a reply stating no verdict, a truncated reply, a
timeout and a backend error are all **inconclusive**, which says nothing about the content. It fails
closed on every path: a check that half worked lands where a check that did not run lands.

The three outcomes are told apart on your screen. "This looks like an attempt to give instructions" and
"nothing looked at this" are different facts about different risks.

## A verdict is advice

Unless you have turned auto-vetting on, **the only thing a verdict decides is which prompt you read.**
The bytes are in front of you whatever the check said, the keys that answer are the same whatever it
said, and the answer is yours. Safe promotes nothing by itself, and unsafe withholds nothing.

The one key a verdict does decide is the one that answers no question: the standing key offering to turn
auto-vetting on, which appears only where the check completed and found nothing.

What the check says reaches you and no model. Neither the verdict word nor the sentence reaches the
planner, whichever way you answer, and neither reaches a second check. The sentence is free text about
content an attacker may own, so it is untrusted and private like the content, and it is drawn inside the
margin a prompt puts round anything nobody vouched for. The [audit trail](audit-trail.md) records the
word and never the sentence.

## What promoting does, and does not, grant

Promoting is an assertion about one slot's bytes:

- **One slot, once.** The endorsement names that exact slot and cannot be replayed. An approval given to
  a different question is not one of these: approving a read of what a program printed promotes nothing.
- **No rule is written.** Nothing reaches the trust map. A later read of the same file makes a new slot
  and is quarantined exactly as before, and the same content asked about again asks again.
- **Confidentiality is never lowered.** What comes back is private. The bytes may have come out of your
  workspace, and nothing about a check or about you reading them makes them fit to leave the machine or
  to become a field that decides where something lands. **Vetting unlocks no egress.**

## Auto-vetting is off until you turn it on

With it on, a check that completed and found nothing answers in your place at the two prompts that
promote one slot's bytes: no prompt is drawn and the bytes reach the planner. Three routes turn it on,
differing in how long the answer lasts:

| Route | Read from | Lasts |
|---|---|---|
| `--vet` | the [command line](../reference/cli.md) | this run |
| `~/.bravebot/vetting`, one word | your own directory, written by the standing key at a vetting prompt | until you change it |
| `"vetting": { "auto": true }` | `~/.bravebot/settings.json`, the **home layer only** | every session |

Any one of them is enough. `--vet` outranks both of the others, a recorded `off` included, because it is
the narrowest in time. A choice recorded in your own directory outranks the settings key, so turning the
mode off is written to that file rather than the file being removed, and a settings file cannot turn it
back on for you tomorrow. There is no flag the other way.

Nothing writes your settings file. The standing key writes `~/.bravebot/vetting` instead.

**The settings key is read from the home layer and no other.** A project `.bravebot/settings.json`
naming it, a machine-local one, and a file the command line named are each reported by `doctor` and not
obeyed. Every other name in those files decides where a request goes or how the interface behaves; this
one decides whether you are asked before content nobody vouched for reaches the planner, so a line in a
checkout could turn the asking off for whoever opened it. See
[configuration](../customize/configuration.md) for how the layers work.

A session that leaves nothing behind reads no recorded choice. It reads the model and the theme, because
those decide what it looks like; this decides whether somebody is asked.

**A session that opened with the mode on says so, and goes on saying so.** Once at the top of the
transcript, before the first slot can reach it, and in `/status` for the rest of the session with the
file it is kept in named. A note scrolls away, and the one thing you cannot read off a transcript is a
question that was never put. A session that is asking says nothing, since a line reporting the ordinary
state is a line people learn to skim.

### What it covers, and what it does not

| The prompt | With the mode on |
|---|---|
| `vet_content` | a safe verdict answers |
| `read_output` | a safe verdict answers |
| the vouch offer on a read | **still asks** |

Every other verdict falls back to the prompt, carrying the banner it would have carried anyway. Unsafe
and a check that did not complete are still told apart on the screen.

The line falls on the shape of the grant, not on which tool produced the bytes. Both promotions cover one
slot's bytes once and leave nothing behind, so what a verdict can buy is bounded by a single slot either
way. A trust rule is different in kind, being a standing decision about a whole path rather than about
bytes in front of a reader, and it is the one grant the mode does not touch.

The route the mode covers most often in practice is the output prompt, since a run's output is quarantined
by default. That is the reason the mode reaches it, and equally the reason the mode is off until you turn
it on.

What the mode never decides is anything but who answers: the slot is the planner's choice either way, the
content comes back private either way, the endorsement is single-use either way, and no trust rule is
written either way. Which of the two released the bytes is recorded, because a trail crediting a person
who was never shown them would be the one record a reader cannot check.

## Known costs

- **A quarantined slot's contents reach the backend a second time.** A check is a model call, so asking
  about a page sends it where the page would only have gone if a processor had been asked about it. The
  reader holds nothing, but the call happens whether or not you then approve.
- **A read of a quarantined file now sends the file, before anybody has agreed to anything.** The check in
  front of the vouch prompt reads the whole file, so reading a path nobody vouched for puts those bytes on
  the wire even where you then say no. Refusing keeps the bytes out of the planner rather than off the
  wire. It is bounded by the offer being made once per path per turn, never for a picture or a directory,
  and by the checking conversation holding no tools, no memory and no destination.
- **The check is a model, so it is wrong sometimes, in both directions.** A safe verdict on content that
  is an attack draws the quiet banner under bytes you still read; an unsafe verdict on an ordinary page
  draws the warning under bytes that are fine. Neither changes what approving does.
- **A plausible sentence may persuade you to skim.** The check's own words are attacker reachable and can
  lie, and nothing holds a sentence against the content it describes. What keeps it from deciding anything
  is that the bytes are on the same screen.
- **With auto-vetting on, the bytes are on no screen at all.** On the routes it covers nobody reads the
  content, and a model's word is the whole of what stood between the planner's context and a fetched page.
  That is the mode working as asked rather than a flaw in it, and it is why it is off by default and why
  the key that turns it on is not readable from a checkout.
