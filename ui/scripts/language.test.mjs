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

// A call that has not returned by the deadline fails with the message. The deadline bounds a call
// that waits on a bridge that never answers, and a loaded machine has to stall for all of it.
const DEADLINE_MS = 30000
function within(promise, message) {
  let timer
  const deadline = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), DEADLINE_MS) })
  return Promise.race([promise, deadline]).finally(() => clearTimeout(timer))
}

test('a bridge that answers late leaves the window in English rather than blank', async () => {
  assert.equal(LANGUAGE_WAIT_MS, 3000)
  // The bridge answers when the test says so, which is after the wait has ended, so how late a
  // loaded machine runs the wait's timer does not decide whether the answer was late.
  let answerNow
  const silent = () => new Promise((resolve) => { answerNow = () => resolve({ ok: { inUse: 'fr' } }) })
  assert.equal(await within(askLocale(silent, 20), 'the window waited on a bridge that had not answered'), 'en-US')
  answerNow()
  assert.equal(await within(askLocale(() => new Promise(() => {}), 20), 'the window waited on a bridge that never answers'), 'en-US')
  // An answer inside a wait too long for a loaded machine to outlast is read.
  const prompt = () => new Promise((resolve) => setTimeout(() => resolve({ ok: { inUse: 'fr' } }), 20))
  assert.equal(await within(askLocale(prompt, DEADLINE_MS), 'the window did not read an answer that arrived in time'), 'fr')
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
  assert.deepEqual(messageProblems('i18n\\index.ts', "import { CATALOGS } from './messages.gen'\n"), [], 'a Windows path')
})
