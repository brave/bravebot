# Mobile prototype implementation plan

Status: stages 1–2a are complete: the Rust session view and the stdio TypeScript client in `packages/agent-client`, tested against a real `bravebot-rpc`. Later mobile stages remain proposed. See [current local scope](client-contract.md#implemented-typescript-client).

This is a high-level starting plan, not an exhaustive account of edge cases or behavior. Expect implementation discoveries to change or add to it. Update the affected design, specs, and tests as those decisions are made; resolve security gaps before enabling the affected feature. See the [executive summary](executive-summary.md) for the full proposal in one document.

## Scope and delivery rule

The [decision record](decisions-and-tradeoffs.md) holds settled choices, costs, and revisit criteria. [Open questions](open-questions.md) tracks unresolved decisions and validation evidence by capability. U1 gates real-session control in stage 3b; U2 and U3 gate network access in stage 6. They do not block stages 1–2a. Earlier network exposure also requires U2.

Build a React Native mobile client for both embedded and remote execution. Remote control is the first substantial end-to-end prototype. Validate the common interface on the local host machine first, then run the bounded embedded proof as a parallel track alongside host work. Stage 2b does not gate stages 3–7; its result remains part of overall prototype completion.

The full remote demo uses one user, one host, one explicitly prepared workspace, fresh host-owned sessions in an isolated store, one active controller per session, and authorized viewers. Desktop and phone transfer control explicitly. The phone can start work, view progress, send a turn when idle, answer supported approvals, and cancel. A busy turn rejects another send; the UI retains a draft.

Do not require shared queues, concurrent controllers, normal saved-session import, a supervisor process, automatic restart, generic mutation retries, complete retained history, desktop parity, attachments, file browsing, bots, forks, watches, manifests, settings/hooks editing, public relays, or push. Broader on-device tools and background execution are follow-on work. Host workspace trust is answered on the host, never inferred from pairing.

Keep security boundaries and test evidence precise. Choose concrete names, libraries, limits, and timeouts during implementation. Record minor gaps and explicit non-claims rather than designing a new subsystem for every hypothetical failure. Stop only when an enabled capability would violate its security or correctness contract.

## First secure remote checkpoint

The first deliverable is the local client proof in stages 1–2a. The first secure remote checkpoint then comprises 3a–3d, 4a–4b, 5a, 6, and the narrow phone path in 7. Demonstrate one prepared workspace, fresh sessions, one tested phone platform, a supported approval, denial/cancel, reconnect, and duplicate-send prevention. A second local test client proves takeover. This checkpoint does not claim Electron handoff, embedded execution, or away-from-desktop validation from stage 8.

The local test client supplies the typed startup-trust interaction on the host before handing control to the phone. It uses the same controller identity and U1-protected channel as other local control, with no extra approval channel. Choose a task that needs no later host-only question until a capable host UI is available. Unavailable questions are refused; pairing never supplies the trust answer.

Defer 2b and 5b if they delay this checkpoint. They remain required for the full two-mode demonstration. Keep the first app to the screens and tools that prove this path; device evidence determines whether to extend it.

## Checkpoints and scope cuts

At each checkpoint, record what has been demonstrated, remaining work, and unresolved questions before choosing the next deliverable.

| Work | Checkpoint and useful stopping point |
|---|---|
| 1–2a: Rust view and local client | Real RPC turn, approval/denial, cancellation, and view updates through the thin client. Stop here with a tested local interface if later work is delayed. |
| 3a–3c: targets and local host | Exact targets, ownership, worker completion, and controller loss. Without U1, keep listener work synthetic. |
| 3d: phone experiment and UI validation | One React Native transcript/composer/approval flow, with responsiveness, input, accessibility, and lifecycle evidence. Validate these screens before expanding the UI. |
| 4a–4b and 5a: recovery and two-client handoff | Snapshots, send lookup, and explicit takeover survive forced connection loss. |
| 6–7: secure remote phone control | One platform, one prepared workspace, scoped pairing, supported approvals, and phone process recovery. |
| 8: away-from-desktop evidence | Network changes and host loss, with 5b complete for the desktop handoff claim. |
| 2b: bounded embedded proof | One platform, one supported tool/question, stub backend first. Real credentials require the architecture's storage and revocation checks. |
| 5b: Electron attachment | The supported desktop subset consumes the Rust view and transfers control both ways. |

Start U1 investigation during the local client work. Record a tested boundary or the reason real-session control remains blocked. Assess U2/U3 at stage 6 and resolve them before implementing real-session networking. Record unresolved credential-provider and iOS distribution questions at their required checkpoints; do not claim completion without the required evidence.

The dependency order is 1–2a, then 3a–3c, then 3d, then 4a–4b and 5a, then 6–7. Stage 2b can proceed alongside the host work and share the 3d app shell. Stage 5b follows 5a and can proceed alongside networking. Stage 8's full desktop demonstration needs 5b. These are behavior milestones, not a prescribed number of pull requests; keep each implementation change useful, tested, and independently reviewable.

When reducing scope, use this order:

1. Keep one tested phone platform, one workspace, and the smallest complete approval flow; defer additional screens, tools, and platforms. Unsupported capabilities must be visible and refused.
2. Stop at the [first secure remote checkpoint](#first-secure-remote-checkpoint), deferring 2b and 5b. Preserve its host setup path, refusal behavior, and recovery evidence.
3. If U1–U3 remain unresolved, stop at the local proof or restricted synthetic phone experiment. Keep real sessions blocked until those boundaries are resolved.

Never cut authentication, scope checks, labels, complete approval details, exact targets, single-writer ownership, or the recovery tests for a capability still advertised. Full prototype completion still requires 2b, 5b, and stage 8 evidence. A store release is a separate decision governed by the [iOS limits](architecture.md#ios-capability-and-distribution-limits).

## Stages

The first Rust block is complete: negotiated fresh-session view, typed row/status updates,
existing approval semantics, and real stdio tests. The TypeScript package now has the session
lifecycle (capability refusal, framing and correlation fixtures, view application, trust, send,
cancel and close) and approval replies for `confirm`, `run`, `fetch` and `ask`, tested against a
real `bravebot-rpc` (see
[the implemented client](client-contract.md#implemented-typescript-client)). The local client
program, composed-workflow and save-failure evidence, and the record of the U1 investigation
complete stages 1–2a. No native or listener work is included.

### 1. Define the common client and fixtures

Status: complete.

Create `packages/agent-client/` as a standalone TypeScript package with its own build and test commands, dependency-free wire types, and a thin typed interface. Keep the Node adapter separate. Review `ui/src/main/bridge.ts` for reusable framing/correlation logic, without importing Electron or migrating desktop callers in this stage. Put portable scenario fixtures in `packages/agent-client/test-fixtures/` and a Node test program in `packages/agent-client/scripts/`.

The first proof must build and run independently of the Electron app. React Native package integration belongs to stage 3d and is shared with stage 2b; desktop integration waits until stage 5b.

Map every initial operation to existing bridge behavior or a named addition. Implement the minimal shared Rust view and negotiated additive bridge capability described in the [client contract](client-contract.md#shared-screen-state), with its spec and tests. Reuse or extend existing Rust behavior; keep released-content display transforms outside the driver. Wire types must be generated from or checked against the Rust schema. Fixtures cover supported stdio requests, view updates, interleaved events, exact identity, labels, errors, and one unsupported-operation case. Listener allowlist fixtures arrive in stage 3b. Choose one package build/type/test command and add it to relevant CI and local checks under the [dependency plan](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds). Keep embedded runtime setup and remote pairing outside the common session operations.

Done when the interface and fixtures define the small baseline, its Rust view has a real stdio caller, and legacy callers retain their protocol. No socket framework or local Rust engine in the TypeScript client is required. Inventory existing tests so stage 2 adds binding/client evidence rather than duplicating agent tests.

### 2a. Validate the TypeScript client locally

Status: complete on the local host. Run `make check-agent-client`.

Drive the real `bravebot-rpc` process through the new adapter, with isolated home/project files and a controlled model backend. Reuse patterns from existing bridge process tests. Demonstrate a turn, explicit trust, supported approval/denial, cancellation, and truthful effects.

New evidence is correlation and interleaving through the typed client, the negotiated Rust view, rejection of runtimes without that capability, early events, split/combined frames, and fixture execution. Preserve current malformed-input and EOF behavior. Existing bridge tests remain the evidence for behavior they already pin.

Done when this passes on the local host machine without a mobile runtime, production credentials, desktop migration, or remote networking. Verify the new view through the real process and the unchanged legacy protocol through its existing callers. Reattachment and retry recovery remain later work.

### 2b. Prove the embedded React Native path (parallel track)

Reuse the app shell and package setup owned by stage 3d. Bring forward that shared setup if this track starts first; do not create a separate shell. Only the embedded binding and its proof belong to 2b. Apply the device UI validation and dependency checks when this stage first creates the app shell. Start with a model stub; real model access requires the [embedded credential lifecycle](architecture.md#embedded-model-credentials). Review the selected platform's tool support and distribution limits before advertising them. These on-device agent tool limits do not restrict remote host execution, which remains subject to host permissions and session authorization.

Build on the Android work described in [Current mobile status](README.md#current-mobile-status). At the start of this stage and later integration work that uses the branch, review progress on [android-shell](https://github.com/brave/bravebot/tree/android-shell), compare it with the current reference [`9635f16b`](https://github.com/brave/bravebot/tree/9635f16b49798624e6174b0d15818227773e81a8) (`9635f16b49798624e6174b0d15818227773e81a8`), and record the revision chosen for integration. Revisit relevant branch updates during mobile development to avoid duplicating work.

Inventory runtime bindings, lifecycle handling, input restrictions, mobile interactions, and relevant tests. Adapt the code and relevant checks needed for the selected proof to current APIs and security specs. Prefer cherry-picking or merging applicable commits to preserve contributor attribution; retain original author credit and source references for adaptations. At the pinned revisions, `android-shell` is 18 commits behind the main baseline. Bring the selected work onto current main and reconcile stage-3a bridge changes, including Android host handling of question IDs, startup trust, and the cancel turn parameter, before judging reuse cost. Do not assume it builds there. Validate builds and device behavior on the integrated revision. The existing `android-host.test.mjs` source checks and `compact-layout.test.mjs` layout/source checks do not cover JNI or Kotlin runtime behavior; add tests through the actual native and React Native boundaries. The Android integration is separate from this plan’s `main` baseline, so bringing it into the implementation is an explicit step. This reuse does not set an Android-first delivery requirement.

Build a minimal React Native screen and narrow platform native module reaching the Rust runtime. Do not prescribe Android before iOS: select the first device proof based on integration effort and available hardware, and record tested platforms and gaps. Android can build on the existing Rust/JNI work; iOS needs its own binding. Shared React Native code does not prove either platform’s native behavior. Use the same session interface and domain fixtures. The embedded proof is foreground-only with one app-private workspace, a turn, and a supported question or approval. Audit existing tools to select that path; do not bypass missing platform command confinement.

Done when the actual native/Rust path consumes the same Rust view as stdio and demonstrates its supported denial/cancellation and labelled-display semantics, and runtime loss reports interruption/uncertainty without replaying effects. Record the tool list, platform, distribution method, and whether the backend was a stub. A stub proof does not establish real credential support or store eligibility. Show “This device.” Model inference may still use a remote service. Keep the exercise bounded; full on-device capability does not block the remote track.

### 3a. Add exact bridge action targets

Use session-wide non-reused question IDs qualified by runtime/session, expected-turn cancellation, and a one-use awaiting-startup-trust state. A separate wire token is unnecessary unless an ambiguity remains. Reject repeated trust replies before acquiring the running state lock so they cannot overwrite trust or delay cancellation. Validate targets and apply decisions atomically with pending-state transitions. Update existing callers and capability negotiation explicitly; do not silently claim older clients have stronger protection.

Done when a unit-level controlled race and process-level delayed-action test prove that an old answer or cancellation cannot affect a later turn. Existing direct-stdio and desktop refusal behavior, plus any Android bridge code ported in stage 2b, must retain their regression coverage.

### 3b. Add the direct Rust local host

Add a listener mode or binary in `crates/ui-bridge` that owns the bridge library in-process. No supervisor or RPC child is needed. Add the host-instance identity, session registry, sessions-root-only storage override, private local socket, restricted method/field validation, and worker-completion signal. Keep dispatch separate from transport. Resolve the local control boundary in the architecture before enabling real-session control; same-user peer identity alone does not exclude approved tool processes. Experiments with synthetic data and a controlled model/tool stub may proceed within the stated limits. Keep stdio available for existing direct clients.

Provide the minimum control snapshot and subscription: active turn, closing state, pending startup trust, and supported questions. Connection transport can be exercised here, but reattached clients remain unable to control work until 3c passes.

Make terminal-event publication and the `finished` update one transition as observed by dispatch. An otherwise valid send immediately after the terminal event must be accepted if the session remains open and no competing turn starts. A simple reorder that still exposes a busy-state gap is insufficient; test the boundary with explicit synchronization. Define `closed` as confirmed worker termination, with save outcome unknown unless separately confirmed.

Done when the host persists across connections, reports truthful closing/closed state, enforces local access and input restrictions, and normal supported desktop/terminal list/resume paths cannot open prototype records. Do not change `HOME` as an unexamined shortcut for storage isolation. If isolation is insufficient, implement shared exclusion before exposure rather than weakening the ownership claim.

### 3c. Enable local controller readiness and loss handling

Use one controller, ordered local attach/detach, and answerable kinds fixed for each connection. Enable readiness only after control state and subscription are established. Coordinate question registration, answer consumption, refusal, and event publication so loss cannot approve an action or resurrect a resolved question.

Done when loss before and after each supported question refuses appropriately, future unavailable questions do not hang, and startup trust remains unanswered. Reattachment recovers current control state with stale actions rejected. Network heartbeats/leases wait until stage 6; local readiness and exact controller identity do not.

### 3d. Run a thin phone experiment

This stage owns the minimal React Native app shell, consumption of `packages/agent-client`, and the development transport. Complete the [device UI validation](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes) and [dependency checks](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds) before expanding the screens. Stage 2b reuses these deliverables and may bring their setup forward, but stage 3d does not depend on embedded execution.

Before fixing recovery details, exercise list/view/send/cancel and one approval. Bind the development listener only to loopback on an OS-assigned port and require a fresh per-launch token supplied by the test harness. Keep the token out of URLs and logs. Reach it only through an explicitly configured cable/emulator tunnel whose host endpoint remains loopback-only. Use synthetic data, a controlled model/tool stub, and a disposable session directory; run no private data or real tools. These controls do not resolve U1 or authorize real-session control. Any exposure beyond this development tunnel needs the stage-6 identity and transport protections first.

Done when the app consumes the Rust view through the shared adapter package, the listed operations run through the development transport, and recorded observations cover lock/background, disconnect, process recreation, input layout, and approval-field display. Record responsiveness, keyboard and scrolling behavior, accessibility checks, and untested platforms. Record device/OS details, gaps, and the resulting contract or recovery changes, including when no change is needed. Check loopback binding and missing/wrong-token rejection. This is a development experiment, not proof of secure remote control. Reuse its findings and screens in stage 7; stage 2b is not a dependency.

### 4a. Add bounded display recovery

Extend the shared Rust view from stages 1–2a for persistent-host recovery, using accepted prompts and bridge events. Include message IDs and approval resolutions. Add a bounded recent-history snapshot at one sequence boundary; mark omissions. Reconnect or any sequence gap requests a new snapshot, followed by ordered events. Defer incremental replay. Slow clients must not block execution. Never reopen a saved record to reconstruct a live session.

Done when proxy/barrier tests prove no missing or duplicate state at reconnect, labels survive live events and snapshots, and buffer overflow causes a defined resynchronization or refusal. Unlimited history and pagination are unnecessary.

### 4b. Prevent duplicate sends and recover pending messages

Record a per-session client message ID and request before dispatch. Add explicit status lookup independent of display-history retention. Keep outcomes for the live session's host-memory lifetime, with a finite budget that rejects new sends before evicting retry identities. No generic mutation journal or retry epochs. Retain explicit lookup and bounded ID retention: an accepted message can fall outside the display snapshot, so display history alone cannot settle an uncertain send.

Persist only the phone's pending send ID, text, target, and instance before transmission. Do not automatically retry other mutations or send to a new host instance. Unknown outcomes remain unknown; saved record identity does not imply persistence.

Done when a lost acknowledgement and client process recreation produce one turn, changed content under an existing ID is rejected, and host restart does not cause automatic re-execution. The budget limit must reject safely. Client-side durable pending-message storage may be completed with stage 7, before its recovery claim.

### 5a. Prove takeover with two TypeScript clients

Use the local test program as a second client. Prove one controller plus viewers, expected-epoch takeover, and rejection of delayed former-controller actions. Transfer pending questions only during explicit connected takeover; controller loss refuses them. The incoming client supplies its own answerable kinds. Busy sends retain drafts rather than queueing. This provides the handoff evidence needed for phone integration without depending on Electron. Include the typed host-local startup-trust screen in this client. Test that a phone cannot claim this capability and that its answer still requires the current local controller. Other host-only reply kinds stay unavailable until a capable host UI implements them.

### 5b. Attach the desktop

Renderer state integration may be the largest part of this stage. At its start, compare a main-process compatibility adapter against direct renderer consumption of the shared Rust view, and record the cost and test implications. Either approach consumes Rust-computed session state without recreating domain behavior in TypeScript. This work does not block stages 6–7, but remains required for full desktop handoff.

Integrate the shared client into a limited desktop prototype mode using the host service. Preserve the ordinary desktop path for unsupported features, and publish a capability matrix. Reuse stage-5a takeover. Scope Electron changes to the main-process transport adapter, application of shared Rust view updates for prototype sessions, and disabling unsupported renderer controls. Remove renderer-local prompt insertion/queue assumptions from that mode. Keep ordinary desktop behavior separate.

Done when desktop and a second local client see the same session, transfer control both ways, and delayed actions from the former controller fail. Closing a view does not close execution. A send during a turn returns busy and remains an unsent draft. Shared queues and concurrent control are not prerequisites for networking or mobile integration.

### 6. Enable authenticated networking

Write the short pairing/transport and effective-scope decision notes described in the architecture before enabling network access. Add encrypted transport, host identity verification, host-approved pairing, revocation, and scoped read/control authorization to the direct host. Test the adapter locally before exposing it to a phone. Add finite readiness expiry for half-open network connections and backgrounded clients.

Enforce the operation and reply-field allowlist from the contract. Host-only trust/scope-expanding answers remain unavailable on the phone. Determine effective scope from the agent's resolved rules, modes, trust, programs, directories, and exposure state. A device authorized for an earlier scope must not see or control a widened one without explicit reauthorization. Resolve private-display destination authorization against policy before transmitting content.

Done when unauthorized, revoked, wrong-host, viewer, stale-controller, and out-of-scope calls cause no protected disclosure or effect. Test direct bypass attempts, not only UI controls. Tailscale Serve is a development reachability candidate, not the application authorization system.

### 7. Connect the React Native remote adapter

Use the stage-3d phone experiment and Rust view/display helpers through the thin client; reuse stage-2b screens where available without making that track a dependency. Share transport and UI wiring where useful; native views need their own label/approval tests. Add pairing, protected pending-message storage, target selection, and connection/controller status. Remote-only use does not start the embedded runtime; never fall back to local execution automatically.

Done when the phone starts host work, takes control from the second TypeScript client, answers supported approvals, cancels, and returns control. Repeat with Electron when stage 5b is ready; desktop handoff remains required for the full demo. Test phone process recreation after host acceptance but before acknowledgement using the persisted message ID. Test keyboard/layout and full approval details on hardware.

Keep the phone foregrounded when demonstrating approvals. Also lock/background it deliberately and show that unavailable questions are refused, not silently approved or promised for later. Returning to the app shows the actual resulting state.

Choose a demo task that does not require phone approval of command-output admission, or demonstrate explicit desktop takeover before that question. The phone cannot authorize “let the model read this output”; a run-tests-then-read-failures loop needs host handling. If no capable controller is ready, the question is refused rather than held for later.

### 8. Demonstrate away-from-desktop use

Test away from local Wi-Fi and across Wi-Fi/mobile-data changes. Show the same host-owned session on desktop and phone, explicit takeover, and recovery while the host remains alive. Host sleep/restart shows unavailable/interrupted state and no unsupported continuation claim.

Record device/OS versions and tested platforms. Done when stage 5b and the required acceptance rows below have recorded evidence. The overall initial result includes this remote demonstration and the bounded embedded proof. Later on-device work should target useful tasks and their actual platform permissions, without requiring desktop command parity.

## Test-driven development

Use red/green development for each small behavior added or changed in these stages. Apply `testing-preflight` before choosing tests, and revisit it against the final diff.

1. Define the observable result from the governing spec or task, read nearby tests, and name a plausible incorrect implementation that existing tests could miss. Choose a fixture and assertion that distinguish that mistake from the required result.
2. **Red:** write the focused test before implementing the behavior. Run it and confirm it fails for the intended behavioral reason. A compile error, setup failure, or unrelated timeout does not establish that the test detects the fault.
3. **Green:** implement enough to satisfy the behavior and pass the test. Preserve existing guarantees and assertions.
4. **Refactor:** improve the code while keeping tests green, then run the relevant integration, build, type, and repository checks for affected callers. A unit test alone does not prove that each adapter or UI uses the shared code correctly.

For new or changed regression tests, demonstrate failure against the old behavior when feasible. For security boundaries, cancellation, concurrency, or usage accounting, select a plausible fault and verify that the relevant test catches it. A narrow temporary mutation can supply this evidence. Preserve the exact original contents, use an isolated copy or scoped reversible edit, restore the code, check for leftover mutations, and rerun the focused test. Do not add a mutation framework or a quota of experiments.

If a meaningful failure demonstration is unsafe or impractical, explain why and record the remaining uncertainty. Report whether protection was demonstrated, reasoned only, or not run, together with final check results and uncovered paths. Repeat affected checks if later changes invalidate the evidence.

Work one behavior at a time, including behavior discovered during implementation. The acceptance table guides this work; it does not require writing the entire test suite before coding. Reuse existing evidence where it applies, and add tests at materially different changed boundaries. Documentation-only edits need document and repository checks, not invented runtime tests.

## Verification and repository checks

Apply `testing-preflight` before behavior or assertion changes. Apply `design` for governed visible `ui/` work, and carry its relevant approval/label quality requirements into the React Native surfaces. Use the repository's required final-diff checks. The first-block evidence is recorded in the [client contract](client-contract.md#implemented-rust-block).

Use a transport fault proxy plus a model stub with controlled gates for observable connection faults. Use narrow in-process barriers for atomic target and snapshot races that external proxies cannot force. Assert file bytes, invocation counts, and captured model requests, not screenshots alone. Screenshots and physical-device logs supplement automated evidence.

When shared bridge/confirmer code changes, retain the existing refusal and direct-stdio EOF assertions. Run `cargo test -p bravebot-ui-bridge`, applicable Electron refusal tests, and `make check-spec`, plus the repository checks required by the final diff. Fix confirmed regressions rather than adapting assertions to hide changed guarantees.

Review governing `labels.md`, `layering.md`, `prompting.md`, `sessions.md`, and `network-egress.md` as applicable. Add a host/client spec if needed; include behavior, tests, clause counts, and `verified-by` references together. A native mobile label test supplements the desktop marking tests; those DOM tests do not prove React Native rendering.

## Additional focused checks

Add these checks when the named boundary is implemented, using red/green evidence rather than expanding the prototype’s feature scope:

| Stage | Check |
|---|---|
| 1–2a, 4a | Shared Rust view preserves busy/pending/label state across interleaved events and snapshot boundaries; adapters apply it without a second domain reducer. Model stubs distinguish planner requests from legitimate processor/vetting requests; a deliberate sentinel leak must fail the planner check. |
| 2b, 3d, 7 | Shared display fixtures cover forged markings, control/bidirectional characters, and long lines. Each approval field is displayed or explicitly excluded with a reason. Native UI tests verify the actual marking and layout. |
| 3a–3b | Repeated trust replies cannot overwrite trust or stall cancellation. Exercise turn-end/start ordering: a valid send immediately after the terminal event is accepted without a transient busy error when no other turn competes and the session remains open. Test worker completion independently of saving. |
| 3b, 6 | Classify every bridge method and reply kind in a default-deny allowlist; fail on unclassified additions. Test oversized/unterminated frames, connection floods, and finite pre-authentication limits. |
| 3b–4a | On a real socket, a consumer that stops reading cannot stall cancellation, other sessions, or turn execution. Test the selected local control boundary against a tool process attempting approval. |
| 3–8 | Check forbidden effects through file bytes, marker files, fetch counters, and classified model requests, not only error replies. Exercise cancel/close/host exit while a real tool runs before claiming tool-lifetime behavior. |

For changed bridge callers, run the applicable `ui/scripts/drive-*.mjs` approval/cancellation scenarios as well as Rust tests. Record existing tests reused, new coverage, device-only checks, and any gaps. Apply approval/label UI quality requirements to React Native regardless of its directory.

## Required acceptance evidence

Each row applies when its named milestone is enabled; the table is not a prerequisite for starting the local client. These are the initial checks for the chosen scope, not every possible failure case. Add or revise checks when implementation reveals behavior or risks that the plan did not foresee. Keep evidence for the security boundaries of each enabled feature.

| Milestone | Scenario and required evidence |
|---|---|
| 1–2a | Existing behavior and the additive Rust view are mapped to fixtures. Capability negotiation rejects an older runtime; legacy callers retain their behavior. Early events, split/combined frames, response correlation, answerable/unanswerable malformed input, and EOF behave as specified through the new typed client. |
| 2a and 2b | Approval versus denial produces the expected real effect; cancellation grants no pending approval. Embedded tests cross the actual native module and consume the shared Rust view. |
| 2b with real credentials | Native test credentials survive only the specified lifecycle; lock, invalidation, deletion, restore, endpoint changes, and log/planner exclusion match the architecture. Record the issuer revocation test. |
| 2b, 3d and distribution | Record the device UI validation, dependency checks, supported tools and platforms. Developer installation does not prove TestFlight/App Store eligibility; review both embedded tools and remote access before distribution. |
| 2b and 7 | Labelled sentinel content reaches the phone display and stays out of captured planner input through non-promoting decisions. Target identity and permissions cannot cross between modes. |
| 3a | Unit-level target race and delayed old-turn reply/cancel leave the later turn untouched. |
| 3b | Private socket/directory permissions and unauthorized-peer refusal are tested using available OS facilities; this does not cover same-user tool processes or resolve U1. Unsafe socket paths and forbidden methods/fields cause no dispatch. |
| 3b | Ordinary supported terminal/desktop discovery and explicit resume cannot open prototype records. Closure keeps ownership while a worker can still save. |
| 3c | Recover trust/control state before readiness. Lose control before/after question registration and race a reply; no late event or action reverses refusal. |
| 4a | Snapshot/subscription boundary and gap resynchronization preserve authoritative state and labels within defined bounds; omissions are visible and slow clients do not stall execution. |
| 4b | Lost send acknowledgement produces one turn; changed payload under one ID fails; explicit lookup works after display eviction; capacity exhaustion does not forget retry IDs. |
| 4b and 7 | Persist send identity before transmission. Kill/recreate the phone client after host acceptance and reconcile without a duplicate effect. Restarted-host outcomes remain unknown. |
| 5a–5b | One controller plus viewers. Host-local startup trust uses that controller state; phone capability claims cannot enable host-only replies. Explicit takeover invalidates previous-controller actions; viewers cannot mutate; busy send is not queued. |
| 6 | Unauthorized, revoked, wrong-host, out-of-scope, and forbidden-field requests disclose nothing protected and have no effects. Include `remember`, attachments, arbitrary directories, and raw-dispatch attempts. |
| 6 | Changed effective grants suspend remote access until authorized. Pairing never answers trust. Destination authorization is explicit before private content leaves the host. |
| 7–8 | Foreground approvals work; background/lock refuses unavailable questions. Network changes recover actual state; host sleep/restart never claims continued or saved work without evidence. |
| Persistence | Force save failure after an observed effect. Live completion remains truthful and saved progress remains unknown. |

## Deferred work and open implementation choices

Optional follow-ons are shared queues, concurrent controllers, shared-store record locks/import, complete history, automatic host startup/restart, generic mutation retries, notifications/bounded approval waiting, broader on-device tools, and offline inference. None blocks the bounded prototype.

Routine choices left to implementation include listener naming, local socket library, network library, schema spellings, buffer/submission budgets, heartbeat duration, pairing-code presentation, and protected mobile storage APIs. Pairing identity/security and effective-scope policy require the stage-6 decision notes. Record these with tests when they become relevant. Revise the architecture and proposed behavior when implementation evidence warrants it. Minor gaps are expected; they do not require predicting every remaining case before coding.
