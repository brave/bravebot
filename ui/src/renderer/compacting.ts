/**
 * What the window says when a Compact Conversation… request ends (CMD-34).
 *
 * The request and its four endings are the bridge's (`crates/ui-bridge/src/compaction.rs`), the same
 * four the terminal's `/compact` tells apart. This turns one `compact.done` into what is shown: a
 * toast for an ending that changed nothing or shortened the conversation, and a problem for a
 * failure, since failures are not toasts.
 */
import type { EventMap } from '../shared/protocol'
import type { Live } from './live'

export type Compacted = EventMap['compact.done']

export type CompactNotice =
  | { toast: { title: string; body?: string; kind: 'done' | 'note' } }
  | { problem: string }

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`

export function compactNotice(data: Compacted): CompactNotice {
  switch (data.status) {
    case 'compacted':
      return {
        toast: {
          title: 'Conversation compacted',
          body: `Summarised ${plural(data.summarised ?? 0, 'earlier message')}, kept the last ${data.kept ?? 0} as they are`,
          kind: 'done',
        },
      }
    case 'nothing':
      return { toast: { title: 'Nothing to compact', body: 'There is nothing to summarise yet', kind: 'note' } }
    case 'stopped':
      return { toast: { title: 'Compaction stopped', body: 'The conversation is as it was', kind: 'note' } }
    case 'failed':
      return { problem: `The conversation could not be summarised: ${data.message ?? 'no reason was given'}` }
  }
}

/**
 * The chat while its compaction runs. It is drawn as running, so the working row shows the
 * `compacting` phase the bridge sends, the menu greys what a running turn greys, and Cancel Turn is
 * offered and stops it.
 */
export function compactionBegun(live: Live): Live {
  return { ...live, running: true, compacting: true, phase: 'compacting', tokens: 0, thinking: '' }
}

/** The chat once its compaction has ended, whichever way it ended. */
export function compactionEnded(live: Live): Live {
  return { ...live, running: false, compacting: false, phase: null, hook: null }
}
