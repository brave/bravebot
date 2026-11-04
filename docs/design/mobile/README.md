# Mobile agent and remote-control prototypes

Status: stages 1–2a, 3a, 3b and 3c are built: the Rust session view, the stdio TypeScript client in `packages/agent-client`, exact action targets, and the local host `bravebot-host` with controller loss and takeover after a client leaves. The React Native app in `mobile/` has its shell and session screens, tested against a fake client, with no target connected. Stages 2b, 4a, 4b, 5a, 5b and 6–8 remain proposed, and 3d has no device observations yet. See [current local scope](client-contract.md#implemented-typescript-client).

Start with the [executive summary](executive-summary.md). It is written to stand alone for readers who need the goal, design choices, implementation sequence, and prototype limits.

## Problem and intended result

Bravebot mobile should support both an agent executing on the device and remote control of sessions executing on a user's host machine. Coding is an initial proving task; the mobile product should also support other useful work as its tools and permissions expand.

Use React Native for native mobile controls and shared Android/iOS screens, with device validation of the first session and approval screens before expanding the UI. Keep shareable session behavior and presentation transforms in Rust; TypeScript supplies thin adapters and UI wiring. See the [UI and Rust decisions](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes). The plan does not prioritize either platform; choose the first device proof based on integration effort and available hardware, and record platform coverage. Give it one session client interface with embedded and remote adapters. The user explicitly chooses an execution target such as “This device” or a paired host. A session has one execution owner, with its own files, tools, credentials, permissions, and records. Switching targets selects or creates a session; it does not migrate live execution or copy grants.

On-device agent execution means the Rust agent runs on the phone. Model inference is a separate configuration choice: the agent may still use a remote model service. On-device execution does not promise an on-device model or offline operation.

For remote sessions, users can leave the desktop, read progress on the phone, send a follow-up, answer a permission request, or stop work. The host retains execution and state while the phone connection may disappear.

The first deliverable, a TypeScript client interface and client on the local host machine driving a real RPC process, is implemented in `packages/agent-client`. Run a bounded React Native embedded proof through a platform native module as a parallel track; it does not gate host development. The Rust/JNI work on `android-shell` is a possible starting point for the Android binding; iOS needs its own native binding. Develop the persistent host and remote-control track, and connect the same mobile screens through the remote adapter. Remote control is the first substantial end-to-end prototype; the early embedded exercise tests the shared interface and native integration without requiring full on-device capability.

## Delivery rule

Stages 1 and 2a defined the thin client interface and the shared Rust view, exposed that view as an additive RPC capability, and proved the client against the real process. Later-stage questions did not block that deliverable and still do not block stage 3a. The plan sets security boundaries and milestone evidence; it is not intended to settle every implementation detail in advance.

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

The React Native app, the TypeScript client, the shared Rust view and the local host are built on `main`. Stage 2b (the embedded path) is not started.

The [android-shell](https://github.com/brave/bravebot/tree/android-shell) branch is an unmerged Android proof of concept, six commits by Aubrey Keus and far behind `main`. Two parts of it are relevant, both to stage 2b: the Rust/JNI binding, which runs the agent and UI bridge inside the Android app, and the input restrictions of its Kotlin host (`Host.kt`), which allowlists bridge methods and strips `turn.send` fields as the desktop main process does, with Node tests (`ui/scripts/android-host.test.mjs`) that keep the two lists equal. Those tests read source and do not exercise JNI or Kotlin at runtime. Its WebView renderer and layout are superseded by the React Native app. When stage 2b starts, check whether the binding and restrictions still apply, take it by cherry-pick or merge where feasible to keep authorship, and otherwise credit the original author and commits in the adaptation.

## Desktop behavior that stage 5b changes

- `ui/src/main/bridge.ts` runs `bravebot-rpc` as one child process for the app, and `ui/src/main/index.ts` disposes it when the window closes or the app quits. Desktop sessions end with the window.
- The renderer holds and dispatches follow-up messages (`ui/src/renderer/use-turn.ts`) and inserts submitted prompts into its own transcript. The bridge rejects a turn while another runs. Prototype sessions reject a busy send and keep the draft; ordinary desktop queues keep their behavior.
- Electron's main process allowlists bridge methods and strips raw file lists from renderer turn requests. The host enforces its own allowlist ([local-host.md](../../specs/local-host.md)); Electron's list is not reused for it.

## Recommendation

Create a standalone TypeScript adapter package at `packages/agent-client/` with its own build, tests, and local test program. The Rust execution owner supplies shared session view state; no parallel TypeScript domain reducer is planned. Keep Node, DOM, Electron, and JNI dependencies out of its common module. Reuse suitable framing/correlation logic from `ui/src/main/bridge.ts` without depending on the Electron app. Desktop integration waits until the handoff stage. Validate it against a real stdio RPC process with isolated files, a controlled model backend, and portable scenario fixtures. Existing process tests provide agent-behavior evidence; the new tests must prove the new client mapping.

Prove a small React Native embedded adapter through the chosen platform’s native/Rust boundary independently of host work. Implement the persistent host as one Rust process owning the bridge library and listening on a local socket. Exact action targets, readiness/loss handling, close completion, and storage isolation precede reattached control.

Use one active controller per session and authorized viewers, with explicit takeover between desktop and phone. Reject sends during an active turn rather than building a shared queue. Protect text sends with a message ID recorded before dispatch and explicit status lookup; do not build generic mutation retries. Keep unknown outcomes honest after host restart.

Before phone access, enforce authentication, host identity, the restricted operation/reply-field list, whole-session read/control scope, and private-display destination authorization. Workspace trust and scope-expanding decisions stay on the host. Demonstrate approvals with the phone foregrounded; locked/backgrounded-phone waiting and notifications are later work.

A private network is sufficient for the remote demo. Tailscale Serve is a candidate for HTTPS reachability within a tailnet, not an application authorization mechanism. See [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve).

The host track is client contract and shared screen state, local stdio proof, exact bridge targets, local Rust host/controller lifecycle, a synthetic phone experiment, bounded recovery, two-client takeover, authenticated networking, and the remote phone demonstration. Embedded execution and Electron integration are separate tracks; both remain part of full prototype completion. Shared queues, concurrent control, saved import, automatic restart, and broader on-device tools are not prerequisites.

The proposal includes React Native for both remote control and embedded execution. Embedded real-backend use requires the [credential lifecycle](architecture.md#embedded-model-credentials); iOS capability and distribution claims require the [platform checks](architecture.md#ios-capability-and-distribution-limits). New packages follow the [dependency plan](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds). Source findings do not establish demonstrated behavior.

## Document ownership

Maintain this proposal in `docs/design/mobile/`. Earlier notebook drafts are historical copies. Update these documents as implementation resolves open choices, and add normative specs and test evidence with the behavior they govern.
