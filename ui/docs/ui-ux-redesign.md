# UI and UX implementation audit

Historical audit of `codex/ui-ux-redesign`, subsequently merged through PRs #37,
#39, #40, #41 and #42. Counts and live-run observations below describe the original
implementation, not a fresh test result. Use [testing](testing.md) for current gates.

The full review is the scope. Runtime checks use the actual Electron application and isolated test projects.
Compilation, interaction assertions, screenshots, and live-agent checks provide
complementary evidence; their boundaries are recorded below.

## Requirements and evidence

- [x] 1. Persist per-session drafts and scroll; background sessions; visible run,
  approval, completion and failure states; safe explicit stopping.
- [x] 2. Actionable errors with diagnostic details and safe recovery actions.
- [x] 3. Growing composer, secondary toolbar, header export, keyboard hints,
  drafting during runs, explicit queue, persistent Stop.
- [x] 4. Conversation-first widths, responsive inspector drawer, focus mode,
  softer message colors and retained avatars.
- [x] 5. Overview / Changes / Files inspector; compact empties; persistent
  pending decisions; event navigation; accurate trust distinctions.
- [x] 6. Near-bottom follow, new-activity button, approval navigation,
  reduced-motion scrolling and retained reading position.
- [x] 7. Approval summaries, readable expandable diffs, visible permission scope,
  revocation controls, separate approval and application outcomes.
- [x] 8. Bot overview and conversation history; continue/new actions; bot badges;
  titles and status; search; session pin/archive and discoverable menus.
- [x] 9. Wider bot editor; project explanation/duplication; readable/raw memory,
  update history, deliberate edit/reset; persistent-memory explanation.
- [x] 10. Copy/wrap code; conversation search; project-wide filename search;
  preview/open-in-editor; validated local references; explicit file attachments.
- [x] 11. Typography, contrast, density, hit targets, composer name, status
  announcements, modal focus containment/restoration, keyboard verification.
- [x] 12. Welcome actions and recent projects; first-project guidance; backend
  readiness/diagnostics; recent models; app-supported capabilities and context limits.

## Acceptance evidence

The Electron driver is `scripts/drive-ux-acceptance.mjs`. It launches the production
build with a fresh user-data directory and disposable project. It records screenshots
(currently 42) and fails on assertion failures or renderer exceptions. Screenshots
were opened and visually inspected, including the final dark scrollbar and
memory-reset confirmation refinements.

| Requirement | Visual evidence (screenshot prefixes) | Functional evidence |
| --- | --- | --- |
| 1. Conversation continuity | 02, 14–16, 40 | Independent drafts across switching and reload; background question remains reachable; Stop invalidates pending question; queue remains paused until Resume; failed first turn remains one sidebar row even with a stale session list. Native restart also retained a draft. |
| 2. Error recovery | 15, 17, 35–36, 38, 40 | Cancellation, rate-limit and authentication states; continuation appends to an existing draft and does not resend actions; setup help and diagnostics open; readiness can be checked again. |
| 3. Composer | 02, 10, 13, 15, 17, 37 | Growing multiline input; draft while busy; queue, Stop and Resume; attachments; header Export; keyboard hints. Queue waits across both the start and completion of automatic memory maintenance. |
| 4. Layout and identity | 02–04, 19, 26–27 | Focus hides sidebars and restores their prior state; reading width stays bounded; 950 px window uses an inspector overlay; comfortable/compact density; light/dark colors and avatars. |
| 5. Inspector | 02, 29–32 | Overview, Changes and Files; compact empties; pending write visible; one normalized path despite a reference-backed tool target; applied state follows the tool outcome. |
| 6. Reading position | 33–34 | Incoming events preserve scroll position while reading; switching restores it; New activity returns to the bottom; reduced-motion preference exercised. |
| 7. Approvals and grants | 05–06, 28–31 | Trusted parent path, untrusted exception and exact command scope; revoke both grant types; expanded numbered diff; waiting → applying → applied distinct from the approval decision. Native backend also applied a reviewed disposable-file update. |
| 8. Bots and sessions | 14, 18–19, 39–41 | Pin, archive and restore; bot conversation history; Continue and New; badges/status; duplicate appears as a separate bot in its chosen project. Search controls inspected. |
| 9. Memory/editor | 20–25, 41 | Real main-process storage: edit → save → readable/raw → explicit reset → revision review → restore → save. Project remains fixed; duplication creates a new bot. Reset confirmation scrolls clear of the sticky footer. |
| 10. Content tools | 07–10, 32 | Conversation search, copy to clipboard, code wrap, local-reference preview, default-app dispatch, attachment selection/removal, filename search. Real file boundary/search/attachment validation covered by regression tests and native file search/preview. |
| 11. Accessibility | 03–08, 20, 23, 27 | Modal Tab wrapping, Escape and launcher-focus restoration; input names; visible focus; density; semantic status/alerts; readable accent text; built-in accent/message foreground contrast tests. |
| 12. Onboarding/models | 01, 11–12, 35–36, 38–39 | Welcome/recent projects; project trust choice; disabled send with missing backend; Check again; model selection, Recent ordering, context windows and only app-supported capability badges. |

## Verification boundaries

The deterministic driver replaces provider/bridge replies and native file-picker
answers, including permission-list fixtures and the default-app dispatch endpoint.
It uses the real renderer, preload IPC, experience persistence, bot definitions,
and memory/history file operations. It does not spend tokens with a provider or
alter a user's project. These fixtures verify UI state and interaction behavior;
they do not claim a new live-provider integration run for every screenshot.

Earlier native tests exercised the actual backend: an unanswered question remained
active in a background conversation; answering it sent the queued follow-up; Stop
woke a pending request without approval; an explicitly resumed queue completed;
and a reviewed untrusted patch changed only the disposable sample file. Native
search found an unopened source path and previewed its contents. The draft survived
an application quit/relaunch. Inconsistent native captures were discarded and the
remaining visual work was completed with the authorized Electron driver.

## Fixes found during acceptance

- Cancellation now wakes a waiting confirmer; reused request IDs cannot rewrite
  answers or approvals from earlier turns.
- Failed first turns migrate to their durable ID without losing their sidebar row.
- Reference-backed writes and approval paths produce one Changes row.
- Queued work stays paused after Stop/error, but a new explicit task with no old
  queue does not inherit a stale pause. Automatic bot memory maintenance reserves
  the session until its completion announcement releases the queue.
- Recovery preserves an existing draft and asks to review current project state
  before continuing, rather than automatically repeating writes or commands.
- Dialog focus, sticky editor controls, readable memory headings, reset visibility,
  primary-button styling, text accent contrast and dark scrollbars were corrected
  against screenshots. Empty archives have a direct, useful explanation.

## Checks and reproduction

- `npm run build`: passed (TypeScript, bridge, main, preload and renderer).
- `node --test scripts/ux-state.test.mjs scripts/bot-model.test.mjs`: 14 passed.
- `cargo test -p bravebot-bridge`: 87 passed during this implementation.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `node scripts/drive-ux-acceptance.mjs`: full acceptance run passed.
- `git diff --check`: passed.

Build before running the driver. GUI execution needs permission outside the
sandbox on macOS. By default screenshots go to `/private/tmp/bravebot-ux-final`;
set `UX_OUTPUT` to choose a different evidence directory. Provider credentials
remain a deployment/environment requirement, and the setup UI describes that
boundary without exposing credentials.
