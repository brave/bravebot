/** What a bot's Recall block says, from how far its vectors have caught up (RETRIEVE-29). */
import type { RetrievalStatus } from '../shared/protocol'
import { t } from './i18n'

export interface RecallLine {
  text: string
  /** How far the index has got, from 0 to 1, while it is still building. */
  progress?: number
  /** Said under a building index: what the bot does until it is done. */
  until?: string
}

const UNAVAILABLE: Record<string, () => string> = {
  key: () => t.uiRecallUnavailableKey(),
  unreachable: () => t.uiRecallUnavailableUnreachable(),
  failed: () => t.uiRecallUnavailableFailed(),
  'out of time': () => t.uiRecallUnavailableOutOfTime(),
  unconfigured: () => t.uiRecallUnconfigured(),
}

/**
 * The block's one line, or `null` where there is nothing to say: the setting is off, or nothing
 * has been asked. A last turn or indexing pass the embedding service gave nothing outranks the
 * index's progress, since neither building it nor searching it can go on without that service.
 */
export function recallLine(status: RetrievalStatus | null, fellBack: string | null | undefined): RecallLine | null {
  if (!status || status.state === 'off') return null
  if (status.state === 'untrusted') return { text: t.uiRecallNotAvailable() }
  if (status.state === 'unconfigured') return { text: t.uiRecallUnconfigured() }
  const why = fellBack && UNAVAILABLE[fellBack] ? fellBack : status.failure
  const unavailable = why ? UNAVAILABLE[why] : undefined
  if (unavailable) return why === 'key' ? { text: unavailable(), until: t.uiRecallFixKey() } : { text: unavailable() }
  const progress = status.eligible ? Math.min(1, status.indexed / status.eligible) : 0
  const counts = { indexed: status.indexed, eligible: status.eligible }
  if (status.state === 'indexing') return { text: t.uiRecallIndexing(counts), progress, until: t.uiRecallUntil() }
  if (status.state === 'rebuilding') return { text: t.uiRecallRebuilding(counts), progress, until: t.uiRecallUntil() }
  return { text: t.uiRecallReady({ count: status.indexed }) }
}

/**
 * Whether the index is still being built, and so worth asking about again shortly. Not while the
 * service is failing it: the next pass is a turn away, and the block asks again when one ends.
 */
export const building = (status: RetrievalStatus | null): boolean =>
  !status?.failure && (status?.state === 'indexing' || status?.state === 'rebuilding')
