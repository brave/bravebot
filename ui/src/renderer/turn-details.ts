import type { BridgeEvent } from '../shared/protocol'

export interface AuditRecord {
  sequence: number
  event: Record<string, unknown>
  bytes: number
}

export interface TurnDetails {
  turn: number
  status: 'running' | 'complete' | 'interrupted'
  started: boolean
  notices: string[]
  noticesOpen: boolean
  statsOpen: boolean
  model?: string
  tokens?: number
  outputTokens?: number
  steps?: number
  clean?: boolean
  audit: AuditRecord[]
  received: number
  omitted: number
  bytes: number
}

export type Turns = Record<number, TurnDetails>
export type TurnDisclosure = 'noticesOpen' | 'statsOpen'
export const AUDIT_EVENTS_PER_TURN = 1000
export const AUDIT_BYTES_PER_TURN = 256 * 1024
export const AUDIT_BYTES_PER_SESSION = 2 * 1024 * 1024

function fresh(turn: number): TurnDetails {
  return { turn, status: 'running', started: false, notices: [], noticesOpen: true,
    statsOpen: false, audit: [], received: 0, omitted: 0, bytes: 0 }
}

/** Numbers missing from older payloads are unavailable, not zero. */
function count(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined
}

/** This map belongs to one session handle. Only numbered events may change it. */
export function receiveTurn(turns: Turns, message: BridgeEvent): Turns {
  if (!['turn.started', 'turn.done', 'turn.error', 'audit'].includes(message.event)) return turns
  const data = message.data as unknown as Record<string, unknown>
  const number = count(data.turn)
  if (number === undefined) return turns
  const previous = turns[number] ?? fresh(number)
  let next = { ...previous }
  if (message.event === 'turn.started') {
    // A worker can emit audit records before its turn.started announcement reaches the UI.
    next.started = true
  } else if (message.event === 'turn.done') {
    const notices = Array.isArray(data.notices) ? data.notices.filter((line): line is string => typeof line === 'string') : []
    const prior = Object.values(turns).filter((turn) => turn.turn < number && turn.status === 'complete')
      .sort((a, b) => b.turn - a.turn)[0]
    next = { ...next, status: 'complete', notices,
      noticesOpen: previous.status === 'complete' ? previous.noticesOpen : JSON.stringify(notices) !== JSON.stringify(prior?.notices),
      model: typeof data.model === 'string' && data.model ? data.model : undefined,
      tokens: count(data.tokens), outputTokens: count(data.outputTokens), steps: count(data.steps),
      clean: typeof data.clean === 'boolean' ? data.clean : undefined }
  } else if (message.event === 'turn.error') {
    next.status = 'interrupted'
  } else {
    const event = data.event
    if (!event || typeof event !== 'object' || Array.isArray(event)) return turns
    const bytes = new TextEncoder().encode(JSON.stringify(event)).length
    next.received++
    // Retain whole records only. A partially shown decision is not reviewable evidence.
    if (next.audit.length >= AUDIT_EVENTS_PER_TURN || next.bytes + bytes > AUDIT_BYTES_PER_TURN) next.omitted++
    else {
      next.audit = [...next.audit, { sequence: next.received, event: event as Record<string, unknown>, bytes }]
      next.bytes += bytes
    }
  }
  const result = { ...turns, [number]: next }
  let bytes = Object.values(result).reduce((sum, turn) => sum + turn.bytes, 0)
  // Bound the whole session too; opening many turns must not retain unbounded evidence.
  for (const turn of Object.values(result).sort((a, b) => a.turn - b.turn)) {
    if (bytes <= AUDIT_BYTES_PER_SESSION) break
    if (!turn.bytes) continue
    bytes -= turn.bytes
    result[turn.turn] = { ...turn, audit: [], omitted: turn.omitted + turn.audit.length, bytes: 0 }
  }
  return result
}

export function isRefusal(event: Record<string, unknown>): boolean {
  return event.kind === 'gate_blocked' || (event.kind === 'action_field' && event.allowed === false)
}

const text = (value: unknown): string => typeof value === 'string' ? value : ''

/** Labels come from structured tags; arbitrary reasons remain the bridge's own words. */
export function auditDescription(event: Record<string, unknown>): { title: string; detail: string } {
  switch (event.kind) {
    case 'gate_blocked': return { title: 'Gate blocked', detail: text(event.reason) || text(event.detail) }
    case 'gate_passed': return { title: 'Gate passed', detail: text(event.detail) }
    case 'action_field': {
      const label = event.label as { integrity?: unknown } | null
      const untrustedRouting = event.allowed === false && event.role === 'routing' && label?.integrity === 'untrusted'
      return { title: untrustedRouting ? event.field === 'path' ? 'Untrusted data used as a destination' : 'Untrusted data used to direct an action' :
        event.allowed === false ? 'Action field blocked' : event.allowed === true ? 'Action field allowed' : 'Action field checked',
      detail: [text(event.tool), text(event.field), text(event.role)].filter(Boolean).join(' · ') }
    }
    case 'observed': return { title: 'Data observed', detail: text(event.capability) }
    case 'slot_written': return { title: 'Slot written', detail: text(event.slot) }
    case 'slot_deferred': return { title: 'File reserved for later reading', detail: text(event.origin) || text(event.slot) }
    case 'declassified': return { title: 'Data authorized for release', detail: text(event.reason) }
    default: return { title: 'Audit event', detail: text(event.kind) }
  }
}
