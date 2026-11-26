import { memo, useEffect, useState } from 'react'
import { Icon } from '../nala'
import { loopStatusWords, type LoopList } from '../../shared/loop'

/** How often the session's loops are read again. A start, a tick or an ending shows within this. */
const POLL_MS = 2000

/**
 * The row beside the composer that says a session's loops are live (LOOP-13): how many, and how
 * long until the soonest tick while that moment is ahead. It reads `loops.list` every two seconds
 * and redraws every second between reads, so it counts down with no input. A session with no loops
 * draws nothing.
 */
export const LoopStatus = memo(function LoopStatus({ session }: { session: string }): React.JSX.Element | null {
  const [read, setRead] = useState<{ list: LoopList; at: number } | null>(null)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    setRead(null)
    let gone = false
    const ask = async () => {
      try {
        const response = await window.bravebot.request<LoopList>('loops.list', { session })
        // A failed read leaves the last answer in place; the next one replaces it.
        if (!gone && response.ok) setRead({ list: response.ok, at: Date.now() })
      } catch {
        // As above: the row keeps what it last read.
      }
    }
    void ask()
    const timer = setInterval(() => void ask(), POLL_MS)
    return () => { gone = true; clearInterval(timer) }
  }, [session])
  const live = (read?.list.loops.length ?? 0) > 0
  useEffect(() => {
    if (!live) return
    const tick = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(tick)
  }, [live])
  if (!read) return null
  return <LoopStatusRow list={read.list} elapsed={Math.max(0, Math.floor((now - read.at) / 1000))} />
})

/** The row itself, for `elapsed` seconds after `list` was read, or nothing where there are no loops. */
export function LoopStatusRow({ list, elapsed }: { list: LoopList; elapsed: number }): React.JSX.Element | null {
  const words = loopStatusWords(list, elapsed)
  if (!words) return null
  // Not a live region: the words change every second, and a screen reader would read each one.
  return (
    <div className="composer-tray composer-notice loop-status" data-test="loop-status">
      <Icon name="clock" />
      <span className="notice-text">{words}</span>
    </div>
  )
}
