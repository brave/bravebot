/**
 * The desktop window's title, named after the session on screen and marked with what it is doing,
 * so several windows can be told apart. It mirrors `crates/tui/src/title.rs`.
 *
 * Unlike the terminal's, the window always has a title: with no name and nothing to say about the
 * state it is the bare prefix the page already shows.
 */

/** What the session on screen is doing, as the title says it. */
export type TitleState = 'ready' | 'working' | 'waiting'

/** What every title starts with, in the casing the page title uses. */
export const PREFIX = 'Bravebot'

/** The most code points a title takes, the marker and the `…` counted. */
export const MOST_COLUMNS = 60

const MARKERS: Record<TitleState, string | undefined> = {
  ready: undefined,
  working: '✦',
  waiting: '[!]',
}

/**
 * `text` with each C0 and C1 control character replaced by a visible one, as `render::printable`
 * does: a C0 control or DEL by its control picture (U+2400 to U+2421), a C1 control written out as
 * `‹U+XXXX›`.
 */
function printable(text: string): string {
  let out = ''
  for (const c of text) {
    const code = c.codePointAt(0) ?? 0
    if (code <= 0x1f) out += String.fromCodePoint(0x2400 + code)
    else if (code === 0x7f) out += '␡'
    else if (code >= 0x80 && code <= 0x9f) out += `‹U+${code.toString(16).toUpperCase().padStart(4, '0')}›`
    else out += c
  }
  return out
}

/** `text` cut to `most` code points, ending in `…` where anything was cut. */
function cut(text: string, most: number): string {
  const points = Array.from(text)
  if (points.length <= most) return text
  return points.slice(0, most - 1).join('') + '…'
}

/** The window title for a session named `name` in `state`. */
export function windowTitle(name: string, state: TitleState): string {
  const trimmed = name.trim()
  const title = trimmed === '' ? PREFIX : `${PREFIX} · ${printable(trimmed)}`
  const marker = MARKERS[state]
  return cut(marker === undefined ? title : `${marker} ${title}`, MOST_COLUMNS)
}
