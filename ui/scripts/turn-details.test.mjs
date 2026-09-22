import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/renderer/turn-details.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { receiveTurn, isRefusal, auditDescription, AUDIT_EVENTS_PER_TURN, AUDIT_BYTES_PER_TURN, AUDIT_BYTES_PER_SESSION } = module.exports
const event = (event, turn, data = {}) => ({ event, session: 's1', data: { turn, ...data } })
const done = (turn, data = {}) => event('turn.done', turn, { model: 'reported/model', tokens: 12840, outputTokens: 1620, steps: 4, clean: true, notices: ['Loaded AGENTS.md'], ...data })

test('early audit, multiple turns, and late events retain their numbered association', () => {
  let turns = receiveTurn({}, event('audit', 2, { event: { kind: 'gate_blocked', reason: 'Untrusted path' } }))
  turns = receiveTurn(turns, event('turn.started', 2))
  turns = receiveTurn(turns, done(2))
  turns = receiveTurn(turns, event('turn.started', 3))
  turns = receiveTurn(turns, event('audit', 2, { event: { kind: 'slot_written', slot: 'old' } }))
  assert.equal(turns[2].status, 'complete')
  assert.equal(turns[2].audit.length, 2)
  assert.equal(turns[3].audit.length, 0)
  assert.equal(turns[2].model, 'reported/model')
  const otherSession = receiveTurn({}, done(2, { model: 'other/model' }))
  assert.equal(otherSession[2].model, 'other/model')
  assert.equal(turns[2].model, 'reported/model')
})

test('notices preserve wording and order; only identical consecutive groups default collapsed', () => {
  let turns = receiveTurn({}, done(1))
  assert.equal(turns[1].noticesOpen, true)
  turns = receiveTurn(turns, done(2))
  assert.equal(turns[2].noticesOpen, false)
  const notices = ['Could not load skill\nfile missing', 'Loaded AGENTS.md']
  turns = receiveTurn(turns, done(3, { notices }))
  assert.equal(turns[3].noticesOpen, true)
  assert.deepEqual(turns[3].notices, notices)
  turns = { ...turns, 3: { ...turns[3], noticesOpen: false, statsOpen: true } }
  turns = receiveTurn(turns, event('audit', 3, { event: { kind: 'future_event' } }))
  assert.equal(turns[3].noticesOpen, false)
  assert.equal(turns[3].statsOpen, true)
})

test('unavailable metrics stay distinct from zero; cancellation never invents final usage', () => {
  const missing = receiveTurn({}, event('turn.done', 1))[1]
  assert.equal(missing.tokens, undefined)
  assert.equal(missing.clean, undefined)
  const zero = receiveTurn({}, done(1, { tokens: 0, outputTokens: 0, steps: 0 }))[1]
  assert.equal(zero.tokens, 0)
  assert.equal(zero.steps, 0)
  let turns = receiveTurn({}, event('turn.started', 1))
  turns = receiveTurn(turns, event('tokens', 1, { written: 90 }))
  turns = receiveTurn(turns, event('turn.error', 1, { kind: 'cancelled' }))
  assert.equal(turns[1].status, 'interrupted')
  assert.equal(turns[1].tokens, undefined)
})

test('a turn that failed keeps what it was told, having no reply to carry it', () => {
  let turns = receiveTurn({}, event('turn.started', 1))
  turns = receiveTurn(turns, event('turn.error', 1, { kind: 'chat', notices: ['Hook turn-finished could not start /usr/bin/fmt', 7] }))
  assert.equal(turns[1].status, 'interrupted')
  assert.deepEqual(turns[1].notices, ['Hook turn-finished could not start /usr/bin/fmt'])
  assert.deepEqual(receiveTurn({}, event('turn.error', 2, { kind: 'cancelled' }))[2].notices, [])
})

test('audit retention keeps whole evidence and reports both per-turn and session omissions', () => {
  let turns = {}
  for (let i = 0; i < AUDIT_EVENTS_PER_TURN + 2; i++) turns = receiveTurn(turns, event('audit', 1, { event: { kind: 'gate_passed', detail: 'ok' } }))
  assert.equal(turns[1].audit.length, AUDIT_EVENTS_PER_TURN)
  assert.equal(turns[1].omitted, 2)
  turns = receiveTurn(turns, event('audit', 2, { event: { kind: 'gate_blocked', reason: '字'.repeat(AUDIT_BYTES_PER_TURN) } }))
  assert.equal(turns[2].audit.length, 0)
  assert.equal(turns[2].omitted, 1)
  for (let turn = 3; turn < 30; turn++) turns = receiveTurn(turns, event('audit', turn, { event: { kind: 'gate_passed', detail: 'x'.repeat(200000) } }))
  assert.ok(Object.values(turns).reduce((bytes, turn) => bytes + turn.bytes, 0) <= AUDIT_BYTES_PER_SESSION)
  assert.equal(turns[3].audit.length, 0)
  assert.equal(turns[3].omitted, 1)
})

test('structured decisions explain refusals without interpreting arbitrary prose as authority', () => {
  const routing = { kind: 'action_field', allowed: false, refusal: true, role: 'routing', label: { integrity: 'untrusted', confidentiality: 'public' }, tool: 'write_file', field: 'path' }
  assert.equal(isRefusal(routing), true)
  assert.match(auditDescription(routing).title, /Untrusted data/)
  assert.doesNotMatch(auditDescription({ ...routing, role: 'content' }).title, /destination/)
  assert.deepEqual(auditDescription({ kind: 'future_event' }), { title: 'Audit event', detail: 'future_event' })
  for (const kind of ['gate_passed', 'gate_blocked', 'observed', 'slot_written', 'slot_deferred', 'declassified', 'action_field']) assert.notEqual(auditDescription({ kind }).title, 'Audit event')
})

test('the verdict decides a refusal, and an unreadable one is drawn as a refusal rather than dropped', () => {
  // Read, not re-derived: a kind this build has never heard of is a refusal when the agent
  // said so, and a shape it knows how to unpick is not one when the agent said it passed.
  assert.equal(isRefusal({ kind: 'future_event', reason: 'blocked', refusal: true }), true)
  assert.equal(isRefusal({ kind: 'gate_blocked', gate: 'route', refusal: false }), false)
  assert.equal(isRefusal({ kind: 'action_field', allowed: false, refusal: false }), false)
  // The safe direction, which the kernel and the trail reader take too. A verdict that is
  // missing or is not a boolean is a check whose answer nobody can read, and the refusal list
  // is the screen a reviewer reads to find out what the agent was refused: showing a record
  // there that need not have been is recoverable, leaving one out is not.
  assert.equal(isRefusal({ kind: 'action_field', allowed: true }), true)
  assert.equal(isRefusal({ kind: 'action_field', allowed: false, refusal: 'false' }), true)
  assert.equal(isRefusal({ kind: 'gate_passed', detail: 'ok', refusal: null }), true)
})
