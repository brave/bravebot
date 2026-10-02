---
name: nala-ui-quality
description: The quality bar and methodology for any new or changed surface in the Bravebot desktop UI (ui/src/renderer). Use before designing, building, restyling or reviewing any component, layout, dialog, menu, card, icon, copy, or stylesheet in ui/, and again against the final diff. Covers the Nala-only rule, quiet neutral chrome, tokens, icons, states, layout stability, security markings, performance budgets, and the gates to run.
---

# Nala UI quality bar

Every new or changed UI surface in `ui/` passes through this bar before it is called done. It
distils the Nala UI redesign into
rules that outlive that plan. The target is Codex/Grok/Claude-level polish: quiet, precise, fast.

Keep the work proportional. A one-line copy fix needs the microcopy rules and the gates, not a
gallery review. A new surface needs all of it.

## The spirit

- **Quiet neutral chrome.** Surfaces are monochrome, dividers are hairlines, chrome is scarce.
  Leo blue appears only on primary actions, focus, selection accents and links. Status colour
  (success, warning, error, info) comes only from `--leo-color-systemfeedback-*`.
- **Earn every element.** Test each piece of UI against one question: does it help someone reading
  or steering the agent right now? If not, remove it, merge it, or move it to where it is needed.
  Persistent status text that restates state the UI already shows (working row, composer, sidebar
  dot) is noise. Only actionable states get a marker.
- **Put content where it belongs.** Headers carry identity and actions only. A banner about
  history, trust, or vetting belongs in the transcript; a blocker on sending belongs on the
  composer; an app-level problem is a toast. Never stack banners in a header.
- **Layout is negotiable, capability is not.** Restructure freely, but keep every capability,
  accessible name, security marking and `data-test` hook unless a deliberate evaluation removes it.
- **Do not add what the trust model forbids.** No approval accelerators (a keyboard shortcut that
  approves a decision card), no card that steals focus from the composer, no drag or paste attach
  (attachments need the native picker's grant). See `ui/docs/security.md` and
  `ui/docs/file-access-security.md`.

## Hard rules

1. **Nala only.** Import components only from `ui/src/renderer/nala.ts`. Use `--leo-*` tokens (or
   the semantic layer in `styles/tokens.css` that aliases them) for colour, type, spacing, radius,
   elevation and motion. No raw hex, no raw `box-shadow`, no odd pixel values.
   `node scripts/check-nala.mjs` (run by `npm run typecheck`) must pass. It only gets stricter:
   never loosen it or grow an allowlist or ratchet to make a change pass.
2. **Icons are Leo `Icon`s.** Three sizes only: 14 (meta), 16 (controls), 20 (empty states and
   dialog headers), one stroke family. No inline `<svg>` (except the `BotAvatar`/`FlatAvatar`
   artwork), and no unicode glyphs as icons (`↑ ↓ ✓ ▸ › ⋯`, CSS `content:` arrows). Keyboard hints
   like "⌘N" are typography. `name` props are typed `IconName` literals, never `string`. Check the
   name exists in `node_modules/@brave/leo/icons/*.svg`.
3. **Icon-only controls use `IconButton`** (`components/IconButton.tsx`): Leo `Button` + `Icon` +
   `Tooltip` showing label and shortcut. A tooltip supplements `aria-label`, never replaces it.
4. **Security markings stay unmistakable.** `.quarantine` keeps its hatched treatment,
   `.confirm.untrusted` stays visually distinct, trust Labels stay loud. Do not touch the class
   contracts in `scripts/marking.test.mjs` or weaken LAYER-5 trust marking. The marking must
   survive `@media (forced-colors: active)` as a system-colour border plus a text label, never
   colour or a background image alone.
5. **Keep the hooks.** Preserve `data-test` attributes, IDs and the structural classes tests query
   (`.session`, `.bubble.user`, `.confirm`, `.approve`, `.tool`, `.tool-run`, `.quarantine-head`,
   `.model-trigger`, `.decided`, and so on). Where a visible label or DOM location changes, update
   the drivers in `ui/scripts` in the same change. Button texts that tests or security docs pin
   stay word-for-word.
6. **No duplicate or dead CSS.** Styles live in the module for their surface under
   `src/renderer/styles/`. One rule per selector per module; merge, don't override with a second
   `.app ...` block. Delete rules that target things that no longer exist, including native
   `button` rules aimed at what are now Leo hosts.

## Quality bar for every surface

A surface is done only when all of this holds in light and dark, at
1440×900 and at the minimum window size.

**Grid and alignment**
- Spacing from the Leo scale only (4/8/12/16/24).
- One left text edge per column; icons, titles, meta and row content line up on it.
- Icons sit optically centred on the text line (16px icon on a 20–22px line, 14px with meta).
- Hit targets are at least 28px (32 comfortable), even when the glyph is 14px.

**Typography**
- At most four sizes on a screen: caption 12, body 14, title 16, and a heading on the welcome
  screen only. Use the `--type-*` role tokens, not size-only tokens.
- Three ink levels (primary, secondary, tertiary) plus disabled. No ad-hoc opacity for text.
- `font-variant-numeric: tabular-nums` on every count, time, token figure, line number and diff
  stat, so streaming numbers don't jitter.
- `text-wrap: balance` on headings, `pretty` on body paragraphs. Sentence case everywhere.

**States**
- Every interactive element has hover, pressed, focus-visible and disabled. Hover feedback lands
  within `--motion-fast`. Nothing animates on first paint.
- Every list and panel has designed empty, loading and error states. Loading shows a skeleton or
  12px ring only after 150ms (`useDelayedFlag`), never a blank flash.
- Session switching shows the target's skeleton immediately and restores scroll, with no flash of
  the previous session.

**No layout shift**
- Hover action rows overlay; they never push content.
- Labels that change ("Copy"→"Copied", Send→Stop, "Show"→"Hide") keep a fixed width.
- Rows that appear and disappear (the working row) reserve their height.
- A streaming unclosed code fence renders as a code card from its first line.

**Truncation**
- Text in a fixed-width row truncates with an ellipsis and shows the full text in a Tooltip.
- File paths middle-truncate and keep the filename visible (`middleTruncate`).

**Native macOS feel**
- Chrome: default cursor (pointer only for links) and `user-select: none`; transcript text and
  readable dialog bodies stay selectable.
- Chrome dims when the window is inactive (`data-window-inactive`); accent fills drop to neutral.
- Thin token-coloured overlay scrollbars. `::selection` uses a Leo token.
- Tooltips: 500ms first delay, then instant while moving between controls.

**Motion**
- Hover/press use `--motion-fast`; panels, menus and cards appearing use a short fade plus a small
  rise; folds use `--motion-panel` (keep `FOLD_MS` in `columns.ts` in sync).
- Everything respects `prefers-reduced-motion` (tokens zero out; shimmer becomes static).

**Microcopy**
- Menu items that open a dialog end in "…". Buttons use verbs. Tooltips have no trailing period.
- Errors say what happened and what to do, in one sentence.

**Accessibility**
- One consistent focus-visible ring. Status is announced through the existing `aria-live`
  regions. Check contrast of caption ink on sunken surfaces. Dialogs keep `aria-labelledby`.

## Methodology

1. **Evaluate before building.** For a new or changed element, state whether it should exist,
   merge, or move. Prefer removing chrome to styling it. Note the decision in the PR text.
2. **Compose from what exists.** Reach for Nala components, `IconButton`, `Modal` (with its `size`
   and standard header/subtitle/actions), the shared notice pattern (caption row, 14px icon, one
   sentence, inline link), and the shared decision-card anatomy (kind icon, short title, stat
   pills, trust Label, sunken preview, collapsed "Details", right-aligned actions, collapsed
   decided state) before inventing a new pattern.
3. **Tokens first.** If a value is missing, add a semantic token in `tokens.css` that aliases a
   Leo token, then use it. Don't inline the value.
4. **Design the edge cases.** Empty, loading, error, very long titles, 12-level-deep paths, 400-line
   diffs, 3,000-line code blocks, 500-entry transcripts, 0 or 80 sessions, a very long model name,
   5 attachments, 4 queued messages, the minimum window size, forced colours.
5. **Protect the render path.** Typing in the composer must re-render only the composer. Keep
   `EntryList` and `Row` memoised, pass callbacks from `App.tsx` through `useCallback`, and keep
   per-keystroke state (the draft) out of shared parents. Never add a subscription that makes the
   transcript re-render on input.
6. **Look at it.** Visual taste can't be fully specified. Capture the affected states with
   `scripts/drive-visual.mjs` in light and dark, and compare against the surrounding surfaces.
   For a new surface or a shell/transcript/dialog-wide change, show the screenshots and get
   sign-off before building on top of it.
7. **Review the diff against this file** before saying done, surface by surface.

## Performance budgets

Measured by `scripts/drive-perf.mjs` on a 500-entry fixture; a change must not regress them.
- Keystroke to paint in the composer under 16ms.
- Transcript scroll holds 60fps; streaming a long answer doesn't drop frames.
- Opening the model menu, session menu or find bar takes under 100ms.
- Syntax highlighting: only closed fences while streaming; no re-highlight per token. Highlighting
  must never change the text content (`drive:markdown` checks this).

## Gates

Find the current commands in `ui/package.json` and `ui/docs/testing.md`; those win over this
list if they differ. From `ui/`:

- `npm run typecheck` (includes `check-nala`) and `npm run build`. Warnings count.
- `node --test scripts/*.test.mjs`, including `marking.test.mjs` and `ux-state.test.mjs`.
- The non-paid drivers that touch what you changed (for example `drive`, `drive:columns`,
  `drive:panels`, `drive:models`, `drive:menu`, `drive:export`, `drive:fork`, `drive:tree`,
  `drive:theme`, `drive:bots`, `drive:about`, `drive:vetting`, `drive:remembered-trust`,
  `drive:markdown`, `drive:perf`, plus `drive-ux-acceptance.mjs`,
  `drive-conversation-workflow.mjs` and `drive-visual.mjs`). If you moved a label or a DOM
  location, update the drivers and demo scenes that assert on it.
- Docs: update `ui/docs/interface.md` (layout, toolbar, keyboard model), `ui/docs/development.md`
  (tokens, components, check rules) and `ui/docs/testing.md` (drivers, fixtures, budgets) when
  behaviour they describe changes.

Behavioural changes also follow `agents/skills/testing-preflight/SKILL.md`.

## Done means

Report, per surface: which rules above you checked and how (driver, screenshot, or reasoning),
which gates ran and their results, and anything not verified (for example a state you could not
capture, or a driver you could not run). Do not claim a visual result you did not see.
