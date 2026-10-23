import type { JsonValue, Pending, Row } from '@brave/agent-client'

// Builders for the payload shapes the bridge sends (ui/docs/phase-0-rpc-protocol.md, section 8),
// so a test reads as the rows it shows.

export function prompt(id: number, turn: number, text: string): Row {
  return { id, turn, kind: 'prompt', event: null, data: { kind: 'user', text }, resolved: false }
}

export function narration(id: number, turn: number, text: string): Row {
  return { id, turn, kind: 'narration', event: 'narration', data: { text }, resolved: false }
}

export function activity(
  id: number,
  turn: number,
  fields: { verb: string; target: string; shownTarget?: string; why?: string; note: string | null; failed?: boolean; untrusted?: boolean },
): Row {
  return {
    id,
    turn,
    kind: 'activity',
    event: fields.note === null ? 'tool.started' : 'tool.finished',
    data: {
      verb: fields.verb,
      target: fields.target,
      shownTarget: fields.shownTarget ?? fields.target,
      why: fields.why ?? '',
      note: fields.note,
      failed: fields.failed ?? false,
      untrusted: fields.untrusted ?? false,
      waitedSeconds: null,
      changes: [],
    },
    resolved: false,
  }
}

export function approval(id: number, turn: number, kind: 'confirm' | 'run' | 'fetch' | 'ask', data: Record<string, JsonValue>, resolved = false): Row {
  return { id, turn, kind: 'approval', event: `${kind}.request`, data, resolved }
}

export function quarantined(
  id: number,
  turn: number,
  shown: { origin: string; label: string; preview: string[]; lines: number; reach?: 'not_the_planner' | 'no_model' },
): Row {
  return { id, turn, kind: 'quarantined', event: 'quarantined', data: { reach: 'not_the_planner', ...shown }, resolved: false }
}

export function reply(id: number, turn: number, text: string): Row {
  return {
    id,
    turn,
    kind: 'reply',
    event: 'turn.done',
    data: { turn, reply: text, model: 'test', steps: 1, clean: true, tokens: 0, outputTokens: 0, notices: [], id: null, archived: 0, prompt: null, rewind: [] },
    resolved: false,
  }
}

export function failure(id: number, turn: number, kind: 'chat' | 'cancelled', message: string): Row {
  return {
    id,
    turn,
    kind: 'error',
    event: 'turn.error',
    data: { turn, kind, message, category: kind === 'chat' ? 'unavailable' : 'cancelled', cutOff: null, notices: [], prompt: null, id: null },
    resolved: false,
  }
}

export function resolved(row: Row): Row {
  return { ...row, resolved: true }
}

/** The pending question a waiting approval row stands for. */
export function pendingOf(row: Row): Pending {
  const data = row.data as { request: number }
  const kind = (row.event ?? '').replace('.request', '')
  return { row: row.id, request: data.request, kind, supported: true, data: row.data }
}
