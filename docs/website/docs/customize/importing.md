---
sidebar_position: 16
title: Importing from another agent
description: What a first run reads from Claude Code and opencode, what it asks a running Ollama, LM Studio or llama-server, and what it writes once you say yes.
---

# Importing from Claude Code, opencode or a local model server

A first run with nothing configured, in a terminal, reads what Claude Code and opencode set up in your
home directory, checks which API keys you have exported, and asks a running Ollama, LM Studio and
llama-server what each serves, before it names the routes. Where any of them reaches a service bravebot can use, it shows what it would add to
`~/.bravebot/settings.json` and asks:

```
Claude Code configures a model service bravebot can use, in /home/you/.claude/settings.json.
Importing it adds these to /home/you/.bravebot/settings.json:
  env.BRAVEBOT_USE_BEDROCK: "1"
  env.AWS_REGION: "us-west-2"
  env.ANTHROPIC_DEFAULT_SONNET_MODEL: "us.anthropic.claude-sonnet-4-5-20250929-v1:0"
  model: "sonnet"
Import this from Claude Code? [y/N]
```

Only a yes writes. Anything else, or the end of input, declines, and the start is refused as before. A
decline is not remembered, so the next start asks again. After a write the settings are read back from
disk, and the session opens on them. Where the model the session would use reads its key from a
variable you have not exported, the start names the variable and stops; an unset variable on any other
imported gateway is named and the session opens anyway. A settings file that changed while you were
answering is not written over.

**What is read.** Claude Code's `settings.json` in `$CLAUDE_CONFIG_DIR` (by default `~/.claude`), and
`CLAUDE_CODE_USE_BEDROCK` if you export it. opencode's `opencode.json` and `opencode.jsonc` in
`$XDG_CONFIG_HOME/opencode` (by default `~/.config/opencode`), the file `OPENCODE_CONFIG` names, and
its `auth.json` in `$XDG_DATA_HOME/opencode`. A checkout's `.claude/`, `opencode.json` and
`.opencode/` are never opened: they hold whatever the repository's author wrote, and would otherwise
decide where your key is sent.

**An exported key.** `OPENROUTER_API_KEY`, `OPENAI_API_KEY` and `ANTHROPIC_API_KEY` are checked in your
environment for whether each is set. The key is never read into anything, shown or written. A set
variable is offered as an entry that names it, at the address bravebot has for that service:

```
An API key for a model service bravebot can use is exported in this process's environment.
Importing it adds these to /home/you/.bravebot/settings.json:
  provider.openai, reached at https://api.openai.com/v1: {"env":["OPENAI_API_KEY"]}
Import this from your environment? [y/N]
```

A key names no model, so the start that writes the entry ends asking you to set `model`, and so does every start after it until
`model` is set: nothing yet asks which model to use ([#3440](https://github.com/brave/bravebot/issues/3440)). A key exported beside a host of its own, `ANTHROPIC_BASE_URL`,
`OPENAI_BASE_URL` or `OPENAI_API_BASE`, was issued for that host and is not offered at the usual
address: it is named as left, and a `provider` block that states the host and names the variable
reaches it. `GOOGLE_API_KEY` needs no import, since exporting it alone configures Google Vertex AI.

**A running Ollama.** Ollama is asked what it has pulled at the address `OLLAMA_HOST` names, read as
Ollama reads it, or at `http://localhost:11434` where it is unset. Only an address on this machine is
asked; one on another machine is named as left, and nothing is sent to it. A server that answers is
offered as [the block on the gateway page](providers/openai-compatible.md#a-local-ollama-or-another-gateway-that-wants-no-key), with the newest
model that can call tools as `model`:

```
Ollama is running at http://localhost:11434, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "ollama/qwen3-coder-oc:latest"
  provider.ollama, reached at http://localhost:11434/v1: {"name":"Ollama (local)","options":{"baseURL":"http://localhost:11434/v1"}}
Import this from Ollama? [y/N]
```

A model that reports it cannot call tools is never the one written, since every turn calls them; an
Ollama with none that can is named as left. Nothing answering, or an answer that is not Ollama's
listing, says nothing. The request waits a few seconds at most. Where your settings file already has
an entry for a server on the same port, under any name, Ollama is not offered again.

A second request asks which models are loaded. For the model shown, the question gives the context
window the server loaded it with. Where that is below the 131,072 tokens bravebot otherwise budgets
against, the question says the server drops the earliest turns beyond it and names
`OLLAMA_CONTEXT_LENGTH`, and the entry written carries the figure as `contextWindow`, so a
conversation is kept within it. A model that is not loaded has no line.

**A running LM Studio or llama-server.** Each is asked once, the way Ollama is, on this machine only,
with no key, and says nothing where it does not answer. A server that answers is offered as a block
of the same shape as Ollama's, under the id `lmstudio` or `llama-server`.

- LM Studio is asked for its model list at `http://127.0.0.1:1234`. It reads no variable for where it
  listens, so a server on another port is named in `BRAVEBOT_LM_STUDIO_HOST`, written as `OLLAMA_HOST`
  is. Only a language model trained for tool use is offered, and the one written is the first that
  LM Studio has loaded, or the first it lists where none is loaded. The context length it has the
  model loaded with is shown, and written as `contextWindow` where it is below 131,072 tokens.
- llama-server is asked at the port `LLAMA_ARG_PORT` names, which is the variable llama-server itself
  reads, on the address `LLAMA_ARG_HOST` names. With no port named, bravebot asks at `9931`, the port
  llama-server defaults to, and at `8080`, the one earlier releases and its container image use.
  It reports no capabilities and no loaded context, so neither is checked or shown.

```
LM Studio is running at http://127.0.0.1:1234, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "lmstudio/qwen/qwen3-coder-30b"
  provider.lmstudio, reached at http://127.0.0.1:1234/v1: {"name":"LM Studio (local)","options":{"baseURL":"http://127.0.0.1:1234/v1"}}
Import this from LM Studio? [y/N]
```

A server that asks for a key is not offered, because the request carries none. Where more than one
of Ollama, LM Studio and llama-server answers, each is asked about in that order, and one at a port an
earlier answer already wrote is not offered again.

[Ollama](providers/ollama.md), [LM Studio](providers/lm-studio.md) and
[llama-server](providers/llama-server.md) each have a page that also gives the block to write
yourself.

**What can be imported.** A Claude Code Bedrock setup, under bravebot's own names, when a region is
named. An opencode `provider` entry reached through an OpenAI-compatible SDK, with only the fields
bravebot reads, and an `auth.json` API key for a gateway whose endpoint bravebot knows. An `openai` entry that names
no SDK, or an `auth.json` key for `openai`, imports, and so does one naming `@ai-sdk/openai` that states no
`baseURL` or OpenAI's. One naming it that states another host is left as another SDK's. The
`ANTHROPIC_API_KEY` in Claude Code's settings file imports as an `anthropic` entry,
unless a Claude Code `ANTHROPIC_BASE_URL` is stated beside it. opencode's
`disabled_providers` and `enabled_providers` are honoured, and a name already set in your settings file,
or pinned by an administrator, is left as it is. A model opencode's `amazon-bedrock` entry lists that
one of your Bedrock tiers already names, such as the Claude Code import's
`ANTHROPIC_DEFAULT_OPUS_MODEL`, is not added to the entry again.

**Keys.** A source that names a variable has only the name written, and opencode's `{env:VAR}` becomes
`"env": ["VAR"]`. A key the source holds itself is asked about on its own question, which names the host
it would be sent to and says it is kept in plain text; the key is never shown. Declining writes the
entry reading the variable opencode reads for it, such as `OPENROUTER_API_KEY`, and says to export it.
A `{file:path}` key is not followed, and neither is a substitution inside a longer value, such as
`Bearer {env:TOKEN}`: that entry is named as left.

**What is never imported**: commands, permissions, hooks and MCP servers. What was found and cannot be
used, such as an Anthropic bearer token or a Vertex AI setup, is named with the reason before the routes,
and its value is never shown.

A run that cannot ask (`-p`, `--json`, a pipe or a redirect, `doctor`) imports nothing, and its
refusal names the command that asks:

```sh
bravebot import-providers
```

That command asks the same questions at any time, whether or not a service is configured. It refuses
without a terminal and in an incognito session. An incognito session asks none of the servers either,
since it has nothing to write.
