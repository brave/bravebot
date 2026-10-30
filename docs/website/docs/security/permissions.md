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

You cannot endorse a destination you were not shown. A key that approves is refused until every row
that decides the question has been drawn whole at the width of the box, and the line under the body
says how many are left. That covers writes, output, vetting, vouching, exposure, server, tool list,
call and plan prompts. A refusal and ctrl-c work at every draw.

A prompt also says what approving **does**, and what it does not. The run prompt says what the command is
confined to (the directories and what each program brings beyond them), or that it is not sandboxed
where nothing confines it. On Linux and Windows it also warns when a program other than git can write
a repository's hooks and configuration, which git runs later with no sandbox. It asks for the side effects and the output together, and names the exact
command it would vouch for. The trust prompt explains the consequence and names both answers. The prompt about a
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

The questions for `read_output` and `vet_content` show every line of what a yes would release, in
the terminal, in `--plain` and in ACP. The cap of 40 lines still applies to every other question,
such as a write.

Some characters draw as nothing: zero-width characters, bidirectional controls, variation selectors
and the tag characters, which write ASCII letters that a model reads. Those two questions write each
one out as `‹U+XXXX›`, and a tag character also with the letter it stands for. A sentence above the
text says how many there are. The planner receives the text itself.

A check's **verdict word** sits outside the margin, because it is the system's own. The **sentence**
the check wrote about the content sits inside it, because that is free text about bytes somebody else
may own, and it is the one line on such a screen you might otherwise read as the program's.

The steps of a manifest plan are the one body here drawn without a bar. They are the system's own
rendering of a plan fixed before anything was read, so a bar down them would mark as untrusted the one
thing on the screen that is not.

A review stays legible or says it could not: a long body keeps the question on screen and offers the
rest to scroll to, a small edit in a large file shows only the change, an empty output says so, and a
diff that cannot be computed says so rather than showing nothing. In the full-screen interface, and
on the decision cards of a desktop conversation, no approval is taken until the lines it rests on have
been on screen:
until then the question says how many are left to read, a refusal is taken at any time, and a change
of width starts the count again.

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

**A note says why, or what to do instead.** At the write prompt (a whole-file write, an edit and a
download), the run prompt and the prompt for a call to an
[MCP server's](../customize/mcp-servers.md) tool, Tab opens a one-line field. Enter sends the answer
with what you typed, and Esc closes the field and answers nothing, so the question is still there. The
field sends no until you press Tab to switch it to yes, so an empty field or a stray Enter approves
nothing. Where the question's own `y` is not available, such as a plan whose rows have not all been
on the screen, the field offers no yes either.

The agent is told your words after the result of an approved call, or in place of the question it
would otherwise end on when you declined. A decline with no note ends on asking what you would prefer,
which costs another round and another interruption. The note is one line, cut at a fixed length, and
it belongs to the one answer it was typed at. It is recorded in the audit trail as yours. The keys that
stand for a lasting answer (`a`, `r`, `s`, `m`, `e` and a number) open no field. A Tab or an Enter that
arrived together with other keys, as a paste does, opens or sends nothing. The trust question, the
desktop window, a session in lines and a one-shot run offer no field.

## Changing a write before approving it

At the write prompt in the terminal, **e** opens the proposed contents in your editor, the one
`$VISUAL` or `$EDITOR` names, as Ctrl-G does for the prompt box. When you save and quit, the write is
put to you again as a diff of what you saved, and nothing is written until you answer that question.
Quitting without saving, or saving what was there, leaves the question where it was, and an editor
that would not start is said above it.

Only a write whose contents the agent wrote itself offers it. A file made from quarantined content
does not, since what you kept of it would still be bytes nobody read. What you save is scanned for
credentials as the agent's own text is, and a credential in it refuses the write. The agent is told
that you changed the contents and not what they say, and it reads the file to find out. The desktop
application and `--plain` do not offer it.

## Vouching for a command

The run prompt is the one place a standing permission is offered, and it offers two of them with
different lifetimes:

```
  y run it    a always this session    r remember it    s read pull requests and issues in brave/bravebot    n don't    ctrl-c stop the turn
```

`s` is offered only for the lines described under [a number that changes](#when-a-lines-arguments-change-every-time). The keys are labelled that way so both lifetimes can be read off the screen. `a` is a **vouch** and
lasts the session. `r` [remembers the line past the session](#remembering-a-line-past-the-session) and
grants strictly less. Enter reaches neither, and declining or Ctrl-C records nothing.

`a` grants two things together, and the prompt asks for both:

1. **the command runs again unasked**, side effects and all;
2. **what it prints becomes trusted**, so the planner reads it instead of a reference.

The second is a human assertion, not an inference. Nothing establishes that a vouched command is
side-effect-free or that its output is free of influence, and nothing tries. `git log` prints commit
messages whoever contributed wrote. It is trusted for exactly the reason a directory in the trust map
is trusted: you said so.

An entry is keyed by **resolved path, the path that starts it, exact arguments, and the directory it
was given in**. `git log` says nothing about `git push`, and nothing about `git log --all`. `$PATH` and
aliases decide what a name means, so an assertion never follows a name onto a different binary. A
program named through a link, such as a virtualenv's `python`, is started by the link so it can find
its virtualenv. An entry for it does not cover the interpreter the link points to when that
interpreter is run directly. In a line of several steps, *every* step must be vouched for or the
whole output is untrusted.

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
   [`run`](../reference/run-tool.md#a-line-that-only-reads-what-you-vouched-for-does-not-ask): where every
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
its arguments and an assignment is in neither.

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
even where a rule allows the line. See [`run`](../reference/run-tool.md#a-redirection-is-a-write).

:::note
The vouched-for list is **not an allowlist** and must never become one. It never decides what may run:
a command nobody vouched for still runs after a prompt, nothing is refused for being absent, and the
set is empty at the start of every session. What holds is the label on the output, not a belief about
the binary.
:::

On Linux and macOS a program can read the machine except the places that hold a credential, and
writes only the directories the session was opened on, its scratch directory, the system temporary
directory and the toolchain caches, plus what its own command brings: the credentials a `git push`
needs, never a private key. The run prompt lists them. Under the `strict` sandbox mode it reads only
those directories, the system files every program needs and what its command brings, and a path
outside them is refused by the system, so a `cat ~/notes.txt` fails until you add the directory. On
Windows a program runs in a container that reaches only those directories, what its command brings,
the system and program directories and git's configuration files, in every mode but `off`. The set
of programs someone might ask for cannot be listed
in advance, so the profile follows the command and not a list. A profile bounds files and not the
environment. The one exception to what a program is handed is bravebot's **own** credentials, which
are withheld from every program it runs: you approve an argv, never an environment, so a credential
travelling alongside one would be handed over without your having seen it. See [`run`](../reference/run-tool.md#what-a-program-is-handed).

## Remembering a line past the session

`r` at a run prompt records that exact command line under `~/.bravebot`, keyed by the directory you are
working in. Every session begun in that directory honours it from then on, resumed or fresh, and the
session you pressed it in honours it immediately. The prompt shows what would be recorded and where,
because that is the whole of the grant.

**It stops the asking and nothing else.** A covered line runs unasked, side effects and all, but what
it prints stays untrusted and private, exactly as it would without the record. `a` is the key that
decides a label; `r` is the key that decides a lifetime. If you want a command's output readable, press
`a`.

**It covers one line, not a family**, apart from the `s` answer described [below](#when-a-lines-arguments-change-every-time). A later line is covered when the program's name, the binary that
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
| an [incognito session](../using/incognito.md) | it adds nothing to `~/.bravebot` |
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

`r` covers one argument list, and no key at a prompt widens a grant to a family, with one exception
described at the end of this section: bounding one means
knowing which argument carries a value and which names something to run, and nothing at a prompt can
tell those apart. `npm run <script>` and `ssh <host> <command>` put what runs into an argument, and
`git --no-pager <sub-command>` puts a sub-command into one.

So where you have already answered a prompt for the same binary under different arguments, the prompt
says so, and says that a pattern for the family is written in a settings file rather than answered at a
prompt. It names the file. It does not offer a pattern, because which argument carried the value is
your judgment to make. What a pattern costs is stated with it: it covers lines nobody has read, it
stops the asking, and it makes nothing readable. See
[Configuration](../customize/configuration.md#permissions).

**The exception: reading pull requests and issues.** For `gh pr view`, `gh pr diff`, `gh pr checks` and
`gh issue view`, written as `gh pr view 1081 --repo brave/bravebot`, the prompt also offers `s`. It asks
in words, "Read pull requests and issues in brave/bravebot", and saves one entry holding all four
sub-commands on that repository with the number free: none of them is then asked about again for any
whole number, in that directory. These four were checked by hand against what `gh` does, and they are
listed in the source; nothing is worked out from your answers. Another repository, a number that is not
digits, any other flag (`--web`, `--json`) and any sub-command that writes are still asked about, as is
a line that is not in the list. `s` stops the asking and nothing else, as `r` does, and deleting the
entry's line from the file takes all four back.

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

**Shift-Tab** walks the session through five modes (asking, accepting edits, plan mode, auto and
bypassing), and the one in force is drawn under the input box. A mode is a standing answer to the
questions above, given once instead of one at a time. [Permission modes](permission-modes.md) has what
each one answers, what accepting edits stops at, what `--dangerously-skip-permissions` skips, and the
two things no mode answers.

## Rules you write down in advance

The `permissions` block of `~/.bravebot/settings.json` holds three lists (`deny`, `ask` and `allow`)
saying which actions to refuse outright and which to stop and ask you about. They are the same three
lists Claude Code keeps, with the same spellings. A rule decides whether you are asked, and whether
an action happens at all. Nothing else. [Permission rules](permission-rules.md) has what a deny rule
refuses, what a rule covers beyond the spelling used to ask, why an allow rule has to be yours, and
the prompts no rule can answer.

## What survives, and what does not

Three grants are standing. Two are written into the session record and restored by `--resume`, because
the person resuming is the person who gave them:

- the [trust map](trust.md);
- the list of commands you said to stop asking about, which is empty at the start of every session.

The third is not restored by a resume, because it is not in the session record at all: the lines and
credential files you pressed `r` for are kept per directory and read by **every** session begun there,
resumed or fresh. It reaches a fresh session because the key that wrote it said how long its answer
lasts, and because what it carries is the asking rather than any trust.

A fresh session in the same directory restores neither of the first two and asks again.

[Auto-vetting](vetting.md) is not one of the three and is not in the record either. It is not a grant
about any particular thing: it says which of two questions a session asks, so a resumed session reads
it from a flag or a settings file exactly as a fresh one does.

A turn that fails or that you cancel keeps the commands you said to stop asking about, so the next
turn does not ask again. It does not turn a single-use approval or a refusal into a standing one.

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

A [manifest run](../using/manifest-mode.md) is the one prompt about a whole run rather than about one thing at
the moment it is due. It can be, because that mode fixes every step while your task is still the only
input, so nothing the run goes on to read can change the plan.

The prompt shows your task in your own words and then every step, in order, each naming what it would do
and every field that decides where it lands. Every step, never a count and never the first few: one
answer covers all of them, and the step below the fold is as binding as the first. A plan longer than the
box is scrolled to rather than cut short. Every prompt that shows a body longer than the box (write,
output, run, vetting, call, plan and the questions a session opens with) scrolls with Up, Down, `k`,
`j`, PageUp, PageDown, Home and End. `j` and `k` are never answers. In the run prompt, the answer that
remembers a requested credential scope for every session is `e`, not `k`.

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

On a run that also skips permissions, the same flag is what lets that check refuse. Bypassing releases
both of those slots unshown on its own, so the check's word is the only thing left that can keep content
back, and with the flag a verdict that objects or a check that did not complete does keep it back. Without
the flag no check is made on that run at all, since nothing would read the word. The vouch offer is not
one of the two: it writes a standing rule about a path rather than releasing one slot, and bypassing
answers it either way.
