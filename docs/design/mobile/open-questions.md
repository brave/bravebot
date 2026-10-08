# Mobile prototype open questions

Status: proposal with Q2 resolved for the first Rust block. This is the single record of unresolved decisions and choices awaiting validation. The [decision record](decisions-and-tradeoffs.md) holds settled choices and their reasons; the linked architecture and contract sections hold implementation requirements.

**Unresolved** means an answer is still needed. **Awaiting validation** means the plan has a proposed answer that needs evidence. Neither status establishes implemented behavior. Each entry names the capability that needs its answer; unrelated work can proceed.

## What can proceed

Stages 1–2a are complete; Q2 records the placement of the view. U1 blocks real-session listener control; U2 and U3 block authenticated network exposure. The restricted stage-3d synthetic experiment can proceed under its [development transport limits](implementation-plan.md#3d-run-a-thin-phone-experiment). Device UI, real credentials, distribution, and dependency additions have the separate checkpoints below.

## Unresolved security decisions

These are not accepted risks or implementation details that can be skipped. The current limits apply until the required evidence is recorded.

### U1. Local control-channel boundary

- **Status:** unresolved before stage 3b enables real-session control; applies to desktop and network backends too.
- **Decision needed:** how to prevent tool processes from taking control or answering later approvals.
- **Reason:** an approved unconfined command may run as the same OS user. Socket permissions, peer identity, or a token that command can read do not distinguish it from an authorized client.
- **Current limit:** synthetic data and controlled model/tool stubs only until the boundary is demonstrated. General same-user malware resistance is not claimed.
- **Candidates to assess:** OS-enforced separation between tool execution and the control service; or a private connection capability that tools cannot inherit or obtain, such as an inherited descriptor with no public listener. Neither is a proven solution here. Check the cost of platform confinement and how authorized clients reconnect after the original client exits. Process ancestry alone is not a boundary because a child can detach.
- **Completion evidence:** a short design note and an adversarial test with an approved unconfined child, including a child that detaches before attempting to connect and answer. A passing peer-credential test does not establish this boundary. See [local control boundary](architecture.md#local-control-boundary).
- **Investigation so far (2026-10-07, macOS only):** the stdio client's control channel is an inherited anonymous descriptor with no name. Node created it as a socketpair (`lsof` showed `unix` for descriptors 0–2 of `bravebot-rpc`). A real `run` approved through the client, with the planner a test stub, ran `lsof -p <bravebot-rpc pid>` under the macOS sandbox. It listed those descriptors by number but gave the child no path or address to connect to, and `ls /dev/fd` in the same sandbox was refused. A child can therefore see the channel and has no way found to write to it on this host. Not tested: Linux, where `/proc/<pid>/fd` can reopen a pipe for the same user (a socketpair it cannot); a launcher that uses pipes; an unconfined child; a child that detaches first; same-user debugger attach, which is outside the prototype claim. A persistent host that clients reconnect to needs a named endpoint, which the inherited-descriptor candidate does not provide. **Real-session listener control stays blocked.** Stage 3b may use stdio-like inherited descriptors for synthetic experiments only. The adversarial test above is still to be written.

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

## Implementation choices and validation

### Q1. Does the React Native implementation meet the mobile interaction requirements?

- **Status:** React Native selection settled; device validation pending.
- **Question and reason:** do the implemented screens meet the required responsiveness, input, accessibility, approval-display, and lifecycle behavior on the tested platform?
- **Current assumption:** use React Native for both modes, as recorded in the [UI decision](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes). No competing WebView or PWA proof is required.
- **Evidence needed:** device checks of a transcript, composer, and complete approval card, covering responsiveness, keyboard, scrolling, accessibility, labels, and lifecycle. Record gaps and untested platforms.
- **Before:** expanding the screens beyond the first stage-2b or stage-3d device proof.

### Q2. Where should shared Rust presentation code live?

- **Status:** resolved for the minimal local Rust block; native display validation remains pending.
- **Decision:** `crates/ui-bridge/src/view.rs` owns session projection under LAYER-1's existing
  non-presentation boundary. It copies complete labelled wire payloads and branches only on
  bridge-owned event names and status/identity metadata. `wire::submitted` reuses saved-history
  tags, and `Running`/`BridgeConfirmer` keep approval semantics.
- **Evidence:** the stdio caller exercises `session.view.start`, initial state and typed updates
  through the real process in `crates/ui-bridge/tests/fetch.rs`. The emitter barrier test pins
  terminal ordering; view tests pin approval replacement payloads. No new dependency is needed.
- **Presentation choice:** no released-content transform is needed by this transport-only block.
  Existing CLI/TUI marking helpers inspect text and produce terminal output; importing them into
  bridge dispatch would violate its boundary. Extract a narrow allowed presentation module or
  crate when the first rendering caller needs shared transforms, with its layering entry and tests.
- **Remaining:** native rendering and marking. See [implemented scope](client-contract.md#implemented-typescript-client).
  The TypeScript client library carries payloads as received and reads none of them. Its terminal
  program escapes control, zero-width and bidirectional characters locally in a diagnostic dump and renders
  nothing, so neither needed a presentation module. This decision completes the placement choice for the stdio client, not
  device presentation.

### Q3. How are embedded API keys set up, stored, and revoked after device loss?

- **Status:** awaiting validation.
- **Question and reason:** where do embedded-mode API keys come from, how are they entered and stored on the phone, and how can they be revoked if the phone is lost? Secure entry, request use, and teardown must keep API keys out of JavaScript and preserve Rust credential-buffer protections.
- **Current assumption:** test native secure entry, the proposed storage and authentication settings, and stopping embedded work on lock/background with one separately issued, revocable gateway API key entered by the person on the phone and bound to a confirmed endpoint. These mechanisms are provisional; revise them when device evidence supports an alternative that preserves the [credential requirements](architecture.md#embedded-model-credentials) and foreground-only scope. Model and pairing credentials remain separate.
- **Evidence needed:** demonstrate API-key issuance/setup, secure entry and storage, and the architecture's lifecycle checks with test keys: lock, invalidation, backup/restore, deletion, endpoint changes, and log exclusion. Verify provider-side API-key revocation without access to the lost phone, separately from host-pairing revocation. Local deletion does not establish provider-side revocation. Record any platform requirement that cannot be met.
- **Before:** real-backend embedded use. The stage-2b proof can use a controlled model stub without production credentials.

### Q4. Which iOS capabilities and distribution routes can be supported?

- **Status:** awaiting validation of embedded capabilities; distribution eligibility unresolved.
- **Question and reason:** which on-device tools work within iOS limits, and which distribution route permits the intended on-device agent and remote-control features? Host tool execution is governed by host permissions and authorization, not the phone's sandbox.
- **Current assumption:** begin the on-device agent proof with developer-installed, foreground execution and app-private files. This proof excludes shell commands, downloaded executables, package installation, and arbitrary filesystem access. Those exclusions do not apply to remote host tools. Assess app distribution separately for the features shipped in each mode.
- **Evidence needed:** hardware results for the supported tools and lifecycle, with device/OS and distribution method recorded; review the enabled features, entitlements, SDKs, privacy disclosures, and account flow against current platform rules. Follow the [iOS limits](architecture.md#ios-capability-and-distribution-limits); record uncertainty rather than predict store acceptance.
- **Before:** advertising an embedded capability, and before TestFlight/App Store submission or promising that distribution route. Other platform and local host work can proceed.

### Q5. What dependency cost does the mobile app add?

- **Status:** dependency selection unresolved; the review and check requirements are settled.
- **Question and reason:** which runtime, native, and build dependencies are necessary, and can their cost be justified under the repository's existing policy?
- **Current assumption:** an independent thin adapter package, no root npm workspace requirement, and the smallest runnable mobile app without extra UI kits or state frameworks.
- **Evidence needed:** direct/transitive dependency inventory, licences, scripts and native-code review, reproducible builds, and advisory/check coverage for the selected managers under the [dependency plan](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds).
- **Before:** landing the additions in stage 1 or the first app work in 2b/3d; reassess the cost at Q1's UI checkpoint.

## Keeping this record current

Update an entry when its assumption, evidence, or required checkpoint changes. When resolved, mark it resolved and replace the open discussion with a link to the decision and its evidence. Update the affected architecture, contract, and stages in the same change. Keep U1–U3 and Q1–Q5 identifiers stable so existing references continue to work.

Add a question when it affects a capability, security boundary, or delivery choice. Routine names, numerical limits, and libraries can be chosen during implementation and recorded with their tests. Keep checkpoints and scope cuts in the [implementation plan](implementation-plan.md#checkpoints-and-scope-cuts); review remaining work there as these questions are answered.
