import { useCallback, type MutableRefObject, type SetStateAction } from 'react'
import { conversationPreferences, setConversation } from './experience'
import { begun, forgotten, refusal, restored, settled, type Enhanced } from './enhancing'
import { type Live, liveKey } from './live'
import { call } from './rpc'
import { named } from './staging'

/** What `useEnhance` takes from the component that owns the sessions. */
export interface EnhanceInputs {
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
}

/** Making the message in the box more specific, and putting back what the person wrote (INPUT-50). */
export function useEnhance({ openedLives, updateSession, setProblem }: EnhanceInputs) {
  /** The box's draft for `live`, which is kept per conversation and not in the session. */
  const draftOf = (live: Live): string => conversationPreferences(liveKey(live)).draft

  /** Ask the bridge to rewrite the box. The answer arrives as `enhance.done`, taken by [`receive`]. */
  const enhance = useCallback(async (handle: string) => {
    const live = openedLives.current.get(handle)
    if (!live || live.enhancement?.asking) return
    const draft = draftOf(live)
    const why = refusal(draft, live.running, named(live.staging, draft).length > 0)
    if (why) {
      updateSession(handle, (old) => (old ? { ...old, enhancement: { ...old.enhancement, note: why } } : old))
      return
    }
    updateSession(handle, (old) => (old ? { ...old, enhancement: begun(old.enhancement ?? {}, draft) } : old))
    try {
      await call('session.enhance', { session: handle, draft, model: live.model ?? null })
    } catch (error) {
      updateSession(handle, (old) => (old ? { ...old, enhancement: { ...old.enhancement, asking: null } } : old))
      setProblem(String(error))
    }
  }, [])

  /** Take the bridge's answer: put the rewrite in the box where the line it was asked for still is. */
  const receive = useCallback((handle: string, data: Enhanced) => {
    const live = openedLives.current.get(handle)
    if (!live) return
    const key = liveKey(live)
    const { held, write } = settled(live.enhancement ?? {}, data, conversationPreferences(key).draft)
    if (write !== null) setConversation(key, { draft: write })
    updateSession(handle, (old) => (old ? { ...old, enhancement: held } : old))
  }, [])

  /** Stop the request that is out. The bridge answers it with `stopped`. */
  const stop = useCallback((handle: string) => {
    void call('session.enhance.cancel', { session: handle }).catch((error) => setProblem(String(error)))
  }, [])

  /** Escape over a rewrite: put back what the person wrote. Says whether there was one to put back. */
  const restore = useCallback((handle: string): boolean => {
    const live = openedLives.current.get(handle)
    const back = live ? restored(live.enhancement ?? {}) : null
    if (!live || !back) return false
    setConversation(liveKey(live), { draft: back.write })
    updateSession(handle, (old) => (old ? { ...old, enhancement: back.held } : old))
    return true
  }, [])

  /** Dismiss the note, and end a rewrite whose box has since been emptied. */
  const dismiss = useCallback((handle: string) => {
    const live = openedLives.current.get(handle)
    const draft = live ? draftOf(live) : ''
    updateSession(handle, (old) => (old ? { ...old, enhancement: { ...forgotten(old.enhancement ?? {}, draft), note: null } } : old))
  }, [])

  return { enhance, receive, stop, restore, dismiss }
}
