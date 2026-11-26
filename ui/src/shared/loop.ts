/** What the bridge answers for `loop.command`. */
export interface LoopAnswer {
  /** Each line already has its control characters pictured by the bridge. */
  lines: string[]
  /** True when the command ended in a refusal, as the bridge reported its outcome; it is not worked out from the text of a line. */
  refused: boolean
  /** The number of the loop the line started, or null for an answer that started none. */
  number: number | null
  /** How the loop is paced, in words, where one was started. */
  pace: string | null
  /** Set where the interval typed was held to the bounds (LOOP-7). The sentence saying so is one of `lines`. */
  adjusted: { held: 'raised' | 'capped'; every: string } | null
}

/** One loop a session holds, as `loops.list` reports it. */
export interface LoopRow {
  number: number
  /** The first line of the loop's prompt, pictured. */
  prompt: string
  pace: string
  when: string
  ticks: number
  quiet: number
  /** True for a line sent once, later (`/after`). */
  once: boolean
  /** Seconds until this loop's next tick, or null where no moment is set: a tick in flight, or a self-paced loop not yet told when. */
  until: number | null
}

/** What `loops.list` answers: a session's loops, and the seconds to the soonest tick where one is ahead. */
export interface LoopList {
  loops: LoopRow[]
  soonest: number | null
}

/** `loop.tick`: a loop's line is about to be sent as a turn (LOOP-13). */
export interface LoopTick {
  number: number
  tick: number
  quiet: number
}

/**
 * `loop.ended`: one loop ended, named by `number`, or a stop ended `count` of them (LOOP-11).
 * `message` is set for `failed`, where the tick's turn could not start.
 */
export interface LoopEnded {
  number?: number
  count?: number
  reason: 'finished' | 'silent' | 'sent' | 'stopped' | 'aged' | 'failed'
  message?: string
}

/** `loop.armed`: a turn asked to look again, and its line became a loop that ticks `after` from now (LOOP-14). */
export interface LoopArmed {
  number: number
  after: string
}

/**
 * A number of seconds in its two largest units, as the terminal spells it (`4m 12s`, `1h 5m`,
 * `0s`), from `bravebot_agent::repeat::spell`.
 */
export function spell(seconds: number): string {
  let left = Math.max(0, Math.floor(seconds))
  const parts: string[] = []
  for (const [unit, worth] of [['d', 86_400], ['h', 3_600], ['m', 60], ['s', 1]] as const) {
    const count = Math.floor(left / worth)
    if (count > 0) {
      parts.push(`${count}${unit}`)
      left -= count * worth
    }
    if (parts.length === 2) break
  }
  return parts.length ? parts.join(' ') : '0s'
}

/**
 * What the row beside the composer says about a session's loops, `elapsed` seconds after `list`
 * was read, or null where the session holds none (LOOP-13).
 *
 * The same rules as the terminal's row under the box (`crates/tui/src/render/hint.rs`). Loops are
 * counted first, and a line `/after` holds is named only where no loop is live. The time to the
 * soonest moment is added while it is at least a second ahead. A moment already passed is a tick
 * waiting for the session to be free, and a countdown sitting at `0s` would read as a stalled loop.
 */
export function loopStatusWords(list: LoopList, elapsed: number): string | null {
  const group = (once: boolean) => {
    const rows = list.loops.filter((row) => row.once === once)
    const moments = rows.flatMap((row) => row.until === null ? [] : [row.until - elapsed])
    const soonest = moments.length ? Math.min(...moments) : null
    return { count: rows.length, next: soonest !== null && soonest >= 1 ? spell(soonest) : null }
  }
  const looping = group(false)
  if (looping.count > 0) {
    const live = looping.count === 1 ? 'Loop live' : `${looping.count} loops live`
    return looping.next ? `${live} · next in ${looping.next}` : live
  }
  const later = group(true)
  if (later.count > 0) return later.next ? `Sending in ${later.next}` : 'Sending later'
  return null
}

/**
 * The argument of a line that is `/loop` or `/loop <argument>`, or null for any other line.
 *
 * Applied only to the text a person typed into the composer (CMD-1). It cuts the command word off
 * and nothing more: the bridge reads the argument. A longer word such as `/looping` is a prompt, as
 * it is in the terminal.
 */
export function loopArgument(line: string): string | null {
  const rest = line.trim()
  if (rest === '/loop') return ''
  if (!rest.startsWith('/loop ')) return null
  return rest.slice('/loop '.length).trim()
}
