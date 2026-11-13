# Mobile prototype implementation plan

This is a high-level starting plan, not an exhaustive account of edge cases or behavior. Expect implementation discoveries to change or add to it. Update the affected design, specs, and tests as those decisions are made; resolve security gaps before enabling the affected feature. See the [executive summary](executive-summary.md) for the full proposal in one document.

## Scope and delivery rule

The [decision record](decisions-and-tradeoffs.md) holds settled choices, costs, and revisit criteria. [Open questions](open-questions.md) tracks unresolved decisions and validation evidence by capability. U1 gates real-session control in stage 3b; U2 and U3 gate network access in stage 6. They do not block stages 1–2a. Earlier network exposure also requires U2.

Build a React Native mobile client for both embedded and remote execution. Remote control is the first substantial end-to-end prototype. Validate the common interface on the local host machine first, then run the bounded embedded proof as a parallel track alongside host work. Stage 2b does not gate stages 3–7; its result remains part of overall prototype completion.

The full remote demo uses one user, one host, one explicitly prepared workspace, fresh host-owned sessions in an isolated store, and every attached device able to send, answer and cancel, with no takeover step. Desktop and phone see the same session at once. The phone can start work, view progress, send a turn when idle, answer supported approvals, and cancel. A busy turn rejects another send; the UI retains a draft.

Do not require shared queues, normal saved-session import, a supervisor process, automatic restart, generic mutation retries, complete retained history, desktop parity, attachments, file browsing, bots, forks, watches, manifests, settings/hooks editing, public relays, or push. Broader on-device tools and background execution are follow-on work. Host workspace trust is answered on the host, never inferred from pairing.

Keep security boundaries and test evidence precise. Choose concrete names, libraries, limits, and timeouts during implementation. Record minor gaps and explicit non-claims rather than designing a new subsystem for every hypothetical failure. Stop only when an enabled capability would violate its security or correctness contract.

## First secure remote checkpoint

The first deliverable is the local client proof in stages 1–2a. The first secure remote checkpoint then comprises 3a–3d, 4a–4b, 5a, 6, and the narrow phone path in 7. Demonstrate one prepared workspace, fresh sessions, one tested phone platform, a supported approval, denial/cancel, reconnect, and duplicate-send prevention. A second local test client, attached to the same session as the phone, proves that both see and act on it. This checkpoint does not claim desktop attachment, embedded execution, or away-from-desktop validation from stage 8.

The local test client supplies the typed startup-trust interaction on the host before the phone joins the session. It uses the same U1-protected channel as other local clients, with no extra approval channel. Choose a task that needs no later host-only question until a capable host UI is available. Unavailable questions are refused; pairing never supplies the trust answer.

Defer 2b and 5b if they delay this checkpoint. They remain required for the full two-mode demonstration. Keep the first app to the screens and tools that prove this path; device evidence determines whether to extend it.

## Checkpoints and scope cuts

At each checkpoint, record what has been demonstrated, remaining work, and unresolved questions before choosing the next deliverable.

| Work | Checkpoint and useful stopping point |
|---|---|
| 1–2a: Rust view and local client | Real RPC turn, approval/denial, cancellation, and view updates through the thin client. Stop here with a tested local interface if later work is delayed. |
| 3a–3c: targets and local host | Exact targets, ownership, worker completion, and client loss. Without U1, keep listener work synthetic. |
| 3d: phone experiment and UI validation | One React Native transcript/composer/approval flow, with responsiveness, input, accessibility, and lifecycle evidence. Validate these screens before expanding the UI. |
| 4a–4b and 5a: recovery and two clients on one session | Snapshots, send lookup, and two clients acting on one session survive forced connection loss. |
| 6–7: secure remote phone control | One platform, one prepared workspace, scoped pairing, supported approvals, and phone process recovery. |
| 8: away-from-desktop evidence | Network changes and host loss, with 5b complete for the desktop claim. |
| 2b: bounded embedded proof | One platform, one supported tool/question, stub backend first. Real credentials require the architecture's storage and revocation checks. |
| 5b: desktop attachment | The supported desktop subset consumes the Rust view and acts on the same session as another client. |

Start U1 investigation during the local client work. Record a tested boundary or the reason real-session control remains blocked. Assess U2/U3 at stage 6 and resolve them before implementing real-session networking. Record unresolved credential-provider and iOS distribution questions at their required checkpoints; do not claim completion without the required evidence.

The dependency order is 1–2a, then 3a–3c, then 3d, then 4a–4b and 5a, then 6–7. Stage 2b can proceed alongside the host work and share the 3d app shell. Stage 5b follows 5a and can proceed alongside networking. Stage 8's full desktop demonstration needs 5b. These are behavior milestones, not a prescribed number of pull requests; keep each implementation change useful, tested, and independently reviewable.

When reducing scope, use this order:

1. Keep one tested phone platform, one workspace, and the smallest complete approval flow; defer additional screens, tools, and platforms. Unsupported capabilities must be visible and refused.
2. Stop at the [first secure remote checkpoint](#first-secure-remote-checkpoint), deferring 2b and 5b. Preserve its host setup path, refusal behavior, and recovery evidence.
3. If U1–U4 remain unresolved, stop at the local proof or restricted synthetic phone experiment. Keep real sessions blocked until those boundaries are resolved.

Never cut authentication, scope checks, labels, complete approval details, exact targets, one running turn per session, approvals answered once, or the recovery tests for a capability still advertised. Full prototype completion still requires 2b, 5b, and stage 8 evidence. A store release is a separate decision governed by the [iOS limits](architecture.md#ios-capability-and-distribution-limits).

## Stages

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

The Android Rust/JNI binding on `android-shell` may be a starting point; see [Current mobile status](README.md#current-mobile-status).

Inventory runtime bindings, lifecycle handling and input restrictions. Bring any selected binding onto current main and reconcile it with the stage-3a bridge changes (question IDs, startup trust, the cancel turn parameter) before judging reuse cost. Do not assume it builds there. Validate builds and device behavior on the integrated revision, and add tests through the actual native and React Native boundaries. Reuse does not set an Android-first delivery requirement.

Build a minimal React Native screen and narrow platform native module reaching the Rust runtime. Do not prescribe Android before iOS: select the first device proof based on integration effort and available hardware, and record tested platforms and gaps. Android can build on the existing Rust/JNI work; iOS needs its own binding. Shared React Native code does not prove either platform’s native behavior. Use the same session interface and domain fixtures. The embedded proof is foreground-only with one app-private workspace, a turn, and a supported question or approval. Audit existing tools to select that path; do not bypass missing platform command confinement.

Done when the actual native/Rust path consumes the same Rust view as stdio and demonstrates its supported denial/cancellation and labelled-display semantics, and runtime loss reports interruption/uncertainty without replaying effects. Record the tool list, platform, distribution method, and whether the backend was a stub. A stub proof does not establish real credential support or store eligibility. Show “This device.” Model inference may still use a remote service. Keep the exercise bounded; full on-device capability does not block the remote track.

### 3a. Add exact bridge action targets

Status: implemented in the bridge and the TypeScript client (RPCVIEW-6). Question numbers come from a counter the session owns, `turn.cancel` accepts the target of the turn or run it is for, `trust.reply` is taken once, and `capabilities.actionTargets` advertises all three. The TypeScript client names the target when the runtime advertises it, and claims nothing for a runtime that does not. Stage 2b is deferred, so the Android bridge code it would have ported is not a constraint here.

Use session-wide non-reused question IDs qualified by runtime/session, expected-target cancellation, and a one-use awaiting-startup-trust state. A separate wire token is unnecessary unless an ambiguity remains. Reject repeated trust replies before acquiring the running state lock so they cannot overwrite trust or delay cancellation. Validate targets and apply decisions atomically with pending-state transitions. Update existing callers and capability negotiation explicitly; do not silently claim older clients have stronger protection.

Done when a unit-level controlled race and process-level delayed-action test prove that an old answer or cancellation cannot affect a later turn. Existing direct-stdio and desktop refusal behavior, plus any Android bridge code ported in stage 2b, must retain their regression coverage.

### 3b. Add the direct Rust local host

Status: built (`bravebot-host`, [HOST](../../specs/local-host.md)). It covers a host instance ID and a registry of sessions by owning connection, a sessions-root-only override, local clients that the host launches over anonymous socket pairs with a `launch`/`quit` command on the host's input as the person's way to start the next one, an allowlist of methods and parameters (every bridge method is offered or denied by name, enforced by a test), a worker-completion signal behind `closing` and `closed`, and the Linux non-dumpable outcome consumed at startup and reported in `agent.info`. Not built: application operation names and workspace IDs in place of bridge method names and directories (stage 6), and any caller in the product. The terminal-event and `finished` transition was already one step under the emitter lock (RPCVIEW-2). That normal terminal and desktop list and resume paths cannot open the host's records is shown for the desktop bridge (`session.list`, `session.open`) against a running host. For the terminal (`sessions list`, `--resume`) it is shown with a record placed in a root with the host's layout inside the same state directory, without running the host.

Add a listener mode or binary in `crates/ui-bridge` that owns the bridge library in-process. No supervisor or RPC child is needed. Add the host-instance identity, session registry, sessions-root-only storage override, host-launched local clients over anonymous socket pairs (`control_link`, RPCVIEW-8) with no named local endpoint, restricted method/field validation, and worker-completion signal. Keep dispatch separate from transport. The [local control boundary](architecture.md#local-control-boundary) is decided for host-launched clients; its tool-path test runs against the host process on macOS, and before enabling real-session control it must also pass on Linux. Experiments with synthetic data and a controlled model/tool stub may proceed within the stated limits. Keep stdio available for existing direct clients.

Provide the minimum control snapshot and subscription: active turn, closing state, pending startup trust, and supported questions. Stage 3c builds it. Connection transport can be exercised here.

Read the outcome of the Linux non-dumpable mark, report it in `agent.info`, and treat a failure as the channel not being safe before enabling real-session control; check that host supervision on Linux does not depend on the bridge's `/proc/<pid>` entries (see [U1](open-questions.md#u1-local-control-channel-boundary)).

Make terminal-event publication and the `finished` update one transition as observed by dispatch. An otherwise valid send immediately after the terminal event must be accepted if the session remains open and no competing turn starts. A simple reorder that still exposes a busy-state gap is insufficient; test the boundary with explicit synchronization. Define `closed` as confirmed worker termination, with save outcome unknown unless separately confirmed.

Done when the host persists across connections, reports truthful closing/closed state, enforces local access and input restrictions, and normal supported desktop/terminal list/resume paths cannot open prototype records. Do not change `HOME` as an unexamined shortcut for storage isolation. If isolation is insufficient, implement shared exclusion before exposure rather than weakening the ownership claim.

### 3c. Enable local controller readiness and loss handling

Status: built in the host ([HOST-7](../../specs/local-host.md#HOST-7), [HOST-10](../../specs/local-host.md#HOST-10), [HOST-11](../../specs/local-host.md#HOST-11)). A session has one controller, the connection that opened it or took it over with `session.attach`. A client can answer only after it has the view of the session, and only the kinds the allowlist offers replies for. A question nobody can answer is refused when it is asked, for every kind. When the controller's connection ends, the waiting question is refused and shown as resolved, and the turn goes on, so a later client can take the session over with `session.attach` and receives its status, waiting question and most recent rows (HOST-11). A stale answer fails on its question number, and trust stays unanswered until a controller answers it. The TypeScript client lists the host's sessions with `instance()` and takes one over with `attach()`, starting its view from the snapshot ([RPCVIEW-12](../../specs/session-view.md#RPCVIEW-12)). Not built: more than one client attached to a session at once, which is stage 5a. Answerable kinds are the same for every local connection, set by the host and not by what a client advertises. The host serves one client at a time, so no test has two connections open at once. The guard that refuses a live controller's session is tested on the host's own state.

Use one controller, ordered local attach/detach, and answerable kinds fixed for each connection. Enable readiness only after control state and subscription are established. Coordinate question registration, answer consumption, refusal, and event publication so loss cannot approve an action or resurrect a resolved question.

Done when loss before and after each supported question refuses appropriately, future unavailable questions do not hang, and startup trust remains unanswered. Reattachment recovers current control state with stale actions rejected. Network heartbeats/leases wait until stage 6; local readiness and exact controller identity do not.

### 3d. Run a thin phone experiment

Status: the app shell and session screens are built in `mobile/` ([app-shell.md](app-shell.md)), with a `Target` interface, approval cards and `make check-mobile` in CI. The development transport is built: `bravebot-host` runs the relay in `packages/agent-client/scripts/dev-relay.ts` as its client, which listens on 127.0.0.1 on a port the system picks and admits one WebSocket connection whose first message is the six-digit code it printed for that launch. A development build of the app connects to it from the blank slate. Tests in `packages/agent-client` run the real host and relay with a stub model, and check a session with trust, an approved write and a reply, wrong-code rejection, one connection per launch, and either end closing. A debug build on an Android 17 emulator with a large screen has been run by hand against the relay demo: a wrong code was refused with the message for it and the host kept running, the next launch's code connected, the trust question, a reply and an approved write that landed on disk worked, and the host quitting returned the app to the form with the reason. Not done: the device observations listed below, a phone-sized screen, the iOS simulator and a physical device. The relay closes a connection that sends no code after a minute, and no test waits for that.

This stage owns the minimal React Native app shell, consumption of `packages/agent-client`, and the development transport. Complete the [device UI validation](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes) and [dependency checks](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds) before expanding the screens. Stage 2b reuses these deliverables and may bring their setup forward, but stage 3d does not depend on embedded execution.

Before fixing recovery details, exercise list/view/send/cancel and one approval. Bind the development listener only to loopback on an OS-assigned port and require a fresh per-launch token supplied by the test harness. Keep the token out of URLs and logs. Reach it only through an explicitly configured cable/emulator tunnel whose host endpoint remains loopback-only. Use synthetic data, a controlled model/tool stub, and a disposable session directory; run no private data or real tools. These controls do not resolve U1 or authorize real-session control. Any exposure beyond this development tunnel needs the stage-6 identity and transport protections first.

Done when the app consumes the Rust view through the shared adapter package, the listed operations run through the development transport, and recorded observations cover lock/background, disconnect, process recreation, input layout, and approval-field display. Record responsiveness, keyboard and scrolling behavior, accessibility checks, and untested platforms. Record device/OS details, gaps, and the resulting contract or recovery changes, including when no change is needed. Check loopback binding and missing/wrong-token rejection. This is a development experiment, not proof of secure remote control. Reuse its findings and screens in stage 7; stage 2b is not a dependency.

### 4a. Add bounded display recovery

Extend the shared Rust view from stages 1–2a for persistent-host recovery, using accepted prompts and bridge events. Include message IDs and approval resolutions. Add a bounded recent-history snapshot at one sequence boundary; mark omissions. Reconnect or any sequence gap requests a new snapshot, followed by ordered events. Defer incremental replay. Slow clients must not block execution. Never reopen a saved record to reconstruct a live session.

Done when proxy/barrier tests prove no missing or duplicate state at reconnect, labels survive live events and snapshots, and buffer overflow causes a defined resynchronization or refusal. Unlimited history and pagination are unnecessary.

Status: built ([RPCVIEW-9](../../specs/session-view.md#RPCVIEW-9), [HOST-8](../../specs/local-host.md#HOST-8), [HOST-11](../../specs/local-host.md#HOST-11)). The view keeps the 256 most recent rows and the pending question apart from them. `session.attach` and `session.view.resync` send a snapshot of them at one sequence, with the count of rows left out. The TypeScript client asks for a snapshot after a sequence gap, refuses actions until it arrives, and keeps the same bound. The rows kept are also bounded to 4 MiB serialized, and each update says how many rows from the first are no longer kept, which the client follows. The host queues 1024 lines, or 16 MiB, per connection on a writer thread of its own, so a slow client holds up no turn, and a full queue ends the connection, after which the client takes its sessions over again. Rows are identified by their view ids. Message ids a client supplies are stage 4b. The mobile transcript says above its first row how many rows are no longer kept.

### 4b. Prevent duplicate sends and recover pending messages

Record a per-session client message ID and request before dispatch. Add explicit status lookup independent of display-history retention. Keep outcomes for the live session's host-memory lifetime, with a finite budget that rejects new sends before evicting retry identities. No generic mutation journal or retry epochs. Retain explicit lookup and bounded ID retention: an accepted message can fall outside the display snapshot, so display history alone cannot settle an uncertain send.

Persist only the phone's pending send ID, text, target, and instance before transmission. Do not automatically retry other mutations or send to a new host instance. Unknown outcomes remain unknown; saved record identity does not imply persistence.

Done when a lost acknowledgement and client process recreation produce one turn, changed content under an existing ID is rejected, and host restart does not cause automatic re-execution. The budget limit must reject safely. Client-side durable pending-message storage may be completed with stage 7, before its recovery claim.

The message ID is required, not optional. An optional ID would leave a caller that omits it without protection, and nothing would fail when one stopped sending it. Every `turn.send` caller is in this repository and the desktop app runs the bridge it ships with, so all of them change together. Build 4b in three steps:

1. **Require the ID.** `turn.send` refuses a request without a `messageId`. Every caller sends a new ID per action: the desktop composer, the desktop bot turns, the ACP adapter and the TypeScript client. Tests get a shared helper. The specs and the protocol doc describe the field. Nothing is recorded or deduplicated yet.
2. **The send record.** Deduplication, the send states, the lookup method, the budget, and what a lookup answers after a host restart.
3. **TypeScript client recovery.** After a reconnect, look up a send whose outcome is unknown, and retry it under the same ID. This needs the client to reconnect first, so it is built in two parts:
   1. **Take over the session.** `instance()` lists the host's run and the sessions it holds. `attach(id)` takes over a session whose controller's connection ended and starts its view from the snapshot the host sends.
   2. **Settle the send.** `messageStatus` becomes a call of `turn.lookup`. A send cut off by a lost connection reports its outcome as unknown together with its message id, so a caller can keep it. After the next connection, the caller calls `instance()` and `attach()`. If the instance is the one the send went to, it looks the id up: accepted or refused settles the send, except that a refusal with `internal` may have partly run the turn, and only `not_received` allows a resend, under the same id and text. A changed instance, `no_such_session`, a failed attach or a refused lookup leaves the outcome unknown, and nothing is sent again. The client never resends on its own; the caller asks. Keeping the pending send across a restart of the phone app is stage 7.

Status: steps 1 and 2 are built ([RPCVIEW-10](../../specs/session-view.md#RPCVIEW-10), [RPCVIEW-11](../../specs/session-view.md#RPCVIEW-11)). The bridge records each send's answer under its id with a digest of its parameters, answers a repeat from the record, refuses other parameters under a recorded id, and refuses a new id once a session holds 4096 records. `turn.lookup` answers accepted, refused or not received, and an unknown handle is refused, which a client takes as unknown. A lookup says whether a turn started, not how it ended. Step 3.1 is built ([RPCVIEW-12](../../specs/session-view.md#RPCVIEW-12)). Step 3.2 is built in the client ([RPCVIEW-13](../../specs/session-view.md#RPCVIEW-13)): a cut-off send fails with `UnknownSendError` carrying its id, `messageStatus` calls `turn.lookup`, and `resend` is allowed once after each `not_received` answer. Not built: the mobile app keeping a pending send and calling these after it reconnects, and keeping it across a restart of the app, which is stage 7.

### 5a. Attach two TypeScript clients to one session

Use the local test program as a second client, launched by the host, under the [shared control rules](architecture.md#shared-control-across-devices). Both clients attach to the same session and see the same view, approval requests included. Either can send, answer and cancel at any time, with no takeover step. This provides the evidence needed for phone integration without depending on the desktop app.

- The first answer to a question counts; the other client's answer fails on its question number. A send while a turn runs is refused and kept as a draft. A cancel names its target.
- The host records which client made each send, answer and cancel. The view shows the client that acted last, and the trace records each action's client.
- When one client leaves, the other keeps the session and any question it can answer. A question is refused only when no attached client can answer it.
- Each client answers only the kinds the host fixes for its connection (HOST-10). Include the typed host-local startup-trust screen in the test client, and test that a phone cannot claim it. Other host-only reply kinds stay unavailable until a capable host UI implements them.
- The view's last acting client is enough for a client to add a "Take control from Desktop?" confirmation later without a host change ([decision 5](decisions-and-tradeoffs.md#5-every-attached-device-can-act-with-no-takeover-step)).

This replaces the one controller per session the host has now. Change HOST-7, HOST-10 and HOST-11 with the code.

Status: the TypeScript client takes over a session whose controller's connection ended (step 3.1 of 4b). Not built: the host serving more than one client at once, several clients attached to one session, recording which client acted, the second local test client, and the startup-trust screen in it. `takeControl` still fails locally; 5a removes it, because there is no takeover step.

### 5b. Attach the desktop

Renderer state integration may be the largest part of this stage. At its start, compare a main-process compatibility adapter against direct renderer consumption of the shared Rust view, and record the cost and test implications. Either approach consumes Rust-computed session state without recreating domain behavior in TypeScript. This work does not block stages 6–7, but remains required for the full desktop demonstration.

Integrate the shared client into a limited desktop prototype mode using the host service. Preserve the ordinary desktop path for unsupported features, and publish a capability matrix. Reuse stage-5a shared control. Scope desktop changes to the main-process transport adapter, application of shared Rust view updates for prototype sessions, and disabling unsupported renderer controls. Remove renderer-local prompt insertion/queue assumptions from that mode. Keep ordinary desktop behavior separate.

Done when desktop and a second local client see the same session, each can send, answer and cancel, and the view shows which one acted last. Closing a view does not close execution. A send during a turn returns busy and remains an unsent draft. Shared queues are not a prerequisite for networking or mobile integration.

### 6. Enable authenticated networking

Write the short pairing/transport and effective-scope decision notes described in the architecture before enabling network access. Add encrypted transport, host identity verification, host-approved pairing, revocation, and scoped read/control authorization to the direct host. A lost device's pairing can be revoked on the host. Test the adapter locally before exposing it to a phone. Add finite readiness expiry for half-open network connections and backgrounded clients.

Enforce the operation and reply-field allowlist from the contract. Host-only trust/scope-expanding answers remain unavailable on the phone. Determine effective scope from the agent's resolved rules, modes, trust, programs, directories, and exposure state. A device authorized for an earlier scope must not see or control a widened one without explicit reauthorization. Resolve private-display destination authorization against policy before transmitting content.

Done when unauthorized, revoked, wrong-host, stale-target, and out-of-scope calls, and sends, answers, cancels and closes from a read-only grant, cause no protected disclosure or effect. Test direct bypass attempts, not only UI controls. Tailscale Serve is a development reachability candidate, not the application authorization system.

### 7. Connect the React Native remote adapter

Use the stage-3d phone experiment and Rust view/display helpers through the thin client; reuse stage-2b screens where available without making that track a dependency. Share transport and UI wiring where useful; native views need their own label/approval tests. Add pairing, protected pending-message storage, target selection, connection status, and the device that acted last. Remote-only use does not start the embedded runtime; never fall back to local execution automatically.

Done when the phone starts host work, joins a session the second TypeScript client started, answers supported approvals and cancels, and the test client shows each of those actions and that the phone made it. Repeat with the desktop app when stage 5b is ready; desktop attachment remains required for the full demo. Test phone process recreation after host acceptance but before acknowledgement using the persisted message ID. Test keyboard/layout and full approval details on hardware.

Keep the phone foregrounded when demonstrating approvals. Also lock/background it deliberately. With no other attached client able to answer, show that unavailable questions are refused, not silently approved or promised for later. With the test client attached, show that the question stays open there and the phone cannot answer it. Returning to the app shows the actual resulting state.

Choose a demo task that does not require phone approval of command-output admission, or answer that question on the desktop. The phone cannot authorize “let the model read this output”; a run-tests-then-read-failures loop needs host handling. If no capable client is ready, the question is refused rather than held for later.

### 8. Demonstrate away-from-desktop use

Test away from local Wi-Fi and across Wi-Fi/mobile-data changes. Show the same host-owned session on desktop and phone, actions from each device shown on the other, and recovery while the host remains alive. Host sleep/restart shows unavailable/interrupted state and no unsupported continuation claim.

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
| 3b | The hook and `run` paths of the host process, once it exists, leave a detached process that reaches no channel and changes nothing the client reads or sends (RPCVIEW-8), on each supported OS; the stdio bridge and `bravebot-host` are tested on macOS and Linux. Forbidden methods/fields cause no dispatch. |
| 3b | Ordinary supported terminal/desktop discovery and explicit resume cannot open prototype records. Closure keeps ownership while a worker can still save. |
| 3c | Recover trust/control state before readiness. Lose control before/after question registration and race a reply; no late event or action reverses refusal. |
| 4a | Snapshot/subscription boundary and gap resynchronization preserve authoritative state and labels within defined bounds; omissions are visible and slow clients do not stall execution. |
| 4b | Lost send acknowledgement produces one turn; changed payload under one ID fails; explicit lookup works after display eviction; capacity exhaustion does not forget retry IDs. |
| 4b and 7 | Persist send identity before transmission. Kill/recreate the phone client after host acceptance and reconcile without a duplicate effect. Restarted-host outcomes remain unknown. |
| 5a–5b | Two clients attached to one session see the same view and each can send, answer and cancel. The first answer counts and the second fails; a busy send is not queued; one client leaving refuses no question the other can answer. Each action records the client that made it. Host-local startup trust is answered only on a connection the host lets answer it; phone capability claims cannot enable host-only replies. |
| 6 | Unauthorized, revoked, wrong-host, out-of-scope, and forbidden-field requests, and read-only-grant sends, answers, cancels and closes, disclose nothing protected and have no effects. Include `remember`, attachments, arbitrary directories, and raw-dispatch attempts. |
| 6 | Changed effective grants suspend remote access until authorized. Pairing never answers trust. Destination authorization is explicit before private content leaves the host. |
| 7–8 | Foreground approvals work; a locked or backgrounded phone cannot answer, and a question no attached client can answer is refused. Network changes recover actual state; host sleep/restart never claims continued or saved work without evidence. |
| Persistence | Force save failure after an observed effect. Live completion remains truthful and saved progress remains unknown. |

## End goal beyond the prototype

The end goal is one host that runs sessions and several clients of them at once: the desktop app, and Android, iOS and iPad apps. A person sends a prompt from a phone, opens an iPad, and sees the same session there with the latest prompt, tool calls and results. The terminal UI is not one of these clients ([sessions the terminal UI runs](#sessions-the-terminal-ui-runs)). The bounded snapshot (256 rows, 4 MiB) is enough to show the most recent work.

The prototype leaves these for later:

- the host as a long-running service that starts without a terminal and outlives its clients, with automatic startup and restart;
- attaching a client the host did not launch ([U4](open-questions.md#u4-attaching-a-client-the-host-did-not-launch));
- the desktop app as a full client of host sessions, beyond the stage-5b subset;
- one session store that every client lists and resumes, and that survives a host restart;
- several paired devices at once, such as a phone and an iPad, with revoking a lost device from another paired device;
- a question waiting when the last device that can answer it leaves stays open for a device that attaches later, instead of being refused;
- the terminal UI as a client of the host.

### Sessions the terminal UI runs

A phone reaches only sessions that an application runs on `bravebot-host`. In this plan that is the desktop app, for the sessions it runs on the host (stage 5b). A session started in the terminal UI, including a background session (`--bg`, `/bg`, [background-sessions.md](../../specs/background-sessions.md)), runs in a process of the terminal UI's own, so neither the phone nor the desktop app can follow or control it while it runs. The desktop app's ordinary path can resume it from the shared store once it has ended ([SESSION-28](../../specs/sessions.md#SESSION-28), [SESSION-38](../../specs/sessions.md#SESSION-38)). A person who works in the terminal UI cannot use the phone with those sessions, and the first release says so.

Making the terminal UI a host client is a separate task outside this plan. The terminal UI would stop running sessions itself, start or connect to `bravebot-host`, and draw the host's view of the session.

What it gives:

- **Live control.** A phone sees a running turn and can answer an approval the terminal is waiting on, so a person can start a long task at the terminal, leave, and approve or cancel it from the phone. The desktop app can follow the same session.
- **The plan's model with one more client.** Every attached device can act ([decision 5](decisions-and-tradeoffs.md#5-every-attached-device-can-act-with-no-takeover-step)), the view shows which device acted last, and the [U3](open-questions.md#u3-effective-session-scope) scope check applies without new rules.
- **One place to run sessions.** The host owns execution, saving and permissions, and the terminal UI no longer keeps its own driver loop.

What it costs:

- **A large rewrite.** The terminal UI draws from in-process agent events today and would draw from the host's view instead.
- **A larger host protocol.** Everything the terminal UI shows and does needs a host message, including shell mode, `@` files, pasting, dropping, undo, the inspector panes and hooks. The host's view does not carry all of these today. Each is a new place to check the rule that untrusted content never reaches the driver or the planner.
- **A running host.** The terminal UI needs the long-running host from the list above, or starts its own as the desktop app does. Either way its reliability depends on the host, and it loses its standalone mode unless it keeps a fallback path.
- **Background sessions.** They already give the terminal UI a long-running session process, with a supervisor, a roster of sessions across projects, a named socket guarded by a secret ([BG-15](../../specs/background-sessions.md#BG-15)) and one attached terminal at a time ([BG-9](../../specs/background-sessions.md#BG-9)). This plan's host has no name to be found by ([HOST-13](../../specs/local-host.md#HOST-13)) and lets every attached device act. Before the long-running host is built, the task has to decide whether background sessions become host sessions, and which attach model and which rule for attached clients hold.

## Deferred work and open implementation choices

Optional follow-ons are shared queues, the takeover confirmation dialog in [decision 5](decisions-and-tradeoffs.md#5-every-attached-device-can-act-with-no-takeover-step), complete history, generic mutation retries, notifications, broader on-device tools, and offline inference. None blocks the bounded prototype.

Routine choices left to implementation include listener naming, local socket library, network library, schema spellings, buffer/submission budgets, heartbeat duration, pairing-code presentation, and protected mobile storage APIs. Pairing identity/security and effective-scope policy require the stage-6 decision notes. Record these with tests when they become relevant. Revise the architecture and proposed behavior when implementation evidence warrants it. Minor gaps are expected; they do not require predicting every remaining case before coding.
