---
sidebar_position: 8
title: Memory
description: Rules the planner always follows and facts it can recall, kept under ~/.bravebot and shown by /memory.
---

# Memory

Bravebot keeps two kinds of thing for you across sessions, and treats them differently on purpose.

- **Rules** are instructions. Every rule is loaded in full, every turn, and is never dropped to save
  tokens or ranked against anything.
- **Facts** are things worth knowing. They are looked up when a question calls for one, and the few that
  match your prompt closely are put in front of the planner as background, not as instructions.

Both live under `~/.bravebot`, so both are trusted for sitting there, and neither is ever written from
content nobody vouched for.

## Rules

A rule comes from one of three layers, read in this order:

| Layer | File | Applies to |
|---|---|---|
| home | `~/.bravebot/AGENTS.md` | every project |
| per project | `~/.bravebot/projects/<id>/rules.md` | the project you are in |
| project | the project's own `AGENTS.md` | that project |

Where your file and the project's disagree, the project's wins, and the system prompt says so. The
project is named by its origin once you have vouched for the workspace, and by its path before. See
[Instructions](instructions.md) for how the files are found and sized.

The planner can propose a rule with [`propose_rule`](../reference/tools.md#propose_rule). You are
asked, shown the text, the file and the reason, and only your yes writes it. You can type your own
wording instead. Each rule is one line, `- <text> (<date>) why: <reason>`, and `/undo` takes it back.
You can also edit either file by hand.

A rule is never promoted from a fact. If a fact deserves to be a rule, say so, or approve the planner's
proposal.

Rules from different layers can disagree. At the start of a turn after a layer changes, bravebot asks
the model to judge the pairs of lines that look alike, at most 20 pairs, and tells you which side
wins. A layer it has already checked is not checked again, and with nothing to ask, the check is
skipped and the session says so once.

## Facts

A fact is one markdown file, `~/.bravebot/projects/<id>/facts/<key>.md` for one project or
`~/.bravebot/facts/<key>.md` for all of them, with a few lines of front matter at the top: who asked for
it (`source`), whether it is `public` or `private`, when it was saved and last verified, and the key
of any fact that replaced it. A fact holds one thing. You can write or edit them by hand; a file with
the front matter missing is read as an agent's, private fact.

The planner saves one with [`remember`](../reference/tools.md#remember), looks one up with
[`recall`](../reference/tools.md#recall) and marks one as checked with
[`recall_verify`](../reference/tools.md#recall_verify). A fact the planner learned from a private
context is private, and reading a private fact makes the rest of the turn private, so it cannot be
sent to a public surface. Ask for something to be remembered and the fact is kept as yours, and the
planner will not overwrite it.

Before a turn that plans, your prompt is ranked against your facts, and up to three that match strongly
on at least two distinct words are put in the system prompt, each with its key and verified date, once
per session. A short prompt or a slash command injects nothing. Delegates are given no facts.

## Looking at what is kept

[`/memory`](../reference/commands.md#memory) lists the rule files and each fact, with its key, scope,
source, label, verified date and whether another replaced it. It never shows a fact's text.
[`/memory-tidy`](../reference/commands.md#memory-tidy) proposes what to tidy and changes nothing:
facts that say nearly the same thing, facts that name code and have gone 30 days unchecked, broken
`[[links]]`, oversized facts and rules that disagree.

## Incognito

An [incognito](../using/sessions.md#a-session-that-leaves-nothing-behind) session saves no fact, proposes no rule, runs no disagreement check
and makes no project directory. It still reads your rules and facts, so it behaves as you configured.
