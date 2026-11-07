---
id: SDK
title: TypeScript SDK
status: normative
governs:
  - packages/sdk/src/args.ts
  - packages/sdk/src/errors.ts
  - packages/sdk/src/index.ts
  - packages/sdk/src/lines.ts
  - packages/sdk/src/mailbox.ts
  - packages/sdk/src/process.ts
  - packages/sdk/src/run.ts
  - packages/sdk/src/session.ts
  - packages/sdk/src/types.ts
documented-by:
  - docs/website/docs/using/sdk.md
---

## Scope

A TypeScript library, `packages/sdk`, that a Node program uses to run the `bravebot` command line
and read what it writes. It starts the command as a subprocess, sends it lines, and returns typed
events and result objects. It is not a crate, links no Rust code, and does not use the session view
of [session-view.md](session-view.md), which `packages/agent-client` speaks. The command line it
wraps is described in [cli.md](cli.md): the event stream is `--json-stream` and the line input is
`--input-format lines`.

The library is TypeScript only. The repository already builds, lints and tests Node packages with
pnpm in CI, and `packages/agent-client` is the precedent for a client library kept beside the
crates. A Python package would add a toolchain, a lockfile policy and a CI job, and is left to a
follow-up. The package is named `@brave/bravebot-sdk` and lives in `packages/sdk`, beside
`packages/agent-client`.

The library is a client of the command line. Everything it can do is something a person can do by
typing the same flags and lines, so the command line, and not the library, decides each permission,
each trust question and each label. A program that uses the library is a program deciding, which is
why a person starts it, and nothing here lets that program stand in for the person.

## Clauses

<a id="SDK-1"></a>
### SDK-1: the library starts the command line and adds no capability the command line lacks

A one-shot run starts one process with `--json-stream`, the options the caller gave, and
`-p` followed by the prompt. A session starts one process with `--json-stream` and
`--input-format lines`, the options the caller gave, and `--trust-input-lines` only when the caller
asked for it ([SDK-4](#SDK-4)). Both read the `bravebot` found on the path, or the command the
caller names. Nothing else is started, no file is read or written by the library, and no
environment value is read except to hand the caller's environment to the process.

**Why.** A library that could do more than the command line would be a second place where a
permission is decided, and the second place is the one nobody reviews. Keeping it a framing layer
leaves every decision to the program that is already specified and tested.

`verified-by: by-construction (packages/sdk/tests/args.test.ts holds the exact command line of a one-shot run and of a session and every flag an option becomes, and tests/real-binary.test.ts runs both against a real bravebot; make check-sdk runs them and the SDK CI job runs that target)`

<a id="SDK-2"></a>
### SDK-2: the options are a closed list, and none lowers a permission, vouches for a path or writes the trust map

The options are the working directory, the environment, the command, the model, the effort, the
output schema, the round and token limits, the tool names and the no-shell switch (both narrow
what the planner can call), the system prompt, incognito, the session record to resume, an abort
signal, a sink for the error stream, and a callback for each event. A session adds the trust of its
own lines ([SDK-4](#SDK-4)), a handler for questions ([SDK-6](#SDK-6)), and a callback for failures.
Every other option name is refused with an error before anything is started, so a flag that
lowers a permission, accepts edits, trusts a workspace or directory, adds a directory or replaces the
settings cannot be passed under another name. The library has no
function that writes the trust map, vouches for a path, or approves an effect.

**Why.** Each of those flags is something a person types or sets once, knowing what it grants. A
library option for one would let a program grant it for every run on the person's behalf.

`verified-by: by-construction (packages/sdk/tests/args.test.ts holds the exact option list, faults an option under any spelling that is not on it, and holds that no command line it builds carries a flag from the list above; tests/public-surface.test.ts holds the exported names and faults a function that approves, answers a permission or vouches for a path; make check-sdk runs them)`

<a id="SDK-3"></a>
### SDK-3: a value that begins with a dash is refused before the command is started

A prompt or an option value that begins with `-` is refused with an error and nothing is started,
because the command line would read it as a flag. A number option that is not a positive integer,
a tool name that holds a comma, and an empty string are refused the same way. The library does not
rewrite a value to make it acceptable.

**Why.** Turning a prompt into a flag would let words from anywhere decide which command runs.

`verified-by: by-construction (packages/sdk/tests/args.test.ts faults a value starting with a dash, a number that is not a positive integer and an empty value before any process is started)`

<a id="SDK-4"></a>
### SDK-4: the words a session sends are not the person's unless the caller says so

A session sends each prompt and steer as one JSON line. Unless the caller started the session
with `trustInputLines`, the command line treats the line as untrusted and private: the planner is
given a reference to the text and never its words, as for piped input. With `trustInputLines` the
command line is started with `--trust-input-lines`, which takes the lines as the person's own
words. The option is off by default, is named for what it does, and is the only way the library
changes how a line is labelled. A prompt sent without it is a turn with no prompt of its own and
the words as piped input ([the command line spec](cli.md)), so the planner is given a reference and
the user message of that turn can be empty until `brave/bravebot#2950` is fixed, which a provider
may refuse. A caller that needs the planner to read its words passes `trustInputLines`.
The library never sends a line it built from the content of an event, a result or the error stream.

**Why.** The flag says that the program sending lines is the person typing them. Only the person
who starts the program can say that, so a caller has to pass it on purpose and the library does
not default to it.

`verified-by: by-construction (packages/sdk/tests/session.test.ts holds that the flag is passed only when asked for and that a prompt with quotes, newlines and U+2028 is one line; tests/real-binary.test.ts holds against a real bravebot that without the option the planner never receives the words and with it does; make check-sdk runs them)`

<a id="SDK-5"></a>
### SDK-5: the library reads only the envelope of what the command line writes

Each line of standard output is one JSON object. A line with an `event` field is an event, and the
field must be a string. A line without one is the result object that ends a run or a turn, and its
`ok` must be a boolean and its `status` a number. For an event named `question` the
library also reads its number, which an answer must name. It reads nothing else to decide anything:
`reply`, `message`, `target`, `reason`, the words of a question, a refusal's text and every
field it does not know are handed to the caller whole, as the strings the command line wrote.
The `target` of a call, the `reply` of a result and a question's words can hold text from outside
the run, so they are data and not instructions, and the library neither matches them against
anything nor puts them in a command line or a line it sends. A line that is not JSON, is not an
object, or is neither an event nor a result of that shape ends the run
with an error that does not repeat the line, and the process is stopped.

**Why.** A branch whose condition comes from such a string would be a decision made by whoever wrote
the string. The envelope is fixed by the command line, so it is the only thing safe to branch on.

`verified-by: by-construction (packages/sdk/tests/run.test.ts feeds a reply, a message and a call target full of JSON, newlines and escapes and holds that they come through whole, holds a result cut into three-byte chunks inside multi-byte characters, and faults a line that is not JSON without repeating it; tests/session.test.ts holds the same through a session; make check-sdk runs them)`

<a id="SDK-6"></a>
### SDK-6: a planner's question is answered by positions, and nothing else is offered to answer

A session with a `onQuestion` handler calls it with the `question` event and sends the positions
it returns as an `answer` line for that event's number: one array of positions for each question,
and an empty array to decline one. The library does not turn words into positions, check the
positions against the choices, or send a typed answer, so the command line enforces its own limits
and reports a refused answer as an `input_refused` event. A handler that throws, or returns
anything but arrays of integers, declines every question in the event and the failure goes to the
caller's failure callback. A session with no handler sends no answer, and the command line declines
the question when the turn is stopped or the input closes. The library has no handler, option or
method for a question that asks whether to allow an effect, and the command line writes no such
question to a session: it refuses an effect that would ask. The line input
also takes a typed answer with `--trust-input-lines`, and the library has no method that sends one.

**Why.** Positions are numbers a program picks among options the kernel released, so a program
answers without receiving or sending words from outside. Permission questions are a decision about
an effect, which the person makes.

`verified-by: by-construction (packages/sdk/tests/session.test.ts holds the handler, a handler that throws or returns words, a session with no handler, and the lines for an answer, a stop and a steer; tests/real-binary.test.ts answers a real question by position; tests/public-surface.test.ts faults a permission handler or a typed-answer method on the exported surface; make check-sdk runs them)`

<a id="SDK-7"></a>
### SDK-7: a result and its events keep the names the command line wrote

A result carries every field of the result object, including `status`, `reason`, `identifier`,
`message`, `reply`, `session`, `structured` and the lists of calls and refusals, and keeps fields
added within a schema number. A refusal carries its `gate` and `principle` as the fixed names the
command line wrote, so a program can branch on a name and on the exit status, and has no reason to
read prose. An event of a kind the library does not know passes through with all its fields.
The library invents no outcome such as an interrupted turn, since the result has no field for one.
A run that ends badly resolves with its result. It rejects only when there is no result to give:
the command could not be started, the run was aborted, or the process ended or wrote something that
is not the stream before a result.

**Why.** The command line adds fields and events within a schema number, so a library that dropped
or renamed what it did not know would hide them from the programs written for them.

`verified-by: by-construction (packages/sdk/tests/run.test.ts holds a result with unknown fields and an unknown event kind, a run that ends in a refusal and resolves with its status, reason and principle, a process that ends without a result, a stdout that closes in the middle of a line, a command that does not exist, and an abort; tests/real-binary.test.ts holds the fields of a real result and a real refusal, and an input_refused event; make check-sdk runs them)`

<a id="SDK-8"></a>
### SDK-8: the error stream goes where the caller says and is never read

The standard error of the process is passed to the caller's sink as text, or inherited when there is
no sink. The library does not parse it, because it holds progress, messages and trails that are
prose and are localized. A one-shot run closes the process's standard input at once, so the command
line never waits for input, and a session closes it only when the caller closes the session. A
session whose process ends while it is open rejects its next turn.

`verified-by: by-construction (packages/sdk/tests/run.test.ts and tests/real-binary.test.ts hold that the sink receives the text, that stdout holds only the stream, and that a run without a sink does not fail; tests/session.test.ts holds a process that ends while the session is open and a kill in the middle of a turn)`

<a id="SDK-9"></a>
### SDK-9: what the library does not offer

The library offers no partial text, no user or assistant message events, no permission events with a
way to answer them, no tool extension, no server configuration, no per-session system prompt beyond
the option of [SDK-2](#SDK-2), and no enabled-tool list beyond narrowing the tools. The command
line's stream has none of these. Two costs of the line input hold for a session: a later turn
cannot work on a reference an earlier turn made, since each turn is its own task, and the planner
is told that the user chose a position the program picked. A typed answer to a planner's
question is built in the line input, and a session cannot send one, since it hands a program a
decision that a person makes.

`verified-by: by-construction (packages/sdk/tests/public-surface.test.ts holds the exported names, so an addition to the surface is a diff to that list and to this clause)`

<a id="SDK-10"></a>
### SDK-10: no callback receives quarantined content

The callbacks are `onEvent`, `onQuestion`, `onError` and `onStderr`. The text they receive is what
the command line wrote: the events and results on standard output, including the words of a
`question` event that the kernel released to put the question, and the text of the error stream, which
goes to the caller's own sink. A prompt or steer line the caller sent without `trustInputLines` is
never echoed back to a callback by the library. The library has no callback, method or event that
receives the content of a file, a fetched page or a tool result.

**Why.** Quarantined content is carried to the planner's processors and never shown to a program that
the planner's output reaches. A callback that received it would hand that content to code that
branches on it.

`verified-by: by-construction (packages/sdk/tests/args.test.ts holds the exact option list, so a new callback is a diff to that list and to this clause; tests/real-binary.test.ts holds against a real bravebot that no callback receives the words of an unvouched prompt; tests/public-surface.test.ts holds the exported names; make check-sdk runs them)`
