/**
 * Whether the rows a decision card's answer rests on have been on screen (PROMPT-4).
 *
 * A card marks each element its answer rests on with `data-deciding`: `all` for every drawn row of
 * it, `first` for its first row only, and `standing` for a row only a standing answer waits on. A
 * row counts once all of it has been in view at some moment: inside every box that clips it, in a
 * window that is showing, and not covered by anything else in the window. A change in the card's
 * width starts the count again, and so does a change in how many rows an element draws. Before the
 * first measurement nothing counts, so a card drawn where nothing measures takes no approval.
 */
import { createContext, useEffect, useState, type RefObject } from 'react'

export interface Box { top: number; right: number; bottom: number; left: number }

/** A row as measured once: its width, and the part of it in view, from its own left edge. */
export interface Row { width: number; seen: readonly [number, number] | null }

/** One deciding element as measured once. `hidden` is an element with text and no drawn row. */
export interface Looked<K> { key: K; standing: boolean; rows: readonly Row[]; hidden: boolean }

/** Rows not yet read: those every approval waits on, and those only a standing answer waits on. */
export interface Left { left: number; standing: number }

/** How far, in pixels, a measurement may be off and still count. */
const SLACK = 1

const union = (spans: [number, number][], [from, to]: readonly [number, number]): [number, number][] => {
  const all = [...spans, [from, to] as [number, number]].sort((a, b) => a[0] - b[0])
  const merged: [number, number][] = []
  for (const span of all) {
    const last = merged[merged.length - 1]
    if (last && span[0] <= last[1] + SLACK) last[1] = Math.max(last[1], span[1])
    else merged.push([span[0], span[1]])
  }
  return merged
}

const covers = (spans: [number, number][], width: number): boolean =>
  spans.some(([from, to]) => from <= SLACK && to >= width - SLACK)

/** What has been seen of one card's deciding rows across measurements. */
export class Ledger<K> {
  private width: number | null = null
  private readonly kept = new Map<K, [number, number][][]>()

  /** Whether row `at` of `key`, drawing `rows` rows at this `width`, has been read already. */
  read(key: K, rows: number, at: number, width: number, row: number): boolean {
    const seen = this.kept.get(key)
    return this.width === width && seen?.length === rows && covers(seen[at] ?? [], row)
  }

  take(width: number, looked: readonly Looked<K>[]): Left {
    if (this.width === null || Math.abs(width - this.width) > SLACK / 2) {
      this.kept.clear()
      this.width = width
    }
    const left: Left = { left: 0, standing: 0 }
    const present = new Set<K>()
    for (const { key, standing, rows, hidden } of looked) {
      present.add(key)
      let seen = this.kept.get(key)
      if (!seen || seen.length !== rows.length) {
        seen = rows.map(() => [])
        this.kept.set(key, seen)
      }
      const unread = rows.filter((row, at) => {
        const spans = row.seen ? union(seen[at] ?? [], row.seen) : seen[at] ?? []
        seen[at] = spans
        return !covers(spans, row.width)
      }).length + (hidden ? 1 : 0)
      if (standing) left.standing += unread
      else left.left += unread
    }
    for (const key of [...this.kept.keys()]) if (!present.has(key)) this.kept.delete(key)
    return left
  }
}

/** Line boxes grouped into rows: boxes that share most of their height are one row. */
export function rowsOf(boxes: readonly Box[]): Box[] {
  const rows: Box[] = []
  for (const box of [...boxes].sort((a, b) => a.top - b.top || a.left - b.left)) {
    const row = rows[rows.length - 1]
    const shared = row ? Math.min(row.bottom, box.bottom) - Math.max(row.top, box.top) : 0
    if (row && shared >= Math.min(row.bottom - row.top, box.bottom - box.top) / 2) {
      row.top = Math.min(row.top, box.top)
      row.bottom = Math.max(row.bottom, box.bottom)
      row.left = Math.min(row.left, box.left)
      row.right = Math.max(row.right, box.right)
    } else {
      rows.push({ ...box })
    }
  }
  return rows
}

/** The boxes the text in `element` is drawn in; for `first`, only its first line of text. */
function lineBoxes(element: Element, first: boolean): Box[] {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT)
  const range = document.createRange()
  const boxes: Box[] = []
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.nodeValue ?? ''
    const start = text.search(/\S/)
    if (start < 0) continue
    if (first) {
      const end = text.indexOf('\n', start)
      range.setStart(node, start)
      range.setEnd(node, end < 0 ? text.length : end)
    } else {
      range.selectNodeContents(node)
    }
    for (const box of range.getClientRects()) {
      if (box.width >= SLACK && box.height >= 2 * SLACK) boxes.push({ top: box.top, right: box.right, bottom: box.bottom, left: box.left })
    }
    if (first && boxes.length) break
  }
  return boxes
}

interface Frame { clip: Box; opaque: boolean }

const meet = (a: Box, b: Box): Box => ({
  top: Math.max(a.top, b.top), right: Math.min(a.right, b.right), bottom: Math.min(a.bottom, b.bottom), left: Math.max(a.left, b.left),
})

/** For an element, the part of the window its content can be drawn in, and whether it is opaque. */
function frames(): (element: Element) => Frame {
  const root = document.documentElement
  const view: Frame = { clip: { top: 0, left: 0, right: root.clientWidth, bottom: root.clientHeight }, opaque: true }
  const known = new Map<Element, Frame>()
  const of = (element: Element | null): Frame => {
    if (!element) return view
    const kept = known.get(element)
    if (kept) return kept
    const above = of(element.parentElement)
    const style = getComputedStyle(element)
    let clip = above.clip
    if (style.display !== 'inline' && style.display !== 'contents') {
      const box = element.getBoundingClientRect()
      const left = box.left + element.clientLeft
      const top = box.top + element.clientTop
      const inner = { left, top, right: left + element.clientWidth, bottom: top + element.clientHeight }
      if (style.overflowX !== 'visible') clip = meet(clip, { ...clip, left: inner.left, right: inner.right })
      if (style.overflowY !== 'visible') clip = meet(clip, { ...clip, top: inner.top, bottom: inner.bottom })
    }
    const frame = { clip, opaque: above.opaque && Number(style.opacity) >= 0.9 && style.visibility === 'visible' }
    known.set(element, frame)
    return frame
  }
  return of
}

/** The part of `row` in view and uncovered, from its left edge; null unless all its height is. */
function sight(element: Element, row: Box, clip: Box): [number, number] | null {
  if (row.top < clip.top - SLACK || row.bottom > clip.bottom + SLACK) return null
  const from = Math.max(row.left, clip.left)
  const to = Math.min(row.right, clip.right)
  if (to - from < SLACK) return null
  for (const x of [from + SLACK, (from + to) / 2, to - SLACK]) {
    for (const y of [row.top + SLACK, (row.top + row.bottom) / 2, row.bottom - SLACK]) {
      const hit = document.elementFromPoint(x, y)
      if (!hit || !element.contains(hit)) return null
    }
  }
  return [from - row.left, to - row.left]
}

/** Measures the deciding rows of `card` once, skipping rows `ledger` has already counted. */
export function measure(card: Element, ledger: Ledger<Element>): { width: number; looked: Looked<Element>[] } {
  const width = card.getBoundingClientRect().width
  const frameOf = frames()
  const looked = [...card.querySelectorAll('[data-deciding]')].map((element) => {
    const want = element.getAttribute('data-deciding')
    const drawn = rowsOf(lineBoxes(element, want === 'first'))
    const boxes = want === 'first' ? drawn.slice(0, 1) : drawn
    const frame = frameOf(element)
    const rows = boxes.map((box, at): Row => {
      const span = box.right - box.left
      if (ledger.read(element, boxes.length, at, width, span)) return { width: span, seen: [0, span] }
      return { width: span, seen: frame.opaque ? sight(element, box, frame.clip) : null }
    })
    return { key: element, standing: want === 'standing', rows, hidden: !rows.length && !!element.textContent?.trim() }
  })
  return { width, looked }
}

/** What an answer row knows: whether anything was measured yet, and the rows left to read. */
export interface OnScreen extends Left { measured: boolean; note: string }

export const NOT_MEASURED: OnScreen = { measured: false, left: 0, standing: 0, note: '' }

export const ShownContext = createContext<OnScreen>(NOT_MEASURED)

/** Whether an answer may be given yet. A standing answer also waits on the rows only it grants by. */
export const mayAnswer = (shown: OnScreen, standing = false): boolean =>
  shown.measured && shown.left === 0 && (!standing || shown.standing === 0)

/** How often the rows are measured again when nothing has said the window changed. */
const POLL_MS = 500

/** The rows left to read in the card holding `answers`, kept current while it is mounted. */
export function useShown(answers: RefObject<HTMLElement | null>): Left & { measured: boolean } {
  const [shown, setShown] = useState({ measured: false, left: 0, standing: 0 })
  useEffect(() => {
    const card = answers.current?.closest('.confirm')
    if (!card) return
    const ledger = new Ledger<Element>()
    let frame = 0
    const update = (): void => {
      frame = 0
      if (document.visibilityState === 'hidden') return
      const { width, looked } = measure(card, ledger)
      const next = ledger.take(width, looked)
      setShown((last) => last.measured && last.left === next.left && last.standing === next.standing ? last : { measured: true, ...next })
    }
    const soon = (): void => { if (!frame) frame = requestAnimationFrame(update) }
    const resized = new ResizeObserver(soon)
    resized.observe(card)
    for (let at = card.parentElement; at; at = at.parentElement) {
      const { overflowX, overflowY } = getComputedStyle(at)
      if (overflowX !== 'visible' || overflowY !== 'visible') resized.observe(at)
    }
    const events = ['scroll', 'animationend', 'transitionend', 'visibilitychange'] as const
    for (const event of events) document.addEventListener(event, soon, { capture: true, passive: true })
    window.addEventListener('resize', soon)
    const poll = setInterval(soon, POLL_MS)
    soon()
    return () => {
      cancelAnimationFrame(frame)
      clearInterval(poll)
      resized.disconnect()
      for (const event of events) document.removeEventListener(event, soon, { capture: true })
      window.removeEventListener('resize', soon)
    }
  }, [answers])
  return shown
}

/** What the answer row says while rows are left, or null once there are none. */
export function leftWords(shown: OnScreen): string | null {
  if (!shown.measured) return null
  const say = (rows: number, before: string): string => `${rows} more line${rows === 1 ? '' : 's'} to read before ${before}`
  if (shown.left > 0) return say(shown.left, 'approving')
  if (shown.standing > 0) return say(shown.standing, 'remembering')
  return null
}
