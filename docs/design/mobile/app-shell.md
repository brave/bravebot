# Mobile app shell

Status: built in `mobile/`. It builds and runs on the Android emulator and a device and on the iOS simulator. The session screens are built and tested against a fake client, and no target is offered yet, so the app opens on a screen that says nothing is connected.

## Purpose

The shell is the React Native app: a chat screen with a transcript, composer and approval card, a drawer that lists sessions, and a drawer that lists the calls a session made. It depends on the common client interface in `packages/agent-client` and never on which target is behind it, so a target is added without changing the screens.

It settles the app's structure, its dependency cost and its behavior on a device before host or embedded integration starts.

## Scope

In scope:

- One React Native app that builds and runs on the Android emulator or a device and on the iOS simulator, with the project, icon and check wiring described in [mobile/README.md](../../../mobile/README.md).
- The session screen, the drawer, the Calls made drawer, the approval cards for `confirm`, `run`, `fetch` and `ask`, and the startup-trust card.
- A `Target` that gives the app a client, and listing and opening sessions through the client interface.
- Escaping of displayed text from outside the app.
- Dependency review and repository check wiring for the new paths (decision 9, Q5).

Out of scope: any target (a host, the embedded runtime, a recording), pairing, notifications, persistence across launches, markdown rendering, landscape, and tablet-specific layouts.

## Behavior

### Opening and navigation

- The app opens on a new empty session in the selected target.
- A button in the top left corner opens the sessions drawer on the left. Android Back closes the drawer before it does anything else.
- The drawer shows, from top to bottom: the target indicator, a New session action, and the sessions for the selected target.
- New session offers the workspaces from `workspaces()`. The app calls a workspace a project, as the desktop does; the client interface keeps the name workspace.
- A session row shows a snippet of its first prompt on one line, then the project name and the session's state in words, and an Answer mark when a question is waiting. Tapping the row opens that session, and tapping the one already on screen only closes the drawer.
- Switching target selects another list. It never moves a live session.
- The target (for example a host or This device) is visible in the drawer, the session header and every approval card.
- The head sits on the ground above the conversation card. From left to right: the menu button, the session title, the target as an outlined pill, and a button that opens the Calls made drawer on the right.
- The Calls made drawer slides in from the right as a card on the ground, as the desktop's inspector does. It has one tab, Overview, and one section, Calls made, with a count, a fold control and one row per call of the session in order: a terminal glyph and `Verb(target)` in the monospace type, for example `Read(package.json)`. A running call carries a Running chip, a call on outside content carries a From outside chip, and a failed call is drawn in the error colour. An empty list says "No calls yet." Tapping outside the panel or Android Back closes it.

Opening a session means different things by target. Against a host it attaches to a live session, shows bounded history with omissions marked, and takes over control. Opening a saved record is not offered, because saved-record import is excluded from the prototype.

### Transcript

- Rows render by kind: `prompt`, `narration`, `quarantined`, `approval`, `reply`, `error`, `activity`.
- Released content (`quarantined` rows and any row whose payload carries a label) renders as plain text. The label is always visible, in a region the content cannot draw into.
- All row text is plain text in the first pass, including replies. Whether replies render markdown is decided later.
- Control, format, separator and bidirectional characters are escaped so they are visible and cannot reorder surrounding text. Long lines wrap.

### Approvals

- The pending question is a card pinned above the composer while it waits. The transcript holds a summary row for it: what is asked, about what, and whether it is waiting, approved, rejected or answered. The app remembers the person's own answer per row, because the view does not say how a question was answered.
- Every field the [client contract](client-contract.md#first-non-stdio-listener-allowlist) lists for the kind is shown: the full command, the program it resolves to and where it runs for `run`; the host the request connects to, apart from the address as sent, for `fetch`; the path, effect, changes, credentials found and notes for `confirm`; the questions and offered choices for `ask`. A write of untrusted text carries a dashed warning band in words.
- A card taller than its viewport scrolls and says "Scroll to read the rest before you answer" until the end is reached.
- Only the pending request can be answered, and a reply carries the request number the card showed. A reply for a request that is no longer on screen is refused by the client and the app shows the refusal.
- A payload that does not read completely, or that names a request other than the pending one, is shown as unanswerable. Cancel is the only action, and it refuses the question. A question of an unsupported kind is handled the same way.
- A command is approved once and the reply never carries `remember`.
- Every word of a card that came from the agent or from outside is drawn through the escaping text, including the program names and paths of a command, the headings and the choices of a question, and the labels a screen reader speaks. A question takes either a choice or typed text for each prompt: typing clears the choice and choosing clears the text, so what is shown is what is sent.

### Composer and status

The composer is the desktop's two-tier object: a message box with a round send control at its lower right, and under it a strip naming the folder the session works in. A sentence above it says why sending is not possible, or what failed.

| Status | Composer |
|---|---|
| `awaiting_trust` | The box is disabled, and the trust card is pinned above it. |
| `idle`, `completed`, `failed`, `cancelled` | Send is available once there is text. |
| `running`, `waiting` | The round control becomes a stop control labelled Cancel. The draft stays in the field. There is no queue. The control keeps its size, so nothing moves. |
| `detached`, or a view that has `ended` | Disabled with the reason (closed, connection lost, sequence gap). No recovery is claimed. |

### Empty session

A session with no rows shows "Ready in" and the project name, one sentence on what to ask and that nothing is written without approval, and, once trusted, the suggested prompts.

### Look

The app takes its look from the desktop. Colors, radii and type are Leo's resolved tokens for light and dark under the desktop's role names (`mobile/src/theme.ts`), so a role means the same thing on both. The window is the ground (`surface.app`), and the conversation is a card on it with a hairline border and the card radius, inset 8 from the screen edges. The conversation reads at the large role (16 on 24), text inside cards at the default role (14 on 22), activity and metadata in the small roles, and the empty state's heading at the page-title role. Accent is used for the primary action, selection and the active tab.

Icons are drawn from views (`mobile/src/components/icons.tsx`) so the app needs no icon package. They are simpler than Leo's glyphs, and matching those exactly would take an SVG renderer, which is a dependency decision (decision 9) not taken here.

The app icon is the desktop's robot, cut for each platform from `ui/build/icon.svg` by `mobile/scripts/make-icons.sh`: a square with no alpha channel on iOS, and an adaptive icon with legacy variants on Android.

Differences from the desktop: cards carry no elevation shadow, decision cards have no kind icon in the head, the inspector has no Plan section or Files tab, and the composer shows no model, mode or branch controls, because the view carries none of them.

### Layout

Portrait only. Landscape is unsupported: Android ignores a rotation request, and iOS is configured portrait only. A tablet shows the phone layout with the content width capped so lines stay readable. The composer sits against the keyboard while it is open. Accessibility checks (screen reader labels, text size) and keyboard and scrolling behavior are recorded as the device evidence Q1 requires.

## Targets

A `Target` (`mobile/src/targets.ts`) is an id, a label, a description, the suggested prompts and a `create` function that returns an `AgentClient`. `App` takes its targets as a prop and uses the first one. With none, it shows the blank slate: the usual frame with "Nothing is connected", a composer that is disabled and says "No target is connected.", and no menu, target pill or calls control. Choosing between several targets is not built.

The app lists and opens sessions through `listSessions()` and `openSession(id)`, which are capability-gated additions to the common interface. `TargetInfo.sessionList` says whether a target has them. `RpcAgentClient` reports `sessionList: false` and rejects both with `UnsupportedError`, so a target that cannot list shows an empty list. Hiding the drawer for such a target waits for the first one.

The tests run the app against `FakeClient` and `FakeSession` in `mobile/src/__tests__/support/`. A test sets the session's view directly and records what the app asked of the session, so the screens are tested for what they show and send, not for what a target does with it.

## Rendering untrusted text

[Q2](open-questions.md#q2-where-should-shared-rust-presentation-code-live) defers display transforms to the first rendering caller, and the shell is that caller. [UI-006](../../best-practices/ui.md#UI-006) and [SHARE-001](../../best-practices/shared-implementation.md#SHARE-001) put shared display behavior in Rust, and the shell has no Rust binding until stage 2b.

The shell therefore escapes control and bidirectional characters with a TypeScript function (`mobile/src/text/displayText.ts`) that is named and documented as a placeholder. Its ranges are listed explicitly instead of using Unicode property escapes, and a test compares them with the Cc, Cf, Zl and Zp categories for every code point. Content is also wrapped in bidirectional isolates, so right-to-left letters cannot reorder the labels around it. The joiner, the non-joiner and the variation selectors are escaped even where a script or an emoji sequence needs them, so Persian, Indic and emoji text shows them written out, because they can hide text from a person who is approving something. It is used only for display. The TypeScript function is replaced by the shared Rust helper at stage 2b, where the first caller with the binding arrives, and Q2 records that. The escape table covers characters that draw as nothing or reorder text, and leaves the Unicode space separators (no-break space, ideographic space and the like) alone, because ordinary prose, including model replies, contains them and writing each one out would bury the text. A path or program name that differs from another only by such a space reads the same on the card. A screen reader's label cannot be styled, so an escape is spoken as `[U+XXXX]` and content that contains that text literally is spoken the same way. Native marking and label drawing are tested in the app and are not covered by the desktop DOM tests.

## Location and dependencies

The app lives in `mobile/` with its own pnpm lockfile. It consumes `packages/agent-client` by link without a root workspace, as decision 9 requires. The shell uses core React Native components only: no navigation library, gesture library, UI kit, state framework or Expo. The drawer is built from `Animated` and `Pressable`, and the menu icon from views. The one native dependency beyond React Native is `react-native-safe-area-context`. The dependency review is [app-dependencies.md](app-dependencies.md).

pnpm works with Metro when `nodeLinker` is `hoisted`. Gradle also needs the hoisted layout to find `@react-native/gradle-plugin`. [mobile/README.md](../../../mobile/README.md) holds the install, run and iOS conversion steps.

## Done when

- The app builds reproducibly on the Android emulator or a device and on the iOS simulator, and imports `@brave/agent-client`.
- Against a fake client, the drawer lists sessions and opens one in its recorded state, and the approval cards work for all four kinds with approve and reject outcomes.
- A session whose content contains a forged label, control and bidirectional characters, or a very long line, displays labelled, escaped plain text, and nothing it contains changes the surrounding layout or markings.
- The Calls made drawer lists the session's calls, one row per call.
- The dependency review, lockfile checks, affected-check selection and CODEOWNERS entries exist for the new paths (decision 9). CI runs type checks, unit tests and dependency checks. Device builds are not part of CI yet.

## What this does not establish

The fake client proves screens, not a target. It does not show host connectivity, embedded execution, pairing, recovery, lifecycle behavior, security properties of a real session, or that any approval reached a real tool. The app keeps the sessions it has opened so a person can switch back to one with its draft and answers intact, and it does not close them, or the client the target gave it, when it switches session, changes target or exits. A tool finish is matched to the latest running call by arrival order, because no content of a call may decide the row, so two calls in flight can swap their rows. A list request that fails keeps the last list that arrived, or shows an empty list if none has, the same as a target with no sessions. Both wait for a target with a real session lifecycle. Hiding the content behind an open panel from a screen reader is set for both platforms and has not been checked with TalkBack or VoiceOver. Physical devices and a release build are untested.
