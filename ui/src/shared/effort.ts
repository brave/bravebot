/** The levels the agent accepts for `effort`, cheapest first, as the terminal's `/effort` offers them. */
export const EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'] as const

export type Effort = (typeof EFFORTS)[number]

export function isEffort(value: unknown): value is Effort {
  return typeof value === 'string' && (EFFORTS as readonly string[]).includes(value)
}

/** Worded as the terminal's picker words them, so the two read the same. */
export const EFFORT_HINTS: Record<Effort | 'default', string> = {
  default: 'Left to whichever service answers',
  low: 'Least thinking, for simple work',
  medium: 'Less thinking, where that holds up',
  high: 'The usual amount, for work needing care',
  xhigh: 'More thinking, for code and long runs',
  max: 'The most thinking, cost aside',
}

export const EFFORT_LABELS: Record<Effort | 'default', string> = {
  default: 'Default',
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  xhigh: 'Extra high',
  max: 'Max',
}
