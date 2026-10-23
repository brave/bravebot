---
sidebar_position: 2.1
title: Permission modes
description: What asking, accepting edits, plan mode and bypassing each answer for you, what they stop at, and the two things no mode answers.
---

# Permission modes

**Shift-Tab** walks the session through four modes, and the one in force is drawn under the input
box. A mode is a standing answer to the [questions a session puts to you](permissions.md), given once
instead of one at a time.

| Mode | What it answers |
|---|---|
| **asking** | nothing: every write, run, command output and unvouched file is put to you |
| **accepting edits** | the write prompt, except for a write that would create a credential |
| **plan mode** | it refuses a write rather than asking about one |
| **bypassing** | every permission question, including the two that decide trust |

**Asking is where a session opens**, and it is what holds when nobody has chosen. The key comes round
to the first again, so no mode is one you cannot press your way out of, and it works while a turn
runs: the turn in flight follows it from its next question or tool call, and a diff already on your
screen does not have its question withdrawn from under you.

**A session started with `--dangerously-skip-permissions` says so when it stops bypassing.** The line
under the input box and `/status` name asking there, where a session started without the flag draws
nothing for it, including one that reached bypassing with the key. Nothing is printed in the
transcript when you press the key.

**Accepting edits stops at writes on purpose.** A write lands in a tree you can read afterwards and
`git diff` shows you all of it; a program runs with everything your own shell has, leaves no diff,
and what it prints is what the next round reads. A mode named for edits that also stopped asking
about programs would be granting the larger thing quietly. It does accept a write to any path the
workspace reaches, since the prompt was the only thing that would have shown you the path. A rule in
the settings file is what narrows that. Under the sandbox mode `read-only`
([confinement](security.md#confinement)) the agent's file tools write nothing, in this mode or in
bypassing.

**A write that would create a credential is still put to you.** A file that is one rare value and
nothing else is how a generated key reaches a tree, so a write the credential scan finds such a value
in is shown to you with what it found, under accepting edits as under asking. A rule or a vouched path
does not answer it. A value that announces itself as a credential is refused in every mode, before
any prompt. The prompt offers two answers that last longer than yes:

```
  y write it    a always this session    r remember it    n leave it alone    ctrl-c stop the turn
```

`a` stops the asking about that file for the rest of the session. `r` stops it for every session
begun in this directory, and is written where [remembered lines](permissions.md#remembering-a-line-past-the-session)
are, on the same terms. Both are about the file the write lands in, not the name the planner used, so
the same name after `/cd`, in another checkout or behind a link is asked about again. `a` ends at
`/cd`, `/clear` and `--resume`, and a delegate and a manifest run start without it. `r` is not offered
in an incognito session or for a file outside the directory. Both settle the secret only: a write a
rule asks about is still asked about. With nobody to ask, the write is refused unless the run is
bypassing, even where a rule allows it. The desktop application and `--plain` offer yes and no only.

**Plan mode refuses a write rather than asking**, whatever you would have answered, and the planner
is told so and why, so a run of refusals reads as a constraint to work inside rather than as a series
of mistakes. It refuses where the prompt would have been approved and equally where there would have
been no prompt: a path the trust map already covers, or one a rule allows, is refused too. Commands are
still asked about, because research is most of what planning is. It constrains the write tools rather
than making the turn incapable of changing anything: a command you approve may write whatever it likes.

A [manifest run](../using/manifest-mode.md) is refused from its frozen plan, before the plan is put to you,
where any step in it writes a file. A plan that writes nothing runs. The plan is where the mode has to
be answered, because a body the plan carried going to a path you vouched for raises no write prompt at
all, so a refusal that waited for one would let the whole run through.

:::note
**`defaultMode` in the settings file selects no mode.** The key is parsed so a file carrying it is not
rejected, and nothing is chosen from it: if you wrote `acceptEdits` there you get the prompts you would
have got without it. The command line and Shift-Tab are what choose a mode.
:::

## Bypassing

```sh
bravebot --dangerously-skip-permissions
```

The flag opens the session in that mode. Without it a session opens asking, and Shift-Tab reaches the
mode on the third press, after plan mode. Nothing is asked when it gets there: the line under the
input box names it in colour, and one more press is back to asking. A settings file that sets
`permissions.bypassUnreachable` takes the mode off the key and refuses the flag.

The flag may go anywhere in the command line and composes with `-p`, `--resume`, `--continue`,
`--mode` and `--incognito` alike. A one-shot run has no key to press, so the flag is the whole of
what can say.

It answers the two questions that decide trust as well as the others, and those are the ones that
cost the most. Vouching is what decides whether a file's contents are shown to the planner or held
behind a reference, so in this mode every file the planner asks to read is shown to it, and a file
holding instructions rather than data is read as instructions. The [startup trust
question](trust.md) is not put either: the session starts with the rule a yes would have written,
since the tree becomes trusted a file at a time in any case. A directory your settings file asked for
is opened and vouched for without being put to you. So are the three questions about a server a
checkout asks for: whether to start it, whether to offer its tools, and whether to make a call to
one of them. Each is answered yes and none is written to the files that remember an answer, so a
later run without the flag asks. A remote server whose reply points somewhere else is still not
followed: a yes would rewrite your declaration to an address the server chose, so the hop is refused
without asking. See [MCP servers](../customize/mcp-servers.md#approving-one).

Starting a language server is still put to you in this mode, and is refused where nobody can be asked.

**No check runs where nothing would read its word.** A [vetting check](vetting.md) normally sits in front
of every prompt whose answer would promote quarantined content, and this is the one mode where those
prompts are answered without being shown to anybody. A check there would be a model call whose word
nobody reads, so it is not made. This is the only exemption, and `--vet` is what takes it back for the
two prompts that release one slot: with that flag the check's word is what answers in your place, and a
word that objects refuses. The vouch offer reads no word in this mode either way.

**Command output can come back in the result that ran it.** What a run prints is still quarantined in
this mode, but with no check asked for, `read_output` releases it without showing it to anybody. So a
run that sets [`read: true`](../reference/run-tool.md#reading-the-output-in-the-same-result) gets the
output at once, unless it is longer than one result may hold, and the audit trail records the release
as the mode's, as it would for `read_output`.
With `--vet` the check still reads it first, through `read_output`, and `read` changes nothing. The
result for such a run says the argument was not applied.

What stays is the structural guarantee, that untrusted content cannot *decide* what happens. What
goes is the narrower protection of not showing the planner bytes nobody vouched for. **This is a mode
for a container with no network and nothing in it worth losing**, which is what its spelling is
for.

**A write that would create a credential is written.** Accepting edits puts it to you; here the flag
is your answer, as it is to every other question, and nothing is recorded for a later session. A
value that announces itself as a credential is still refused.

A run approved this way vouches for no program. The list of commands you said to stop asking about is
written into the session record and outlives the mode, so a record claiming you approved programs you
were never shown would be a standing permission nobody granted.

## Two things no mode answers

**A `deny` rule holds in every mode**, including the one that asks about nothing. It refuses before
there is a prompt, so there is nothing for a mode to answer. A flag that quietly undid a rule you
wrote down would take protection away at the moment you were relying on a mode to save keystrokes.

**A question the planner posed reaches you in every mode**, and so does a line you type unprompted.
Neither asks for consent: the first asks for information, and the second is you speaking. An answer
invented on your behalf is reported to the planner as your own words.

**The same call three times running is held and asked about**, in every mode, bypassing included. When
the planner asks for a call whose tool and arguments equal those of the two calls before it, with no
different call between them, it does not run. You are asked whether to refuse it and tell the planner
to try another approach (the default, so a bare Enter is safe), run it this once, or stop the turn.
A yes covers that one call and starts the count again. Only the tool name and the arguments are
compared, never a result, so polling the same `read_output` or running the same test again with no
edit between is held at the third call, and a different call between two identical ones starts the
count again. A call that differs by one argument each time is not caught. Where nobody can be asked (a
one-shot run, a planned run or a delegate) the call is dropped and the planner is told. The trail
records the round, the count and your answer, and nothing of the call. There is no setting for the
number.

## In the desktop application

The control beside the model in the composer offers asking, accepting edits and plan mode, and
`⇧⌘M` (`Ctrl+Shift+M` on Windows and Linux) walks them in that order. Bypassing is not offered: the
desktop has no command line for the flag to be given on. The mode belongs to the open session and
every session opens asking, a resumed one and a fork included. A change made while a turn
runs applies to the rest of that turn.

## A mode belongs to the sitting it was chosen in

A resumed session opens by asking, whatever the session that wrote the record was doing when it
ended. The mode is not written into the session record. `--resume` with the flag opens in bypass,
because the flag was given again.

A [delegate](../reference/tools.md#spawn_agent) inherits the mode of the turn that spawned it, since
a delegate is that turn's work done somewhere else.
