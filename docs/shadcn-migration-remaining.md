# shadcn Migration — Remaining Steps

Working checklist for finishing the migration described in
[`shadcn-migration-plan.md`](./shadcn-migration-plan.md). That document is the
authority on *what* the end state is. This one is the authority on *what is left
to do*, in what order, and what must not break while doing it.

Branch: `ui-styling`. One commit per batch. A batch is not done until its gates
are green.

---

## 1. Current state

### Completed

| Batch | Commit | Scope |
| --- | --- | --- |
| 0 | `7f1f0b1` | Foundation: all theme tokens consolidated into `shadcn.css` |
| — | `05d61fc` | Sidebar widened to 25rem; session cards hover-reveal their actions |
| 1 | `ca9ed0f` | Sidebar tabs render from shadcn `Tabs` defaults |
| 2 | `d4c2207` | Sessions column migrated to shadcn defaults |
| 3 | `f577cf5` | Bots column migrated to shadcn defaults |

Batch 3 deliberately left the bots **form, memory and overview** screens on their
custom CSS, plus avatar rendering, for a later batch. Those are Batch 5 work.

### CSS still on disk

| File | Lines | Role |
| --- | --- | --- |
| `ui/src/renderer/modern.css` | 2637 | Legacy paint layer, scheduled for deletion surface-by-surface. Header comment says so. |
| `ui/src/renderer/styles.css` | 2028 | Legacy base. Carries app layout, document typography, security markings — and a lot of retired paint. |
| `ui/src/renderer/shadcn.css` | 351 | Tailwind v4 tokens, `@theme inline` aliases. The target. |
| `ui/src/renderer/export.css` | 334 | PDF typography. Stays custom, permanently. |

`modern.css` is the layer that overrides shadcn defaults. Most of the work from
here is deleting it, not writing new CSS.

---

## 2. Invariants — these hold in every batch

### Gates (all must pass, run from `ui/`)

```sh
npm run typecheck
npm run build
node --test scripts/*.test.mjs
node scripts/drive-theme.mjs
node scripts/drive-manual-walkthrough.mjs
```

Extras, run when time allows:

```sh
node scripts/drive.mjs
node scripts/drive-bots.mjs
```

`tsc -b` is **not** a gate for this migration.

### Drive-script hooks stay in the markup

Bespoke class names are load-bearing for the Playwright drivers even after their
paint is gone. Keep them as inert markup hooks; never delete one because "the
CSS is dead now".

Hooks on the transcript surface: `.transcript`, `.transcript-head`, `.where`,
`.entries`, `.entry-hit`, `.bubble`, `.bubble.user`, `.bubble.assistant`,
`.tool`, `.verb`, `.replayed`, `.tool-run-head`, `.working`, `.narration`,
`.attached`, `.consolidation`, `.watch-turn`, `.ask-question`, `.choices`,
`.choice`, `.choice.picked`, `.typed`, `.asked-answer`, `.confirm`,
`.confirm.ask`, `.confirm.untrusted`, `.confirm.run`, `.confirm.run.releases`,
`.confirm.output`, `.confirm.vouch`, `.vetted-read`, `.reject`, `.approve`,
`.decided`, `.unanswered`, `.composer`, `.send`, `.attach-files`, `.model-trigger`,
`.model-current`, `.export-open`, `.export-split`, `.queued-messages`,
`.attachment-chips`, `.attention-bar`, `.context-status`, `.backend-status`,
`.turn-notices`, `.turn-footer`, `.turn-audit-link`, `.error-card`,
`.fork-here`, `.fork-banner`, `.count`, `.confined`, `.quarantine`,
`.quarantine-head`, `.origin`, `.mark`, `.label`, `.preview`, `.processor-remark`,
`.credential-finding`, `.permission-scope`, `.interrupted-request`,
`.vetting-notice`.

Other surfaces keep their own: `.session`, `.session-row`, `.bot`,
`.tree-row`, `.popitem`, `.theme-row`, `.panel-head`, `.popover`, etc.

### Byte-exact markup (`ui/scripts/marking.test.mjs`)

These are asserted as exact substrings against server-rendered HTML. Adding a
Tailwind class to any of these elements **breaks the test**, because the needle
includes the closing quote:

- `class="quarantine-head"` — exactly one occurrence
- `class="quarantine-foot"` — exactly one occurrence
- `<pre class="preview">` — the `pre` carries no other class
- `<span class="mark">confined</span>`

Tolerated but still keep them intact:

- `class="origin"` — the regex allows extra attributes; the `title` attribute is
  what defeats the `FORGED_CHROME` assertion. Do not drop it.
- `confirm untrusted` must stay adjacent. `ApprovalCard` renders
  `class="my-3 gap-0 py-0 confirm untrusted"`; the prefix is fine, a class
  inserted *between* `confirm` and `untrusted` is not.
- `processor-remark` — one occurrence.
- `<strong>…untrusted</strong>` inside the processor remark.

### No inline styles

`FETCHING` asserts no `style=` attribute appears in drawn transcript or error
markup. Layout goes in Tailwind utilities or in the keep-list CSS. Never
`style={{...}}` in `Transcript.tsx` or `ErrorCard.tsx`.

### The keep-list ("What Stays Custom")

Only these survive to the end state:

1. **Application layout** — `.app` grid, `.transcript` flex column, `.entries`
   scroller, `.composer` footer, `.drag` / `.fold-toggle` (`-webkit-app-region`
   cannot be expressed in Tailwind), density (`.app.compact`).
2. **Document typography** — the `.bubble.assistant` markdown block, and
   `Markdown.tsx`'s `md-image`, `md-dead-link`, `local-file-link`.
3. **Avatar rendering** — `BotAvatar` and its stage/figure.
4. **Security markings** — quarantine, `confined`, `mark`, `origin`, `preview`,
   `confirm.untrusted` / `.output` / `.vouch` / `.run.releases`,
   `interrupted-request`, `vetting-notice`, `processor-remark`,
   `credential-finding`, `permission-scope`, `stages`, `warn`.
5. **Numbered diff and provenance markings** — `Diff.tsx`, `.line-number`,
   `.sign`, `.diff-review`.
6. **PDF typography** — `export.css`, untouched.

Everything else retires.

### Cascade gotcha

In the compiled stylesheet `.bg-transparent` sorts *after* `.bg-accent`, so a
`bg-accent` added alongside a ghost/outline variant silently loses. Use the
group variant instead:

```tsx
className="... group-[.bot-open]:bg-accent"
```

This is why the bots' current-row accent needed verifying by computed-style
probe. Any new "selected"/"open"/"active" ground in a later batch needs the same
treatment.

---

## 3. Component migration status

The plan's **Migration Scope** table and steps 2–3 of **Implementation Order** are
about replacing custom UI with shadcn compositions, not merely restyling it. That
work is further along than the rest of this document implies, so it is worth
stating plainly what is left of it.

### 3.1 The invariant: no raw controls

Verified across every file in `ui/src/renderer/components/*.tsx`:

**Zero** raw `<button>`, `<input>`, `<select>`, `<textarea>`, `<details>` or
`<summary>` elements remain. Every interactive control is a shadcn primitive.
Guard it, so the migration cannot silently slide backwards:

```sh
grep -rnE '<(button|input|select|textarea|details|summary)([ >])' src/renderer/components/*.tsx
# must print nothing
```

Widen the glob if components are ever added in a subdirectory. `<svg>` is not a
control and is exempt — `ForkIcon` draws one.

### 3.2 Components with no shadcn primitive

Five files import nothing from `components/ui`. Three are legitimate keep-list
exceptions; two are not, and are real work.

| Component | What it actually is | Verdict |
| --- | --- | --- |
| `ExportView.tsx` | The PDF export preview | **Keep.** PDF typography is keep-list; `export.css` is its stylesheet. |
| `BotAvatar.tsx` | Seeded figure renderer | **Keep.** Avatar rendering is keep-list. |
| `ForkIcon.tsx` | A hand-drawn SVG mark | **Keep.** An icon, not UI. The project has no icon set and shadcn has no icon primitive. |
| `FileGlyph.tsx` | Extension badge, `<span className="tree-glyph …">` | **Migrate → `Badge`.** Small: wrap the existing `glyphOf()` output in `Badge variant="outline"` and keep the family→colour mapping as utilities. Belongs with `FileTree` in §6.2. |
| `Gutter.tsx` | 1px `role="separator"` resize track | **Open question — do not just convert.** See below. |

**`Gutter` is the one genuinely open component decision.** The scope table names
`Resizable` for resizable columns, and the plan's intent is to use it. But this
gutter is deliberately *not* a handle: its own comment says the divider "is the
border between the panels rather than something drawn beside one," and it is a
1px grid track with a transparent pointer pad. `Resizable`'s handle is a visible,
focusable element with its own width. Swapping it in changes the resting
appearance of the whole window, and `drive-columns.mjs` / `drive-resize.mjs`
measure it.

Decide it explicitly in §6.9 rather than discovering it during a CSS sweep.
Either (a) keep the seam and record it as an application-layout keep-list
exception, or (b) adopt `Resizable` and re-tune the drivers that measure the
gutter. What must not happen is leaving it as an unexamined third category.

### 3.3 Remaining composition work

Control-level migration is done. What remains is *composition* — custom
arrangements built out of primitives. All of it is Batch 5 work that needs a
structural change, not just a restyle.

| Component | Primitives | Remaining custom composition | Step |
| --- | --- | --- | --- |
| `Diff.tsx` | 2 | `code-toolbar`, `diff-review` chrome | §6.1 — numbered-diff rendering itself stays |
| `Markdown.tsx` | 5 | `code-block`, `code-toolbar` | §6.1 — `md-*` typography stays |
| `TrustPrompt.tsx` | 1 | `AlertDialog` with no Header/Title/Description/Footer | §6.5 |
| `Notice.tsx` | 3 | `notice-actions` | §6.5 |
| `About.tsx` | 4 | custom hero composition | §6.8 |
| `Permissions.tsx` | 6 | `permission-row` → `Item` | §6.5 |
| `Context.tsx` | 7 | `panel` / `panel-inner` arrangement | §6.2 |
| `FileTree.tsx` | 8 | row shape | §6.2 |
| `AuditInspector.tsx` | 8 | event cards | §6.3 |
| `AgentSettings.tsx` | 12 | already `TabsList variant="line"` + `Field` / `Select` | §6.6 — mostly CSS |

`PopMenu.tsx` is **not** a "wrapper that merely preserves the old markup" (plan
step 2). It is backed by `DropdownMenu` and carries real anchoring, open-state and
ref behaviour. Keep it.

### 3.4 Net position

Component migration is complete at the control layer and substantially complete
at the composition layer. The remaining bulk of this migration is CSS
retirement, which is why §4 onward is weighted that way. Two items carry real
component work: `FileGlyph` → `Badge`, and the open `Gutter` decision.

---

## 4. Batch 4 — Transcript surface

Everything in the middle column. The components are already on shadcn primitives
(`Bubble`, `Message`, `Marker`, `Collapsible`, `Card`, `Alert`, `InputGroup`,
`Attachment`, `ToggleGroup`, `FieldSet`, `Button`); what is left is stripping the
legacy paint off their classNames and deleting the CSS that painted it.

### 5.1 `ui/src/renderer/components/Transcript.tsx`

Convert className-by-className, keeping the hook class and adding only the
utilities the component default does not already provide.

**Header and toolbar**

- `transcript-head` — keep CSS (app layout: position, padding, border-bottom,
  and the `.app.left-folded` traffic-light inset). Retire the `modern.css`
  duplicate.
- `head-row`, `head-titles` — structural flex; either keep in CSS or move to
  `flex items-center gap-2.5` / `min-w-0 flex-1`. Batch 3 kept structural CSS
  in `styles.css` and moved paint to utilities; follow that.
- `drag`, `fold-toggle`, `fold-chevron` — **keep CSS in full.** `-webkit-app-region`
  is not expressible in Tailwind.
- `conversation-toolbar`, `conversation-search` — layout flex/gap/padding to
  utilities; the `> button` paint block retires (shadcn `Button`/`Toggle` carry it).
- `backend-status`, `context-status`, `attention-bar` — box model to utilities.
  `backend-status` keeps its warning ground as a utility (`bg-warn/10`).
- `empty-state` / `empty-body` / `welcome-mark` / `welcome-recents` /
  `welcome-hint` — Empty component defaults plus utilities. Note the two
  `styles.css` override blocks at 1881–1882 that restate `h1` at 28px; decide
  whether the welcome headline keeps that scale or takes the `EmptyTitle` default.
- `primary` — the send/new-project button. shadcn `Button` default is already
  primary; the `.primary` hover/active paint retires.

**Entries and rows**

- `entries` — keep CSS (scroller + the `padding-inline: max(20px, calc((100% - 840px)/2))`
  measure and `.app.compact` density). This is document layout.
- `bubble` / `.user` / `.assistant` — user bubble takes the shadcn `Bubble`
  default ground; `.assistant` markdown block stays (typography).
- `narration`, `attached`, `consolidation`, `watch-turn`, `tool`, `tool-run`,
  `tool-run-head`, `working`, `working-bot`, `count`, `cancel`, `turn-audit-link`
  — paint to utilities, classes stay as hooks.
- `entry-hit` — keep CSS. It is `display: contents`; an element with no box
  cannot be scrolled to, which the reading-anchor effect depends on.

**Composer**

- `composer` — keep CSS (flex footer, top border, canvas ground) plus the
  `padding-inline` measure at line 1937.
- `composer-box` — the only place with a bespoke rounded control. InputGroup
  already has border, radius, shadow and focus ring; verify the ring reads
  correctly, then retire the rule rather than re-expressing it.
- `composer-toolbar`, `composer-hint`, `send`, `stop`, `attach-files` — paint to
  utilities. Keep `send` and `model-trigger` (driven).
- `queued-messages`, `attachment-chips`, `attachment-context` — `Card` /
  `AttachmentGroup` / `Alert` defaults plus utilities.

**Approvals, quarantine, questions**

These are the security surface — the `keep-list` applies, so most of this CSS
**stays**, but should be consolidated into one place in `styles.css` with the
rest of the security markings:

`quarantine`, `quarantine-head`, `mark`, `origin`, `label`, `preview`,
`quarantine-foot`, `confirm`, `confirm-head`, `intent`, `path`, `counts`, `warn`,
`permission-scope`, `stages`, `argv`, `resolved`, `ask-question`, `header`,
`question`, `any`, `given`, `choices`, `choice`, `picked`, `typed`,
`asked-answer`, `confirm-actions`, `trust-actions`, `approve`, `reject`,
`decided`, `interrupted-request`, `processor-remark`, `credential-finding`,
`vetting-notice`, `confined`.

The component wrappers (`ApprovalCard`, `ApprovalHead`, `ApprovalActions`) are
already shadcn `Card` parts. Leave their `cn()` merges alone — the
`confirm untrusted` adjacency depends on them.

**Export menu** — `export-split`, `export-open`, `export-chevron` paint retires;
keep the class names.

### 5.2 `TurnDetails.tsx`, `ErrorCard.tsx`, `ModelPicker.tsx`

- `TurnDetails` — `turn-notices`, `turn-footer`, `turn-statistics-body`,
  `turn-audit-link` (incl. `.has-refusal`) to utilities. Keep hooks.
- `ErrorCard` — `error-card`, `error-actions`, `error-details` to utilities on
  the `Alert` + `Collapsible`. No inline styles.
- `ModelPicker` — the whole popover: `model-picker`, `model-trigger`,
  `model-current`, `model-popover`, `model-heading`, `model-refresh`, `model-search`,
  `model-options`, `model-option`, `model-check`, `model-description`, `model-name`,
  `model-detail`, `model-capabilities`, `model-capability`, `model-default`,
  `model-status`, `model-footnote` → `Popover` + `Command` defaults plus
  utilities. `model-trigger` must stay: `drive-models.mjs` clicks it, and
  `Transcript.tsx`'s own `onChooseModel` does
  `document.querySelector('.composer .model-trigger')?.click()`.

### 5.3 CSS retirement for Batch 4

Delete `modern.css` **367–1342**, five sections:

| Lines | Section |
| --- | --- |
| 367–511 | Transcript header, toolbar, search, and banners |
| 511–686 | Transcript entries and markdown |
| 686–820 | Tool runs, turn metadata, and working state |
| 820–1100 | Composer, queue, attachments, and model picker |
| 1100–1343 | Quarantine, approvals, evidence, and decisions |

Keep **1343+** (`Code, diffs, and file previews`) — it is shared with the context
column and belongs to Batch 5. This is why `Diff.tsx` and `FilePreview.tsx` are
*not* in Batch 4.

Trim the matching `styles.css` blocks, which are scattered across three regions:

- **200–907** — the main transcript block. Keep: app layout (`.transcript`,
  `.transcript-head`, `.drag`, `.head-row`, `.head-titles`, `.fold-*`,
  `.entries`, `.composer`, `.entry-hit`), markdown typography (444–526), and the
  security markings (576–648, 650–758, 773–804). Retire the rest.
- **1860–1939** — overlay rules for `backend-status`, `attach-files`,
  `attachment-chips`, empty-state, `composer-toolbar`, `composer-hint`,
  `turn-notices`, `turn-footer`, `turn-audit-link`, `working`,
  `interrupted-request`, and the three padding-measure rules (1936–1938). Fold
  the survivors into the main block so the transcript CSS lives in one place.
- **2015–2027** — `vetting-notice`, `processor-remark`, `credential-finding`,
  `context-status`, `watch-turn`. Consolidate with the security block.

### 5.4 At-risk on deletion

A selector-by-selector comparison found 32 rules in `modern.css` 367–1342 that
have **no** counterpart in `styles.css`. Deleting the section drops them. Each
needs a destination:

**Convert to utilities in the JSX**

`.conversation-toolbar .export-open` · `.conversation-toolbar > button[aria-expanded]` ·
`.conversation-search button:hover:not(:disabled)` · `.tool.running` ·
`.working .cancel:hover` · `.attention-bar button:hover` · `.composer-box` (+ `:hover`,
`:focus-within`) · `.app .composer textarea:hover` / `:focus` ·
`.model-trigger:hover:not(:disabled)` · `.model-trigger[aria-expanded='true']` ·
`.model-popover[data-slot='popover-content']` · `.model-option[aria-selected='true']` ·
`.primary:hover:not(:disabled)` / `:active` · `.attach-files:disabled` ·
`.composer-toolbar .stop:hover` · `.queued-messages > strong` ·
`.queued-messages button:hover` · `.attachment-chips > span:hover` ·
`.attachment-chips button + button` · `.trust-actions button:hover` ·
`.trust-actions .approve:hover`

**Move into the `styles.css` security block (keep the rule)**

`.confirm .preview` · `.confirm.vouch` · `.permission-scope:first-of-type` ·
`.vetting-notice.safe` · `.processor-remark`

**Misfiled — belongs to Batch 5, do not drop**

`.audit-inspector summary` and `.audit-inspector summary:hover` sit inside the
Batch 4 range but style the audit inspector. Re-home them with the rest of the
audit CSS, or leave them until Batch 5 touches that file.

---

## 5. Batch 5 — Remaining surfaces

Grouped by the `modern.css` section that paints them. The class inventory below
is the current legacy-class list per component, so each step names real selectors.

### 6.1 Code, diffs and file previews — `modern.css` 1343–1446

`Diff.tsx`: `diff`, `diff-review`, `line`, `line-number`, `sign`, `text`,
`expanded-diff`, `code-toolbar`
`FilePreview.tsx`: `file-preview`, `preview-boundary`, `preview-actions`, `code-toolbar`
`Markdown.tsx`: `code-block`, `code-toolbar` (`md-image`, `md-dead-link`,
`local-file-link`, `md-table-wrap` **stay custom**)

Numbered-diff markings stay by decision. Everything else — the surrounding
chrome, toolbars, boundary labels, disclosure controls — moves to `Table`,
`Collapsible` and `Button` defaults plus utilities. This section is the last
blocker for deleting `modern.css`.

### 6.2 Context inspector, panels and file tree — `modern.css` 1446–1641

`Context.tsx`: `context`, `context-head`, `context-content`, `context-overview`,
`context-link`, `panel`, `panel-head`, `panel-inner`, `panel-icon`, `inspector-tabs`,
`inspector-title`, `chevron`, `count`, `tag`, `todos`, `from-record`, `files`,
`files-panel`, `confined-list`, `inactive`, `none`, `drawer-close`, `origin`
`FileTree.tsx`: `tree`, `tree-body`, `tree-row`, `tree-root`, `tree-folder`,
`tree-name`, `tree-note`, `tree-problem`, `tree-glyph`, `tree-more`,
`tree-tool`, `tree-tools`, `tree-search`, `tree-find`, `tree-search-results`,
`dotfiles`, `fold`, `fold-clip`, `chevron`, `none`

Hierarchy and loading logic stay (keep-list); the paint does not. Use
`Accordion`/`Collapsible`/`Item` per the scope table.

Also in this step: **`FileGlyph.tsx` → `Badge`** (§3.2). Wrap the existing
`glyphOf()` output in `Badge variant="outline"`, keep the family→colour mapping
as utilities, keep the `tree-glyph` hook and the `aria-hidden`.

### 6.3 Audit inspector — `modern.css` 1641–1694

`AuditInspector.tsx`: `audit-inspector`, `audit-event`, `audit-refusal`,
`audit-all`, `audit-labels`, `audit-status`, `audit-sequence`, `audit-retention`,
`audit-close`, `inspector-title`, `chevron`
Plus the two `summary` rules re-homed from Batch 4 (§5.4).

### 6.4 Menus — `modern.css` 1694–1740

`PopMenu.tsx`: `popmenu`, `popitem`, `popitem-label`, `popitem-detail`
`ForkIcon.tsx`: `fork-icon`

`DropdownMenu` per the scope table. `popitem` is driven — keep the class.

### 6.5 Modals, trust, notices, themes and permissions — `modern.css` 1740–1867

`TrustPrompt.tsx`: `modal`, `trust`, `path`, `trust-actions`, `approve`, `decline`, `aside`
`Permissions.tsx`: `permission-row`, `bot-note`
`ThemePicker.tsx`: `theme-picker`, `theme-list`, `theme-row`, `theme-name`,
`theme-swatches`, `theme-current`, `theme-hint`, `theme-aside`, `theme-actions`
`Notice.tsx`: `notice`, `notice-body`, `notice-actions`, `approve`

`AlertDialog` replaces the trust confirmation and every `window.confirm()`.
Theme picker keeps its reversible-preview contract — that behaviour is
keep-list, its chrome is not.

### 6.6 Settings and watches — `modern.css` 1867–2014

`AgentSettings.tsx`: `agent-settings`, `settings-heading`, `settings-tabs`,
`settings-body`, `settings-error`, `settings-path`, `settings-actions`,
`hook-editor`, `hook-argument`, `setup-routes`
`Watches.tsx`: `watch-settings`, `watch-list`, `settings-heading`, `settings-actions`
`BotMemory.tsx`: `bot-memory`, `bot-memory-panel`, `memory-tabs`, `memory-history`,
`memory-actions`, `memory-readable`, `memory-reset`, `bot-note`

`FieldGroup`/`Field`/`Select` for forms; `Tabs` + `Collapsible` for memory views
and revision history. Validation moves to the validation components.

### 6.7 Bots form, memory and overview (deferred from Batch 3)

`Bots.tsx`: `bot-editor`, `bot-form`, `bot-form-avatar`, `bot-form-model`,
`bot-field`, `bot-actions`, `bot-save`, `bot-choose`, `bot-fixed`, `bot-spacer`,
`bot-archive`, `bot-archive-button`, `bot-archive-rows`, `bot-overview`,
`bot-overview-title`, `bot-overview-actions`, `bot-overview-summary`,
`bot-conversations`, `bot-history-empty`, `bot-history-search`,
`bot-history-unavailable`, `bot-avatar-refresh`, `session-group-head`,
`session-group-fold`, `session-group-name`, `sessions-head`, `session-list`, `new`, `plus`
`BotAvatar.tsx`: `bot-avatar`, `bot-avatar-frame`, `bot-avatar-status` (rendering itself stays)

### 6.8 Errors, empty state and unconfigured state — `modern.css` 2014–2139

App-level error view, unconfigured-backend state, and the background/about
surfaces (`About.tsx`: `about`, `about-hero`, `about-stage`, `about-mascot`,
`about-figure`, `about-version`, `about-links`, `about-details`, `about-copy`,
`about-footer`, `about-close` — `modern.css` 2139–2371). `Empty`, `Skeleton`,
`Spinner` per the scope table.

### 6.9 Layout and fallbacks — `modern.css` 2371–2637

`Resizable` panels and responsive `Sheet` for the narrow-window drawer; then
sweep the motion and `forced-colors` blocks for selectors whose components have
since been migrated. A stale selector in a `@media (forced-colors: active)` block
is a silent a11y regression, so this is a check, not a delete.

**Decide `Gutter` here** (§3.2), before the sweep rather than during it. It is the
only component in the app with no shadcn primitive and no keep-list entry, and
the scope table does name `Resizable` for resizable columns. Record the outcome
in §2's keep-list either way, so the end state has no unexamined third category.

---

## 6. Batch 6 — Final CSS retirement

- [ ] Delete `ui/src/renderer/modern.css` entirely.
- [ ] Sweep `styles.css` for any remaining rule whose paint is a component's job.
      Keep only the six keep-list categories, in clearly commented sections.
- [ ] Confirm `export.css` is self-contained — it must not depend on a rule that
      only existed in the shared legacy sheets. This is plan step 4's
      "separate PDF typography before removing shared legacy rules", and it is
      the step most likely to break the PDF check if left late.
- [ ] Grep for dangling references to the deleted classes and clean them up, while
      keeping every hook in §2.

---

## 7. Batch 7 — Verification repair

Plan step 5, still outstanding. The current suite pins old class strings and
some `<strong>`/`<summary>` tags, which is what forced the byte-exact carve-outs
in §2.

- [ ] Relax `marking.test.mjs` where the assertion is about class *strings* rather
      than security behaviour. Keep every assertion that a confined body cannot
      forge chrome, cannot render raw `<iframe>`/`<img>`, and cannot impersonate
      an origin — assert those on behaviour, not on `class="quarantine-head"`.
- [ ] Repair outdated fixtures, including the model driver's folder-picker stub,
      **without** weakening filesystem validation.
- [ ] Add coverage for keyboard navigation, nested overlays, long content, and
      narrow layouts — the plan's completion criteria, currently untested.

---

## 8. Risk register

| Risk | Mitigation |
| --- | --- |
| Byte-exact markup assertions break on an added class | §2 list; check every security-marking element against `marking.test.mjs` before committing |
| `bg-accent` silently loses to `bg-transparent` | Use the group variant; verify selected/open state by computed-style probe |
| Security marking loses its visual weight | The security blocks are keep-list — they are not "legacy paint" |
| Driver hook removed as dead markup | §2 list; the extra drives are the regression net |
| A `modern.css` rule had no `styles.css` twin | §5.4 — 32 named selectors, each with a destination |
| PDF layout breaks when shared CSS goes | Separate `export.css` in Batch 6, not earlier |
| `forced-colors` block goes stale | §6.9 is a check, not a delete |
| A raw control reappears and the migration slides back | §3.1 grep guard; run it in CI |
| `Gutter` gets converted to `Resizable` as a drive-by and changes the window's resting appearance | §3.2 — it is an explicit decision in §6.9, with `drive-columns.mjs` / `drive-resize.mjs` re-tuned if adopted |
| A "custom" file is excused as keep-list without a reason | §3.2 table gives each one a verdict; a sixth unexamined exception is the failure mode |
| Batch grows past what can be gated | One commit per batch; do not merge batches to save time |
