/**
 * `/goal` typed into the composer (CMD-1, GOAL-1): the condition a chat works towards until a
 * judge with no tools says it holds. The bridge holds the goal and decides every step
 * (crates/ui-bridge/src/goal.rs); the window sends the line, draws what comes back, and asks for
 * the next step once the prompts the person queued have gone.
 */

/** The goal as the bridge sends it in `goal.changed`, in `session.goal`'s answer and in `goal.judged`. */
export interface GoalSnapshot {
  condition: string
  /** The rounds judged not met so far. */
  rounds: number
  /** The rounds the goal may still send the work back for (GOAL-9). */
  left: number
  paused: boolean
  /** Rounds in a row made no tool call, so the goal stays set and sends nothing more (GOAL-20). */
  stalled: boolean
  lastReason: string | null
  elapsedSeconds: number
  spentTokens: number
}

/** What `session.goal` answers: the goal as it now stands, whether it changed, and the lines to show. */
export interface GoalAnswer {
  goal: GoalSnapshot | null
  changed: boolean
  lines: string[]
}

/** What `session.goal.continue` answers: a turn that carries the work on, a check, or nothing. */
export interface GoalContinued {
  started: 'turn' | 'check' | false
  turn?: number
  target?: string
}

/** What a check of the goal came to, as the bridge sends it in `goal.judged` (GOAL-6). */
export interface GoalJudged {
  verdict: 'met' | 'not-met' | 'impossible' | 'unreadable' | 'quarantined' | 'failed'
  outcome: 'carry-on' | 'met' | 'impossible' | 'unreadable' | 'quarantined' | 'failed' | 'spent' | 'stalled'
  /** What the goal does next: send the work back, end, or stay set and send nothing more. */
  next: 'carry-on' | 'ended' | 'stalled'
  reason: string | null
  /** The bridge's sentence for the notice row. */
  line: string
  rounds: number
  left: number
  elapsedSeconds: number
  spentTokens: number
  goal: GoalSnapshot | null
}

/**
 * How a goal notice is marked: `plain` for one that sets, reports or carries the goal on, `met`
 * for a MET verdict, `warning` for a goal that ended without being met or stopped sending the work
 * back, `paused` for a paused goal, and `refused` for a check that failed.
 */
export type GoalTone = 'plain' | 'met' | 'warning' | 'paused' | 'refused'

/** The mark of a check's notice, from what the bridge says it came to. */
export function verdictTone(outcome: GoalJudged['outcome']): GoalTone {
  if (outcome === 'met') return 'met'
  if (outcome === 'failed') return 'refused'
  if (outcome === 'impossible' || outcome === 'spent' || outcome === 'stalled') return 'warning'
  return 'plain'
}

/** The mark of the notice `session.goal` answered with: paused while the goal it left is paused. */
export function answerTone(answer: GoalAnswer): GoalTone {
  return answer.goal?.paused ? 'paused' : 'plain'
}

/**
 * The whole commands the goal's sentences name, longest first, each drawn as one code span every
 * time it appears. These are the bridge's catalog phrases (desktop-goal-* in en-US.ftl).
 */
export const GOAL_COMMANDS = ['/goal cargo test exits 0', '/goal <condition>', '/goal resume', '/goal clear', '/goal pause'] as const

/**
 * The argument of a line that is `/goal` or `/goal <text>`, or null for any other line.
 *
 * Applied only to the text a person typed into the composer (CMD-1). A longer word such as `/goals`
 * is a prompt (CMD-2). `/goal` alone gives an empty argument, which the bridge answers with a
 * report of the goal.
 */
export function goalArgument(line: string): string | null {
  const rest = line.trim()
  if (rest === '/goal') return ''
  if (!rest.startsWith('/goal ')) return null
  return rest.slice('/goal '.length).trim()
}
