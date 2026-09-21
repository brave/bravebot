# `bravebot-bridge`: the library and its protocol

This document began as the Phase 0 design. The request and event descriptions below
cover the current bridge; §13–§14 retain historical implementation notes. For exact
wire shapes, consult [`protocol.rs`](../crates/bravebot-bridge/src/protocol.rs),
[`wire.rs`](../crates/bravebot-bridge/src/wire.rs),
[`bridge.rs`](../crates/bravebot-bridge/src/bridge.rs) and
[the UI types](../src/shared/protocol.ts). Setup instructions live in [setup.md](setup.md).

The Electron app does not drive a terminal and does not parse one. It talks to a Rust
**library**, `bravebot-bridge`, which lives in this repository and depends on
`bravebot` as an ordinary Cargo dependency.

**`bravebot` is not modified. Zero files, zero new crates, zero refactors.**
That is a hard constraint on this design, not an aspiration, and §2.3 says what would
violate it.

In v1 the library is reached through a thin binary, `bravebot-rpc`, that speaks
newline-delimited JSON on stdin and stdout. The protocol in §4–§9 is the library's
surface expressed as messages; it would be the same set of calls across an FFI boundary,
so the transport is a detail this document deliberately keeps replaceable (§2.2).

Everything here is derived from types that already exist in `bravebot`. Where a
message field maps to a Rust field, the Rust type is named. Where the protocol invents
something, it says so.

---

## 1. Why this shape

`bravebot_agent::turn::resume` already takes its user interface as three traits — a
`Confirmer` for approvals, a `Reporter` for progress, and an `event::Sink` for the audit
trail — plus a `Cancel` token. Nothing in the turn engine knows about a terminal.

`bravebot_tui::remote_confirm` has already exercised that seam for a different reason: a turn
runs on a worker thread, so it cannot touch the terminal, and it talks to the thread that
can over one `mpsc` channel carrying a `ToMain` enum. That enum is, in substance, the
protocol below. This phase moves the far end of that channel out of the process.

Three consequences follow, and they are the reasons for doing it this way:

- **The GUI cannot weaken the security model.** It is a `Confirmer` implementation like
  any other, subject to the same rule that every failure resolves to refusal.
- **Sessions stay one thing.** The bridge reads and writes the same records under
  `~/.bravebot/sessions` as the TUI, so a session begun in the app resumes with
  `bravebot --resume`, and one begun in a terminal appears in the app.
- **Crashes stay on one side.** A renderer bug cannot corrupt a turn; a panic in the
  agent surfaces as a closed pipe, which the protocol already defines as refusal.

### 1.1 Why a subprocess rather than a native addon

The alternative is napi-rs: link the library into the Electron process and call it
directly. It is a reasonable instinct — no framing, no supervision, no serialisation —
and the library layout in §2.1 keeps it available later. It is not what v1 should do, for
three reasons specific to this codebase.

**The turn engine blocks on a human.** `turn::resume` is synchronous and
`Confirmer::confirm_write` blocks its thread until someone answers. In-process that means
a worker thread calling back into JS through a `ThreadsafeFunction` and then blocking on a
channel for the reply — the same `mpsc` dance `remote_confirm.rs` already does, plus
napi's threadsafe machinery, plus a standing invariant that the JS main thread must never
block or the whole app deadlocks. The complexity is relocated, not removed.

**Process isolation makes shutdown explicit.** Closing the bridge's stdin drops the
bridge and refuses pending questions. Electron closes stdin on window close and app
quit. A native addon in the main process would share that process's failure boundary;
the renderer remains a separate process in either design.

This is not a guarantee that every display failure closes the pipe. A renderer-only
crash currently has no dedicated shutdown handler, and the transport ignores stdout
write failures. In those cases a question can remain pending until shutdown or
cancellation, but cannot become an approval. See [security](security.md).

**Practical drag.** A native module needs rebuilding and re-testing per Electron ABI,
with prebuilds per architecture, where a plain binary is already cross-built by the
existing `Makefile` and `Dockerfile.cross`. And Electron's main process would end up
holding the keyring access and `Egress`'s connection pool.

None of this is permanent. §2.2 is what keeps it cheap to revisit.

---

## 2. Where the code lives

Entirely in this repository:

```
crates/bravebot-bridge/
  src/lib.rs               crate root and public modules
  src/bridge.rs            dispatch, session lifecycle and turns
  src/turn.rs              Confirmer, Reporter and audit sink
  src/protocol.rs          message envelopes and errors
  src/wire.rs              agent types projected into JSON
  src/store.rs             shared session records
  src/fork.rs              conversation cuts
  src/running.rs           pending replies and cancellation
  src/bin/bravebot-rpc.rs  stdin/stdout transport
```


`crates/bravebot-bridge/Cargo.toml` depends on the agent as a normal Cargo dependency — a
path dependency into `vendor/bravebot`, which is a git submodule, so the path says where the
sources are and the committed gitlink says which revision they are:

```toml
[dependencies]
bravebot-agent = { path = "../../vendor/bravebot/crates/agent" }
bravebot-session = { path = "../../vendor/bravebot/crates/session" }
bravebot-tui   = { path = "../../vendor/bravebot/crates/tui" }
bravebot-core  = { path = "../../vendor/bravebot/crates/core" }
bravebot-config = { path = "../../vendor/bravebot/crates/config" }
```

This works today, unmodified, and it was checked rather than assumed:

- These are ordinary packages with workspace-inherited metadata and no `publish = false`.
- **Every module the bridge needs is already `pub`**: `bravebot_session::{sessions, store, audit}`,
  every module of `bravebot_agent`, and `bravebot_core::{event, label, todo, trust}`.
- `crates/tui/build.rs` shells out to git to stamp `BRAVEBOT_BUILD` and degrades to
  a version string without a git revision when there is none, so it compiles as a git or vendored dependency.

### 2.1 Reuse the upstream session crate

Upstream now provides `bravebot-session` for the on-disk record, state directory,
prompt history, and audit projection. The bridge uses these public types directly
and reuses `audit::as_json` rather than deriving its own event projection. Standing
watches live in `bravebot_agent::watch`.

`BUILD` still comes from `bravebot_tui::BUILD`, so both front-ends stamp records with
the same string. Session handles receive this stamp explicitly.

### 2.2 Keeping the linkage replaceable

Everything above the transport lives in library modules and knows nothing about stdio. `bravebot-rpc`
is a framing shim: read a line, call a library method, serialise the result. `Bridge::dispatch` routes the methods in §7 and takes a callback for events rather
than writing them directly to stdout.

If in-process linkage is wanted later, it is a second front-end beside `bravebot-rpc`, and the
library does not change. Two rules preserve that, and reviewers should enforce them:

1. **No `println!`, no stdout, no process exit below `src/bin/`.** The kernel never
   prints for the same reason.
2. **Events leave through the callback, never a global.** A second front-end must be able
   to install its own.

### 2.3 What would break the zero-change constraint

Honest limits. Two things could eventually want an upstream change, and neither blocks v1:

- **Structured `doctor` output.** The checks live in `crates/cli/src/main.rs`, a binary,
  so they cannot be called as a library. v1 shells out to `bravebot doctor` and shows its text
  (§7.3). A small upstream extraction would be nicer and is optional.
- **New approval types.** Command approval is implemented. The v0.9.0 fetch-host,
  language-server and manifest-plan requests are currently refused; adding UI support
  requires adapting the bridge, not editing upstream.

If anything else appears to need an upstream edit, that is a signal the bridge is
reaching for something it should not, and it should be raised rather than patched.

---

## 3. Transport

- **Framing.** One JSON object per line, UTF-8, `\n`-terminated. No embedded raw
  newlines: `serde_json`'s compact form never emits one, and the client must not either.
- **Direction.** Client → agent on `bravebot-rpc`'s **stdin**. Agent → client on its
  **stdout**. Nothing else is written to stdout, ever.
- **stderr** carries human-readable diagnostics only. The client should capture it for
  bug reports and must never parse it.
- **No length prefix, no Content-Length header.** A line is a message.
- **Malformed input** — invalid UTF-8, unparseable JSON, an object with no `id` — is
  answered with a protocol error (§7.3) if an `id` can be recovered, logged to stderr if
  not, and otherwise ignored. It never terminates the process, because a client that can
  produce one bad line will produce another and a dead agent loses in-flight work.
- **Exit.** `bravebot-rpc` exits 0 on clean EOF of stdin, having first refused every pending
  confirmation (§8.4). Any other exit is a bug and should be reported as such.

### 3.1 Invocation

```
bravebot-rpc
```

Normal operation takes no arguments. `--version` (or `-V`) prints the bridge version
and the upstream build string. Configuration comes from runtime variables and baked
values via `Config::from_env`; a shell-launched app inherits that shell's environment.
The client should surface the build string, because a transcript read after the
fact is read to find out what went wrong and the first question is which build produced
it.

`bravebot-rpc` is a self-contained binary. It does not shell out to `bravebot` and does not need
one on `PATH`, with the single exception of `doctor` (§7.3, §2.3).

### 3.2 Credentials are a build-time input

The upstream configuration build script bakes available Brave backend values into
`bravebot-rpc`; runtime values override them. This repository opts into unconfigured
builds with `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` in `.cargo/config.toml`, so missing
credentials do not prevent development or CI builds. They do prevent inference unless
configuration is supplied at runtime. See [credentials](setup.md#credentials) for
loading, strict build validation and the difference between shell and Finder launches.

`agent.info` exposes `configured` for readiness checks. Setup help and diagnostics
explain missing configuration without exposing credential values.

## 4. Message envelope

Three shapes, distinguished by which keys are present.

**Request** (client → agent):

```json
{ "id": 17, "method": "turn.send", "params": { ... } }
```

`id` is a client-chosen unsigned integer, unique within the process lifetime.
The parser accepts zero; the Electron client generates increasing IDs.

**Response** (agent → client), exactly one per request, `ok` xor `error`:

```json
{ "id": 17, "ok": { ... } }
{ "id": 17, "error": { "code": "no_such_session", "message": "…" } }
```

**Event** (agent → client), unsolicited, never carries `id`:

```json
{ "event": "tool.started", "session": "s3", "data": { ... } }
```

Every event except `agent.ready` carries `session`. Responses may be interleaved with
events and may arrive out of order relative to one another; the client correlates on
`id`.

---

## 5. Handles and identity

A session's `id` (from `Record::id`) is unique only **within its project directory** —
`~/.bravebot/sessions/<mangled-directory>/`. Rather than make every call carry a
`(directory, id)` pair, `session.open` and `session.new` mint an opaque **session
handle**: a short string, unique for the life of the process, used by every subsequent
call and stamped on every event.

Handles are not persistent and must not be stored by the client. `session.list` returns
`(directory, id)`; `session.open` converts that into a handle.

---

## 6. Type mappings

The protocol is a JSON projection of existing Rust types. This table is normative; the
implementation should have one serialisation function per row and a round-trip test.

| Rust | JSON | notes |
|---|---|---|
| `confirm::Intent` | `"create"` \| `"overwrite"` \| `"edit"` | |
| `confirm::Decision` | `"approve"` \| `"reject"` | |
| `diff::Change` | `{"kind":"kept"\|"added"\|"removed","text":"…"}` or `{"kind":"elided","lines":N}` | |
| `report::Phase` | `"planning"` \| `"thinking"` \| `"compacting"` \| `"reconnecting"` | |
| `report::Reach` | `"not_the_planner"` \| `"no_model"` | |
| `report::Landing` | `"context"` \| `"quarantined"` \| `"reserved"` | |
| `todo::Status` | `"pending"` \| `"active"` \| `"done"` | |
| `conversation::Said` | `{"kind":"user"\|"assistant"\|"tool","text":"…"}` | from `recounted()`, §7.1 |
| `core::event::Event` | as `audit::as_json` already produces | **reuse verbatim**, do not re-derive |
| `label::Label` | `{"integrity":"trusted"\|"untrusted","confidentiality":"public"\|"private"}` | as `audit::label_json` |
| `SystemTime` seconds | JSON number, seconds since epoch | matches `Record::started`/`updated` |

Two rules for the enums above:

1. **Serialise the discriminant, not the prose.** `Landing::describe()` and
   `Reach::describe()` return sentences meant for a screen; they are wording, they will
   change, and a client that matches on them is matching on prose. Send the tag. The
   client may render its own words, or call the Rust wording a hint — but the tag is the
   contract.
2. **Unknown tags degrade toward less trust.** A client reading a tag it does not
   recognise treats it as untrusted/quarantined, mirroring what `Snapshot` and
   `Record::trust_map` already do on the Rust side. Never the other way.

`report::Activity::verb` is a `&'static str` chosen by the dispatch table, never model
output; it is safe to send as-is and safe to switch on.

---

## 7. Requests

### 7.1 Session management

#### `session.list`

```json
{ "id": 1, "method": "session.list", "params": { "directory": null } }
```

`directory` absent or `null` lists sessions across **every** project directory under
`~/.bravebot/sessions`, newest `updated` first. A string lists only that project's.

Note that `sessions::list(project)` is per-project today. The cross-project listing is
new: enumerate the child directories of `~/.bravebot/sessions`, call the existing `list` for
each, and merge. `Record::directory` holds the real path (the directory-name mangling is
not reversible), so it is read out of the record rather than un-mangled.

```json
{ "id": 1, "ok": { "sessions": [
  { "id": "…", "directory": "/Users/me/repos/thing", "project": "thing",
    "branch": "main", "title": "fix the parser", "updated": 1756300000, "bytes": 41233 }
] } }
```

`project` is the basename of `directory`, computed by the agent so both front-ends agree.

#### `session.open`

```json
{ "id": 2, "method": "session.open", "params": { "directory": "…", "id": "…" } }
```

Loads the `Record` via `sessions::load` and the trail and todos via `sessions::recall`.

```json
{ "id": 2, "ok": {
  "session": "s1",
  "record": { "id": "…", "directory": "…", "branch": "main", "title": "…",
              "started": 1756200000, "updated": 1756300000,
              "turns": 4, "tokens": 51234, "build": "0.1.0 (abcdef1)" },
  "said": [ { "kind": "user"|"assistant"|"tool", "text": "…" } ],
  "context": "trusted",
  "todos":  { "1": [ { "content": "…", "status": "done" } ] },
  "audit":  { "1": [ { "at": 1756200003, "event": { "kind": "gate_passed", … } } ] },
  "trust":  { "known": true, "rules": [ { "path": "…", "integrity": "trusted" } ] },
  "branchNote": "the branch has changed since this session ran",
  "buildNote":  null
} }
```

- `said` comes from `Conversation::recounted()`, **not** from `Snapshot::messages`. That
  method is the existing display projection of a stored conversation and it already makes
  the decisions this protocol would otherwise have to re-make, in the same place the TUI
  makes them: system and tool-role messages are dropped; a tool *result* is left out
  because it was written for the planner and a resumed transcript is for the user; an
  assistant message contributes its prose and then one `Said::Tool` line per tool call,
  described by `tools::describe_stored_call`. Sending raw `Snapshot::messages` instead
  would put tool results — whole file bodies, where the live session showed a one-line
  summary — on the screen.
- A `Said::Tool` line says only that a call happened and what it was about. **The record
  does not store what came of it**, so the client must not render a result or a status
  beside a replayed tool line. Live turns get `tool.started`/`tool.finished` with a
  `note`; replayed ones do not, and inventing an outcome would be worse than the gap.
- `context` is `Snapshot::context`, the word for what the conversation has met.
- `todos` and `audit` are keyed by turn number as strings, because JSON object keys are
  strings; the Rust side is a `BTreeMap<usize, _>`.
- `trust.known` is `false` when `Record::trust` is `None` — a record written before the
  map was kept. **Nothing recorded is not the same as nothing trusted**, and the client
  must ask (§9) rather than assume an empty map.
- `branchNote` / `buildNote` are the existing `sessions::branch_note` and
  `sessions::build_note` outputs: a sentence, or `null` when there is nothing to say.

`session.open` does **not** start a turn. If the record has no saved trust map,
it emits `trust.request`; the client must answer before sending a turn.

#### `session.new`

```json
{ "id": 3, "method": "session.new", "params": { "directory": "/Users/me/repos/thing" } }
```

Returns `{ "session": "s2", "directory": "…", "branch": "main" }`. The `Record` is not
written until the first turn, matching `Session::begin` + `save`, so an opened-and-
abandoned window leaves nothing behind.

Errors with `not_a_directory` if the path is not one, or `no_home` if `~/.bravebot` cannot be
located.

#### `session.fork`

```json
{ "id": 5, "method": "session.fork",
  "params": { "session": "s1", "prompt": 2, "text": "make it handle quotes" } }
```

```json
{ "id": 5, "ok": {
  "session": "s7", "id": "1756300000-4711", "directory": "…", "branch": "main",
  "said": [ { "kind": "user"|"assistant"|"tool", "text": "…" } ],
  "prefill": "make it handle quotes",
  "context": "trusted",
  "turns": 2,
  "todos": { "1": [ { "content": "…", "status": "done" } ] },
  "trust": { "known": true, "rules": [ { "path": "…", "integrity": "trusted" } ] },
  "parent": { "id": "…", "directory": "…", "title": "fix the parser", "prompt": 2 }
} }
```

Begins a session holding everything the parent said *before* one of its prompts. `prefill` is
that prompt, handed back rather than kept, because the point of forking is to ask it
differently: the front-end puts it in the composer and the person edits it.

- `prompt` is a **0-based ordinal over `Said::User`** — the prompts a transcript drew — and not
  a turn number. `text` is what that prompt said. Both are sent because they check each other:
  the ordinal says where to cut, and the text says the front-end's idea of where agrees with the
  conversation's. They can disagree. The agent writes user-role messages of its own that
  `recounted` does not filter — a context file arrives as `Contents of …`, and a turn that
  spends its tool budget is nudged with one — so a window that never drew them counts
  differently from the conversation that holds them. A mismatch is `bad_request`; a fork taken
  one prompt away from where somebody pointed is worse than one that did not happen.
- The ordinal is turned back into a message index by walking `archive ++ messages` and consuming
  the drawn prompts in order, matching on text. That is an **alignment, not a second copy of
  `recounted`'s rules**: those rules drop a user message on what its text starts with, so a
  message whose text equals a prompt the transcript showed cannot have been one of the dropped
  ones. If a later build filters on something the text does not carry, the walk runs out of
  matches and the fork is refused rather than cut somewhere else.
- **The cut is a well-formed request by construction.** It lands in front of a prompt, which is
  the same boundary compaction uses, so it cannot come between a call and its result; and
  `Conversation::with_system` answers any call left unanswered anyway. Nothing here re-implements
  tool-call pairing, and `crates/bravebot-bridge/src/fork.rs` pins both halves of that.
- A cut inside the archive takes the child's whole history out of it: `archive` empties into
  `messages`, since there is no longer a request for a compaction summary to stand in for. A cut
  after it keeps both, summary included.
- `measured` is reset to **0** — the figure described a conversation that no longer exists, and a
  child inheriting it would open by trying to compact a history it has not sent. `references` is
  carried, because it exists so a name is never handed out twice. `context` is carried verbatim
  and **never raised**: integrity is met over a session's whole life and no message records its
  own, so it cannot be recomputed for a prefix, and the only direction it may be wrong in is
  downwards.
- The **trust map, the vouched programs, and any extra open directories** are inherited
  from the parent's live state — the same person, the same directory, the same window,
  which is the argument `session.open` already makes for a resume. Extra directories are
  the other half of a grant a trust rule cannot express: an absolute path is refused
  unless its directory is open, whatever the map says. This front-end has no `/add-dir`,
  so a session begun here never opens any; they are still carried so a session begun in
  the terminal does not lose them when it is forked or saved here. It is worth naming
  what the rest of this inheritance gives up: a program vouched for *after* the cut is
  not in the child's history, and `TrustedPrograms` has no timeline to filter by. The
  alternative is asking the same person about the same command again, which is how people
  are taught to click through questions. `trust.known` is `false` when the parent was
  itself still holding the question, and then the fork emits `trust.request` exactly as a
  new session does.
- `turns` and `todos` are the parent's, cut to the same place: a turn's plan belongs to its turn.
  Tokens start at nothing, because that figure answers "what has this session cost me".
- The fork keeps the **title of the session it came from**, since its title is derived from the
  first thing said in the history it kept. A fork at the first prompt has kept nothing, so it is
  named by whatever is sent next, like any new session.
- **Nothing is written.** The `id` is real and reserved from here, but the record appears on the
  first turn, matching `session.new`: a fork opened and abandoned leaves no trace.
- Refused with `turn_in_flight` while the parent has a turn running. Not tidiness: a worker holds
  the session's state for the whole of its turn and dispatch is one thread, so a fork that waited
  for that lock would stop the bridge answering anything — including the question the turn is
  blocked on.
- Refused with `bad_request` for a session that has not been written down yet: it has no history
  to fork and no id to point back at.
- Session ids are `<second>-<pid>`, so two begun in the same second in one process are the same
  session as far as the store is concerned. Nothing else can reach that — every other way of
  starting one has a turn's worth of time in front of it — but two forks are two clicks. The
  bridge therefore waits a second out rather than handing back an id something else is holding.

**Lineage is not in the record.** `Record` has no field for a parent, and adding one is an
upstream change (§2); worse, `Handle::save` rebuilds the record wholesale, so a key written
beside it would be erased by the fork's first turn. The front-end keeps it instead, in a
`forks` key in `bravebot-ui.json` under `userData`, written by the main process **from this response** —
never from what the renderer asked for. That is the same promise the recents list makes: the
renderer can read the list and ask for something on it, and has no way to write to it.

#### `session.close`

```json
{ "id": 4, "method": "session.close", "params": { "session": "s2" } }
```

Releases the handle. If a turn is running it is cancelled first and any pending
confirmation is **refused** (§8.4). Returns `{}` once the worker has joined.

### 7.2 Turns

#### `turn.send`

The optional `model` parameter selects a model ID for this and subsequent turns on the
open session handle. Provider IDs remain qualified (for example,
`openrouter/anthropic/claude-haiku-4.5`). Omitting it or passing `null` retains the
handle's previous choice, or uses the configured default for a fresh handle.
The UI remembers explicit choices by durable session ID and resends them on reopening.

`models.list` takes no parameters and returns `{ models, defaultModel, warnings }`.
Each model contains `id`, `name`, `provider`, `premium`, and nullable `contextWindow`.
It also includes `capabilities`, an array of reported capability identifiers: `text`,
`vision`, `image-output`, `audio-input`, `audio-output`, `video`, `files`, `tools`,
`reasoning`, and `structured-output`. An empty array means no supported capabilities
were reported to the UI, rather than proof that the model lacks them.
Discovery uses configured backends; partial failures return warnings alongside available
models and the configured default. `session.new` and `session.open` include `model`
with the configured default, or `null` when configuration is unavailable.

```json
{ "id": 5, "method": "turn.send",
  "params": { "session": "s1", "prompt": "why does the parser drop trailing commas?",
              "files": ["notes.md"], "dropped": ["/Users/me/briefing.md"] } }
```

Builds a `Workspace` over the session's project, then re-opens any extra directories the
record still lists. A workspace is built per turn and opens the project only, so those
paths have to be opened again here: the trust rules about them came back with the map,
and a rule about a directory nothing can open refuses every path under it for escaping
the workspace. One that has since moved or been deleted is left closed — the refusal it
causes is the one that was already happening, and this protocol has no way to say so
outside a turn.

Builds a `Task::new(prompt)`, applies `with_file` per entry of `files`,
`with_dropped_text` per entry of `dropped`, and `with_home(home::directory())`.

Both lists are optional and both default to empty. They differ in where a path may point
and in nothing else: `files` is workspace-relative and read inside the project, while
`dropped` may name anything on the disk — upstream calls it the read that is not confined
to the workspace, because a dropped path came from a gesture rather than from anything a
model said. Both are trusted for the same reason, both are vouched for by being named
(`policy.vouch_for_named_path`), and both land in the conversation as ordinary user
messages reading `Contents of <path>: …`, which means **they accumulate**: a file attached
to every turn is a copy of that file per turn.

A path that cannot be read as text ends the turn — `turn.error` with `kind: "workspace"` —
rather than being skipped. A caller that attaches a file it maintains has to make sure the
file is there immediately before the send, not once when it was first named.

Neither list may be named by a renderer in this app; see §9 and `src/main/index.ts`. Spawns the worker thread and calls `turn::resume` with an
RPC `Confirmer`, an RPC `Reporter`, and a `Trail` sink — the same call shape as
the upstream TUI, differing only in where the three handles send.

Returns immediately: `{ "turn": 5 }`, the turn number within the session. Progress
arrives as events; completion as `turn.done` or `turn.error`.

The prompt is appended to `~/.bravebot/history` (via `store::append_history`), so
recall works across both front-ends, and it names the session if the session has no name yet.

`recall` (optional, default `true`) governs both. A front-end that sends a turn **on its own
account** rather than on a person's — the window asking a bot to bring its memory up to date after
a compaction, say — sets it `false`, and the prompt then joins neither the history nor the title.
Recall is for what somebody typed: boilerplate one front-end wrote would otherwise turn up under
the up-arrow in the other, and a conversation would be named after the one thing nobody in it
asked. Anything that is not a boolean reads as the default, so a caller that has never heard of it
keeps the behaviour it had.

**One turn at a time per session.** A second `turn.send` on a session with a turn in
flight errors `turn_in_flight`. Different sessions run concurrently.

#### `turn.cancel`

```json
{ "id": 6, "method": "turn.cancel", "params": { "session": "s1" } }
```

Calls `Cancel::cancel()` on that turn's token — a fresh token per turn, never reused,
matching the upstream cancellation model. Returns `{}` immediately; the turn ends with
`turn.error` / `Cancelled` when the engine notices. Cancelling when nothing is running is
not an error.

A pending confirmation checks cancellation at most every 50 ms and resolves to refusal.
No approval is sent, and no additional client reply is required. Cancellation also
covers the race where the question is registered just after the stop request.

#### `confirm.reply`

```json
{ "id": 7, "method": "confirm.reply",
  "params": { "session": "s1", "request": 3, "decision": "approve" } }
```

See §8. Returns `{}`. An unknown or already-answered `request` errors
`no_such_request` and changes nothing — an approval is single-use and cannot be replayed.

#### Other decision replies

`run.reply`, `output.reply`, `vouch.reply` and `ask.reply` all require `session` and
`request`, and must match the pending question's kind as well as its ID.
`run.reply` accepts `decision` and `remember`; only an approval with literal
`remember: true` records a command grant. Output and vouch replies accept `decision`.
`ask.reply` accepts an `answers` array, whose entries contain `typed` text or `chosen`
indices; unreadable entries decline. Choices are fitted to the question before use.

#### Permissions

`permissions.list` takes `session` and returns `paths` and `commands`.
`permissions.revoke` takes `session` plus either `kind: "path"` and `path`, or
`kind: "command"` and `command: { program, args }`. It returns the updated lists.
These methods refuse with `turn_in_flight` while the session is running. Revocation
can reduce existing grants; it cannot add trust.

### 7.3 Trust and diagnostics

#### `trust.reply`

```json
{ "id": 8, "method": "trust.reply",
  "params": { "session": "s1", "directory": "/Users/me/repos/thing", "trusted": true } }
```

Records the user's answer to the startup question into that session's `TrustStore`,
before the first `turn.send`. See §9.

#### `doctor`

```json
{ "id": 9, "method": "doctor", "params": {} }
```

The app will otherwise fail opaquely on a machine with no credentials, which is the first
machine anyone will try it on.

**v1 shells out.** The checks live in `crates/cli/src/main.rs`, a binary, so they cannot
be called as a library without an upstream change (§2.3). The bridge runs `bravebot doctor`,
captures stdout and stderr, and returns their combined text:

```json
{ "id": 9, "ok": { "structured": false, "text": "…", "exitCode": 0,
                   "found": true } }
```

`found: false` when no `bravebot` is on `PATH` — the app should say so plainly and stay
usable, since `doctor` is a diagnostic and nothing else depends on it. This is the one
place the bridge needs the CLI installed.

If the checks are ever exposed as a library upstream, this returns
`{ "structured": true, "checks": [ { "name": "…", "ok": true, "detail": "…" } ] }`
instead. The flag exists so the client can be written once against both.

#### `agent.info`

`{ "build": "…", "version": "0.1.0", "home": "/Users/me/.bravebot",
"configured": true, "defaultModel": "…" }`. `version` is the bridge package
version; `home` and `defaultModel` may be null. Sent as the
`agent.ready` event at startup and also available as a request.

### 7.4 Error codes

| code | meaning |
|---|---|
| `bad_request` | malformed envelope, unknown method, missing or ill-typed params |
| `no_such_session` | unknown session handle |
| `no_such_request` | unknown or already-answered confirmation id |
| `turn_in_flight` | a turn is already running on that session |
| `not_a_directory` | the path given to `session.new` is not a directory |
| `no_home` | `~/.bravebot` could not be located or created |
| `config` | `Config::from_env` failed; `message` carries the detail |
| `internal` | a bug; `message` is for a report, not for a user |

---

## 8. Events

Approval, progress and lifecycle events carry `session`, except for `agent.ready`.

| event | `data` | source |
|---|---|---|
| `agent.ready` | `{ build, version, home, configured, defaultModel }` | startup, no `session` |
| `turn.started` | `{ turn }` | `turn.send` accepted |
| `phase` | `{ phase }` | `Reporter::phase` |
| `narration` | `{ text }` | `Reporter::narration`, **empty ones dropped** |
| `tool.started` | `Activity` | `Reporter::tool_started` |
| `tool.finished` | `Activity` | `Reporter::tool_finished` |
| `landed` | `{ landing }` | `Reporter::landed` |
| `quarantined` | `Shown` | `Reporter::quarantined` |
| `todos` | `{ rows: [ { content, status } ] }` | `Reporter::todos` |
| `tokens` | `{ written }` | `Reporter::output_tokens`, **only when the figure changes** |
| `audit` | `{ at, turn, event }` | the `Sink`, via `audit::as_json` |
| `confirm.request` | see §8.1 | `Confirmer::confirm_write` |
| `run.request` | `{ request, stages, directory, plan, writes, releasesPrivate, vouches, summary }` | command approval |
| `output.request` | `{ request, command, reference, lines, output, summary }` | admit command output |
| `vouch.request` | preview and label fields from `wire::vouch_request` | trust a quarantined path |
| `ask.request` | `{ request, prompts }` | user questions |
| `trust.request` | `{ directory }` | initial project trust |
| `turn.done` | see §8.2 | `Ok(Outcome)` |
| `turn.error` | see §8.3 | `Err(TurnError)` |

`Activity` serialises as:

```json
{ "verb": "read", "target": "src/main.rs", "note": "412 lines",
  "failed": false, "untrusted": false,
  "changes": [ { "kind": "added", "text": "…" } ] }
```

`note: null` means the call is still running — that is what distinguishes an unfinished
line from one that finished with nothing to say, and the client must render the two
differently.

`Shown` serialises as:

```json
{ "origin": "https://example.com/page", "reach": "not_the_planner",
  "label": "(U,priv)", "preview": ["…"], "lines": 240 }
```

`preview` is already trimmed by the kernel to 12 lines of at most 160 characters
(`turn.rs` `PREVIEW_LINES` / `PREVIEW_WIDTH`) and released for display and nothing else.
`lines` is the true total, so the client can say what it left out.

`audit` events stream **live**, as the sink receives them, in addition to being written to
`<id>.audit.jsonl` at the end of the turn by `append_audit`. The `at` stamp is the
event's own time, taken as it was emitted — not the time it was written down. A trail
whose events all share one second cannot say which came first.

### 8.1 `confirm.request`

```json
{ "event": "confirm.request", "session": "s1", "data": {
  "request": 3,
  "path": "src/parser.rs",
  "intent": "edit",
  "untrusted": false,
  "existing": true,
  "changes": [ { "kind": "kept", "text": "fn parse(" },
               { "kind": "removed", "text": "  let x = 1;" },
               { "kind": "added",   "text": "  let x = 2;" },
               { "kind": "elided",  "lines": 40 } ]
} }
```

- `request` is agent-assigned and **single-use** for its pending question. IDs can
  recur in later turns; clients must discard pending questions when a turn ends.
- `changes` is `WriteRequest::diff()`, already condensed. The full `contents` is **not**
  sent: the whole point of the design is that a reviewer reads a few lines rather than
  spotting a difference in a whole file, and shipping the body to the renderer invites a
  client to show it instead.
- `existing` distinguishes create from overwrite without sending the old body.
- `added`, `removed` and `exact` describe the diff counts and whether it is exact.
- `untrusted: true` means the body came from somewhere nobody vouched for — a file the
  planner never read, returned by an isolated processor. The Rust doc comment is explicit
  that reviewing this is a different act from reviewing the model's own work and the
  screen must not make the two look alike. **The client must render it distinctly**, and
  a UI review should check this specifically.

### 8.2 `turn.done`

```json
{ "event": "turn.done", "session": "s1", "data": {
  "turn": 5,
  "reply": "The parser drops them in `skip_separators`…",
  "model": "claude-…", "steps": 3, "clean": true,
  "tokens": 51234, "outputTokens": 812,
  "notices": ["loaded AGENTS.md"],
  "trust": { "rules": [ { "path": "…", "integrity": "untrusted" } ] },
  "id": "0f1c…", "archived": 0
} }
```

`reply` is `Outcome::reply_for_display()`, which was authorised inside the turn while the
policy was still open, so the release is in the audit trail. **Never send
`Outcome::reply`**, the `Labelled<String>`; the released string is the only one that may
leave.

`trust` is the map *after* the turn: a turn that wrote untrusted data into a trusted path
records that path as untrusted, and the next turn must carry it forward or it would read
the data back as trusted. The agent persists this itself; it is echoed so the client can
show the change.

`id` is the session's durable name, and this is the first moment it is one: a session
writes no record until it has something to say, so `session.new` has nothing to hand back
and the id is minted by the save below. `null` only where a record could not be written. A
client keeping a note of its own about a session — which is the only way to keep one, the
agent's record having no field for anybody else's — learns the name here rather than by
guessing which row in a refreshed list is the one it just made.

`archived` is how many messages compaction has taken out of this conversation, in total.
It only ever rises, and it rises exactly when the conversation stopped carrying what was
said before the summary. It is reported rather than inferred from the `compacting` phase
because that phase is emitted *before* compaction is attempted — so it also fires when
there was nothing worth compacting, and then on every round of a conversation that is over
budget and cannot get under it. Anything a client puts at the top of a session and needs to
stay there has to watch this figure, not that phase. `session.open` carries the same field,
read off the record, so a session resumed in a new process knows it without having watched
it happen.

The record is saved (`Session::save` with a `Standing`) before this event is emitted, so
a client that reloads on receipt sees the same thing on disk.

### 8.3 `turn.error`

```json
{ "event": "turn.error", "session": "s1", "data": {
  "turn": 5, "kind": "cancelled"|"precommit"|"workspace"|"chat", "message": "…",
  "id": "saved-session-id-or-null" } }
```

The four `TurnError` variants. A failed turn is still part of the conversation and the
conversation is handed back either way — the next question is usually about it — so the
client must keep the transcript, not discard it. `id` identifies the recoverable
record when the failed turn was saved; clients should migrate unsent draft
preferences to this ID just as they do for `turn.done`.

### 8.4 Failure semantics — the load-bearing part

`remote_confirm` states the asymmetry, and it is inherited exactly:

> A write **asks**. Progress **announces**.

Therefore:

- **Cancellation, session close and bridge shutdown refuse pending questions.**
  EOF on stdin drops the bridge. Malformed decisions never approve. The current
  transport ignores stdout write errors, so a closed stdout alone can leave a question
  waiting; likewise a renderer-only crash does not automatically stop the child.
  These are liveness limitations, not approval paths.

- **There is no timeout-to-approve.** There is no timeout at all in v1: a write waits for
  a human indefinitely, which is what it should do. If a timeout is ever added it
  resolves to refusal.
- **A progress event that cannot be delivered is dropped, silently.** Failing a turn
  because nobody was watching would let the display outrank the work.
- **An approval is bound to its `request` id and consumed on use.** A replayed
  `confirm.reply` errors and changes nothing.
- **A cancel refuses a pending write.** It wakes the blocked confirmer without approval.

These are the protocol's actual security properties. §12 makes them tests.

---

## 9. Trust

A `Record` carries its own trust map, deliberately: a map kept per directory would answer
the startup question on behalf of a user who was never asked.

The flow:

1. `session.new` → the agent emits `trust.request` with the directory. The client shows a
   modal. The client sends `trust.reply`. Only then may it `turn.send`.
2. `session.open` on a record with `trust.known: true` → inherited, no question. The
   person resuming is the person who gave it.
3. `session.open` on a record with `trust.known: false` → the client **must** ask before
   the first `turn.send`, because nothing recorded is not nothing trusted.

`turn.send` before trust has been answered for that session errors `bad_request`. Not a
default: defaulting either way is the mistake this design exists to avoid.

---

## 10. Concurrency

- One thread reads stdin and dispatches. It never blocks on a turn.
- One writer, holding a mutex on stdout, so lines cannot interleave. This is the same
  reason the kernel never prints.
- One worker thread per running turn, holding its own `Cancel`, its own `Egress`, and
  its configuration and workspace state.
- The `Confirmer` blocks its worker on an `mpsc::Receiver<Decision>`, which the dispatch
  thread feeds from `confirm.reply`. Unchanged from `RemoteConfirmer`; only the source of
  the answer moves.
- A session permits one running turn. There is currently no global four-turn cap;
  different sessions can run concurrently and each can incur model usage.

---

## 11. Current limits

- Fetch-host, language-server and manifest-plan approval requests are refused until
  their UI is implemented. Command, output, vouch and question approvals are implemented.
- Replies arrive whole in `turn.done`; output-token events report counts, not text.
- MCP configuration, subscription import and skills authoring have no dedicated UI.
- File browsing, previews and attachments are Electron IPC features, not RPC methods.
- One `bravebot-rpc` process has one client. There is no multi-client transport.

---

## 12. Verification contracts

The first six are the security properties, not nice-to-haves. Each should fail loudly if
someone later makes the obvious "simplification".

1. A closed stdin with a confirmation outstanding yields `Decision::Reject`.
2. Transport-loss coverage must distinguish stdin EOF from a stdout-only failure;
   the latter currently has the waiting-state limitation described in §8.4.
3. `session.close` during a pending confirmation rejects it, then joins the worker.
4. A replayed `confirm.reply` for a consumed `request` errors and performs no write.
5. `turn.cancel` wakes and refuses a pending confirmation; it never approves it.
6. `turn.send` before `trust.reply` on a fresh session is refused.
7. A malformed line is answered or logged and the process survives; the next valid
   request is served.
8. Every type in §6 round-trips, and an unrecognised enum tag deserialises to the
   less-trusted variant.
9. A session written by `bravebot-rpc` is listed and resumed by the TUI, and one written by
   the TUI is listed and opened by `bravebot-rpc`. This is the whole point of §1 and should be
   an integration test, not a claim.
10. `Outcome::reply` never reaches stdout — only `reply_for_display()`. Assert on the
    serialiser.
11. Nothing below `src/bin/` writes to stdout or exits the process (§2.2). A grep in CI
    is enough and is worth more than a convention nobody re-checks.
12. Refusing when nothing is pending queues nothing. A stale `Reject` sitting in the
    answer channel would be picked up by the *next* write, refusing it without anyone
    being asked — the failure mode is silent and looks like the model giving up.
13. The dispatch thread never learns a turn has ended by probing the answer channel.
    Sending anything down it to test whether it is still connected delivers a real
    decision to a real write; a completion flag is what it reads instead.

---

## 13. Historical implementation order and status

The following records the original Phase 0 milestone: **47 tests passing, upstream
clean at `1ba33f9`**. It is historical evidence, not the current dependency pin or
test count. See [testing](testing.md) for current validation commands.

A real turn has now run end to end through `bravebot-rpc` (`scripts/smoke-turn.sh`, in a shell
where direnv has loaded the agent's `.envrc`): five tool rounds, a gate refusal, two
isolated processors, a quarantined read the planner never saw, and a record on disk that
`bravebot --resume` lists beside the ones the terminal wrote. The confinement behaved as
designed with the directory declined — the file went to a slot, the planner worked from a
processor's summary, and `clean=false` recorded that a gate had refused something.

Two things the live run changed, neither of them visible from the design:

- **`output_tokens` is reported on a timer, not on a change.** 130 of the run's 168 events
  were token updates and 63 of those repeated a figure already sent. A terminal redrawing
  a counter does not care; a front-end across a pipe is woken for each one. Now coalesced
  (§8, `tokens`), which loses nothing because the figure is cumulative.
- **Narration arrives empty** when the model went straight from one tool call to the next.
  Dropped rather than forwarded, so an interface does not draw a row of blank messages.


0. `crates/bravebot-bridge` builds against the `vendor/bravebot` submodule and a test
   prints `bravebot_tui::BUILD`. This is the step that proves §2's zero-change claim, it takes
   an hour, and everything else assumes it. Do it first and stop if it fails.
1. `wire.rs`: the §6 projections and their round-trip tests. Pure functions, no I/O.
2. `bravebot-rpc` skeleton: envelope, dispatch loop, `agent.info`, `agent.ready`, error codes.
   No turns yet. Drive it by hand with `echo … | bravebot-rpc`.
3. `session.list` / `session.open` / `session.new` / `session.close`, read-only. Only the
   cross-project listing is new code; the rest wraps `bravebot_session::sessions`. **The Electron
   left column can be built against this alone.**
4. `BridgeReporter` + the `Sink`, then `send_turn` with a `Confirmer` hardwired to
   `RefuseWrites`. **The centre column works, read-only, at this point** — real turns,
   no writes possible, which is also the safest thing to demo.
5. `BridgeConfirmer`, `confirm.request` / `confirm.reply`, and tests 1–5. Do not let this
   step and the previous one merge: a `Confirmer` written alongside its first UI acquires
   a convenience default, and the default is always approve.
6. `turn.cancel`, `trust.request` / `trust.reply`, `doctor`.
7. Integration test 9, both directions.

Steps 3 and 4 each unblock a column of the UI, so Phase 1 can start once step 3 lands
rather than waiting for the whole of Phase 0.

---

## 14. Historical decisions and follow-ups

The original decisions below explain the implementation sequence. Later UI work
added grouping and bot lists; live audit streaming is now implemented (§8).

Settled:

- **Upstream is strictly read-only.** No PRs. `doctor` shells out (§7.3) and upstream
  drift is caught by our own tests (§12).
- **The library lives here**, as `crates/bravebot-bridge`, with `bravebot-rpc` as a thin transport
  over it (§2).
- **The left-hand column is one flat list across every project**, newest first, with the
  project name as the secondary line — so `session.list` with no `directory` is the call
  the interface actually makes, not a convenience.
- **Phase 1 is electron-vite + React + TypeScript.**
- **The agent is pinned, by submodule.** `vendor/bravebot` is a git submodule and the path
  dependencies point into it, so the revision compiled is the one this repository commits.
  Path-dependency-against-a-sibling was right while the two were being written together and
  wrong as soon as anybody else built this: an upstream field addition broke a checkout that
  had changed nothing. What the bridge depends on carries no compatibility promise, so the
  pin is load-bearing and moving it is a test pass rather than a version bump — which is
  exactly what a pin buys, a break that arrives in a pull request instead of in somebody's
  `npm run dev`.

Still open:

- **Tools missing from the agent's own describe table replay as bare "Tool".**
  `verb_for` and `target_key` in `crates/agent/src/tools.rs` do not know `ask_user`, so
  every stored call to it recounts as the word "Tool" with no target — visible in a real
  session on this machine, and identical in the TUI, so it is upstream behaviour rather
  than a projection bug. Read-only means we cannot fix it there. The interface should
  therefore draw an unrecognised `Said::Tool` line quietly rather than giving it the
  prominence a named call gets.
- **Should `audit` events stream live at all?** They are written at end-of-turn today.
  Streaming is nicer for the right-hand column but means the client holds a trail the
  disk does not have yet, and they will differ if the turn dies. Streaming plus a reload
  on `turn.done` is the suggestion; it is not free.


## 0.9 desktop extensions

- `vet.request` carries `request`, `origin`, `expects`, full `content`, `lines` and
  `vetting: { verdict, reason, detail }`. `vet.reply` carries the session, request and
  explicit decision. Its kind is distinct from output and path-vouch replies.
- `output.request` and `vouch.request` include `vetting`. `confirm.request` includes an
  optional `remark: { preview, lines, label }`. None is interpreted as an approval.
- `watches.list/add/stop` operate on an open session. Add names a project-relative file;
  stop names a number or `all: true`. Listings include remaining lifetime and state.
  Main-process `watches.poll` advances the upstream watch clock and starts eligible turns.
  `watch.fired` names the number/path; `watch.ended` names the number/reason. Both are
  session-scoped. Watch prompts do not enter typed-prompt recall or title a conversation.
- `settings.inspect` optionally takes a session and reports the linked agent's effective
  service configuration, file layers, managed keys and network transport, without credentials.
  `settings.select` is main-process-only and selects/clears a validated file for future turns.
  The native `bravebot:settings:select` picker grants the path; renderer requests cannot set it.
  Model discovery uses the same override and the session's registered project directory.
  `bravebot-rpc --settings <path>` restores the override on process restart.
- `doctor` no longer runs an external CLI. It returns `found: true`, `structured: true`
  and the linked agent's configuration report as formatted `text` for existing consumers.
- Open/fork responses and completion/error events include `contextTokens`, the last
  request's measured size (zero means unmeasured), separately from accumulated usage.
  The desktop category `model-unconfigured` identifies a selected Brave model without
  Brave configuration when another gateway or Bedrock service is configured; select
  a model from that service instead of replacing its credential.
  Error events include stable `category`, optional `status` and `attempts`; raw backend
  diagnostic text is not sent as a turn error.
- `bravebot:hooks:read/save` are main-process IPC endpoints, not arbitrary RPC methods.
  A save takes the edited JSON and the previously read text (null for an absent file).
