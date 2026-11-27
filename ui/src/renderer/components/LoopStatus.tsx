import { memo, useEffect, useState } from 'react'
import { Icon } from '../nala'
import { LOOPS_CHANGED, loopStatusWords, type LoopList } from '../../shared/loop'

/** How often the session's loops are read again where nothing has said they changed. */
const POLL_MS = 2000

/**
 * The row beside the composer that says a session's loops are live (LOOP-13): how many, and how
 * long until the soonest tick while that moment is ahead. It reads `loops.list` at once whenever a
 * loop is answered, ticks, ends or is armed, and every two seconds besides, and redraws every
 * second between reads, so it counts down with no input. A session with no loops draws nothing.
 *
 * `busy` is whether the session has a turn running or lines queued, which is what a line `/after`
 * holds waits for once its moment has passed.
 */
export const LoopStatus = memo(function LoopStatus({ session, busy }: { session: string; busy: boolean }): React.JSX.Element | null {
  const [read, setRead] = useState<{ list: LoopList; at: number } | null>(null)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    setRead(null)
    let gone = false
    const ask = async () => {
      try {
        const response = await window.bravebot.request<LoopList>('loops.list', { session })
        // A failed read leaves the last answer in place; the next one replaces it.
        if (!gone && response.ok) {
          const at = Date.now()
          setRead({ list: response.ok, at })
          setNow(at)
        }
      } catch {
        // As above: the row keeps what it last read.
      }
    }
    void ask()
    const timer = setInterval(() => void ask(), POLL_MS)
    const unsubscribe = window.bravebot.onEvent((message) => {
      if (message.session === session && message.event.startsWith('loop.')) void ask()
    })
    const answered = (event: Event) => {
      if ((event as CustomEvent<string>).detail === session) void ask()
    }
    window.addEventListener(LOOPS_CHANGED, answered)
    return () => {
      gone = true
      clearInterval(timer)
      unsubscribe()
      window.removeEventListener(LOOPS_CHANGED, answered)
    }
  }, [session])
  const live = (read?.list.loops.length ?? 0) > 0
  useEffect(() => {
    if (!live) return
    const tick = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(tick)
  }, [live])
  if (!read) return null
  return <LoopStatusRow list={read.list} elapsed={Math.max(0, Math.floor((now - read.at) / 1000))} busy={busy} />
})

/** The row itself, for `elapsed` seconds after `list` was read, or nothing where there are no loops. */
export function LoopStatusRow({ list, elapsed, busy = false }: { list: LoopList; elapsed: number; busy?: boolean }): React.JSX.Element | null {
  const words = loopStatusWords(list, elapsed, busy)
  if (!words) return null
  // Not a live region: the words change every second, and a screen reader would read each one.
  return (
    <div className="composer-tray composer-notice loop-status" data-test="loop-status">
      <Icon name="clock" />
      <span className="notice-text">{words}</span>
    </div>
  )
}
