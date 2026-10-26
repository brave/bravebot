---
sidebar_position: 5
title: OpenRouter
description: Reach models through OpenRouter with a key, which models are offered, and how to name one.
---

# Reaching a model through OpenRouter

OpenRouter is reached as an [OpenAI-compatible gateway](openai-compatible.md). bravebot knows its
endpoint, so an entry keyed `openrouter` needs no `baseURL`. A stated `baseURL` always wins, so a
proxy or a private deployment is reached by writing where it is.

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

## The key

Name a variable in `env` and keep the token wherever you already keep secrets. Or store it with
bravebot:

```sh
bravebot auth login gateway openrouter
```

It asks for the key with nothing shown as you type it, and keeps it in
`~/.bravebot/gateway-keys.json`, readable only by your account. No block has to be written first:
where none names `openrouter`, the sign-in adds `"provider": {"openrouter": {}}` to your settings,
which reaches `openrouter.ai` and names no variable. [Signing in](../signing-in.md#gateway) has the
details. Where more than one source of the key is present, a variable wins, then the stored
key, then `options.apiKey` ([The credential](openai-compatible.md#the-credential)).

## Which models are offered

A block that lists `models` is offered those and no others, as the block above is, which is the way
to pin a short list out of a service offering hundreds. A block that lists none has OpenRouter asked,
and [Which models are offered](openai-compatible.md#which-models-are-offered) says what the answer is
filtered by.

## Naming one

Where one model is reachable through more than one service, put the gateway's id in front of the
name to say which you mean:

```
openrouter/z-ai/glm-4.6
```

The name is split once, at the first slash, because most gateway names contain one. The id picks the
service and only the remainder is sent ([Naming one](openai-compatible.md#naming-one)).
