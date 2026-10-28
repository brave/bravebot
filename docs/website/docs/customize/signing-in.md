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
  1. brave    Brave's premium service, with a key typed here and kept by bravebot
  2. bedrock  An AWS account, for Amazon Bedrock
  3. import   A model service Claude Code, opencode, Ollama, LM Studio or llama-server has, imported into settings
  4. gateway  A key for openrouter, openai, anthropic or a gateway a provider block in settings names, typed here and kept by bravebot
  5. chatgpt  A ChatGPT Plus or Pro account, signed in to in a browser
  6. xai      An xAI SuperGrok subscription, signed in to with a code entered on any device
Which one? Type its number or its name, or nothing to stop:
```

A way that already holds a sign-in says so at the end of its line. For xAI that is what `auth status xai` says:
when the access token expires, that it is sent to `api.x.ai` alone, and that the refresh token is sent to
`auth.x.ai` alone.

Each way is the command it was before, run for you:

| Way | Same as | More |
|---|---|---|
| `brave` | nothing before it: a premium key typed at the terminal | [Brave's premium service](premium.md) |
| `bedrock` | the AWS sign-in a session makes on its first turn | [AWS Bedrock](providers/bedrock.md) |
| `import` | `bravebot import-providers` | [Importing](importing.md) |
| `gateway` | nothing before it: the key used to go in a variable or a settings file | [below](#gateway) |
| `chatgpt` | nothing before it: a sign-in in a browser | [ChatGPT](providers/chatgpt.md) |
| `xai` | no other command: a sign-in with a code entered on any device | [below](#xai) |

A script names the way, so nothing is asked: `bravebot auth login brave`,
`bravebot auth login bedrock`, `bravebot auth login import`, `bravebot auth login chatgpt`,
`bravebot auth login xai`. With no way named and no terminal to ask on, the command is refused and
prints these forms. `brave` and `gateway` are the exception, since the key is typed at the terminal and
never given on the command line. `chatgpt` shows an address to open and waits for the browser, and `xai`
shows an address and a code and waits for them to be used, so each needs a terminal on standard error.

**Brave** asks for a premium key without showing it and keeps it in `~/.bravebot/brave-key.json`. If a
key is already stored it asks before replacing it. `bravebot auth logout brave` forgets the key, which
still works at Brave until it is revoked there. See [Brave's premium service](premium.md).

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

**ChatGPT** opens a browser at OpenAI, where you sign in, and keeps the sign-in it is given in
`chatgpt-account.json` beside the other credential files. It prints the address as well, so a machine
with no browser can finish the sign-in elsewhere through a forwarded port, and it waits five minutes.
If a sign-in is already stored it asks before replacing it. The account's models are offered in
`/model` as `chatgpt/<model>`. Where nothing serves the model in force and no gateway or AWS account is
configured, `bravebot` lists the account's models in the terminal and asks which one to use, remembers the answer for the next start, and opens on
it. An answer that is not the number of a model, a start in a script or in an incognito session, and an
account whose models could not be listed are refused as before, with the forms that name a model, and a
listing that gave no model is said why. `bravebot auth logout chatgpt` forgets the copy here and ends nothing at
OpenAI. See [ChatGPT](providers/chatgpt.md).

## xAI

`xai` shows an address and a code on standard error and waits for them to be used:

```
$ bravebot auth login xai
Open this address on any device to sign in to xAI:
https://accounts.x.ai/oauth2/device?user_code=ABCD-EFGH
The code to enter, if the page asks for one: ABCD-EFGH
Waiting for the code to be used. It stops working in about 30 minutes.
signed in to xAI; the access token expires 2026-10-26T14:08:44Z. The sign-in is stored in /home/you/.bravebot/xai-account.json
```

Open the address on any device, the one you are typing on or a phone, sign in to xAI there and enter the
code if the page asks for one. The command asks xAI whether the code has been used every few seconds, opens
no browser, and ends when the code is used, when xAI answers that the sign-in is denied, or when it stops
working, which the command says. Where the sign-in was denied, the code stopped working or was not used in
time, nothing is stored and the sentence names `bravebot auth login xai` to try again. The tokens are kept in
`xai-account.json` beside the other credential files, readable only by your account. Signing in again
replaces them once it completes, and one that fails leaves what was stored. The refresh token, which is sent
only to `auth.x.ai`, can replace the access token.

Once the tokens are stored the account's models are offered in `/model` as `xai/<model>`, and a turn on one is
sent to xAI in chat completions with the access token as a bearer. The refresh token, which is sent only to
`auth.x.ai`, replaces the access token before it expires, inside the turn that needs it. Where nothing serves
the model in force and no gateway or AWS account is configured, `bravebot` started on its own in a terminal
lists the account's models and asks which one to
use, remembers the answer for the next start, and opens on it, as it does for a ChatGPT account. With both
accounts signed in it asks once, over both lists, and an account that could not be listed is said above the
other's models. An answer that is not the number of a model, a start in a script or in an incognito session, and
an account whose models could not be listed are refused, with the forms that name a model, such as
`bravebot -i "<prompt>" --model xai/<model>`, and a listing that gave no model is said why.
`bravebot auth logout xai` forgets the copy here and ends nothing at xAI. `bravebot auth status xai` says when
the access token expires, that it is sent to `api.x.ai` alone and that the refresh token is sent to `auth.x.ai`
alone, and `bravebot doctor` names
the sign-in and says what would end each token. See [xAI](providers/xai.md).

## Gateway

`gateway` stores an API key for `openrouter`, `openai` or `anthropic`, which bravebot knows the
endpoints of, or for a gateway a provider block names, such as an
[OpenAI-compatible gateway](providers/openai-compatible.md). A key for one of the three needs no
block written first:

```
$ bravebot auth login gateway
Services a key can be stored for:
  1. work        gateway.work.example (a key stored)
  2. openrouter  openrouter.ai
  3. openai      api.openai.com
  4. anthropic   api.anthropic.com
Which one? Type its number or its id, or nothing to stop: 3

Key for openai, sent to api.openai.com (not shown as you type):
the key for openai is stored in /home/you/.bravebot/gateway-keys.json, and /home/you/.bravebot/settings.json names openai under provider now, so sessions started from now on send it to api.openai.com
```

The list is the gateways your settings name, then each of the three that no block names, each with
the host its key is sent to. A block with the id of one of the three is listed once, with its own
host. Where the machine-level file names the gateways, only those are offered, since an entry
written in your settings would not be read there, and the list and the refusal of any other id name
that file.

With one service offered, or one named as in `bravebot auth login gateway openai`, it asks for the
key straight away. Nothing appears as the key is typed or pasted. Ctrl-U starts it again, and
Escape, Ctrl-C, Ctrl-D or Enter on nothing stops without storing anything. A service that already
has a key stored asks before replacing it.

The key is kept in `~/.bravebot/gateway-keys.json`, readable only by your account, under the
service's id. It holds no host, so a key goes only where the provider block with that id sends
requests. For one of the three that no block names, the sign-in adds `"provider": {"openai": {}}`
to `~/.bravebot/settings.json` once the key is stored, so sessions send the key to the host built
into bravebot. The file is rewritten whole, so its keys come out in order of name and its spacing is
replaced. A file with a comment in it is refused before the key is asked for, and is never
rewritten. The entry names no variable, so `OPENAI_API_KEY` in your shell is not read for it: name
it in the block's `env` to have it used. Nothing is written where a block or the file already names
the id. A settings file that cannot take the entry, because it is not a settings document, is past
the size bravebot reads, or names the id as something that is not a provider block, is said before
the key is asked for, and nothing is stored. A variable the block's `env` names wins over a stored
key while it is set, and the sign-in says so when one is set. A session reads the files when it
starts, so one already open keeps what it had.

A key stored with no block naming its id is sent by no session. Running the sign-in for that id
again records the block, whether you replace the key or keep it.

Storing the key does not choose a model. Until one of the service's is named, `bravebot` refuses to
start with `no model service is configured yet` and a line naming the
[`model` key](configuration.md#model), because the model in force is still the one the build came
with. The next step is to name one, as `openai/<model id>` in the `model` key or after `--model`.
[OpenAI](providers/openai.md) shows the block and the `model` key, and the pages for
[Anthropic](providers/anthropic.md) and [OpenRouter](providers/openrouter.md) show theirs.

`bravebot auth logout gateway openai` forgets the key. With only one stored the id can be left off.
Forgetting it here does not revoke it: it works at the service until you revoke it there, and the
line names the host. Logging out leaves the provider entry in your settings, and `auth status` and
`doctor` then say that none is needed, until the entry is removed or a key is stored again.

## Checking a sign-in

```sh
bravebot auth status brave
```

prints whether a premium key is stored, and exits 0 only if one is, so a script can test it. `bedrock`,
`gateway [id]`, `chatgpt` and `xai` ask about the others. `gateway openai` says whether a
session would send a key for `openai`, and a key stored for it with no block naming it is reported
as not signed in. With no way named it asks about all of them and exits 0 if any is usable. See [the
reference](../reference/cli.md#auth).

## In an incognito session

In an [incognito session](../using/incognito.md), `brave`, `import`, `gateway`, `chatgpt` and `xai` are
refused, since each keeps something in bravebot's own directory.
`bedrock` is allowed: the AWS CLI keeps that session, and a session in this mode signs in to it as
well. It writes nothing to settings, so `BRAVEBOT_USE_BEDROCK=1` still has to be exported. Forgetting a
premium key or a gateway key and signing out of ChatGPT or xAI are allowed.
