---
sidebar_position: 12
title: Choosing how hard to think
description: Choose reasoning effort with /effort, understand model support and service-specific limits, and keep the choice across sessions.
---

# Choosing how hard to think

```
/effort
/effort session
```

opens a picker of five levels, cheapest first (`low`, `medium`, `high`, `xhigh` and `max`), above a
row for asking for no level at all. `/effort high` takes one without opening the panel. The choice made
with `/effort` is written to `~/.bravebot/effort`, so it outlives the session and applies in every
directory whose own settings name no [`effort`](configuration.md#effort), and `/status` reports it beside
the model.

**`/effort session` chooses a level for this session only.** It opens the same picker, and
`/effort session high` takes a level without opening it. Either puts the level in force for the running
session without writing `~/.bravebot/effort`, so the next session opens on the level it would have had.
The title of the picker says which scope it is in, `effort · every session` after `/effort` and
`effort · this session only` after `/effort session`, and the first footer row says what Enter and Tab
do. Tab switches between the two scopes, so you can change your mind before you press Enter. The row for
no level, taken in the this-session scope, puts no level in force for this session and leaves the record
as it is. A level chosen this way outranks the `effort` key in every settings file and
[`--effort`](../reference/cli.md#--effort-level) for this session. Where it is the level being sent, the
info panel marks it with a row `this session only` under the effort, and `/status` says so. `/clear`,
`/handoff` and `/branch` keep it. `/resume` opens another session, which opens on the level a new session
would have. `session` is the only word `/effort` takes besides the
levels, and it is matched whole and in lower case. A word after it that names no level changes nothing
and says so.

**On a ChatGPT account a level applies from the next session, or from `/clear`.** A session on a model
of a [ChatGPT account](providers/chatgpt.md) is sent the level it began with in every request that asks
for one, because OpenAI's prompt cache was seen to be per level: changing the level inside a session cost
one miss at each change, and requests hit again after. A level chosen later with `/effort` or
`/effort session`, or named by a skill, changes nothing for that session. The terminal says so in a
sentence that names both levels, and `/status` names the level sent and the level you chose where they
differ. `/clear` begins a session at the level you chose. A level chosen with `/effort` applies from the
next session, and the sentence says so. A level chosen with `/effort session` applies from `/clear` and
is not recorded, so no later session opens on it, and the sentence says that instead. A request that
asks for no level, such as a summary or a goal check, sends none, and the desktop window offers no way to
choose a level, so its sessions send none. A model of the account that refuses a level is sent none from
then on, as any model is. A level chosen on any other service applies at once.

**An xAI account is sent no level.** A model of the [xAI account](providers/xai.md) reads none, because the
service was not asked with one, so a level chosen in a session does nothing for it, and the terminal says so
where a level is in force.

**Nothing infers a level.** Until you choose one the request carries no such field at all and each
service applies its own default. Taking the row for no level in the every-session scope removes the record rather than writing an
empty one, which puts you back where you were before you ever chose. A word bravebot does not define
changes nothing and says so.

The picker is not the only route: the [`effort`](configuration.md#effort) key names a level in a settings file, which
is how a project asks for one and how an unattended machine has one at all, and
[`--effort`](../reference/cli.md#--effort-level) names one for a single one-shot run.

**A level goes only where the roster says it is read.** Reasoning is two parameters rather than one on
a gateway, so a model can reason and still not read a level sent this way. Where the listing describing
your model says it reads none, no level is sent and `/status` says so. The choice is kept either way
and applies again the moment you pick a model that reads one. A model no listing described is not a
model stated to read nothing: a name from your settings file, a roster reporting no parameters, and a
listing that could not be fetched all leave the level to go out and be judged at the far end. A row
that does advertise the parameter says it reads one without saying which words it accepts, so a model
may reject or silently round a level it does not know.

**Where nothing describes the model, the answer arrives as a refusal.** Neither AWS Bedrock nor a
settings block that names its own models says which parameters a model takes, so a level goes out to
be judged. A model that refuses the field is asked again without it and is sent no level for the rest
of the session, and the interface reports it as reading none rather than going on showing your level
as in force. What one model refuses says nothing about another, on Bedrock or on the same gateway. A
request refused with the level already gone settles nothing and is not remembered.

A level you wrote into a model's own [`options`](providers/openai-compatible.md#what-a-models-options-can-and-cannot-do) is not
given up this way. That field is carried into the request as it stands, so it fills the level again
after the one you chose in the interface has been withdrawn, and a service that does not take it
refuses the request as it would refuse any other option it does not know.

:::caution
**The Brave endpoint accepts the level and discards it.** A nonsense value is answered exactly as a
real one is, and a model that reports a reasoning-token count reports the same count whatever level
was asked for. A level chosen against a Brave-served model is carried, sent and dropped, while the
interface goes on reporting it as in force. Bedrock and gateways are unaffected.
:::
