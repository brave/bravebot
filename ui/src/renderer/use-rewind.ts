import { useCallback, useState, type MutableRefObject, type SetStateAction } from 'react'
import type { RewoundSession } from '../shared/protocol'
import type { Live } from './live'
import { pointForPrompt, rewindFailure, rewoundLive } from './rewind'
import { call } from './rpc'
import { showToast } from './toasts'

interface Rewinding {
  handle: string
  steps: number
  busy: boolean
}

/** What `useRewind` takes from the component that owns the sessions and the composer. */
export interface RewindInputs {
  liveRef: MutableRefObject<Live | null>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  refresh: () => Promise<void>
  draft: string
  setDraft: (text: string) => void
  setProblem: (problem: string | null) => void
}

/** Asking to undo turns, and putting the session back once that is confirmed. */
export function useRewind({ liveRef, updateSession, refresh, draft, setDraft, setProblem }: RewindInputs) {
  /**
   * Which rewind is waiting on its confirmation, for which session.
   *
   * The handle is kept so a dialog asked about one session is never answered against another.
   */
  const [rewinding, setRewinding] = useState<Rewinding | null>(null)
  const askRewind = useCallback((steps: number) => {
    const current = liveRef.current
    if (!current || current.running || !current.rewind?.some((point) => point.steps === steps)) return
    setRewinding({ handle: current.handle, steps, busy: false })
  }, [liveRef])
  const rewindEntry = useCallback((id: string) => {
    const entry = liveRef.current?.entries.find((candidate) => candidate.id === id)
    const point = entry?.kind === 'user' ? pointForPrompt(liveRef.current?.rewind ?? [], entry.prompt) : null
    if (point) askRewind(point.steps)
  }, [askRewind, liveRef])

  /** Put the session back and draw it again from what the agent says it now holds. */
  const confirmRewind = useCallback(async () => {
    if (!rewinding || rewinding.busy) return
    const { handle, steps } = rewinding
    setRewinding({ ...rewinding, busy: true })
    try {
      const rewound = await call<RewoundSession>('session.rewind', { session: handle, steps })
      updateSession(handle, (old) => old ? rewoundLive(old, rewound) : old)
      const draftNow = liveRef.current?.handle === handle ? draft : ''
      setDraft(rewound.text + (draftNow.trim() ? `\n\n${draftNow}` : ''))
      showToast(`Back to before turn ${rewound.turn}`)
      setProblem(rewound.refused.length
        ? `These files still hold what the undone turns wrote, so check them before going on: ${rewound.refused.join(', ')}`
        : null)
      void refresh()
    } catch (error) {
      setProblem(rewindFailure(error))
    } finally {
      setRewinding(null)
    }
  }, [rewinding, draft, setDraft, updateSession, refresh, liveRef, setProblem])

  const cancel = useCallback(() => setRewinding(null), [])

  return { rewinding, askRewind, rewindEntry, confirmRewind, cancel }
}
