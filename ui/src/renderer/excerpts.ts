/**
 * The words a recalled row opens to (RETRIEVE-32).
 *
 * They never ride an event: a row asks the bridge for its exchange's sends the first time one of
 * them is opened, and every row of that exchange reads the same answer. An answer is kept until
 * the record it was read from can have changed, which is when a turn ends, the session is put
 * back, or another session is opened. A failed read is not kept, so Retry asks again.
 */
import type { RetrievalExcerpts, RetrievalSend, SentExcerpt } from '../shared/protocol'

export interface ExcerptsKey {
  directory: string
  id: string
  prompt: number
}

/** What a row draws: each block one of its exchange's sends carried for its message. */
export interface ExcerptBlock {
  send: Pick<RetrievalSend, 'label' | 'refreshed' | 'replacedInTurn'>
  excerpt: SentExcerpt
}

type Ask = (key: ExcerptsKey) => Promise<unknown>

const kept = new Map<string, Promise<RetrievalExcerpts>>()

const named = (key: ExcerptsKey): string => `${key.directory}\u0000${key.id}\u0000${key.prompt}`

const ask: Ask = async (key) => {
  const answer = await window.bravebot.request<RetrievalExcerpts>('retrieval.excerpts', { ...key })
  if (answer.error || answer.ok === undefined) throw new Error(answer.error?.message ?? 'no answer')
  return answer.ok
}

/** The exchange's sends, read once and shared by its rows until `forgetExcerpts`. */
export function readExcerpts(key: ExcerptsKey, request: Ask = ask): Promise<RetrievalExcerpts> {
  const name = named(key)
  const known = kept.get(name)
  if (known) return known
  const reading = request(key).then((value) => {
    const read = excerptsIn(value)
    if (!read) throw new Error('unreadable answer')
    return read
  })
  kept.set(name, reading)
  reading.catch(() => { if (kept.get(name) === reading) kept.delete(name) })
  return reading
}

/** Drop every kept answer, because the record they were read from may have moved on. */
export function forgetExcerpts(): void {
  kept.clear()
}

/** The blocks sent for one message, in the order its exchange sent them. */
export function blocksFor(read: RetrievalExcerpts, ordinal: number): ExcerptBlock[] {
  return read.sends.flatMap((send) => send.excerpts
    .filter((excerpt) => excerpt.ordinal === ordinal)
    .map((excerpt) => ({ send: { label: send.label, refreshed: send.refreshed, replacedInTurn: send.replacedInTurn }, excerpt })))
}

const whole = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0

/** The bridge's answer as this build reads it, an excerpt it cannot read left out, or `undefined` if none of it is readable. */
export function excerptsIn(value: unknown): RetrievalExcerpts | undefined {
  if (!value || typeof value !== 'object') return undefined
  const { sends, contextBytes } = value as Record<string, unknown>
  if (!Array.isArray(sends) || !whole(contextBytes)) return undefined
  return {
    contextBytes,
    sends: sends.flatMap((send: unknown): RetrievalSend[] => {
      if (!send || typeof send !== 'object') return []
      const { label, refreshed, replacedInTurn, model, excerpts } = send as Record<string, unknown>
      if (!Array.isArray(excerpts)) return []
      return [{
        label: typeof label === 'string' ? label : '',
        refreshed: refreshed === true,
        replacedInTurn: replacedInTurn === true,
        model: typeof model === 'string' ? model : '',
        excerpts: excerpts.flatMap((excerpt: unknown): SentExcerpt[] => {
          if (!excerpt || typeof excerpt !== 'object') return []
          const { ordinal, chunk, from, context, text } = excerpt as Record<string, unknown>
          if (!whole(ordinal) || !whole(chunk) || typeof from !== 'string' || typeof text !== 'string') return []
          return [{ ordinal, chunk, from, context: context === true, text }]
        }),
      }]
    }),
  }
}
