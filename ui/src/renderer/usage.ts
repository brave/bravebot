import type { SessionUsage } from '../shared/protocol'

/** Floored to the second, as the terminal's indicator draws it, so a part under one is not `0s`. */
export function elapsed(ms: number): string {
  const total = Math.floor(ms / 1000)
  if (total < 60) return `${total}s`
  return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, '0')}s`
}

export function spentLine(usage: SessionUsage): string {
  if (usage.turns === 0 && usage.tokens === 0) return 'Nothing spent yet'
  return `${usage.turns} ${usage.turns === 1 ? 'turn' : 'turns'} · ${usage.tokens.toLocaleString()} tokens`
}

/** Where the time went, naming only the parts that came to a second or more; `null` under a second. */
export function timeLine({ timing }: SessionUsage): string | null {
  if (timing.wallMs < 1000) return null
  const parts = ([[timing.inferenceMs, 'model'], [timing.toolsMs, 'tools'], [timing.stalledMs, 'waiting for you']] as const)
    .filter(([ms]) => ms >= 1000)
    .map(([ms, what]) => `${elapsed(ms)} ${what}`)
  return `${elapsed(timing.wallMs)} working${parts.length ? `: ${parts.join(', ')}` : ''}`
}

/** The last turn's cache, naming only what happened; `null` when the service reported nothing. */
export function cacheLine({ cached }: SessionUsage): string | null {
  if (!cached) return null
  const parts = ([[cached.read, 'read'], [cached.written, 'written']] as const)
    .filter(([count]) => count > 0)
    .map(([count, what]) => `${count.toLocaleString()} ${what}`)
  return parts.length ? `Last turn’s cache: ${parts.join(', ')}` : null
}

export interface TurnRow {
  label: string
  tokens: string
  /** Of the session total, floored as `/cost` floors it; `null` when the total is nothing. */
  share: number | null
}

/** One row per turn, then what the total holds that no turn accounts for, so the rows add up. */
export function turnRows(usage: SessionUsage): TurnRow[] {
  const share = (tokens: number) => (usage.tokens > 0 ? Math.floor((tokens * 100) / usage.tokens) : null)
  const rows: TurnRow[] = usage.byTurn.map(({ turn, tokens }) => ({
    label: turn === 0 ? 'Before the first turn' : `Turn ${turn}`,
    tokens: tokens.toLocaleString(),
    share: share(tokens),
  }))
  if (usage.unattributed > 0) {
    rows.push({ label: 'Not attributed to a turn', tokens: usage.unattributed.toLocaleString(), share: share(usage.unattributed) })
  }
  return rows
}
