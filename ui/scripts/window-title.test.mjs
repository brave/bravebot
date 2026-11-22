// What the desktop window tells the bridge about the session on screen, and how it writes the
// title the bridge answers with. The title's text is built in Rust, by `bravebot_agent::title`
// and `bravebot_ui_bridge::window_title`, whose tests pin it.

import test from 'node:test'
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { loadSync } from './render-support.mjs'

const { sessionStatus, titleState } = loadSync(fileURLToPath(new URL('../src/renderer/session-list.ts', import.meta.url)))
const live = (over = {}) => ({ handle: 'h', summary: { id: 's', title: 'fix', project: 'p', branch: null, directory: '/p' }, entries: [], running: false, bot: null, ...over })
const lives = {
  working: live({ running: true }),
  answer: live({ entries: [{ kind: 'ask', id: 'q', request: { request: 1 }, answers: null }] }),
  approval: live({ entries: [{ kind: 'run', id: 'r', request: { request: 2 }, decision: null }] }),
  failed: live({ entries: [{ kind: 'error' }] }),
  quiet: live(),
}

test('a session row and the window title read the same state', () => {
  assert.equal(sessionStatus(lives.working), 'working')
  assert.equal(sessionStatus(lives.answer), 'answer')
  assert.equal(sessionStatus(lives.approval), 'approval')
  assert.equal(sessionStatus(lives.failed), 'failed')
  assert.equal(sessionStatus(lives.quiet), undefined)
  assert.equal(titleState(lives.working), 'working')
  assert.equal(titleState(lives.answer), 'waiting')
  assert.equal(titleState(lives.approval), 'waiting')
  assert.equal(titleState(lives.failed), 'ready', 'a failed turn is ready again, as in the terminal')
  assert.equal(titleState(lives.quiet), 'ready')
  assert.equal(titleState(null), 'ready')
})

test('a question waiting outranks a running turn', () => {
  const both = live({ running: true, entries: lives.answer.entries })
  assert.equal(sessionStatus(both), 'answer')
  assert.equal(titleState(both), 'waiting')
})

const { titleWriter } = loadSync(fileURLToPath(new URL('../src/renderer/window-title.ts', import.meta.url)))

/** A page that records each title written, and an ask that answers as `answer` says. */
function harness(answer = (name, state) => Promise.resolve(`${state}:${name}`)) {
  const writes = []
  const asks = []
  const page = { get title() { return writes.at(-1) ?? '' }, set title(text) { writes.push(text) } }
  const write = titleWriter(page, (name, state) => {
    asks.push([name, state])
    return answer(name, state)
  })
  return { write, writes, asks }
}

test('an unchanged name and state ask nothing and write nothing', async () => {
  const { write, writes, asks } = harness()
  await write('fix', 'ready')
  await write('fix', 'ready')
  await write('fix', 'working')
  await write('fix', 'working')
  await write('', 'ready')
  assert.deepEqual(asks, [['fix', 'ready'], ['fix', 'working'], ['', 'ready']])
  assert.deepEqual(writes, ['ready:fix', 'working:fix', 'ready:'])
})

test('an answer that is the title already shown is not written again', async () => {
  const { write, writes } = harness(() => Promise.resolve('Bravebot'))
  await write('', 'ready')
  await write('   ', 'ready')
  assert.deepEqual(writes, ['Bravebot'])
})

test('an answer overtaken by a newer request is dropped', async () => {
  const pending = []
  const { write, writes } = harness((name) => new Promise((resolve) => pending.push([name, resolve])))
  const first = write('old', 'ready')
  const second = write('new', 'ready')
  pending[1][1]('Bravebot · new')
  await second
  pending[0][1]('Bravebot · old')
  await first
  assert.deepEqual(writes, ['Bravebot · new'])
})

test('a failed ask leaves the title as it is', async () => {
  const { write, writes } = harness(() => Promise.reject(new Error('no bridge')))
  await write('fix', 'ready')
  assert.deepEqual(writes, [])
})
