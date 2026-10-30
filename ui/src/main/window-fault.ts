/**
 * The main process's half of the window fault log (DIAG-10): what the page says about itself and
 * what Electron says about a process that is gone, forwarded to the agent, which writes the line.
 *
 * Only the five fields the agent reads are forwarded, so a message, a stack or anything else the
 * page adds stays here. The agent matches each against its own list of words and writes `other`
 * for the rest; this module holds no list of its own.
 */
import type { Ask } from './bug-report'

/** Most reports forwarded in one run, so a page that fails in a loop cannot fill the log. */
export const REPORT_LIMIT = 20

/** Longest string forwarded. Nothing the agent recognises is longer. */
const LONGEST = 64

/** The strings a report may carry besides its kind. */
const WORDS = ['name', 'reason', 'process'] as const

/**
 * A function that forwards a report and says nothing back.
 *
 * Anything that is not an object with a text `kind` is dropped, as is every report past the limit.
 * A bridge that is not running, or that refuses, is not a reason for the window to hear about it.
 */
export function faultReporter(ask: Ask): (report: unknown) => void {
  let sent = 0
  return (report) => {
    if (typeof report !== 'object' || report === null || sent >= REPORT_LIMIT) return
    const given = report as Record<string, unknown>
    if (typeof given.kind !== 'string') return
    sent += 1
    const params: Record<string, unknown> = { kind: given.kind.slice(0, LONGEST) }
    for (const key of WORDS) {
      const value = given[key]
      if (typeof value === 'string') params[key] = value.slice(0, LONGEST)
    }
    // Electron reports an exit code that is a count; a negative one is not.
    if (typeof given.exit === 'number' && Number.isInteger(given.exit) && given.exit >= 0) params.exit = given.exit
    void Promise.resolve().then(() => ask('window.fault', params)).catch(() => undefined)
  }
}
