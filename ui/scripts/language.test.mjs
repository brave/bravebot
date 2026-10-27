import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { messageProblems } from './check-messages.mjs'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { askLocale, LANGUAGE_WAIT_MS } = load('src/renderer/i18n/choose.ts')
const { relaunchUnlessRunning } = load('src/main/relaunch.ts')

test('the window is read in the catalog the bridge names', async () => {
  assert.equal(await askLocale(async () => ({ ok: { inUse: 'fr', saved: null, available: [], overridden: false } })), 'fr')
})

test('a bridge that fails leaves the window in English', async () => {
  assert.equal(await askLocale(async () => { throw new Error('the agent is not running') }), 'en-US')
  assert.equal(await askLocale(async () => ({ error: { code: 'no_bridge', message: 'gone' } })), 'en-US')
  assert.equal(await askLocale(async () => ({ ok: { inUse: 7 } })), 'en-US')
})

test('a bridge that answers late leaves the window in English rather than blank', async () => {
  assert.equal(LANGUAGE_WAIT_MS, 3000)
  const slow = () => new Promise((resolve) => setTimeout(() => resolve({ ok: { inUse: 'fr' } }), 200))
  const started = Date.now()
  assert.equal(await askLocale(slow, 20), 'en-US')
  assert.ok(Date.now() - started < 150, 'the wait was not cut short')
  assert.equal(await askLocale(() => new Promise(() => {}), 20), 'en-US')
  assert.equal(await askLocale(slow, 1000), 'fr')
})

test('a restart waits for every running turn, since quitting would end it', () => {
  const calls = []
  const app = { relaunch: () => calls.push('relaunch'), quit: () => calls.push('quit') }
  assert.equal(relaunchUnlessRunning(new Set(['s1']), app), false)
  assert.deepEqual(calls, [])
  assert.equal(relaunchUnlessRunning(new Set(), app), true)
  assert.deepEqual(calls, ['relaunch', 'quit'])
})

const usesCatalog = "import { t } from '../i18n'\n"

test('a message looked up by a computed key is refused', () => {
  const problems = messageProblems('components/X.tsx', `${usesCatalog}const a = t[key]()\nconst b = t [\`ui\${n}\`]()\n`)
  assert.deepEqual(problems.map((p) => p.line), [2, 3])
  assert.match(problems[0].message, /computed key/)
})

test('a message named in the source passes, and so does a t[ in a file that never reads the catalog', () => {
  assert.deepEqual(messageProblems('components/X.tsx', `${usesCatalog}const a = t.uiSettingsTheme()\n// t[key] in a comment\n`), [])
  assert.deepEqual(messageProblems('components/Y.tsx', 'const t = [1]\nconst a = t[0]\n'), [])
})

test('the generated catalogs are reached only through i18n', () => {
  const problems = messageProblems('components/X.tsx', "import { CATALOGS } from '../i18n/messages.gen'\n")
  assert.equal(problems.length, 1)
  assert.match(problems[0].message, /through i18n/)
  assert.deepEqual(messageProblems('i18n/index.ts', "import { CATALOGS } from './messages.gen'\nt[x]\n"), [])
})
