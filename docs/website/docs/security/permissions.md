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
| [a directory your settings file asked for](#a-directory-your-settings-file-asked-for) | the path it would open, after the name is resolved |
| [one quarantined slot](vetting.md) | the bytes, where they came from, and what the check said about them |
| [the plan a manifest run will walk](#approving-a-whole-run-in-advance) | your task in your own words, then every step in order |

You cannot endorse a destination you were not shown.

A prompt also says what approving **does**, and what it does not. The run prompt says the command is
not sandboxed, asks for the side effects and the output together, and names the exact command it would
vouch for. The trust prompt explains the consequence and names both answers. The prompt about a
directory your settings file asked for says that opening it grants reach and trust, and that a file
asked for it. The prompt over one quarantined slot says what approving does *not* do: no path is
vouched for, so the same thing read again asks again.

Where a check has read the content a prompt is about, the prompt says what the check found, in one of
three forms: it found no attempt to give instructions, it thinks the content looks like one, or it did
not complete so nothing has looked. The three prompts that can carry a verdict are drawn by one piece
of machinery, so a prompt cannot carry a verdict and forget to say what it was. See
[vetting](vetting.md).

## Content in a prompt is still untrusted

An untrusted body is marked as such, and command output is drawn inside a margin it cannot forge. The
margin is on every drawn row, not every line of content, so a line wider than the box is broken to the
width by the same step that draws the margin and each row carries a bar of its own.

A check's **verdict word** sits outside the margin, because it is the system's own. The **sentence**
the check wrote about the content sits inside it, because that is free text about bytes somebody else
may own, and it is the one line on such a screen you might otherwise read as the program's.

The steps of a manifest plan are the one body here drawn without a bar. They are the system's own
rendering of a plan fixed before anything was read, so a bar down them would mark as untrusted the one
thing on the screen that is not.

A review stays legible or says it could not: a long body keeps the question on screen and offers the
rest to scroll to, a small edit in a large file shows only the change, an empty output says so, and a
diff that cannot be computed says so rather than showing nothing.

## One answer is never taken for another

**Each endorsement is single-use and bound to the exact value it was given for.** An approved write
does not approve a run, and does not approve the plan a write is in. A write approval is not an answer
to a question, and an answer to a question is not consent to a write. Approving a read of what a
program printed does not promote a quarantined slot, which covers more. These are separate grants that
happen to use the same keyboard.

Bound to the exact value means the tree a plan runs in, the binary each step resolved to, and each
destination a step's output is sent to, every one of them by that path's own bytes rather than by how
it was printed on your screen. Two files whose names print the same way are two different approvals.

## Declining is not cancelling

Saying no to a write does not stop the turn. The agent carries on and can try something else. That is
how you steer without starting over.

**Ctrl-C refuses and stops.** Declining, and Ctrl-C, vouch for nothing.

## Vouching for a command

The run prompt is the one place a standing permission is offered, and it offers two of them with
different lifetimes:

```
  y run it    a always this session    r remember it    n don't    ctrl-c stop the turn
```

The keys are labelled that way so both lifetimes can be read off the screen. `a` is a **vouch** and
lasts the session. `r` [remembers the line past the session](#remembering-a-line-past-the-session) and
grants strictly less. Enter reaches neither, and declining or Ctrl-C records nothing.

`a` grants two things together, and the prompt asks for both:

1. **the command runs again unasked**, side effects and all;
2. **what it prints becomes trusted**, so the planner reads it instead of a reference.

The second is a human assertion, not an inference. Nothing establishes that a vouched command is
side-effect-free or that its output is free of influence, and nothing tries. `git log` prints commit
messages whoever contributed wrote. It is trusted for exactly the reason a directory in the trust map
is trusted: you said so.

An entry is keyed by **resolved path, exact arguments, and the directory it was given in**. `git log`
says nothing about `git push`, and nothing about `git log --all`. `$PATH` and aliases decide what a name
means, so an assertion never follows a name onto a different binary. In a line of several steps, *every*
step must be vouched for or the whole output is untrusted.

**The directory is part of the key, and it is one directory rather than the tree beneath it.**
`sh check.sh` names a different file in every tree it is read in, so an answer given in `sub/` is not an
answer about a `check.sh` at the root, and it grants nothing in `sub/nested/` either. `git log` pointed
at a vendored dependency asks again however often you vouched for `git log` at the root, and what it
prints there is quarantined, because no entry names that directory. `sub`, `./sub` and a symlink
pointing at `sub` are one directory and not three.

Four things can answer the run prompt, and nothing else:

1. **you answered it before**, in this session, for that exact command;
2. **you pressed `r`** for that exact line, in this directory, in this or an earlier session;
3. **a rule you wrote in advance** covers the line, which stops the asking and raises no label;
4. **a proof about a program's options**, which covers a short list of audited reading commands and is
   [`run`](../reference/tools.md#a-line-that-only-reads-what-you-vouched-for-does-not-ask): where every
   step of a line is one of those, writes nothing, and reads only paths you vouched for, the line runs
   unasked and its output comes back as text.

Never a property of the command this system worked out for itself, never a step declaring itself
harmless, and never anything derived from what a program printed. There is no *declared* read-only
category: `foo --bar` might write to disk and nothing here can tell, and a step declaring itself
harmless only helps if the declaration is honest. An unprompted write is worse than an unwanted prompt,
so nothing that could be wrong about a write may answer the question. An audited entry is a claim
checked by hand against one program's full option list, which is why it may, and it is narrow for the
same reason: anything it does not fully recognise asks.

**A line that writes asks every time**, whatever you have vouched for, and neither `a` nor `r` is
offered for it. Vouching is keyed on a program and its arguments, and a redirection's destination is
neither, so `grep -rn thing src/` running unasked must not let `grep -rn thing src/ > notes.txt` run
unasked too. An entry made at that prompt would hold no destination, so the only line it could ever
cover is this one with the redirection gone, which is a line you never read. Your answer is bound to the
whole compiled plan rather than to the text of the line, so it cannot be reused for the same steps
joined differently, writing somewhere else, or run in another directory. See
[`run`](../reference/tools.md#run).

**A variable set in front of a program asks every time**, and no key is offered for it.
`LD_PRELOAD=./evil.so git log` is asked about however often you vouched for `git log`, and what it
prints is quarantined. An assignment decides what a program loads and reads before its own arguments
are looked at, so the line is a different proposition from the one you read, and nothing in a vouched
entry records it. No rule can answer this one either, because a rule is matched against the program and
its arguments run together and an assignment is in neither.

:::tip
`NO_COLOR=1 cargo test` is ordinary work, and it asks every time. The spelling that can be answered
once is `env NO_COLOR=1 cargo test`, which is a program called `env` with three arguments: the
assignment is in the argv you approve, so a vouch covers that line and no other. What is refused is a
line whose meaning is not in its argv, not the setting of a variable.
:::

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

## Remembering a line past the session

`r` at a run prompt records that exact command line under `~/.bravebot`, keyed by the directory you are
working in. Every session begun in that directory honours it from then on, resumed or fresh, and the
session you pressed it in honours it immediately. The prompt shows what would be recorded and where,
because that is the whole of the grant.

**It stops the asking and nothing else.** A covered line runs unasked, side effects and all, but what
it prints stays untrusted and private, exactly as it would without the record. `a` is the key that
decides a label; `r` is the key that decides a lifetime. If you want a command's output readable, press
`a`.

**It covers one line, not a family.** A later line is covered when the program's name, the binary that
name resolved to, every argument, every variable the line set, and where its output went are all the
same. Sending the errors somewhere else makes a different line. A pipeline is covered only where every
stage is.

**Where `r` is not offered**, because those lines are asked about whatever is recorded and the key would
stop no prompt:

| Not offered | Why |
|---|---|
| a run fed your private data | the same reason `a` is not offered there |
| a line naming a file to write | the record holds no destination |
| a line setting a variable in front of a program | asked about whatever is recorded |
| a line running anywhere but the workspace root | including one that names a directory |
| a line a `deny` or `ask` rule matches | a keypress must not overturn a rule you wrote |
| an [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind) | it adds nothing to `~/.bravebot` |
| bypassing | no run prompt is drawn for a key to reach |

A rule you write **afterwards** takes a remembered line back: an `ask` rule puts it to you again and a
`deny` rule refuses it.

**A one-shot run reads no record at all.** What a record answers is a prompt, so where no prompt can be
put it would be deciding which effects may happen with nobody to see them. See
[Non-interactive use](../using/headless.md#nothing-is-approved).

:::warning
The boundary is the prompt, not the person. A tick of a `/loop`, a round of a goal and a delegate's own
step each draw prompts to a live session, so each reads the record and each runs a covered line without
stopping. Pressing `r` and then leaving a loop running overnight grants more than pressing it and
staying.

A covered line is also only as good as the tree it runs in. `make check` runs whatever the makefile in
that tree has come to say, and `npm test` whatever its `package.json` says, so pressing `r` is a
decision about the tree as much as about the command. A vouch has the same property and the session
bounds it; this record is bounded only by your deleting the line.
:::

Because a covered line is never put to you again, it can no longer be vouched for and its output stays
quarantined. [`read_output`](../reference/tools.md#read_output) still puts one result to you, and the
way back is to delete the line.

## When a line's arguments change every time

`r` covers one argument list, and no key at a prompt widens a grant to a family: bounding one means
knowing which argument carries a value and which names something to run, and nothing at a prompt can
tell those apart. `npm run <script>` and `ssh <host> <command>` put what runs into an argument, and
`git --no-pager <sub-command>` puts a sub-command into one.

So where you have already answered a prompt for the same binary under different arguments, the prompt
says so, and says that a pattern for the family is written in a settings file rather than answered at a
prompt. It names the file. It does not offer a pattern, because which argument carried the value is
your judgment to make. What a pattern costs is stated with it: it covers lines nobody has read, it
stops the asking, and it makes nothing readable. See
[Configuration](../customize/configuration.md#permissions).

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
of mistakes. It refuses where the prompt would have been approved and equally where there would have
been no prompt: a path the trust map already covers, or one a rule allows, is refused too. Commands are
still asked about, because research is most of what planning is. It constrains the write tools rather
than making the turn incapable of changing anything: a command you approve may write whatever it likes.

A [manifest run](../using/headless.md) is refused from its frozen plan, before the plan is put to you,
where any step in it writes a file. A plan that writes nothing runs. The plan is where the mode has to
be answered, because a body the plan carried going to a path you vouched for raises no write prompt at
all, so a refusal that waited for one would let the whole run through.

:::note
**`defaultMode` in the settings file selects no mode.** The key is parsed so a file carrying it is not
rejected, and nothing is chosen from it: if you wrote `acceptEdits` there you get the prompts you would
have got without it. The command line and Shift-Tab are what choose a mode.
:::

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
since the tree becomes trusted a file at a time in any case. A directory your settings file asked for
is opened and vouched for without being put to you.

**No check runs for a prompt that is not drawn.** A [vetting check](vetting.md) normally sits in front
of every prompt whose answer would promote quarantined content, and this is the one mode where those
prompts are answered without being shown to anybody. A check there would be a model call whose word
nobody reads, so it is not made. This is the only exemption.

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

**Three prompts no rule can answer.** A run that would put your private data into a program asks
whatever the rules say, because a rule saying which commands may run is not consent to hand one your
data. A write whose destination is known only through a reference asks too: that prompt is the only
moment such a path is shown to anybody, so nothing a pattern says can stand in for having looked. And a
run carrying a variable set in front of one of its programs asks, because a rule is matched against the
program and its arguments run together: `Bash(git log)` matches `LD_PRELOAD=./evil.so git log`, and no
rule you could write tells the two apart.

**A rule that cannot be read is dropped, named, and takes nothing with it.** A line that is not a rule,
names no tool family, or has no anchor to resolve is skipped and the rest of the file still applies.
Every one dropped is reported, both by `bravebot doctor` and in the session that read the file, because
a misspelled `deny` rule reads as protection that is not there. Refusing the whole file instead would
mean a typo in an `allow` rule quietly removed a `deny` rule's protection.

Rules are read **once per session**, so a file you edit while a session is open describes the next one.

:::note
**A `deny` list is not a sandbox.** A rule is matched against a program's argv rather than against
what the program then does, so `Bash(git *)` covers `git -c core.fsmonitor=<script> diff`, which runs
a program the rule never named. And a path rule does not reach a program's own file access at all:
`run cat .env` is checked against the `Bash` rules and against the run prompt, not against a `Read`
rule covering `.env`. What confines a process is [confinement](security.md#confinement); what holds
regardless is the label on the output.
:::

## What survives, and what does not

Three grants are standing. Two are written into the session record and restored by `--resume`, because
the person resuming is the person who gave them:

- the [trust map](trust.md);
- the list of commands you said to stop asking about, which is empty at the start of every session.

The third is not restored by a resume, because it is not in the session record at all: the lines you
pressed `r` for are kept per directory and read by **every** session begun there, resumed or fresh. It
reaches a fresh session because the key that wrote it said how long its answer lasts, and because what
it carries is the asking rather than any trust.

A fresh session in the same directory restores neither of the first two and asks again.

[Auto-vetting](vetting.md) is not one of the three and is not in the record either. It is not a grant
about any particular thing: it says which of two questions a session asks, so a resumed session reads
it from a flag or a settings file exactly as a fresh one does.

**Nothing else survives.** A single-use endorsement is created by one approval, is bound to one value,
and is never written down, so a resumed turn cannot replay a write or a run an earlier turn was
allowed. Answers to the planner's own questions are remembered only in the live session.

## Reading permissions back

```
/status
```

lists the trust rules in force and the commands that now run unasked. Every other prompt in a session
announces itself by appearing; this is the one that stops appearing, so `/status` is the only thing
that can tell you a command now runs unasked and that its output is being read as trusted.

It lists **every** vouched command rather than a count of them, and names the directory each one covers,
since two entries for one command in two directories are two separate grants. Remembered lines are
listed separately, and each says whether this session's own answer covered it or an earlier session's
did, because a flat list would not tell you which answers you are still carrying from last week. The
listing also says where the record is kept, since deleting a line from it is the way back.

## When the planner asks you something

The `ask_user` tool puts up to four questions to you, one at a time, with options to choose from. You
can always answer in your own words, or skip.

An answer is trusted as a first label, and only for a trustworthy question. **Asking stops once the
planner's context has met something untrusted**, because at that point the question itself could have
been shaped by content nobody vouched for. A quarantined read does not stop the planner asking, since
a reference carries no instruction.

Skipping is an answer to work with rather than a reason to ask again, and an answer is remembered for
the session, question by question.

## A directory your settings file asked for

A directory named in `additionalDirectories` is not opened by the file naming it. It is put to you as a
question of its own when the session opens, and one you accept is opened by the route `/add-dir` takes
and trusted for the session on the same terms. One you decline is neither reachable nor vouched for.

The name is resolved **before** the question is put, and the question shows what it resolved to, because
opening a directory follows a name wherever it leads: a link inside your checkout can resolve somewhere
else entirely, and a box showing the spelling would collect an answer about a different tree. A name that
cannot be opened whatever you answer is reported rather than asked about, and a directory that two layers
both named is one question.

A session resumed with its own trust map is asked nothing and opens none of them, since the directories
it has open are the ones its own record reopened. `/clear` closes the ones that were open and opens none.

:::note
How many questions a session opens with is the file's to choose. Every name is one box, so a file naming
thirty directories is thirty of them before you can type a prompt, and the way out of a list you do not
want to answer is Ctrl-C, which starts no session. No box grants anything by itself.
:::

## Approving a whole run in advance

A [manifest run](../using/headless.md) is the one prompt about a whole run rather than about one thing at
the moment it is due. It can be, because that mode fixes every step while your task is still the only
input, so nothing the run goes on to read can change the plan.

The prompt shows your task in your own words and then every step, in order, each naming what it would do
and every field that decides where it lands. Every step, never a count and never the first few: one
answer covers all of them, and the step below the fold is as binding as the first. A plan longer than the
box is scrolled to rather than cut short.

It says that nothing the run reads can add a step, drop one, or send anything anywhere the plan does not
already name; that approving the plan is **not** approving its writes, which are each still put to you as
they come up; and that nothing has happened yet, so declining leaves everything as it is.

No standing form is offered and Enter does not approve a plan. A plan is written afresh for each run, so
remembering an answer to one would be approving steps nobody has seen.

## Where nobody can be asked

A one-shot run refuses effects rather than applying them unseen, and declines every question rather
than inventing an answer. A plan is refused too, so a manifest run with nobody to ask stops before its
first step rather than walking a program nobody read. No record of remembered lines is read, since what
such a record answers is a prompt.

A `deny` rule and an `ask` rule you wrote in advance hold there as they do
in a session. An `allow` rule does not, since what it answers is a prompt and there is nobody to
prompt. `--dangerously-skip-permissions` is the one thing that lifts the first half, and the one thing
that lets an allow rule decide again, because a flag somebody typed is an instruction rather than a
guess; it does not lift the second, since the planner's questions are not permissions. See
[Non-interactive use](../using/headless.md#nothing-is-approved).

`--vet` is the one other answer you can give in advance here, and it answers one question only: with it,
a check that completes and finds nothing promotes the one slot the planner asked to be shown. Nothing
else about an unattended run changes. See [vetting](vetting.md).
