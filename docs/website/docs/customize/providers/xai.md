---
sidebar_position: 11
title: xAI
description: Sign in with a SuperGrok subscription with a code entered on any device, and use the models the account reaches.
---

# Reaching a model with an xAI SuperGrok subscription

A SuperGrok subscription is reached by signing in to the account with a code, and the models the account may
use are then offered in `/model` beside every other service's. A pay-per-token xAI API key is a different
route: it is a [gateway](openai-compatible.md) entry, and it needs none of this.

```sh
bravebot auth login xai
```

bravebot presents the client identifier that xAI's open-source Grok Build sets as its default and that
opencode presents. The maintainer stated that bravebot has permission to present it.

Every statement on this page about xAI's service was observed once, with one account on 2026-10-26, except where
it says that something was not observed. [What was observed](#what-was-observed) says what was observed and what
was not tried.

## What the command does

1. It asks xAI for a code, and prints an address, the code to enter if the page asks for one, and how long
   the code lasts. It opens no browser and starts no program.
2. You open the address on any device, the one you are typing on or a phone, and sign in to xAI there.
3. bravebot asks xAI whether the code has been used every few seconds. It waits for an hour at most, whatever
   xAI says the code lasts, and the wait between two asks is never more than 63 seconds. A request that the
   network drops is asked again at the next interval, up to three in a row, and a fourth ends the sign-in, so a
   machine that cannot reach xAI is told so about 24 seconds later, or up to about 114 where each connection
   hangs. Each request is made on a connection of its own.
4. When the code is used it stores the access token, the refresh token and when the access token expires,
   and says so, and then says how to reach the account's models: run `bravebot` to be asked which to use where
   a start would ask, or name one with `--model xai/<model>`. It keeps nothing else of xAI's reply: the token
   that carries your name and email is read for nothing and stored nowhere.

The command stops with nothing stored, in one sentence, when xAI answers that access or authorization is
denied, when the code stops working before it is used, when the time it waits for a code passes, or when a
request to xAI fails. The sentences for a denial, a code that stopped working and the wait that passed name
`bravebot auth login xai` to try again, and the two for a request that failed say that nothing was stored. It is refused, before the code is asked
for, in an [incognito session](../../using/incognito.md), where standard error is not a terminal, and where a
`provider` entry named `xai` is a gateway, since one name cannot select two services. A sign-in that
completes replaces a stored one without asking.

## Naming a model

A model of the account is named `xai/<model>`, in `--model` or the `model` key, and `/model` offers them
once a sign-in is stored. Only the part after `xai/` is sent. A name without it never selects the account,
so `grok-4.5` stays a gateway's model of that name, or Brave's. A name that begins with `xai/` is the
account's even where a gateway lists a model of that name, and that gateway's model is named with the
gateway's id ahead of it, as `litellm/xai/<name>`.

`/model` offers what xAI lists for the account, leaving out the models its chat route refuses. xAI's
listing does not say which those are, so a rule written into bravebot keeps an id that is `grok-` and then a
digit or `build-`, without `multi-agent`, `imagine`, `image` or `video`. On the day it was observed that kept seven of fourteen
ids: `grok-4.20-0309-non-reasoning`, `grok-4.20-0309-reasoning`, `grok-4.3`, `grok-4.5`, `grok-4.6`,
`grok-4.7` and `grok-build-0.1`. A model the rule leaves out is still reached by naming it. A model it keeps
that the route refuses is refused at the first request like any other. The aliases xAI lists for a model are
not offered. Each row has the window the listing states. None is offered without a sign-in, and where the
listing cannot be read `/model` says so and offers nothing for the account, since no list is built in.

A person who has signed in to xAI and configured nothing else has no model name to give, so `bravebot` started
on its own in a terminal, outside an incognito session, asks which of the account's models to use. It lists them
by number, takes a number, remembers the answer as `/model` does, and opens on it, and the next start asks
nothing. A person who has signed in to ChatGPT as well is asked once, over both accounts' models. An account
that could not be listed is said above the other's models, and where none could be, or where the start is in a
script or an incognito session, or names a model, nothing is asked: the start ends saying that an xAI sign-in
is stored and naming `bravebot -i "<prompt>" --model xai/<model>` and `bravebot "<task>" --model xai/<model>`.
A person who has a gateway or an AWS account configured is asked about that service's models and not the
account's, and the stored xAI sign-in is not read then. The same models are in `/model` once a session is open, or
set the `model` key.

An entry named `xai` that is a gateway keeps the name: no model of the account is offered then, and
`auth login xai` is refused until it is renamed.

No effort level is sent to the account. The listing states levels for four of its models, in a field
bravebot does not read, and the service was not asked with one, so `/effort` changes nothing for a model of
the account, and the terminal says the model reads none where a level is in force.

## What is sent where

Every destination is compiled in. No page, reply, model, settings file or variable names one, and a redirect
from any of them is a failure and is not followed.

| Destination | Takes |
|---|---|
| `https://auth.x.ai/oauth2/device/code` | the request for a code |
| `https://auth.x.ai/oauth2/token` | the device code and the refresh token |
| `https://api.x.ai/v1` | the access token, with the model requests and the listing |

The access token goes to the last of these and to no other, and the device code and the refresh token go to
the second and to no other. A request to the account is made in chat completions, as a request to any gateway
is, with the access token as a bearer and nothing else: a gateway's key is never sent to xAI, and the account's
token is never sent to a gateway.

## Staying signed in

The access token is replaced before it expires, with the refresh token, inside the turn that needs it. xAI
replaced the refresh token at a refresh, observed once with one account, and whether the previous one still works
was not tried, so a renewal the file could not take may leave a refresh token that no longer works. The terminal
interface says so once, in the turn that made the renewal, with what to check. A one-shot or plain run that
succeeds prints nothing of it, and the next start finds the sign-in ended.

A request xAI refuses with a 400, a 401 or a 403 is sent again once with a new access token. xAI answered
an access token it could not validate, one that had been altered, with a 403, and a bearer that is not a token at
all with a 400, and a 400 is also what a request that is wrong is answered with, so bravebot decides from the
status alone: if the new token is refused with a 401 the turn ends with a sentence that says the sign-in was
refused and names `bravebot auth login xai`, if it is refused with a 403 the turn ends with the sentence for the
account below, and if it is refused with a 400 the request is the failure, as for any gateway. Nothing of xAI's
reply is read or repeated, since an error body carried the account's team identifier. A turn on an account model
with no usable sign-in ends before a request is sent, with a sentence that names the command. Each request
refused with a 400, a 401 or a 403 spends a refresh and costs at least one more request, and a refused token is
presented twice before it is replaced where the status is a 400. A model id the rule leaves out that you name is
refused with a 400 and does the same. Where xAI's token endpoint is limiting requests, the renewal's failure is
told in place of the 400, so a prompt that is too long is not shortened.

A request that xAI refuses with a 403 after the token was replaced ends the turn saying that xAI refused the
request for this account, that signing in again will not change that, and that you can check the plan at xAI or
choose another model. It names no command. No refusal for a plan was observed, and bravebot reads only the
status, so a 403 for another reason is told the same way. A 403 on a token refresh is told as a refresh that
failed. A listing of the models that xAI answers with a 403 is asked again once with a new token, and if it is
refused again shows no models for the account and says the listing could not be read.

`bravebot auth status xai` says when the access token expires, that it is sent to `api.x.ai` alone, and that the
refresh token is sent to `auth.x.ai` alone, and `bravebot doctor` says the same and what would end each token.
Neither prints a token.

## Signing out

`bravebot auth logout xai` removes the copy kept here and ends nothing at xAI. xAI's revocation endpoint
ended the refresh token when it was called, and an access token issued before it was accepted on a request made
right after, so a token that was copied may work until it expires. bravebot does not call the endpoint.

## What was observed

One account, one day, 2026-10-26. The access token lasted 21600 seconds. xAI replaced the refresh token at a
refresh. The listing held fourteen ids and the chat route answered seven of them, and refused the rest with a
400. It accepted the access token as a bearer on `GET /models` and `POST /chat/completions`.

Not tried: streaming and tool calls, a request that carries an effort level, how long a refresh token lasts,
what xAI answers an account on a plan that cannot use this, what happens to an access token past its expiry,
what a chat request answers an altered token (only the listing was tried, and it answered a 403), whether xAI
answers a request that names an alias such as `grok-code-fast-1` under the canonical id, and
`https://cli-chat-proxy.grok.com`, the host xAI's own command line client uses. The observations that could not
be made are to be made under brave/bravebot#3377.
