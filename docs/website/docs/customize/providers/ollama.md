---
sidebar_position: 4
title: Ollama
description: Reach a model served by a local Ollama with no key, what the first run writes for you, and the block to write yourself.
---

# Reaching a model through Ollama

A local Ollama is reached as an [OpenAI-compatible gateway](openai-compatible.md) that wants no API
key, so its block names none. What the gateway page says about listing models, naming one and the
context window applies to it.

## Let the first run write it

A first run with nothing configured, in a terminal, asks an Ollama running on this machine what it
has pulled. The address is the one `OLLAMA_HOST` names, read as Ollama reads it, or
`http://localhost:11434` where it is unset. bravebot shows what it would add to
`~/.bravebot/settings.json` and asks:

```
Ollama is running at http://localhost:11434, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "ollama/qwen3-coder-oc:latest"
  provider.ollama, reached at http://localhost:11434/v1: {"name":"Ollama (local)","options":{"baseURL":"http://localhost:11434/v1"}}
Import this from Ollama? [y/N]
```

Only a yes writes. A decline is not remembered, so the next start asks again.

- **Only this machine is asked.** An `OLLAMA_HOST` naming another machine is named as left, and
  nothing is sent to it.
- **The model written is the newest that can call tools.** Every turn calls tools, so a model that
  reports it cannot is never the one written, and an Ollama with none that can is named as left.
- **A small context window is said.** Where Ollama has the model loaded with fewer than the 131,072
  tokens bravebot otherwise budgets against, the question says Ollama drops the earliest turns beyond
  it and names `OLLAMA_CONTEXT_LENGTH`. The entry written carries the figure as `contextWindow`, so a
  conversation is kept within it.
- **An Ollama already in your settings is not offered again.** That holds where the file has an entry
  for a server on the same port, under any name.

[Importing from Claude Code, opencode or a local model
server](../configuration.md#importing-from-claude-code-opencode-or-a-local-model-server) covers the
same question for the other sources.

## Write it yourself

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
[compiled in](openai-compatible.md#where-the-requests-go) for. There is no `models` key, so Ollama is
asked what it has pulled, and `/model` lists what came back without the models that cannot call
tools. To pin a short list instead, see [Which models are
offered](openai-compatible.md#which-models-are-offered). A model is named with `ollama/` in front,
as in the block above.

## The context window

Ollama's window is one setting of the server, so a block that lists no models states it once, for
every model of the provider:

```json
"options": { "baseURL": "http://localhost:11434/v1", "contextWindow": 32768 }
```

Without it bravebot assumes 131,072 tokens. [The context
window](openai-compatible.md#the-context-window) says what outranks it.
