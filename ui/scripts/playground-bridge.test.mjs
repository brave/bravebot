// The playground bridge stands in for the agent. A reply it writes has to be in the transcript a
// fork cuts and a closed conversation reads back, and a scenario continues only after the reply
// that carried an answer has resolved.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

const store = new Map()
globalThis.localStorage = {
  getItem: (key) => store.get(key) ?? null,
  setItem: (key, value) => { store.set(key, String(value)) },
  removeItem: (key) => { store.delete(key) },
}
const frames = []
globalThis.requestAnimationFrame = (fn) => { frames.push(fn); return frames.length }
globalThis.cancelAnimationFrame = () => undefined
globalThis.document = { querySelector: () => null, querySelectorAll: () => [] }
globalThis.window = globalThis

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { install, play, playground } = load('playground/bridge.ts')
install()

const request = (method, params) => window.bravebot.request(method, params)
const tide = 'Why does the import drop the last row?'

async function settle() {
  while (frames.length) frames.shift()()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

async function conversation() {
  const created = await request('session.new', { directory: '/work/app' })
  const handle = created.ok.session
  playground.reply = async (script) => { script.done('First reply.') }
  await request('turn.send', { session: handle, prompt: 'First question' })
  await settle()
  playground.reply = async (script) => { script.done('Second reply.') }
  await request('turn.send', { session: handle, prompt: 'Second question' })
  await settle()
  return handle
}

test('a fork at a later prompt includes the earlier reply', async () => {
  const handle = await conversation()
  const forked = await request('session.fork', { session: handle, prompt: 1, text: 'Second question' })
  assert.deepEqual(forked.ok.said.map((entry) => entry.kind), ['user', 'assistant'])
  assert.equal(forked.ok.said[0].text, 'First question')
  assert.equal(forked.ok.said[1].text, 'First reply.')
})

test('a scenario continues after the answer has been returned', async () => {
  const created = await request('session.new', { directory: '/work/app' })
  const handle = created.ok.session
  const order = []
  play(handle, 'race', async (script) => {
    await script.ask('manifest', 4, 'reject')
    order.push('resumed')
  })
  order.push('asked')
  const pending = request('manifest.reply', { session: handle, request: 4, decision: 'reject' })
  order.push('called')
  await pending
  order.push('returned')
  assert.deepEqual(order, ['asked', 'called', 'returned'])
  await new Promise((resolve) => setTimeout(resolve, 0))
  assert.deepEqual(order, ['asked', 'called', 'returned', 'resumed'])
})

test('closing a new chat and opening it again reads the replies it wrote', async () => {
  const handle = await conversation()
  const listed = await request('session.list')
  const row = listed.ok.sessions.find((session) => session.title === 'First question')
  assert.ok(row, 'the new conversation is in the list')
  await request('session.close', { session: handle })
  const opened = await request('session.open', { id: row.id, directory: row.directory })
  const texts = opened.ok.said.map((entry) => entry.text)
  assert.ok(texts.includes('First reply.'), texts.join('\n'))
  assert.ok(texts.includes('Second reply.'), texts.join('\n'))
  assert.ok(!texts.includes(tide), texts.join('\n'))
})
