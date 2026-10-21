---
sidebar_position: 3
title: Anthropic
description: Reach Claude with an Anthropic API key through its Messages API, and what the block, the roster, the reply ceiling and prompt caching do.
---

# Reaching Claude with an Anthropic API key

An entry keyed `anthropic` reaches Anthropic's own API with an API key:

```json
{
  "provider": {
    "anthropic": {
      "env": ["ANTHROPIC_API_KEY"]
    }
  },
  "model": "anthropic/<model id>"
}
```

Export the key as `ANTHROPIC_API_KEY=<your key>`, or store it with
`bravebot auth login gateway anthropic`. The block has to say where the key lives: a block naming no
`env`, no `options.apiKey` and no stored key sends none, and Anthropic refuses the request.
`ANTHROPIC_API_KEY` is a name other tools read as well, and naming it in `env` is what makes
bravebot send the key your shell holds. Like any variable a `provider` block names, it is
[withheld from the programs bravebot runs](../../reference/run-tool.md#what-a-program-is-handed).

This is a different route from [AWS Bedrock](bedrock.md), which reaches Claude through an AWS
account and signs its requests. Nothing here needs an AWS account.

## Where the requests go

The endpoint is `https://api.anthropic.com/v1`, compiled in, so the block needs no `baseURL`. A stated
`baseURL` wins, so a proxy in front of the API is reached by writing where it is, and is still spoken
to in Anthropic's Messages protocol.

The key travels in the `x-api-key` header, beside an `anthropic-version` header that the API
requires, and in no other header or part of the request. It is not sent as a bearer token, and a
redirect to another host is sent the request without it.

A model is named with the id in front, as `anthropic/<model id>`, in `--model` or the `model` key.
Anthropic answers an alias such as `claude-haiku-4-5` with the dated model behind it, and that is not
reported as a substituted model.

## Which models are offered

With no `models` key, Anthropic is asked what it serves, in one request for the whole listing, and
`/model` offers everything it lists, including models it still serves and has deprecated. Each row
carries the context window the listing states for it, and whether the model reads an effort level.

A block that lists `models` is offered those and no others, which is how to keep the picker to the
models you use:

```json
{
  "provider": {
    "anthropic": {
      "env": ["ANTHROPIC_API_KEY"],
      "models": {
        "<model id>": { "limit": { "context": 1000000, "output": 128000 } }
      }
    }
  }
}
```

## What a request carries

**Effort.** The level chosen with [`/effort`](../configuration.md#choosing-how-hard-to-think), or the
`effort` setting, is sent in `output_config`. It is not sent to a model whose listing says it reads
none, and a model the listing does not describe, including every model of a block that lists `models`,
is sent it and judged by Anthropic's answer: one that refuses it is not sent it again and is shown as
reading none. Left unchosen, nothing is sent and the model keeps its own default.

**A reply ceiling.** Anthropic requires every request to state one. bravebot sends 32,000 tokens
unless you state another: a model's `limit.output`, as above, or `BRAVEBOT_OUTPUT_BUDGET`, which
outranks every model's. A model that refuses 32,000 is asked again with 8,192, and a figure you stated
is never lowered for you.

**Prompt caching.** The system prompt and the end of the conversation are marked, so a turn that
sends its history again each round does not pay for all of it each time. The `promptCacheTtl` setting
chooses how long Anthropic keeps what was marked. A model that refuses the marks is asked again
without them, and `/status` reports how much of each prompt was read from the cache and how much was
written to it.

**Options of your own.** A model's `options` are put in the request as written. bravebot does not
read them, and they cannot replace the model or the messages a turn built. Leave `thinking` out of
them: bravebot does not send a model's thinking back to it, so a request that asks for thinking and
carries a tool call from an earlier round is refused, and a model that no longer accepts a thinking
budget refuses it from the first round.

## A gateway that is called `anthropic`

The id is Anthropic's, so a block keyed `anthropic` is reached in Anthropic's protocol, as opencode
reads it. A block of your own that used the name for an OpenAI-compatible gateway names the SDK it
uses, as opencode does, and is reached as any other gateway is:

```json
{
  "provider": {
    "anthropic": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "https://gateway.example.invalid/v1" }
    }
  }
}
```

## What is not supported

- Signing in with a Claude Pro or Max account instead of a key.
- Showing the model's thinking, or asking for it. No `thinking` setting is sent, so each model keeps
  its own default, and what it writes while thinking is not part of the reply.
- Holding a run to `--output-schema` at the service. The reply is still checked against the schema
  once it arrives.

## Importing it

`bravebot import` offers an opencode `anthropic` entry that names no SDK or names `@ai-sdk/anthropic`,
and an opencode `auth.json` key for the id, as a block like the first one above. An entry for another
id that names `@ai-sdk/anthropic`, an `anthropic` entry that names `@ai-sdk/openai-compatible`, and a
Claude Code `ANTHROPIC_API_KEY` are left: add the block by hand and name the variable.
