---
sidebar_position: 10
title: Signing in
description: Every way to sign in to a model service, from one command.
---

# Signing in

```sh
bravebot auth login
```

lists the ways to sign in to a model service, says which are already in use, and runs the one you
pick by number or by name:

```
Ways to sign in to a model service:
  1. leo      Brave Leo Premium, from a Brave install that subscribes
  2. bedrock  An AWS account, for Amazon Bedrock
  3. import   A model service Claude Code, opencode or Ollama has, imported into settings
  4. gateway  A key for a gateway a provider block in settings names, typed here and kept by bravebot
Which one? Type its number or its name, or nothing to stop:
```

Each way is the command it was before, run for you:

| Way | Same as | More |
|---|---|---|
| `leo` | `bravebot import-leo-creds [channel]` | [Leo Premium](premium.md) |
| `bedrock` | the AWS sign-in a session makes on its first turn | [AWS Bedrock](providers/bedrock.md) |
| `import` | `bravebot import-providers` | [Importing](configuration.md#importing-from-claude-code-opencode-or-ollama) |
| `gateway` | nothing before it: the key used to go in a variable or a settings file | [below](#gateway) |

A script names the way, so nothing is asked: `bravebot auth login leo nightly`,
`bravebot auth login bedrock`, `bravebot auth login import`. With no way named and no terminal to
ask on, the command is refused and prints these forms. `gateway` is the exception, since the key is
typed at the terminal and never given on the command line.

**Leo** asks which channel, and nothing typed means stable. If a subscription is already imported it
asks before signing in again, because each import registers this machine with Brave as one more
device. `bravebot auth logout leo` forgets the import.

**Bedrock** signs in to every AWS account your configuration names: the one `AWS_PROFILE` names
with `AWS_REGION` and a tier model such as `ANTHROPIC_DEFAULT_OPUS_MODEL` set, and each
`amazon-bedrock` provider block. An account whose session is still good is left alone. A profile
that still has no usable session after signing in is named, and the others are signed in to anyway.
Picking Bedrock turns the backend on: once the `AWS_PROFILE` account signs in,
`"BRAVEBOT_USE_BEDROCK": "1"` is added to the `env` block of `~/.bravebot/settings.json`, so a
session uses it without the variable exported. Where that file already names
`BRAVEBOT_USE_BEDROCK`, or something else turns Bedrock off, nothing is written. `AWS_REGION` and
`AWS_PROFILE` are not written, so keep them exported or put them in the same block. To sign out, run
`aws sso logout`.

## Gateway

An [OpenAI-compatible gateway](providers/openai-compatible.md) such as OpenRouter is a provider
block in settings, and the block says where requests go. `gateway` stores the key it is sent:

```
$ bravebot auth login gateway
Gateways configured:
  1. openrouter  openrouter.ai
  2. work        gateway.work.example (a key stored)
Which one? Type its number or its id, or nothing to stop: 1

Key for openrouter, sent to openrouter.ai (not shown as you type):
the key for openrouter is stored in /home/you/.bravebot/gateway-keys.json, and sessions started from now on send it to openrouter.ai
```

With one gateway configured, or one named as in `bravebot auth login gateway openrouter`, it asks
for the key straight away. Nothing appears as the key is typed or pasted. Ctrl-U starts it again,
and Escape, Ctrl-C, Ctrl-D or Enter on nothing stops without storing anything. A gateway that
already has a key stored asks before replacing it.

The key is kept in `~/.bravebot/gateway-keys.json`, readable only by your account, under the
block's id. It holds no host, so a key goes only where the block with that id sends requests. A
variable the block's `env` names wins over a stored key while it is set, and the sign-in says so
when one is set. A session reads the file when it starts, so one already open keeps the key it had.

`bravebot auth logout gateway openrouter` forgets the key. With only one stored the id can be left
off. Forgetting it here does not revoke it: it works at the gateway until you revoke it there.

## Checking a sign-in

```sh
bravebot auth status leo
```

prints whether Leo is signed in and usable, and exits 0 only if it is, so a script can test it.
`bedrock` and `gateway [id]` ask about the other two. With no way named it asks about all of them and
exits 0 if any is usable. See [the reference](../reference/cli.md#auth).

## In an incognito session

In an [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind), `leo`,
`import` and `gateway` are refused, since each keeps something in bravebot's own directory.
`bedrock` is allowed: the AWS CLI keeps that session, and a session in this mode signs in to it as
well. It writes nothing to settings, so `BRAVEBOT_USE_BEDROCK=1` still has to be exported. Signing
out of Leo and forgetting a gateway key are allowed.
