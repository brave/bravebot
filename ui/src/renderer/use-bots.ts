import { useCallback, useEffect, useRef, useState, type MutableRefObject, type SetStateAction } from 'react'
import type { Bot } from '../shared/bots'
import type { BotFormValue } from './components/Bots'
import type { Live } from './live'

/** What `useBots` takes from the component that owns the sessions. */
export interface BotsInputs {
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  setLive: (action: SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
}

/** The bot list on screen, and the edits that change it that need no session to be closed. */
export function useBots({ openedLives, updateSession, setLive, setProblem }: BotsInputs) {
  const [bots, setBots] = useState<Bot[]>([])

  /**
   * Read the bot list again.
   *
   * The main process owns it, and writes to it that this window did not make: a bot's session id
   * and its compaction watermark are taken off what the agent answered. So this is asked for after
   * a turn as well as after an edit — the copy on screen is a copy.
   */
  const readBots = useCallback(async () => {
    setBots(await window.bravebot.readBots().catch(() => []))
  }, [])

  // Read inside the event listener and inside `send`, both installed once and neither able to
  // close over a list that changes under them.
  const botsRef = useRef<Bot[]>(bots)
  botsRef.current = bots

  useEffect(() => {
    const edited = (event: Event) => {
      const slug = (event as CustomEvent<string>).detail
      for (const session of openedLives.current.values()) {
        if (session.bot?.slug === slug) updateSession(session.handle, (old) => old?.bot ? { ...old, bot: { ...old.bot, grounded: false } } : old)
      }
    }
    document.addEventListener('bravebot:memory-edited', edited)
    return () => document.removeEventListener('bravebot:memory-edited', edited)
  }, [openedLives, updateSession])

  const saveBot = useCallback(
    async (bot: BotFormValue) => {
      try {
        const saved = await window.bravebot.writeBot(bot)
        if (!saved) throw new Error('Could not save bot.')
        setLive((old) => old?.bot?.slug === saved.slug && !old.running
          ? { ...old, model: saved.model ?? old.model } : old)
        await readBots()
        return true
      } catch (error) { setProblem(String(error)); return false }
    },
    [readBots, setLive, setProblem],
  )

  /**
   * Take a bot away for good.
   *
   * No session to let go of, unlike `retireBot`. This is only ever reached from a row in
   * the archive, and archiving is what put it there — which closed its session on the way past.
   * A bot that is deletable is therefore a bot that is not on screen, by construction rather than
   * by a check here.
   */
  const removeBot = useCallback(
    async (slug: string) => {
      try {
        await window.bravebot.removeBot(slug)
        await readBots()
      } catch (error) { setProblem(String(error)) }
    },
    [readBots, setProblem],
  )

  return { bots, botsRef, readBots, saveBot, removeBot }
}
