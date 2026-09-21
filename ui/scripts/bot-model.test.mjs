import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/shared/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { parseBots, withBot, isBotModel } = module.exports
const definition = { slug: 'web-dev', name: 'Web dev', purpose: 'Build websites', directory: '/tmp/web-dev', avatar: 'v2:preview', session: null }

test('model selection survives storage, updates, and unrelated bot edits', () => {
  const original = parseBots({ bots: [{ ...definition, model: 'provider/model-a' }] }).bots[0]
  const changed = withBot([original], { ...original, model: 'provider/model-b' })
  const saved = parseBots(JSON.parse(JSON.stringify({ bots: changed }))).bots[0]
  assert.equal(saved.model, 'provider/model-b')
  assert.equal(saved.avatar, definition.avatar)
  const renamed = parseBots({ bots: [{ ...saved, name: 'Frontend developer' }] }).bots[0]
  assert.equal(renamed.model, 'provider/model-b')
})

test('older and malformed model fields preserve the bot using the configured default', () => {
  for (const model of [undefined, null, '', '  ', 42, {}, 'bad\0model', 'x'.repeat(513)]) {
    const result = parseBots({ bots: [{ ...definition, model }] }).bots
    assert.equal(result.length, 1)
    assert.equal(result[0].model, null)
  }
})

test('IPC model validation accepts opaque IDs and rejects malformed inputs', () => {
  for (const value of [null, 'openrouter/provider/model:free', 'custom-model']) assert.equal(isBotModel(value), true)
  for (const value of [undefined, '', ' ', 1, {}, 'a\0b', 'x'.repeat(513)]) assert.equal(isBotModel(value), false)
})

test('main-process storage retains model changes across a fresh module load', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-model-storage-'))
  const source = buildSync({ entryPoints: ['src/main/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const load = () => {
    const module = { exports: {} }
    const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
    new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
    return module.exports
  }
  try {
    const storage = load()
    storage.saveBot(parseBots({ bots: [{ ...definition, model: 'provider/model-a' }] }).bots[0])
    storage.saveBot({ ...storage.bot(definition.slug), model: 'provider/model-b' })
    assert.equal(JSON.parse(readFileSync(join(profile, 'bravebot-ui.json'), 'utf8')).bots[0].model, 'provider/model-b')
    assert.equal(load().bot(definition.slug).model, 'provider/model-b')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})
