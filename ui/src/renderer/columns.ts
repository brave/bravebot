/**
 * How wide the three columns are, and what stops them being silly.
 *
 * The widths live here rather than in CSS because a drag has to clamp against the window,
 * not just against each column's own limits: two side columns that are each individually
 * reasonable can still leave no room for the transcript. Every value that decides a width
 * is in this file so there is one place to reason about the arithmetic.
 */

/** What each side column may be, and where it starts. */
export const SIDES = {
  left: { min: 200, max: 400, initial: 250 },
  right: { min: 240, max: 420, initial: 300 },
} as const

export type Side = keyof typeof SIDES

/**
 * The narrowest the transcript is allowed to get.
 *
 * The transcript is the reason the window is open; the other two columns are apparatus.
 * So when something has to give, it is never this.
 */
const CENTER_MIN = 480

/**
 * The two 1px dividers, which are part of the window even though nobody sizes them.
 *
 * They sit in the same grid as the columns, so a window's worth of room is two pixels
 * short of what the columns may divide between them. Left out of the arithmetic, the
 * transcript's floor was quietly two pixels lower than [`CENTER_MIN`] claims.
 */
const DIVIDERS = 2

export interface Widths {
  left: number
  right: number
}

/** Which columns are folded shut, and how wide they will be when they come back. */
export interface Layout {
  widths: Widths
  collapsed: Record<Side, boolean>
}

export const INITIAL: Widths = { left: SIDES.left.initial, right: SIDES.right.initial }

export const INITIAL_LAYOUT: Layout = {
  widths: INITIAL,
  collapsed: { left: false, right: false },
}

/**
 * How wide a column should actually be drawn.
 *
 * A folded column keeps its width in [`Layout.widths`] the whole time it is shut. That is
 * what makes it come back where it was rather than at the default, and it is why collapse
 * is a flag rather than a width of zero: [`fit`] clamps widths up to their minimum, so a
 * zero would be clamped straight back to a visible column.
 *
 * Zero is known here and nowhere else.
 */
export const shown = (layout: Layout, side: Side): number =>
  layout.collapsed[side] ? 0 : layout.widths[side]

/**
 * How long a fold takes, for the code that has to wait for one.
 *
 * Must match `--panel-duration` in the stylesheet, which is the timing the context panels
 * already fold on. The value is duplicated because CSS owns the animation and JavaScript
 * owns when it is armed; there is no reading one from the other that is worth the cost.
 */
export const FOLD_MS = 180

const clamp = (value: number, min: number, max: number): number =>
  Math.min(Math.max(Math.round(value), min), max)

/**
 * Widths that fit, given how much room there actually is.
 *
 * Each column is held to its own limits first, and then — if the transcript would be
 * squeezed below [`CENTER_MIN`] — the side columns give up the difference, the right one
 * first. Right before left because the context panel is glanceable and the session list is
 * navigational: losing a few pixels of "files read" costs less than truncating the session
 * titles you steer by.
 *
 * A window too narrow to satisfy even the minimums leaves both sides at their minimum and
 * lets the transcript take what is left. Something has to be wrong at that size, and
 * columns that keep shrinking below the point of usefulness only hide it. That branch is
 * in fact unreachable through the interface: the narrowest window Electron will allow is
 * 900px against `200 + 240 + 380` and two 1px dividers, so both columns can always be
 * opened at their minimums with the transcript still above its floor. The arithmetic is
 * split across this file and the window's `minWidth`, which is why it is written down.
 *
 * A folded column takes no part in any of it. It occupies nothing, so it is not in the
 * sum; and its width is passed back unclamped, because that width is not describing
 * anything on screen — it is the width the column will be given when it is unfolded, and
 * clamping it against a window it is not currently in would quietly erode it. It is
 * clamped at the moment it comes back, which is the moment it starts to mean something.
 */
export function fit(widths: Widths, available: number, collapsed: Record<Side, boolean>): Widths {
  // On small windows the inspector overlays the conversation and consumes no grid width.
  if (available <= 1120) collapsed = { ...collapsed, right: true }

  let left = collapsed.left ? widths.left : clamp(widths.left, SIDES.left.min, SIDES.left.max)
  let right = collapsed.right ? widths.right : clamp(widths.right, SIDES.right.min, SIDES.right.max)

  let excess =
    (collapsed.left ? 0 : left) +
    (collapsed.right ? 0 : right) +
    CENTER_MIN -
    (available - DIVIDERS)
  if (excess > 0 && !collapsed.right) {
    const fromRight = Math.min(excess, right - SIDES.right.min)
    right -= fromRight
    excess -= fromRight
  }
  if (excess > 0 && !collapsed.left) {
    left -= Math.min(excess, left - SIDES.left.min)
  }

  return { left, right }
}

/**
 * The widths from last time, if they are still usable.
 *
 * Held by the main process in a file. `localStorage` was the obvious home and is the wrong
 * one: the renderer is loaded from `file://`, and Chromium keeps no storage for that origin
 * between launches — a write succeeds, reads back fine all session, and is gone by the next
 * one. That failure is silent, which is what makes it worth a comment here.
 *
 * Anything unreadable — no file yet, a truncated one, a hand-edited one — comes back as
 * `null` and the defaults stand. A remembered layout is a convenience; treating a bad one
 * as an error would be the tail wagging the dog.
 */
export async function remembered(): Promise<Layout | null> {
  try {
    const stored = await window.bravebot.readLayout()
    if (!stored) return null
    return { widths: { left: stored.left, right: stored.right }, collapsed: stored.collapsed }
  } catch {
    return null
  }
}

/** Best-effort. A layout that cannot be written down is not worth an error. */
export function remember(layout: Layout): void {
  try {
    window.bravebot.writeLayout({
      left: layout.widths.left,
      right: layout.widths.right,
      collapsed: layout.collapsed,
    })
  } catch {
    // Nothing to do and nothing worth saying: the columns still work this session.
  }
}
