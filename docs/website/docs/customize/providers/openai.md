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
`OPENAI_API_KEY=<your key>`, or store it with `bravebot auth login gateway openai`. The block has to say
where the key lives: a block naming no `env`, no `options.apiKey` and no stored key sends none, and
OpenAI refuses the request. `OPENAI_API_KEY` is a name other OpenAI tools read as well, and naming it
in `env` is what makes bravebot send the key your shell holds. Like any variable a `provider` block
names, it is [withheld from the programs bravebot runs](../../reference/run-tool.md#what-a-program-is-handed).

With no `models` key, OpenAI is asked what it serves and `/model` offers everything it lists. The
listing says nothing about what a model can do, so embedding, speech and image models are in it beside
the chat models, and so are models OpenAI serves only through its Responses API. An entry reaches those
by naming `@ai-sdk/openai` in `npm`, as it does in opencode, and without that line picking one fails. A
block that lists `models` is offered those and no others, which is how to keep the picker to the models
you use:

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
