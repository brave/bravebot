---
sidebar_position: 6
title: llama-server
description: Reach a model served by llama-server with no key, what the first run writes for you, and the block to write yourself.
---

# Reaching a model through llama-server

llama-server is reached as an [OpenAI-compatible gateway](openai-compatible.md) that wants no API
key, so its block names none. It listens on port `9931` by default, and on `8080` in earlier
releases and in its container image. What the gateway page says about listing models, naming one and
the context window applies to it.

## Let the first run write it

A first run with nothing configured, in a terminal, asks a llama-server running on this machine what
it serves. bravebot shows what it would add to `~/.bravebot/settings.json` and asks:

```
llama-server is running at http://127.0.0.1:9931, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "llama-server/../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"
  provider.llama-server, reached at http://127.0.0.1:9931/v1: {"name":"llama-server (local)","options":{"baseURL":"http://127.0.0.1:9931/v1"}}
Import this from llama-server? [y/N]
```

Only a yes writes. A decline is not remembered, so the next start asks again.

- **The address and port are the ones llama-server itself reads.** `LLAMA_ARG_HOST` names the
  address, `127.0.0.1` where it is unset, and `LLAMA_ARG_PORT` names the port. Where no port is named,
  or the value is not a port number or is `0`, bravebot asks at `9931` and at `8080`.
- **Only this machine is asked.** A `LLAMA_ARG_HOST` naming another machine is named as left, and
  nothing is sent to it.
- **A model the server names as a path is written as it is named**, so the question shows what the
  file will hold, as in the example above.
- **Nothing is checked or shown about capabilities or context.** llama-server reports neither.
- **A server that asks for a key is not offered**, because the request carries none.
- **Where more than one local server answers**, each is asked about in turn: Ollama, then LM Studio,
  then llama-server.

[Importing from Claude Code, opencode or a local model
server](../configuration.md#importing-from-claude-code-opencode-or-a-local-model-server) covers the
same question for the other sources.

## Write it yourself

```json
{
  "provider": {
    "llama-server": {
      "name": "llama-server (local)",
      "options": { "baseURL": "http://127.0.0.1:9931/v1" }
    }
  },
  "model": "llama-server/../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"
}
```

`baseURL` is written down because `llama-server` is not one of the names an endpoint is
[compiled in](openai-compatible.md#where-the-requests-go) for, and it holds the port llama-server was
started on. There is no `models` key, so llama-server is asked what it serves, and `/model` lists
what came back. A model is named with `llama-server/` in front and then the name the server gives
it. The name is split once, at the first slash ([Naming one](openai-compatible.md#naming-one)).

## The context window

llama-server reports no context length, so bravebot cannot show or check it. Where the server runs
with fewer than the 131,072 tokens bravebot otherwise budgets against, state the figure once, for
every model of the provider:

```json
"options": { "baseURL": "http://127.0.0.1:9931/v1", "contextWindow": 32768 }
```

[The context window](openai-compatible.md#the-context-window) says what outranks it.
