---
sidebar_position: 2
title: An OpenAI-compatible gateway
description: Name a gateway, the models it offers and where its credential lives, including a local Ollama that wants no key.
---

# Reaching an OpenAI-compatible gateway

A `provider` block names a gateway, the models it offers and where its credential lives. An entry
keyed `amazon-bedrock` names [an AWS account](bedrock.md#naming-more-than-three-models) instead, which is
reached by signing rather than by a token; everything below is about the other entries. The models
a gateway ends up with are offered in `/model` beside Brave's roster and any AWS tiers. It takes nothing away
from those rosters and does not move the default: what answers when nobody has chosen stays what it
was, and the conversation budget stays where it was too.

```json
{
  "provider": {
    "openrouter": {
      "name": "OpenRouter",
      "env": ["OPENROUTER_API_KEY"],
      "models": {
        "z-ai/glm-4.6": { "limit": { "context": 200000, "output": 8192 } }
      }
    }
  }
}
```

| Where | Field | What it holds |
|---|---|---|
| the key under `provider` | | the gateway's id, which is also what a picker row names it by |
| the entry | `name` | something friendlier to show than the id |
| | `env` | variable names that may hold the bearer token, tried in order |
| | `models` | the models to offer, keyed by the name the gateway knows each by |
| `options` | `baseURL` | where requests go |
| | `apiKey` | a token written into the file directly |
| a model | `limit.context` | that model's context window, in prompt tokens |
| | `options` | anything extra to put in the request body |

**The block is opencode's, field for field**, so one copied out of `opencode.json` works unedited.
Nothing is required that opencode does not require, and a field bravebot does not know is read past
rather than refused. opencode's `cost`, `modality` and `package` fields do nothing here.

### Where the requests go

**A gateway bravebot already knows an endpoint for needs no `baseURL`.** `openrouter` is the name it
knows today. Any other id needs one written down, and an entry with neither a known name nor a stated
endpoint configures no service. A stated `baseURL` always wins, so a known name stays usable against a
proxy or a private deployment.

The names it knows are compiled in, and nothing is fetched to resolve one. This value is where a
bearer credential gets sent, so a service that could decide it could redirect your token by answering
a request.

### Google Vertex AI

Vertex AI has an OpenAI-compatible endpoint that takes an API key. An entry keyed `google-vertex`
reaches it:

```json
{
  "provider": {
    "google-vertex": {
      "env": ["GOOGLE_API_KEY"],
      "options": { "project": "<your project id>", "location": "global" }
    }
  },
  "model": "google-vertex/google/gemini-2.5-flash"
}
```

The host is built from `project` and `location`, which is `global` where you state none, and a
`project` is required. One holding a character a project id cannot hold, such as `/` or `@`, configures
nothing rather than sending your key somewhere else. A stated `baseURL` still wins.

The key is read from the variable `env` names and sent in the `x-goog-api-key` header, which is the
only place Google reads it from. Export it as `GOOGLE_API_KEY=<your key>`, or name another variable in
`env`. A block naming no `env` and no `options.apiKey` sends no key, and Vertex AI refuses it.

With no block, exporting `GOOGLE_API_KEY` and `GOOGLE_CLOUD_PROJECT` is enough, and `VERTEX_LOCATION`
sets a location other than `global`. Both of the first two are needed, and a block replaces this. The
service is only asked once a model named `google-vertex/...` is chosen. `GOOGLE_API_KEY` is a name
other Google tools read as well, so the key you exported for one of them is what is sent here once
you choose such a model.

Vertex AI has no model listing a key can call, so `/model` offers a short list of Gemini models
built into bravebot, which `bravebot doctor` names. A block that lists `models` is offered those
instead. No preview model is on the built-in list, because Google withdraws previews without notice.
The list is what the `global` location serves, and another location may not serve all of it. A model
on the built-in list is named with the id in front, as `google-vertex/google/gemini-2.5-pro`, in
`--model` or the `model` key, and one off it is named the same way, as
`google-vertex/google/gemini-3-flash-preview`. Signing in with Google Cloud credentials instead of a
key is not supported.

### The credential

Name a variable in `env` and keep the token wherever you already keep secrets. Or store it with
bravebot:

```sh
bravebot auth login gateway openrouter
```

asks for the key with nothing shown as you type it, and keeps it in `~/.bravebot/gateway-keys.json`,
readable only by your account, under the block's id. [Signing in](../signing-in.md#gateway) has
the details. `options.apiKey` is read too, because it is opencode's field. Where more than one is
present a variable wins, then the stored key, then `options.apiKey`. A long-lived token in a
settings file is a token in a file people paste into issues.

A variable is read at the point a request needs it rather than once at startup, so exporting a new
one takes effect in a session already open. A stored key is read when a session starts, so one
stored while a session is open reaches the next one. A block that names somewhere for a credential
to live and finds nothing there is a stale or missing token, and its requests are refused with the
remedy named rather than sent. `bravebot doctor` says whether a credential was found, and whether it
was a stored one, and never what it was. It prints an `ends` line for the block too, naming the host
the token is presented to: that host is the only surface that revokes it, and deleting the value
from this file, unsetting the variable or running `bravebot auth logout gateway` ends this machine's
custody and leaves the token live there.

**A block naming no credential at all is a different statement, and a supported one.** No `env`, no
`options.apiKey` and no stored key is you saying this gateway wants none: its requests carry no
`authorization` header and its roster is asked for without one. `doctor` reports it as needing none
rather than as missing one. Deciding this by endpoint instead would refuse the same local service reached across a LAN or
through a reverse proxy, and a dummy `apiKey` would just teach people to write fake credentials into a
file they paste into issues.

### A local Ollama, or another gateway that wants no key

Ollama wants no API key, so its block names none:

```json
{
  "provider": {
    "ollama": {
      "name": "Ollama (local)",
      "options": { "baseURL": "http://localhost:11434/v1" }
    }
  },
  "model": "ollama/qwen3-coder:30b"
}
```

`baseURL` is written down because `ollama` is not one of the names an endpoint is
[compiled in](#where-the-requests-go) for. There is no `models` key, so Ollama is asked what it has
pulled and `/model` lists what came back. A first run with nothing configured offers to write this
block for you where Ollama is running ([Importing from Claude Code, opencode or
Ollama](../configuration.md#importing-from-claude-code-opencode-or-ollama)).

### Which models are offered

**A block that lists `models` is taken at its word**, in the order you wrote them, and costs no round
trip. That is what keeps a configured gateway working with no network, and is the way to pin a short
list out of a service offering hundreds.

**A block that lists none has the gateway asked**, except on [Google Vertex AI](#google-vertex-ai),
which has no listing to ask. That is the ordinary case rather than a mistake: opencode resolves its
roster from a registry it fetches, so the commonest block copied out of it names a credential and
nothing else. What your credential may reach is asked for first, and the service's full catalogue
answers only where a gateway does not offer the narrower question. Models that cannot call tools are
left out.

Nothing is capped. Ordering does that work instead: the model a session would use comes first and the
rest are sorted by name. A listing that cannot be fetched contributes nothing and takes nothing away
from the rest of the roster.

### Naming one

Where one model is reachable through more than one service, put the gateway's id in front of the name
to say which you mean:

```
openrouter/z-ai/glm-4.6
```

The name is split once, at the first slash, because most gateway names contain one. The id picks the
service and only the remainder is sent, the id being bravebot's own filing that no gateway has heard
of. A bare name your block lists still finds its gateway, so a choice already recorded by `/model`
keeps working.

### The context window

`limit.context` is optional. A model that states none is assumed to have 131,072 prompt tokens, the
same deliberately low figure a Bedrock tier gets and for the same reason: a budget above the real
window does not compact a conversation late, it stops compacting it at all. A window a gateway reports
is taken where the file stated none; a figure in the file outranks it. Following opencode, `limit`
needs `output` alongside `context` or it is not a `limit` and its figure is not read. `output` states
[how far a reply may run](../configuration.md#how-long-a-reply-may-run).

### What a model's `options` can and cannot do

Whatever you put there reaches the request body as it stands. Nothing parses it, knows what any of its
fields mean, or validates them, so a misspelled routing field is a request the gateway rejects, or
worse one it silently routes somewhere you did not intend.

It cannot replace what the turn itself built. The settings file names a destination, not what was
asked.
