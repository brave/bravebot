---
sidebar_position: 11
title: Choosing a model
description: Choose a model with /model, understand the offered model roster and keep the selected model across sessions.
---

# Choosing a model

```
/model
/model session
```

opens a picker on the model currently in use, as a panel in the middle of the screen. The panel opens
at once with a line saying the models are loading, and the rows appear when the endpoints have
answered. What you type meanwhile narrows them when they arrive, and Enter chooses nothing until
then. A listing that fails closes the picker, leaves the model as it was and says why. The list comes
from the endpoint rather than from a set compiled in, so it is whatever the backend actually offers
today. Google Vertex AI is the exception, having no listing to ask (see [Reaching an
OpenAI-compatible gateway](providers/openai-compatible.md#google-vertex-ai)). The choice made with `/model` is written to
`~/.bravebot/model`, so it outlives the session that made it and applies in every directory, except
where the file `--settings` names has a [`model`](configuration.md#model) key. A checkout's own settings cannot name a
model. A one-shot run reads the same record, so a script uses the model you picked
unless [`--model`](../reference/cli.md#--model-name) names another. A model your AWS account named
for a tier is written as that tier's word, `opus`, `sonnet` or `haiku`, so the record names whatever
the tier variable names when the ARN is replaced.

**`/model session` picks a model for this session only.** It opens the same picker, and Enter puts
the model in force for the running session without writing `~/.bravebot/model`, so the next session
opens on the model it would have had. The title says `this session only` and the first footer row
says what Enter and Tab do. Tab switches between this scope and the one `/model` opens, which records
the pick for every session, so you can change your mind before you press Enter. The info panel
marks a session-only pick with a row `this session only` under the model. `/clear`, `/handoff` and
`/branch` keep the pick, and `/resume` opens another session, which opens on the recorded pick. A
model named by a definition under `--agent` or by a skill for plan mode still outranks the pick, and
`/model session` opens no picker under a definition that names a model. A session started with
[`--model`](../reference/cli.md#--model-name) opens on that model, and a pick replaces it.

**Type to narrow the list rather than arrowing through it.** A search matches the name shown, the name
a request would carry and the service that answers, ignoring case and anywhere in any of them, and
every word you type has to match something. The cursor stays on the model it was on while the list
narrows, and falls to the first match once that model no longer matches. A search matching nothing
says so, and there is nothing to select while it does.

Rows are **grouped under the service that answers them**, one heading per service, and the heading of
the section you are scrolling through is held on the top line. Left folds the section the cursor is in
down to its heading and Right opens it again, and PageDown and PageUp move a screenful at a time.

The list is exactly what the endpoint returns, and nothing is added to it. With no choice made,
requests name `claude-3-haiku`. The model requested is not necessarily the model used: some entries
are weighted ensembles that resolve per request.

**The roster is curated for Brave Bot.** Requests to Brave's endpoint carry a header naming this
product, and the endpoint answers with the models chosen for it rather than the full Leo list. Leo's
roster is chosen for a chat assistant and a good fraction of it cannot call tools at all, which would
make an agent that can read and write nothing. A gateway is sent no such header, since what a
third-party service gets is the shape it documents.

If you wrote `automatic` or `automatic-bravebot` anywhere, it is rewritten to the default model
before a request carries it, whether it came from a settings file, an exported variable or a choice
recorded by an older version. The rewrite is one-way: those names cannot be requested.

The list is drawn for a person, and the names in it never reach a model. What you picked becomes the
`model` routing field of later requests.

**A prompt file can open the picker on a model.** A file in `~/.bravebot/prompts` whose front matter
has `model: <name>` puts its text in the box and opens this picker with the cursor on the row whose
key or name is that model. Nothing changes until you press Enter on a row, and Escape keeps the model
in use. The picker is the one `/model` opens, so Enter records the pick for every session, and Tab
switches it to this session only. A name the list lacks is said. Under `--agent` with a definition that names a model the
picker does not open. See [Prompt files](../reference/commands.md#prompt-files).

With an AWS account or a gateway configured the picker offers those models alongside this list rather
than instead of it, each under its own heading. See
[Reaching a model through AWS Bedrock](providers/bedrock.md) and
[Reaching an OpenAI-compatible gateway](providers/openai-compatible.md).
