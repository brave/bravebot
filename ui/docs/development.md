# Development and packaging

Start with [setup](setup.md). Run the `npm` commands from `ui/`; this is not an npm
workspace of the package at the repository root, so nothing there reaches these scripts.
Run the `cargo` commands from the root, where the workspace the two front-end crates are
members of lives.

The UI depends on [Brave's Leo (Nala)](https://github.com/brave/leo) design system as a
git dependency. Installing it runs Leo's `prepare` script, which needs `pnpm`. Enable it
once with `corepack enable` (ships with Node 18+); `npm ci` and `npm install` then work.

| Command | What it does |
| --- | --- |
| `npm ci` | Install locked npm dependencies and set up Electron |
| `npm run dev` | Set up Electron, build both Rust executables, start hot reload |
| `npm run bridge` | Build `bravebot-rpc` and `bravebot-ui-files`, loading credentials where configured |
| `npm run setup:electron` | Install the Electron runtime if needed and name the development app |
| `npm run name-dev-app` | Restore “Brave Bot” as the development app's menu-bar name |
| `npm run typecheck` | Run `tsc --noEmit` |
| `npm run build` | Build both Rust executables, typecheck, bundle into `out/` |
| `npm start` | Set up Electron and preview the existing bundle; does not rebuild it |
| `npm run package` | Build both Rust executables, bundle, package for macOS or Linux; does not typecheck |
| `make app-bundle` (from the root) | The same bundle, carrying release executables built with credentials required, fused |
| `make app-release` (from the root) | A disk image per Mac architecture, from the cross-built executables in `dist/`; see [releasing](../../docs/development/releasing.md#the-desktop-application) |
| `make app-release-linux` (from the root) | A `.deb` and an `.rpm` per Linux architecture, from the same; see [releasing](../../docs/development/releasing.md#the-linux-packages) |
| `make app-release-windows` (from the root) | A Windows installer per architecture, from the same, on Windows or a Mac; see [releasing](../../docs/development/releasing.md#the-windows-installers) |
| `make check-ui` (from the root) | Install, build the file helper, and run every `scripts/*.test.mjs` |
| `cargo test -p bravebot-ui-bridge -p bravebot-ui-files` | Test the two front-end crates |
| `cargo test --all` | Test the whole workspace, agent crates included |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint the whole workspace |
| `npm run drive` / `npm run drive:<name>` | Run a named Electron driver; see [testing](testing.md) |
| `npm run demo -- --record` | Record a walkthrough; see [demo costs and setup](demo.md) |

## Working with Leo (Nala) components

Renderer code imports Leo from `src/renderer/nala.ts`, never from `@brave/leo` directly.
Three things about Leo's React wrappers shape how the components are used:

- **`aria-*` props are dropped.** The wrappers hand only the Svelte component's own props to
  its custom element, so `aria-label`, `aria-expanded`, `aria-pressed` and friends never
  become attributes. `Button`, `Input`, `TextArea`, `Checkbox`, `RadioButton` and `Toggle` are
  therefore wrapped by `withShadowAttrs` (`nala-a11y.tsx`), which writes those attributes, and
  `title`, onto the host and onto the control drawn inside its shadow root, and puts them
  back when Leo rebuilds it. Write `<Button aria-label="Close">` as usual. It also flattens
  the `tabindex="1"` Leo puts on every field it draws.
- **`ButtonMenu` wraps its anchor in `role="button"`.** The wrapper (`withPlainMenuAnchor`)
  takes that element out of the accessibility tree and the tab order, so a `Button` in the
  anchor slot is the only button. Put `aria-haspopup` and `aria-expanded` on that `Button`.
- **Shadow boxes do not see the host's CSS.** A dialog's width, padding and radius come from
  `--leo-dialog-*` custom properties, a collapse's chrome from `--leo-collapse-*`, a field's
  from `--leo-control-*`, and a button's from `--leo-button-*`. A `width` or `border` on the
  host lands on an empty element. A disabled `<fieldset>` does not reach a Leo control either,
  so pass `disabled` / `isDisabled` to each one.

Labels go in the component's default slot (`<Input>Name</Input>`), which Leo renders as a real
`<label>` around the field, rather than in a wrapping `<label>`.

Some UI is deliberately not Leo, because it has no equivalent or because the swap would lose
behaviour: the trust-marked cards (`.quarantine`, `.confirm`), which tests assert as markup;
external links in rendered Markdown; the column splitter; the avatar; the file tree's rows;
the session and bot rows; `Fold`, kept for group headers, the archive and tree folders, whose
headers carry a second action or tree semantics that a `<summary>` cannot; and the ask
card's choices, which can be clicked again to clear, as a radio button cannot.

## Styling

### The token layer

`src/renderer/styles/tokens.css` is the semantic layer between Leo and the rest of the
stylesheets. Every name in it aliases a `--leo-*` token, and a rule elsewhere reaches for the role
that says what a value is *for* rather than for a Leo name:

| Family | Roles |
| --- | --- |
| Surfaces | `--surface-app` (the window ground), `-panel`, `-raised`, `-sunken` (code, evidence), `-hover`, `-selected`, `-scrim` |
| Borders | `--border-hairline`, `-subtle`, `-strong`, `-focus` |
| Ink | `--ink-primary`, `-secondary`, `-tertiary`, `-disabled`, plus `--ink-link` and `--ink-accent`; no ad-hoc opacity for text |
| Status | `--status-success`, `-warning`, `-error`, `-info` and their `-bg`; only ever from `--leo-color-systemfeedback-*` |
| Type | `--type-heading`, `-title`, `-body`, `-meta`, `-caption` (and `-strong` forms), `-code`, `-code-body`: font shorthands |
| Radius | `--radius-chip`, `-control`, `-card`, `-field`, `-pill` |
| Elevation | `--shadow-raised`, `--shadow-floating`, `--shadow-focus` |
| Layout | `--titlebar` (44px), `--lights`, `--card-gap`, `--row-h`, `--session-row-h`, `--reading-width`, `--hit` (28px), `--turn-gap`, `--part-gap` |
| Icon sizes | `--icon-meta` 14, `--icon-control` 16, `--icon-hero` 20 (and `--icon-caption` 12) |
| Motion | `--motion-fast` (hover and press), `--motion-panel` (folds; `FOLD_MS` in `columns.ts` must match `--leo-duration-m`) |

Light and dark come from Leo's own `prefers-color-scheme` and `data-theme` rules, so a role needs
no dark-mode override of its own; the syntax colours in `syntax.css` are the exception, and pick
the light or dark primitive by the same two conditions. `prefers-reduced-motion` zeroes the motion
tokens. The legacy names at the foot of
`tokens.css` (`--bg`, `--ink-dim`, `--accent` and the rest) alias the roles until the last rule
using them goes.

### Stylesheet modules

`src/renderer/styles.css` is only a list of `@import`s, in order, of the files under
`src/renderer/styles/`. These are plain CSS files, not CSS Modules: class names stay global,
because the drivers and `marking.test.mjs` query them (`.session`, `.bubble.user`, `.confirm`,
`.quarantine`). Each file owns one part of the window and says so in its header comment.

| File | Owns |
| --- | --- |
| `tokens.css` | the token layer above |
| `base.css` | cursor and selection, numerals, scrollbars, the inactive window, reduced motion, forced colours, and the one tooltip |
| `shell.css` | the grid, the inset card, the column heads and folding |
| `sidebar.css` | the left column: its titlebar, the session and bot lists, and the foot |
| `menus.css` | the inside of Leo menu items: leading icons and check columns |
| `transcript.css` | the conversation header, the find bar, the toasts and the reading column |
| `activity.css` | tool calls and runs, the working row and meta lines |
| `cards.css` | decision cards, confined content and error cards |
| `composer.css` | the message box and the trays docked to it |
| `inspector.css` | the context column and one turn's audit |
| `dialogs.css` | the dialog frame and each dialog's content |
| `markdown.css` | rendered replies, scoped to `.bubble.assistant` |
| `syntax.css` | the seven syntax colour roles |
| `legacy.css` | what has not yet been moved into a module; it shrinks, and nothing is added to it |

Put a new rule in the module whose part of the window it belongs to, and use a role from
`tokens.css`. Markdown rules must never reproduce the app's own trust signals (the hatched
confine border, the warn bar), or a reply could draw something the reader is meant to read as
chrome.

### IconButton

Every icon-only control uses `components/IconButton.tsx`, not a bare `Button`. It takes an `icon`
(a Leo icon name from `nala.ts`), a `label`, and a `kind` and `size`. The label is the accessible
name and is required. The visible name is a tooltip drawn by `TooltipLayer` from the
`data-tooltip` attribute, which `IconButton` sets from `tooltip` or, if that is absent, the label.
A `shortcut` such as `"⌘F"` is written into the tooltip and into `aria-keyshortcuts` (`Meta+F`),
so a shortcut is said once, in the platform's form. `pressed`, `expanded`, `controls` and
`hasPopup` set the matching ARIA state; `description` adds a spoken suffix for state the icon
alone carries.

`TooltipLayer` is the only tooltip in the window: one Leo `Tooltip` laid over whichever
`[data-tooltip]` element the pointer rests on or the keyboard reaches, with a 1s first delay
and no delay while moving along a toolbar. Use `data-tooltip` on anything else that needs one
rather than a native `title`, which would draw a second box. A tooltip supplements the accessible
name and never replaces it, and a disabled control shows none.

Hit targets are at least 28px (`--hit`), even when the glyph is 14px.

### Render isolation

The transcript can hold hundreds of entries, so typing must not re-render them. `draft` lives in
`App`, and this is what keeps a keystroke from reaching the list:

- `Composer` is `memo`ised and given only stable callbacks. `Transcript` wraps the handlers it
  passes down in `useEvent`, so their identity does not change when the draft does.
- `EntryList`, `ToolRun` and `Row` in `Transcript.tsx` are `memo`ised, and `EntryList` derives
  its runs from `entries` with `useMemo`.
- A callback added to one of these must be stable (`useEvent`, `useCallback`, or a module
  function). One that closes over the draft, or is rebuilt on every render, silently undoes the
  isolation, and nothing but the perf driver will notice; see [testing](testing.md#performance-budgets).

### The Nala checks

`npm run typecheck` runs `scripts/check-nala.mjs` before `tsc`. It reads every `.css` file under
`src/renderer/` and every `.ts` and `.tsx` file, and fails on:

- a hard-coded colour (hex or `rgb()`/`rgba()`) or a `px` font size, including in the `font`
  shorthand, in any stylesheet;
- a `box-shadow` that is not `none`, a single `var(--…)`, or a focus ring of the form
  `0 0 0 <n>px <var or transparent>` (optionally `inset`); use `--shadow-*` or `--leo-effect-*`;
- a `var(--leo-…)` in the colour, font, spacing, radius, effect, duration, easing, typography,
  gradient or elevation families that Leo does not define, which is a typo that silently paints
  nothing (checked in stylesheets and in TypeScript);
- the same selector declared twice in one stylesheet and at-rule context (`legacy.css` is exempt);
- a raw `<svg` in the renderer other than in `BotAvatar.tsx`, which is artwork, not an icon;
- a unicode glyph used as an icon (`↑ ↓ ✓ ▸ › ⋯ ↗`) in JSX text, or an arrow in CSS `content:`.
  The `GLYPH_ALLOW` list at the top of the script exempts prose arrows in a sentence, one entry
  each;
- more raw `px` spacing (`padding`, `margin`, `gap`, `inset`, `top`, `left`, `right`, `bottom`),
  or more size-only `--leo-typography-*-font-size` reads, than the two ratchets allow.

The ratchets (`PX_SPACING_MAX` and `TYPOGRAPHY_SIZE_MAX`) are the counts as of the last module
that landed. They may only go down: when you replace raw spacing with `--leo-spacing-*`, or a
size-only read with a `--type-*` role, lower the constant in the same change. The last line the
script prints shows both counts against their limits.

### The quality bar

A surface is finished when it holds in light and dark at 1440×900 and
at the minimum window size. In short:

- **Grid.** Spacing comes from the Leo scale (4/8/12/16/24). Each column has one left text edge.
  Icons come in three sizes, 14, 16 and 20, from one stroke family, and are centred on the text
  line.
- **Type.** No more than four sizes on a screen; three ink levels plus disabled;
  `font-variant-numeric: tabular-nums` on every count, time, token figure, line number and diff
  stat; `text-wrap: balance` on headings and `pretty` on paragraphs; sentence case.
- **States.** Hover, pressed, focus-visible and disabled on everything interactive, with one
  focus ring. Every list and panel has an empty, loading and error state.
- **No layout shift.** Hover actions overlay rather than push, labels that change (Copy to
  Copied, Send to Stop) keep their width, and the working row reserves its height.
- **Truncation.** Text in a fixed-width row ends in an ellipsis and carries the full text in a
  tooltip.
- **Native feel.** Default cursor on chrome, pointer only for links; chrome is not selectable and
  transcript text is; chrome dims with `data-window-inactive`; scrollbars are thin and overlay.
- **Motion.** Everything that animates respects `prefers-reduced-motion`.
- **Forced colours.** Under `@media (forced-colors: active)` the quarantine hatch, the untrusted
  card border and the trust labels stay visible through a system-colour border and their text, so
  a marking never depends on colour or a background image alone.
- **Copy.** Menu items that open a dialog end in "…", buttons use verbs, tooltips carry no
  trailing period, and button texts that tests or [security](security.md) pin stay word for word.
- **Performance.** The [budgets](testing.md#performance-budgets), on a 500-entry transcript and a
  1,000-session list.

The [visual gallery](testing.md#the-visual-gallery) is how this is checked by eye, and it fails
the run on any interactive target under 28px.

### Syntax colour

Fenced code in a reply and the lines of a proposed diff are coloured by the same grammars, from
`src/renderer/highlight.ts`. It uses [`lowlight`](https://github.com/wooorm/lowlight) (a
`highlight.js` grammar set that produces a syntax tree rather than HTML), and `Markdown.tsx` hands
the same `LANGUAGES`, `ALIASES` and `PLAIN_TEXT` to `rehype-highlight`, so the two never disagree
about what a keyword looks like. The tree is walked into React spans with `hljs-*` class names and
text children; nothing turns a string into markup, and highlighting must not change the text
content.

Only a subset of grammars is registered, because every registered grammar is bundled whether or
not anybody writes in it:

`bash`, `c`, `cpp`, `css`, `diff`, `go`, `ini`, `java`, `javascript`, `json`, `markdown`,
`python`, `rust`, `shell`, `sql`, `swift`, `typescript`, `xml`, `yaml`.

A fence is coloured only if it names one of these, or an alias: `ts` and `tsx` for
`typescript`, `js` and `jsx` for `javascript`, `toml` for `ini`, `jsonc` and `json5` for `json`,
`shell-script` for `bash`. Detection is off, so an untagged or unknown fence is left plain, and
`text`, `txt`, `plain`, `plaintext`, `output` and `log` are plain on purpose. A diff picks its
grammar from the file's extension through the `EXTENSIONS` table in the same file.

To add a language:

1. Import its grammar from `highlight.js/lib/languages/<name>` in `highlight.ts` and add it to
   `LANGUAGES`, keeping the list alphabetical.
2. Add any fence aliases to `ALIASES` (for example `ts` for `typescript`).
3. Add the file extensions people will meet it under to `EXTENSIONS`, so diffs of those files are
   coloured.
4. Look at it in the [visual gallery](testing.md#the-visual-gallery), whose conversation scene
   holds a `ts` fence; add a fence of the new language to that fixture's reply to see it in light
   and dark.

There is no per-language stylesheet. `syntax.css` defines seven roles (keyword, string, number,
comment, function, type, punctuation) in Leo primitives, the same for every language, so a new
grammar needs no colour work unless it emits an `hljs-*` class that no rule there covers.

TypeScript uses strict checking, including `noUncheckedIndexedAccess`,
`noUnusedLocals` and `noUnusedParameters`. There is no ESLint or Prettier gate.
See [testing](testing.md#what-ci-runs) for exactly what CI checks and what must run locally.

## Build and preview

```bash
npm run build
npm start
```

With a desktop display available, `npm run drive:smoke` checks that the window and
main UI are visible and the Rust bridge responds, then saves a screenshot under
`/tmp/bravebot-ui/`. To check a Linux package, run
`npm run drive:smoke -- "./dist/Brave Bot-linux-x64/Brave Bot"`.

`scripts/build-bridge.sh` builds the Rust bridge and secure-file helper. TypeScript
then checks the code and electron-vite bundles the main process, preload and React
renderer into `out/`. Development executables are in the workspace `../target/debug/`.

The bridge talks to the pinned agent library over a child-process protocol; it does
not drive a terminal. See the [protocol design](phase-0-rpc-protocol.md) and
[security boundaries](security.md).

## Packaging

```bash
npm run typecheck
npm run package
```

`npm run package` does not typecheck. It builds the Rust executables and Electron
bundles, then `scripts/package.mjs` uses `@electron/packager` to create
`dist/Brave Bot-darwin-<arch>/Brave Bot.app` on macOS or
`dist/Brave Bot-linux-<arch>/` on Linux. Launch the Linux package with
`"./dist/Brave Bot-linux-x64/Brave Bot"` (replace `x64` for other architectures).
The platform follows the Node process, and so does the architecture unless
`node scripts/package.mjs --arch=arm64` or `--arch=x64` names one, in Electron's names rather than
the cross-build's `amd64`. Rust uses its configured toolchain target; for a native bundle, use
matching Node and Rust architectures.

`--platform=win32` packages `dist/Brave Bot-win32-<arch>/` instead, and is the one platform that
does not have to be the host's: the icon and the version resource are all that distinguishes a
Windows bundle, and `@electron/packager` writes both with resedit rather than with a Windows tool.
Because the bundle is somebody else's platform, it takes `--executables=<dir>` with a pair built
for Windows in it; `make app-bundles-windows` is that command with the cross-build's pair staged
for it. Packaging reads each executable's header, as it does for a Mac release, and refuses one
that is not a Windows executable for the architecture asked for.

Which Rust build the bundle carries is the one thing the finished bundle does not record: it is
named, versioned and laid out identically either way, and the two overwrite each other in
`dist/`. `npm run package` carries the debug executables, which is what a checkout has already
built and what makes it the right command for testing the bundle itself. The last line it prints
says which it took:

```
packaged: dist/Brave Bot-linux-x64 (debug executables, not fused)
```

The bundle's version is `package.json`'s, and that is the repository's version rather than one
of the front end's own: the app ships an agent build, so the two are one release.
`make bump-version` at the root rewrites this manifest and its lockfile along with the workspace's,
and `make check-versions` fails a tree where they disagree. Neither is edited by hand.

Both `bravebot-rpc` and `bravebot-ui-files` are copied into the app's Resources.
Packaged builds use those copies; development builds use the workspace `../target/debug/`.

A bundle for anybody else is `make app-bundle`, from the repository root: it builds both
executables in release mode with the backend credentials required rather than optional, and
packages those instead. `make app-release` packages the release's own cross-built pair for each
Mac architecture instead, through `--executables=<dir>`, and packaging reads the header of each
executable in that pair and refuses one built for another platform or architecture than the
bundle's. Neither command signs or notarises the
result, so what comes out is installable and not distributable, and how a release gets the rest
is [releasing](../../docs/development/releasing.md#the-desktop-application). Git commit
signatures are separate from macOS app signing.

Those two are fused and `npm run package` is not. `scripts/fuses.mjs` turns off the Electron
fuses that let the binary run code that is not the app's: `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`
and the `--inspect` arguments. It turns on asar integrity validation and loading only from the
asar. A fused app cannot be driven, because Playwright attaches through `--inspect`, so
`npm run drive:packaged` and `SECURE_FILES_APP` take the unfused bundle `npm run package` writes.
All three write to the same `dist/` path, and `drive:packaged` refuses a fused bundle there rather
than wait out its launch timeout.

What `npm run package` writes on Linux is a directory somebody has to unpack and run themselves,
which is why it is not what a release ships. `scripts/linux-package.mjs` lays that bundle out as
an install under `/opt/brave-bot`, with a launcher entry, the icon in the theme at every size in
`build/icons/` and again as the drawing, and `chrome-sandbox` setuid root, and writes the `.deb`
and `.rpm` descriptions from that one tree; `make app-release-linux` at the root packs it with
`dpkg-deb` and `rpmbuild` in pinned containers. The setuid bit is the point of the exercise: where unprivileged user
namespaces are unavailable, Electron aborts at start without it, and only an installer can set
it. `scripts/linux-package.test.mjs` covers what the two formats are told, and builds a real
`.deb` where `dpkg-deb` is present and a real `.rpm` of each architecture where `rpmbuild` is.

A Windows bundle is also a directory, and `scripts/windows-installer.mjs` puts it in a per-user
NSIS installer with electron-builder, which is handed the bundle as `prepackaged` and so changes
nothing in it: the release signs the bundle before this step, and those signatures are what gets
installed. It refuses a bundle for the other architecture or one that is not fused, and refuses to
run on Linux, where electron-builder would need Wine. `scripts/windows-installer.test.mjs` covers
the refusals and the names the first release fixes, and builds a real installer from a stand-in
bundle on macOS and Windows, checking the bundle comes out byte for byte as it went in. On macOS
it also checks that each architecture's archive uses only coders the installer's own 7-Zip decodes,
since the 7-Zip electron-builder fetches for Windows cannot open an installer to look.
`scripts/check-windows-install.mjs` installs, starts, upgrades and uninstalls the real one, in the
account running it, so it is CI's on a runner of each architecture rather than a local check.

Every macOS bundle carries the bundle id `com.brave.bravebot` and the icon `build/icon.icns`. macOS
keys privacy grants and keychain items on the bundle id, so it does not change between releases.
A Windows bundle carries `build/icon.ico` and a version resource naming Brave as the company and
"Brave Bot" as the product, which is what its properties dialog and Task Manager show; without one
they both say Electron, because the resource would be the one Electron's own build left behind.
The icon is the flat robot avatar in Brave orange on a macOS tile, drawn in `build/icon.svg`; after
changing the drawing, remake all three sets of icons from it:

```bash
inkscape build/icon.svg -w 1024 -h 1024 -o /tmp/icon-1024.png
mkdir /tmp/icon.iconset
for s in 16 32 128 256 512; do
  sips -z $s $s /tmp/icon-1024.png --out /tmp/icon.iconset/icon_${s}x${s}.png
  sips -z $((s*2)) $((s*2)) /tmp/icon-1024.png --out /tmp/icon.iconset/icon_${s}x${s}@2x.png
done
iconutil -c icns /tmp/icon.iconset -o build/icon.icns
python3 -c "from PIL import Image; Image.open('/tmp/icon-1024.png').save('build/icon.ico', \
  sizes=[(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)])"
for s in 16 32 48 64 128 256 512; do
  sips -z $s $s /tmp/icon-1024.png --out build/icons/${s}x${s}.png
done
```

The `.ico` holds every size Windows asks for, from the file list at 16 to the extra-large view at
256. A file holding only the largest is legal, and Windows scales it down itself, but a mascot with
two eyes in it does not survive that to 16 pixels.

`build/icons/` is the same set for Linux, where the packages install one file per size into the
`hicolor` theme. They are rendered here rather than at packaging time for the reason the other two
are: a bitmap committed beside the drawing keeps a rasteriser out of a release build. A desktop is
required to read PNG and free to ignore SVG, so the drawing alone would leave the launcher entry
without an icon anywhere GTK has no librsvg loader.

A development checkout can produce a configured or unconfigured binary depending
on the build environment. Follow [credentials](setup.md#credentials) before packaging
for inference; Finder does not load shell configuration. Packaging itself does not
check that backend credentials exist.

To check the bundle, use the [packaged-app drivers](testing.md#current-regression-checks).
AppKit takes the menu-bar name from the bundle's `CFBundleName`; changing
`app.name` would also change Electron's user-data location.
