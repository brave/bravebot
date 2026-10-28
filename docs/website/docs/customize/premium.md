---
sidebar_position: 5
title: Brave's premium service
description: Set a premium key and a premium endpoint so requests for Brave's models go to Brave's premium service.
---

# Brave's premium service

Requests for Brave's models go to Brave's premium service when bravebot holds a premium key and a
premium endpoint. The key is sent as a bearer token. Brave's AI gateway is still in development, so
prefer one of the other ways of reaching a model for now: AWS Bedrock, a gateway, a ChatGPT account or
an xAI account. See [Signing in](signing-in.md).

## The two variables

| Variable | What it is |
|---|---|
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | the premium host |
| `BRAVE_AI_CHAT_PREMIUM_KEY` | the premium key |

Both are optional, and premium is used only when both are held. A key with no premium endpoint is
dropped. A premium endpoint that holds a query or a fragment is dropped too, which drops the key with
it.

**A build can bake them in.** A build made with either variable set holds it. Baked values are masked
in the binary so that `strings` does not print them; that hides them and does not encrypt them, so a
binary built with a key should be treated as holding one. A build without them works, and sends its
requests to the free endpoint. A source checkout sets them in `.envrc`, and `contrib/envrc.example`
shows how.

**The environment outranks what is baked.** A value exported in the environment is used in place of
the built-in one. A [settings file's `env` block](configuration.md#environment-variables) supplies
either variable when nothing is built in.

## What is sent, and where

When both are held and the premium endpoint's host is one of Brave's, a request for the Brave backend
goes to the premium endpoint's `/v1/chat/completions` with `Authorization: Bearer <key>`. Such a
request is not signed, so it carries no `digest` header and no `Signature` authorization. A redirect
in answer is a failure and is not followed.

**The key is never sent to the free endpoint or to a host that is not Brave's.** A host is Brave's
when it ends in `.brave.com`, `.brave.software` or `.bravesoftware.com`. The host is the one a request
connects to, so a Brave name after a `?` or a `#` in `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` does not count:
`https://attacker.example#.brave.com` is not Brave's.

**Only a turn whose model Brave serves is handed the key.** A turn whose model is served by AWS Bedrock
or by a gateway is not, since those services were set up with credentials of their own.

Without a key, requests go to the free endpoint, signed with the services key built into the binary. A
session whose model is one of Brave's, with no key held and no other model service configured (no
Bedrock block, gateway, ChatGPT sign-in or xAI sign-in), does not start. Instead it says no model
service is configured and lists the ways to configure one, premium among them.

**Programs bravebot starts do not see the key.** `run`, hooks, language servers and the AWS CLI are
started without `BRAVE_AI_CHAT_PREMIUM_KEY`, as they are without the services key and its id. See
[the run tool](../reference/run-tool.md).

## Which tier a turn actually ran on

**What `/status` says about the tier is what the last turn actually did**, not what the build was
compiled with. With a key held, and before the first turn, it says "premium available, nothing sent
yet". After a turn it says "premium, the premium key was sent" or "the premium key was not sent",
whichever that turn did. With no key it says "no premium key configured". A turn that ran on a model
named by an addressed definition or by a skill is recorded the same way, so `/status` does not stay on
"nothing sent yet" or on an earlier turn's tier after one.

The opening screen draws the tier beside the confinement, from the configuration, in the words
`/status` uses before a turn has run. A pane too narrow for the wordmark still reports both.

Where the server reports using a model other than the one you asked for, **both are shown**: the
choice you made and the model that actually answered. This is said once when it starts happening
rather than every turn. The automatic entry (`automatic-bravebot`, shown as **Automatic**) resolving
to a concrete model is not a substitution: that is the server choosing per request, which is what
picking it means.

## Limits

A session spend limit can count premium requests instead of tokens: `/limit 200 credits`, or
`"limit": "200 credits"` in a settings file. Each request that presents the key counts as one,
delegates' included. A session that holds no key presents none and is never stopped by a limit in
credits. See [`/limit`](../reference/commands.md#limit-amount--off).

## Checking what you have

```sh
bravebot doctor
```

reports the premium endpoint on a `premium` line, as one of:

- `<premium url>, with a premium key`
- `<premium url>, without a premium key, so requests go to the endpoint above`
- `not configured`

The key's value is never printed. `doctor` also lists the premium key among the credentials this build
holds: it is issued by Brave's premium service, which is also the only surface that revokes it, and
unsetting the variable or building without it ends this machine's custody of the key and leaves the key
live there.

## A file left by an earlier version

Earlier versions imported a subscription from a Brave profile and kept its credentials in
`~/.bravebot/leo-premium.json`. bravebot no longer reads or writes that file, and
`bravebot import-leo-creds` is a retired command that prints a message saying so. The file can be
deleted by hand.
