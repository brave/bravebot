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
Which one? Type its number or its name, or nothing to stop:
```

Each way is the command it was before, run for you:

| Way | Same as | More |
|---|---|---|
| `leo` | `bravebot import-leo-creds [channel]` | [Leo Premium](premium.md) |
| `bedrock` | the AWS sign-in a session makes on its first turn | [AWS Bedrock](providers/bedrock.md) |
| `import` | `bravebot import-providers` | [Importing](configuration.md#importing-from-claude-code-opencode-or-ollama) |

A script names the way, so nothing is asked: `bravebot auth login leo nightly`,
`bravebot auth login bedrock`, `bravebot auth login import`. With no way named and no terminal to
ask on, the command is refused and prints these forms.

**Leo** asks which channel, and nothing typed means stable. If a subscription is already imported it
asks before signing in again, because each import registers this machine with Brave as one more
device. `bravebot auth logout leo` forgets the import.

**Bedrock** signs in to every AWS account your configuration names: the one `AWS_PROFILE` names
with `BRAVEBOT_USE_BEDROCK` and `AWS_REGION` set, and each `amazon-bedrock` provider block. An
account whose session is still good is left alone. A profile that still has no usable session after
signing in is named, and the others are signed in to anyway. Signing in does not turn the backend
on, so `BRAVEBOT_USE_BEDROCK=1` still has to be set for a session to use it. To sign out, run
`aws sso logout`.

**An OpenAI-compatible gateway** such as OpenRouter is not on the list yet. Its API key goes in the
environment or a settings file, as [OpenAI-compatible gateways](providers/openai-compatible.md)
describes.

In an [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind), `leo` and
`import` are refused, since each keeps something in bravebot's own directory. `bedrock` is allowed:
the AWS CLI keeps that session, and a session in this mode signs in to it as well. Signing out of
Leo is allowed.
