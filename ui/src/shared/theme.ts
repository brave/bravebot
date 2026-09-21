/**
 * The inks this window draws itself in, and the palettes it can be repainted with.
 *
 * The nine roles, the twenty-one named palettes and the JSON dialect are a port of
 * `crates/tui/src/theme.rs` in the agent's own repository, kept faithful to it so that a palette
 * written for one is recognisable in the other and a scheme called `nord` means the same thing in
 * both. That is where the format came from, and it is the whole of the relationship.
 *
 * It is a port and not a link. Nothing here reads anything the agent owns: the palettes are
 * compiled into this app, the ones somebody writes are read from this app's own directory, and
 * which one is chosen is a key in `bravebot-ui.json` beside the column widths. The agent is a
 * subprocess this window drives, not something it is installed alongside — a machine may have this
 * app and no `~/.bravebot` at all, and a window that could not paint itself until the terminal had
 * been run once would be a strange thing to have built.
 *
 * What is *not* ported is the part that is a terminal's problem: the OSC 11 round trip that asks
 * the emulator what colour its background is. The equivalent here is `prefers-color-scheme`, which
 * the stylesheet has always used, and which is what `brave` still means.
 *
 * Nothing here imports `electron` or `react`, for the reason `commands.ts` next door gives: the
 * main process reads these files off disk and the renderer paints from them, and neither should
 * have to reach through the other for the definition of what a theme is.
 */

/**
 * The nine meanings a palette assigns a colour to.
 *
 * Nine rather than the nineteen tokens `styles.css` actually uses, because the extra ten are
 * derivable — a rule at 12% of the failure colour, a ground four points off the background — and
 * asking somebody writing a palette for nineteen values would be asking them to do arithmetic the
 * stylesheet can do itself. The derivation lives in the `:root[data-theme]` block there.
 */
export const ROLE_NAMES = [
  'background',
  'text',
  'muted',
  'ok',
  'fail',
  'running',
  'accent',
  'note',
  'primary',
] as const

export type Role = (typeof ROLE_NAMES)[number]

/**
 * One ink: `#rrggbb`, or `null` for "leave this one as the window has it".
 *
 * `null` is what the JSON spells `"none"` and what a key left out of a palette file means. It is
 * the same idea as `Color::Reset` in the terminal — inherit rather than paint — and it is the
 * reason a palette can be three lines long and still be a palette.
 */
export type Ink = string | null

export type Palette = Record<Role, Ink>

export interface Theme {
  name: string
  palette: Palette
}

/** The default, and what every window has today: the app's own macOS palette,
 * following the system between light and dark. */
export const BRAVE = 'brave'

/** `brave` as a theme, for the places that need one and have nothing else — a list not yet read,
 * a chosen name that is no longer on offer. It is also the way back, so it is never absent. */
export const BRAVE_THEME: Theme = { name: BRAVE, palette: inheritAll() }

/**
 * The longest a theme name may be.
 *
 * A value longer than this is not a choice that got garbled, it is not a theme name at all, and
 * carrying a kilobyte of it around in the remembered state would be taking it seriously.
 */
const MAX_THEME_BYTES = 64

/**
 * What `brave` resolves to, so that a palette which inherits a role has something to inherit.
 *
 * Exported although nothing imports them: `scripts/drive-theme.mjs` finds them by this spelling.
 *
 * These duplicate the `:root` block in `styles.css`, which is a real cost and a deliberate one.
 * The alternative is reading the tokens back out of the document with `getComputedStyle`, and that
 * only tells the truth while no theme is applied — the moment one is, the values it would report
 * are the theme's own, so a partial palette applied second would inherit from the first. A pair of
 * literals cannot drift silently either: `scripts/drive-theme.mjs` asserts they still match what
 * the stylesheet computes.
 *
 * `note` and `primary` are the same orange in both, because the accent is the one thing the dark
 * block in `styles.css` deliberately does not override.
 */
export const BRAVE_LIGHT: Palette = {
  background: '#ffffff',
  text: '#1c1c1e',
  muted: '#6b6b70',
  ok: '#1a7f37',
  fail: '#c0392b',
  running: '#9a6700',
  accent: '#6f42c1',
  note: '#f2600c',
  primary: '#f2600c',
}

export const BRAVE_DARK: Palette = {
  background: '#1c1c1e',
  text: '#f2f2f7',
  muted: '#a1a1a6',
  ok: '#3fb950',
  fail: '#f85149',
  running: '#d4a72c',
  accent: '#b392f0',
  note: '#f2600c',
  primary: '#f2600c',
}

/** Every role inherited: `brave` paints nothing of its own, which is what makes it the default. */
function inheritAll(): Palette {
  return {
    background: null,
    text: null,
    muted: null,
    ok: null,
    fail: null,
    running: null,
    accent: null,
    note: null,
    primary: null,
  }
}

function named(name: string, palette: Omit<Palette, never>): Theme {
  return { name, palette }
}

/**
 * The palettes compiled in: `brave` first, then twenty-one named schemes alphabetically.
 *
 * Hex values are the agent's, taken from `theme.rs` unchanged rather than re-derived from each
 * scheme's own publication — two ports of Nord that disagree by a digit would be worse than one
 * port that is wrong in both places, because only the second is findable.
 */
export const BUILTINS: readonly Theme[] = sortBraveFirst([
  BRAVE_THEME,
  named('catppuccin-mocha', {
    background: '#1e1e2e',
    text: '#cdd6f4',
    muted: '#6c7086',
    ok: '#a6e3a1',
    fail: '#f38ba8',
    running: '#f9e2af',
    accent: '#cba6f7',
    note: '#fab387',
    primary: '#89b4fa',
  }),
  named('catppuccin-macchiato', {
    background: '#24273a',
    text: '#cad3f5',
    muted: '#6e738d',
    ok: '#a6da95',
    fail: '#ed8796',
    running: '#eed49f',
    accent: '#c6a0f6',
    note: '#f5a97f',
    primary: '#8aadf4',
  }),
  named('catppuccin-latte', {
    background: '#eff1f5',
    text: '#4c4f69',
    muted: '#9ca0b0',
    ok: '#40a02b',
    fail: '#d20f39',
    running: '#df8e1d',
    accent: '#8839ef',
    note: '#fe640b',
    primary: '#1e66f5',
  }),
  named('tokyonight', {
    background: '#1a1b26',
    text: '#a9b1d6',
    muted: '#565f89',
    ok: '#9ece6a',
    fail: '#f7768e',
    running: '#e0af68',
    accent: '#bb9af7',
    note: '#ff9e64',
    primary: '#7aa2f7',
  }),
  named('tokyonight-storm', {
    background: '#24283b',
    text: '#c0caf5',
    muted: '#565f89',
    ok: '#9ece6a',
    fail: '#f7768e',
    running: '#e0af68',
    accent: '#bb9af7',
    note: '#ff9e64',
    primary: '#7aa2f7',
  }),
  named('gruvbox-dark', {
    background: '#282828',
    text: '#ebdbb2',
    muted: '#928374',
    ok: '#b8bb26',
    fail: '#fb4934',
    running: '#fabd2f',
    accent: '#d3869b',
    note: '#fe8019',
    primary: '#83a598',
  }),
  named('gruvbox-light', {
    background: '#fbf1c7',
    text: '#3c3836',
    muted: '#928374',
    ok: '#79740e',
    fail: '#9d0006',
    running: '#b57614',
    accent: '#8f3f71',
    note: '#af3a03',
    primary: '#076678',
  }),
  named('dracula', {
    background: '#282a36',
    text: '#f8f8f2',
    muted: '#6272a4',
    ok: '#50fa7b',
    fail: '#ff5555',
    running: '#f1fa8c',
    accent: '#bd93f9',
    note: '#ffb86c',
    primary: '#8be9fd',
  }),
  named('nord', {
    background: '#2e3440',
    text: '#d8dee9',
    muted: '#4c566a',
    ok: '#a3be8c',
    fail: '#bf616a',
    running: '#ebcb8b',
    accent: '#b48ead',
    note: '#d08770',
    primary: '#88c0d0',
  }),
  named('rose-pine', {
    background: '#191724',
    text: '#e0def4',
    muted: '#6e6a86',
    ok: '#31748f',
    fail: '#eb6f92',
    running: '#f6c177',
    accent: '#c4a7e7',
    note: '#ebbcba',
    primary: '#9ccfd8',
  }),
  named('kanagawa', {
    background: '#1f1f28',
    text: '#dcd7ba',
    muted: '#727169',
    ok: '#98bb6c',
    fail: '#c34043',
    running: '#e6c384',
    accent: '#957fb8',
    note: '#ffa066',
    primary: '#7e9cd8',
  }),
  named('everforest', {
    background: '#2d353b',
    text: '#d3c6aa',
    muted: '#859289',
    ok: '#a7c080',
    fail: '#e67e80',
    running: '#dbbc7f',
    accent: '#d699b6',
    note: '#e69875',
    primary: '#7fbbb3',
  }),
  named('ayu-dark', {
    background: '#0b0e14',
    text: '#bfbdb6',
    muted: '#565b66',
    ok: '#aad94c',
    fail: '#d95757',
    running: '#ffb454',
    accent: '#d2a6ff',
    note: '#ff8f40',
    primary: '#59c2ff',
  }),
  named('one-dark', {
    background: '#282c34',
    text: '#abb2bf',
    muted: '#5c6370',
    ok: '#98c379',
    fail: '#e06c75',
    running: '#e5c07b',
    accent: '#c678dd',
    note: '#d19a66',
    primary: '#61afef',
  }),
  named('solarized-dark', {
    background: '#002b36',
    text: '#839496',
    muted: '#586e75',
    ok: '#859900',
    fail: '#dc322f',
    running: '#b58900',
    accent: '#d33682',
    note: '#cb4b16',
    primary: '#268bd2',
  }),
  named('solarized-light', {
    background: '#fdf6e3',
    text: '#657b83',
    muted: '#93a1a1',
    ok: '#859900',
    fail: '#dc322f',
    running: '#b58900',
    accent: '#d33682',
    note: '#cb4b16',
    primary: '#268bd2',
  }),
  named('github-dark', {
    background: '#0d1117',
    text: '#e6edf3',
    muted: '#848d97',
    ok: '#3fb950',
    fail: '#f85149',
    running: '#d29922',
    accent: '#a371f7',
    note: '#db6d28',
    primary: '#4493f8',
  }),
  named('monokai', {
    background: '#272822',
    text: '#f8f8f2',
    muted: '#75715e',
    ok: '#a6e22e',
    fail: '#f92672',
    running: '#e6db74',
    accent: '#ae81ff',
    note: '#fd971f',
    primary: '#66d9ef',
  }),
  named('flexoki-dark', {
    background: '#100f0f',
    text: '#cecdc3',
    muted: '#878580',
    ok: '#879a39',
    fail: '#d14d41',
    running: '#d0a215',
    accent: '#a02f6f',
    note: '#da702c',
    primary: '#4385be',
  }),
  named('oxocarbon', {
    background: '#161616',
    text: '#f2f4f8',
    muted: '#525252',
    ok: '#42be65',
    fail: '#ee5396',
    running: '#ff7eb6',
    accent: '#be95ff',
    note: '#3ddbd9',
    primary: '#78a9ff',
  }),
  named('cobalt2', {
    background: '#193549',
    text: '#ffffff',
    muted: '#5a7b92',
    ok: '#3ad900',
    fail: '#ff628c',
    running: '#ffc600',
    accent: '#ff0088',
    note: '#ff9d00',
    primary: '#0088ff',
  }),
])

/**
 * `brave` first, every other name alphabetical.
 *
 * The default belongs at the top because it is the row somebody scrolling the picker is most
 * likely to be looking for — the way back. Everything else is alphabetical so that a list which
 * grows stays scannable.
 */
export function sortBraveFirst(themes: Theme[]): Theme[] {
  return [...themes].sort((a, b) => {
    if (a.name === BRAVE) return b.name === BRAVE ? 0 : -1
    if (b.name === BRAVE) return 1
    return a.name.localeCompare(b.name)
  })
}

/** The theme of a given name among those on offer, or nothing.
 * `system` was the old name for `brave`, and is still understood. */
export function findTheme(themes: readonly Theme[], name: string): Theme | undefined {
  const wanted = name === 'system' ? BRAVE : name
  return themes.find((theme) => theme.name === wanted)
}

/**
 * Which theme is chosen, out of the remembered state.
 *
 * Never null, for the reason `parseView` gives about its own shape: no key, a key with rubbish in
 * it, and a key somebody blanked all describe the same window, and telling them apart would hand
 * the caller a decision it does not have to make. All three are `brave`, which is the window this
 * app has always drawn.
 *
 * A name that is not on offer is *kept* rather than dropped, which is where this parts company
 * with `parsePanels` next door — that one drops a panel name this build does not have. The
 * difference is that a theme can be a file somebody wrote: moving `themes/mine.json` out of the
 * way for an afternoon should not permanently reset a choice made weeks ago. So the name survives
 * here and the *resolution* falls back to `brave` at the moment of painting, which means the
 * palette comes back when the file does.
 */
export function parseChosenTheme(value: unknown): string {
  if (typeof value !== 'string') return BRAVE
  const name = value.trim()
  if (name.length === 0 || name.length > MAX_THEME_BYTES) return BRAVE
  return name === 'system' ? BRAVE : name
}

/** Rec. 709 luma, in integers so that a threshold is a comparison rather than a float two call
 * sites could round differently. The terminal decides light from dark the same way. */
function isLight(hex: string): boolean {
  const channels = channel6(hex.startsWith('#') ? hex.slice(1) : hex)
  if (!channels) return false
  const [r, g, b] = channels
  return 2126 * r + 7152 * g + 722 * b > 1_270_000
}

/** Choose the higher WCAG contrast, using linear-light luminance rather than encoded RGB. */
function contrastInk(hex: string): string {
  const channels = channel6(hex.replace(/^#/, '')) ?? [0, 0, 0]
  const linear = channels.map((channel) => { const c = channel / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4 })
  const luminance = .2126 * linear[0]! + .7152 * linear[1]! + .0722 * linear[2]!
  return (luminance + .05) / .05 >= 1.05 / (luminance + .05) ? '#000000' : '#ffffff'
}

function channel6(hex: string): [number, number, number] | null {
  if (!/^[0-9a-fA-F]{6}$/.test(hex)) return null
  return [
    Number.parseInt(hex.slice(0, 2), 16),
    Number.parseInt(hex.slice(2, 4), 16),
    Number.parseInt(hex.slice(4, 6), 16),
  ]
}

/**
 * One palette file from the themes directory, or nothing if it is not one.
 *
 * The dialect is the agent's: `#rrggbb` for a colour, `"none"` to inherit, a `defs` block of names
 * a role may refer to instead of repeating a hex, and any key left out inheriting too. A def that
 * names another def is refused rather than chased, which is one rule instead of a cycle check.
 *
 * A file with a bad colour in it yields nothing at all rather than a palette with eight roles in
 * it. Half a theme is harder to notice than no theme, and this is somebody's own file: they should
 * find out it is broken by it not appearing, not by one panel being the wrong colour a week later.
 */
export function parseUserTheme(name: string, contents: string): Theme | null {
  let file: unknown
  try {
    file = JSON.parse(contents)
  } catch {
    return null
  }
  if (typeof file !== 'object' || file === null || Array.isArray(file)) return null
  const held = file as Record<string, unknown>

  const defs = new Map<string, string>()
  if (typeof held.defs === 'object' && held.defs !== null && !Array.isArray(held.defs)) {
    for (const [key, value] of Object.entries(held.defs as Record<string, unknown>)) {
      if (typeof value === 'string') defs.set(key, value.trim())
    }
  }

  const palette: Partial<Palette> = {}
  for (const role of ROLE_NAMES) {
    const raw = held[role]
    if (raw === undefined) {
      palette[role] = null
      continue
    }
    if (typeof raw !== 'string') return null
    const ink = resolveInk(raw, defs)
    if (ink === undefined) return null
    palette[role] = ink
  }
  return { name, palette: palette as Palette }
}

/** A colour, `null` for inherit, or `undefined` for "that is not a colour". */
function resolveInk(value: string, defs: Map<string, string>): Ink | undefined {
  const raw = value.trim()
  if (raw === 'none') return null
  if (raw.startsWith('#')) return channel6(raw.slice(1)) ? raw.toLowerCase() : undefined
  const def = defs.get(raw)
  if (def === undefined) return undefined
  if (def === 'none') return null
  return def.startsWith('#') && channel6(def.slice(1)) ? def.toLowerCase() : undefined
}

/** Whether a theme paints its own ground, rather than leaving the window's. */
export function paintsBackground(theme: Theme): boolean {
  return theme.palette.background !== null
}

/**
 * The custom properties a theme sets on the document root.
 *
 * Every role resolves to a real colour here — inherited ones against `brave` for the appearance in
 * force — so that the stylesheet's derivations never have to reason about a missing value. The two
 * `-ink` entries and `--role-scheme` are the three things arithmetic in CSS cannot do for itself:
 * whether text on a given ground should be black or white, and whether the native scrollbars and
 * form controls in this window should be drawn dark.
 */
export function roleVariables(theme: Theme, dark: boolean): Record<string, string> {
  const base = dark ? BRAVE_DARK : BRAVE_LIGHT
  const resolved = {} as Record<Role, string>
  for (const role of ROLE_NAMES) {
    resolved[role] = theme.palette[role] ?? (base[role] as string)
  }
  const variables: Record<string, string> = {}
  for (const role of ROLE_NAMES) variables[`--role-${role}`] = resolved[role]
  variables['--role-note-ink'] = contrastInk(resolved.note)
  variables['--role-primary-ink'] = contrastInk(resolved.primary)
  variables['--role-scheme'] = isLight(resolved.background) ? 'light' : 'dark'
  return variables
}
