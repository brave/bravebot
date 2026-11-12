---
sidebar_position: 7
title: Ollama
description: Connect local Ollama models without an API key, use first-run discovery or manual gateway configuration, and set the context window.
---

# Reaching a model through Ollama

A local Ollama is configured as a [gateway](openai-compatible.md) that wants no API key, so its
block names none. Its block names Ollama's own SDK in `npm`, so bravebot speaks Ollama's own API at
`/api`: each request asks the server for the context window the session budgets for, and each
model's window is read from Ollama. What the gateway page says about naming a model and which models
are offered applies to it.

## Let the first run write it

A first run with nothing configured, in a terminal, asks an Ollama running on this machine what it
has pulled. The address is the one `OLLAMA_HOST` names, read as Ollama reads it, or
`http://localhost:11434` where it is unset. bravebot shows what it would add to
`~/.bravebot/settings.json` and asks:

```
Ollama is running at http://localhost:11434, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "ollama/qwen3-coder-oc:latest"
  provider.ollama, reached at http://localhost:11434/api: {"name":"Ollama (local)","npm":"ollama-ai-provider-v2","options":{"baseURL":"http://localhost:11434/api"}}
Import this from Ollama? [y/N]
```

Only a yes writes. A decline is not remembered, so the next start asks again.

- **Only this machine is asked.** An `OLLAMA_HOST` naming another machine is named as left, and
  nothing is sent to it.
- **The model written is the newest that can call tools.** Every turn calls tools, so a model that
  reports it cannot is never the one written, and an Ollama with none that can is named as left.
- **A small context window is said.** Where Ollama has the model loaded with fewer than the 65,536
  tokens bravebot budgets an Ollama model against, the question says each request asks for the window the
  model was trained with instead, which takes more memory, and that `contextWindow` asks for less.
- **An Ollama already in your settings is not offered again.** That holds where the file has an entry
  for a server on the same port, under any name.

[Importing from Claude Code, opencode or a local model server](../importing.md) covers the same
question for the other sources.

## Write it yourself

```json
{
  "provider": {
    "ollama": {
      "name": "Ollama (local)",
      "npm": "ollama-ai-provider-v2",
      "options": { "baseURL": "http://localhost:11434/api" }
    }
  },
  "model": "ollama/qwen3-coder:30b"
}
```

`baseURL` is written down because `ollama` is not one of the names an endpoint is
[compiled in](openai-compatible.md#where-the-requests-go) for. `npm` may also be
`ollama-ai-provider`, opencode's older name for the same SDK. There is no `models` key, so Ollama is
asked what it has pulled, and `/model` lists what came back without the models that cannot call
tools. To pin a short list instead, see [Which models are
offered](openai-compatible.md#which-models-are-offered). A model is named with `ollama/` in front,
as in the block above.

## The context window

Ollama allocates a model's window when it loads it, and left to itself it uses a default of a few
thousand tokens and drops the start of any longer prompt without saying so. bravebot asks for the
window on every request instead, as `options.num_ctx`, so a conversation is never cut short behind
your back and `OLLAMA_CONTEXT_LENGTH` need not be set.

The window asked for is the one bravebot budgets the conversation against. Where the block lists no
models, it is read for each model from Ollama's `/api/show`, which states the window the model was
trained with, and held to 65,536 tokens. A model whose window Ollama does not state is given
65,536.

A larger window takes more memory, and Ollama reloads a model when the window asked for changes.
Where the machine is short of memory, ask for less, for every model of the provider:

```json
"options": { "baseURL": "http://localhost:11434/api", "contextWindow": 32768 }
```

or for one model, in a block that lists it, as `limit` (which needs `output` beside `context`):

```json
"models": { "qwen3-coder:30b": { "limit": { "context": 32768, "output": 8192 } } }
```

A `num_ctx` among a model's `options` never replaces the window above, so the figure bravebot
budgets against and the one Ollama allocates are the same. [The context
window](openai-compatible.md#the-context-window) says what outranks what.

## The OpenAI-compatible endpoint

A block without `npm` and with `baseURL` at `http://localhost:11434/v1` still works, reached as any
OpenAI-compatible gateway. That endpoint takes no window, so Ollama uses its own default, set by
`OLLAMA_CONTEXT_LENGTH` on the server, and drops the earliest turns beyond it. Where you keep that
form, state the server's window as `contextWindow` so bravebot compacts the conversation before
Ollama cuts it.
