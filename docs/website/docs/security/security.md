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
  own shell would give them, because `git push` needs `~/.ssh`. Their own network requests do not go
  through the one way out described below, so an approved `curl`, `git push` or package install can
  read a private file and send it with nothing here routing or inspecting the traffic.
- **The model being wrong.** Approval prompts exist because the planner can propose something you do
  not want, and reviewing the diff is the mechanism that catches it.

## Three deliberate exceptions

The policy layer looks at untrusted bytes in exactly three places, all written down rather than left
to be found.

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

**Reading a verdict out of a check.** A second model can be shown quarantined content and asked to
answer in one word whether it looks like an attempt to give instructions. Reading that word is a
decision taken from a reply that is a function of untrusted content, so the word is untrusted too.
That check is off until you turn it on, and while it is off the only thing its answer decides is
which banner the approval prompt carries: the bytes are drawn below it either way, the keys that
answer the question are offered either way, and nothing is promoted until you say so. An attacker who
owns the content can force the reassuring word, and what that buys them is a quieter sentence above
content you are still reading for yourself.

## What leaves the process

There is **one way out**, and it is not optional: every outbound request carrying labelled content
goes through a single call, and the HTTP client is private to that module so no other crate can open a
second path.

- **Redirects are revalidated on every hop.** They are followed by hand and each new URL is put to the
  gate before it is fetched, so a permitted host cannot hand off to a denied one. The chain is bounded.
- **A redirect may not leave `https` for cleartext.** A hop from an `https` URL to a plain `http` one
  is refused, whatever asked for the request. Every hop re-sends the whole request, headers and body
  alike, so a chain that lost TLS would put in the clear what the hop before it carried under TLS. A
  chain that had no TLS to begin with continues, and one that picks TLS up part way through cannot put
  it down again: what is refused is leaving `https`, not a hop that changes scheme.
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
content or model output, so no labelled value escapes the gate. It follows its own redirects through
its library's default rather than through the loop above, so the refusals in that list, the cleartext
one included, do not reach it.

## Certificates and proxies

Both are read from the environment on purpose and set on every client explicitly, rather than left to
a library's default, so what is in force is a decision here and not a property of a dependency's
version. `bravebot doctor` reports both, which is where to look first when a connection fails.

**Certificate authorities.** A build ships a set of them, and `SSL_CERT_FILE` (a bundle) or
`SSL_CERT_DIR` (a directory of them) names others **in their place**. What they name replaces the
shipped set rather than adding to it, because that is what every other client on the machine does with
these variables: somebody pinning a private authority has ruled the public ones out on purpose. The
two are read independently, so setting one that turns out to be empty does not discard what the other
held. Where neither holds anything, nothing is trusted rather than the shipped set quietly coming
back, and `doctor` names the path that yielded nothing.

This is what to set on a network that inspects TLS, where the certificate presented comes from an
authority whoever set the machine up installed. The **platform trust store is not read**: on a machine
where neither variable is set, an authority installed into the system store is invisible here, and a
line in a shell profile is the remedy.

**A proxy.** `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY` are read in that order and in either case,
with `NO_PROXY` naming the hosts that bypass it. `doctor` names the proxy by protocol, host and port,
the hosts it is not used for, and whether it requires a credential; the credential itself is never
printed, because a diagnostic is something people paste into issues. A proxy whose protocol this build
cannot connect through is not used at all, and `doctor` says that one was named and is not the route.

A proxy carries request bodies, which here means conversation content. It can read them only where it
also terminates TLS, which takes an authority this process trusts and therefore one of the two
variables above: a second statement by the same person, on the same machine, that every other client
on it is held to as well. A proxy nobody stated sees nothing, and one somebody stated sees what they
already let it see.

## Confinement

`bravebot doctor` reports the operating-system confinement available on your platform and whether
the kernel enforces the network denial, printed rather than assumed, because the guarantee differs
by platform and kernel.

The opening screen and `/status` name that same platform level, and `/status` says beside it that
nothing in the session is confined. Confinement bounds a process started to run code Brave Bot did
not write, so a session that starts none of those is inside no such boundary, and the level is what
your machine offers rather than something holding the session back.

Where confinement is used, it **fails closed**: if it cannot be established the process does not run,
rather than running unconfined. A profile starts denying everything and grants accumulate onto it, and
a policy that would confine nothing is rejected rather than applied. The network is denied unless it
was asked for. A backend reports what the kernel actually enforces rather than what it was asked for,
and a policy demanding something it cannot deliver is refused.

**A policy is granted as written, or it is refused.** A backend that cannot install a grant for one of
the paths refuses the whole policy and names that path, rather than confining the process to the rest
of them. Running under fewer paths than the policy names while the record says the policy was applied
is the same silent degradation, reached one grant at a time.

**A write grant covers moving a file inside it.** Writing a temporary file and renaming it into place
is how a compiler, a package manager and an editor write anything, so a grant to write a path covers
moving a file from anywhere under it to anywhere else under it. On Linux this needs a kernel right that
arrived in Landlock's second version, so on kernels between 5.13 and 5.19, which a long-term
distribution release still ships, confinement is unavailable and a process is refused rather than run
under a policy that cannot be applied in full. A confinement that denied the move would leave a tool
that renames files for a living copying and unlinking instead, so the work appears to succeed and has
quietly stopped being atomic.

**A path that is not on disk is left out, and named.** Profiles are assembled from lists naming more
paths than any one machine has, so refusing every policy that mentions an absent path would mean a
machine with no `~/.pyenv` is a machine where nothing runs. Where the platform can grant a path that
does not exist, the policy is what was wanted. Where it cannot, an absent path is left out before the
policy is built and reported back, so the difference between the grant that was decided on and the
grant a program got stays visible. Naming the parent directory in its place is not done, since that
would grant every other file in it. A path is created in its place only where the list naming it says
whether it is a file or a directory, and only for a program meant to create it, which is how a fresh
account with no `~/.ssh/known_hosts` would get one rather than a push that fails; what is created is
reachable by your account and nobody else. The lists a confined process runs under today say neither,
so nothing is created for one.

**The environment is the caller's.** A confined process starts with the environment this process
holds. A grant over paths can neither withhold nor hand over what sits in a variable, and a credential
or an agent socket does, so which of its own variables a program is trusted with stays the caller's
decision rather than something that differs per platform.

Confinement does not cover the rest of the system. A processor is a model call made by our own code,
and a program you asked for runs with the access your own shell would give it. Everywhere else, the
boundary is the capability set and the label on a value. Windows has published binaries and no
confinement backend yet, and failing closed means refusing rather than running unconfined, so anything
that has to be confined is refused there until that platform has one.

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
model you chose. Session records hold what the planner was allowed to hold, which means whatever file
content it was shown. **Nothing untrusted is ever written down**, by construction rather than by
filtering, and quarantined content is not written at all. A pasted picture is written, because it was
part of your own message. Deleting a session record removes that session; the prompt history is a
separate file holding every prompt you have submitted across all runs, and deleting a session does not
touch it.

Leo Premium credentials live in a mode-0600 file under `~/.bravebot`, readable only by you. They are
not encrypted at rest, which is what the browser they are imported from does with the same secret.
See [Leo Premium](../customize/premium.md#where-they-are-kept).

The credentials Brave Bot reaches a model with are overwritten in memory when the value holding
them goes, rather than handed back to the allocator with the bytes still in them, and the process
turns off its own core dump before it reads the first one, so a crash leaves no file with a key in
it. The single-use credentials of an imported Leo Premium subscription are held the same way, and
so is the text of the file they are read out of. Two things are not covered: swap, because keeping
pages out of it needs the allocator those buffers come from, and the one-shot presentation a
premium request carries, which is derived from a credential rather than being one and lives only as
long as the request.

## Reporting a problem

Brave Bot is experimental and developed in the open. Please report security issues through the
[repository](https://github.com/brave/bravebot/issues), and see the
[mini-specs](https://github.com/brave/bravebot/tree/main/docs/specs) for the clause-level
statement of everything on this page.
