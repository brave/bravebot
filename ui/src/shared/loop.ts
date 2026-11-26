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
