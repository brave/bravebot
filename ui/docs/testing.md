# Testing

The gates a change has to pass, and the drivers that prove the window works. Setup is in the [README](../README.md).

## Backend v0.9.0 compatibility

The bridge crates are members of this workspace, so the agent they compile against is
whatever the tree is at.
Session storage and audit types come from `bravebot-session`; watches come from
`bravebot-agent`. Command approvals show any reference routed to standard input,
and network diagnostics include every certificate trust problem reported upstream.
The bridge uses policy-audited model-list decoding and Bedrock's per-model names and
context windows. Trust maps are rooted in the session project. Resuming a terminal
session preserves its saved side conversations and rewind checkpoints.
The UI presents fetch, language-server, plan, credential-exposure and MCP approvals. A
session's first turn starts the MCP servers its settings request.
`crates/ui-bridge/tests/servers.rs` drives that turn through the binary against a stub HTTP MCP
server and a stub model.

Vetted reads now have one-time approval cards, existing output/path approvals include checker
advice, and write approvals show the processor's remark beside the diff.

## Current regression checks

### Writing selectors for Leo controls

Most controls are Leo custom elements whose real `<button>`, `<input>` or `<textarea>` sits
in a shadow root. Playwright's role, label and CSS locators pierce it, but a few habits
matter:

- Find controls by role and accessible name (`getByRole('button', { name: 'Close audit
  inspector' })`) or by `data-test`, not by tag. A class lands on the host, so
  `.tree-tool.dotfiles` names the host and `.tree-find input` names the field inside it.
- `fill` and `press` need the field, not the host: `.composer textarea` works; a bare
  `.session-find` does not.
- A focused Leo control is reported as its host by `document.activeElement`. Compare the
  host as well as the element (`el === document.activeElement ||
  el.getRootNode().host === document.activeElement`).
- Selected and open state is on the element Leo draws: a segmented control's items are
  `[role="option"]` with an `aria-selected` attribute, and a collapse's state is the `open`
  property of the `<details>` in its shadow root.
- A dialog is `getByRole('dialog', { name })`; Leo draws its box in the top layer, so measure
  it through `shadowRoot.querySelector('dialog')`.
- An approving button on a decision card stays shut until the rows it rests on have been on
  screen (PROMPT-4), and a shut Leo button is `aria-disabled`, which Playwright's `isDisabled`
  and `isEnabled` do not read. A driver that approves a card taller than its window scrolls the
  card through first, or waits for `.approve:not([aria-disabled="true"])`.


- `pnpm run drive:manual-walkthrough`: the manual 0.9 verification through the real app
  and backend, with a local model fixture. Runs in CI; detailed coverage is below.
- After building, `node scripts/drive-agent-settings.mjs`: 0.9 settings, hook forms and
  conflicts, watch controls, approval evidence, background cancellation, context/failure
  states, focus restoration and narrow layouts. Uses deterministic IPC fixtures.
- `pnpm run drive:fetch`: a fetch the planner asks for, through the real app and bridge,
  against a model service and a website the script serves itself. The website is the
  witness: an approval sends one request to the host the card showed, a refusal sends none,
  and an earlier approval does not answer a later fetch. No paid inference.
- `pnpm run drive:language-server`: a language server the planner asks for, through the real
  app and bridge, against a model service and a server the script supplies. The server is
  the witness: an approval starts one process, the next message asks nobody and starts none,
  and the process has ended once the app has closed. No paid inference. Not for Windows.
- `pnpm run drive:plan`: a manifest run started from the composer, through the real app and
  bridge, against a model service the script serves itself. The plan writes one file, so
  whether it ran is read off the disk. Covers an approved plan, a declined plan, and a turn
  taken afterwards, which is sent nothing the run said. Then reads both runs' records from
  the session list, and checks that a record has no message box. No paid inference.
- `pnpm run drive:exposure`: a read of a file holding a credential, through the real app and
  bridge, against a model service the script serves itself. An approval hands the model the
  file, a refusal keeps it back, and a new conversation asks again. The script checks that
  the value is nowhere in the window's markup at any point. No paid inference.
- `pnpm run drive:rules`: permission rules from settings files, through the real app and
  bridge, against a model service and a website the script serves itself. A host the person's
  file allows is fetched with no card, a host a rule refuses is neither asked about nor
  fetched, and the banner names what is not in force. No paid inference.
- `pnpm run drive:permission-mode`: the composer's permission mode, through the real app and
  bridge, against a model service the script serves itself, in a directory nobody trusted.
  Accepting edits writes with no card and still asks about a command, the control works while a
  turn runs, plan mode writes nothing and asks nothing, and the Chat menu item walks the
  modes. No paid inference.
- After building, `node scripts/drive-agent-rpc.mjs`: an actual automatic watch turn against
  a local fake gateway, real lifecycle hook subprocesses, context measurements and stop/close.
  Uses an isolated agent home; no paid inference or real credentials.
- `pnpm run build`: bridge and secure-file helper builds, TypeScript, and Electron bundles.
- `make check-ui` from the repository root, or `node --test scripts/*.test.mjs` here after an
  install and a `cargo build -p bravebot-ui-files`: renderer state, models, file access,
  memory retention, bot grounding, avatar motion and traits. File tests use the actual
  secure-file helper, the grounding tests included: they assert that a briefing names the
  memory file and quotes no byte of it, and that a link at the memory path or at the
  briefing path is refused or displaced rather than followed. `marking.test.mjs` needs no
  build: it renders the transcript's own components through `react-dom/server` and asserts
  on the markup, which is what pins the three properties
  [LAYER-5](../../docs/specs/layering.md#LAYER-5) states for this surface.
- After building, `node scripts/drive-turn-details.mjs`: notices, completed-turn usage,
  live and unavailable audits, background delivery, disclosure persistence, cancellation,
  focus restoration and the narrow context drawer, using an isolated profile and synthetic events.
- `cargo test --all` and `cargo clippy --all-targets --all-features -- -D warnings`: bridge
  and secure-file helper gates, including deterministic parent-directory replacement attacks.
- After building, `node scripts/drive-ux-acceptance.mjs`, `node scripts/drive-bot-history.mjs`,
  and `node scripts/drive-models.mjs`: isolated Electron acceptance checks with deterministic
  provider replies. These cover the current tab layout, search, history, drafts, approvals,
  memory, and model selection. Command approvals include the resolved execution plan and
  files written by redirections.
- `node scripts/drive-secure-files.mjs`: real preview and memory IPC, symlink rejection,
  edit conflicts, history deletion and bot recreation, without model calls. To verify packaging,
  build with `node scripts/package.mjs`, then set `SECURE_FILES_APP` to the packaged executable
  when running the same driver. Both `bravebot-rpc` and `bravebot-ui-files` must ship in Resources.
- After building, `node scripts/drive-at-mentions.mjs` (`npm run drive:at-mentions`): the `@`
  list, its keys and Enter's two meanings in the real app against the real bridge and a model
  service the script serves, the file the model was sent, the **Read** row, `@../outside.txt`
  refused at send, and a bot's conversation with no project reading and listing the bot's home
  folder and refusing a name from the project. Disposable `HOME` and profile; screenshots of the
  list in light and dark. The rules themselves are the terminal's, pinned in `crates/mentions`, and
  the bridge's `mentions.offer`, `mentions.named` and `turn.send` refusals in
  `crates/ui-bridge/tests/mentions.rs`.
- For a change to the agent crates the bridge links, `cargo test --all --locked` from the
  repository root covers both sides in one run.
  On Apple Silicon with an Intel Rust toolchain, add `--target aarch64-apple-darwin`
  and `--config 'target.aarch64-apple-darwin.runner=["/usr/bin/arch","-arm64"]'`
  (install the target with `rustup target add aarch64-apple-darwin`). The upstream confinement tests
  need native subprocesses: translated binaries can fail with `Failed to open libRosettaRuntime`.

The session-store and confinement tests need filesystem and subprocess access outside the
Codex sandbox. A missing temporary session record under that sandbox is not a passing test;
rerun with the required access.

The Rust side has eight integration suites under `crates/ui-bridge/tests/`: the protocol
projections, dispatch, the layering rules the crate docs describe, and the refusal
guarantees that the security model rests on.

The interface is tested by **driving the real window** with Playwright, because the things
worth asserting here are the ones a screenshot cannot show — that a fold passes through
intermediate heights rather than snapping, that a column comes back at the width it left
at, that a control keeps keyboard focus through an animation.

| Command | Covers |
| --- | --- |
| `pnpm run drive` | Launch, list sessions, filter them by title and project, group them by checkout, fold one away, start one from a heading, open one |
| `pnpm run drive:session-list` | A list of 260 sessions: that it draws a page and a **Show more** row that moves focus to the first row it drew, that search reaches past the page, that the open session and a working one stay drawn past the page, that grouped every checkout keeps a heading counting all its sessions and a **Show more** row of its own, that the archive has its own page, and that a row's actions menu is mounted only while open and opens, shuts and returns focus by mouse and keyboard |
| `pnpm run drive:resize` | Divider drags, the clamps, keyboard resizing, persistence |
| `pnpm run drive:columns` | Folding each side column, and what is remembered |
| `pnpm run drive:panels` | The context panels, the row of buttons that turns them on and off, and the transcript's tool runs |
| `pnpm run drive:markdown` | Markdown rendering, light and dark |
| `pnpm run drive:models` | Model defaults, composer placement, search and keyboard selection, turn payload, per-conversation persistence, and discovery error recovery. Also bot creation, Avatar refresh and layout, saved bot models, composer changes, and persistence after reload. Uses deterministic replies without paid inference. |
| `pnpm run drive:run` | Approving a command from the window, end to end through a live turn |
| `pnpm run drive:fetch` | Approving and refusing a fetch from the window, end to end, against a local model service and website |
| `pnpm run drive:language-server` | Starting a language server from the window, that it is kept for the conversation, and that it ends with the app |
| `pnpm run drive:plan` | Starting a manifest run from the composer, approving and declining its plan, that the run stays out of the conversation, and that its record is read and cannot be typed into |
| `pnpm run drive:exposure` | Answering a read that would expose a credential, and that the value is never drawn |
| `pnpm run drive:shown` | That no decision card takes an approval before the rows it rests on have been on screen: every kind drawn too tall for a 900x560 window, shut until scrolled through, shut again on a change of width, deaf to a press while shut, and a refusal taken at once. Screenshots of a card waiting and read, light and dark |
| `pnpm run drive:rules` | Permission rules from settings files: what is refused, what is not asked, and what is reported as not in force |
| `pnpm run drive:permission-mode` | The composer's permission mode: accepting edits, plan mode, a change while a turn runs, and the menu shortcut |
| `pnpm run drive:ask` | Answering a series of questions the planner asks, likewise live |
| `pnpm run drive:menu` | The application menu: what it offers, what it greys, and what it refuses to offer |
| `pnpm run drive:export` | Exporting a conversation to text, Markdown and PDF, with and without the tool calls, and what the file leaves out either way |
| `pnpm run drive:fork` | Cutting a session in two: that the fork holds the right half and the session it came from is untouched |
| `pnpm run drive:rewind` | Undo and rewind from each entry point against the real bridge: the file back on disk, the turn gone from the transcript, its prompt in the composer, the coverage warning for a command, and every entry point greyed while a turn runs |
| `pnpm run drive:tree` | The file tree: listing, expanding, the dotfile toggle, the name filter, and that a session with no root and a symlink out of the project both list nothing |
| `pnpm run drive:theme` | Appearance: System / Light / Dark painting and persistence, legacy palette fallback, and the export renderer's light-only guarantee |
| `pnpm run drive:bots` | Bots: that the column has two lists and remembers which, that a bot survives a relaunch with what was typed into it, and that two bots have different faces while one bot keeps its own across a rename, asserted on the *form* the seed built, since the face is turning while it is looked at. Also the archive: that a bot put away survives field-for-field and comes back as itself, and that deleting one asks before it does anything |
| `pnpm run drive:packaged` | A built `.app`: that a release hides the developer items and finds its agent |
| `pnpm run drive:bot-turn` | A live turn as a bot: that a purpose nobody typed reaches the model, that the memory file is real and in the checkout, and that reopening the bot resumes the same session |
| `pnpm run drive:bot-memory` | That a bot is asked to keep its memory current without anybody asking it to: that one which has gone quiet is handed its briefing again with a line saying so, that the count resets on the nudge rather than on every turn, and that a prompt somebody typed to read like the app's own house-keeping is drawn as a prompt in a reopened transcript, with its words and the cut that is taken on its ordinal |
| `pnpm run drive:visual` | The gallery: every surface as screenshots, in light and dark, with a hit-target audit and forced-colours captures; see [below](#the-visual-gallery) |
| `pnpm run drive:perf` | The performance budgets on a 1,000-session list and a 500-entry transcript; see [below](#performance-budgets) |
| `node scripts/drive-turn.mjs` | A live inference request through the window, to prove the binary carries its credentials rather than inheriting them |
| `node scripts/drive-models-live.mjs` | Live inference before and after changing the conversation model, checking which model the agent actually used |
| `scripts/smoke-turn.sh` | A live turn straight through `bravebot-rpc`, no app |

`drive:menu` cannot press a menu's own keystroke: Playwright's keyboard reaches the web
contents over CDP, and an AppKit key equivalent never sees it. So it asserts the accelerator
*string* as a contract and drives the effect by clicking the item. The packaged case it cannot reach at all, because it drives a checkout;
`drive:packaged` covers that separately, against a real bundle. What is left for a hand is
⌘C/⌘V actually reaching the composer — the role assertion proves the item is there, only a
person proves the keystroke arrives.

Each driver launches the app, prints a line per assertion and leaves screenshots under
`/tmp/bravebot-ui/` or a driver-specific path in `/tmp`. Eight of the checks cost real tokens:
`drive:markdown`, `drive:run`, `drive:ask`, `drive:bot-turn`, `drive:bot-memory`, `drive-turn.mjs`,
`drive-models-live.mjs` and `smoke-turn.sh` send an
actual prompt. Live checks require backend credentials from the process environment or compiled
into the agent binary; the UI's setup help does not store credentials. See
[setup](setup.md#credentials).

Some older drivers share the default profile's `bravebot-ui.json`, so one that leaves a column folded — or a panel turned off —
would make the next one's measurements meaningless. `drive-columns.mjs` normalises the columns at
the start of a run and puts them back at the end, `drive-panels.mjs` turns every panel back on
before it measures one and again before it finishes, and `drive-tree.mjs` puts the file tree back
on before it tests it. Anything new in this
area should do the same, and a driver that seeds a fixture should replace its own key rather than
the file: the other keys are somebody's arrangement of this window.

`drive-theme.mjs` does the same for the `theme` key. It also verifies that the export document is
pinned to light and that its renderer bundle cannot switch appearance at runtime.

### Which drivers touch your real profile

**Many drivers run against the real user profile.** A driver that launches Electron without
`--user-data-dir` uses the app's real `userData`, so it reads and writes your `bravebot-ui.json`
(columns, bots, recents, theme) and `experience.json`, and the agent it starts uses your `HOME`,
which means your real `~/.bravebot` sessions, history and configuration. Read a driver before
running it, and do not run one you have not read on a machine whose bots or sessions you care
about. These launch without `--user-data-dir`:

`drive`, `drive-ask`, `drive-bot-memory`, `drive-bot-turn`, `drive-bots`, `drive-columns`,
`drive-export`, `drive-fork`, `drive-markdown`, `drive-menu`, `drive-packaged`, `drive-panels`,
`drive-resize`, `drive-run`, `drive-smoke`, `drive-theme`, `drive-tree` and `drive-turn`.

Of those, **`drive-run`, `drive-ask` and `drive-markdown` send live model turns** against the
real profile (and so do `drive-bot-turn`, `drive-bot-memory` and `drive-turn`), which spends
credits from whatever backend is configured. `drive-columns`, `drive-panels`, `drive-tree` and
`drive-theme` put back the key they change; do not assume any of the others do.

These pass `--user-data-dir` with a temporary profile, so the app's own state is isolated:

`drive-about`, `drive-agent-settings`, `drive-at-mentions`, `drive-bot-history`, `drive-conversation-workflow`,
`drive-manual-walkthrough`, `drive-models`, `drive-models-live`, `drive-remembered-trust`,
`drive-rewind`, `drive-secure-files`, `drive-session-list`, `drive-turn-details`, `drive-ux-acceptance`,
`drive-vetting`, `drive-visual` and `drive-perf`.

The flag isolates only the app's own state. Most of these also replace the bridge with IPC
fixtures, so they need no credentials and make no requests; `drive-models-live` does not, and
sends a live turn through the isolated profile using your real credentials.
`drive-manual-walkthrough` and `drive-rewind` also set a disposable `HOME`. `drive-agent-rpc` never opens a window:
it spawns the bridge on its own with a temporary `HOME`. When you write a new driver, launch with
a temporary `--user-data-dir` and mock or isolate whatever the bridge would reach.

### The visual gallery

`scripts/drive-visual.mjs` is the tool for looking at the interface rather than asserting on it.
It launches the built app with a temporary profile and a mocked bridge (IPC fixtures, like
`drive-models.mjs`), so it is deterministic and makes no requests:

```bash
pnpm exec electron-vite build && node scripts/drive-visual.mjs
```

Screenshots go to `VISUAL_OUTPUT`, or `bravebot-visual/` under the system temp folder when it is
unset, as `<scene>-light.png` and `<scene>-dark.png`. The scenes are numbered, and cover the welcome screen, a conversation with
markdown, tables and code, a running turn with tool calls, each decision card (trusted and
untrusted), a command's output and the vouch question, a series of questions, a finished and a
failed turn, the inspector's file tree, the model and export menus, the find bar, Permissions,
File watches, Appearance, About, Agent settings, Bots, the queue with attachments, and the
minimum-size window (900×560; the others use 1440×900).

Two things make it more than a folder of images:

- **Stress fixtures.** One scene loads 80 sessions across six projects, one with a 120-character
  title and one in a path twelve levels deep, and shows the list flat and grouped at the minimum
  window size. The queue scene shows five attachments and four queued messages there too. (The
  400-line diff, 3,000-line code block and 500-entry transcript in the plan are not in the
  gallery; the 500-entry transcript is the perf driver's fixture.)
- **Forced colours.** The last scene turns on `emulateMedia({ forcedColors: 'active' })` and
  captures the quarantine and the untrusted card, so the security markings are seen without their
  colour.

Every scene also audits interactive targets, piercing shadow roots, and the run fails if any is
under 28px (set `VISUAL_ALLOW_SMALL=1` to list them without failing), if a scene throws, or if the
renderer reports an error. `VISUAL_PROBE` takes a JavaScript expression evaluated in the page with
the conversation open and prints the result as JSON, and `VISUAL_ONLY_CONVERSATION=1` stops the
run there; both are for layout work. Use the gallery for side-by-side review before and after a
change to styles: it is the check for the [quality bar](development.md#the-quality-bar).

### Performance budgets

`scripts/drive-perf.mjs` enforces the budgets on a long session list and a long conversation. It
is new alongside the redesign; this describes the plan it implements, so read the script for the
exact thresholds and methods. Like the gallery it uses a temporary profile and a mocked bridge
that lists **1,000** sessions, and loads a deterministic **500-entry** transcript, then measures
in the page from `requestAnimationFrame` timing and Chrome DevTools Protocol `Performance`
metrics:

| Budget | Measured |
| --- | --- |
| Keystroke to next paint in the composer under **16ms** | Typing into the message box; also checks that typing does not re-render the transcript |
| Scrolling holds **60fps** | Frame times while scrolling the transcript |
| Streaming a long answer drops no frames | Frame times while a reply arrives |
| Menus open in under **100ms** | The model menu, the header's More menu and the find bar |
| The window opens with **1,000** stored sessions: first row painted in under **1s**, no long task over **50ms** | `longtask` entries and the first `.session-row` paint, watched from before the page's script runs; also checks the list draws one page of rows and a **Show more** row |

It exits non-zero when a measurement is over its budget. Software rendering or a busy machine
inflates frame times, so `PERF_TOLERANCE=2` scales the time budgets rather than editing the
script. The budgets exist because of [render isolation](development.md#render-isolation): a change
that makes a keystroke re-render the transcript is the regression this catches.

## Automated 0.9 manual walkthrough

Run `pnpm run drive:manual-walkthrough` to build and exercise the manual verification
steps through the real Electron app, preload, main process, Rust bridge, file helper,
watch poller, and hook subprocesses. It uses a disposable HOME, app profile and project;
it neither reads your credentials nor changes your hooks. A local HTTP model fixture
returns scripted planner tool calls, checker verdicts, and processor output. No paid
model requests are made.

The driver asserts:

1. Actual agent diagnostics and refresh; valid/invalid run overrides and clearing;
   arrow/Home/End tab navigation, Tab/Shift+Tab containment, Escape and restored focus.
2. A fake API key loaded from `provider.local.options.apiKey`, wrong-backend diagnosis
   when the Brave default is selected, recovery through the model picker, measured
   context, and cancellation of an in-flight model request. The gateway asserts the
   expected Authorization header; no real credentials are used.
3. A hook saved through the UI executes; removing it through the UI prevents later execution.
4. Missing-file errors, an actual file change triggering an automatic turn without
   leaking file bytes, individual watch stop, stop-all, and no turn after a stopped watch changes.
5. Standing-trust refusal followed by vetted-read rejection/approval, content isolation
   at the planner boundary, no standing grant in fresh sessions, background-session
   routing and cancellation of a pending approval.
6. Real isolated processor output and remarks, a rejected write leaving the file
   unchanged, and an approved write producing exactly the expected file contents.
7. Narrow settings/watch/approval layouts with reachable decision controls and cleanup.
   The Agent settings button stays within both sidebar tabs at the default, minimum
   and maximum panel widths (250, 200 and 400 pixels).

The native file-picker **result** is supplied by the test; the actual selection handler,
JSON validation and configuration loading run normally. OS dialog appearance, real
provider credentials/availability and live-model tool selection remain manual checks.
The existing `drive:agent-settings` driver separately covers fixture-only error and
checker states such as unsafe/inconclusive verdicts and summarised context.

On failure, the walkthrough prints the current UI and fixture errors and writes
`/tmp/bravebot-walkthrough-failure.png`. It closes the app/server and removes its
isolated files on exit. On Linux it needs a display; use
`xvfb-run -a node scripts/drive-manual-walkthrough.mjs` after building for a headless run.

## What CI runs

`.github/workflows/ci.yml` is the gate on a pull request, and it is not the same set as the
table above. Two jobs:

- **Typecheck**: `pnpm install --frozen-lockfile` with `ELECTRON_SKIP_BINARY_DOWNLOAD=1` (the Electron
  *package* is needed for types; the 100 MB binary is not, since nothing here launches a
  window) and then `pnpm run typecheck`.
- **Lint and test the bridge** is the repository's own `check` job, which runs
  `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test --all --locked`
  over every workspace member. The `Front end` job adds the desktop build, Node regression tests, and
  the real manual walkthrough under Xvfb with a local model fixture. `make check-ui` at the
  repository root runs the Node tests on their own, `marking.test.mjs` among them, so the marking
  rule has a local gate as well as this job: it installs with `--ignore-scripts` and builds only
  `bravebot-ui-files`, since nothing in that set opens a window. What stays this job's alone is the
  Electron build and the walkthrough.

The workflow does not run the other Electron drivers, packaged-app checks, or the
upstream agent's full test suite. Run the applicable local checks above. The exception is the
Windows installers, which `Install on Windows` installs and starts on a runner of each
architecture; [releasing](../../docs/development/releasing.md#the-windows-installers) says what it
checks.

So Clippy *is* a lint step, on the Rust side; there is none on the TypeScript side, where `tsc`
is the whole gate. `cargo fmt --all -- --check` is deliberately absent because the bridge is not
rustfmt-clean; introducing that gate requires a separate formatting change. The reasoning for each of these lives in
comments in the workflow itself.

The manual walkthrough runs in CI without credentials. Drivers that require a live
provider or a packaged app remain local checks.
