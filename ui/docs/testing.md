# Testing

The gates a change has to pass, and the drivers that prove the window works. Setup is in the [README](../README.md).

## Backend v0.9.0 compatibility

The submodule pins `5d56df1d`, upstream `main` after the `v0.9.0` release.
Session storage and audit types come from `bravebot-session`; watches come from
`bravebot-agent`. Command approvals show any reference routed to standard input,
and network diagnostics include every certificate trust problem reported upstream.
The bridge uses policy-audited model-list decoding and Bedrock's per-model names and
context windows. Trust maps are rooted in the session project. Resuming a terminal
session preserves its saved side conversations and rewind checkpoints.
The UI does not yet present fetch-host, language-server, or manifest-plan approvals;
those new requests are refused without consuming another pending approval.

Vetted reads now have one-time approval cards, existing output/path approvals include checker
advice, and write approvals show the processor's remark beside the diff.

## Current regression checks

- `npm run drive:manual-walkthrough`: the manual 0.9 verification through the real app
  and backend, with a local model fixture. Runs in CI; detailed coverage is below.
- After building, `node scripts/drive-agent-settings.mjs`: 0.9 settings, hook forms and
  conflicts, watch controls, approval evidence, background cancellation, context/failure
  states, focus restoration and narrow layouts. Uses deterministic IPC fixtures.
- After building, `node scripts/drive-agent-rpc.mjs`: an actual automatic watch turn against
  a local fake gateway, real lifecycle hook subprocesses, context measurements and stop/close.
  Uses an isolated agent home; no paid inference or real credentials.
- `npm run build`: bridge and secure-file helper builds, TypeScript, and Electron bundles.
- After building, `node --test scripts/*.test.mjs`: renderer state, models, file access,
  memory retention, avatar motion and traits. File tests use the actual secure-file helper.
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
- For a submodule update, also run
  `cargo test --manifest-path vendor/bravebot/Cargo.toml --target-dir target --all --locked`.
  On Apple Silicon with an Intel Rust toolchain, add `--target aarch64-apple-darwin`
  and `--config 'target.aarch64-apple-darwin.runner=["/usr/bin/arch","-arm64"]'`
  (install the target with `rustup target add aarch64-apple-darwin`). The upstream confinement tests
  need native subprocesses: translated binaries can fail with `Failed to open libRosettaRuntime`.

The session-store and confinement tests need filesystem and subprocess access outside the
Codex sandbox. A missing temporary session record under that sandbox is not a passing test;
rerun with the required access.

The Rust side has eight integration suites under `crates/bravebot-bridge/tests/` — the protocol
projections, dispatch, the layering rules the crate docs describe, and the refusal
guarantees that the security model rests on.

The interface is tested by **driving the real window** with Playwright, because the things
worth asserting here are the ones a screenshot cannot show — that a fold passes through
intermediate heights rather than snapping, that a column comes back at the width it left
at, that a control keeps keyboard focus through an animation.

| Command | Covers |
| --- | --- |
| `npm run drive` | Launch, list sessions, filter them by title and project, group them by checkout, fold one away, start one from a heading, open one |
| `npm run drive:resize` | Divider drags, the clamps, keyboard resizing, persistence |
| `npm run drive:columns` | Folding each side column, and what is remembered |
| `npm run drive:panels` | The context panels, the row of buttons that turns them on and off, and the transcript's tool runs |
| `npm run drive:markdown` | Markdown rendering, light and dark |
| `npm run drive:models` | Model defaults, composer placement, search and keyboard selection, turn payload, per-conversation persistence, and discovery error recovery. Also bot creation, Avatar refresh and layout, saved bot models, composer changes, and persistence after reload. Uses deterministic replies without paid inference. |
| `npm run drive:run` | Approving a command from the window, end to end through a live turn |
| `npm run drive:ask` | Answering a series of questions the planner asks, likewise live |
| `npm run drive:menu` | The application menu: what it offers, what it greys, and what it refuses to offer |
| `npm run drive:export` | Exporting a conversation to text, Markdown and PDF — with and without the tool calls, and what the file leaves out either way |
| `npm run drive:fork` | Cutting a session in two: that the fork holds the right half and the session it came from is untouched |
| `npm run drive:tree` | The file tree: listing, expanding, the dotfile toggle, the name filter, and that a session with no root and a symlink out of the project both list nothing |
| `npm run drive:theme` | Themes: that previewing repaints before anything is written down, that Escape restores exactly, that every derived token survives a palette, that editing a palette repaints without a relaunch, and that a PDF stays white regardless |
| `npm run drive:bots` | Bots: that the column has two lists and remembers which, that a bot survives a relaunch with what was typed into it, and that two bots have different faces while one bot keeps its own across a rename — asserted on the *form* the seed built, since the face is turning while it is looked at. Also the archive: that a bot put away survives field-for-field and comes back as itself, and that deleting one asks before it does anything |
| `npm run drive:packaged` | A built `.app`: that a release hides the developer items and finds its agent |
| `npm run drive:bot-turn` | A live turn as a bot: that a purpose nobody typed reaches the model, that the memory file is real and in the checkout, and that reopening the bot resumes the same session |
| `npm run drive:bot-memory` | That a bot is asked to keep its memory current without anybody asking it to: that one which has gone quiet is handed its briefing again with a line saying so, that the count resets on the nudge rather than on every turn, and that a turn the app sent is drawn as house-keeping in a reopened transcript rather than as a prompt |
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

`drive-theme.mjs` does the same for the `theme` key, and has one duty beyond the file: it writes
palettes into `themes/` beside it, so it removes the ones it wrote on the way out however it exits,
and removes the directory too if it was the one that made it.

## Automated 0.9 manual walkthrough

Run `npm run drive:manual-walkthrough` to build and exercise the manual verification
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

- **Typecheck** — `npm ci` with `ELECTRON_SKIP_BINARY_DOWNLOAD=1` (the Electron *package* is
  needed for types; the 100 MB binary is not, since nothing here launches a window) and then
  `npm run typecheck`.
- **Lint and test the bridge** — one checkout with submodules, so it compiles the same agent
  revision the gitlink pins, then `cargo clippy --all-targets --all-features -- -D warnings`
  and `cargo test --all`, followed by the desktop build, Node regression tests, and
  the real manual walkthrough under Xvfb with a local model fixture.

The workflow does not run the other Electron drivers, packaged-app checks, or the
upstream agent's full test suite. Run the applicable local checks above.

So Clippy *is* a lint step, on the Rust side; there is none on the TypeScript side, where `tsc`
is the whole gate. `cargo fmt --all -- --check` is deliberately absent because the bridge is not
rustfmt-clean; introducing that gate requires a separate formatting change. The reasoning for each of these lives in
comments in the workflow itself.

The manual walkthrough runs in CI without credentials. Drivers that require a live
provider or a packaged app remain local checks.
