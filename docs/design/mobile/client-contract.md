# Client interface and prototype contract

Status: the first Rust session-view block and the stdio TypeScript client's session lifecycle are implemented. Approval replies, the local client program and the remaining stage 1–2a evidence are outstanding, so stages 1–2a are incomplete. Later mobile stages remain proposed. See [current local scope](client-contract.md#implemented-typescript-client).

This is a high-level starting plan, not an exhaustive account of edge cases or behavior. Expect implementation discoveries to change or add to it. Update the affected design, specs, and tests as those decisions are made; resolve security gaps before enabling the affected feature. See the [executive summary](executive-summary.md) for the full proposal in one document.

## Code location and reuse

Create a standalone TypeScript package at `packages/agent-client/` with its own build and test commands. Its common module contains wire types, a thin session interface, and client-local UI state without Node, DOM, Electron, or JNI dependencies. Shared session behavior and presentation transforms live in Rust, as required by [UI-006](../../best-practices/ui.md#UI-006). Put the Node/stdin/stdout adapter in a separate submodule and a small local test program under `packages/agent-client/scripts/`. Building and testing this package must not require the Electron app or its build configuration.

Review `ui/src/main/bridge.ts` for reusable framing/correlation code and tests. Reuse suitable logic without importing Electron modules or requiring desktop migration during the local client proof. Electron binary discovery and app lifecycle remain in the desktop app. Desktop adoption of the shared client belongs to the later handoff stage.

Use a small language-independent scenario fixture directory, `packages/agent-client/test-fixtures/`, for typed operations, serialized boundary payloads, expected responses/events, labels, and symbolic IDs. Thin runners bind fixtures to the TypeScript client, real stdio process, local socket, and the actual platform native module. Run native scenarios on each supported platform; success on one does not establish support on the other. Framing cases apply only to stream transports. Use explicit synchronization steps rather than timing-dependent transcripts.

The React Native app imports the package’s dependency-free common module through the app shell and package-consumption setup owned by stage 3d. Stage 2b reuses that setup; if embedded work starts first, it may bring forward this shared setup without creating a second shell or making stage 3d depend on the embedded runtime. Keep package setup small; repository-wide workspace tooling is not a prerequisite for the local proof. Reuse and extend the Rust crates that own each behavior. Extract a small presentation crate if shared display transforms need a separate home under the layering rules; add its spec entry and tests with its first caller. The client package does not require a root npm workspace.

## Shared screen state

The execution owner computes session view state in one shared Rust implementation: transcript rows, turn status, pending approvals, and, when supported, controller status. The stdio process, embedded runtime, and persistent host call the same code. Start with the state needed for the local proof, then extend it at the named milestones. Put session transitions beside the bridge and carry content opaquely. Keep transforms that inspect released display content in the separate [Rust presentation layer](architecture.md#rust-sharing-boundary), outside bridge dispatch, `bravebot-core`, and `bravebot-agent`. Display results never decide execution or become planner input.

Expose that view through an additive, negotiated bridge capability. Existing callers retain their current protocol. The new client requires the capability and refuses an older runtime that lacks it; it does not reconstruct the view from legacy events. The local proof therefore includes a small Rust bridge addition as well as the TypeScript adapter. It preserves legacy behavior but is not a claim that the unmodified RPC already provides the new view.

An initial view supplies the current state before updates begin; stage 4a adds bounded history snapshots for recovery at one sequence boundary. Subsequent updates carry replacement rows and authoritative status fields with stable IDs and sequence numbers. TypeScript applies those replacements, detects sequence gaps, and requests resynchronization once supported. It does not infer session state from content or implement a second domain reducer. Before stage 4a, a lost connection ends the transcript view without claiming history recovery; stage 3's control snapshot can still restore current control state. Drafts, selection, focus, scrolling, and connection indicators stay client-local; an optimistic send stays separate until Rust reports acceptance with its message ID.

Rust also supplies reusable approval field descriptions and presentation-only label/control-character transforms. Native components draw the supplied segments and trusted markings and still need tests for legibility, accessibility, and complete approval details. Every supported approval field must be displayed or explicitly excluded with a reason. Shared fixtures verify serialization and bindings against the single Rust implementation, rather than keep two implementations of its rules in step.

| Responsibility | Home and access |
|---|---|
| Policy, tools, settings, credentials, session records | Existing Rust crates, extended through typed bridge operations when needed; no TypeScript reimplementation. |
| Session view state and approval semantics | Shared Rust code called by stdio, embedded, and host execution owners; serialized view updates to clients. |
| Released-content display transforms | Shared Rust presentation code; transport preserves labels and components draw markings that content cannot forge. |
| Framing, request correlation, view-update application | Thin TypeScript adapters; wire types generated from or checked against the Rust schema. |
| Layout, navigation, focus, local drafts, platform lifecycle | React Native or Electron UI; narrow native bindings for OS services. |

Remote-only clients need no local Rust agent or Rust state engine: the host supplies the same view that the embedded binding returns in-process. This avoids adding WebAssembly or a second native Rust runtime merely to share the reducer. Full snapshots on every event are unnecessary; stage 4a tests bounded row updates and snapshot replacement under load.

Stage 1 covers supported stdio behavior and one unsupported-operation case. Add listener allowlist scenarios with stage 3b. Give the package one build/type/test command and wire it into the relevant CI and local checks when the package is added.

## Implemented Rust block

`agent.info` and `agent.ready` advertise `capabilities.sessionView` version 1. A client checks
availability, creates a fresh session, and calls `session.view.start` with that version before
its first turn. It receives an initial event and ordered typed updates. Existing callers remain
on their legacy protocol unless they opt in. The [RPC contract](../../../ui/docs/phase-0-rpc-protocol.md#shared-session-view-version-1)
and [normative spec](../../specs/session-view.md) give the exact shapes and limits.

`view.rs` beside the bridge owns row IDs, sequence numbers, status and pending approval metadata.
It copies labelled payloads whole. Accepted prompts reuse `wire::submitted` and saved-history tag
rules. Answers reuse `Running` and `BridgeConfirmer`; no second approval reducer exists. There is
no content-reading display transform or new crate. Rendering helpers and native marking checks
remain for the first rendering caller, as Q2 records.

The process tests in `crates/ui-bridge/tests/fetch.rs` exercise accepted/rejected sends, approval,
denial, cancellation, two interleaved sessions, labels and planner exclusion. The existing process
suite covers malformed input, EOF and refusal. View unit tests cover unsupported approval kinds,
opaque replacement payloads and failure status; an emitter barrier test covers completion order.
These tests do not establish TypeScript adapter behavior or native rendering.

This block supplies no saved-history import, late subscription, reconnect, controller state,
send deduplication, persistent listener, networking or embedded bindings. Session identity lasts
for the connection. Closing detaches the view without claiming worker termination or save
success. Legacy reply targets are unchanged; stage 3a still supplies stronger stale-action
protection.

## Implemented TypeScript client

`packages/agent-client` is a standalone package with its own lockfile, build and tests; run it with
`make check-agent-client`, which builds `bravebot-rpc` first. `src/common` holds the wire types,
framing, request correlation, view application and the typed `AgentClient`/`AgentSession`
interfaces, with no Node, DOM, Electron or JNI dependency; `tsconfig.common.json` type-checks it
with no ambient types. `src/node` holds the child-process adapter. Language-independent scenarios
are in `test-fixtures/`.

**Reused from Rust.** The session view, its sequence numbers, status and pending metadata, and
`wire::submitted` are called through the process; the client applies what they send. A Rust test
writes `wire-contract.json` from the `view.rs` types and fails if it differs, and the package tests
compare their enumerations and decoders with it, so the TypeScript types are checked against Rust
rather than kept in step by hand. Framing and correlation follow `ui/src/main/bridge.ts`; the
desktop does not import the package, and no desktop caller changed.

**Kept in the client.** Framing, request ids, buffering of events that precede the response naming
their session, the in-memory copy of rows and status, and sequence-gap detection. The client has no
second session reducer: it does not derive busy state, approval state or turn outcomes. It reads no
released content.

**Operations in this increment.** The client can:

- describe the target, with the state-directory path dropped;
- list the configured workspaces by id and name;
- create a fresh session in one of them with the view started (`createSession` never accepts a path);
- answer startup trust when asked to, after which the question is no longer offered;
- send, cancel, and close.

`raw` on the stdio connection (and on `RpcAgentClient`) sends any bridge method for diagnostics and tests. The connection's `client` is typed as `AgentClient`, which has no `raw`, and `raw` is not confined to configured workspaces.

`attach`, `takeControl` and `messageStatus` fail locally. A question a turn asks appears in the
view and can be cancelled, which refuses it. Answering it is not yet supported.

A runtime without version 1 of the capability is refused before any session is created. A close
reports the view as detached, and worker termination and save success as unknown. A failed close
keeps the session subscribed to view updates and connection loss, unless the bridge answers that the session no longer exists.

A request unanswered past its deadline (30 seconds by default) ends the connection and stops the
child, since its outcome cannot be known. Nothing is retried. The cleanup request sent after a
failed session startup has no deadline, so a silent bridge cannot end the connection. Stdout EOF,
a read error, or a failed write to the child also ends the connection immediately, even if the
child is still alive.

A view listener, event handler, response handler, diagnostic hook or close callback that throws or
rejects is reported through the diagnostic hook where one is set. It does not affect other
listeners, other sessions, the rest of the data being read, the request it belongs to, or
shutdown. A diagnostic hook that throws does not lose the message being read.

A startup that breaks the protocol (a malformed or out-of-order initial view, a repeated row id)
refuses the session for good, even if a valid initial view follows. Startup trust questions are
held only while a session is being created, and at most 64 sessions' worth are held, earliest first.

**Evidence.** The scenarios run under four read-chunkings against a scripted server: split and
combined frames, early events, interleaved sessions, out-of-order responses, gaps, malformed
input, labelled rows carried whole, connection loss, an unknown method and a question of an
unsupported kind. The real-process tests use a model service of the test's own, an empty home and
a scratch project. They show:

- trust deciding whether a write is asked about;
- two sessions with turns in flight together keeping their own rows;
- a cancelled write not landing, and its late approval refused;
- close, EOF, a killed process and unreadable input.

**Not established.** Answering questions, a local program, real-process evidence for labelled
released content, native rendering, remote security, reconnect and recovery, send deduplication,
controller ownership, a stronger stale-action guarantee than the bridge's, a persistent host, and
operating systems other than macOS.

## Common interface

Use a typed `AgentClient` interface for sessions, turns, progress, supported approvals, and cancellation. Target setup is separate: embedded-runtime initialization is not host pairing, and neither requires the common interface to expose sockets. An adapter converts and validates requests; the selected execution owner enforces authority.

Every session, draft, message, and pending action is qualified by target and runtime instance. A remote host restart or embedded-runtime restart creates a new instance. Capabilities describe supported tools and lifecycle behavior. Unsupported calls return a defined failure; never fall back to another execution target.

| Operation | Meaning | Existing mapping or required work |
|---|---|---|
| Describe target | Sanitized identity/version, supported operations, readiness. | Project `agent.info`/`agent.ready`; strip the state-directory path returned by `agent.info` and other raw host-home paths. |
| List workspaces | Only workspaces authorized for this client. | Host-owned project-ID table; no raw client filesystem paths. |
| Create session | Fresh session in an approved workspace, not an implicit trust answer. | `session.new`; map workspace ID to a configured path. No automatic retry. |
| Attach/view | Current control state and subscription for an authorized live session. | New host registry and snapshot operation; never `session.open` on a live record. |
| Take control | Explicit authorized handoff using expected controller epoch. | New atomic host transition; old-epoch actions fail. |
| Send | Text plus client message ID; acknowledge acceptance separately from completion. | `turn.send`; add narrow send deduplication below. Busy returns no queued turn; record that rejection. |
| Answer | Typed reply to the exact session/turn/question under current control. | Existing reply kinds plus new bridge target checks; phone subset below. |
| Cancel | Apply cancellation to an exact active turn. | Add expected-turn validation to existing session-only `turn.cancel`. |
| Close | Enter `closing`; reach `closed` only after writer shutdown. | Existing `session.close` acknowledges before the worker necessarily finishes; add completion evidence. |
| Detach | End this client's subscription/control availability. | Does not close a persistent host session. Stdio EOF retains its current process-ending semantics. |

Local host trust controls and embedded trust remain explicit typed capabilities. Remote workspace trust is unavailable in the first prototype. The person answers through a capable host-local controller before the session can run. Before Electron attachment, the stage-5a local test client provides this typed startup-trust interaction over the U1-protected control channel. The host grants that capability based on authenticated local access; a phone cannot obtain it by advertising another answerable kind. Creating a session from the phone can therefore produce `awaiting host setup` rather than immediately executing. The demo may use a workspace explicitly prepared on the host; project pairing alone never answers trust.

## First non-stdio listener allowlist

Local OS-user identity is not sufficient proof that a person controls a client: an approved unconfined command can run as the same user. Resolve the [local control boundary](architecture.md#local-control-boundary) before exposing live-session control. The initial local proof does not establish protection against same-user programs.

Define and test the application allowlist before exposing the local listener; add remote field/kind restrictions before enabling remote clients. Do not reuse the broader Electron or Android prototype lists wholesale. The Android work at the [pinned revision](README.md#project-findings) provides input restrictions and Node source-consistency checks to build on; these do not test JNI or Kotlin runtime behavior. Assess their fit for the narrower remote API and add runtime integration coverage. The listener checks the connection role, authorized workspace/session, control epoch, and exact action target before dispatch.

The initial remote operations are target description, authorized workspace/live-session listing, create, attach, detach, take control, send, supported replies, cancel, close, and message-status lookup. Their names are application operations, not arbitrary bridge method strings. Listing is scoped to explicit authorized project IDs; raw `session.list` across all directories is unavailable. Saved open, forks, watches, manifests, settings, hooks, permissions editing, diagnostics, file operations, exports, and raw bridge dispatch are denied.

| Phone input | Allowed fields/behavior |
|---|---|
| Create | Host-issued workspace ID; no directory or settings path. |
| Send | Exact session/control target, message ID, and text. Use the host-selected model initially. Reject `files`, `dropped`, `attachments`, `composed`, `recall`, and arbitrary model/configuration overrides. |
| Write decision (`confirm.reply`) | Exact approval target and approve/reject, within the authorized effect scope. |
| Command decision (`run.reply`) | Exact target and approve-once/reject; host composes `remember: false`. Reject a supplied `remember` field, including `true`. Show the full command and affected scope. |
| Fetch decision (`fetch.reply`) | Exact target and approve-once/reject. Show the parsed destination host separately. |
| User question (`ask.reply`) | Exact question target and answers validated against the offered question schema. |
| Cancel/close/takeover | Only their typed identity and expected-state fields. Read-only grants cannot invoke them. |

Require host handling for workspace trust, vouching, command-output admission, vetted-content promotion, credential-exposure approval, language-server startup, and other unlisted reply kinds. Some have session-lived authority even without `remember`; do not classify all replies as one-use. The phone advertises only the kinds it can actually render and answer. Host-only handling requires a capable host-local controller under the [same takeover rules](architecture.md#one-controller-and-explicit-takeover); the initial local test client supports startup trust among those host-only kinds. The phone cannot widen this set through capability advertisement. When nobody capable is ready, refusal applies; startup trust remains unanswered. Adding another phone kind is a small later contract/test change.

Reject unknown request fields. Unknown additive event fields may be ignored, but unknown labels/kinds must degrade safely and unsupported approval kinds cannot be answered. Keep content labels and paths needed for informed consent; sanitizing target metadata must not hide an approval's actual destination.

## Send identity and uncertain outcomes

Protect `send` with a client-generated message ID scoped to target, host instance, and live session. Before sending, the client saves that ID, original text, and target in a small protected pending-message record. This is not a generic mutation journal. Keep credentials separate; delete settled entries according to a defined retention rule and clear sensitive local pending data when revocation is learned, as best effort.

The host records the normalized submission before bridge dispatch. States distinguish dispatching, accepted with turn identity, rejected, terminal outcome, and unknown. A duplicate ID with the same content returns the recorded state without dispatch; different content under the same ID is rejected. A send acknowledgement means the bridge accepted the turn, not that an effect completed or its history was saved.

Message lookup is explicit and independent of truncated display history. Retain send outcomes for the live session's host-memory lifetime. Set a finite submission and metadata budget; at the limit reject new sends before dispatch rather than evicting IDs that may be retried. No retry epochs or expiry cache is needed. Revalidate the caller's current access when returning status; deduplication never bypasses authorization.

After host restart or loss of the live session, lookup returns unknown/stale, never “not sent” based solely on an absent record. Do not automatically resend to a new instance. After phone process recreation with the host still alive, query the persisted ID; the controller may resend the same payload/ID under current control only if the original instance/session remains valid. The host then deduplicates or records it once. If old-controller input was delayed across takeover, its old epoch still fails.

After a definite busy/rejected send, a later explicit Send action uses a new message ID; a retry of the rejected ID returns its original rejection. Keep the draft visible so the user can make that decision.

Other mutations are not automatically retried. Recover current state after a lost acknowledgement; if it cannot establish the outcome, show uncertainty and require an explicit new action. Create may leave an extra idle session; do not silently create another. Exact-target cancel/close and single-use replies remain necessary even without generic deduplication. An approval acknowledgement means its answer was consumed, not that its effect succeeded.

## Capabilities by milestone

| Milestone | Passing establishes | Does not establish |
|---|---|---|
| 1–2a Local stdio client | The new typed client consumes the shared Rust view through a real process; legacy framing/correlation behavior is preserved. | Reconnect, takeover, retry recovery, or remote access. |
| 2b Embedded device proof | Common session semantics cross the React Native/native/Rust boundary for selected capabilities. | Background survival, full host tools, offline inference, or support on an untested platform. |
| 3a–3c Persistent local host | Exact targets, isolated ownership, safe close, minimum control state, and loss handling. | Networking, full transcript history, or phone recovery. |
| 4a–4b Bounded recovery | Retained display state and send-ID lookup survive connection loss while the host lives. | Crash-safe acceptance, generic mutation retries, shared queues. |
| 5a–5b Control handoff | One controller and viewers share a live session; stale-controller actions fail. | Concurrent controllers or ordinary saved-session import. |
| 6–8 Network and phone | The enabled contract works with authentication, scope checks, and tested device lifecycle limits. | Unattended approval while the phone is locked. |

## Evidence required

Stages 1–2a exercise the shared Rust view through interleaved events and the TypeScript adapter; stage 4a adds snapshot-boundary fixtures. Stage 2 relies on existing bridge tests for agent behavior and adds evidence through the new client. Inventory `protocol.rs`, `refusal.rs`, `dispatch.rs`, `remembered_trust.rs`, `rules.rs`, and `fetch.rs` before writing overlapping tests. None has been run for this proposal; passing the existing suite alone does not prove the new client.

Use a controllable byte/frame proxy to delay, split, combine, or drop transport messages, plus a model stub that blocks on test-controlled gates. Native-module runners consume the same domain fixtures without inventing socket semantics. Use narrow internal barriers for atomic dispatch/pending-state races and snapshot publication boundaries that a proxy cannot force. Bounded waits report failure; sleeps are not proof of reaching a state.

Critical scenarios are listed by milestone in the [implementation plan](implementation-plan.md). Include labelled sentinel content from stage 1. Display it through live events, snapshots where supported, and the actual React Native path. Have the model stub distinguish planner requests from legitimate processor and vetting requests. Confirm captured planner requests exclude it, including after phone write/command/fetch decisions that do not promote content. Explicit promotion is outside that test and outside the initial phone allowlist.

Preserve baseline stdio details: events may precede responses; malformed input with a recoverable ID gets an error; unanswerable input gets no invented response ID and does not prevent a later valid request; EOF ends the child and refuses pending questions. Do not require a final event after EOF unless intentionally added to the contract.
