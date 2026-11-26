# Mobile prototype open questions

Status: proposal. This file lists only what is unanswered or waiting for evidence. A decision goes in the [decision record](decisions-and-tradeoffs.md), behavior in the [specs](../../specs/README.md), and planned work in the [implementation plan](implementation-plan.md). **Unresolved** means an answer is still needed. **Awaiting validation** means the plan has a proposed answer that needs evidence. Neither status establishes implemented behavior. Each entry names the capability that needs its answer; unrelated work can proceed.

### Q1. Does the React Native implementation meet the mobile interaction requirements?

- **Status:** React Native selection settled; device validation pending.
- **Question and reason:** do the implemented screens meet the required responsiveness, input, accessibility, approval-display, and lifecycle behavior on the tested platform?
- **Current assumption:** use React Native for both modes, as recorded in the [UI decision](decisions-and-tradeoffs.md#1-react-native-for-both-mobile-modes). No competing WebView or PWA proof is required.
- **Evidence needed:** device checks of a transcript, composer, and complete approval card, covering responsiveness, keyboard, scrolling, accessibility, labels, and lifecycle. Record gaps and untested platforms.
- **Before:** expanding the screens beyond the first stage-2b or stage-3d device proof.

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

### Q7. When does the mobile app close a session it opened?

- **Status:** unresolved.
- **Question and reason:** the app keeps every session it has opened, so a person can switch back with the draft and answers intact, and it does not close one when the person switches session, changes target or leaves. A listed session that arrives after a newer pick is dropped without being closed, because closing it could release something the target shares. On a target with real sessions this holds a session and a connection for each past one.
- **Current assumption:** acceptable while the only targets are a test fake and the development relay, which is for synthetic data only. Over the relay the sessions are `bravebot-host`'s. When the relay's connection ends, each session the app opened stays open on the host with no controller, its waiting question refused and its turn running to its end ([HOST-7](../../specs/local-host.md#HOST-7)). The client can take one over with `attach()` ([RPCVIEW-12](../../specs/session-view.md#RPCVIEW-12)), but the app does not call it, so a session from an earlier connection stays open until the host quits, and every connection can add more.
- **Evidence needed:** what close, detach and take-over mean on the first real target, which of them the app may call for a session it opened, what to do with a result that arrives late, and a bound on the sessions kept. Tests for switching, a target change and exit.
- **Before:** the first target used with real data, which is the stage-7 remote adapter.
- **Issue:** [#2770](https://github.com/brave/bravebot/issues/2770), "Release the sessions the mobile app opens when it switches, changes target or exits".

## Keeping this record current

Update an entry when its assumption, evidence, or required checkpoint changes. When an entry is answered, record the decision in the decision record, the requirement in the architecture or contract, the work in the plan and the behavior in a spec with the change that makes it true, then delete the entry. Identifiers are not reused, so a gap in the numbering is a deleted entry.

Add a question when it affects a capability, security boundary, or delivery choice. Routine names, numerical limits, and libraries can be chosen during implementation and recorded with their tests. Keep checkpoints and scope cuts in the [implementation plan](implementation-plan.md#checkpoints-and-scope-cuts); review remaining work there as these questions are answered.
