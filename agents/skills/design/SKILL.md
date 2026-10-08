---
name: design
description: The design reference for the Bravebot desktop UI (ui/src/renderer). It describes the current look of the application and the quality bar for any new or changed surface. Use before designing, building, restyling or reviewing any component, layout, dialog, menu, card, icon, copy, or stylesheet in ui/, and again against the final diff. Covers the window structure, the Nala-only rule, tokens, type, icons, states, layout stability, security markings, performance budgets, and the gates to run.
---

# Design

This file describes how the desktop UI looks and behaves today, and the bar every new or changed
surface in `ui/` meets. New work matches what is described here. When a change alters the
application's look (a new surface pattern, a token, a layout rule), update this file in the same
change so it stays the reference.

Keep the work proportional. A one-line copy fix needs the microcopy rules and the gates. A new
surface needs all of it.

## The look

- **Quiet neutral chrome.** Surfaces are monochrome, dividers are hairlines, and chrome is kept to
  what is needed. Leo blue appears only on primary actions, focus, selection accents and links.
  Status colour (success, warning, error, info) comes only from `--leo-color-systemfeedback-*`,
  through the `--status-*` tokens.
- **Every element has a purpose.** Each piece of UI has to help someone read or steer the agent
  right now. If it does not, remove it, merge it, or move it to where it is needed. Status text
  that restates state the UI already shows (the working row, the composer, the sidebar dot) is
  removed. Only states a person can act on get a marker.
- **Content goes where it is needed.** A header carries identity and actions only. A note about
  history, trust or vetting is a note at the top of the transcript. A blocker on sending is a tray
  on the composer. An application problem is a toast. Banners are not stacked in a header.
- **Layout can change, capability stays.** Restructure freely, but keep every capability,
  accessible name, security marking and `data-test` hook unless a deliberate evaluation removes it.
- **Nothing approves for the person.** No approval accelerators (a keyboard shortcut that approves a
  decision card), no card that takes focus from the composer, no paste attach (a file goes only
  through the native picker's grant, an `@` name the bridge reads back out of the sent prompt and
  confines to the conversation's folder, or a file a person dropped, which the preload takes only
  from a trusted drop event and the main process grants by an opaque id). See
  `docs/best-practices/ui.md#UI-003`, `ui/docs/security.md` and `ui/docs/file-access-security.md`.

## The window

The structure is set in `styles/shell.css` and `columns.ts`. New surfaces fit into it.

- **Three columns.** Sessions and bots on the left, the conversation in the middle, the context
  inspector on the right. The side columns are resizable and foldable, and their widths and fold
  states persist. Folding both is the focus mode; there is no separate one.
- **Ground and cards.** The sidebar sits flush on the app ground (`--surface-app`). The
  conversation and the inspector are two cards (`--surface-panel`, hairline border,
  `--radius-control`) inset by `--card-gap` (8px) from the window edge and from each other. The
  gap between them is the divider that resizes the inspector. With the sidebar folded, the
  conversation card gains ground on its left.
- **Heads.** The three column heads are `--titlebar` (44px) tall so they line up across the window.
  The sidebar head leaves room for the inset traffic lights (`--lights`, 0 outside macOS). The
  strip of ground above the cards drags the window; every control inside a drag region opts out.
- **Narrow windows.** Below 1120px the inspector overlays the conversation and takes no grid width.
  The minimum window is 900×560, and the column minimums are enforced in `columns.ts`, not in CSS.
- **Reading column.** Conversation content is centred at `--reading-width` (720px). The composer
  is lined up under it and is as wide as the column.
- **Settings.** Settings take the whole window: a page list on the app ground where the sidebar
  is, and the page on a raised card. The chat view stays mounted underneath and is inert. The
  pages are General, Connectors and Agent settings; Connectors is a page here and has no entry
  in the sidebar. The header shows a back arrow only on a page inside a page (a connector's setup,
  the review before connecting), and it goes back one level; Esc does the same. A top-level page
  has no arrow, and "Back to BraveBot" in the page list leaves settings.
- **Inner elements in settings.** An element inside the content area of a settings page (a table,
  a bordered box, a collapsible, a connector card or row) uses `--radius-control`. Only the page's
  own card uses `--radius-card`.
- **Dividers.** A divider is a seam. It is invisible until hovered, focused or dragged, then shows
  a 2px accent line.
- **Dialogs.** Dialogs use `Modal` in one of four widths: `sm` 400, `md` 500, `lg` 760, `xl` 1080.
  The head is a title and at most one line saying what the dialog is for. The primary action is at
  the right of the footer. A secondary action that should stand apart (Cancel, Stop all) carries
  `modal-leading` and goes to the left. A dialog of a sentence or two passes `compact` to `Modal`
  to tighten the vertical spacing around the body; the archive confirmation also keeps Cancel next
  to the primary action at the right.

## Hard rules

1. **Nala only.** Import components only from `ui/src/renderer/nala.ts`. Use `--leo-*` tokens or
   the semantic layer in `styles/tokens.css` that aliases them for colour, type, spacing, radius,
   elevation and motion. No raw hex, no raw `box-shadow`, no odd pixel values.
   `node scripts/check-nala.mjs` (run by `npm run typecheck`) must pass. It only gets stricter.
   Never loosen it or raise a ratchet to make a change pass.
2. **Icons are Leo `Icon`s.** Sizes are 12 (caption), 14 (meta), 16 (controls) and 20 (empty
   states and dialog headers), from one stroke family. No inline `<svg>` except the pixel avatar
   artwork in `BotAvatar.tsx`, and no unicode glyphs as icons (`↑ ↓ ✓ ▸ › ⋯`, CSS `content:`
   arrows). Keyboard hints like "⌘N" are typography. `name` props are typed `IconName` literals,
   never `string`. Check that the name exists in `node_modules/@brave/leo/icons/*.svg`.
3. **Icon-only controls use `IconButton`** (`components/IconButton.tsx`): a Leo `Button`, an `Icon`
   and a tooltip showing the label and shortcut. A tooltip adds to `aria-label` and never replaces
   it.
4. **Security markings stay unmistakable.** `.quarantine` keeps its hatched treatment,
   `.confirm.untrusted` stays visually distinct, and trust Labels stay prominent. Do not change the
   class contracts in `scripts/marking.test.mjs` or weaken LAYER-5 trust marking. Under
   `@media (forced-colors: active)` a marking is a system-colour border plus a text label, never
   colour or a background image alone. Markdown rules never reproduce the app's own trust signals
   (the hatched border, the warn bar).
5. **Keep the hooks.** Preserve `data-test` attributes, IDs and the structural classes tests query
   (`.session`, `.bubble.user`, `.confirm`, `.approve`, `.tool-run`, `.quarantine-head`,
   `.model-trigger`, `.decided`, and so on). Where a visible label or DOM location changes, update
   the drivers in `ui/scripts` in the same change. Button texts that tests or security docs pin
   stay word for word.
6. **No duplicate or dead CSS.** Styles live in the module for their surface under
   `src/renderer/styles/`, and a selector is declared once per module. `legacy.css` only shrinks;
   nothing is added to it. Delete rules that target things that no longer exist, including native
   `button` rules aimed at what are now Leo hosts.

## Tokens

`styles/tokens.css` is the only place a value is named. If a value is missing, add a role there
that aliases a Leo token, then use it.

| Family | Roles |
| --- | --- |
| Surface | `--surface-app`, `-panel`, `-raised`, `-sunken` (code and evidence), `-hover`, `-selected`, `-scrim` |
| Border | `--border-hairline`, `-subtle`, `-strong`, `-focus` |
| Ink | `--ink-primary`, `-secondary`, `-tertiary`, `-disabled`, `-link`, `-accent` |
| Status | `--status-success`, `-warning`, `-error`, `-info` and their `-bg` |
| Type | `--type-page-title` 28, `-heading` 20, `-title` 16, `-lede` 16, `-body` 14, `-meta` 12, `-caption` 11, `-code`, `-code-body`, with `-strong` forms |
| Radius | `--radius-chip`, `-control`, `-card`, `-field`, `-bubble`, `-pill` |
| Elevation | `--shadow-raised`, `--shadow-floating`, `--shadow-focus` |
| Motion | `--motion-fast` for hover and press, `--motion-panel` for folds |

The names at the foot of `tokens.css` under "Legacy aliases" (`--bg`, `--ink-dim`, `--accent`) are
not used in new code.

## Quality bar for every surface

A surface is done only when all of this holds in light and dark, at 1440×900
and at the minimum window size.

**Grid and alignment**
- Spacing comes from the Leo scale only: `--leo-spacing-s` 4, `-m` 8, `-l` 12, `-xl` 16, `-2xl` 24.
- Each column has one left text edge. Icons, titles, meta and row content line up on it.
- Icons sit centred on the text line: a 16px icon on a 20–22px line, a 14px icon beside meta text.
- Hit targets are at least 28px (`--hit`), even when the glyph is 14px.

**Typography**
- A screen uses at most four sizes. Use the `--type-*` roles, not size-only tokens. The page title
  appears only on the welcome greeting, a bot's first-run page and settings pages.
- Ink has three levels (primary, secondary, tertiary) plus disabled. Text does not use ad-hoc
  opacity.
- Every count, time, token figure, line number and diff stat uses `font-variant-numeric:
  tabular-nums`, so streaming numbers do not shift.
- `text-wrap: balance` on headings, `pretty` on body paragraphs. Sentence case everywhere.
- Activity (tool calls, runs, meta lines) is set in meta and caption type with secondary or
  tertiary ink, below the conversation's own text.

**States**
- Every interactive element has hover, pressed, focus-visible and disabled states. Hover feedback
  lands within `--motion-fast`. Nothing animates on first paint.
- Every list and panel has designed empty, loading and error states. Loading shows a skeleton or a
  12px ring only after 150ms (`useDelayedFlag`), with no blank flash.
- Switching sessions shows the target's skeleton immediately and restores scroll, with no flash of
  the previous session.
- An inactive window dims its chrome through `data-window-inactive`: selected rows go grey and
  accent ink falls back to secondary.

**No layout shift**
- Hover action rows overlay the row and do not push content.
- Labels that change ("Copy" to "Copied", Send to Stop, "Show" to "Hide") keep a fixed width.
- Rows that appear and disappear, such as the working row, reserve their height.
- A streaming unclosed code fence renders as a code card from its first line.

**Truncation**
- Text in a fixed-width row ends in an ellipsis and shows the full text in a tooltip.
- File paths middle-truncate and keep the filename visible (`middleTruncate`).

**Native macOS feel**
- Chrome has the default cursor and `user-select: none`. The pointer is for links and for clickable
  rows (sessions, tree rows, connector and bot-history rows), listed once in `base.css`; a new
  clickable row is added to that list. Menu items keep the arrow. Transcript text and the readable
  parts of dialogs stay selectable.
- Scrollbars are thin, overlay and token-coloured. `::selection` uses a Leo token.
- Tooltips wait 1s the first time, then show instantly while the pointer moves between controls.
  There is one tooltip layer, driven by `data-tooltip`; a native `title` is not used.

**Motion**
- Hover and press use `--motion-fast`. Panels, menus and cards appearing use a short fade plus a
  small rise. Folds use `--motion-panel`, and `FOLD_MS` in `columns.ts` stays in sync with it.
- Everything respects `prefers-reduced-motion`: motion tokens go to zero and shimmer becomes static.

**Microcopy**
- Menu items that open a dialog end in "…". Buttons use verbs. Tooltips have no trailing period.
- An error says what happened and what to do, in one sentence.

**Accessibility**
- One focus-visible ring everywhere; on a 1px element such as a divider the line itself shows.
- Status is announced through the existing `aria-live` regions. Caption ink on sunken surfaces is
  checked for contrast. Dialogs keep `aria-labelledby`.

## Patterns to reuse

Compose from these before inventing a new pattern.

- **Decision card** (`cards.css`): a head with a kind icon and a short title saying what is being
  asked and about what, stat pills, the trust Label, a sunken evidence body, a collapsed "Details",
  right-aligned answers with the primary last, and a collapsed decided state.
- **Notice**: a caption row with a 14px icon, one sentence and at most one inline link. Notes about a
  session (fork, remembered trust, vetting) sit at the top of the transcript and are not entries,
  so they are never exported.
- **Tray**: what is said about sending docks onto the top edge of the composer at the composer's
  width (backend not set up, the queue). What needs attention floats above it as a pill.
- **Toast**: bottom-right of the conversation card. Confirmations clear themselves after four
  seconds; errors stay until dismissed and use `role="alert"`.
- **Inspector**: one head, one row height and one empty state for every list in it.
- **List rows**: `--session-row-h` rows in the sidebar, three lines of text, a hover control that
  overlays the time, sticky caption-ink group headings.
- **Menus**: Leo menu items with a leading icon column and a check column (`menus.css`).
- **Modal**: `Modal` with a `size`, the standard head and the footer actions described above.

## Methodology

1. **Evaluate before building.** For a new or changed element, decide whether it should exist,
   merge into something, or move. Prefer removing chrome to styling it. Record the decision in the
   PR text.
2. **Compose from what exists.** Use Nala components, `IconButton`, `Modal` and the patterns above.
3. **Use tokens.** Add a semantic token in `tokens.css` for a missing value instead of inlining it.
4. **Design the edge cases.** Empty, loading, error, very long titles, 12-level-deep paths,
   400-line diffs, 3,000-line code blocks, 500-entry transcripts, 0, 80 or 1,000 sessions, a very
   long model name, 5 attachments, 4 queued messages, the minimum window size, forced colours.
5. **Protect the render path.** Typing in the composer re-renders only the composer. Keep
   `EntryList` and `Row` memoised, pass callbacks from `App.tsx` through `useCallback`, and keep
   per-keystroke state (the draft) out of shared parents. Do not add a subscription that makes the
   transcript re-render on input.
6. **Look at it.** Visual quality cannot be fully specified. Capture the affected states with
   `scripts/drive-visual.mjs` in light and dark and compare them with the surrounding surfaces. For
   a new surface, or a change to the shell, transcript or dialogs as a whole, show the screenshots
   and get sign-off before building on top of it.
7. **Review the diff against this file** before saying the work is done, surface by surface.

## Performance budgets

Measured by `scripts/drive-perf.mjs` on a 500-entry transcript and a 1,000-session list. A change
does not regress them.
- Keystroke to paint in the composer is under 16ms.
- Transcript scroll holds 60fps, and streaming a long answer drops no frames.
- Opening the model menu, session menu or find bar takes under 100ms.
- With 1,000 stored sessions the window paints its first session row in under 1s and runs no task
  over 50ms. A list that grows with history draws a page at a time, and a popup that only one row
  can have open is mounted when it opens, not on every row.
- Syntax highlighting runs on closed fences only while streaming, and not per token. It never
  changes the text content (`drive:markdown` checks this).

## Gates

The current commands are in `ui/package.json` and `ui/docs/testing.md`, which win over this list if
they differ. From `ui/`:

- `npm run typecheck` (includes `check-nala`) and `npm run build`. Warnings count.
- `node --test scripts/*.test.mjs`, including `marking.test.mjs` and `ux-state.test.mjs`.
- The non-paid drivers that touch what you changed, for example `drive`, `drive:columns`,
  `drive:panels`, `drive:models`, `drive:menu`, `drive:export`, `drive:fork`, `drive:tree`,
  `drive:theme`, `drive:bots`, `drive:about`, `drive:vetting`, `drive:remembered-trust`,
  `drive:markdown`, `drive:perf`, `drive:session-list`, `drive:visual`, and
  `drive-ux-acceptance.mjs` and `drive-conversation-workflow.mjs`. If you moved a label or a DOM
  location, update the drivers and demo scenes that assert on it.
- Docs: update `ui/docs/interface.md` (layout, toolbar, keyboard model), `ui/docs/development.md`
  (tokens, components, check rules) and `ui/docs/testing.md` (drivers, fixtures, budgets) when the
  behaviour they describe changes. Update this file when the application's look changes.

Behavioural changes also follow `agents/skills/testing-preflight/SKILL.md`.

## Done means

Report, per surface, which rules above you checked and how (driver, screenshot or reasoning), which
gates ran and their results, and anything not verified, such as a state you could not capture or a
driver you could not run. Do not claim a visual result you did not see.
