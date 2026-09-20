---
sidebar_position: 2
title: Approvals and permissions
description: Every prompt the system puts to you, and exactly what your answer grants.
---

# Approvals and permissions

Every consequential effect stops and puts something to you. This page is what each of those prompts
grants, and what it does not.

## A prompt shows what is at stake

A prompt shows the thing itself, not a summary of it:

| Prompt | What it shows |
|---|---|
| a write | the path and the body |
| an overwrite | what it replaces |
| an edit | the diff |
| a run | the compiled plan: every step, the binary each resolved to, the directory, and every file the line would write |
| reading a command's output | the bytes, and the command that printed them |
| a fetch | the URL, and on its own line the host it will reach |
| trusting a file | the path and its first lines |

You cannot endorse a destination you were not shown.

A prompt also says what approving **does**, and what it does not. The run prompt says the command is
not sandboxed, asks for the side effects and the output together, and names the exact command it would
vouch for. The trust prompt explains the consequence and names both answers.

## Content in a prompt is still untrusted

An untrusted body is marked as such, and command output is drawn inside a margin it cannot forge. The
margin is on every drawn row, not every line of content, so a line wider than the box is broken to the
width by the same step that draws the margin and each row carries a bar of its own.

A review stays legible or says it could not: a long body keeps the question on screen and offers the
rest to scroll to, a small edit in a large file shows only the change, an empty output says so, and a
diff that cannot be computed says so rather than showing nothing.

## One answer is never taken for another

**Each endorsement is single-use and bound to the exact value it was given for.** An approved write
does not approve a run. A write approval is not an answer to a question, and an answer to a question
is not consent to a write. These are separate grants that happen to use the same keyboard.

## Declining is not cancelling

Saying no to a write does not stop the turn. The agent carries on and can try something else. That is
how you steer without starting over.

**Ctrl-C refuses and stops.** Declining, and Ctrl-C, vouch for nothing.

## Vouching for a command

The run prompt is the one place a standing permission is offered:

```
  y run it    a always    n don't    ctrl-c stop the turn
```

`a` grants two things together, and the prompt asks for both:

1. **the command runs again unasked**, side effects and all;
2. **what it prints becomes trusted**, so the planner reads it instead of a reference.

The second is a human assertion, not an inference. Nothing establishes that a vouched command is
side-effect-free or that its output is free of influence, and nothing tries. `git log` prints commit
messages whoever contributed wrote. It is trusted for exactly the reason a directory in the trust map
is trusted: you said so.

An entry is keyed by **resolved path and exact arguments**. `git log` says nothing about `git push`,
and nothing about `git log --all`. `$PATH` and aliases decide what a name means, so an assertion never
follows a name onto a different binary. In a line of several steps, *every* step must be vouched for
or the whole output is untrusted.

Two things can answer the run prompt, and nothing else. One is your having answered it before, in this
session, for that exact command. The other is a proof about a program's options, which covers a short
list of audited reading commands and is
[`run`](../reference/tools.md#a-line-that-only-reads-what-you-vouched-for-does-not-ask): where every
step of a line is one of those, writes nothing, and reads only paths you vouched for, the line runs
unasked and its output comes back as text.

There is no *declared* read-only category. `foo --bar` might write to disk and nothing here can tell,
and a step declaring itself harmless only helps if the declaration is honest. An unprompted write is
worse than an unwanted prompt, so nothing that could be wrong about a write may answer the question.
An audited entry is a claim checked by hand against one program's full option list, which is why it
may, and it is narrow for the same reason: anything it does not fully recognise asks.

**A line that writes asks every time**, whatever you have vouched for. Vouching is keyed on a program
and its arguments, and a redirection's destination is neither, so `grep -rn thing src/` running
unasked must not let `grep -rn thing src/ > notes.txt` run unasked too. Your answer is bound to the
whole compiled plan rather than to the text of the line, so it cannot be reused for the same steps
joined differently, writing somewhere else, or run in another directory. See
[`run`](../reference/tools.md#run).

**Private input asks every time**, whatever is vouched for, and `a` is not offered for those runs at
all. Untrusted input is fine, since carrying bytes decides nothing. Private input hands your data
to a program, and that releases it somewhere this policy stops governing. Vouching for what a file
contains is not consenting to send it somewhere.

A line reaches that gate two ways: the planner names a quarantined reference and bravebot supplies the
bytes, or the line redirects a file in with `<`. A file's bytes are your own data whatever the trust
map says about the path, so `run cat < ~/.ssh/id_rsa` asks even where you have vouched for `cat` and
even where a rule allows the line. See [`run`](../reference/tools.md#a-redirection-is-a-write).

:::note
The vouched-for list is **not an allowlist** and must never become one. It never decides what may run:
a command nobody vouched for still runs after a prompt, nothing is refused for being absent, and the
set is empty at the start of every session. What holds is the label on the output, not a belief about
the binary.
:::

Programs are not confined. They run with the access your own shell would give them, because `git push`
needs `~/.ssh` and the set of programs someone might ask for cannot be listed in advance. The one
exception is bravebot's **own** credentials, which are withheld from every program it runs: you
approve an argv, never an environment, so a credential travelling alongside one would be handed over
without your having seen it. See [`run`](../reference/tools.md#what-a-program-is-handed).

## A fetch is approved one URL at a time

No standing permission is offered at the fetch prompt. Your answer is bound to the URL you were
shown, so the next fetch asks again even on the same host. An `a` here would be keyed to a host, and
a host is not what was in front of you: you approved one page, and every later URL on that host is
one you have not seen.

To let a host through unasked, write the rule down in advance:
`WebFetch(domain:docs.example.com)` in `allow` covers that host and its subdomains. A `deny` rule for
a host refuses without asking, and refuses a redirect trying to reach it.

**Approving a fetch never trusts what comes back.** The body stays quarantined whatever you answer,
because consent to talk to a host is not a claim about what it returns. See
[`fetch_url`](../reference/tools.md#fetch_url).

## Answering in advance: modes

**Shift-Tab** walks the session through four modes, and the one in force is drawn under the input
box. A mode is a standing answer to the questions above, given once instead of one at a time.

| Mode | What it answers |
|---|---|
| **asking** | nothing: every write, run, command output and unvouched file is put to you |
| **accepting edits** | the write prompt, and no other |
| **plan mode** | it refuses a write rather than asking about one |
| **bypassing** | every permission question, including the two that decide trust |

**Asking is where a session opens**, and it is what holds when nobody has chosen. The key comes round
to the first again, so no mode is one you cannot press your way out of, and it works while a turn
runs: the turn in flight keeps the mode it began with, so a diff already on your screen does not have
the question withdrawn from under you.

**Accepting edits stops at writes on purpose.** A write lands in a tree you can read afterwards and
`git diff` shows you all of it; a program runs with everything your own shell has, leaves no diff,
and what it prints is what the next round reads. A mode named for edits that also stopped asking
about programs would be granting the larger thing quietly. It does accept a write to any path the
workspace reaches, since the prompt was the only thing that would have shown you the path. A rule in
the settings file is what narrows that.

**Plan mode refuses a write rather than asking**, whatever you would have answered, and the planner
is told so and why, so a run of refusals reads as a constraint to work inside rather than as a series
of mistakes. Commands are still asked about, because research is most of what planning is. It
constrains the write tools rather than making the turn incapable of changing anything: a command you
approve may write whatever it likes.

### Bypassing

```sh
bravebot --dangerously-skip-permissions
```

The flag is the only way to reach that mode: without it the key walks the other three however many
times you press it. It may go anywhere in the command line and composes with `-p`, `--resume`,
`--continue`, `--mode` and `--incognito` alike. A one-shot run has no key to press, so the flag
is the whole of what can say.

It answers the two questions that decide trust as well as the others, and those are the ones that
cost the most. Vouching is what decides whether a file's contents are shown to the planner or held
behind a reference, so in this mode every file the planner asks to read is shown to it, and a file
holding instructions rather than data is read as instructions. The [startup trust
question](trust.md) is not put either: the session starts with the rule a yes would have written,
since the tree becomes trusted a file at a time in any case.

What stays is the structural guarantee, that untrusted content cannot *decide* what happens. What
goes is the narrower protection of not showing the planner bytes nobody vouched for. **This is a mode
for a container with no network and nothing in it worth losing**, which is what its spelling is
for.

A run approved this way vouches for no program. The list of commands you said to stop asking about is
written into the session record and outlives the mode, so a record claiming you approved programs you
were never shown would be a standing permission nobody granted.

### Two things no mode answers

**A `deny` rule holds in every mode**, including the one that asks about nothing. It refuses before
there is a prompt, so there is nothing for a mode to answer. A flag that quietly undid a rule you
wrote down would take protection away at the moment you were relying on a mode to save keystrokes.

**A question the planner posed reaches you in every mode**, and so does a line you type unprompted.
Neither asks for consent: the first asks for information, and the second is you speaking. An answer
invented on your behalf is reported to the planner as your own words.

### A mode belongs to the sitting it was chosen in

A resumed session opens by asking, whatever the session that wrote the record was doing when it
ended. The mode is not written into the session record. `--resume` with the flag opens in bypass,
because the flag was given again.

A [delegate](../reference/tools.md#spawn_agent) inherits the mode of the turn that spawned it, since
a delegate is that turn's work done somewhere else.

## Rules you write down in advance

The `permissions` block of `~/.bravebot/settings.json` holds three lists (`deny`, `ask` and `allow`)
saying which actions to refuse outright and which to stop and ask you about. They are the same three
lists Claude Code keeps, with the same spellings, so a block copied out of `~/.claude/settings.json`
governs bravebot unedited. See
[Configuration](../customize/configuration.md#permissions) for how a rule is written.

**A rule decides whether you are asked, and whether an action happens at all. Nothing else.**

**A deny rule refuses before the file is opened or the program is looked for.** A denied file is not
read, not enumerated, not searched and not written, and a `Read` deny rule also stops a write to the
path it covers. A file whose contents are off limits is not protected if it can be overwritten.
Naming the path through a reference reaches the same refusal, including on the one route allowed to
read what nobody vouched for: a processor is handed no denied file either. The planner is told the
rule refused and that retrying is not the answer.

A deny rule also holds against **a workspace you trusted**, which is what makes one worth writing:
saying yes at startup trusts the whole tree, and a rule is how one file is kept out of that answer
without declining the rest of it.

**An allow rule stops the asking and grants nothing else.** It does not make a command's output
trusted: output carries what it would have carried. Pressing `a` grants those two together because
you are looking at one command and can answer for both; a pattern covers commands nobody has read, so
it cannot carry the second claim. If a rule could trust output, one line in a settings file would turn
fetched bytes into routing, which is the whole thing labels exist to prevent. Nor does an allow rule
extend reach: it cannot open a path the workspace and the directories you opened do not already cover.

**Two prompts no rule can answer.** A run that would put your private data into a program asks
whatever the rules say, because a rule saying which commands may run is not consent to hand one your
data. A write whose destination is known only through a reference asks too: that prompt is the only
moment such a path is shown to anybody, so nothing a pattern says can stand in for having looked.

:::note
**A `deny` list is not a sandbox.** A rule is matched against a program's argv rather than against
what the program then does, so `Bash(git *)` covers `git -c core.fsmonitor=<script> diff`, which runs
a program the rule never named. And a path rule does not reach a program's own file access at all:
`run cat .env` is checked against the `Bash` rules and against the run prompt, not against a `Read`
rule covering `.env`. What confines a process is [confinement](security.md#confinement); what holds
regardless is the label on the output.
:::

## What survives, and what does not

Two grants are standing, and both are written into the session record and restored by `--resume`,
because the person resuming is the person who gave them:

- the [trust map](trust.md);
- the list of commands you said to stop asking about.

**Nothing else survives.** A single-use endorsement is created by one approval, is bound to one value,
and is never written down, so a resumed turn cannot replay a write or a run an earlier turn was
allowed. Answers to the planner's own questions are remembered only in the live session.

A fresh session in the same directory restores neither and asks again.

## Reading permissions back

```
/status
```

lists the trust rules in force and the commands that now run unasked. Every other prompt in a session
announces itself by appearing; this is the one that stops appearing, so `/status` is the only thing
that can tell you a command now runs unasked and that its output is being read as trusted.

## When the planner asks you something

The `ask_user` tool puts up to four questions to you, one at a time, with options to choose from. You
can always answer in your own words, or skip.

An answer is trusted as a first label, and only for a trustworthy question. **Asking stops once the
planner's context has met something untrusted**, because at that point the question itself could have
been shaped by content nobody vouched for. A quarantined read does not stop the planner asking, since
a reference carries no instruction.

Skipping is an answer to work with rather than a reason to ask again, and an answer is remembered for
the session, question by question.

## Where nobody can be asked

A one-shot run refuses effects rather than applying them unseen, and declines every question rather
than inventing an answer. A `deny` rule and an `ask` rule you wrote in advance hold there as they do
in a session. An `allow` rule does not, since what it answers is a prompt and there is nobody to
prompt. `--dangerously-skip-permissions` is the one thing that lifts the first half, and the one thing
that lets an allow rule decide again, because a flag somebody typed is an instruction rather than a
guess; it does not lift the second, since the planner's questions are not permissions. See
[Non-interactive use](../using/headless.md#nothing-is-approved).
