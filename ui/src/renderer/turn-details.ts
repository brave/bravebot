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
  /** The service the session's model is served by, as the bridge named it from configuration. */
  service?: string
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

/** What the turn said about itself, which both of the events ending a turn carry. */
function noticesIn(data: Record<string, unknown>): string[] {
  return Array.isArray(data.notices) ? data.notices.filter((line): line is string => typeof line === 'string') : []
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
    const notices = noticesIn(data)
    const prior = Object.values(turns).filter((turn) => turn.turn < number && turn.status === 'complete')
      .sort((a, b) => b.turn - a.turn)[0]
    next = { ...next, status: 'complete', notices,
      noticesOpen: previous.status === 'complete' ? previous.noticesOpen : JSON.stringify(notices) !== JSON.stringify(prior?.notices),
      model: typeof data.model === 'string' && data.model ? data.model : undefined,
      // Only where the turn ran on the session's own model: another model's service is not known.
      service: data.sessionModel === true && typeof data.service === 'string' && data.service ? data.service : undefined,
      tokens: count(data.tokens), outputTokens: count(data.outputTokens), steps: count(data.steps),
      clean: typeof data.clean === 'boolean' ? data.clean : undefined }
  } else if (message.event === 'turn.error') {
    // A turn that failed produced no reply to carry these, so this event is where they arrive and
    // the only place the reader can still be told a hook of theirs went wrong.
    next = { ...next, status: 'interrupted', notices: noticesIn(data) }
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

/**
 * Whether a record is a refusal, as the agent decided it.
 *
 * Read, never re-derived. The kernel writes its own verdict beside every event, and working it
 * back out of the kind and the fields here was a second answer to a question that has one: a
 * record this build did not know to unpick would drop out of the refusal list rather than
 * appearing in it, which is the wrong direction for the screen a reviewer reads to check what
 * the agent was refused.
 *
 * Anything but an explicit `false` reads as a refusal, which is that direction: a verdict nobody
 * can read is not one to draw as though the check had passed. Every record on this channel is
 * serialised by the agent this window ships with, in the turn it is streaming, so one arriving
 * without a readable verdict is a malformed record rather than an older one. The file the agent
 * reads a resumed trail back out of is the other case and is not this: a trail written before the
 * verdict was recorded has only its shape to go on, and the agent falls back to that there.
 */
export function isRefusal(event: Record<string, unknown>): boolean {
  return event.refusal !== false
}

const text = (value: unknown): string => typeof value === 'string' ? value : ''

/**
 * Text with each control character replaced by its Unicode picture, as the terminal's
 * `printable` in `bravebot-approval` draws it (LAYER-3). A name from a checkout can hold an escape
 * or a bell; a text node drops those without a trace, and a character silently dropped is one nobody
 * can tell was ever in the file. A tab is left as it is. The recorded evidence is JSON, which
 * escapes them already.
 */
export function pictured(value: string): string {
  return Array.from(value, (c) => {
    const code = c.codePointAt(0)!
    return (code < 0x20 || (code >= 0x7f && code <= 0x9f)) && c !== '\t' ? String.fromCodePoint(0x2400 + code) : c
  }).join('')
}

/** A field of a record as the window draws it. */
const shown = (value: unknown): string => pictured(text(value))

/** Labels come from structured tags; arbitrary reasons remain the bridge's own words. */
export function auditDescription(event: Record<string, unknown>): { title: string; detail: string } {
  switch (event.kind) {
    case 'gate_blocked': return { title: 'Gate blocked', detail: shown(event.reason) || shown(event.detail) }
    case 'gate_passed': return { title: 'Gate passed', detail: shown(event.detail) }
    case 'action_field': {
      const label = event.label as { integrity?: unknown } | null
      const untrustedRouting = event.allowed === false && event.role === 'routing' && label?.integrity === 'untrusted'
      return { title: untrustedRouting ? event.field === 'path' ? 'Untrusted data used as a destination' : 'Untrusted data used to direct an action' :
        event.allowed === false ? 'Action field blocked' : event.allowed === true ? 'Action field allowed' : 'Action field checked',
      detail: [text(event.tool), text(event.field), text(event.role)].filter(Boolean).join(' · ') }
    }
    case 'observed': return { title: 'Data observed', detail: text(event.capability) }
    case 'slot_written': return { title: 'Slot written', detail: text(event.slot) }
    case 'slot_deferred': return { title: 'File reserved for later reading', detail: shown(event.origin) || shown(event.slot) }
    case 'declassified': return { title: 'Data authorized for release', detail: shown(event.reason) }
    default: return { title: 'Audit event', detail: text(event.kind) }
  }
}

/** The action fields a tool's call was checked on, in the order recorded. */
export interface FieldGroup {
  tool: string
  records: AuditRecord[]
}

/**
 * The records that say where an effect went and what it carried: each run of `action_field`
 * records for one tool, and each release, in the order recorded.
 *
 * A run rather than every record for the tool: the trail carries no call id, so two calls of one
 * tool with another in between are drawn as two groups rather than merged into one.
 */
export function routingAndContent(audit: AuditRecord[]): (FieldGroup | AuditRecord)[] {
  const rows: (FieldGroup | AuditRecord)[] = []
  for (const record of audit) {
    if (record.event.kind === 'declassified') rows.push(record)
    else if (record.event.kind === 'action_field') {
      const tool = text(record.event.tool)
      const last = rows.at(-1)
      if (last && 'tool' in last && last.tool === tool) last.records.push(record)
      else rows.push({ tool, records: [record] })
    }
  }
  return rows
}
