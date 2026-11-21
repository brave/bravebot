import { useCallback, type MutableRefObject, type SetStateAction } from 'react'
import { askingFirst } from './can-send'
import { conversationPreferences, setConversation } from './experience'
import { asked, escaped, received, taken, wiped, type HandedOff } from './handoff'
import { type Live, liveKey } from './live'
import { call } from './rpc'
import { RequestFailed } from './rewind'
import { showToast } from './toasts'

/** The chat a new one is handed off from, as `create` is given it. */
export interface HandingOff {
  /** The open chat's handle, which is what the bridge reads the source record from. */
  handle: string
  id: string | null
  title: string
}

/** What `useHandoff` takes from the component that owns the sessions. */
export interface HandoffInputs {
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
  /** Open a new chat in `directory`, linked to `from`. Resolves to its handle, or null where none was made. */
  create: (directory: string, from: HandingOff) => Promise<string | null>
  /** Send `prompt` in the chat `handle` as a message the person typed. */
  send: (prompt: string, handle: string) => Promise<void>
}

/** Hand Off to New Chat…: the brief, the box it is put in, and the new chat Send starts (SESSION-34, CMD-34). */
export function useHandoff({ openedLives, updateSession, setProblem, create, send }: HandoffInputs) {
  const draftOf = (live: Live): string => conversationPreferences(liveKey(live)).draft

  /**
   * Ask the bridge for a brief toward `goal`. Resolves true once the request is out, which is when
   * the dialog that asked for the goal closes. A refusal is the bridge's sentence, shown as the
   * problem, and keeps the dialog open. The answer arrives as `handoff.done`, taken by [`receive`].
   */
  const ask = useCallback(async (handle: string, goal: string): Promise<boolean> => {
    const live = openedLives.current.get(handle)
    if (!live || live.running || live.handoff?.asking) return false
    updateSession(handle, (old) => (old ? { ...old, handoff: asked(old.handoff ?? {}, goal) } : old))
    try {
      await call('session.handoff', { session: handle, goal, model: live.model ?? null })
      setProblem(null)
      return true
    } catch (error) {
      updateSession(handle, (old) => (old ? { ...old, handoff: { ...old.handoff, asking: null } } : old))
      setProblem(error instanceof RequestFailed ? error.reason : String(error))
      return false
    }
  }, [])

  /** Take the bridge's answer: a ready brief goes in the box, and any other ending is said. */
  const receive = useCallback((handle: string, data: HandedOff) => {
    const live = openedLives.current.get(handle)
    if (!live) return
    const { held, write, notice } = received(live.handoff ?? {}, data, draftOf(live))
    if (write !== null) setConversation(liveKey(live), { draft: write })
    updateSession(handle, (old) => (old ? { ...old, handoff: held } : old))
    if (notice && 'toast' in notice) showToast(notice.toast.title, notice.toast.body, notice.toast.kind)
    else if (notice) setProblem(notice.problem)
  }, [])

  /** Stop the request that is out. The bridge answers it with `stopped`. */
  const stop = useCallback((handle: string) => {
    void call('session.handoff.cancel', { session: handle }).catch((error) => setProblem(String(error)))
  }, [])

  /** Escape over a brief: drop the handoff and put back what the box held. Says whether there was one. */
  const escape = useCallback((handle: string): boolean => {
    const live = openedLives.current.get(handle)
    const back = live ? escaped(live.handoff ?? {}) : null
    if (!live || !back) return false
    setConversation(liveKey(live), { draft: back.write })
    updateSession(handle, (old) => (old ? { ...old, handoff: back.held } : old))
    return true
  }, [])

  /** Drop the handoff where the box has been emptied, and where the person dismisses the banner. */
  const drop = useCallback((handle: string, force = false) => {
    const live = openedLives.current.get(handle)
    if (!live?.handoff?.armed) return
    const held = force ? { ...live.handoff, armed: null } : wiped(live.handoff, draftOf(live))
    if (held !== live.handoff) updateSession(handle, (old) => (old ? { ...old, handoff: held } : old))
  }, [])

  /**
   * Send on an armed brief: open a new chat linked to this one and send the box's text there as
   * the first message. Resolves false where nothing is armed, so the caller sends as usual.
   *
   * Where the new chat cannot be made, the brief goes back in the box, still armed. Where the new
   * chat asks a question before its first turn, as one in a folder not yet trusted does, the text
   * waits in its box to be sent once that is answered.
   */
  const take = useCallback(async (handle: string): Promise<boolean> => {
    const source = openedLives.current.get(handle)
    if (!source) return false
    const took = taken(source.handoff, draftOf(source))
    if (!took) return false
    const armed = source.handoff?.armed ?? null
    setConversation(liveKey(source), { draft: '' })
    updateSession(handle, (old) => (old ? { ...old, handoff: took.held } : old))
    const made = await create(source.summary.directory, { handle, id: source.summary.id, title: source.summary.title })
    if (!made) {
      setConversation(liveKey(source), { draft: took.prompt })
      updateSession(handle, (old) => (old ? { ...old, handoff: { ...old.handoff, armed } } : old))
      return true
    }
    const opened = openedLives.current.get(made)
    if (!opened) return true
    if (askingFirst(opened)) setConversation(liveKey(opened), { draft: took.prompt })
    else await send(took.prompt, made)
    return true
  }, [create, send])

  return { ask, receive, stop, escape, drop, take }
}
