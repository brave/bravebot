---
sidebar_position: 5
title: Brave's premium service
description: Store a premium key with bravebot auth login brave so requests for Brave's models go to Brave's premium service.
---

# Brave's premium service

Requests for Brave's models go to Brave's premium service when you have stored a premium key. The key is
sent as a bearer token. Brave's AI gateway is still in development, so prefer one of the other ways of
reaching a model for now: AWS Bedrock, a gateway, a ChatGPT account or an xAI account. See
[Signing in](signing-in.md).

## Storing the key

```sh
bravebot auth login brave
```

asks for the key in a terminal and shows nothing as you type. It stores the key in
`~/.bravebot/brave-key.json`, a small JSON file in the state directory (`BRAVEBOT_HOME` moves it). The
file is created readable by your account alone before the key is written to it: mode 0600 on macOS and
Linux, and an access list granting only your account on Windows. The key is written to a file beside it,
and that file is then renamed over it.

A key that is already stored is replaced only if you answer yes. The command is refused in these cases:

| Case | Exit status |
|---|---|
| standard input or standard error is not a terminal | 2 |
| an [incognito session](../using/incognito.md) | 1 |
| nowhere to keep the key | 1 |

```sh
bravebot auth logout brave
```

removes the file. It works in an incognito session. It ends nothing at Brave: the key still works there
until it is revoked there.

```sh
bravebot auth status brave
```

prints one line and exits 0 only when a key is stored:

```
brave: signed in: a premium key is stored and is sent to Brave's premium service alone
brave: not signed in: no premium key is stored; run bravebot auth login brave
brave: unusable: ...
```

The third form names the file when it cannot be read.

## What is sent, and where

The premium host is compiled in, so nobody sets an address. A build whose free endpoint is under
`brave.software` or `bravesoftware.com` has the development host and any other build has the production
host. Both are `https://ai-chat-premium.bsg.brave.software` while the premium service is live only in
development. A build that reaches no Brave backend (AWS Bedrock only) has no premium host and holds no
key.

With a key stored, a request for the Brave backend goes to the premium host's `/v1/chat/completions`
with `Authorization: Bearer <key>`. Such a request is not signed, so it carries no `digest` header and
no `Signature` authorization. A redirect in answer is a failure and is not followed.

`BRAVE_AI_CHAT_PREMIUM_ENDPOINT`, baked into a build or exported, is an optional override of the host. A
value with no scheme, or with a `?` or a `#`, is not used and the compiled-in host stands.

**The key is never sent to the free endpoint or to a host that is not Brave's.** A host is Brave's when it
ends in `.brave.com`, `.brave.software` or `.bravesoftware.com`. The host is the one a request connects
to, so a Brave name after a `?` or a `#` does not count: `https://attacker.example#.brave.com` is not
Brave's.

**Only a turn whose model Brave serves is handed the key.** A turn whose model is served by AWS Bedrock
or by a gateway is not, since those services were set up with credentials of their own.

Without a key, requests go to the free endpoint, signed with the services key built into the binary. A
session whose model is one of Brave's, with no key stored and no other model service configured (no
Bedrock block, gateway, ChatGPT sign-in or xAI sign-in), does not start. Instead it says no model
service is configured and lists the ways to configure one. The last is Brave's premium service, with
`bravebot auth login brave`.

A stored key that cannot be presented is reported as:

```
the premium key could not be used: {detail}. Run `bravebot auth login brave` to replace it, or `bravebot auth logout brave` to send requests to the free endpoint instead
```

## Which tier a turn actually ran on

**What `/status` says about the tier is what the last turn actually did**, not what the build was
compiled with. With a key stored, and before the first turn, it says "premium available, nothing sent
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
delegates' included. A session with no key stored presents none and is never stopped by a limit in
credits. See [`/limit`](../reference/commands.md#limit-amount--off).

## Checking what you have

```sh
bravebot doctor
```

reports the premium host on a `premium` line, as one of:

- `<premium url>, with a premium key`
- `<premium url>, without a premium key, so requests go to the endpoint above`

A build that reaches Brave always shows a URL. When the key file cannot be read, a `premium key` line
says so with the file's path, and the run fails with exit 1. The key's value is never printed. `doctor`
also lists the premium key among the credentials this build holds: it is issued by Brave's premium
service, which is also the only surface that revokes it, and `bravebot auth logout brave` ends this
machine's custody of the key and leaves the key live there.

## A file left by an earlier version

Earlier versions imported a subscription from a Brave profile and kept its credentials in
`~/.bravebot/leo-premium.json`. bravebot no longer reads or writes that file, and
`bravebot import-leo-creds` is a retired command that says to store a key with `bravebot auth login brave`
and exits with status 2. The file can be deleted by hand.
