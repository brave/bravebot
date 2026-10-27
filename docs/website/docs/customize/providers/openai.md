---
sidebar_position: 2
title: OpenAI
description: Reach OpenAI with an API key, which models it offers, and how an entry reaches the Responses API.
---

# Reaching OpenAI with an API key

An entry keyed `openai` reaches OpenAI with an API key, through its chat-completions endpoint:

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

The endpoint is `https://api.openai.com/v1`, and a stated `baseURL` wins, so a proxy or another
deployment speaking the same protocol is reached by writing where it is. Export the key as
`OPENAI_API_KEY=<your key>`, or store it with `bravebot auth login gateway openai`, which needs no
block written first: it adds `"provider": {"openai": {}}` to your settings, and that entry sends the
stored key to `api.openai.com`. A block you write yourself has to say where the key lives: one
naming no `env`, no `options.apiKey` and no stored key sends none, and OpenAI refuses the request.
`OPENAI_API_KEY` is a name other OpenAI tools read as well, and naming it in `env` is what makes
bravebot send the key your shell holds, so the entry the sign-in adds does not name it. Like any
variable a `provider` block names, it is [withheld from the programs bravebot
runs](../../reference/run-tool.md#what-a-program-is-handed).

With no `models` key, OpenAI is asked what it serves and `/model` offers what it lists, less the
models that take only OpenAI's Responses API, which the chat-completions route cannot reach: the
`-codex` and `-pro` ids among them. That list is compiled into bravebot from OpenAI's published API
description and from the model pages at `developers.openai.com`, read on 2026-10-27. It is not checked
against a reply from OpenAI. The listing says nothing about what a model can do, so embedding, speech
and image models are in it beside the chat models, and picking one of those fails. A Responses-only
model that neither document names, such as one released since, is offered too, and fails when picked.
An entry that names `@ai-sdk/openai` in `npm`, as it
does in opencode, reaches the Responses API and is offered the whole listing. A block that lists
`models` is offered those and no others, which is how to keep the picker to the models you use:

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
you reach by writing its `baseURL` is sent them, whatever the entry is called.

## The Responses API

OpenAI also serves models through its Responses API, and some only through it. An entry that names
`@ai-sdk/openai` is sent requests there, at `/responses` under its `baseURL`, instead of at
`/chat/completions`:

```json
{
  "provider": {
    "openai": {
      "npm": "@ai-sdk/openai",
      "env": ["OPENAI_API_KEY"],
      "models": { "<model id>": {} }
    }
  }
}
```

The SDK decides for every model the entry lists, and the entry's `id` does not matter except for
`google-vertex`: any other entry naming it, a proxy included, is reached in this protocol. The conversation is never kept at the service
(`store` is off), the reply is always streamed, and the model's earlier reasoning is not sent back with
a later request. Pictures are sent, and a document attachment is not. The effort level is sent as
`reasoning.effort`, with a summary of the reasoning that `/thinking` shows.

A model is named with the id in front, as `openai/<model id>`, in `--model` or the `model` key.

Signing in with a ChatGPT account instead of a key is a different route, and its models are named
`chatgpt/<model>`: see [ChatGPT](chatgpt.md). It does not use `OPENAI_API_KEY`, and an `openai/` model
does not use the sign-in.
