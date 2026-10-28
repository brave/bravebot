---
sidebar_position: 2
title: OpenAI
description: Reach OpenAI with an API key, which models it offers, and how to reach it through chat completions instead of the Responses API.
---

# Reaching OpenAI with an API key

An entry keyed `openai` reaches OpenAI with an API key, through its Responses API:

```json
{
  "provider": {
    "openai": {
      "env": ["OPENAI_API_KEY"]
    }
  },
  "model": "openai/<model id>"
}
```

The endpoint is `https://api.openai.com/v1`, and requests go to `/responses` under it. A stated `baseURL` wins,
so a proxy or another deployment is reached by writing where it is. An entry that states a host other than
OpenAI's is sent chat completions, as [Chat completions](#chat-completions) says. Export the key as
`OPENAI_API_KEY=<your key>`, or store it with `bravebot auth login gateway openai`, which needs no
block written first: it adds `"provider": {"openai": {}}` to your settings, and that entry sends the
stored key to `api.openai.com`. A block you write yourself has to say where the key lives: one
naming no `env`, no `options.apiKey` and no stored key sends none, and OpenAI refuses the request.
`OPENAI_API_KEY` is a name other OpenAI tools read as well, and naming it in `env` is what makes
bravebot send the key your shell holds, so the entry the sign-in adds does not name it. Like any
variable a `provider` block names, it is [withheld from the programs bravebot
runs](../../reference/run-tool.md#what-a-program-is-handed).

With no `models` key, OpenAI is asked what it serves and `/model` offers what it lists, less the models whose
listed `shutdown_date` is today or earlier, which OpenAI no longer serves. The listing says nothing about what
a model can do, so embedding, speech, audio, image and video models are in it beside the chat models, and
picking one fails. On 2026-10-27, asked once for a short reply through chat completions, each of those answered
with an error. The Responses route has not been tried with them. A block that lists `models` is offered those
and no others, which is how to keep the picker to the models you use:

```json
{
  "provider": {
    "openai": {
      "env": ["OPENAI_API_KEY"],
      "models": { "<model id>": {} }
    }
  }
}
```

OpenAI caches a repeated prompt without being asked, so bravebot sends no cache marks to
`api.openai.com`, and [`promptCacheTtl`](../configuration.md#promptcachettl) does not apply to it. A proxy
you reach by writing its `baseURL` is sent them, whatever the entry is called. The cache lines of
`/status` show what a turn read from the cache. A line for what the turn wrote appears only for the
models whose replies state a write. On 2026-10-27 `gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`,
`gpt-6-astra`, `gpt-6-luna`, `gpt-6-sol`, `gpt-6.1-sol` and `gpt-6.1-terra` did, and the older models that
answered did not, so a turn on one of those shows none, whatever the cache took in.

## Effort levels

A level chosen with [`/effort`](../effort.md) or `--effort` is sent as `reasoning.effort`, with a summary of
the reasoning that `/thinking` shows. A level OpenAI refuses costs the level and not the turn: bravebot sends
the request again without it.

Through chat completions the level is sent as `reasoning_effort`. On 2026-10-27 `gpt-4o-mini` and
`gpt-4.1-mini` refused it, and so did `gpt-5-mini`, `gpt-5.4-mini` and `gpt-5.6-luna` for the levels they do
not take: `gpt-5-mini` read `low`, `medium` and `high` but not `xhigh`, for example. The turn carries on,
because bravebot sends the request again without the level. It then sends that model no level until bravebot
is restarted, including a level the model would have taken, and
[`/status`](../../reference/commands.md#status) reports the model as reading none.

## The Responses API

The conversation is never kept at the service (`store` is off), the reply is always streamed, and the model's
earlier reasoning is not sent back with a later request. Pictures are sent, and a document attachment is not.

An entry that names `@ai-sdk/openai` in `npm`, as it does in opencode, is sent requests at `/responses` under
its `baseURL` whatever its id, so it reaches a proxy that serves that route:

```json
{
  "provider": {
    "openai": {
      "npm": "@ai-sdk/openai",
      "env": ["OPENAI_API_KEY"],
      "options": { "baseURL": "https://<proxy host>/v1" }
    }
  }
}
```

## Chat completions

An entry that names `@ai-sdk/openai-compatible` in `npm` is sent requests at `/chat/completions` under its
`baseURL`:

```json
{
  "provider": {
    "openai": {
      "npm": "@ai-sdk/openai-compatible",
      "env": ["OPENAI_API_KEY"]
    }
  }
}
```

An `openai` entry that states a `baseURL` at another host is sent chat completions without naming it, since a
proxy may serve that protocol alone. So is an entry under another id that states `https://api.openai.com/v1`.

Two things differ on this route at `api.openai.com`:

- OpenAI refuses a request that offers a function tool to `gpt-6-sol` and to `gpt-6.1-sol`, and a turn offers
  bravebot's tools, so a turn on either ends with an HTTP 400.
- The listing leaves out the models that take only OpenAI's Responses API, which this route cannot reach: the
  `-codex` and `-pro` ids among them. That list is compiled into bravebot from OpenAI's published API
  description and from the model pages at `developers.openai.com`, so it changes with a release of bravebot. A
  Responses-only model that neither document names, such as one released since, is offered. `gpt-image-1-mini`
  is one, and this route refused it.

A model is named with the id in front, as `openai/<model id>`, in `--model` or the `model` key.

Signing in with a ChatGPT account instead of a key is a different route, and its models are named
`chatgpt/<model>`: see [ChatGPT](chatgpt.md). It does not use `OPENAI_API_KEY`, and an `openai/` model
does not use the sign-in.
