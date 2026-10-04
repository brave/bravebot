# `bravebot-ui-bridge`: the library and its protocol

This document began as the Phase 0 design. The request and event descriptions below
cover the current bridge; §13–§14 retain historical implementation notes. For exact
wire shapes, consult [`protocol.rs`](../../crates/ui-bridge/src/protocol.rs),
[`wire.rs`](../../crates/ui-bridge/src/wire.rs),
[`bridge.rs`](../../crates/ui-bridge/src/bridge.rs) and
[the UI types](../src/shared/protocol.ts). Setup instructions live in [setup.md](setup.md).

The Electron app does not drive a terminal and does not parse one. It talks to a Rust
**library**, `bravebot-ui-bridge`, which lives in this repository and depends on
`bravebot` as an ordinary Cargo dependency.

**`bravebot` was not modified while it was a separate repository.** That was a hard
constraint on the original design, and §2.3 says what would have violated it. It no longer
holds: the two repositories are one workspace, so widening a shared function is an ordinary
change and is the right answer whenever this side would otherwise re-decide something the
agent already decides. What has not changed is which side decides: the agent, always.

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
crates/ui-bridge/
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


`crates/ui-bridge/Cargo.toml` depends on the agent as a sibling member of one workspace, so
there is no pin to move and no revision to keep agreeing:

```toml
[dependencies]
bravebot-agent = { path = "../agent" }
bravebot-session = { path = "../session" }
bravebot-stamp = { path = "../stamp" }
bravebot-core  = { path = "../core" }
bravebot-config = { path = "../config" }
```

Every module the bridge needs is `pub`: `bravebot_session::{sessions, store, audit}`, every
module of `bravebot_agent`, and `bravebot_core::{event, label, todo, trust}`. A change to any
of them that breaks this crate now breaks the pull request that made it, which is the whole
reason the two repositories became one.

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
- **New approval types.** Command, fetch, language-server, plan, credential-exposure and MCP
  approvals are implemented. MCP server startup is shared with the terminal through
  `bravebot_agent::servers`, which takes its questions through an `Asker` the bridge supplies.

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
| `conversation::Said` | `{"kind":"user"\|"assistant"\|"tool","text":"…"}` with `"prompt":N` on `user` only and `"why":"…"` on `tool` only, `{"kind":"attached","path":"…"}`, `{"kind":"watch","number":N,"path":"…"}`, `{"kind":"consolidation"}` or `{"kind":"vetted","reference":"…","media":"…"}` | from `recounted()`, §7.1 |
| `core::event::Event` | as `audit::as_json` already produces, `refusal` included | **reuse verbatim**, do not re-derive |
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
   `Record::trust_map` already do on the Rust side. Never the other way. An audit record
   whose `refusal` cannot be read is drawn as a refusal for the same reason: a check whose
   answer nobody can read is not one to show as though it passed.
3. **A decision the agent has taken is sent, not reconstructed.** Whether a record is a
   refusal, and which of the user's messages a prompt is, are both facts the agent holds,
   so both travel beside what they describe. A client working one back out of the other
   fields has to be taught every shape the answer takes, and the shape it was not taught is
   the one that goes wrong quietly.

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
  "said": [ { "kind": "user", "text": "…", "prompt": 0 }, { "kind": "attached", "path": "…" } ],
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
- **A message composed rather than typed carries a tag and no text.** A file somebody named
  arrives as `{"kind":"attached","path":"…"}`, a watch that fired as
  `{"kind":"watch","number":N,"path":"…"}`, a prompt a front end composed on its own account
  as `{"kind":"consolidation"}`, and a picture or a PDF `vet_content` let through as
  `{"kind":"vetted","reference":"…","media":"…"}`, the reference it was held under and its media
  type. All four are user-role messages in the request, because that is how a file reaches a
  planner, and none is a prompt: the client writes its own row from the tag
  and whatever fields come with it. The text is withheld rather than merely unused. The words of
  an attached message are a line the agent wrote followed by **the file's own bytes**, so a client
  that read them back to decide what to draw would let whoever wrote that file choose which row it
  appears as, the interface's own rows included; and the words of a composed prompt are wording a
  front end chose, so a client matching on them would draw that row over any prompt anybody typed
  the same sentence into. Rule 1 of §6, applied where it matters most.
- `prompt` appears on a `user` entry and no other, and is which of the user's messages it is:
  the coordinate `session.fork` cuts on. **A client must not count this for itself.** A count
  over the rows a transcript calls prompts is a second copy of a rule this side already applies,
  and it agrees only for as long as nobody adds a kind of user message the client is not told
  about: a turn nudged for spending its tool budget adds one mid-turn and no event reports it.
  `session.fork` checks the prompt's text against the ordinal, so a count short by one is a
  refusal. A prompt a client has just sent and has no `Said` for yet is numbered by `turn.done`.
- `context` is `Snapshot::context`, the word for what the conversation has met.
- `todos` and `audit` are keyed by turn number as strings, because JSON object keys are
  strings; the Rust side is a `BTreeMap<usize, _>`.
- `trust.known` is `false` when `Record::trust` is `None` (a record written before the
  map was kept) and no kept answer (§9) settles the directory. **Nothing recorded is not the
  same as nothing trusted**, and the client must ask (§9) rather than assume an empty map.
  Where a kept answer settles it, `known` is `true` and `rules` are the rule a yes writes.
- `remembered` and `keeping` are what §9 says of a kept answer. Both are `null` on a record
  that kept its map, which answers first.
- `branchNote` / `buildNote` are the existing `sessions::branch_note` and
  `sessions::build_note` outputs: a sentence, or `null` when there is nothing to say.

`session.open` does **not** start a turn. If the record has no saved trust map and no kept
answer settles the directory, it emits `trust.request`; the client must answer before sending
a turn.

#### `session.new`

```json
{ "id": 3, "method": "session.new", "params": { "directory": "/Users/me/repos/thing" } }
```

Returns `{ "session": "s2", "directory": "…", "branch": "main", "remembered": null,
"keeping": "…" }`, the last two as §9 has them. The `Record` is not written until the first
turn, matching `Session::begin` + `save`, so an opened-and-abandoned window leaves nothing
behind.

Errors with `not_a_directory` if the path is not one. Where there is no state directory the
session still opens and its turns run, and nothing is written (SESSION-28).

#### `session.delete`

```json
{ "id": 6, "method": "session.delete",
  "params": { "directory": "…", "id": "…" } }
```

```json
{ "id": 6, "ok": { "deleted": true } }
```

Removes a stored session from disk: its record and the trail beside it, and nothing else
(SESSION-30). `directory` and `id` are the ones `session.list` gave. A fork is a complete copy and
is unaffected.

- `no_such_session` if no record of that id is stored for that directory.
- `bad_request` if `id` is not shaped like a session name, and if the session is open in this
  bridge and idle: close it with `session.close` first.
- `turn_in_flight` while a turn is running in it.
- `internal` if a file could not be removed. The trail goes first, so the session still opens.

#### `session.fork`

```json
{ "id": 5, "method": "session.fork",
  "params": { "session": "s1", "prompt": 2, "text": "make it handle quotes" } }
```

```json
{ "id": 5, "ok": {
  "session": "s7", "id": "1756300000-4711", "directory": "…", "branch": "main",
  "said": [ { "kind": "user", "text": "…", "prompt": 0 }, { "kind": "attached", "path": "…" } ],
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

- `prompt` is a **0-based ordinal over `Said::User`**, and not a turn number. It is the ordinal
  this side minted and sent, on the `Said` (§7.1) or on the `turn.done` that reported the
  prompt; a client echoes one back rather than arriving at one of its own. `text` is what that
  prompt said. Both are sent because they check each other: the ordinal says where to cut, and
  the text says the client is holding the conversation this cut is being asked of. They can
  disagree: a client working from a transcript the conversation has moved on from is the case.
  A mismatch is `bad_request`; a fork taken one prompt away from where somebody pointed is worse
  than one that did not happen.
- **A message composed rather than typed is not a prompt on either side.** It is tagged in the
  record and reported as `attached`, `watch`, `consolidation` or `vetted`, so neither the window nor
  the cut has to recognise one by its wording. That is what keeps the two counts aligned rather than
  approximately aligned: while an attachment was recognised by its first line, a file whose own
  first line read `Contents of x:` moved every later ordinal in the session, and one that did not
  read that way moved none.
- The ordinal is turned back into a message index by walking `archive ++ messages`, skipping
  anything tagged, and consuming the drawn prompts in order, matching on text. That is an
  **alignment, not a second copy of `recounted`'s rules**: those rules drop an untagged user
  message on what its text starts with, so a message whose text equals a prompt the transcript
  showed cannot have been one of the dropped ones. If a later build filters on something neither
  the tag nor the text carries, the walk runs out of matches and the fork is refused rather than
  cut somewhere else.
- **The cut is a well-formed request by construction.** It lands in front of a prompt, which is
  the same boundary compaction uses, so it cannot come between a call and its result; and
  `Conversation::with_system` answers any call left unanswered anyway. Nothing here re-implements
  tool-call pairing, and `crates/ui-bridge/src/fork.rs` pins both halves of that.
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
  itself still holding the question and no kept answer settles the directory, and then the
  fork emits `trust.request` exactly as a new session does. `remembered` and `keeping` are
  as §9 has them.
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

The optional `composed` parameter says the client composed this prompt itself rather than
a person typing it, and takes exactly one word: `"consolidation"`, a turn sent to ask a bot
to bring its memory up to date. Omitting it or passing `null` says a person typed it, which is
the ordinary case. It records the tag §7.1 reports, so a transcript drawn from the record can
tell such a turn from a prompt without reading either one's words, and such a prompt is not one
of the places `session.fork` may cut. Any other value, `"attached"` and `"watch"` included, is
`bad_request`: those are the agent's own account of what a turn did, and a client able to claim
one could have a transcript draw a file's row around a line a person typed.

Both lists are optional and both default to empty. They differ in where a path may point
and in nothing else: `files` is workspace-relative and read inside the project, while
`dropped` may name anything on the disk — upstream calls it the read that is not confined
to the workspace, because a dropped path came from a gesture rather than from anything a
model said. Both are trusted for the same reason, both are vouched for by being named
(`policy.vouch_for_named_path`), and both land in the conversation as user messages reading
`Contents of <path>: …`, which means **they accumulate**: a file attached to every turn is a
copy of that file per turn. Each is recorded with the tag §7.1 describes, so a transcript
drawn from the record names the file without reading the body back to find out what it is.

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

`run.reply`, `output.reply`, `vouch.reply`, `fetch.reply`, `server.reply`,
`manifest.reply`, `exposure.reply`, `mcp-server.reply`, `mcp-tools.reply`, `mcp-call.reply`,
`mcp-move.reply` and `ask.reply` all require `session` and `request`, and
must match the pending question's kind as well as its ID. `run.reply` accepts `decision` and `remember`; only an approval with literal
`remember: true` records a command grant. Output, vouch, fetch, server, manifest and
exposure replies accept `decision`. A fetch reply has no `remember`: an approval covers the one URL it was given
for. A server reply has none either: an approval lasts as long as the session does.
`mcp-server.reply` and `mcp-call.reply` accept `decision` and `remember`, read as a run's are:
`remember: true` with an approval is the third answer, every future server in this project for
the first and no more questions about this tool in this project for the second. `mcp-tools.reply`
and `mcp-move.reply` accept `decision`.
`ask.reply` accepts an `answers` array, whose entries contain `typed` text or `chosen`
indices; unreadable entries decline. Choices are fitted to the question before use.

#### Permissions

`permissions.list` takes `session` and returns `paths`, `commands` and `remembered`, the
kept answer about the session's directory as §9 has it, read when the list is asked for.
`permissions.revoke` takes `session` plus either `kind: "path"` and `path`,
`kind: "command"` and `command: { program, startedAs, args }`, or `kind: "remembered"`, which removes
every answer kept about the directory so the next session there is asked, and leaves this
session's map as it is. It returns the updated lists, and refuses `kind: "remembered"` with
`bad_request` where nothing is kept. These methods refuse with `turn_in_flight` while the
session is running. Revocation can reduce existing grants; it cannot add trust.

### 7.3 Trust and diagnostics

#### Connectors

The MCP servers a person declares, approves and turns on from the window (SERVERS-3). Each method
except `connectors.preview` returns what `connectors.list` returns.

- `connectors.list` takes nothing and returns `{ home, state, writable, unavailable, connectors }`.
  `home` is the person's home directory, which a `~/` in the window's catalog stands for. Each
  connector carries `alias`, `transport`, `command` or `url`, `variables` (`{ name, stored }`; a
  stored value is never sent), `reads`, `directory`, `digest`, `approved`, `requested` (in the home
  settings file), `connected` (both), `changed` and `refused`, or `alias` and `problem` for a
  declaration that cannot be used.
- `connectors.preview` takes a form, `{ alias, transport, url }` or `{ alias, transport, command,
  variables, directory }`, where a variable is `{ name, value }` to store a value, `{ name }` to
  read it from the environment at launch, or `{ name, keep: true }` to keep the value the existing
  declaration stores. It writes nothing, and returns the declaration drawn as above with
  `fingerprint` (the whole digest), `exists`, `same` and `fetching`.
- `connectors.connect` takes the same form and the `fingerprint` the person was shown, and
  `replace: true` to write over a different declaration of the same name. It declares, approves and
  requests the server in `~/.bravebot/settings.json`, and refuses with `bad_request` where the form
  no longer resolves to that fingerprint.
- `connectors.disconnect` takes `alias` and takes the request out of the home settings file,
  keeping the declaration and its approval. `connectors.remove` also deletes those.

#### `trust.reply`

```json
{ "id": 8, "method": "trust.reply",
  "params": { "session": "s1", "directory": "/Users/me/repos/thing", "trusted": true,
              "remember": false } }
```

Records the user's answer to the startup question into that session's `TrustStore`,
before the first `turn.send`. See §9. Returns `{ "trusted": true, "kept": null }`.

`remember` is optional, and `true` keeps the answer for later sessions in the directory. It is
refused with `bad_request`, and nothing is recorded, with `trusted: false` or where the
question offered no `keeping`; the offer is spent once the question is answered. `kept` is
`true` when the answer was written down, `false` when writing it failed (the answer is then a
yes for this session only, and the client says so, naming the file), and `null` when nothing
was asked to be kept.

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
| `turn.started` | `{ turn, mode }` | `turn.send` accepted |
| `phase` | `{ phase }` | `Reporter::phase` |
| `composing` | `{ call }`, a verb word or `null` | `Reporter::composing`, **a delegate's dropped** |
| `narration` | `{ text }` | `Reporter::narration`, **empty ones dropped** |
| `tool.started` | `Activity` | `Reporter::tool_started` |
| `tool.finished` | `Activity` | `Reporter::tool_finished` |
| `check.started` | `{ lines }` or `{ file }` | `Reporter::check_started` |
| `check.finished` | `{}` | `Reporter::check_finished` |
| `landed` | `{ landing }` | `Reporter::landed` |
| `quarantined` | `Shown` | `Reporter::quarantined` |
| `todos` | `{ rows: [ { content, status } ] }` | `Reporter::todos` |
| `tokens` | `{ written }` | `Reporter::output_tokens`, **only when the figure changes** |
| `audit` | `{ at, turn, event }` | the `Sink`, via `audit::as_json` |
| `confirm.request` | see §8.1 | `Confirmer::confirm_write` |
| `run.request` | `{ request, stages, directory, line, plan, writes, releasesPrivate, vouches, summary }` | command approval |
| `output.request` | `{ request, command, reference, lines, output, summary }` | admit command output |
| `vouch.request` | preview and label fields from `wire::vouch_request` | trust a quarantined path |
| `fetch.request` | `{ request, url, host, ambient, summary }` | fetch one URL; `host` is the agent's reading of `url` and is drawn as sent |
| `server.request` | `{ request, language, program, workspace, runsBuildTooling, summary }` | start a language server for the session |
| `manifest.request` | `{ request, task, steps }` | run a frozen plan; one line per step |
| `manifest.started` | `{ run }` | a manifest run began; `run` counts runs in this open session |
| `manifest.done` | `{ run, reply, model, steps, clean, tokens, outputTokens, notices, attempt, record, trust }` | a run finished |
| `manifest.error` | `{ run, kind, message, category, attempts, status, stopped, declined, problem, attempt, record, notices }` | a run stopped |
| `exposure.request` | `{ request, path, credentials, summary }` | let the planner read a file holding a credential |
| `mcp.starting` | `{ servers }` | a session's first turn is starting the MCP servers its settings request |
| `mcp-server.request` | `{ request, alias, transport, command, url, program, variables, reads, directory, digest, requestedBy, changed, fetching }` | use an MCP server (SERVERS-4) |
| `mcp.started` | `{ servers, confined, notes }` | what started, and a line for each request that did not |
| `mcp-tools.request` | `{ request, alias, tools, refused, changed, vetting }` | offer a server's tools to the model (SERVERS-8) |
| `mcp-call.request` | `{ request, alias, tool, name, arguments, description, mayStand }` | call a server's tool (SERVERS-7) |
| `mcp-move.request` | `{ request, alias, declared, destination, authority, mayRecord }` | a remote server's reply pointed elsewhere (SERVERS-11) |
| `ask.request` | `{ request, prompts }` | user questions |
| `trust.request` | `{ directory, keeping }` | initial project trust; `keeping` as §9 |
| `turn.done` | see §8.2 | `Ok(Outcome)` |
| `turn.error` | see §8.3 | `Err(TurnError)` |

`Activity` serialises as:

```json
{ "verb": "read", "target": "src/main.rs", "why": "see the entry point", "note": "412 lines",
  "failed": false, "untrusted": false, "waitedSeconds": null,
  "changes": [ { "kind": "added", "text": "…" } ] }
```

`why` is the planner's own line on what the call is for, the same on `tool.started` and
`tool.finished`, and empty where it gave none. It is released for the screen and read by
nothing else, so a client draws it beside the call or not at all.

`note: null` means the call is still running — that is what distinguishes an unfinished
line from one that finished with nothing to say, and the client must render the two
differently.

`waitedSeconds` is whole seconds this call spent at a model of its own, and `null` for a
call that asked none, which is nearly every call: only a confined check and a processor's
own round ask one. A client has no clock on a tool call, so without it a call that was slow
because a model was slow is indistinguishable from a slow program. A wait under a second
arrives as `0` and is not worth drawing.

`check.started` and `check.finished` bracket a confined check that runs *inside* a call
already reported, over `lines` of quarantined content, or over one `file` that is a
`"picture"` or a `"pdf"` and has no lines to count. The pair carries nothing else: not
a fragment of the content, not the verdict, not the checker's sentence. What it decided
reaches a person on the approval card and no model at all (`CHECK-9`). The end arrives
however the check ended, the backend failure that yields no verdict included, so a client
is never left saying a check is running because the backend was down. Drawn ahead of
`phase`, not instead of it: the round's phase does not change while a check runs, so it is
the one word here that is not what the session is waiting on, and it is what the client
goes back to saying once the check is over.

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
  "id": "0f1c…", "archived": 0, "prompt": 4
} }
```

`prompt` is where this turn's prompt landed among the things the user said, the same
coordinate `Said.prompt` carries and the one `session.fork` cuts on. It is here because a
client cannot count it: the turn adds a user message for every file named in the prompt,
and another if it spends its tool budget, and a transcript draws none of those as a prompt.
`null` where the conversation does not hold the prompt, which is a prompt that cannot be
forked rather than one to guess a place for.

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
  "category": "too-long",
  "cutOff": { "ceiling": 32000, "call": { "tool": "write_file" }, "thought": false },
  "notices": ["hook turn-finished: /usr/bin/fmt could not be started"],
  "prompt": 4, "id": "saved-session-id-or-null" } }
```

`prompt` is as on `turn.done`: a turn that failed still said what it was asked, so the
prompt is in the conversation and is still a place a fork can be cut at.

`cutOff` is set where a reply stopped at the output ceiling is what failed, and `null`
otherwise. That is `category: "too-long"`, or `"internal"` where the stop was in a plan's
step (`kind: "manifest"`). `ceiling` is the limit in tokens. `call` is the tool call the
reply was part way through, or `null` where it was in none, and its `tool` is the request's
own name for a tool it offered, `null` for a call to one it did not. `thought` says whether
any reasoning arrived. A client names the ceiling and which of these it was, since a reply
that spent the limit on one call's arguments is asked for in parts and one that spent it
thinking is not.

The four `TurnError` variants. A failed turn is still part of the conversation and the
conversation is handed back either way — the next question is usually about it — so the
client must keep the transcript, not discard it. `id` identifies the recoverable
record when the failed turn was saved; clients should migrate unsent draft
preferences to this ID just as they do for `turn.done`.

`notices` is what the turn said about itself as it ran, as §8.2 carries for a turn that
answered. A failed turn produces no outcome for those sentences to arrive on, and one of
them is a hook of the person's own that could not be started, so a client that drops this
field is a client on which nobody learns their formatter has stopped running.

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

**A remembered answer** is the one exception, and it is one the person made
([trust-map.md](../../docs/specs/trust-map.md#TRUST-23)). Where a yes was kept about exactly
this directory, a session the flow above would ask is not asked: it starts from the rule a yes
writes, emits no `trust.request`, reports `trust.known: true` where its response has `trust`,
and its response carries `remembered: { at, path }`, when
the answer was given (seconds since the epoch) and the file it is kept in, for the client to
say at the top of the conversation. Otherwise `remembered` is `null`. The record is the one
the terminal keeps, so an answer kept in either front end settles the other.

Where the question is put and its answer may be kept, the response and the `trust.request`
carry `keeping`, the file a `trust.reply` with `remember: true` would write; the client names
it on the question. `keeping` is `null` where the answer may not be kept: no state directory,
a filesystem root, the home directory or one holding it, or a filesystem that cannot say when
the directory was made.

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

- Command, output, vouch, fetch, language-server, plan, credential-exposure, MCP and question
  approvals are implemented. A plan-first run is offered no MCP server's tools.
- A manifest run's audit events carry `run` and no `turn`. The window does not show them yet.
- What a manifest run released for a screen is not saved, so a run read back does not show it.
- Replies arrive whole in `turn.done`; output-token events report counts, not text.
- MCP configuration (declaring, approving ahead of time, enabling and forgetting), subscription
  import and skills authoring have no dedicated UI. `bravebot mcp` in a terminal does the first.
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


0. `crates/ui-bridge` builds against the agent crates and a test prints
   `bravebot_stamp::BUILD`. This is the step that proves §2's zero-change claim, it takes
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
- **The library lives here**, as `crates/ui-bridge`, with `bravebot-rpc` as a thin transport
  over it (§2).
- **The left-hand column is one flat list across every project**, newest first, with the
  project name as the secondary line — so `session.list` with no `directory` is the call
  the interface actually makes, not a convenience.
- **Phase 1 is electron-vite + React + TypeScript.**
- **The agent is a sibling crate, in one repository.** This went through all three
  arrangements. A path dependency on a sibling checkout was right while the two were written
  together and wrong as soon as anybody else built this, because an upstream field addition
  broke a checkout that had changed nothing. A submodule pin fixed that by deferring the
  break to whoever next moved the pin, which is the one place furthest from the change that
  caused it. Folding the front end in gets what both were reaching for: what the bridge
  depends on still carries no compatibility promise, and it no longer needs one, because a
  change that breaks this crate now fails the pull request that made it.

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

- `vet.request` carries `request`, `origin`, `expects`, full `content`, `lines`, `picture` and
  `vetting: { verdict, reason, detail }`. `picture` is `null` for text. For a picture or a PDF it
  is `{ path, media, bytes }`, a copy of the file for the person to open, deleted once the request
  is answered, and `content` is empty. `vet.reply` carries the session, request and
  explicit decision. Its kind is distinct from output and path-vouch replies.
- `output.request` and `vouch.request` include `vetting`. `confirm.request` includes an
  optional `remark: { preview, lines, label }`. None is interpreted as an approval.
- `watches.list/add/stop` operate on an open session. Add names a project-relative file;
  stop names a number or `all: true`. Listings include remaining lifetime and state.
  Main-process `watches.poll` advances the upstream watch clock and starts eligible turns.
  `watch.fired` names the number/path; `watch.ended` names the number/reason. Both are
  session-scoped. Watch prompts do not enter typed-prompt recall or title a conversation.
- `settings.inspect` optionally takes a session and reports the linked agent's effective
  service configuration, file layers, managed keys, the `model` and `provider` keys a project file
  named and the agent ignored (`ignored`), the limits in force (`limits`), and network transport,
  without credentials. `limits` holds `readsStayInWorkspace` and `bypassUnreachable`, each
  `{ value, path, managed }` (`value` is null where no file named the key, `path` the weakest file
  that did, `managed` true where the administrator's file did), `unreadable` (keys a file spelled as
  something other than a boolean), and `run.{defaultSeconds, maxSeconds, maxOutput}`, each null
  where nothing set it so the agent's built-in figure applies. The window displays these and
  writes none of them.
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
  Error events include stable `category`, optional `status` and `attempts`, and `cutOff` for a
  turn the output ceiling ended; raw backend
  diagnostic text is not sent as a turn error.
- `hooks.inspect` reports the hooks file as the agent reads it: `path`, its `text` (null for a
  file this could not read at all), the `hooks` it declares as `{ on, tool, run, timeout, firesForNothing }`,
  `timeoutSeconds` as `{ min, max }`, the whole numbers of seconds the reader accepts for an
  entry's `timeout` (an entry with any other `timeout` is dropped), and `entire`, false where the
  reader passed over part of the file, so that composing it back
  from `hooks` alone would drop what it did not read. An editor that offers a form refuses one for
  such a file. `no_home` where the platform names no state directory. Nothing else parses the
  file: which entries this build can use, and which name a tool on a moment that has none, are the
  same answers a turn fires hooks from.
- `bravebot:hooks:save` is a main-process IPC endpoint, not an arbitrary RPC method. It takes the
  edited JSON and the text `hooks.inspect` last reported (null for an absent file), and writes
  through a descriptor-pinned atomic replace that refuses a symlink and stale text. There is no
  read endpoint: a renderer reads the file through `hooks.inspect`.
- `fetch.request` carries `request`, `url`, `host`, `ambient` and `summary`. `host` is taken
  from the URL by the agent's parser and a front end draws it as sent, on a line of its own,
  because a URL can be written to read as another host. `ambient` has the shape a run's has
  and is empty for every host but a machine's metadata service. Nothing of a reply is sent:
  the question is put before the request goes out. `fetch.reply` carries the session, request
  and explicit decision, and its kind is distinct from every other reply's. An approval is
  consent to that one request, trusts nothing that comes back, and is not remembered.
- A question answered with a yes or a no is added in four places and no others: a `Kind` and
  a `Reply` in `turn.rs`, whose `Kind::refusal` does not build until the new kind has one; a
  projection in `wire.rs`; a row in `Bridge::dispatch`; and, in the window, a row in `REPLY`
  in `src/renderer/transcript.ts` with its entry, its card and its line in the main process's
  allow-list, which `scripts/fetch-card.test.mjs` holds to each other.
- `server.request` carries `request`, `language`, `program`, `workspace`, `runsBuildTooling`
  and `summary`. `program` is the absolute path the server's name resolved to. The name comes
  from a table in the agent, so nothing a turn read chooses what runs. `runsBuildTooling` is
  sent as the fact, and a front end says what it means: code from the dependency tree runs
  with the person's own access. `server.reply` carries the session, request and explicit
  decision, and its kind is distinct from every other reply's.
- The servers a session started are held by the session in the bridge, taken by each turn and
  put back after it, so a language approved on one message is not asked about on the next and
  its index is built once. They are never written to a record: a reopened or forked session
  starts with none and asks. Closing the session stops them, and so does the process ending.
- `manifest.run` takes `session`, `task` and an optional `model`, and answers `{ run }` once the
  run has begun. It is refused with `bad_request` for an empty task, for any `files`, `dropped`
  or `attachments`, and before `trust.reply`. It is refused with `turn_in_flight` while a turn
  or another run is in flight. `turn.cancel` stops a run.
- A run is not a turn. The conversation is not sent to the planner and nothing is added to it,
  the session's turn count does not change, and the session's record is not written. The run
  is saved as its own record by `bravebot_session::sessions::record_manifest_run`, and
  `record` names it. A run the person stopped is not saved, and `record` is null.
- `manifest.request` carries `request`, `task` and `steps`. `manifest.reply` carries the
  session, request and explicit decision. An approval covers that plan and does not approve its
  writes, which arrive as `confirm.request` when their steps are reached.
- `manifest.error` reports the cause of the failure in `kind` and `category`. `declined` says
  the plan was put to the person and not approved. `stopped` says the person stopped the run.
  `problem` is the agent's own sentence, and is null for a model service failure.
  `attempt` holds `goal`, `proposed`, `plan` and `steps`, whatever the run got as far as making.
- `manifest.done` carries `reply`, which is what the plan's last step released for a screen.
  It can be the text of a file nobody vouched for. A front end draws it as plain text in a
  marked container, and does not add it to the conversation.
- Every row of `session.list` carries `manifest`, which is true for a manifest run's record.
  A run has no conversation, so `session.open` refuses one with `bad_request` and makes no
  session. The terminal's picker refuses one in the same way.
- `manifest.read` takes `directory` and `id` and returns `{ record, model, manifest }`. It
  opens no session, so there is nothing to close afterwards. `manifest` holds `goal`,
  `proposed`, `plan` and `steps`, as `attempt` does, and `failure`, which is the agent's
  sentence about why the run stopped and is null for a run that finished. It is refused with
  `bad_request` for a session's record, and with `no_such_session` for an id that names none.
- `exposure.request` carries `request`, `path`, `credentials` and `summary`. `credentials` has
  one line per finding, as the agent wrote it: the kind, where it is, and a mask of the value.
  No part of a value and no text of the file is sent. `exposure.reply` carries the session,
  request and explicit decision, and its kind is distinct from every other reply's.
- An approval covers the file for the session, as the agent keeps it. It is not written to the
  record, so a reopened or new session asks again.
- A session's first turn starts the MCP servers its settings request, after the trust question,
  and sends `mcp.starting` and then `mcp.started`. The questions it puts while it does are
  `mcp-server.request` and `mcp-move.request`, through the turn like any other. The servers are
  held until the session closes and are not written to the record, so a reopened or forked
  session starts its own. A turn stopped while they start starts none, and the next turn asks
  again. `session.new`, `session.open` and `session.fork` no longer carry `serversNote`.
- `mcp-server.request` carries the declaration as fields. `command` is the argv of a local
  server and `url` the address of a remote one; `program` is where a local server's program
  resolved, sent only where that is not its first word. `variables` holds `{ name, stored }`,
  and a stored value is never sent. `fetching` is the agent's lines about a runner that fetches
  what it runs. `digest` is what an approval binds to.
- `mcp-tools.request` carries each tool as the client drew it: `name` (`alias:tool`),
  `arguments` (one line each) and `description`, which is the server's own text. `refused`
  counts the tools that could not be offered. `vetting` is shaped as on `vet.request`.
- `mcp-call.request` carries `arguments` as `{ name, value }` with the value as JSON, and
  `mayStand`, which says whether the third answer can be recorded. A `remember: true` sent
  where it is false is read as a yes to the one call.
- `session.new`, `session.open`, `session.fork` and `permissions.list` carry `settingsRules`:
  `{ deny, ask, allow, unreadable, proposed, directories }`. The first three are the rules in
  force, as the files spelled them. `unreadable` holds `{ rule, said }` for each entry that is
  not a rule, where `said` is the agent's sentence about why. `proposed` holds `{ rule, file }`
  for each `allow` rule a checkout wrote, which is not in force. `directories` holds the names
  in `additionalDirectories`, none of which is opened.
- The rules are read when a session opens and kept for its turns. A fork takes its parent's.
  A settings file edited while a session is open governs the next one.
- A manifest run is passed the session's rules, and the agent's runner does not read them.
- `session.mode` takes `session` and `mode`, one of `ask`, `acceptEdits` or `plan`, and answers
  `{ permissionMode }`. Any other word, `bypass` included, is `bad_request`: bypassing is reached
  only through the command-line flag (MODE-5). It is accepted while a turn runs and changes the
  next turn, not the running one (MODE-8).
- `session.new`, `session.open` and `session.fork` carry `permissionMode`, which is always `ask`.
  A mode is not written to the record and a fork does not take its parent's (MODE-10).
- `turn.started` carries `mode`, the mode the turn reads once and keeps to its end.

## Shared session view, version 1

The optional `capabilities.sessionView` in `agent.info` and `agent.ready` advertises a shared Rust
view for fresh sessions. Require version 1 before using it; an absent capability is unsupported.
After `session.new`, call `session.view.start` with `{ "session": "s1", "version": 1 }` before
sending any turn. Its initial event precedes the response. Repeating the call, subscribing after
a turn, or subscribing to saved/forked history fails. Manifest runs are unavailable on a viewed
session. Clients that do not subscribe keep their existing stream.

`session.view.initial` and `session.view.update` use the Rust `view::Update` shape:

```json
{
  "event": "session.view.update",
  "session": "s1",
  "data": {
    "sequence": 1,
    "turn": 0,
    "status": "idle",
    "pending": null,
    "rows": []
  }
}
```

The initial sequence is 0. Apply updates consecutively, replacing each listed row by its `id` and
replacing all status fields. Rows have `id`, `turn`, `kind`, `event`, `data`, and `resolved`.
Kinds are `prompt`, `narration`, `quarantined`, `approval`, `reply`, `error`, and `activity`.
`event` names the original event; it is null for accepted prompts, whose `data` uses the saved
transcript's user/composed shape. Other row payloads preserve the original event's full `data`.
A resolved approval replacement retains its event name and payload.

`pending` holds `row`, `request`, `kind`, `supported`, and `data`. Supported kinds are `confirm`,
`run`, `fetch`, and `ask`; use their existing reply operations. Unsupported kinds require a capable
local surface or cancellation. Startup trust uses the existing local operation. The view never
authorizes an action, and does not strengthen legacy request targeting.

Status is `awaiting_trust`, `idle`, `running`, `waiting`, `completed`, `failed`, `cancelled`, or
`detached`. Session close emits `detached`, which means the view ended, not that the worker stopped
or saving succeeded. A gap or lost connection ends the view: version 1 has no reconnect or history
recovery. IDs are scoped to the current connection lifetime and session, with request IDs also
qualified by turn. Keep drafts and optimistic UI separate until an accepted prompt row arrives.

The [shared session view spec](../../docs/specs/session-view.md) defines the supported rows, ordering,
approval transitions, label preservation and limits. This is the Rust part of the first mobile
block; no TypeScript client or native renderer is supplied yet.
