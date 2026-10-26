---
sidebar_position: 8
title: LM Studio
description: Reach a model served by LM Studio's local server with no key, what the first run writes for you, and the block to write yourself.
---

# Reaching a model through LM Studio

LM Studio's local server is reached as an [OpenAI-compatible gateway](openai-compatible.md) that
wants no API key, so its block names none. The server listens on port `1234` once it is started.
What the gateway page says about listing models, naming one and the context window applies to it.

## Let the first run write it

A first run with nothing configured, in a terminal, asks an LM Studio running on this machine for
its model list at `http://127.0.0.1:1234`. bravebot shows what it would add to
`~/.bravebot/settings.json` and asks:

```
LM Studio is running at http://127.0.0.1:1234, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "lmstudio/qwen/qwen3-coder-30b"
  provider.lmstudio, reached at http://127.0.0.1:1234/v1: {"name":"LM Studio (local)","options":{"baseURL":"http://127.0.0.1:1234/v1"}}
Import this from LM Studio? [y/N]
```

Only a yes writes. A decline is not remembered, so the next start asks again.

- **A server on another port is named in `BRAVEBOT_LM_STUDIO_HOST`.** LM Studio reads no variable
  for where it listens, so this one is bravebot's own. It is written as Ollama's `OLLAMA_HOST` is,
  and only an address on this machine is asked. One naming another machine is named as left, and
  nothing is sent to it.
- **Only a language model trained for tool use is offered.** The one written is the first that LM
  Studio has loaded, or the first it lists where none is loaded.
- **A small context window is said.** The context length LM Studio has the model loaded with is
  shown, and written as `contextWindow` where it is below the 131,072 tokens bravebot otherwise
  budgets against.
- **A server that asks for a key is not offered**, because the request carries none.
- **Where more than one local server answers**, each is asked about in turn: Ollama, then LM Studio,
  then llama-server.

[Importing from Claude Code, opencode or a local model server](../importing.md) covers the same
question for the other sources.

## Write it yourself

```json
{
  "provider": {
    "lmstudio": {
      "name": "LM Studio (local)",
      "options": { "baseURL": "http://127.0.0.1:1234/v1" }
    }
  },
  "model": "lmstudio/qwen/qwen3-coder-30b"
}
```

`baseURL` is written down because `lmstudio` is not one of the names an endpoint is
[compiled in](openai-compatible.md#where-the-requests-go) for. There is no `models` key, so LM Studio
is asked what it serves, and `/model` lists what came back. A model is named with `lmstudio/` in
front, and the name LM Studio gives it can hold a slash, as `qwen/qwen3-coder-30b` does. The name is
split once, at the first slash ([Naming one](openai-compatible.md#naming-one)).

## The context window

LM Studio's window is one setting of the server, so a block that lists no models states it once, for
every model of the provider:

```json
"options": { "baseURL": "http://127.0.0.1:1234/v1", "contextWindow": 32768 }
```

Without it bravebot assumes 131,072 tokens. [The context
window](openai-compatible.md#the-context-window) says what outranks it.
