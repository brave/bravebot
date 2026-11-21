import { useCallback, type MutableRefObject, type SetStateAction } from 'react'
import { compactionBegun, compactionEnded, compactNotice, type Compacted } from './compacting'
import type { Live } from './live'
import { call } from './rpc'
import { RequestFailed } from './rewind'
import { showToast } from './toasts'

/** What `useCompact` takes from the component that owns the sessions. */
export interface CompactInputs {
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
}

/** Shortening a chat's conversation from the Chat menu, and saying how it ended (CMD-34). */
export function useCompact({ openedLives, updateSession, setProblem }: CompactInputs) {
  /**
   * Ask the bridge to compact the chat. Resolves true once the request is out, which is when the
   * dialog that asked for the focus closes. The ending arrives as `compact.done`, taken by [`receive`].
   */
  const compact = useCallback(async (handle: string, focus: string): Promise<boolean> => {
    const live = openedLives.current.get(handle)
    if (!live || live.running) return false
    updateSession(handle, (old) => (old ? compactionBegun(old) : old))
    try {
      await call('session.compact', { session: handle, focus: focus.trim() || null, model: live.model ?? null })
      return true
    } catch (error) {
      updateSession(handle, (old) => (old ? compactionEnded(old) : old))
      // A refusal is the bridge's sentence, and the problem says it.
      setProblem(error instanceof RequestFailed ? error.reason : String(error))
      return false
    }
  }, [])

  /** Take the bridge's answer: the chat stops running, and the ending is said. */
  const receive = useCallback((handle: string, data: Compacted) => {
    updateSession(handle, (old) => (old ? compactionEnded(old) : old))
    const notice = compactNotice(data)
    if ('toast' in notice) showToast(notice.toast.title, notice.toast.body, notice.toast.kind)
    else setProblem(notice.problem)
  }, [])

  return { compact, receive }
}
