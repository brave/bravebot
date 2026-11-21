---
sidebar_position: 6.5
title: The TypeScript SDK
description: Run bravebot from a Node program, read its events, and answer its questions by position.
---

# The TypeScript SDK

`packages/sdk` is a TypeScript library that starts the `bravebot` command line and reads what it
writes. It is a thin wrapper over [`--json-stream`](../reference/cli.md#--json-stream) and
[`--input-format lines`](../reference/cli.md#--input-format-lines), so it can do what a person
typing those flags can do and nothing more. It is in the repository and is not published to npm.

```ts
import { run, startSession } from '@brave/bravebot-sdk'

const { events, result } = await run('summarise what src/ does', { cwd: '/path/to/project' })
console.log(result.status, result.reply)
```

`run` resolves with the events a run wrote and the result object that ended it. A run that ended
badly still resolves: `result.status`, `result.reason` and `result.refusals[].principle` say how,
and are fixed names a program can branch on. It rejects only when there is no result, such as when
the command cannot be started or the run is aborted. `stream` gives each line as it is written.

## A conversation

```ts
const session = startSession({ cwd: '/path/to/project' })
const first = await session.prompt('review the diff')
const second = await session.prompt('now fix what you found')
await session.close()
```

A session is one process with `--input-format lines`. Each `prompt` is a turn over the conversation
the last one left, `steer` reaches the turn that is running, and `stop` ends it. `nextTurn` gives
turns in the order they end.

## Questions

```ts
const session = startSession({
  onQuestion: (event) => event.questions.map(() => [0]), // pick the first choice of each
})
```

A question the agent asks arrives as a `question` event, and the handler returns positions among its
choices: one array for each question, and an empty array to decline. There is no handler that
approves a write, a command or a fetch, and the command line refuses one that would ask.

Two costs of answering by position hold for a session. A later turn cannot work on a reference an
earlier turn made, since each turn is its own task. The agent is told that the user chose a position,
including when the program picked it.

## Whose words the lines are

The text of a prompt is a program's words. Unless the session is started with `trustInputLines`, the
agent is given a reference to the text and never the words, as it is for piped input. Pass
`trustInputLines: true` only when the lines are the words of the person who started the program.

Without it, a prompt is a turn with no prompt of its own, and the agent is given a reference to the
words. Until `brave/bravebot#2950` is fixed, the message the agent is sent for that turn can be empty,
which a provider may refuse. A program that needs the agent to read its words passes
`trustInputLines`.

## What it does not offer

Options are a closed list, one for each flag the library passes. The library refuses any other
name, and a value that begins with `-`, before it starts anything. It has no option for
`--dangerously-skip-permissions`, `--accept-edits`, `--trust-workspace`, `--add-dir` or
`--settings`, and nothing that writes the trust map or vouches for a path.

The `reply`, `message` and a call's `target` are text that can come from outside the run. Treat them
as data to show or store, and do not run them or put them in a command.

No callback receives the content of a file, a fetched page or a tool result. `onEvent`, `onQuestion`,
`onError` and `onStderr` receive what the command line wrote, and the words of a prompt sent without
`trustInputLines` are never passed back to them.

The library is TypeScript only, because the repository already builds and tests Node packages with
pnpm. A Python package would add a toolchain, a lockfile policy and a CI job, and is left to a
follow-up. The name `@brave/bravebot-sdk` and the directory `packages/sdk` follow the repository's
other package, `packages/agent-client`.

To build and test the package, run `make check-sdk` from the repository root. The behaviour is
specified in [the SDK spec](https://github.com/brave/bravebot/blob/main/docs/specs/sdk.md).
