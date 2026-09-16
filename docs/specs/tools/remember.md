---
id: REMEMBER
title: remember
status: normative
---

## Scope

The planner's only write path into persistent memory during a turn. `kind` is routing; `text` and
`mtype` are content. `kind` selects core (`core.jsonl`) or episodic (`episodic.jsonl` plus
vectors). Episodic writes require a configured embedding block.

## Clauses

<a id="REMEMBER-1"></a>
### REMEMBER-1: `kind` is routing

`kind` chooses core or episodic and which jsonl file under `~/.bravebot/agent_memory` the write
reaches: text that will appear in the **Core** block in the system prompt on every later turn, or
text held for episodic recall in the user turn only. `text` and `mtype` are content. A successful
call writes immediately (no approval prompt); the tool result states what was saved.

<a id="REMEMBER-2"></a>
### REMEMBER-2: a successful remember appends to the correct jsonl store

When memory is on and incognito is off, a valid call with `kind: episodic` persists in
`episodic.jsonl` when embeddings are configured; `kind: core` persists in `core.jsonl` without embeddings.

<a id="REMEMBER-3"></a>
### REMEMBER-3: incognito skips the write

In incognito, the tool may answer but does not change memory files on disk.
