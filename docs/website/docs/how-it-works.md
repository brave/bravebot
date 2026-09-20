---
sidebar_position: 3
title: How Brave Bot works
description: The planner, the driver, labels, quarantine and processors.
---

# How Brave Bot works

> **Untrusted content never enters the driver's context or the planner's.**

Everything else on this page follows from that rule.

The **planner** is the model deciding what to do next. The **driver** is the Rust code around it.
Both are held to the same rule, because moving a decision from one into the other does not remove
it.

## The two roles

| | What it is | What it may do with untrusted content |
|---|---|---|
| **Planner** | the model | never sees it: it is handed a reference instead |
| **Driver** | the program | may **carry** it and hand it to an effect, never **read** it |
| **Policy layer** | the gates inside the driver | the only code allowed to read untrusted bytes, and only at a gate |
| **Processor** | an isolated model call with no tools | reads it, rewrites it, and can direct nothing |

## Labels

Every value carries a label on two axes:

```
L = I × C      I ∈ {T, U}       trusted / untrusted
               C ∈ {pub, priv}  public  / private
```

Untrusted input degrades integrity. Private input raises confidentiality. `(U,priv)` and `(T,pub)`
are incomparable, so this is a lattice rather than a pair of booleans.

A derived value is labelled by taint over its inputs: one untrusted input taints the result, one
private input makes it private, the axes degrade independently and the order of the inputs does not
matter. **Labels only ever degrade.** Nothing constructs a label better than its inputs had, in any
crate. That is laundering. If a value derived from untrusted input has to be trusted for something to
work, the design is wrong rather than the label.

A *first* label is not an upgrade. Model output is a function of the model's context, so when the
context held only trusted input, what it produced is labelled accordingly. The same applies to a
program's output, a file read from the workspace, a line you ran yourself in shell mode, an answer you
typed to a question, and your own configuration. Each of those is a label a value receives for the
first time, assigned from provenance the policy layer tracked, never a relabelling of something that
already had one.

**The ways in are enumerated, and none of them reads what it labels.** Provenance decides, so opening
a new road means adding a row to that list where a reviewer sees it. Two of the entries are that there
is nothing to label: a picture you paste and a prompt you type while a turn is running both join your
own message, which carries no label either, so they are recorded in the [audit
trail](security/audit-trail.md) instead.

## Quarantine and references

Untrusted content is never placed in a message to the model. It goes into a write-once slot and the
planner is given a **reference** instead:

```
ref:2  notes.md, 84 lines, 2.1 KiB, (U,priv)
```

The planner acts on content it cannot read by naming that reference, and the policy layer resolves
it when the write or the call actually happens. So the model can:

- read a file it may not see, by passing `path_ref` around;
- feed a quarantined file to a program's stdin, so `sed` and `awk` work on it;
- write quarantined content into a file with `contents_ref`;
- hand it to a processor to be changed.

What it cannot do is see the bytes, or take a decision from them. A reference also cannot choose
**where** something lands: resolving one to the name it stands for authorises nothing by itself, and a
reference naming no file, which is everything a processor produced, is refused as a destination
outright. For a write the name goes to you, and the grant is issued for the path you saw, which is why
such a write always asks.

## Processors

Content that has to be *changed* rather than moved goes to a processor: an isolated model call with
no tools, no memory and no conversation.

| | |
|---|---|
| Tools | none, and the request carries no tool list at all |
| Memory | none: the messages are built from nothing each time |
| Conversation | one request, one reply, no loop to steer |
| Reads | exactly the references it was given, and nothing else |
| Writes | at most one new reference, and nothing else |

A processor's answer is quarantined like anything else, and the planner never sees it either. The
output's label is computed **before** the processor runs, by taint over the inputs, so nothing it
writes has any say in how what it writes is labelled.

A [check](security/vetting.md) is a call of the same shape pointed at one quarantined slot, asking
whether it looks like an attempt to give instructions. It is narrower still, because it can write
nothing at all: no reference, no file, no destination of any kind. Its answer is a word on your screen
and never a decision.

This is how a file the agent may not read still gets fixed: the planner names the file's reference,
says what has to be true of the file afterwards, and passes the reference that comes back to
`write_file` as `contents_ref`. You approve the write from the resulting diff.

A processor's answer is for **one** document, and may be written only to the file the planner said
the call was about. Everything before the document marker in its reply is a remark for the person
watching: it reaches your screen and stops there. No model reads it, it is part of no file, and it
cannot be another processor's input.

**The remark is put in the approval box, above the diff it describes.** It decides nothing: no gate
reads it, you approve from the diff of the real bytes, and the write goes the same way with the remark
as without it. It is there so the claim and the evidence are read in one place. Nothing checks a remark
against its document and nothing could, so a processor can say it fixed one line while the document
does something else. "I only fixed the typo" beside three hundred changed lines is visibly untrue;
remembered from several rounds earlier it is not. It is drawn as untrusted content, inside a margin it
cannot forge, and capped, since a remark long enough to push the diff out of the box would cost you the
evidence to gain the claim. A write the planner composed itself has no remark and shows none.

## Delegates

The planner can hand a sub-task to a **delegate**: a second planner with a context of its own and a
narrower set of capabilities. It is for a sub-task that would otherwise fill the conversation with
reading.

| | |
|---|---|
| Tools | its kind's, and never a way to delegate again, ask you something, or fetch a URL |
| Memory | none of its parent's exchange: it begins with the task it was given |
| Conversation | a loop of its own, bounded |
| Reads | whatever its capabilities and the paths you vouched for allow |
| Writes | files, each shown to you first, and slots in a quarantine of its own |

A planner that runs the build reads the whole log. A planner that asks a delegate to run the build is
told what failed. The work happens either way, and only the first spends the conversation on it.

**Delegates run alongside the turn and alongside each other.** Starting one hands the planner its
round straight back and the work goes on behind it, so a turn that asked three questions waits on
the slowest rather than on the sum. Nothing is shared between two of them: each holds its own
conversation, its own quarantine and its own copy of what you have vouched for, so no delegate can
see another's work any more than it can see the turn's. One question is put at a time, so a delegate
wanting a write approved while you are reading another delegate's diff waits for you to finish.

A turn does not answer while something it started is still working. If it would otherwise finish
first, the reports are waited for and put in front of it, and it answers again knowing what came
back.

**A run whose own context has already met something untrusted cannot delegate at all**, because the
task it would compose is a function of those bytes. A delegate is not trusted more than its parent.
It holds capabilities and holds no untrusted content (what it may not read is quarantined and it is
handed a reference, exactly as its parent would be), so there is no point in the run where untrusted
bytes and a capability are in the same context.

The report that comes back is labelled by the integrity of the delegate's own context and passes the
same gate as any other result, so nothing is trusted on a delegate's say-so. Its gate decisions go
into the same audit trail as the turn that spawned it, named so the two can be told apart.

See [`spawn_agent`](reference/tools.md#spawn_agent).

## Routing and content

Every effect splits in two:

- **Routing**: the part that decides where the effect lands: a path, a program name, its
  arguments, a URL. Routing must be `(T,pub)`, and must be endorsed by a person.
- **Content**: the part that is merely carried: a file body, a program's stdin, a request body.
  Content may be untrusted. It must not be private at the moment it is released.

The built-in tools are native rather than MCP calls, because an opaque call erases this split.

The planner's command line is compiled rather than interpreted. `run` reads the line into the
programs, arguments and destinations it names, a plan you endorse, and nothing is ever passed to a
shell, so `; rm -rf /` inside quotes is one argument and stays one.

## Gates

A gate is a check that has to pass before anything consequential happens: content reaching the
model, a file being written, a program being run, a request leaving the process. Each one decides a
single question and **refuses rather than warns**, so there is no path to a consequence that does
not go through one.

Every gate decision is recorded, allowed or refused, and the record holds no content: only gate
names, capabilities, labels, paths and slot ids. That is why it can be shown on your screen and
written to a file for a workspace nobody vouched for. See [The audit trail](security/audit-trail.md).

```
ok      precommit: routing fields ["task"] fixed before any observation
ok      promote: read_file.path proposed by the model, confined and non-destructive
ok      file_read.path [routing] (T,pub)
observe file_read produced (T,priv)
ok      trust: notes.md read as trusted, from a trusted path
ok      render: read_file: content reshaped for presentation, still (T,priv)
ok      present: tool_result: notes.md is (T,priv), so the planner may read it
```

## Where trust comes from

Nothing is trusted until a person grants it, and trust is never inferred from silence, from a
path's shape, or from anything a model or a file said. There are exactly a few gestures that grant
it, and each grants one thing:

| Gesture | What it grants |
|---|---|
| answering yes at startup | the working directory, for this session |
| `@path` in a prompt, or `--file` | that one file, for the rest of the session |
| dragging a file onto the terminal | that one file, wherever on disk it is |
| `/add-dir <path>` | that directory: reachable **and** trusted, for this session |
| `/cd <path>` | that directory, as the new working directory, for this session |
| accepting a directory your settings file named | that directory, on the terms `/add-dir` uses |
| answering yes at a quarantined read | that one path, for the rest of the session |
| `a` at a run prompt | that exact command: runs unasked, and its output is trusted |
| putting a file in `~/.bravebot` | trusted by provenance, as your own configuration |

Every one of them is a person's gesture. None of them is something the system worked out from
content. See [Trusted directories](security/trust.md).

## The limits of all this

These three are deliberate rather than oversights:

- **Trusting a directory trusts what lands in it.** A rule is about a path, not about the files that
  were there when the rule was made. `npm install`, `git pull`, your editor, or a program the agent
  was allowed to run can all put a file into a vouched-for tree, and it will be read as trusted.
- **A fresh session forgets what an earlier one poisoned.** The rule that untrusted data marks its
  destination untrusted holds within a session and across a resume of it, not across a fresh start.
- **A vouched-for command's output is trusted because you said so.** Nothing establishes that
  `git log` is free of influence. Its output is whatever contributors wrote. It is trusted for the
  same reason a directory is: a person took responsibility.

Every one of these is written down in the specs under "Known costs", because an unlisted exception
is indistinguishable from a violation.
