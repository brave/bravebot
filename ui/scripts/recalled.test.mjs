// The rows of "Used N earlier messages", drawn from the window's own transcript (RETRIEVE-27).
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { recalledLines, recalledRows } = load('src/renderer/recalled.ts')
const { receiveTurn } = load('src/renderer/turn-details.ts')
const { readExcerpts, forgetExcerpts, blocksFor, excerptsIn } = load('src/renderer/excerpts.ts')

const entries = [
  { kind: 'user', id: 'u0', text: 'Plan the launch\nfor Tuesday', prompt: 0 },
  { kind: 'replayed-tool', id: 't0', text: 'read_file notes.md', why: '' },
  { kind: 'assistant', id: 'a0', text: '' },
  { kind: 'assistant', id: 'a1', text: 'Tuesday works.' },
  { kind: 'user', id: 'u1', text: 'Hold until the migration is verified', prompt: 1 },
  { kind: 'tool', id: 't1', activity: { verb: 'Ran', shownTarget: 'migrate --check' } },
  { kind: 'assistant', id: 'a2', text: 'Holding.' },
  { kind: 'user', id: 'u2', text: 'What did I say about the launch?', prompt: 2 },
  { kind: 'assistant', id: 'a3', text: 'You said to hold it.', turn: 4 },
]

test('each place is drawn from the row it names, and a place no row holds is drawn as missing', () => {
  const rows = recalledRows(entries, [
    { prompt: 0, said: 'prompt', nth: null },
    { prompt: 0, said: 'reply', nth: 0 },
    { prompt: 0, said: 'tool', nth: 0 },
    { prompt: 1, said: 'tool', nth: 0 },
    { prompt: 1, said: 'tool', nth: null },
    { prompt: 7, said: 'prompt', nth: null },
  ])
  assert.deepEqual(rows, [
    { id: 'u0', who: 'you', line: 'Plan the launch' },
    { id: 'a1', who: 'reply', line: 'Tuesday works.' },
    { id: 't0', who: 'tool', line: 'read_file notes.md' },
    { id: 't1', who: 'tool', line: 'Ran migrate --check' },
    { id: 'u1', who: 'exchange', line: 'Hold until the migration is verified' },
    { id: null, who: 'missing', line: '' },
  ])
})

test('a line sits under the last reply of the exchange that retrieved, live or resumed', () => {
  let turns = receiveTurn({}, { event: 'turn.retrieved', session: 's', data: { turn: 4, used: [{ prompt: 1, said: 'prompt', nth: null }], tokens: 30, fellBack: null } })
  turns = receiveTurn(turns, { event: 'turn.retrieved', session: 's', data: { turn: 4, used: [{ prompt: 1, said: 'prompt', nth: null }, { prompt: 0, said: 'reply', nth: 0 }, { said: 'nonsense' }], tokens: 60, fellBack: null } })
  assert.equal(turns[4].recalled.length, 2, 'a row this build cannot read is left out')
  assert.equal(turns[4].recalledOpen, false)
  const resumed = [{ prompt: 1, used: [{ prompt: 0, said: 'prompt', nth: null }] }]
  const lines = recalledLines(entries, turns, resumed)
  assert.deepEqual([...lines.keys()], ['a2', 'a3'])
  assert.equal(lines.get('a3').used.length, 2)
  assert.equal(lines.get('a3').prompt, 2, 'the exchange is asked back by its prompt number')
  assert.equal(lines.get('a2').prompt, 1)
})

test('a fallback names nothing, so it draws no line', () => {
  const turns = receiveTurn({}, { event: 'turn.retrieved', session: 's', data: { turn: 4, used: [], tokens: 0, fellBack: 'not_ready' } })
  assert.equal(turns[4].recalled, undefined)
  assert.equal(recalledLines(entries, turns, []).size, 0)
})

// RETRIEVE-32: a numbered place is one message, so rows are kept by that number.
test('numbered places are kept by their number, so two drawn from one row are both kept', () => {
  const rows = recalledRows(entries, [
    { prompt: 0, said: 'prompt', nth: null, ordinal: 1 },
    { prompt: 0, said: 'tool', nth: null, ordinal: 2 },
    { prompt: 0, said: 'tool', nth: null, ordinal: 3 },
    { prompt: 0, said: 'tool', nth: null, ordinal: 3 },
    { prompt: 7, said: 'prompt', nth: null, ordinal: 40 },
  ])
  assert.deepEqual(rows, [
    { id: 'u0', who: 'you', line: 'Plan the launch', ordinal: 1 },
    { id: 'u0', who: 'exchange', line: 'Plan the launch', ordinal: 2 },
    { id: 'u0', who: 'exchange', line: 'Plan the launch', ordinal: 3 },
    { id: null, who: 'missing', line: '', ordinal: 40 },
  ])
})

test('without a number, from an older bridge, a row is kept once', () => {
  const rows = recalledRows(entries, [
    { prompt: 0, said: 'tool', nth: null },
    { prompt: 0, said: 'prompt', nth: null },
  ])
  assert.deepEqual(rows, [{ id: 'u0', who: 'exchange', line: 'Plan the launch' }])
})

test('a place reads its number when the bridge sends one and goes without it otherwise', () => {
  const turns = receiveTurn({}, { event: 'turn.retrieved', session: 's', data: { turn: 4, used: [
    { prompt: 1, said: 'prompt', nth: null, ordinal: 6 },
    { prompt: 0, said: 'reply', nth: 0 },
    { prompt: 0, said: 'reply', nth: 0, ordinal: -1 },
  ], tokens: 30, fellBack: null } })
  assert.deepEqual(turns[4].recalled, [
    { prompt: 1, said: 'prompt', nth: null, ordinal: 6 },
    { prompt: 0, said: 'reply', nth: 0 },
    { prompt: 0, said: 'reply', nth: 0 },
  ])
})

const answer = {
  contextBytes: 400,
  sends: [
    { label: 'Trusted: earlier excerpts', refreshed: false, replacedInTurn: true, model: 'm', excerpts: [
      { ordinal: 6, chunk: 0, from: 'user', context: false, text: 'Hold until verified' },
      { ordinal: 6, chunk: 1, from: 'user', context: false, text: 'and tell me' },
      { ordinal: 2, chunk: 0, from: 'tool', context: true, text: 'migrate' },
    ] },
    { label: 'Trusted: earlier excerpts', refreshed: true, replacedInTurn: false, model: 'm', excerpts: [
      { ordinal: 6, chunk: 0, from: 'user', context: false, text: 'Hold until verified' },
      { ordinal: 'x', chunk: 0, from: 'user', text: 'unreadable' },
    ] },
  ],
}

test('a row opens to each block its exchange sent for its message, and only those', () => {
  const read = excerptsIn(answer)
  assert.equal(read.sends[1].excerpts.length, 1, 'an excerpt this build cannot read is left out')
  const six = blocksFor(read, 6)
  assert.deepEqual(six.map((block) => [block.excerpt.chunk, block.send.replacedInTurn, block.send.refreshed]), [[0, true, false], [1, true, false], [0, false, true]])
  assert.deepEqual(blocksFor(read, 2).map((block) => block.excerpt.context), [true])
  assert.deepEqual(blocksFor(read, 9), [])
  assert.equal(excerptsIn({ sends: 'no' }), undefined)
})

test('an exchange is read once for all its rows, a failure is asked again, and forgetting reads afresh', async () => {
  forgetExcerpts()
  const key = { directory: '/p', id: 's1', prompt: 2 }
  let asked = 0
  let failing = true
  const request = async () => { asked++; if (failing) throw new Error('down'); return answer }
  await assert.rejects(readExcerpts(key, request))
  failing = false
  const first = await readExcerpts(key, request)
  const second = await readExcerpts(key, request)
  assert.equal(asked, 2, 'the failed read was not kept and the good one was')
  assert.equal(first, second)
  await readExcerpts({ ...key, prompt: 3 }, request)
  assert.equal(asked, 3, 'another exchange is its own read')
  forgetExcerpts()
  await readExcerpts(key, request)
  assert.equal(asked, 4, 'after a turn ends the record is read again')
  await assert.rejects(readExcerpts({ ...key, id: 's2' }, async () => ({ nonsense: true })))
})
