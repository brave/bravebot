# Client interface and prototype contract

Status: proposed, not implemented. Operation names are provisional. This document defines behavior and capability limits; stage 1 supplies exact schemas and fixtures. See the [architecture](architecture.md) for ownership and permission rules.

This is a high-level starting plan, not an exhaustive account of edge cases or behavior. Expect implementation discoveries to change or add to it. Update the affected design, specs, and tests as those decisions are made; resolve security gaps before enabling the affected feature. See the [executive summary](executive-summary.md) for the full proposal in one document.

## Code location and reuse

Create a standalone TypeScript package at `packages/agent-client/` with its own build and test commands. Its common module contains domain types and the session interface without Node, DOM, Electron, or JNI dependencies. Put the Node/stdin/stdout adapter in a separate submodule and a small local test program under `packages/agent-client/scripts/`. Building and testing this package must not require the Electron app or its build configuration.

Review `ui/src/main/bridge.ts` for reusable framing/correlation code and tests. Reuse suitable logic without importing Electron modules or requiring desktop migration during the local client proof. Electron binary discovery and app lifecycle remain in the desktop app. Desktop adoption of the shared client belongs to the later handoff stage.

Use a small language-independent scenario fixture directory, proposed `packages/agent-client/test-fixtures/`, for typed operations, serialized boundary payloads, expected responses/events, labels, and symbolic IDs. Thin runners bind fixtures to the TypeScript client, real stdio process, local socket, and the actual platform native module. Run native scenarios on each supported platform; success on one does not establish support on the other. Framing cases apply only to stream transports. Use explicit synchronization steps rather than timing-dependent transcripts.

The React Native app imports the package’s dependency-free common module through the app shell and package-consumption setup owned by stage 3d. Stage 2b reuses that setup; if embedded work starts first, it may bring forward this shared setup without creating a second shell or making stage 3d depend on the embedded runtime. Keep package setup small; repository-wide workspace tooling is not a prerequisite for the local proof. A new Rust crate is not required for the client. Any new crate introduced later needs the corresponding layering spec entry.

## Shared screen state

The shared package owns a pure reducer: snapshot plus ordered events produces transcript rows, busy state, pending approvals, and controller status. Both React Native and the later Electron prototype consume it. The host remains authoritative for execution and supplies one documented snapshot/event shape; client screen state never grants authority. Shared fixtures keep the Rust host projection and TypeScript reducer consistent.

Also share presentation-only label and control-character handling as display segments. Native components still need tests for visible marking, layout, and complete approval details. For each supported approval kind, enumerate every payload field as displayed or deliberately excluded with a reason; never omit a field needed for informed consent.

Stage 1 covers supported stdio behavior and one unsupported-operation case. Add listener allowlist scenarios with stage 3b. Give the package one build/type/test command and wire it into the relevant CI and local checks when the package is added.

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

Local host trust controls and embedded trust remain explicit typed capabilities. Remote workspace trust is unavailable in the first prototype. The user answers it on the host before the session can run. Creating a session from the phone can therefore produce `awaiting host setup` rather than immediately executing. The demo may use a workspace explicitly prepared on the host; project pairing alone never answers trust.

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

Require host handling for workspace trust, vouching, command-output admission, vetted-content promotion, credential-exposure approval, language-server startup, and other unlisted reply kinds. Some have session-lived authority even without `remember`; do not classify all replies as one-use. The phone advertises only the kinds it can actually render and answer. Host-only handling requires desktop control; when nobody capable is ready, refusal applies. Adding another phone kind is a small later contract/test change.

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
| 1–2a Local stdio client | New typed client and fixtures work against a real process; framing/correlation preserves existing behavior. | Reconnect, takeover, retry recovery, or remote access. |
| 2b Embedded device proof | Common session semantics cross the React Native/native/Rust boundary for selected capabilities. | Background survival, full host tools, offline inference, or support on an untested platform. |
| 3a–3c Persistent local host | Exact targets, isolated ownership, safe close, minimum control state, and loss handling. | Networking, full transcript history, or phone recovery. |
| 4a–4b Bounded recovery | Retained display state and send-ID lookup survive connection loss while the host lives. | Crash-safe acceptance, generic mutation retries, shared queues. |
| 5a–5b Control handoff | One controller and viewers share a live session; stale-controller actions fail. | Concurrent controllers or ordinary saved-session import. |
| 6–8 Network and phone | The enabled contract works with authentication, scope checks, and tested device lifecycle limits. | Unattended approval while the phone is locked. |

## Evidence required

Stages 1–2a exercise the reducer with interleaved events and later snapshot-boundary fixtures. Stage 2 relies on existing bridge tests for agent behavior and adds evidence through the new client. Inventory `protocol.rs`, `refusal.rs`, `dispatch.rs`, `remembered_trust.rs`, `rules.rs`, and `fetch.rs` before writing overlapping tests. None has been run for this proposal; passing the existing suite alone does not prove the new client.

Use a controllable byte/frame proxy to delay, split, combine, or drop transport messages, plus a model stub that blocks on test-controlled gates. Native-module runners consume the same domain fixtures without inventing socket semantics. Use narrow internal barriers for atomic dispatch/pending-state races and snapshot publication boundaries that a proxy cannot force. Bounded waits report failure; sleeps are not proof of reaching a state.

Critical scenarios are listed by milestone in the [implementation plan](implementation-plan.md). Include labelled sentinel content from stage 1. Display it through live events, snapshots where supported, and the actual React Native path. Have the model stub distinguish planner requests from legitimate processor and vetting requests. Confirm captured planner requests exclude it, including after phone write/command/fetch decisions that do not promote content. Explicit promotion is outside that test and outside the initial phone allowlist.

Preserve baseline stdio details: events may precede responses; malformed input with a recoverable ID gets an error; unanswerable input gets no invented response ID and does not prevent a later valid request; EOF ends the child and refuses pending questions. Do not require a final event after EOF unless intentionally added to the contract.
