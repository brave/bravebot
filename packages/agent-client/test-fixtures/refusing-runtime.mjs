#!/usr/bin/env node
// A bridge stand-in that asks for a write approval and then refuses every reply to it, so a client
// is left with a question it cannot answer. On a cancel it ends the turn as cancelled, or, when
// IGNORE_CANCEL is set, accepts the cancel and does nothing. It supports nothing else.
import { createInterface } from 'node:readline'

const capability = { version: 1, start: 'session.view.start', scope: 'fresh_session', approvals: ['confirm', 'run', 'fetch', 'ask'], reconnect: false }
const send = (message) => process.stdout.write(`${JSON.stringify(message)}\n`)
const update = (sequence, status, rows, pending) =>
  send({ event: 'session.view.update', session: 's1', data: { sequence, turn: 1, target: 1, status, pending, rows } })
const question = { request: 4, path: 'a.txt' }
const asked = { id: 2, turn: 1, kind: 'approval', event: 'confirm.request', data: question, resolved: false }

createInterface({ input: process.stdin }).on('line', (line) => {
  const request = JSON.parse(line)
  switch (request.method) {
    case 'agent.info':
      return send({ id: request.id, ok: { build: 'b', version: 'v', capabilities: { sessionView: capability } } })
    case 'session.new':
      return send({ id: request.id, ok: { session: 's1' } })
    case 'session.view.start':
      send({ event: 'session.view.initial', session: 's1', data: { sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [] } })
      return send({ id: request.id, ok: {} })
    case 'turn.send':
      send({ id: request.id, ok: { turn: 1, target: 1 } })
      update(1, 'running', [{ id: 1, turn: 1, kind: 'prompt', event: null, data: { text: 'go' }, resolved: false }], null)
      return update(2, 'waiting', [asked], { row: 2, request: 4, kind: 'confirm', supported: true, data: question })
    case 'confirm.reply':
      return send({ id: request.id, error: { code: 'internal', message: 'the bridge could not apply the reply' } })
    case 'turn.cancel':
      send({ id: request.id, ok: {} })
      if (process.env.IGNORE_CANCEL) return
      return update(3, 'cancelled', [{ ...asked, resolved: true }, { id: 3, turn: 1, kind: 'error', event: 'turn.error', data: { kind: 'cancelled' }, resolved: false }], null)
    default:
      return send({ id: request.id, ok: {} })
  }
})
