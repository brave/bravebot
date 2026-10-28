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

test('the service is kept only from a turn that ran on the session model', () => {
  assert.equal(receiveTurn({}, done(1, { service: 'Brave', sessionModel: true }))[1].service, 'Brave')
  for (const data of [{ service: 'Brave', sessionModel: false }, { service: 'Brave' }, { service: null, sessionModel: true },
    { service: '', sessionModel: true }, { service: 7, sessionModel: true }, { sessionModel: 'true', service: 'Brave' }]) {
    assert.equal(receiveTurn({}, done(1, data))[1].service, undefined, JSON.stringify(data))
  }
})

test('a record is drawn from the words the bridge pictured, and the renderer holds no definition of a control character', () => {
  const name = 'x\u001b]0;PWNED-TITLE\u0007\u001b[31mred.txt'
  const sent = 'x\u241b]0;PWNED-TITLE\u2407\u241b[31mred.txt'
  assert.equal(auditDescription({ kind: 'slot_deferred', slot: 'ref:1', origin: name }, { slot: 'ref:1', origin: sent }).detail, sent)
  assert.equal(auditDescription({ kind: 'gate_passed', detail: `read_file: ${name}` }, { detail: `read_file: ${sent}` }).detail, `read_file: ${sent}`)
  assert.equal(auditDescription({ kind: 'gate_blocked', reason: `${name}\nx`, detail: 'd' }, { reason: `${sent}\u240ax`, detail: 'd' }).detail, `${sent}\u240ax`)
  assert.equal(auditDescription({ kind: 'declassified', reason: name }, { reason: sent }).detail, sent)
  // Words the bridge sent are drawn as they came, whatever they hold: the renderer does not picture.
  assert.equal(auditDescription({ kind: 'gate_passed', detail: 'a' }, { detail: 'a\u001bb' }).detail, 'a\u001bb')
  // A field with no words from the bridge is drawn as nothing, never as the raw text a page would drop characters from.
  for (const event of [{ kind: 'slot_deferred', slot: 'ref:1', origin: name }, { kind: 'gate_passed', detail: name },
    { kind: 'gate_blocked', reason: name }, { kind: 'declassified', reason: name }]) assert.equal(auditDescription(event).detail, '')
  assert.equal(module.exports.pictured, undefined)
})

test('the words the bridge pictured are kept with the record, and anything that is not text is dropped', () => {
  const turns = receiveTurn({}, event('audit', 1, { event: { kind: 'gate_passed', detail: 'a\u001b' }, pictured: { detail: 'a\u241b', gate: 7, nested: { x: 1 } } }))
  assert.deepEqual(turns[1].audit[0].pictured, { detail: 'a\u241b' })
  for (const pictured of [undefined, null, 'text', ['a']]) {
    assert.deepEqual(receiveTurn({}, event('audit', 1, { event: { kind: 'gate_passed' }, pictured }))[1].audit[0].pictured, {})
  }
})

test('routing and content groups each run of one tool’s fields and keeps releases in order', () => {
  const { routingAndContent } = module.exports
  const field = (tool, name, role, allowed = true) => ({ kind: 'action_field', tool, field: name, role, allowed, refusal: !allowed,
    label: { integrity: 'trusted', confidentiality: 'public' } })
  let turns = {}
  for (const recorded of [
    field('write_file', 'path', 'routing'), { kind: 'gate_passed', gate: 'g', refusal: false }, field('write_file', 'content', 'content'),
    field('fetch_url', 'url', 'routing', false),
    { kind: 'declassified', slot: 's2', from: { integrity: 'untrusted', confidentiality: 'private' }, to: { integrity: 'untrusted', confidentiality: 'public' }, refusal: false },
    field('write_file', 'path', 'routing'),
  ]) turns = receiveTurn(turns, event('audit', 1, { event: recorded }))
  const rows = routingAndContent(turns[1].audit)
  assert.deepEqual(rows.map((row) => 'tool' in row ? [row.tool, row.records.map((r) => r.event.field)] : ['released', row.event.slot]), [
    ['write_file', ['path', 'content']],
    ['fetch_url', ['url']],
    ['released', 's2'],
    ['write_file', ['path']],
  ])
  assert.deepEqual(rows[0].records.map((r) => r.sequence), [1, 3])
  assert.deepEqual(routingAndContent([]), [])
})
