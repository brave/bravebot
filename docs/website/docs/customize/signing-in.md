---
sidebar_position: 10
title: Signing in
description: Model-service authentication commands, xAI and gateway sign-in, credential status checks and authentication during incognito sessions.
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
  4. gateway  A key for openrouter, openai, anthropic, google-vertex (Gemini on Google Vertex AI) or a gateway a provider block in settings names, typed here and kept by bravebot
  5. chatgpt  A ChatGPT Plus or Pro account, signed in to in a browser
  6. xai      An xAI SuperGrok subscription, signed in to with a code entered on any device
  7. google   Google, for Gemini on Vertex AI, signed in to with the Google Cloud CLI in a browser and a project named here, with no key kept by bravebot
Which one? Type its number or its name, or nothing to stop:
```

A way that already holds a sign-in says so at the end of its line. For xAI that is what `auth status xai` says:
when the access token expires, that it is sent to `api.x.ai` alone, and that the refresh token is sent to
`auth.x.ai` alone. For Google it is `(signed in)`, shown where the Google Cloud CLI gives an access token for
a project your settings name; drawing the list asks the CLI for a token and never starts a sign-in or a
browser, and waits at most five seconds for it.

Each way is the command it was before, run for you:

| Way | Same as | More |
|---|---|---|
| `brave` | nothing before it: a premium key typed at the terminal | [Brave's premium service](premium.md) |
| `bedrock` | the AWS sign-in a session makes on its first turn | [AWS Bedrock](providers/bedrock.md) |
| `import` | `bravebot import-providers` | [Importing](importing.md) |
| `gateway` | nothing before it: the key used to go in a variable or a settings file | [below](#gateway) |
| `chatgpt` | nothing before it: a sign-in in a browser | [ChatGPT](providers/chatgpt.md) |
| `xai` | no other command: a sign-in with a code entered on any device | [below](#xai) |
| `google` | nothing before it: the Google Cloud CLI's sign-in, run for you, and the project recorded | [below](#google) |

A script names the way, so nothing is asked: `bravebot auth login brave`,
`bravebot auth login bedrock`, `bravebot auth login import`, `bravebot auth login chatgpt`,
`bravebot auth login xai`, `bravebot auth login google`. With no way named and no terminal to ask on, the command is refused and
prints these forms. `brave`, `gateway` and `google` are the exception, since the key or the project is typed at the
terminal and never given on the command line. `chatgpt` shows an address to open and waits for the browser, and `xai`
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

## Google

`google` signs in to Gemini on Google Vertex AI with the Google Cloud CLI, so no key is stored. It needs
[`gcloud`](https://cloud.google.com/sdk/docs/install) installed, a terminal, a Google Cloud project that has the
Vertex AI API enabled, and a role on it that may call the API, such as Vertex AI User:

```
$ bravebot auth login google
Google Cloud project that has the Vertex AI API enabled, or nothing to stop: example-project-1
Go to the following link in your browser: https://accounts.google.com/o/oauth2/auth?...
signed in to Google with the Google Cloud CLI
/home/you/.bravebot/settings.json names example-project-1 under provider.google-vertex now, so sessions started from now on reach aiplatform.googleapis.com with the Google Cloud CLI's access token and no key
```

The command checks everything that needs no answer first: that you are not in an incognito session, that
`gcloud` is installed (it says where to get it if not), that a terminal is there, and that the settings file can
take the block. It then asks for the project, except where a `google-vertex` block or `GOOGLE_CLOUD_PROJECT` already
configures the service, which names its own project and is signed in as it is. A project holds letters, digits, `_`, `.`, `:` and `-` and starts
with a letter or a digit. Anything else is refused with exit status 2, without being repeated, before `gcloud`
starts. Typing nothing stops without running anything.

It then runs `gcloud auth application-default login`, the sign-in a session makes before a turn when the CLI
gives no token, and shows each line the CLI writes as it arrives, so the address to open appears while the CLI
waits. Where the CLI already gives a token it opens no browser and says so. A sign-in that fails or is
cancelled records nothing, exits 1 and says so in one fixed sentence; what the CLI wrote after the lines already
shown is not repeated.

Once the sign-in is made, `~/.bravebot/settings.json` gains
`"provider": {"google-vertex": {"options": {"project": "<project>", "location": "global"}}}` and no key, variable
or file. No location is asked for. Edit `location` in the block to reach another one. The settings you had are
kept. Where a `google-vertex` block or `GOOGLE_CLOUD_PROJECT` already configures the service, no project is asked
for and nothing is written. The command runs the sign-in and says the configuration takes the CLI's token and
names its own project, or, where it names a key or another credential, that it was left as it is.

The next session answers a `google-vertex/google/...` model with the CLI's token and no key, as
[Google Vertex AI](providers/google-vertex.md#signing-in-with-google-cloud-credentials-instead-of-a-key) says.
`bravebot auth status google` asks the CLI for a token, never for a sign-in, and exits 0 only if it gives one.
It says not signed in where no block or variable names a project, and unusable where `gcloud` is missing or
does not answer within twenty seconds. The token and the account are never printed.

`bravebot auth logout google` removes the block `auth login google` wrote, only where it still holds exactly
that, and leaves any other `google-vertex` block, and a block a stored key is sent to, as they are. It does not end
the CLI's sign-in by default, because other programs on the machine read the same Application Default
Credentials, and it says the sign-in still works. At a terminal it then asks whether to end it too. Only a yes
runs `gcloud auth application-default revoke --quiet`, and any other answer leaves the credentials in place. In a
script the command asks nothing and runs nothing, so `gcloud auth application-default revoke` is the way to end it
there.

## Gateway

`gateway` stores an API key for `openrouter`, `openai`, `anthropic` or `google-vertex` (Gemini on
Google Vertex AI), which bravebot knows how to reach, or for a gateway a provider block names, such as an
[OpenAI-compatible gateway](providers/openai-compatible.md). A key for one of the four needs no
block written first:

```
$ bravebot auth login gateway
Services a key can be stored for:
  1. work           gateway.work.example (a key stored)
  2. openrouter     openrouter.ai
  3. openai         api.openai.com
  4. anthropic      api.anthropic.com
  5. google-vertex  aiplatform.googleapis.com (Gemini on Google Vertex AI)
Which one? Type its number or its id, or nothing to stop: 3

Key for openai, sent to api.openai.com (not shown as you type):
the key for openai is stored in /home/you/.bravebot/gateway-keys.json, and /home/you/.bravebot/settings.json names openai under provider now, so sessions started from now on send it to api.openai.com
```

The list is the gateways your settings name, then each of `openrouter`, `openai` and `anthropic` that no
block names, then `google-vertex` where no block and no variable names it, each with the host its key
is sent to. A block with the id of one of them is listed once, with its own host. Where the
machine-level file names the gateways, only those are offered, since an entry written in your settings
would not be read there, and the list and the refusal of any other id name that file.

With one service offered, or one named as in `bravebot auth login gateway openai`, it asks for the
key straight away. Nothing appears as the key is typed or pasted, including a key pasted the moment the
question appears. Where the terminal cannot be set to show nothing, the key is not asked for. Ctrl-U starts it again, and
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
key while it is set, and the sign-in says so when one is set. A stored key wins over the file the
block's `options.keyFile` names and over `options.apiKey`. A session reads the files when it
starts, so one already open keeps what it had.

A key stored with no block naming its id is sent by no session. Running the sign-in for that id
again records the block, whether you replace the key or keep it.

### Gemini on Google Vertex AI

`google-vertex` is the id bravebot reaches Google Vertex AI under. Picking it asks two questions before
the key, and the answers decide which host the key is sent to:

```
$ bravebot auth login gateway google-vertex
Google Cloud project to use, or nothing for express mode, which needs only a key:
```

- Typing nothing is express mode. The key alone reaches Google's native `generateContent` API at
  `aiplatform.googleapis.com`, no location is asked for, and the entry recorded in
  `~/.bravebot/settings.json` is `"provider": {"google-vertex": {}}`.
- Typing a project asks for a location next (`Location, such as us-central1, or nothing for global:`).
  Nothing typed is `global`. The entry states both, and the key is sent to
  `aiplatform.googleapis.com` for `global` and to `<location>-aiplatform.googleapis.com` for any other
  location:

  ```json
  {
    "provider": {
      "google-vertex": { "options": { "project": "<project>", "location": "<location>" } }
    }
  }
  ```

A project holds letters, digits, `_`, `.`, `:` and `-` and starts with a letter or a digit. A location
holds lowercase letters, digits and `-`. A value that holds anything else is refused before the key is
asked for, with exit status 2, and nothing is stored or written. The sentence names which answer was
wrong and does not repeat it. Ending the input at either question stops without storing anything.

The key is typed with nothing shown, as for the other gateways, and kept in
`~/.bravebot/gateway-keys.json` under `google-vertex`. Where a block already names `google-vertex`, or
`GOOGLE_API_KEY` or `GOOGLE_CLOUD_PROJECT` configures the service, neither question is asked and no
entry is written, since an entry would replace what is configured. In the second case the sign-in says
that the stored key is read after an exported `GOOGLE_API_KEY`, so it is sent only while that variable
is not exported and `GOOGLE_CLOUD_PROJECT` or the `env` block of settings names the service. Where a settings entry for
`google-vertex` configures no service, the sign-in says so before it asks anything.

Storing the key does not choose a model. Name one as `google-vertex/google/gemini-2.5-flash` in the
`model` key or after `--model`. [Google Vertex AI](providers/google-vertex.md) lists the models
bravebot offers and says how a project is reached with Google Cloud credentials instead of a key,
which [`bravebot auth login google`](#google) does for you.
`bravebot auth login gateway` does not start `gcloud`.

`bravebot auth logout gateway google-vertex` forgets the key and names the host it still works at until
it is revoked: the one the entry in settings states, and `aiplatform.googleapis.com` where none does.
`bravebot auth status gateway google-vertex` reports a key stored with no entry as not signed in, and
an express entry with no key stored as not signed in, since Vertex AI refuses a request with no key. The
desktop app's Accounts settings do not ask for a project, so storing a key for `google-vertex` there is
refused and names this command.

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
`gateway [id]`, `chatgpt`, `xai` and `google` ask about the others. `gateway openai` says whether a
session would send a key for `openai`, and a key stored for it with no block naming it is reported
as not signed in. With no way named it asks about all of them and exits 0 if any is usable. See [the
reference](../reference/cli.md#auth).

## In an incognito session

In an [incognito session](../using/incognito.md), `brave`, `import`, `gateway`, `chatgpt`, `xai` and `google` are
refused, since each keeps something in bravebot's own directory, and `google` records the project in your settings.
`bedrock` is allowed: the AWS CLI keeps that session, and a session in this mode signs in to it as
well. It writes nothing to settings, so `BRAVEBOT_USE_BEDROCK=1` still has to be exported. Forgetting a
premium key or a gateway key, signing out of ChatGPT, xAI or Google, and `auth status` are allowed.
