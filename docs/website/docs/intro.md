---
sidebar_position: 1
slug: /
title: Overview
description: What Brave Bot is, what makes it different, and where to start.
---

# Brave Bot

Brave Bot is a general-purpose coding agent for your terminal. It reads a repository, edits files,
runs programs and answers questions about the code in front of it, and it is a drop-in replacement
for the agents you already use.

```sh
npm install -g @brave/bravebot
cd your-project
bravebot
```

Its defining property is **structural resistance to indirect prompt injection**. Content that
nobody vouched for never reaches the part of the system that decides what to do next. That is a
property of the plumbing rather than of the prompt, so it holds whatever the content says. Most
agents instead ask a model to be careful with the web pages, dependency READMEs and command output
it reads.

## What it can do

- **Answer questions about a codebase.** Read files, list directories and search for literal text,
  with paging so a large file does not swallow the conversation. See
  [Tools](reference/tools.md).
- **Write and edit code.** Every write and every edit is approved by you first, as a body or as a
  diff. See [Approvals and permissions](security/permissions.md).
- **Run programs.** `run` takes a command line and compiles it into the programs, arguments and
  destinations it names, so nothing is ever handed to a shell on the model's behalf. See
  [the `run` tool](reference/tools.md#run).
- **Run your own shell commands.** Type `!` on an empty prompt and the line goes to `$SHELL`, with
  globs and redirection intact, and its output reaches the model in full so you can follow it with
  "fix the first failure". See [Shell mode](using/shell-mode.md).
- **Work on content it is not allowed to read.** Quarantined files are handed to an isolated
  processor that rewrites them, and the result is written back without either the planner or the
  driver ever seeing the bytes. See [How Brave Bot works](how-it-works.md#processors).
- **Take standing instructions.** `AGENTS.md` and skills apply to every task in a directory. See
  [Instructions](customize/instructions.md) and [Skills](customize/skills.md).
- **Show its work.** Every gate decision is recorded: what was checked, what label a value carried,
  what was released. Read it live with Ctrl-T or after the fact with `--trace`. See
  [The audit trail](security/audit-trail.md).

## What makes it different

| | |
|---|---|
| **Labels, not vibes** | Every value carries a label on two axes: trusted or untrusted, public or private. Labels only ever degrade, and no code path can hand a value a better one than its inputs had. |
| **Quarantine, not warnings** | Untrusted content is never placed in a message to the model. The planner gets a reference (origin, line count, byte count, label) and acts on content it cannot read. |
| **Approval bound to what you saw** | An approval is single-use and bound to the exact value it was given for. Approving a write is not approving a run, and no approval survives into a later session unless it was explicitly a standing one. |
| **No shell for the planner** | The model never gets a shell tool. Not behind a capability, not behind a prompt. It writes a command line, and bravebot compiles it rather than interpreting it. |
| **Specified clause by clause** | Behaviour is written down as numbered clauses, each naming the tests that pin it, and the code is reviewed against them. |

## Start here

- **[Quickstart](quickstart.md)**: install it, run a first task, and understand what it asks you.
- **[How Brave Bot works](how-it-works.md)**: the planner, the driver, labels and processors.
- **[Trusted directories](security/trust.md)**: the question at startup, and what your answer buys.
- **[CLI reference](reference/cli.md)**: every flag and subcommand.

## Status

Brave Bot is experimental. It is developed in the open at
[brave-experiments/bravebot](https://github.com/brave-experiments/bravebot), where the
[mini-specs](https://github.com/brave-experiments/bravebot/tree/main/docs/specs) are the source of
truth for how it behaves. Where this site and a spec disagree, believe the spec and please
[file an issue](https://github.com/brave-experiments/bravebot/issues).
