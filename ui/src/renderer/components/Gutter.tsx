import { useCallback, useEffect, useRef, useState } from 'react'
import {
  FOLD_MS,
  INITIAL,
  INITIAL_LAYOUT,
  type Layout,
  SIDES,
  type Side,
  type Widths,
  fit,
  remember,
  remembered,
} from '../columns'

/**
 * The draggable divider between two columns, and the state behind it.
 *
 * The divider *is* the border between the panels rather than something drawn beside one:
 * a 1px grid track that the panels no longer draw themselves. That keeps the resting
 * appearance identical to the fixed layout it replaces — a seam, not a control — while a
 * transparent pad either side gives the pointer something realistic to catch.
 */

/**
 * Live column widths, clamped to the window, and which columns are folded shut.
 *
 * Returns the widths, a drag starter for each side, which side is being dragged, and a
 * fold toggle. The widths are re-fitted whenever the window changes size, so a layout that
 * was fine on a wide display does not leave the transcript in a sliver when the window is
 * made narrow.
 *
 * Widths and folds are one piece of state rather than two. Kept apart, a fold and the
 * re-fit it implies would land in separate renders, and the frame between them would draw
 * a layout that has been folded but not yet made to fit.
 */
export function useColumns(): {
  widths: Widths
  collapsed: Record<Side, boolean>
  dragging: Side | null
  folding: boolean
  start: (side: Side, event: React.PointerEvent<HTMLDivElement>) => void
  reset: (side: Side) => void
  nudge: (side: Side, by: number) => void
  toggle: (side: Side) => void
} {
  const [layout, setLayout] = useState<Layout>(() => ({ ...INITIAL_LAYOUT, collapsed: { left: false, right: window.innerWidth <= 1120 } }))
  const [dragging, setDragging] = useState<Side | null>(null)
  // Armed by a fold and by nothing else. A drag, an arrow key and a window resize all
  // change the same widths and must all land instantly; only a fold is a movement anyone
  // wants to watch, so only a fold turns the animation on.
  const [folding, setFolding] = useState(false)
  const { widths, collapsed } = layout

  // The stored layout arrives from the main process, so it cannot seed `useState`. Until
  // it lands, nothing is written back: otherwise the defaults this started with would
  // overwrite the widths still being read.
  const loaded = useRef(false)
  useEffect(() => {
    let live = true
    void remembered().then((stored) => {
      if (live && stored) {
        setLayout({
          widths: fit(stored.widths, window.innerWidth, stored.collapsed),
          collapsed: window.innerWidth <= 1120 ? { ...stored.collapsed, right: true } : stored.collapsed,
        })
      }
      loaded.current = true
    })
    return () => {
      live = false
    }
  }, [])

  // What the drag is measured against. In a ref rather than state because it is read on
  // every pointer move and must not itself cause a render.
  const from = useRef<{ x: number; widths: Widths } | null>(null)

  useEffect(() => {
    let narrow = window.innerWidth <= 1120
    const onResize = (): void => {
      const nextNarrow = window.innerWidth <= 1120
      // Capture the transition before React runs the updater; `narrow` changes below.
      const enteringNarrow = nextNarrow && !narrow
      setLayout((old) => {
        const collapsed = enteringNarrow ? { ...old.collapsed, right: true } : old.collapsed
        return { ...old, collapsed, widths: fit(old.widths, window.innerWidth, collapsed) }
      })
      narrow = nextNarrow
    }
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  const start = useCallback((side: Side, event: React.PointerEvent<HTMLDivElement>): void => {
    // Only the primary button drags. A right-click on a divider should do nothing at all
    // rather than begin a resize the user cannot see the start of.
    if (event.button !== 0) return
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    from.current = { x: event.clientX, widths }
    setDragging(side)
  // `widths` is read at the moment the drag begins, which is exactly what the ref records.
  }, [widths])

  useEffect(() => {
    if (!dragging) return

    const onMove = (event: PointerEvent): void => {
      const origin = from.current
      if (!origin) return
      const delta = event.clientX - origin.x
      // The right divider moves the opposite way to the pointer: dragging it left makes
      // the right column wider.
      const next =
        dragging === 'left'
          ? { ...origin.widths, left: origin.widths.left + delta }
          : { ...origin.widths, right: origin.widths.right - delta }
      setLayout((old) => ({ ...old, widths: fit(next, window.innerWidth, old.collapsed) }))
    }

    const stop = (): void => {
      from.current = null
      setDragging(null)
    }

    // On window rather than on the divider, so a pointer that outruns the element — which
    // is normal during a fast drag — keeps steering it.
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', stop)
    window.addEventListener('pointercancel', stop)
    return () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', stop)
      window.removeEventListener('pointercancel', stop)
    }
  }, [dragging])

  // Written down when the drag ends rather than on every move: one write per gesture
  // instead of one per frame. A fold has no end to wait for, so it is written at once.
  useEffect(() => {
    if (!dragging && loaded.current) remember(layout)
  }, [dragging, layout])

  const reset = useCallback((side: Side): void => {
    setLayout((old) => ({
      ...old,
      widths: fit({ ...old.widths, [side]: INITIAL[side] }, window.innerWidth, old.collapsed),
    }))
  }, [])

  // Keyboard resizing goes through the same fitting as a drag, so a held arrow key stops
  // in exactly the place the pointer would have.
  const nudge = useCallback((side: Side, by: number): void => {
    setLayout((old) => ({
      ...old,
      widths: fit({ ...old.widths, [side]: old.widths[side] + by }, window.innerWidth, old.collapsed),
    }))
  }, [])

  const toggle = useCallback((side: Side): void => {
    setLayout((old) => {
      const collapsed = { ...old.collapsed, [side]: !old.collapsed[side] }
      // Re-fitted against the folds it is about to have rather than the ones it has, so a
      // column unfolding onto a narrow window arrives already legal instead of becoming so
      // a frame later.
      return { widths: fit(old.widths, window.innerWidth, collapsed), collapsed }
    })
    setFolding(true)
  }, [])

  // The animation is only armed while something is actually folding. The timer is re-armed
  // by a second fold landing mid-way through the first, which is why the folds themselves
  // are a dependency and not just the flag.
  useEffect(() => {
    if (!folding) return
    const timer = setTimeout(() => setFolding(false), FOLD_MS + 40)
    return () => clearTimeout(timer)
  }, [folding, layout.collapsed])

  return { widths, collapsed, dragging, folding, start, reset, nudge, toggle }
}

/**
 * One divider.
 *
 * Beside a folded column it stays on screen — it is the transcript's border, and the grid
 * needs its track — but it stops being a control: there is nothing to resize, and a drag
 * that silently changed a width nobody can see would be a second, invisible way to fold.
 * The handlers are removed rather than guarded from the inside, so no pointer can be
 * captured in the first place.
 */
export function Gutter({
  side,
  width,
  dragging,
  collapsed,
  onStart,
  onReset,
  onNudge,
}: {
  side: Side
  width: number
  dragging: boolean
  collapsed: boolean
  onStart: (side: Side, event: React.PointerEvent<HTMLDivElement>) => void
  onReset: (side: Side) => void
  onNudge: (side: Side, by: number) => void
}): React.JSX.Element {
  const keys = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    // A divider that can only be dragged is a divider some people cannot move at all.
    const step = event.shiftKey ? 32 : 8
    if (event.key === 'ArrowLeft') onNudge(side, -step)
    else if (event.key === 'ArrowRight') onNudge(side, step)
    else if (event.key === 'Home' || event.key === 'Enter') onReset(side)
    else return
    event.preventDefault()
  }

  return (
    <div
      className={`gutter ${side} ${dragging ? 'dragging' : ''} ${collapsed ? 'inert' : ''}`}
      // Kept as a separator, and kept named, even when it does nothing: announcing it as
      // unavailable says more than having it disappear from under the reader.
      role="separator"
      aria-orientation="vertical"
      aria-label={side === 'left' ? 'Resize the session list' : 'Resize the context panel'}
      // The one place in the window where a tooltip is the *only* way to learn what a
      // control does. A 1px seam has no room for a label, and the double-click that puts
      // the column back where it shipped is invisible until somebody does it by accident.
      // Dropped while the column is folded, along with every handler below: a seam that
      // promised a gesture it would ignore would be worse than a silent one.
      title={collapsed ? undefined : 'Drag to resize · double-click to reset'}
      aria-disabled={collapsed || undefined}
      aria-valuenow={width}
      aria-valuemin={SIDES[side].min}
      aria-valuemax={SIDES[side].max}
      tabIndex={collapsed ? -1 : 0}
      onPointerDown={collapsed ? undefined : (event) => onStart(side, event)}
      // The way back to the layout the app shipped with, without hunting for the pixel.
      onDoubleClick={collapsed ? undefined : () => onReset(side)}
      onKeyDown={collapsed ? undefined : keys}
    />
  )
}
