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
const staging = load(`export * from './src/renderer/staging'; export { definitionMarker } from './src/shared/summons'`)

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

const lantern = { name: 'lantern', description: 'Lights the way' }
const reviewerDefinition = { name: 'reviewer', description: null }
const ada = { slug: 'ada', name: 'Ada', avatar: 'v2:ada' }
const empty = { made: 0, staged: [] }

/** The words `bravebot_mentions` reads as `@` names: whitespace-separated words that start with `@`. */
const atWords = (line) => line.split(/\s+/).filter((word) => word.startsWith('@'))

test('a definition’s marker is never an @ name', () => {
  assert.equal(staging.definitionMarker('lantern'), '[@lantern]')
  assert.equal(staging.definitionMarker('[@x]'), '[@x]')
  assert.equal(staging.definitionMarker('two  words'), '[@two words]')
  assert.deepEqual(atWords(staging.definitionMarker('two words')), [])
})

test('picking a definition replaces the half-typed word with a marker that is not an @ word', () => {
  const picked = staging.stageDefinition(empty, lantern, 'ask @lan', '@lan')
  assert.equal(picked.draft, 'ask [@lantern] ')
  assert.equal(picked.caret, picked.draft.length)
  assert.deepEqual(atWords(picked.draft), [])
  assert.deepEqual(picked.staging.staged, [{ marker: '[@lantern]', via: 'address', definition: lantern }])
  assert.deepEqual(staging.addressOf(staging.named(picked.staging, picked.draft)), lantern)
  assert.equal(staging.summonOf(staging.named(picked.staging, picked.draft)), null)
  assert.equal(staging.stageDefinition(empty, lantern, 'no name here', '@lan'), null)
  assert.equal(staging.stageDefinition(empty, lantern, 'ask lan', 'lan'), null)
})

test('a second definition pick replaces the first, marker and all', () => {
  const first = staging.stageDefinition(empty, lantern, 'ask @lan', '@lan')
  const second = staging.stageDefinition(first.staging, reviewerDefinition, `${first.draft}or @rev`, '@rev')
  assert.equal(second.draft, 'ask or [@reviewer] ')
  assert.deepEqual(second.staging.staged.map((item) => item.definition.name), ['reviewer'])
  assert.deepEqual(staging.addressOf(staging.named(second.staging, second.draft)), reviewerDefinition)
})

test('a bot pick replaces a definition pick, and a definition pick replaces a bot pick', () => {
  const addressed = staging.stageDefinition(empty, lantern, 'ask @lan', '@lan')
  const summoned = staging.stageSummon(addressed.staging, ada, `${addressed.draft}or @ad`, '@ad')
  assert.equal(summoned.draft, 'ask or [@Ada] ')
  assert.deepEqual(summoned.staging.staged.map((item) => item.via), ['summon'])
  assert.equal(staging.addressOf(staging.named(summoned.staging, summoned.draft)), null)
  assert.deepEqual(staging.summonOf(staging.named(summoned.staging, summoned.draft)), ada)

  const back = staging.stageDefinition(summoned.staging, lantern, `${summoned.draft}then @lan`, '@lan')
  assert.equal(back.draft, 'ask or then [@lantern] ')
  assert.deepEqual(back.staging.staged.map((item) => item.via), ['address'])
  assert.equal(staging.summonOf(staging.named(back.staging, back.draft)), null)
})

test('a pick leaves drops, pastes and folded text staged', () => {
  const pasted = staging.stageText(empty, 'one\ntwo\nthree\nfour', 'see ', 4)
  const picked = staging.stageDefinition(pasted.staging, lantern, `${pasted.draft}@lan`, '@lan')
  assert.deepEqual(picked.staging.staged.map((item) => item.via), ['text', 'address'])
  assert.equal(picked.staging.made, pasted.staging.made)
})

test('addressOf is null once the marker is deleted from the draft', () => {
  const picked = staging.stageDefinition(empty, lantern, 'ask @lan', '@lan')
  const deleted = staging.withoutMarker(picked.draft, '[@lantern]')
  assert.equal(deleted, 'ask ')
  assert.equal(staging.addressOf(staging.named(picked.staging, deleted)), null)
  assert.equal(staging.addressOf([]), null)
})

const transcript = load(`export { userSaid, fromSaid, addressedReplies, summonedReplies, summonedAt } from './src/renderer/transcript'`)

test('a live addressed prompt labels its first reply with the definition and no reply after it', () => {
  const said = transcript.userSaid('[@lantern] light it', undefined, undefined, 'lantern')
  assert.equal(said.addressed, 'lantern')
  assert.equal(transcript.userSaid('plain').addressed, undefined)
  const entries = [
    said,
    { kind: 'assistant', id: 'a1', text: 'first' },
    { kind: 'assistant', id: 'a2', text: 'second' },
    { kind: 'user', id: 'p2', text: 'next', prompt: 1 },
    { kind: 'assistant', id: 'a3', text: 'own' },
  ]
  assert.deepEqual([...transcript.addressedReplies(entries)], [['a1', 'lantern']])
  assert.deepEqual([...transcript.summonedReplies(entries)], [])
})

test('a watch after an addressed prompt that got no reply is a turn of its own', () => {
  const entries = [
    transcript.userSaid('[@lantern] light it', undefined, undefined, 'lantern'),
    { kind: 'watch', id: 'w1', text: 'File watch 1: a.txt' },
    { kind: 'assistant', id: 'a1', text: 'own' },
  ]
  assert.deepEqual([...transcript.addressedReplies(entries)], [])
})

test('a reopened prompt the driver recorded as addressed for one turn is labelled with the definition', () => {
  const entries = transcript.fromSaid([
    { kind: 'user', text: 'light it', prompt: 0, address: { definition: 'lantern', standing: false } },
    { kind: 'assistant', text: 'lit' },
    { kind: 'user', text: 'and now', prompt: 1 },
    { kind: 'assistant', text: 'planner' },
  ])
  assert.equal(entries[0].addressed, 'lantern')
  assert.deepEqual([...transcript.addressedReplies(entries)], [[entries[1].id, 'lantern']])
})

test('a reopened prompt under a standing definition carries no label', () => {
  const entries = transcript.fromSaid([
    { kind: 'user', text: 'hi', prompt: 0, address: { definition: 'ada', standing: true } },
    { kind: 'assistant', text: 'own' },
  ])
  assert.equal(entries[0].addressed, undefined)
  assert.deepEqual([...transcript.addressedReplies(entries)], [])
})

test('a reopened prompt a summon record claims keeps its bot’s label and not the definition', () => {
  const reopened = transcript.fromSaid([
    { kind: 'user', text: '[@Reviewer] look', prompt: 0, address: { definition: 'reviewer-def', standing: false } },
    { kind: 'assistant', text: 'looked' },
  ])
  const entries = transcript.summonedAt(reopened, new Map([[0, 'reviewer']]))
  assert.deepEqual([...transcript.summonedReplies(entries)], [[entries[1].id, 'reviewer']])
  assert.deepEqual([...transcript.addressedReplies(entries)], [])
})
