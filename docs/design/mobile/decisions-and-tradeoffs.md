# Mobile prototype decisions and tradeoffs

Status: design decisions, not implemented guarantees. Read the [executive summary](executive-summary.md) for the goal and the [implementation plan](implementation-plan.md) for stages and required evidence.

## How to review this plan

This record distinguishes deliberate scope choices from unresolved decisions. “Settled for the prototype” means proceed on that basis, subject to implementation evidence. It does not mean the choice is permanent or exempt from review.

Challenge a choice when there is a concrete security defect, contradiction, missing requirement, or evidence that its cost outweighs its benefit. Name the affected behavior and stage. An alternative alone does not make the current choice a blocker. Unresolved decisions block only the capabilities named below; stages 1–2a can start now.

This is not an exhaustive list of edge cases. Record discoveries as implementation proceeds, update the affected documents and tests, and distinguish demonstrated behavior from reasoning or checks not run. These decisions do not override the [normative specs](../../specs/README.md).

## Settled for the prototype

### 1. React Native for both mobile modes

- **Decision:** use React Native for remote control and on-device execution, with no Android-over-iOS priority.
- **Reason:** share mobile screens and TypeScript logic; coding agents’ familiarity with React Native should help implementation.
- **Cost:** native bindings, permission handling, and UI validation still differ by platform. Existing WebView screens do not transfer directly.
- **Revisit when:** device evidence shows a required behavior cannot be delivered within reasonable platform or integration limits. The first device proof does not establish support on the other platform.

### 2. A standalone client and shared screen state

- **Decision:** put the proposed client, reducer, fixtures, and local test program in `packages/agent-client/`, independent of Electron. The reducer turns snapshots and events into screen state; the host retains execution authority.
- **Reason:** validate the interface locally and reuse its behavior across clients without making mobile depend on the desktop app.
- **Cost:** a separate package needs build/test commands and CI coverage. The Rust host projection and TypeScript reducer still need shared contract fixtures.
- **Revisit when:** actual consumers expose a package-boundary or state-model problem. Reuse suitable existing code without making desktop migration a prerequisite.

### 3. Early phone feedback and parallel embedded work

- **Decision:** stages 1–2a come first. Embedded work in 2b proceeds independently of the host track. Stage 3d uses a thin phone experiment; 5a proves takeover with two TypeScript clients before 5b integrates Electron.
- **Reason:** discover phone lifecycle and transport limits early, and avoid making native or desktop integration block host validation.
- **Cost:** the early phone experiment uses synthetic data and controlled tools. It proves neither secure remote access nor complete embedded support.
- **Revisit when:** the experiment reveals a contract or recovery flaw. Both the embedded proof and Electron handoff remain required for full prototype completion.

### 4. One Rust host and an isolated session store

- **Decision:** run the bridge and listener in one process, with a sessions-root override for fresh prototype records. Keep settings, trust stores, and credentials in their normal locations.
- **Reason:** avoid a supervisor/child lifecycle and shared-record writer coordination while proving live attachment.
- **Cost:** host failure interrupts live sessions. Ordinary saved-session import and automatic restart are excluded. Supported front ends must not independently resume host-owned records.
- **Revisit when:** real use requires importing existing sessions or recovering execution after a crash. `closed` confirms worker termination, not successful saving or rollback.

### 5. One controller, explicit takeover, no shared queue

- **Decision:** allow one active controller plus authorized viewers. Transfer control explicitly; reject sends while busy and retain an unsent draft.
- **Reason:** prove continuation between clients without concurrent control or queue-order rules.
- **Cost:** users must take control before acting and wait before submitting a follow-up. Old-controller actions must fail; controller loss refuses pending questions.
- **Revisit when:** observed use shows that queued work or simultaneous control is necessary. A new controller supplies its own supported approval kinds.

### 6. Snapshot-only reconnect recovery

- **Decision:** on reconnect or a sequence gap, fetch a fresh bounded snapshot and continue with ordered events. Defer incremental replay and complete history.
- **Reason:** reduce recovery states and races while retaining current session state.
- **Cost:** reconnect may resend recent content and omit older history. Show omissions and preserve full pending approval details separately.
- **Revisit when:** measured bandwidth, latency, or history requirements justify replay or pagination. Never reopen a saved record to recover a live session.

### 7. Retain narrow send-ID recovery

- **Decision:** record a message ID before dispatch, retain bounded outcomes for the live session, provide status lookup, and persist the phone’s pending send before transmission. Reject new sends at capacity rather than forget retry IDs.
- **Reason:** an accepted prompt can leave the bounded snapshot. Display history alone cannot settle a lost acknowledgement, especially after phone process loss.
- **Cost:** a small pending-message store and lookup API remain necessary. Host restart can still leave an unknown outcome. Other mutations are not automatically retried.
- **Revisit when:** measurements justify changing retention or removing a recovery promise. Do not remove identity storage while continuing to claim recovery across phone process loss.

### 8. Foreground approvals and host-only output admission

- **Decision:** require a ready foreground controller for supported phone approvals. Keep workspace trust, command-output admission, and authority-expanding decisions on the host.
- **Reason:** preserve explicit consent with a small, testable set of phone approval screens.
- **Cost:** locking the phone may cause refusal. A run-tests-then-read-failures task needs desktop takeover before output admission; it is not a phone-only demo. Startup trust stays unanswered when unavailable.
- **Revisit when:** a useful task justifies another approval kind or a defined waiting policy. Add complete field display, label handling, permission rules, and tests before enabling it. Push notifications do not themselves grant approval.

## Unresolved security decisions

These are not accepted risks or implementation details that can be skipped.

### U1. Local control-channel boundary

- **Status:** unresolved before stage 3b enables real-session control; applies to desktop and network backends too.
- **Decision needed:** how to prevent tool processes from taking control or answering later approvals.
- **Reason:** an approved unconfined command may run as the same OS user. Socket permissions, peer identity, or a token that command can read do not distinguish it from an authorized client.
- **Current limit:** synthetic data and controlled model/tool stubs only until the boundary is demonstrated. General same-user malware resistance is not claimed.
- **Candidates to assess:** OS-enforced separation between tool execution and the control service; or a private connection capability that tools cannot inherit or obtain, such as an inherited descriptor with no public listener. Neither is a proven solution here. Check the cost of platform confinement and how authorized clients reconnect after the original client exits. Process ancestry alone is not a boundary because a child can detach.
- **Completion evidence:** a short design note and an adversarial test with an approved unconfined child, including a child that detaches before attempting to connect and answer. A passing peer-credential test does not establish this boundary. See [local control boundary](architecture.md#local-control-boundary).

### U2. Pairing and transport identity

- **Status:** unresolved before stage 6 network access, or earlier network exposure during the phone experiment.
- **Decision needed:** host identity verification, credential creation/storage, revocation, backend access, input limits, and authorization to display private content.
- **Reason:** reachability and encryption alone do not establish who may control or read a session.
- **Current limit:** Tailscale is a reachability candidate, not assumed application authentication. A reachable backend must not trust forgeable proxy identity headers.
- **Completion evidence:** a short protocol decision and tests for unauthorized, revoked, wrong-host, oversized, and stalled requests. See [networking decisions](architecture.md#decisions-before-authenticated-networking).

### U3. Effective session scope

- **Status:** unresolved before stage 6 exposes session content or control.
- **Decision needed:** map resolved grants and settings to device access, including changes applied during a session.
- **Reason:** rules read at session open and settings read per turn have different lifetimes. A single scope API does not yet exist.
- **Current limit:** scope-expanding answers stay on the host; widened remote access requires authorization before disclosure. Ordinary one-use decisions should not cause needless reauthorization.
- **Completion evidence:** a short state/transition mapping and tests of initial scope, widening, and revocation. See [permissions and effective scope](architecture.md#permissions-and-effective-scope).

## Maintaining this record

Update an entry when evidence changes its decision, cost, or completion condition. Resolve U1–U3 in place or link their short decision notes here. Keep the architecture, client contract, implementation stages, and executive summary consistent. Routine choices such as names, libraries, and numerical limits can be made during implementation within these boundaries.
