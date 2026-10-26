---
sidebar_position: 10
title: ChatGPT
description: Sign in with a ChatGPT Plus or Pro account in a browser, and use the models the account reaches.
---

# Reaching a model with a ChatGPT account

A ChatGPT Plus or Pro subscription is reached by signing in to the account in a browser, and the models
the account may use are then offered in `/model` beside every other service's. This is a different route
from an OpenAI API key, which [the `openai` entry](openai.md) takes and which needs none of this.

```sh
bravebot auth login chatgpt
```

Whether OpenAI's terms let a program other than its own use a subscription this way is not something
bravebot can tell you. bravebot stores a sign-in only when OpenAI's reply grants the scope that lets it
use the subscription's tokens and holds an ID token that verifies, and otherwise stops with a sentence
saying which.

Several values here, such as the names in the built-in model list, are read from another program's source
and have not been checked against OpenAI's service. Others are stated in OpenAI's documentation, and have
not been checked against the service either.
[What has been observed](#what-has-been-observed) and [What the documents state](#what-the-documents-state)
say which.

## What the command does

1. It starts a listener on `127.0.0.1` at a port the operating system chooses, prints an address on
   `auth.openai.com`, and asks the platform's own opener (`open`, `xdg-open` or `rundll32`) to open it.
   The address is printed whether or not a browser opens, with a line naming the port.
2. You sign in at OpenAI in the browser. The first time, OpenAI registers a client for bravebot, named
   `bravebot`, and that client is kept with the sign-in.
3. The browser returns to the listener, and bravebot exchanges what it was given for an access token, an
   ID token and a refresh token. It checks the ID token before storing anything. The tab says you can close
   it, and the terminal says the sign-in is stored, when the access token expires, and where.
4. The account is then asked which models it offers. The command says to run `bravebot`, which asks which
   one to use, where nothing serves the model in force, and otherwise to name one, that the account reaches no
   model, or that the listing could not be read. The sign-in is stored first, so a slow answer leaves you
   knowing it was kept.

The ID token is checked against the key set OpenAI publishes, and the check covers its signature, its
issuer, its audience, its expiry, the one-time value bravebot sent and that it names a subject. No other
claim in it is read, and none is stored, sent or shown. A reply with no ID token, or one that does not
verify, ends the command with nothing stored. The key set is fetched with no credential.

The command waits five minutes for the browser and then stops, storing nothing. The listener takes the
one request that carries the value bravebot generated for this sign-in and closes. Any other request to
the port is answered with a fixed page and ignored, and the wait goes on. It reads nothing from the
request but the authorization code, that value and the client OpenAI issued.

**On a machine with no browser,** open the printed address in a browser elsewhere. That works only if the
browser can reach the port on this machine, which means the same machine or a forwarded port. The port
changes at every sign-in, and there is no device-code route. Where the browser cannot reach it, the
redirect fails there and the command ends at the time limit.

The command is refused, before a browser opens, in an [incognito session](../../using/incognito.md),
where standard error is not a terminal, and where a `provider` entry named `chatgpt` states an endpoint,
since one name cannot select two services. Where a sign-in is already stored it asks before replacing it,
and is refused in the same way where standard input is not a terminal.

## Naming a model

A model of the account is named `chatgpt/<model>`, in `--model` or the `model` key, and `/model` offers
them once a sign-in is stored. Only the part after `chatgpt/` is sent. A name without it never selects the
account, so `gpt-5.5` stays the `openai` entry's model of that name, or Brave's, and a choice remembered
as one cannot change service. The reverse holds for a name that begins with `chatgpt/`: it is the account's
even where a gateway lists a model of that name. That gateway's model is named with the gateway's id ahead
of it, as `litellm/chatgpt/<name>`, which is also how its row appears in `/model`.

`/model` offers what OpenAI lists for the account, leaving out rows the listing marks hidden or not
available through its API. Each row has the window the listing states and whether any effort level is
listed for it, and a row that lists none takes no effort level. None is offered without a sign-in.

Where the listing cannot be read, `/model` says so, offers a list of model names built into bravebot, and
says that a row may name a model the account cannot reach. That list is read from another program's
source and none of its names has been checked against the service, so the service refuses at the first
request a model the account cannot reach. A listing that is read and is empty offers nothing, and an
account that reaches no model is still signed in. A listing OpenAI refuses in one of the codes that name
the account, such as its usage limit, offers nothing and says why.

**To offer the models you name instead,** put a `provider` entry named `chatgpt` in your settings with no
endpoint:

```json
{
  "provider": {
    "chatgpt": {
      "models": {
        "<model name>": { "limit": { "context": 272000, "output": 128000 } }
      }
    }
  }
}
```

Those are the models offered, nothing is asked of OpenAI to list them, and the access token is not
renewed for it. A model's window is its `limit` where both `context` and `output` are stated. No other
field of the entry or of a model is read, `options.contextWindow` included, and nothing in it is carried
to a request. An entry that states an endpoint is a gateway as any other entry is, and keeps the name:
no model of the account is offered then, and `auth login chatgpt` is refused until it is renamed.

## What is sent where

Every destination is compiled in. No page, reply, model, settings file or variable names one, and a
redirect from any of them is a failure and is not followed.

| Destination | Takes |
|---|---|
| `https://auth.openai.com/api/accounts/authorize` | the browser, for the sign-in |
| `https://auth.openai.com/api/accounts/oauth/token` | the authorization code and the refresh token |
| `https://auth.openai.com/.well-known/jwks.json` | nothing: the key set the ID token is checked against |
| `https://api.openai.com/v1` | the access token, with the model requests and the listing |

The access token goes to the last of these and to no other, and the code and the refresh token go to the
second and to no other.

A request to the account is made in the Responses protocol, as an [`openai` entry that names
`@ai-sdk/openai`](openai.md#the-responses-api) is, and nothing is asked to be kept at OpenAI. It carries the
access token, three headers that hold an identifier bravebot generated, and `content-encoding: zstd`, and
nothing a settings file or a reply adds. The body is compressed into one zstd frame whatever its size,
because the service refuses an uncompressed body over about 1 MiB. A frame over 1 MiB is not sent, no
token is spent on it, and the turn is told the body is too large and shortens the conversation.
`OPENAI_API_KEY` and a gateway's key are not looked for, and the account's tokens are never sent to a
gateway.

## Staying signed in

The access token is replaced before it expires, with the refresh token, inside the turn that needs it.
Sessions on one machine take turns at that, since OpenAI replaces the refresh token each time it is used.
A token OpenAI turns away early is replaced once and the request sent again.

If OpenAI refuses the refresh token as no longer valid, the stored copy is removed and the turn says the
sign-in has ended: run `bravebot auth login chatgpt` again. bravebot reads that from the error code the
OAuth standard names for it, which this server has not been seen to send, so a sign-in it ends in another
way leaves the file in place and each turn is told the renewal failed until you sign in again. OpenAI's
documentation lists five other codes for a refresh token that cannot be used, which bravebot does not read.
If the renewed tokens could not be saved, the turn says so once, and they last until bravebot exits. A
sign-in is used in an incognito session as in any other, and a renewal is still written to the file there.

A turn that fails on the account says which failure it was in a sentence of its own, with what to run
where a command is the remedy, and no count of attempts after it. A refusal that carries one of seven
error codes ends the turn in a sentence of its own: one says the account has reached its usage limit, and
the others say it may not be used this way. Where the refusal came as a 429 that asked for a wait, the
sentence says how long, in minutes rounded up. The line a failed turn leaves in the terminal and the error
a one-shot run prints add the status after the sentence, as `(HTTP 429)`. `/compact`, the line a turn says
when it could not summarise the conversation, the window and an editor give the sentence alone
([Troubleshooting](../../troubleshooting.md)). The codes are read from another program's source, are listed
in OpenAI's documentation, and have not been seen from this service, so a refusal that says the same in
another code is handled by its status alone. A refusal with one of them is not repeated, and no
[fallback model](../configuration.md#fallbackmodel) is tried, since the account answers for every model it
serves.

## Checking and forgetting

```sh
bravebot auth status chatgpt
```

prints one line: signed in, with the time the access token expires, not signed in, with the command, or
unusable, with the reason and what to run. It exits 0 only where the sign-in is usable. A token past its
time is still signed in, since the next request replaces it. The line holds no token, identifier, email or
account name. Where a sign-in is stored, [`bravebot doctor`](../../reference/cli.md#doctor) names it with
when the access token expires and what would end it. It says nothing of the sign-in where none is stored,
and where the file cannot be read it says so and the report fails.

```sh
bravebot auth logout chatgpt
```

removes the stored copy, and is allowed in an incognito session. It ends nothing at OpenAI. OpenAI's
documentation describes a revocation call a client can make at sign-out, which bravebot does not make.
bravebot's record is that signing out of the ChatGPT account, or removing bravebot's access in the account's
settings, ends the sign-in there. That has not been observed, and an access token already issued may run
to its own expiry.

## What is kept

The sign-in is one file, `chatgpt-account.json`, in the state directory beside the other credential files.
It holds the access token, the refresh token, when the access token expires, the client OpenAI issued and
an identifier bravebot generated for this machine. It does not hold the ID token. It is readable by your
account alone and is never a setting, a command-line argument or something a program bravebot starts is
given. No token or code reaches a log, a transcript, a session record or the terminal. The address printed
at the start holds values for this one sign-in and goes nowhere else.

One account is signed in at a time. Signing in again replaces it.

## What has been observed

A sign-in, a refresh, the listing of models and a streamed turn have been made against a real ChatGPT
account with bravebot's own build. That covers the authorization and token addresses, the registration of
a client, the scopes and fields the requests carry, the replacement of the refresh token on every use, the
listing's fields, and the events of a text turn, a tool call and a reasoning turn.

These are read from another program's source, opencode's, and have not been observed against the service.
[What the documents state](#what-the-documents-state) says what OpenAI's own documents add to some of them:

- the algorithm and the issuer of a real ID token. OpenAI's discovery document states both as bravebot
  expects them, OpenAI's documentation gives the same issuer, and this page records no ID token's header or
  claims. A real token that names another issuer would make every sign-in fail;
- that the authorization server accepts a redirect to any loopback port, where only a port the operating
  system chose has been tried. OpenAI's documentation says only the port may vary at later sign-ins;
- the names in the built-in model list, and in particular the ones ending in `-fast`;
- what the three identifiers do for the service's prompt cache;
- that the service accepts the zstd frames bravebot writes and the size at which it refuses a body, and
  whether that size is of the frame or of what it decodes to;
- the seven error codes that name the account, and where a refusal writes them. OpenAI's documentation gives
  the seven with their statuses and names no `error` event.

Whether an access token issued before a sign-out keeps working is in no source bravebot was written from.
OpenAI's documentation lists the codes of a refused refresh without a status, and does not say what an
expired or revoked access token returns.

## What the documents state

OpenAI's key set and its discovery document, which anyone can read, were read with no account on
2026-10-25. The key set is at the address bravebot uses and holds four RSA keys for RS256 signatures, each
with a 2048-bit modulus. The discovery document names the authorization, token and key set addresses
bravebot uses, states the issuer bravebot compares an ID token with, says ID tokens are signed with RS256
and that `S256` is the only challenge method, lists a revocation endpoint, and advertises no way to bind a
token to the program that holds it. Neither document shows what a real ID token holds.

OpenAI also publishes documentation of this sign-in, which was read on 2026-10-25. It states the redirect
rule, the checks a client makes on an ID token, the seven error codes that name the account and two more that
mean try again later, six error codes for a refresh token that cannot be used, and the revocation call a
client makes at sign-out. These are statements of the documentation and show nothing about what the service
did for bravebot. bravebot differs from it in several places. Among others, it reads one of the six refresh
codes, it forgets its client and host identifier when you sign out, it keeps no ID token, and it makes no
revocation call at sign-out.

The design is [specs/chatgpt-sign-in.md](https://github.com/brave/bravebot/blob/main/docs/specs/chatgpt-sign-in.md).
