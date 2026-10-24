---
sidebar_position: 12
title: Choosing how hard to think
description: Pick an effort level with /effort, which models read it, and where the choice is kept.
---

# Choosing how hard to think

```
/effort
```

opens a picker of five levels, cheapest first (`low`, `medium`, `high`, `xhigh` and `max`), above a
row for asking for no level at all. `/effort high` takes one without opening the panel. The choice is
written to `~/.bravebot/effort`, so it outlives the session and applies in every directory whose own
settings name no [`effort`](configuration.md#effort), and `/status` reports it beside the model.

**On a ChatGPT account the level applies from the next session.** A session on a model of a
[ChatGPT account](providers/chatgpt.md) is sent the level it began with in every request that asks for
one, because the service keeps the prompt cache of a session only while its level stays the same. A level
chosen later with `/effort`, or named by a skill, changes nothing for that session. The terminal says so
in a sentence that names both levels, `/status` names the level sent and the level you chose where they
differ, and `/clear` begins a session at the level you chose. A request that asks for no level, such as a
summary or a goal check, sends none, and the desktop window offers no way to choose a level, so its
sessions send none. A model of the account that refuses a level is sent none from then on, as any model
is. A level chosen on any other service applies at once.

**Nothing infers a level.** Until you choose one the request carries no such field at all and each
service applies its own default. Taking the row for no level removes the record rather than writing an
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
