---
sidebar_position: 4
title: Leo Premium
description: Import a Leo Premium subscription so requests go to the premium tier.
---

# Leo Premium

Brave Bot can register itself against a Leo Premium subscription in a Brave install on this machine
and spend its own credentials on the premium tier.

```sh
bravebot import-leo-creds            # from the stable Brave install
bravebot import-leo-creds beta
bravebot import-leo-creds nightly
bravebot import-leo-creds development
bravebot import-leo-creds --forget   # remove what was imported
```

Without a channel, `stable` is what importing means.

## What it does, and does not do

**This registers as a new device.** Only the subscription's *order id* is read from the browser
profile. The credentials themselves are generated here and signed by Brave, exactly as a second
browser on another machine would do it. The browser keeps its own, and nothing it holds is spent.

Before anything is registered, the order is checked: an unpaid order, one asking for no credentials,
one asking for an implausible number, and one without interval metadata are all refused. The Leo item
is picked out of a multi-item order rather than assumed to be the only one. A batch is then verified
against the issuer's key before it is stored, and tokens are matched by value rather than by position.

## How credentials are spent

Credentials arrive in batches covering a few days and are spent one per request. A spent credential is
never offered again, consecutive spends hand out different credentials, and spending past the end of a
batch is refused.

**A credential is never sent to the non-premium host.** The premium host and the credential travel
together, because a credential belongs to a deployment. A build with no premium host configured stays
on the free tier rather than sending one where it does not belong.

Nothing is written back unless a credential was actually spent, so a session that spends nothing never
touches the store.

## Where they are kept

In one file under `~/.bravebot`, created mode 0600 before anything is written to it and still 0600
after a re-import over an existing one. Nothing asks you for a password, and the file is not
encrypted at rest, as the browser these are imported from keeps the same secret unencrypted in its own
profile.

**One file, not one per channel.** You have one subscription however many Brave builds are installed,
so importing from Nightly replaces what was imported from Stable rather than sitting beside it. The
channel only says which browser profile to read the order id from, which is a fact about your machine
rather than about the agent, so `--forget` takes no channel. Forgetting removes the file, and is not
an error when there was nothing to remove.

A malformed or empty file is reported as such rather than treated as absent credentials, and a
credential without a token is rejected on load. With no home directory there is nowhere a secret
belongs, and that is reported rather than guessed at.

## When a subscription cannot be read

Finding nothing has two causes, and they are not the same fact.

**Nothing imported** is the free tier working as intended, and nothing is said about it. An endpoint
belonging to no environment, such as a local one, is this case too. No credential belongs near it by
design.

**A batch that exists and cannot be spent** is reported to you, with the reason and what to do about
it. That covers a file that could not be read, one another version wrote, and one imported for an
environment this endpoint does not accept. A credential only verifies against the deployment that
signed it, so a batch from the wrong Brave channel is refused with the reason rather than passed
over.

The downgrade is said out loud because its only other symptom is the agent appearing to get worse for
no reason. The request goes out on the free tier, where the endpoint answers a premium model name by
**substituting a weaker model rather than failing**: a 200 and an ordinary reply. A request that
silently lost its credential still returns something that reads like an answer, and nothing else on
your screen would connect that to the store.

## Which tier a turn actually ran on

**What `/status` says about the tier is what the last turn actually did**, not what the build was
compiled with. Before the first turn it says premium is *available*, rather than claiming it is or is
not in use.

The opening screen draws the tier beside the confinement, from the configuration, in the same words
`/status` uses before a turn has run. It deliberately does **not** read the credential store: a
stored batch may be expired, exhausted, or issued for another environment, so finding one would not
settle the tier either. A pane too narrow for the wordmark still reports both.

Where the server reports using a model other than the one you asked for, **both are shown**: the
choice you made and the model that actually answered. This is said once when it starts happening
rather than every turn. The automatic entry resolving to a concrete model is not a substitution:
that is the server choosing per request, which is what it means.

## Requirements and limits

- **macOS and Linux**, including a machine with no desktop session. Nothing here needs one. Windows
  is not supported.
- The build must know the premium host. Without it, premium is unavailable.
- A credential only works against the deployment that issued it, so import from the Brave channel
  matching the environment the binary is configured for. A mismatch is refused before a request is
  made, rather than sent and answered with a 401.
- Sign in to Leo in that Brave install first: a subscription that is not in the profile cannot be
  imported.
- The stored batch is a bearer secret in a file you own. It is not encrypted at rest, which is what
  the browser does with the same secret, and anything running as you can read it. That includes a
  program `run` launches, which is [deliberately unconfined](../security/permissions.md).

## Checking what you have

```sh
bravebot doctor
```

reports whether a subscription is imported, which environment it was issued for, and how many of its
credentials are still unspent. Counts only: a credential is a bearer secret, so none of it is
printed.
