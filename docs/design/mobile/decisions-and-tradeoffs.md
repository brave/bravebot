# Mobile prototype decisions and tradeoffs

Status: design decisions, not implemented guarantees. Read the [executive summary](executive-summary.md) for the goal and the [implementation plan](implementation-plan.md) for stages and required evidence.

## How to review this plan

This record states deliberate scope choices; [open questions](open-questions.md) tracks what is still unanswered and what awaits validation. “Settled for the prototype” means proceed on that basis, subject to implementation evidence. It does not mean the choice is permanent or exempt from review.

Challenge a choice when there is a concrete security defect, contradiction, missing requirement, or evidence that its cost outweighs its benefit. Name the affected behavior and stage. An alternative alone does not make the current choice a blocker. Unresolved decisions block only the capabilities named in that record; stages 1–2a can start now.

This is not an exhaustive list of edge cases. Record discoveries as implementation proceeds, update the affected documents and tests, and distinguish demonstrated behavior from reasoning or checks not run. These decisions do not override the [normative specs](../../specs/README.md).

## Settled for the prototype

### 1. React Native for both mobile modes

- **Decision:** the prototype uses React Native for native mobile controls and shared Android/iOS screens in both execution modes. Neither platform has delivery priority.
- **Reason:** native controls and platform integration support the intended input, navigation, accessibility, and lifecycle behavior. Framework familiarity alone does not justify the choice. Validate the actual implementation on devices; the framework does not guarantee responsiveness or usability.
- **Cost:** new mobile screens, native bindings, a larger dependency tree, and separate marking/approval tests. React DOM/CSS views do not transfer directly.

The alternatives were a native WebView shell reusing the desktop renderer and a browser/PWA for remote control. A WebView offers more screen reuse, but we choose native controls for the mobile interaction model. A browser-only PWA does not supply the planned embedded Rust and OS integration. Neither alternative is a required competing implementation or benchmark for this plan.

`android-shell` is an unmerged proof of concept. Its Rust/JNI binding and its Kotlin host's input restrictions are relevant to stage 2b; see [Current mobile status](README.md#current-mobile-status). Preserve attribution for code reused.

At the first stage-2b or stage-3d device work, validate a transcript, composer, and complete approval card. Record responsiveness, keyboard and scrolling behavior, screen-reader and text-size checks, label spoofing checks, and lifecycle behavior. Fix gaps before expanding the screens and record untested platforms. This checkpoint validates the chosen implementation; React Native selection is settled for this plan.

### 2. Shared Rust behavior and a thin client

- **Decision:** keep reusable session behavior, approval semantics, and presentation transforms in Rust. Stdio, embedded, and persistent-host callers use one implementation and expose its view through typed updates. `packages/agent-client/` contains thin adapters, wire types, local UI state, fixtures, and the test program. Follow [UI-006](../../best-practices/ui.md#UI-006) across mobile and desktop.
- **Reason:** a Rust fix should reach both execution modes without a second TypeScript implementation. Remote clients consume the host's view; embedded clients consume the same view over the native binding. Layout, focus, and other window-only behavior stay in the UI.
- **Cost:** the local proof now includes an additive bridge view capability and a clearly separated Rust presentation boundary. The exact crate/module placement must obey the layering spec; putting content inspection in `bravebot-core` or `bravebot-agent` is forbidden. Bindings and actual UI rendering still need tests.
- **Revisit when:** a consumer needs a missing field or operation. Extend the owning Rust API and its tests before adding client-side behavior. Do not introduce a second domain reducer. A new Rust runtime in remote-only clients needs an amendment to the client contract. See the [responsibility table](client-contract.md#shared-screen-state).

### 3. Early phone feedback and parallel embedded work

- **Decision:** stages 1–2a come first. Embedded work in 2b proceeds independently of the host track. Stage 3d uses a thin phone experiment; 5a attaches two TypeScript clients to one session before 5b integrates the desktop app.
- **Reason:** discover phone lifecycle and transport limits early, and avoid making native or desktop integration block host validation.
- **Cost:** the early phone experiment uses synthetic data and controlled tools. It proves neither secure remote access nor complete embedded support.
- **Revisit when:** the experiment reveals a contract or recovery flaw. The [first secure remote checkpoint](implementation-plan.md#first-secure-remote-checkpoint) can use the local test client for setup and as the second client on a session. Both the embedded proof and desktop attachment remain required for full prototype completion.

### 4. One Rust host and an isolated session store

- **Decision:** run the bridge and listener in one process, with a sessions-root override for fresh prototype records. Keep settings, trust stores, and credentials in their normal locations.
- **Reason:** avoid a supervisor/child lifecycle and shared-record writer coordination while proving live attachment.
- **Cost:** host failure interrupts live sessions. Ordinary saved-session import and automatic restart are excluded. Supported front ends must not independently resume host-owned records.
- **Revisit when:** real use requires importing existing sessions or recovering execution after a crash. `closed` confirms worker termination, not successful saving or rollback.

### 5. Every attached device can act, with no takeover step

- **Decision:** every device attached to a session sees the session live, approval requests included, with a paired device limited to the allowlist of [decision 19](#19-a-paired-device-is-an-approved-screen-that-receives-an-allowlist-of-events), and can send, answer and cancel at any time. There is no controller role, takeover step or controller epoch. A device paired with a read grant views only ([decision 16](#16-a-grant-names-read-or-control-and-one-workspace)).
  - The host records which device made each send, answer and cancel. The view shows the device that acted last, for example "approved on Phone", and the trace records the device for each action.
  - The first answer to a question counts. A later answer from any device fails on its question number (RPCVIEW-6).
  - A send while a turn runs is refused as busy, and the client keeps it as a draft. There is no shared queue.
  - Host-only questions, such as workspace trust and command-output admission, are offered only to connections the host lets answer them. This is a property of the connection, not of control. The host fixes the kinds per connection: for a local client on its command line, and for a paired device by its grant ([decision 16](#16-a-grant-names-read-or-control-and-one-workspace)) ([HOST-10](../../specs/local-host.md#HOST-10), [decision 11](#11-clients-are-named-on-the-hosts-command-line-with-the-kinds-each-may-answer)).
  - A device that leaves or breaks blocks no other device. Its pairing can be revoked on the host ([decision 16](#16-a-grant-names-read-or-control-and-one-workspace)). A locked or backgrounded phone cannot answer approvals (stage 7, decision 8).
- **Reason:** an approval is bound to one exact request and answered once, so a device does not need to hold control to answer it. A takeover prompt would confirm the device, not the effect. Moving from desktop to phone needs no handoff, and a lost phone does not leave a session stuck.
- **Cost:** an approval can be answered on a device the person was not using, such as an unlocked tablet on a desk. Two sends at once start one turn and refuse the other as busy. A session has no controller ([HOST-7](../../specs/local-host.md#HOST-7)).
- **Deferred:** a "Take control from Desktop?" confirmation before a device acts. Because the view names the device that acted last, the dialog is client-side: a client asks before acting when another device acted last. It needs no host role, epoch or protocol change.
- **Revisit when:** an approval or cancel is answered on a device the person was not using, or people report acting from the wrong device. Add the dialog then. Add queued follow-ups only if observed use shows they are needed.

### 6. Snapshot-only reconnect recovery

- **Decision:** on reconnect or a sequence gap, fetch a fresh bounded snapshot and continue with ordered events. Defer incremental replay and complete history.
- **Reason:** reduce recovery states and races while retaining current session state.
- **Cost:** reconnect may resend recent content and omit older history. Show omissions and preserve full pending approval details separately.
- **Revisit when:** measured bandwidth, latency, or history requirements justify replay or pagination. Never reopen a saved record to recover a live session.

### 7. Retain narrow send-ID recovery

- **Decision:** require a message ID on every send, record it before dispatch, retain bounded outcomes for the live session, provide status lookup, and persist the phone’s pending send before transmission. Reject new sends at capacity rather than forget retry IDs.
- **Reason:** an accepted prompt can leave the bounded snapshot. Display history alone cannot settle a lost acknowledgement, especially after phone process loss.
- **Cost:** a small pending-message store and lookup API remain necessary. Host restart can still leave an unknown outcome. Other mutations are not automatically retried.
- **Revisit when:** measurements justify changing retention or removing a recovery promise. Do not remove identity storage while continuing to claim recovery across phone process loss.

### 8. Foreground approvals and host-only output admission

- **Decision:** a phone answers supported approvals only while it is ready and in the foreground. A `control` device answers workspace trust for one session only ([decision 17](#17-a-control-device-answers-workspace-trust-for-one-session-only)). Keep command-output admission and the other authority-expanding decisions on the host.
- **Reason:** preserve explicit consent with a small, testable set of phone approval screens.
- **Cost:** locking the phone may cause refusal when no other attached device can answer. A run-tests-then-read-failures task needs the desktop to answer output admission; it is not a phone-only demo. Startup trust stays unanswered when unavailable.
- **Revisit when:** a useful task justifies another approval kind or a defined waiting policy. Add complete field display, label handling, permission rules, and tests before enabling it. Push notifications do not themselves grant approval.

### 9. Mobile dependencies and builds

The repository already has Node tooling for the desktop and website. Its [dependency rule](../../best-practices/dependencies.md#DEP-001) covers both `Cargo.toml` and `package.json`: each addition needs a reason and a review of its transitive cost. React Native adds runtime, build, and platform dependencies beyond that existing tooling; the security rule about content does not protect against a compromised dependency executing inside the app.

Keep `packages/agent-client/` independent, with no runtime dependencies for its common adapter module unless a concrete need is demonstrated. Adding this directory does not introduce a root npm workspace or connect mobile dependencies to the published CLI wrapper. Add the mobile app's dependencies only with its first runnable screen. Avoid extra UI kits, analytics, remote JavaScript updates, and general state frameworks in the proof.

Before that addition lands:

- Record direct and transitive additions, licences, install/build scripts, native code, and why existing code or platform APIs do not suffice. Apply the same review to credential-storage bindings.
- Commit npm and native-platform lockfiles or equivalent dependency verification metadata. Install reproducibly; review any scripts that must run, using builds without production credentials. Keep signing/release credentials out of dependency-install jobs.
- Extend [lockfile checks](../../../package.json), `make check-npm`, [affected-check selection](../../../contrib/affected-checks.py), and CI to the new package/app paths. Extend [CODEOWNERS](../../../.github/CODEOWNERS) to new manifests, lockfiles, and build entry points using the existing review convention.
- Run `make check-deps` for Rust additions and record justified `.config/deny.toml` exceptions. Its cargo-deny checks do not cover npm, CocoaPods, or Gradle dependencies. Add advisory and licence checks for the selected mobile dependency managers with documented exceptions; do not treat lockfile integrity as an advisory scan. Include their commands in local and CI checks before claiming coverage. Update the affected-check classifier selftests with new package paths, and state which native checks need device hardware or signing setup. Existing desktop checks do not establish mobile coverage.

No dependencies or build tooling are added by this proposal. Stage 1 covers the adapter and Rust additions; the first of stages 2b and 3d that adds the app covers its platform dependencies. Reassess the dependency cost at the UI checkpoint.

### 10. One process-wide spawn gate, before the host exists

- **Decision:** the creation of a control channel and every start of a program by a crate the bridge links take one process-wide lock, one at a time (`bravebot_sandbox::process::creating_a_channel` and `starting_a_program`). It lands with the channel work although `control_link::launch`, the only channel launcher, has test callers and the persistent host does not exist. A test reads the source of every linked crate and fails for a start outside the gate.
- **Reason:** macOS marks a new socket pair or pipe close-on-exec in a second step, and Linux does it atomically (checked in the standard library source). A program another thread starts between the two steps holds both ends, and the bridge hands them on to its tools. That race was reproduced. A start also creates the pipes of its own standard streams inside `spawn`, so a start is a descriptor creation as well as a fork, and two starts overlapping can leave one holding the other's pipe ends open. A version in which starts shared the lock and only channel creation excluded them hung a test, most likely for that reason, and was reverted. The host will create channels while its other threads start hooks, commands, language servers and sandboxed programs, which is when the overlap happens. Adding the gate after the host exists means finding every start again; with the source test, a new start that skips it fails a check instead.
- **Cost:** starts run one at a time. The lock is held until the program has started, since the standard library offers no narrower point between the fork that copies descriptors and the exec. On Linux that includes the Landlock rule setup before the exec, which makes system calls on memory built before the fork and allocates nothing ([SANDBOX-34](../../specs/sandboxing.md#SANDBOX-34)), so a start stuck in its exec holds up every start behind it. The lock is not reentrant, and entering it again panics. A caller that waits for a program to finish starts it under the gate and waits outside, as the AWS CLI calls do. Start latency under the gate was not measured.
- **Revisit when:** measured start latency shows contention, for example many delegates or hooks starting together. Reductions to assess in this order: take the gate only on macOS, where the race exists; create channels before any worker thread exists so tool starts need no gate; start programs through a helper process that holds none of the host's descriptors. Letting starts overlap each other is not a reduction. The root-cause alternative on macOS is to start programs with `posix_spawn` and `POSIX_SPAWN_CLOEXEC_DEFAULT`, which closes every descriptor the child is not explicitly given, whatever created it. That covers the TCP and TLS sockets and other descriptors the gate does not, and would make the gate unnecessary there, but it means leaving `std::process::Command`. A change that drops the gate on a platform needs its own evidence from the `control_boundary` race tests.

### 11. Clients are named on the host's command line, with the kinds each may answer

- **Decision:** the host's command line names each client it launches and the question kinds that client may answer: `--client NAME --answers KINDS --program /ABS [--arg VALUE]...`, repeated for each client. The `-- PROGRAM ARGS` form is removed.
  - KINDS is a list drawn from `confirm`, `run`, `fetch`, `ask` and `trust`, or `none`. There is no default.
  - The host starts every client at startup, in command-line order, and serves them at once. `launch NAME` starts one again after it has ended, and is refused while a client of that name is connected. A bare `launch` is accepted only when one client is defined.
  - The name is the client that acted, as the view and the trace show it.
  - For a local client, names and kinds come only from the command line and the `launch` line the person types. Nothing a client sends and no file sets them. A paired device's name and kinds come from the person's pairing on the host ([decision 14](#14-a-device-pairs-by-a-qr-code-on-the-host-and-a-code-typed-on-the-host), [decision 16](#16-a-grant-names-read-or-control-and-one-workspace)).
- **Reason:**
  - The host launches every local client over an anonymous socket pair, the development relay included, so it cannot tell a host-local client from the phone's relay unless the person tells it.
  - One program after `--` cannot be two clients with different kinds.
  - A shorthand would need default kinds, and it would either give trust to whatever program it names, the relay included, or remove trust from flows that have it.
- **Cost:**
  - Every launcher changes, and every launch spells out its kinds.
  - Each client argument takes its own `--arg`.
  - A phone that leaves ends its relay, and it cannot reconnect until the person types `launch relay` on the host and enters the new port and code ([HOST-12](../../specs/local-host.md#HOST-12)). This happens whenever the phone sleeps, changes network or closes the app. Stage 6 pairing removes it.
- **Revisit when:** stage 6 pairing names devices, which then replace launch names for remote clients.

### 12. The host asks about each workspace's trust at startup

- **Decision:** before it launches any client, the host asks on its own terminal about each `--workspace` that has no kept yes. It always asks, and no option turns the question off.
  - It uses the same question, the same kept record and the same check of whether a yes may be kept as the terminal and the desktop app ([TRUST-23](../../specs/trust-map.md#TRUST-23)).
  - A kept yes answers every later session in that workspace, from the phone as from any other client.
  - A no keeps nothing, and that workspace's sessions ask as before.
  - The host does not ask about a workspace where a yes cannot be kept, because a yes there would answer nothing. It says on standard error that each session there asks.
  - The host reads the answers from its standard input before it starts reading `launch` and `quit`. Each answer is a line. As in a session in lines ([TRUST-23](../../specs/trust-map.md#TRUST-23)), a line that is not yes declines and keeps nothing, except `quit`, which stops the host and keeps nothing for the workspace being asked about.
  - An end of input at the question answers no, and the host then stops, as the end of its input always does ([HOST-1](../../specs/local-host.md#HOST-1)).
- **Reason:** without the startup question, a phone that opens a session in a workspace nobody has trusted waits until the person answers on the host, or from stage 6 until a `control` phone answers for one session ([decision 17](#17-a-control-device-answers-workspace-trust-for-one-session-only)). The person has just typed the host's command line naming those workspaces, so the host asks them there once.
- **Cost:**
  - Host startup is interactive for a workspace with no kept yes. A test or script that starts the host with such a workspace has to answer on standard input.
  - Until stage 6, a session the phone opens, which is always in a `--workspace` ([HOST-2](../../specs/local-host.md#HOST-2)), waits when that workspace has no kept yes, until a client on the host that may answer trust, such as the host test client, attaches and the person answers there. A workspace has none after it was last answered no, after a kept yes is taken back from the terminal or the desktop app, after the directory is replaced by another at the same path, or where a yes cannot be kept: the home directory, a filesystem root, an incognito session, or a filesystem that does not record when a directory was made. The phone's composer then says that the folder is not trusted yet and that the question is answered on the computer the host runs on.
- **Revisit when:** people answer the startup question so rarely that a phone's one-session yes ([decision 17](#17-a-control-device-answers-workspace-trust-for-one-session-only)) is the usual answer. Decision 17 relies on this question, so keep it unless that decision changes.

### 13. Device keys and the pairing list live in host memory

- **Decision (2026-11-24):** the host's private key and the list of paired devices are held in the host process and written nowhere. A device pairs again after the host restarts. A device that sleeps, loses its network or changes network reconnects with its device key.
- **Reason:** [HOST-1](../../specs/local-host.md#HOST-1) says nothing about a client comes from a file a tool could write. A list on disk is such a file: a program the person runs could add its own device, or restore an older list to undo a revocation. Sessions end when the host restarts ([HOST-5](../../specs/local-host.md#HOST-5)), and the person is at the host then.
- **Cost:** every host restart needs a new pairing. A host that restarts while the person is away stays unreachable until they return.
- **Revisit when:** signed macOS builds can hold a non-exportable key and a rollback counter in the keychain (*unverified*). Linux and development builds are thought to have no equivalent (*unverified*).

### 14. A device pairs by a QR code on the host and a code typed on the host

- **Decision (2026-11-24):** the person types a pair command on the host. The host opens a one-time window and shows a QR code with its address, its key hash and a secret of at least 128 bits. The phone pins the host key from the code and proves it knows the secret over the connection. The phone then shows a confirmation code, and the person types it on the host. A phone with no camera types a code of at least 80 bits.
  - The window closes after one pairing or a deadline, and a failed attempt closes it. The secret is never written to the diagnostic log.
  - The person types the device's name on the host. Strings a device sends are escaped before the host prints them.
- **Reason:** a six-digit code has about 20 bits, so an attacker on the network who sees anything keyed by it can guess it offline, and it does not tell the phone the host is real. A program reading the host's screen cannot answer a code shown only on the phone.
- **Cost:** pairing needs the phone's camera or a long typed code. Anyone who can reach the port can close the window, so pairing can be denied and cannot be completed by them. Approval on the host's input assumes host standard input is the person. Input injected with `tmux send-keys` or `TIOCSTI` breaks that assumption.
- **Revisit when:** a password-authenticated key exchange library is judged mature enough to replace the long typed code.

### 15. The host listens on one chosen address

- **Decision (2026-11-24):** `--listen ADDR:PORT` names one specific address, with no wildcard. The host exits when it cannot bind and does not try another. It accepts connections only while a pairing exists or the pairing window is open. Without `--listen` it has no network address, as now. The listener stays in the host process, and the development relay is retired or limited to development builds.
  - Limits before authentication are global and not per source address: connections, handshakes in progress, a small first frame, and the threads used before authentication.
  - Half-open connections are found by an application heartbeat.
- **Reason:** per-source limits mean little when loopback is one address and each tailnet device has its own. A program that takes the port first can deny service and cannot impersonate the host, because the pinned key identifies it.
- **Cost:** [HOST-13](../../specs/local-host.md#HOST-13) and its test change to two sides: no `--listen` means no socket, and `--listen` means exactly that one. A TCP port is reachable by every local user. A separate listener process would keep HOST-13 as written and keep TLS bugs away from the host's memory, but device identity would then be decided outside the host and revocation would cross a process boundary.
- **Revisit when:** a bug class in the network code makes memory isolation worth that cost.

### 16. A grant names read or control, and one workspace

- **Decision (2026-11-24):** the person chooses `read` or `control` when pairing, with no default, and one workspace the device covers. A `read` device adds no kinds to [HOST-10](../../specs/local-host.md#HOST-10)'s set and is refused `session.new`, `session.close`, `turn.send` and `turn.cancel`.
  - A device has one connection. A new authenticated connection from it ends the old one.
  - Revoking a device ends its connection and removes its kinds. A request it sent that the host has not yet run does not run.
  - A pairing does not expire, and a `control` device needs no biometric to use its key.
- **Reason:** [decision 11](#11-clients-are-named-on-the-hosts-command-line-with-the-kinds-each-may-answer) gives no default kinds. A grant limited to one workspace keeps a device away from the desktop sessions on the same host.
- **Cost:** a lost phone stays valid until the person revokes it on the host or the host restarts.
- **Revisit when:** a lost phone stays paired longer than the person expects. Add expiry or a biometric requirement then.

### 17. A control device answers workspace trust for one session only

- **Decision (2026-11-24):** a device with a `control` grant may answer the startup trust question. Its yes applies to that session only and is never written to the kept-trust record. A device cannot keep or remember any grant. A `read` device cannot answer trust.
  - The phone shows the directory and what each answer does before any button works ([TRUST-23](../../specs/trust-map.md#TRUST-23)). A yes that is not kept names no file.
- **Reason:** the host already asks about each `--workspace` at startup and keeps the yes ([decision 12](#12-the-host-asks-about-each-workspaces-trust-at-startup)), so the phone seldom meets the question. A one-session yes lets the person start work from the phone in a workspace they declined, and a lost phone cannot trust a folder for good.
- **Cost:** the phone needs a full trust screen. [Decision 8](#8-foreground-approvals-and-host-only-output-admission) and the shared control rules name this case. A hijacked `control` phone can answer yes for each session it opens in the one workspace its grant names, so the number of sessions is not bounded by the one-session yes. The ceiling of [decision 18](#18-a-devices-scope-is-a-ceiling-set-at-pairing) bounds what that session can reach.
- **Revisit when:** a phone's one-session yes is used to start sessions the person did not intend.

### 18. A device's scope is a ceiling set at pairing

- **Decision (2026-11-24):** pairing shows the person the limits for the device on the host, and they approve them once. The limits are the workspace, trust no wider than it, a sandbox mode no looser than a named one, the additional directories, exposure grants, the permission mode, the trusted programs, the permission rules, the MCP server set, `readsStayInWorkspace`, the references, the skill directories, the kept checkouts, the started language servers, the tools no longer asked about, whether confined `run` lines start unasked and auto-vetting. The defaults are the values in force at pairing.
  - Inside the ceiling the device needs nothing. A change that goes past it suspends the device's reads and control before the session sends anything that follows, until the person approves the session for the device on the host. The host checks when a turn starts, when the trust question is answered, and when the person approves a fetch or a line run with no sandbox for the turn ([HOST-15](../../specs/local-host.md#HOST-15)). A change that narrows never suspends it.
  - **Approving with the widening (2026-11-27):** the prompt on the host that approves a widening also asks whether each paired device watching the session may see it, and the answer defaults to no. A yes approves that session for that device in the same step, so a person who widened a session on purpose need not approve the phone again. An approval holds the device in that session alone: its ceiling stays what the person approved at pairing, and every other session is measured against it. A no, or no answer, suspends the device as above. The device itself never widens anything and never approves its own ceiling.
  - Fields with a clear order compare against the ceiling. A field with no total order, such as the permission rules and the MCP server set, compares with a per-field `covers`, and a change that `covers` cannot show to narrow counts as widening. Auto-vetting must be equal: turning it on releases safe content unasked in every mode but bypass, and narrows bypass, which rejects what a check calls unsafe only while it is on.
  - Suspension removes the device's kinds from [HOST-10](../../specs/local-host.md#HOST-10)'s set. Approval sends a [HOST-11](../../specs/local-host.md#HOST-11) snapshot, which shows what happened during the suspension.
- **Reason:** a snapshot taken at first attach authorizes a scope nobody has seen, and it would suspend the phone on routine changes while the person is away. A widening the person made at the computer applying to the phone without being asked would show a phone in someone else's hands what the person never agreed it could, and asking on the phone would let that phone agree to it. Asking in the widening's own prompt keeps the yes on the host and costs the person no second step.
- **Cost:** a change inside the ceiling reaches the device without a prompt. A settings edit that loosens the sandbox mode within the ceiling applies from the next turn ([SANDBOX-22](../../specs/sandboxing.md#SANDBOX-22)). Stage 6 and the [permissions section](architecture.md#permissions-and-effective-scope) read an earlier scope as this ceiling.
- **Revisit when:** a ceiling is set so wide that it stops meaning anything, or a widening inside it surprises the person.

### 19. A paired device is an approved screen that receives an allowlist of events

- **Decision (2026-11-24):** pairing records the person's yes that the device may show the content they see. The host sends a device only the event names on an allowlist. A quarantined preview becomes a placeholder row that keeps its ID and place in the sequence. A question is never a placeholder: a device that cannot show one in full is refused remote approval, and the person answers on the computer. The choice branches on bridge-owned event names and on labels through the policy layer, never on text the host reads.
  - **What is a placeholder (2026-11-27):** a placeholder replaces every row the bridge marks as coming from somewhere nobody vouched for: a quarantined preview, a tool's activity the agent marked `untrusted`, and a question that holds such content: one of a kind a client of the host does not answer, a write whose body came from such content or that carries an isolated processor's remark, and a fetch a redirect led to. That question is answered on the computer. Where no connection that is not a device can answer it, it is refused when it is asked, or when the last such connection leaves, as a question nobody can answer is. A file name is not withheld for what it is: where an activity or a write question is sent rather than replaced by a placeholder, it names its file whole, even a file the agent holds as a reference because it came from somewhere nobody vouched for, because no flag the bridge reads marks the name.
  - The phone saves no transcript to disk, its pending-send store holds only text the person typed, and it hides the app-switcher snapshot, with `FLAG_SECURE` on Android.
- **Reason:** the view holds quarantined previews whole, and a phone can be lost. Network egress policy covers only requests this process makes, so the stream to a device needs a clause of its own.
- **Cost:** this changes the rule that every attached client receives the same view. The host writes each event once to every connection today ([HOST-7](../../specs/local-host.md#HOST-7)), so the device path needs its own filter in the send path. The host forwards every event of a session and not only the view's, so the allowlist covers all of them.
- **Revisit when:** a placeholder hides something the person needs to approve a request.

### 20. The phone connects over TLS with pinned keys

- **Decision (2026-11-24):** the host and the phone use TLS 1.3 only, with self-signed X.509 certificates, public-key pinning on both sides, ECDSA P-256 keys and no certificate authority. There is no 0-RTT and no session resumption, or the host checks the registry after every handshake. The existing newline-delimited JSON runs over the stream, with no WebSocket. The phone keeps its key out of JavaScript, in native code with a hardware-held key. The Android app supports Android 12 (API 31) and later, the first version with `SSLSockets.exportKeyingMaterial`, which the exporter value needs.
  - **Android, checked 2026-11-25 on a Pixel 10 Pro (Android 17, API 37) with an installed test app against a Rust TLS 1.3 server:**
    - The platform keeps a P-256 key in StrongBox (security level 2). The key reports itself inside secure hardware and its private key object returns no encoding. The platform's TLS 1.3 client signs with that key only when the key allows the `NONE` digest as well as SHA-256. A key with SHA-256 alone is refused with `INCOMPATIBLE_DIGEST`.
    - The platform's TLS stack connects with TLS 1.3 only, refuses a host whose key hash does not match the pinned one, and presents the StrongBox key's certificate as the client certificate. It parses the self-signed certificate that the server's own code writes. Where the server requires a client certificate, it refuses a connection that has none, and the platform reports the server's `certificate required` alert.
    - `android.net.ssl.SSLSockets.exportKeyingMaterial` returns the same exporter value as the server for each connection, and the server verifies the StrongBox key's signature over the label and that value.
    - With no tickets and no session storage on the server, two connections are two full handshakes.
  - **iPhone, checked 2026-11-25 on an iPhone 18 Pro (iOS 27.2) with an installed test app against the same server:**
    - The Secure Enclave holds a P-256 key whose private key cannot be exported. The platform parses a hand-written self-signed certificate signed by that key, and the keychain pairs the certificate with the key as a client identity.
    - Network.framework connects with TLS 1.3 only. Its verify block receives the host's certificate, and a pin comparison there refuses a wrong key. A connection without a client certificate is refused.
    - `sec_protocol_metadata_create_secret_with_context` returns the same exporter value as the server for each connection, and the server verifies the Secure Enclave key's signature over the label and that value. Two connections are two full handshakes.
  - **Not checked:** React Native on either platform, `URLSession`, a phone with no hardware-backed key, a phone with no passcode, a locked phone, Android before API 37, iOS before 27.2, and any network other than one shared Wi-Fi network. The Android module of React Native 0.87.1's `WebSocket` was read, and the iOS module was not. The JavaScript call passes a url, protocols and headers, and the Android module gets its client from `OkHttpClientProvider`. The call carries no trust store, pin, client certificate or exporter request. Neither platform needs a rustls module.
- **Reason:** from JavaScript, React Native's `WebSocket` cannot pin a key learned at pairing, present a client certificate or read the TLS exporter value. A resumed mutual-TLS session skips the client-certificate check, so a revoked device could return. The first claim is read from React Native's source, and what a revoked device could do after resumption was not tested. Tailscale Serve in HTTPS mode would replace the host's certificate and break pinning (*unverified*), so the host binds the tailnet address directly.
- **Cost:** no browser client, and the phone needs native code.
- **Revisit when:** a phone platform's TLS stack can pin and sign from JavaScript-visible APIs.

## Open questions

The [open-question record](open-questions.md) tracks implementation choices and decisions awaiting validation, with the evidence and checkpoint each needs. When an entry is answered, record the decision here and delete the entry. Stages 1–2a can proceed while later security decisions are not built.

## Maintaining this record

Update an entry when evidence changes its decision, cost, or completion condition. Keep unresolved status in [open questions](open-questions.md), and keep the architecture, client contract, implementation stages, and executive summary consistent. Routine choices such as names, libraries, and numerical limits can be made during implementation within these boundaries.
