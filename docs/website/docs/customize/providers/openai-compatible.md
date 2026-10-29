---
sidebar_position: 6
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
| | `contextWindow` | the context window of every model that states none, in prompt tokens (bravebot's own field) |
| | `maxRetries` | how many times a request is sent again after a failure worth another attempt, 2 where unstated (bravebot's own field) |
| | `idleTimeoutMs` | how long a reply may go quiet before it is given up on, in milliseconds, 120000 where unstated (bravebot's own field) |
| | `apiKey` | a token written into the file directly |
| | `headers` | extra request headers, keyed by header name |
| a model | `limit.context` | that model's context window, in prompt tokens |
| | `options` | anything extra to put in the request body |

**The block is opencode's, field for field**, so one copied out of `opencode.json` works unedited.
Nothing is required that opencode does not require, and a field bravebot does not know is read past
rather than refused. opencode's `cost`, `modality` and `package` fields do nothing here.

### Where the requests go

**A gateway bravebot already knows an endpoint for needs no `baseURL`.** [`openrouter`](openrouter.md)
and [`openai`](openai.md) are the names it knows today. Any other id needs one written down, and an entry with neither a known name
nor a stated endpoint configures no service. A stated `baseURL` always wins, so a known name stays
usable against a proxy or a private deployment. A known name supplies the endpoint and no credential:
the block still says where its credential lives, as [below](#the-credential). What is different at
`api.openai.com` follows the host a request goes to, whatever the entry is called, and is on the
[OpenAI page](openai.md).

The names it knows are compiled in, and nothing is fetched to resolve one. This value is where a
bearer credential gets sent, so a service that could decide it could redirect your token by answering
a request.

### Extra request headers

`options.headers` adds headers to every request this gateway answers, including the one that asks
for its model list. A gateway in front of several services usually routes on one: a tenant id, a
routing key, a second credential.

```json
{
  "provider": {
    "acme-gateway": {
      "env": ["ACME_API_KEY"],
      "options": {
        "baseURL": "https://gateway.acme.internal/v1",
        "headers": {
          "x-tenant-id": "platform-eng",
          "x-acme-route": "{env:ACME_ROUTE}"
        }
      }
    }
  }
}
```

A value written as `{env:NAME}` on its own is read from that variable when a request needs it, the
way a credential is, so exporting a new one takes effect in a session already open and the value
stays out of a file people paste into issues. A variable holding nothing sends no header. Braces
inside a longer value are refused rather than substituted.

Header values are treated as credentials: they are in no trace, no error text, and nothing
`doctor` prints.

**Some names are refused, and the rest of the block still goes.** `host`, `content-length`,
`content-type`, `transfer-encoding` and `connection` are the request's own framing. So is the
header your credential goes in, `authorization` or `x-goog-api-key` on a `google-vertex` entry,
where the entry names a credential at all; an entry naming none may set `authorization` itself. A
Bedrock entry takes no headers from here, because AWS signs over the headers it sends.

`bravebot doctor` lists every refused header under the gateway block it was written on, with the
name as you wrote it and which of those reasons refused it, so there is a name to go and edit. The
value is not printed.

**This is read from your own settings file, the file `--settings` names, and a managed layer only.**
A `provider` block in a checkout's `.bravebot/settings.json` or `settings.local.json` is ignored
whole and `bravebot doctor` names the file. A header is where a request goes as much as `baseURL`
is: a checkout that could add one could send your credential and your context somewhere you did not
agree to.

### OpenAI

[Reaching OpenAI with an API key](openai.md) says how an entry keyed `openai` reaches OpenAI, which
models it offers, and how an entry reaches the Responses API.

### Google Vertex AI

[Reaching Google Vertex AI](google-vertex.md) says how an entry keyed `google-vertex`
reaches Vertex AI with or without a project, and what express mode is.

### The credential

Name a variable in `env` and keep the token wherever you already keep secrets. Or store it with
bravebot:

```sh
bravebot auth login gateway openrouter
```

asks for the key with nothing shown as you type it, and keeps it in `~/.bravebot/gateway-keys.json`,
readable only by your account, under the block's id. `openrouter`, `openai` and `anthropic` need no
block first: for one that no block names, the sign-in adds an entry that holds no field. [Signing
in](../signing-in.md#gateway) has the details. `options.apiKey` is read too, because it is
opencode's field. Where more than one is present a variable wins, then the stored key, then
`options.apiKey`. A long-lived token in a settings file is a token in a file people paste into
issues.

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
block for you where Ollama is running ([Importing from Claude Code, opencode or a local model
server](../importing.md)).

LM Studio and llama-server want no key either. [Ollama](ollama.md), [LM Studio](lm-studio.md) and
[llama-server](llama-server.md) each have a page that says how the first run finds the server, what
its block looks like and what to state for its context window.

### Which models are offered

**A block that lists `models` is taken at its word**, in the order you wrote them, and costs no round
trip. That is what keeps a configured gateway working with no network, and is the way to pin a short
list out of a service offering hundreds.

**A block that lists none has the gateway asked**, except on [Google Vertex AI](google-vertex.md),
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
is taken where the file stated none; a figure in the file outranks it. `options.contextWindow` is the
figure for every model of the provider that states none, and is the place to keep one for a block
that lists no models, such as a local Ollama, whose window is one server setting. A model's own
`limit.context` outranks it, and a gateway's report does not. The Ollama and LM Studio imports write
it where the server holds the model with fewer than 131,072 tokens. Following opencode, `limit`
needs `output` alongside `context` or it is not a `limit` and its figure is not read. `output` states
[how far a reply may run](../configuration.md#how-long-a-reply-may-run).

### What a model's `options` can and cannot do

Whatever you put there reaches the request body as it stands. Nothing parses it, knows what any of its
fields mean, or validates them, so a misspelled routing field is a request the gateway rejects, or
worse one it silently routes somewhere you did not intend.

It cannot replace what the turn itself built. The settings file names a destination, not what was
asked.

### Showing a model's thinking

A model that sends its working in a `reasoning_content` field beside its reply has that working drawn
under [`/thinking`](../../reference/commands.md#thinking) while the reply is still arriving. Nothing in the
request asks for it, so a model that sends none shows none, and a service that names the field
differently shows nothing. It is not saved to the session and is not sent back in a later request.

### Retries and a reply gone quiet

A request that fails because the connection gave out, or because the service said it was busy, is
sent again twice, after a pause of one second and then two. A reply that is still arriving is given
up on when nothing has come for two minutes. Both are figures you can change for one gateway in its
`options`:

```json
{
  "provider": {
    "my-gateway": {
      "options": {
        "baseURL": "https://gateway.example/v1",
        "maxRetries": 4,
        "idleTimeoutMs": 300000
      }
    }
  }
}
```

`maxRetries` is how many times a request is sent *again*, so `4` is five requests in all and `0`
sends each one once. It is at most 7. `idleTimeoutMs` is for a model that thinks for longer than two
minutes before it writes; it is at most 600000, ten minutes. Both are written as numbers: `"0"` in
quotes, a fraction, a negative figure and a gap of `0` leave the figure at what it was, and a figure
above its limit is the limit.

Only the count and the wait change. Which failures are sent again does not: a refused credential or
a model the service does not have is refused the same way each time, whatever count you gave. A wait
the service asked for with a `Retry-After`, up to two minutes, is taken before each retry in place of
the pause, so a large count against a service that keeps asking for it is a long wait.

A gateway on this machine, such as a local Ollama, is waited on through any silence while you can
stop the turn, so `idleTimeoutMs` does not reach it. `maxRetries` does.

These two are read from your own `settings.json`, from a file `--settings` names, and from the
machine-level file an administrator keeps. A `.bravebot/settings.json` in a checkout cannot set them,
as it cannot name a gateway: `bravebot doctor` says so when one tries. Brave's own endpoint and the
models you reach through the Bedrock tier variables alone have no entry to put them in and keep the
defaults. An [AWS Bedrock entry](bedrock.md#naming-more-than-three-models) takes the same two.
