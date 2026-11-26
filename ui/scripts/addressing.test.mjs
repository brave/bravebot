/**
 * Addressing one turn to a definition a person picked from the composer's `@` list (ADDRESS-3).
 * The main process's send checks the name against the definition rows the agent's
 * `mentions.offer` returns for the session, and refuses what a window could ask for directly. The
 * send is called with the agent and the other main-process services replaced, since the entry
 * point itself needs Electron.
 */

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const { sendAddressedTurn } = load(`export { sendAddressedTurn } from './src/main/addressing'`)
const { Summoning } = load(`export { Summoning } from './src/main/summon-turn'`)

const SESSION = '11111111-1111-4111-8111-111111111111'
const definitionRow = (path, description = null) => ({ path, shownPath: path, written: path, directory: false, kind: 'definition', description })
const fileRow = (path) => ({ path, shownPath: path, written: path, directory: false, kind: 'file', description: null })

/** Services with a fake agent that offers `rows` and records every request. */
function services({ rows = [definitionRow('lantern', 'Lights the way'), definitionRow('reviewer')], owner, hostBot } = {}) {
  const requests = []
  const summoning = new Summoning()
  const adopted = []
  return {
    requests,
    summoning,
    adopted,
    sent: () => requests.filter(([method]) => method === 'turn.send').map(([, params]) => params),
    request: async (method, params) => {
      requests.push([method, params])
      if (method === 'mentions.offer') return { typed: params.line.slice(1), word: params.line, entries: rows, completes: true }
      if (method === 'turn.send') return { turn: 1 }
      throw new Error(`unexpected ${method}`)
    },
    folderOf: (session) => (session === SESSION ? '/work/site' : undefined),
    ownerOf: () => owner,
    adopt: (session, slug) => adopted.push([session, slug]),
    migrate: async (held) => held,
    host: async () => hostBot ?? null,
    carry: (_session, params) => ({ ...params, files: [] }),
    failure: (error) => ({ code: 'internal', message: String(error) }),
  }
}

test('a name among the offered definition rows sends turn.send addressed to it, with no definition standing in a project chat', async () => {
  const held = services()
  const answer = await sendAddressedTurn(held, SESSION, 'lantern', 'light the path', { shown: '[@lantern] light the path', model: 'm' })

  assert.deepEqual(answer, { ok: { turn: 1 } })
  const [offer] = held.requests
  assert.deepEqual(offer, ['mentions.offer', { session: SESSION, line: '@lantern', cursor: 0, definitions: true }])
  const [sent] = held.sent()
  assert.equal(sent.address, 'lantern')
  assert.equal(sent.prompt, 'light the path')
  assert.equal(sent.shown, '[@lantern] light the path')
  assert.equal(sent.model, 'm')
  assert.equal(sent.session, SESSION)
  assert.equal(typeof sent.messageId, 'string')
  assert.equal('definition' in sent, false)
  // No bot was summoned, so nothing is recorded as a summon.
  assert.equal(held.summoning.has(SESSION), false)
})

test('in a bot’s conversation the bot’s definition stays standing beside the pick', async () => {
  const held = services({ owner: 'ada', hostBot: { slug: 'ada', definition: 'ada', model: 'bot-model' } })
  const answer = await sendAddressedTurn(held, SESSION, 'lantern', 'light the path', { model: 'm' })

  assert.deepEqual(answer, { ok: { turn: 1 } })
  const [sent] = held.sent()
  assert.equal(sent.definition, 'ada')
  assert.equal(sent.address, 'lantern')
  assert.equal(sent.model, 'bot-model')
  assert.deepEqual(held.adopted, [[SESSION, 'ada']])
})

test('a name absent from the definition rows sends nothing and is refused with the rows listed', async () => {
  const held = services()
  const answer = await sendAddressedTurn(held, SESSION, 'ghost', 'boo', {})

  assert.equal(answer.error.code, 'no_such_definition')
  assert.match(answer.error.message, /ghost/)
  assert.match(answer.error.message, /lantern, reviewer/)
  assert.deepEqual(held.sent(), [])
})

test('a name offered only as a file row is no definition, and sends nothing', async () => {
  const held = services({ rows: [fileRow('notes'), definitionRow('reviewer')] })
  const answer = await sendAddressedTurn(held, SESSION, 'notes', 'read them', {})

  assert.equal(answer.error.code, 'no_such_definition')
  assert.match(answer.error.message, /reviewer/)
  assert.doesNotMatch(answer.error.message, /definitions: .*notes/)
  assert.deepEqual(held.sent(), [])
})

test('an empty or whitespace prompt needs a task and sends nothing', async () => {
  for (const prompt of ['', '   \n\t']) {
    const held = services()
    const answer = await sendAddressedTurn(held, SESSION, 'lantern', prompt, {})

    assert.deepEqual(answer.error, { code: 'needs_a_task', message: '/agent lantern needs a task' })
    assert.deepEqual(held.requests, [])
  }
})

test('a name that is not a slug is a bad request and asks the agent nothing', async () => {
  for (const name of ['Lantern', '../escape', '', 42, null, 'a--b']) {
    const held = services({ rows: [definitionRow(String(name))] })
    const answer = await sendAddressedTurn(held, SESSION, name, 'go', {})

    assert.equal(answer.error.code, 'bad_request')
    assert.deepEqual(held.requests, [])
  }
})

test('no agent running and a session that is not open send nothing', async () => {
  const held = services()
  assert.equal((await sendAddressedTurn({ ...held, request: null }, SESSION, 'lantern', 'go', {})).error.code, 'no_bridge')
  assert.equal((await sendAddressedTurn(held, '22222222-2222-4222-8222-222222222222', 'lantern', 'go', {})).error.code, 'bad_request')
  assert.deepEqual(held.sent(), [])
})
