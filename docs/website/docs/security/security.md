---
sidebar_position: 4
title: Security
description: What Brave Bot defends against, what it does not, and what leaves the process.
---

# Security

## What this defends against

**Indirect prompt injection.** Text that arrives from somewhere nobody vouched for (a web page, a
dependency's README, a build log, a program's output, a file in an untrusted directory) never reaches
the model deciding what to do next, and never reaches a decision in the Rust code either.

This is structural rather than instructional. The model is not asked to be careful with such content,
because it never has it: content is quarantined and the planner is handed a reference. What an
attacker can write into a file, they cannot turn into an instruction, because nothing in the pipeline
will read it as one.

The four things that would break it, and are therefore what a code review looks for:

1. **A branch on untrusted bytes.** The driver may carry untrusted content and hand it to an effect. It
   may not branch on it: no `if`, `match`, comparison or early return whose condition derives from
   untrusted bytes. A "careful refusal" computed from attacker-controlled text is still a decision an
   attacker took.
2. **The same branch, moved into the kernel.** Relocating a decision is not removing it, and "it is
   only for a message to the model" does not help, because a message to the model *is* the planner's
   context.
3. **A declassification outside the three gates.** Reading untrusted bytes is allowed only where they
   were already going: a filesystem write, an HTTP body, or a person's screen.
4. **A label built by hand.** Never construct a value with a better label than its inputs had. If a
   value derived from untrusted input has to be trusted for something to work, the design is wrong.

## What it does not defend against

Stated plainly, because an unlisted exception is indistinguishable from a violation.

- **Anything you vouch for.** `@a-file-you-have-not-read.md`, a dropped file, `! cat notes.md`, and
  answering yes to a directory all put content into the planner's context on the strength of your
  gesture. Nothing inspects the bytes, and nothing could.
- **A vouched-for command's output.** `a` at a run prompt makes what that command prints trusted. `git
  log` prints commit messages whoever contributed wrote.
- **What lands in a trusted directory afterwards.** A rule is about a path, not about the files that
  were in it. See [Trusted directories](trust.md#known-costs).
- **A program the agent was allowed to run.** Programs are not confined: they run with the access your
  own shell would give them, because `git push` needs `~/.ssh`.
- **The model being wrong.** Approval prompts exist because the planner can propose something you do
  not want, and reviewing the diff is the mechanism that catches it.

## Two deliberate exceptions

The policy layer looks at untrusted bytes in exactly two places, both written down rather than left to
be found.

**Splitting a processor's answer.** A processor returns one piece of text holding two things: a remark
for the person watching, and the document to be written. It marks where the document begins, and the
policy layer searches for that mark to find where to cut. The mark is not a boundary and cannot be
forged, because there is nothing to forge. The processor writes the whole answer and may put the mark
wherever it likes, and the first one counts. An attacker who owns the file gains: the ability to make
the write be refused, the ability to shift where the cut lands within content that was already
theirs, and the ability to put words in a remark that reaches your screen and stops there. What they
cannot do is choose *which* file is written, which stays the planner's choice plus your approval from
a diff.

**A trailing newline.** Before a file is written back, the code checks whether the file being replaced
ended in a newline, so the new one can end the same way.

## What leaves the process

There is **one way out**, and it is not optional: every outbound request carrying labelled content
goes through a single call, and the HTTP client is private to that module so no other crate can open a
second path.

- **Redirects are revalidated on every hop.** They are followed by hand and each new URL is put to the
  gate before it is fetched, so a permitted host cannot hand off to a denied one. The chain is bounded.
- **Only `http` and `https` ever reach the network.** Any other scheme is refused before a connection
  is attempted, rather than handed to a library to interpret.
- **A body is capped, and a truncated one says so.** A body that stops partway is a failure rather than
  a short success. This is resource hygiene, not content inspection. The bytes are never parsed to
  decide anything.
- **Each phase is bounded separately.** Connecting, starting to reply and continuing to reply are timed
  apart, so a slow answer is not confused with a dead connection.
- **Only "not now" is retried.** A connection that gave out is worth another attempt; a refusal is not.

One crate opens a socket of its own: the subscription client, for [Leo
Premium](../customize/premium.md). That traffic carries credentials and an order id, never workspace
content or model output, so no labelled value escapes the gate.

## Confinement

`bravebot doctor` reports the operating-system confinement available on your platform and the
mechanisms behind it, printed rather than assumed, because the guarantee differs by platform and
kernel.

Where confinement is used, it **fails closed**: if it cannot be established the process does not run,
rather than running unconfined. A profile starts denying everything and grants accumulate onto it, and
a policy that would confine nothing is rejected rather than applied. The network is denied unless it
was asked for.

Confinement does not cover the rest of the system. A processor is a model call made by our own code,
and a program you asked for runs with the access your own shell would give it. Everywhere else, the
boundary is the capability set and the label on a value.

## Data collection, usage, and retention

Brave does not use your data and does not store it. Prompts and used file contents are sent to Brave's
endpoint to produce a reply and are discarded once it has been produced. Nothing is retained and
nothing is used for training.

:::caution[Six lines about your machine go out with every request]
The system prompt states your working directory, whether it is a git repository, the platform, the OS
version, the shell and today's date. The working directory is an absolute path, so on most machines it
contains your username, and the OS version names your kernel build. There is no setting that withholds
them.

Nothing else about the machine is added: no environment variables beyond `$SHELL`, no hostname, no
username on its own, no file contents, no directory listing. See
[Where you are working](../customize/instructions.md#where-you-are-working) for why each is there.
:::

Local state is stored in `~/.bravebot` on your own machine: session records, prompt history and the
model you chose. Session records hold what the planner was allowed to hold. **Nothing untrusted is
ever written down**, by construction rather than by filtering, and quarantined content is not written
at all. A pasted picture is written, because it was part of your own message. Deleting the session
removes it.

Leo Premium credentials live in a mode-0600 file under `~/.bravebot`, readable only by you. They are
not encrypted at rest, which is what the browser they are imported from does with the same secret.
See [Leo Premium](../customize/premium.md#where-they-are-kept).

## Reporting a problem

Brave Bot is experimental and developed in the open. Please report security issues through the
[repository](https://github.com/brave-experiments/bravebot/issues), and see the
[mini-specs](https://github.com/brave-experiments/bravebot/tree/main/docs/specs) for the clause-level
statement of everything on this page.
