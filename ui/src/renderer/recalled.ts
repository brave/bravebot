/**
 * Which rows of the transcript a turn's retrievals drew on (RETRIEVE-27).
 *
 * The bridge names where each message was said and never what it said, so every row here is drawn
 * from this window's own copy of the transcript, and a place it cannot find is drawn as such
 * rather than guessed at.
 */
import type { RecalledExchange, RecalledPlace } from '../shared/protocol'
import type { Entry } from './transcript'
import type { Turns } from './turn-details'

/** One message drawn on: the row to scroll to, who said it, and its first line. */
export interface RecalledRow {
  /** The entry to reveal, or `null` where this transcript does not hold the message. */
  id: string | null
  who: 'you' | 'reply' | 'tool' | 'exchange' | 'missing'
  line: string
  /** The message's place in the record, which its excerpts are asked by (RETRIEVE-32). Absent from an older bridge. */
  ordinal?: number
}

/** The places one exchange's retrievals drew on, and the exchange they are asked back by. */
export interface RecalledLine {
  /** The exchange's prompt number, or `undefined` while this window has not been told it. */
  prompt: number | undefined
  used: RecalledPlace[]
}

interface Exchange {
  prompt: number | undefined
  entries: Entry[]
}

/** The transcript cut at each thing the person said, as the agent numbers its exchanges. */
function exchanges(entries: Entry[]): Exchange[] {
  const out: Exchange[] = [{ prompt: undefined, entries: [] }]
  for (const entry of entries) {
    if (entry.kind === 'user') out.push({ prompt: entry.prompt, entries: [entry] })
    else out[out.length - 1]!.entries.push(entry)
  }
  return out
}

const firstLine = (text: string): string => text.trim().split('\n', 1)[0] ?? ''

/**
 * The places drawn on, by the reply each line sits under: the last reply of each exchange whose
 * turn retrieved. A turn this window ran is read from its details; one it did not, from the record.
 */
export function recalledLines(entries: Entry[], turns: Turns, resumed: RecalledExchange[]): Map<string, RecalledLine> {
  const byPrompt = new Map(resumed.map((exchange) => [exchange.prompt, exchange.used]))
  const lines = new Map<string, RecalledLine>()
  for (const exchange of exchanges(entries)) {
    const replies = exchange.entries.filter((entry): entry is Extract<Entry, { kind: 'assistant' }> => entry.kind === 'assistant')
    const last = replies[replies.length - 1]
    if (!last) continue
    const live = replies.map((reply) => reply.turn === undefined ? undefined : turns[reply.turn]?.recalled)
      .find((used) => used?.length)
    const used = live ?? (exchange.prompt === undefined ? undefined : byPrompt.get(exchange.prompt))
    if (used?.length) lines.set(last.id, { prompt: exchange.prompt, used })
  }
  return lines
}

/**
 * Each place as a row of this transcript, once each. A place the bridge numbered is one message, so
 * two places drawn from the same row are both kept, each opening to its own words. Without a
 * number, from an older bridge, a row is kept once.
 */
export function recalledRows(entries: Entry[], used: RecalledPlace[]): RecalledRow[] {
  const all = exchanges(entries)
  const rows: RecalledRow[] = []
  const seen = new Set<string>()
  for (const place of used) {
    const found = resolve(all, place)
    const row = place.ordinal === undefined ? found : { ...found, ordinal: place.ordinal }
    const key = row.ordinal !== undefined ? `n${row.ordinal}` : row.id === null ? null : `id${row.id}`
    if (key !== null && seen.has(key)) continue
    if (key !== null) seen.add(key)
    rows.push(row)
  }
  return rows
}

function resolve(all: Exchange[], place: RecalledPlace): RecalledRow {
  const exchange = place.prompt === null ? undefined : all.find((each) => each.prompt === place.prompt)
  const prompt = exchange?.entries[0]
  if (!exchange || prompt?.kind !== 'user') return { id: null, who: 'missing', line: '' }
  const near: RecalledRow = { id: prompt.id, who: 'exchange', line: firstLine(prompt.text) }
  if (place.said === 'prompt') return { id: prompt.id, who: 'you', line: firstLine(prompt.text) }
  if (place.said === 'reply' && place.nth !== null) {
    const reply = exchange.entries.filter((entry) => entry.kind === 'assistant' && entry.text.trim())[place.nth]
    return reply?.kind === 'assistant' ? { id: reply.id, who: 'reply', line: firstLine(reply.text) } : near
  }
  if (place.said === 'tool' && place.nth !== null) {
    const call = exchange.entries.filter((entry) => entry.kind === 'tool' || entry.kind === 'replayed-tool')[place.nth]
    if (call?.kind === 'tool') return { id: call.id, who: 'tool', line: firstLine(`${call.activity.verb} ${call.activity.shownTarget}`) }
    if (call?.kind === 'replayed-tool') return { id: call.id, who: 'tool', line: firstLine(call.text) }
  }
  return near
}
