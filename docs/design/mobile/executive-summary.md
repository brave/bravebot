# Bravebot mobile: executive summary

Status: proposal. No prototype has been implemented or demonstrated by this documentation work.

## Two modes in one app

### On-device agent

Bravebot runs on the phone, using the phone's available tools, files, and permissions. The agent may still call a remote model service; running the agent on-device does not imply offline operation.

### Remote control

Bravebot runs on the user's computer. The phone lets the user:

- Start a session or contribute to an existing one.
- Read progress and send follow-up instructions.
- Answer supported permission requests or stop a turn.

The computer owns execution and session state. Work can continue when the phone disconnects, subject to permission requests that need a person to answer.

Coding is an initial proving task. Both modes should support other useful tasks as tools and platform permissions allow.

## Current mobile status

The [android-shell](https://github.com/brave/bravebot/tree/android-shell) prototype provides a substantial starting point for Bravebot on mobile. Its contributor has already brought together:

- **On-device execution:** the Rust agent and UI bridge run inside the Android app through JNI.
- **A mobile application:** the existing renderer runs in a WebView, with phone navigation, drawers, sheets, and touch-layout work.
- **Input checks and focused tests:** the Android host checks allowed methods and filters turn inputs. `android-host.test.mjs` checks source consistency with the desktop host; `compact-layout.test.mjs` exercises layout functions and checks layout source. Neither tests JNI or Kotlin runtime behavior.
- **Build and integration guidance:** Android build scripts and instructions document how the native runtime, app, and backend configuration fit together.

This work informs the proposed client interface, native bindings, approval screens, and tests. React Native changes how the mobile UI is built; the runtime integration and interaction work remain useful foundations.

The mobile implementation owner should track this branch at mobile-stage planning checkpoints, before stage 2b and later integration work that uses it, and revisit relevant updates during integration. Record the revision used, assess new work before duplicating it, and build on applicable code, tests, and design lessons while preserving contributor attribution through cherry-picking or merging applicable commits where feasible. For adaptations that need a fresh commit, retain original author credit and source-commit references.

The source findings here use [`9635f16b`](https://github.com/brave/bravebot/tree/9635f16b49798624e6174b0d15818227773e81a8) for reproducibility. That revision's Android work is separate from the plan's `main` baseline. Refresh the reference when development reaches mobile integration and record build/device validation for the selected revision.

## Why React Native

Use React Native for two reasons:

- **Multiplatform development:** share mobile screens and TypeScript client logic across Android and iOS.
- **Agent familiarity:** coding agents are highly familiar with React Native, which should help them build and maintain the prototype efficiently.

The plan does not prioritize Android over iOS. Choose the first device proof based on integration effort and available hardware, and record which platforms were tested. Native integration remains necessary for the on-device Rust runtime, secure storage, and other platform differences.

Give the app one session interface with two execution adapters: embedded for the on-device agent and networked for remote control. A third, local stdio adapter lets a small test program validate that interface on the local host machine first.

Put this client in a standalone package, `packages/agent-client/`, with its own build and tests. React Native uses it directly. Reuse useful desktop connection code without depending on the Electron app; integrate Electron later for desktop-to-phone handoff.

## Delivery priority and rationale

**Remote control is the first substantial end-to-end prototype.** Start stages 1–2a now. Run the small embedded exercise independently of the host track, and test phone behavior early with synthetic data.

Stage labels match the [implementation plan](implementation-plan.md).

| Stage | Deliverable | Reason for the order |
|---|---|---|
| 1–2a | Define the standalone client, shared screen-state logic, and local test program against a real RPC process. | Validate session operations and how events become screen state before adding networking. |
| 2b, parallel | Assess and port selected prototype code/tests; prove one on-device interaction through React Native. | Check embedded integration without delaying host work. |
| 3a–3c | Add exact action targets, a persistent local host, and controller-loss handling. | Establish ownership and permission boundaries before reattached control. |
| 3d | Build the shared mobile app shell and run a thin phone experiment with synthetic data and controlled tools over a loopback-only, per-launch-token development transport. | Learn about lock/background, disconnects, and approval display before fixing recovery details. This does not authorize real remote work. |
| 4a–4b | Add bounded snapshot recovery and message-ID send protection. | Recover known state and avoid duplicate work after lost replies. |
| 5a | Prove takeover with two small TypeScript clients. | Test control handoff independently of Electron. |
| 5b, alongside later work | Attach Electron for the supported subset. | Complete desktop-to-phone handoff without blocking the initial phone integration. |
| 6–7 | Decide pairing and access scope, then enable secure networking and the remote phone UI. | Apply the local and phone findings before exposing real sessions. |
| 8 | Demonstrate remote use away from the desktop, plus the completed embedded proof. | Verify both modes on hardware, including desktop handoff and failure behavior. |

Broader on-device tools do not block remote-control delivery.

## Existing components

Bravebot already has a Rust session bridge, a JSON-over-stdio RPC process, an Electron client, and tests for existing behavior.

The Android work described in [Current mobile status](#current-mobile-status) supplies native integration, mobile interaction, and test references for stage 2b.

Live remote attachment is new work. Opening a saved record currently creates a new running instance; it does not attach to another process's session.

The proposed host is one Rust process containing the bridge and socket listener. It needs no separate supervisor or RPC child. New work also includes the shared client, React Native UI, recovery, and remote authorization.

## Session ownership and security

- **One execution owner.** Show “This device” or the chosen host. Files, credentials, and permissions stay with that target. Switching targets never migrates a live session or silently runs host work on the phone.
- **One active controller.** Desktop and phone transfer control explicitly; authorized viewers can observe. Reject delayed actions from the former controller.
- **Host-enforced permissions.** Pairing does not grant workspace trust or unrestricted access. Trust and authority-expanding decisions stay on the host. Check the session's full permission scope and authorization to display private content on the phone.
- **Local control is a security boundary.** Programs run by Bravebot may have the same OS user as the host. Socket permissions alone cannot stop them answering later approvals. Resolve and test this boundary before real-session control; use synthetic, controlled experiments until then.
- **Exact action targets.** Apply each approval or cancellation only to its intended current turn or question. Cancellation does not undo completed effects.
- **Preserved content boundaries.** Keep content labels visible and preserve Bravebot's rule that untrusted content never enters the driver or planner context.

## Known limitations

- **Small scope:** one user, one host, one prepared workspace, and fresh sessions in an isolated store. No saved-session import or concurrent controllers.
- **No shared queue:** sending during an active turn returns busy and keeps the unsent draft.
- **Foreground approvals:** if no ready controller can answer, refuse the question; startup trust stays unanswered. A locked phone does not promise deferred approval.
- **Bounded recovery:** retain limited display history and prevent duplicate sends while the host lives. After host restart, outcomes may be unknown; do not automatically resend uncertain work.
- **Host-only output admission:** the phone cannot authorize the model to read command output. A run-tests-then-read-failures task needs desktop takeover before that question; otherwise it is refused.
- **Closed is not saved:** a closed session confirms the worker stopped. Saving needs separate evidence.
- **Deferred features:** full history, automatic restart, public relays, push notifications, broad on-device tools, and offline inference.

## Development method

Use test-driven development for each small behavior:

1. **Red:** define the expected result and a plausible mistake; write a focused test and see it fail for the intended behavioral reason.
2. **Green:** implement enough to make it pass.
3. **Refactor:** improve the code while keeping tests green, then run relevant integration and repository checks.

Follow `testing-preflight`. For security, cancellation, and concurrency changes, demonstrate that selected tests catch a plausible fault. Report when that evidence is impractical.

## What counts as done

The same React Native app demonstrates:

- A bounded on-device session, including supported denial and cancellation.
- Starting host work, taking over desktop-started work, answering supported approvals, cancelling, and returning control.
- Tested permission and content boundaries, reconnect behavior, and duplicate-send prevention.

Record evidence for these behaviors and their limits. A scripted happy path alone is insufficient.

## What remains open

This is a high-level plan, not an exhaustive specification. Implementation will reveal gaps and may require behavior not described here. Update the design, specs, and tests as implementation reveals new requirements; resolve security gaps before enabling the affected feature.

Choose libraries, schema details, storage APIs, limits, and timeouts during implementation. Apply red/green development to discoveries one behavior at a time; do not wait to predict every edge case.

## Supporting documents

Read [decisions and tradeoffs](decisions-and-tradeoffs.md) for the reasons behind prototype choices and the security decisions still open.

See the [client interface](client-contract.md), [architecture](architecture.md), [implementation plan](implementation-plan.md), and [repository findings](README.md).

This proposal is maintained in `docs/design/mobile/`. The [normative specs](../../specs/README.md) remain the source of truth for implemented behavior.
