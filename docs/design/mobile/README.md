# Mobile agent and remote-control prototypes

Status: the first Rust session-view block and the stdio TypeScript client's session lifecycle and approval replies are implemented. The local client program and the remaining stage 1–2a evidence are outstanding, so stages 1–2a are incomplete. Later mobile stages remain proposed. See [current local scope](client-contract.md#implemented-typescript-client).

Start with the [executive summary](executive-summary.md). It is written to stand alone for readers who need the goal, design choices, implementation sequence, and prototype limits.

## Problem and intended result

Bravebot mobile should support both an agent executing on the device and remote control of sessions executing on a user's host machine. Coding is an initial proving task; the mobile product should also support other useful work as its tools and permissions expand.

Use React Native for native mobile controls and shared Android/iOS screens, with device validation of the first session and approval screens before expanding the UI. Keep shareable session behavior and presentation transforms in Rust; TypeScript supplies thin adapters and UI wiring. See the [UI and Rust decisions](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes). The plan does not prioritize either platform; choose the first device proof based on integration effort and available hardware, and record platform coverage. Give it one session client interface with embedded and remote adapters. The user explicitly chooses an execution target such as “This device” or a paired host. A session has one execution owner, with its own files, tools, credentials, permissions, and records. Switching targets selects or creates a session; it does not migrate live execution or copy grants.

On-device agent execution means the Rust agent runs on the phone. Model inference is a separate configuration choice: the agent may still use a remote model service. On-device execution does not promise an on-device model or offline operation.

For remote sessions, users can leave the desktop, read progress on the phone, send a follow-up, answer a permission request, or stop work. The host retains execution and state while the phone connection may disappear.

The first deliverable remains a TypeScript client interface and client on the local host machine driving a real RPC process. Run a bounded React Native embedded proof through a platform native module as a parallel track; it does not gate host development. The Android Rust/JNI work on `android-shell` provides an existing foundation for embedded integration; iOS needs its own native binding. Develop the persistent host and remote-control track, and connect the same mobile screens through the remote adapter. Remote control is the first substantial end-to-end prototype; the early embedded exercise tests the shared interface and native integration without requiring full on-device capability.

## Delivery rule

Start stages 1 and 2a now: define the thin client interface and shared Rust view, expose that view as an additive RPC capability, and prove the client against the real process. Later-stage questions do not block that deliverable. The plan sets security boundaries and milestone evidence; it is not intended to settle every implementation detail in advance.

This is intentionally a high-level plan, not an exhaustive specification of every edge case or behavior. Implementation will reveal gaps, platform limits, and better choices. Some discoveries will require implementation and behavior changes not described here. Resolve them as they arise and update the affected design, specs, and tests. The acceptance checks are a starting set; extend them for risks found during implementation.

Resolve a gap before enabling the behavior it could make unsafe. Prefer a narrow supported behavior and a clear refusal or unknown outcome over a larger recovery system. Choose routine names, limits, timeouts, and libraries during implementation, record them in the contract, and test the chosen boundary. Revise design decisions when implementation evidence warrants it; completing the plan in advance is not a condition for starting work.

The first planned on-device agent proof (stage 2b) uses one session and demonstrates a turn with one supported question or approval. The first secure remote checkpoint uses fresh host-owned sessions shared by the phone and a local test client. The full demonstration adds Electron handoff and the embedded proof; see the [delivery checkpoints](implementation-plan.md#first-secure-remote-checkpoint). Existing saved-record import, full desktop parity, automatic restart, crash-safe queues, and complete retained history may follow independently. The remote prototype uses one controller with explicit takeover and viewers, an isolated session store, and no shared follow-up queue. Scope reductions must be visible in the UI and capability contract. Authentication, correct approval targets, content labels, and single-writer ownership are not optional.

## Documents

- [Decisions and tradeoffs](decisions-and-tradeoffs.md): deliberate scope choices, costs, and revisit criteria.
- [Open questions](open-questions.md): unresolved decisions, choices awaiting validation, and the evidence needed before enabling each capability.
- [Executive summary](executive-summary.md): standalone human-readable plan, implementation steps, security boundaries, and completion criteria.
- [Client interface and local validation](client-contract.md): operations, transport responsibilities, capability stages, and shared contract tests.
- [Architecture and security](architecture.md): session ownership, protocol changes, reconnect behavior, permission boundaries, and tradeoffs.
- [Implementation plan](implementation-plan.md): scope, stages, checkpoints, reduced deliverables, and completion criteria.

These documents are the authoritative mobile design proposal. They describe planned work, not implemented guarantees, and do not replace Bravebot's [normative specs](../../specs/README.md). The findings below come from source review; they are not runtime validation.

## Current mobile status

The [android-shell](https://github.com/brave/bravebot/tree/android-shell) branch is an unmerged Android proof of concept. The reviewed revision contains:

- **On-device execution:** the Rust agent and UI bridge run inside the Android app through JNI.
- **A mobile application:** the existing renderer runs in a WebView, with phone navigation, drawers, sheets, and touch-layout work.
- **Input checks and focused tests:** the Android host checks allowed methods and filters turn inputs. `android-host.test.mjs` checks source consistency with the desktop host; `compact-layout.test.mjs` exercises layout functions and checks layout source. Neither tests JNI or Kotlin runtime behavior.
- **Build and integration guidance:** Android build scripts and instructions document how the native runtime, app, and backend configuration fit together.

This work informs the proposed client interface, native bindings, approval screens, and tests. Review the runtime integration for reuse; the WebView screens do not determine the React Native architecture.

Review this branch before stage 2b and later work that uses it, and revisit relevant updates during integration. Record the revision used, assess new work before duplicating it, and build on applicable code, tests, and design lessons while preserving contributor attribution through cherry-picking or merging applicable commits where feasible. For adaptations that need a fresh commit, retain original author credit and source-commit references.

The source findings here use [`9635f16b`](https://github.com/brave/bravebot/tree/9635f16b49798624e6174b0d15818227773e81a8) for reproducibility. That revision's Android work is separate from the plan's `main` baseline. Refresh the reference when development reaches mobile integration and record build/device validation for the selected revision.

## Project findings

**Source revision:** Android paths and findings below refer to [`9635f16b`](https://github.com/brave/bravebot/tree/9635f16b49798624e6174b0d15818227773e81a8) (`9635f16b49798624e6174b0d15818227773e81a8`) on [android-shell](https://github.com/brave/bravebot/tree/android-shell). The `main` baseline is `c8fed64c89b5f9073c05ddd368936605662a2463`; it does not include that Android integration. See [Current mobile status](#current-mobile-status) for its contributions and the plan to follow its progress.

The review covered the Rust UI bridge, Electron integration, Android shell, session storage, and the relevant security specs. Source paths below are relative to the [repository root](../../../) at the cited review revisions; Android paths resolve in the pinned prototype tree above. The checkout HEAD recorded when documenting the findings was `837c25b0ffe618f72e557641af32a211f868b724`. A follow-up review checked lifecycle operations and Android UI code at `9635f16b49798624e6174b0d15818227773e81a8`. The main-side claims below were also checked by source review against `c8fed64c89b5f9073c05ddd368936605662a2463`; Android claims remain pinned to `9635f16b49798624e6174b0d15818227773e81a8`. These are source findings, not runtime evidence or a full audit.

| Area | Existing capability or behavior | Consequence for the proposal |
|---|---|---|
| Agent interface | `crates/ui-bridge/src/bridge.rs` exposes session listing, creation, resumption, turns, cancellation, and typed approval replies through `Bridge::dispatch`. | Reuse the bridge and agent engine. Add remote coordination around them. |
| Transport | `crates/ui-bridge/src/bin/bravebot-rpc.rs` carries newline-delimited JSON over stdin/stdout. The bridge library emits events through a callback. | Reuse stdio for the first client test. The proposed persistent Rust listener owns the bridge library in-process; it does not supervise another RPC child. |
| Desktop lifecycle | `ui/src/main/bridge.ts` owns a child process. `ui/src/main/index.ts` disposes the bridge when its window closes. | Running sessions are currently tied to the desktop window's lifetime. |
| Android prototype | `android/README.md`, `crates/android`, and `android/app/src/main/java/com/brave/bravebot/Agent.kt` describe and implement an in-process JNI agent. `ui/src/android/bravebot.ts` provides the renderer API. | Build on the Rust/JNI integration, reuse relevant source-consistency and layout checks, and add native runtime tests for the embedded adapter. Use the WebView shell’s session and approval interactions to inform the React Native UI. |
| Android prototype UI and limits | At the pinned revision, `Transcript.tsx`, `Modal.tsx`, and `styles/shell.css` provide a phone app bar, drawers, sheets, and touch adjustments. The branch includes phone-layout work beyond the scope described in its Android README. Reassess workspace and tool support at the revision chosen for integration. | Reuse interaction and approval semantics as references. DOM/CSS components do not transfer directly to React Native; budget native mobile screens. Validate on a device. Embedded execution needs an honest capability list; host execution does not need an Android command backend. |
| Saved sessions | `crates/session/src/sessions.rs` and `docs/specs/sessions.md` define records shared by desktop and terminal. Bridge resumption writes back to the resumed record. | Existing record interoperability is useful later. The prototype uses an isolated store and excludes saved-record import; its desktop mode attaches through the service. |
| Live sessions | `session.open` loads a record and creates a new in-memory handle. It does not attach to another process's active session. | Shared storage alone does not provide remote control. Independent writers can create conflicting histories. |
| Queues | `ui/src/renderer/App.tsx` holds and dispatches follow-up messages. The bridge rejects another turn while one is running. | Shared queues are deferred. Prototype sessions reject sends while busy and retain local unsent drafts. Ordinary desktop queues keep their existing behavior. |
| Approvals | `crates/ui-bridge/src/running.rs` matches reply IDs and kinds and consumes pending answers. IDs may recur in later turns, as documented in `ui/docs/security.md`. | Remote replies need identity that remains unambiguous across reconnects and host restarts. |
| Input restrictions | Electron's main process and the Android prototype's `Host.kt` allowlist methods and strip raw file lists from renderer turn requests. | A remote endpoint must enforce equivalent restrictions on the host. Exposing the raw dispatcher would bypass an existing boundary. |
| Display security | `docs/specs/layering.md` and `ui/docs/security.md` require content labels and marked presentation of released untrusted content. | The mobile UI must retain these distinctions and approval details. |

## Existing versus proposed

Existing components provide agent execution, policy gates, session persistence, request/reply shapes, event emission, and much of the display UI. The shared store supports resumption between front ends.

The following are proposed additions: a React Native UI, shared session interface and portable fixtures, embedded and remote adapters, a direct Rust host, isolated session storage, live attachment and explicit control handoff, device pairing/revocation, a restricted API, bounded display recovery, and duplicate-send protection. The existing Android runtime and UI work inform the new React Native integration; stage 2b assesses and adapts the relevant code and tests. The review did not find an existing mobile remote-session service to enable through configuration.

## Recommendation

Create a standalone TypeScript adapter package at `packages/agent-client/` with its own build, tests, and local test program. The Rust execution owner supplies shared session view state; no parallel TypeScript domain reducer is planned. Keep Node, DOM, Electron, and JNI dependencies out of its common module. Reuse suitable framing/correlation logic from `ui/src/main/bridge.ts` without depending on the Electron app. Desktop integration waits until the handoff stage. Validate it against a real stdio RPC process with isolated files, a controlled model backend, and portable scenario fixtures. Existing process tests provide agent-behavior evidence; the new tests must prove the new client mapping.

Prove a small React Native embedded adapter through the chosen platform’s native/Rust boundary independently of host work. Implement the persistent host as one Rust process owning the bridge library and listening on a local socket. Exact action targets, readiness/loss handling, close completion, and storage isolation precede reattached control.

Use one active controller per session and authorized viewers, with explicit takeover between desktop and phone. Reject sends during an active turn rather than building a shared queue. Protect text sends with a message ID recorded before dispatch and explicit status lookup; do not build generic mutation retries. Keep unknown outcomes honest after host restart.

Before phone access, enforce authentication, host identity, the restricted operation/reply-field list, whole-session read/control scope, and private-display destination authorization. Workspace trust and scope-expanding decisions stay on the host. Demonstrate approvals with the phone foregrounded; locked/backgrounded-phone waiting and notifications are later work.

A private network is sufficient for the remote demo. Tailscale Serve is a candidate for HTTPS reachability within a tailnet, not an application authorization mechanism. See [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve).

The host track is client contract and shared screen state, local stdio proof, exact bridge targets, local Rust host/controller lifecycle, a synthetic phone experiment, bounded recovery, two-client takeover, authenticated networking, and the remote phone demonstration. Embedded execution and Electron integration are separate tracks; both remain part of full prototype completion. Shared queues, concurrent control, saved import, automatic restart, and broader on-device tools are not prerequisites.

The proposal includes React Native for both remote control and embedded execution. The unmerged WebView proof of concept provides an integration reference, not a requirement to retain its UI architecture. Embedded real-backend use requires the [credential lifecycle](architecture.md#embedded-model-credentials); iOS capability and distribution claims require the [platform checks](architecture.md#ios-capability-and-distribution-limits). New packages follow the [dependency plan](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds). Source findings do not establish demonstrated behavior.

## Persistent-host gaps to implement

The local client stages remain the starting point. Before adding a persistent host, account for these existing limits: the bridge offers a fresh-session initial view but no late subscription or history snapshot; the renderer adds submitted prompts locally; existing direct-stdio lifetime handling does not supply persistent-listener controller-loss refusal; approval replies lack a turn target; cancellation targets only a session; repeated startup-trust replies can replace trust and wait on the running state lock; turn completion and view publication share the emitter lock, but worker-termination evidence remains absent; and resumed records restore additional directories and trusted-program grants. The architecture and client contract below specify proposed additions for these gaps. They are not existing guarantees.

## Document ownership

Maintain this proposal in `docs/design/mobile/`. Earlier notebook drafts are historical copies. Update these documents as implementation resolves open choices, and add normative specs and test evidence with the behavior they govern.
