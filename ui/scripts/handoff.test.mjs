// Starting a new chat from a brief, from the desktop's Chat menu (SESSION-34, CMD-34).
//
// The brief and its refusals are the bridge's (crates/ui-bridge/tests/handing_off.rs). What the
// window owns is what it does with the answer: a ready brief is put in the box and arms the
// handoff, the draft it replaced is kept, Escape or an emptied box drops it, and Send takes it.
// The faults these reject are a brief written over the box when none was offered, the earlier
// draft lost, a handoff that survives an emptied box and turns the next ordinary message into a
// new chat, and Send taking a brief that is not armed.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => require(id), module, module.exports)
  return module.exports
}

const h = load("export * from './src/renderer/handoff'")

const BRIEF = 'We fixed the parser.\n\nNext goal: write the tests'

test('a ready brief goes in the box, arms the handoff and keeps the draft it replaced', () => {
  const asking = h.asked({}, 'write the tests')
  assert.deepEqual(asking.asking, { goal: 'write the tests' })
  const { held, write, notice } = h.received(asking, { status: 'ready', text: BRIEF }, 'half a thought')
  assert.equal(write, BRIEF)
  assert.equal(notice, null)
  assert.equal(held.asking, null)
  assert.deepEqual(held.armed, { earlier: 'half a thought' })
  assert.equal(h.armed(held, BRIEF), true)
})

test('a second brief over an armed one keeps the draft from before the first', () => {
  const first = h.received(h.asked({}, 'a'), { status: 'ready', text: 'brief one' }, 'mine').held
  const second = h.received(h.asked(first, 'b'), { status: 'ready', text: 'brief two' }, 'brief one').held
  assert.deepEqual(second.armed, { earlier: 'mine' })
})

test('untrusted, stopped and failed leave the box alone and say why', () => {
  for (const [data, check] of [
    [{ status: 'untrusted' }, (n) => assert.equal(n.toast.title, 'No brief offered')],
    [{ status: 'stopped' }, (n) => assert.equal(n.toast.title, 'Handoff stopped')],
    [{ status: 'failed', message: 'the service refused' }, (n) => assert.match(n.problem, /the service refused/)],
    // A ready answer with no text is not a brief.
    [{ status: 'ready' }, (n) => assert.ok(n.problem)],
  ]) {
    const { held, write, notice } = h.received(h.asked({}, 'goal'), data, 'mine')
    assert.equal(write, null, data.status)
    assert.ok(!held.armed, data.status)
    assert.equal(held.asking, null, data.status)
    check(notice)
  }
})

test('an answer to no request this chat made is ignored', () => {
  const { held, write, notice } = h.received({}, { status: 'ready', text: BRIEF }, 'mine')
  assert.deepEqual(held, {})
  assert.equal(write, null)
  assert.equal(notice, null)
})

test('an emptied box drops the handoff, and a box with text keeps it', () => {
  const held = { asking: null, armed: { earlier: 'mine' } }
  assert.equal(h.wiped(held, BRIEF), held)
  assert.equal(h.wiped(held, '  \n').armed, null)
  assert.equal(h.armed(held, '   '), false)
  assert.equal(h.taken(held, '   '), null)
})

test('Escape drops the handoff and puts back the earlier draft, and does nothing where none is armed', () => {
  const back = h.escaped({ asking: null, armed: { earlier: 'mine' } })
  assert.equal(back.write, 'mine')
  assert.equal(back.held.armed, null)
  assert.equal(h.escaped({}), null)
  assert.equal(h.escaped({ asking: { goal: 'g' }, armed: null }), null)
})

test('Send on an armed brief takes the text as the person left it and ends the handoff', () => {
  const held = { asking: null, armed: { earlier: 'mine' } }
  const took = h.taken(held, `  ${BRIEF} and also lint it  `)
  assert.equal(took.prompt, `${BRIEF} and also lint it`)
  assert.equal(took.held.armed, null)
  assert.equal(h.taken(null, BRIEF), null)
  assert.equal(h.taken({ asking: { goal: 'g' } }, BRIEF), null)
})

test('the chat a record was handed off from is named by its title, or by its id', () => {
  const sessions = [{ id: 'rec-1', title: 'Fix the parser' }]
  assert.deepEqual(h.sourceOf('rec-1', sessions), { id: 'rec-1', title: 'Fix the parser' })
  assert.deepEqual(h.sourceOf('rec-2', sessions), { id: 'rec-2', title: 'rec-2' })
  assert.equal(h.sourceOf(null, sessions), null)
})
