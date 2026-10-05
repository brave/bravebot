# Mobile prototype decisions and tradeoffs

Status: design decisions, not implemented guarantees. Read the [executive summary](executive-summary.md) for the goal and the [implementation plan](implementation-plan.md) for stages and required evidence.

## How to review this plan

This record states deliberate scope choices; [open questions](open-questions.md) tracks unresolved decisions and validation. “Settled for the prototype” means proceed on that basis, subject to implementation evidence. It does not mean the choice is permanent or exempt from review.

Challenge a choice when there is a concrete security defect, contradiction, missing requirement, or evidence that its cost outweighs its benefit. Name the affected behavior and stage. An alternative alone does not make the current choice a blocker. Unresolved decisions block only the capabilities named in that record; stages 1–2a can start now.

This is not an exhaustive list of edge cases. Record discoveries as implementation proceeds, update the affected documents and tests, and distinguish demonstrated behavior from reasoning or checks not run. These decisions do not override the [normative specs](../../specs/README.md).

## Settled for the prototype

### 1. React Native for both mobile modes

- **Decision:** the prototype uses React Native for native mobile controls and shared Android/iOS screens in both execution modes. Neither platform has delivery priority.
- **Reason:** native controls and platform integration support the intended input, navigation, accessibility, and lifecycle behavior. Framework familiarity alone does not justify the choice. Validate the actual implementation on devices; the framework does not guarantee responsiveness or usability.
- **Cost:** new mobile screens, native bindings, a larger dependency tree, and separate marking/approval tests. React DOM/CSS views do not transfer directly.

The alternatives were a native WebView shell reusing the desktop renderer and a browser/PWA for remote control. A WebView offers more screen reuse, but we choose native controls for the mobile interaction model. A browser-only PWA does not supply the planned embedded Rust and OS integration. Neither alternative is a required competing implementation or benchmark for this plan.

`android-shell` is an unmerged proof of concept. Review its Rust/JNI integration and relevant interactions for reuse, following the [integration guidance](README.md#current-mobile-status). Its WebView screens do not determine the product architecture; preserve attribution for code reused.

At the first stage-2b or stage-3d device work, validate a transcript, composer, and complete approval card. Record responsiveness, keyboard and scrolling behavior, screen-reader and text-size checks, label spoofing checks, and lifecycle behavior. Fix gaps before expanding the screens and record untested platforms. This checkpoint validates the chosen implementation; React Native selection is settled for this plan.

### 2. Shared Rust behavior and a thin client

- **Decision:** keep reusable session behavior, approval semantics, and presentation transforms in Rust. Stdio, embedded, and persistent-host callers use one implementation and expose its view through typed updates. `packages/agent-client/` contains thin adapters, wire types, local UI state, fixtures, and the test program. Follow [UI-006](../../best-practices/ui.md#UI-006) across mobile and desktop.
- **Reason:** a Rust fix should reach both execution modes without a second TypeScript implementation. Remote clients consume the host's view; embedded clients consume the same view over the native binding. Layout, focus, and other window-only behavior stay in the UI.
- **Cost:** the local proof now includes an additive bridge view capability and a clearly separated Rust presentation boundary. The exact crate/module placement must obey the layering spec; putting content inspection in `bravebot-core` or `bravebot-agent` is forbidden. Bindings and actual UI rendering still need tests.
- **Revisit when:** a consumer needs a missing field or operation. Extend the owning Rust API and its tests before adding client-side behavior. Do not introduce a second domain reducer or a new Rust runtime in remote-only clients to solve a transport problem. See the [responsibility table](client-contract.md#shared-screen-state).

### 3. Early phone feedback and parallel embedded work

- **Decision:** stages 1–2a come first. Embedded work in 2b proceeds independently of the host track. Stage 3d uses a thin phone experiment; 5a proves takeover with two TypeScript clients before 5b integrates Electron.
- **Reason:** discover phone lifecycle and transport limits early, and avoid making native or desktop integration block host validation.
- **Cost:** the early phone experiment uses synthetic data and controlled tools. It proves neither secure remote access nor complete embedded support.
- **Revisit when:** the experiment reveals a contract or recovery flaw. The [first secure remote checkpoint](implementation-plan.md#first-secure-remote-checkpoint) can use the local test client for setup and takeover. Both the embedded proof and Electron handoff remain required for full prototype completion.

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

### 9. Mobile dependencies and builds

The repository already has Node tooling for the desktop and website. Its [dependency rule](../../best-practices/dependencies.md#DEP-001) covers both `Cargo.toml` and `package.json`: each addition needs a reason and a review of its transitive cost. React Native adds runtime, build, and platform dependencies beyond that existing tooling; the security rule about content does not protect against a compromised dependency executing inside the app.

Keep `packages/agent-client/` independent, with no runtime dependencies for its common adapter module unless a concrete need is demonstrated. Adding this directory does not introduce a root npm workspace or connect mobile dependencies to the published CLI wrapper. Add the mobile app's dependencies only with its first runnable screen. Avoid extra UI kits, analytics, remote JavaScript updates, and general state frameworks in the proof.

Before that addition lands:

- Record direct and transitive additions, licences, install/build scripts, native code, and why existing code or platform APIs do not suffice. Apply the same review to credential-storage bindings.
- Commit npm and native-platform lockfiles or equivalent dependency verification metadata. Install reproducibly; review any scripts that must run, using builds without production credentials. Keep signing/release credentials out of dependency-install jobs.
- Extend [lockfile checks](../../../package.json), `make check-npm`, [affected-check selection](../../../contrib/affected-checks.py), and CI to the new package/app paths. Extend [CODEOWNERS](../../../.github/CODEOWNERS) to new manifests, lockfiles, and build entry points using the existing review convention.
- Run `make check-deps` for Rust additions and record justified `deny.toml` exceptions. Its cargo-deny checks do not cover npm, CocoaPods, or Gradle dependencies. Add advisory and licence checks for the selected mobile dependency managers with documented exceptions; do not treat lockfile integrity as an advisory scan. Include their commands in local and CI checks before claiming coverage. Update the affected-check classifier selftests with new package paths, and state which native checks need device hardware or signing setup. Existing desktop checks do not establish mobile coverage.

No dependencies or build tooling are added by this proposal. Stage 1 covers the adapter and Rust additions; the first of stages 2b and 3d that adds the app covers its platform dependencies. Reassess the dependency cost at the UI checkpoint.

## Open questions

The [open-question record](open-questions.md) tracks U1–U3, implementation choices, and decisions awaiting validation, with the evidence and checkpoint each needs. Resolve entries there and link their decisions back here. Stages 1–2a can proceed while later security decisions remain open.

## Maintaining this record

Update an entry when evidence changes its decision, cost, or completion condition. Keep unresolved status in [open questions](open-questions.md), and keep the architecture, client contract, implementation stages, and executive summary consistent. Routine choices such as names, libraries, and numerical limits can be made during implementation within these boundaries.
