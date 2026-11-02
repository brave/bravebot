// The gate in front of the window's entry of a Brave Premium or gateway key (PREM-16, CRED-24): the
// page only asks from a row, the key is typed in a window the main process owns, and it goes from
// that process to the bridge and never to the page.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

function load(entry) {
  const source = buildSync({
    stdin: { contents: `export * from '${entry}'\n`, resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { KeyEntry, KEY_COOLDOWN_MS } = load('./src/main/key-entry')
const { accountLogo } = load('./src/shared/accounts')

const KEY = 'sk-PLACEHOLDER-TYPED-KEY-k9k9'

const row = (way, id, fields = {}) => ({ way, id, name: id ?? way, standing: 'notSignedIn', keyFrom: null, detail: null, command: null, canSignOut: false, host: null, canEnterKey: true, ...fields })

/** A gate with the bridge, the windows and the clock stubbed. */
function gate({ rows = [row('brave', null), row('gateway', 'openrouter', { name: 'OpenRouter' })], before = { ready: true, stored: false, host: 'openrouter.ai', variable: null, records: true }, stored = { outcome: 'stored', sentence: null, accounts: [] } } = {}) {
  const webContents = {}
  const window = { webContents, focused: true, destroyed: false, isFocused() { return this.focused }, isDestroyed() { return this.destroyed } }
  const sent = []
  const told = []
  const shown = []
  const stub = { clock: 1_000_000, changed: 0, closed: 0, rows: { accounts: rows }, before, stored, otherBusy: false }
  const entry = new KeyEntry({
    ask: async (method, params) => {
      sent.push({ method, params })
      if (method === 'accounts.list') return stub.rows
      if (method === 'accounts.keyBefore') return typeof stub.before === 'function' ? stub.before(params) : stub.before
      if (method === 'accounts.storeKey') return typeof stub.stored === 'function' ? stub.stored(params) : stub.stored
      return {}
    },
    window: () => window,
    tell: (notice) => told.push(notice),
    showWindow: (state) => shown.push(state),
    closeWindow: () => { stub.closed += 1 },
    changed: () => { stub.changed += 1 },
    otherBusy: () => stub.otherBusy,
    now: () => stub.clock,
  })
  return { entry, window, webContents, sent, told, shown, stub }
}

const methods = (sent) => sent.map((each) => each.method)

test('accounts.keyBefore and accounts.storeKey are not methods the page may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)[1]
  assert.doesNotMatch(allowed, /accounts\.keyBefore|accounts\.storeKey/)
  const preload = readFileSync('src/preload/index.ts', 'utf8')
  assert.doesNotMatch(preload, /key-window:save|accounts\.storeKey/, 'the main window has no way to hand over a key')
})

test('an ask is answered only from the focused main window, for a row that can take a key', async () => {
  const g = gate()
  assert.equal(await g.entry.request({}, 'brave', null), null, 'a foreign sender')
  g.window.focused = false
  assert.equal(await g.entry.request(g.webContents, 'brave', null), null, 'an unfocused window')
  g.window.focused = true
  for (const [way, id] of [['chatgpt', null], ['gateway', null], ['gateway', ''], ['gateway', 4], ['bedrock', null]]) {
    assert.equal(await g.entry.request(g.webContents, way, id), null, `${way} ${id}`)
  }
  assert.deepEqual(g.sent, [], 'nothing reached the bridge')
  const none = gate({ rows: [row('gateway', 'openrouter', { canEnterKey: false })] })
  assert.equal(await none.entry.request(none.webContents, 'gateway', 'openrouter'), null)
  assert.equal(await none.entry.request(none.webContents, 'gateway', 'elsewhere'), null)
  assert.deepEqual(methods(none.sent), ['accounts.list', 'accounts.list'], 'a row that cannot take a key is not asked about')
  assert.deepEqual(none.shown, [])
})

test('the bridge is asked before anything is typed, and its refusal is said on the page', async () => {
  const g = gate({ before: { ready: false, sentence: 'An incognito session keeps nothing on disk, so no key can be stored.' } })
  assert.deepEqual(await g.entry.request(g.webContents, 'gateway', 'openrouter'), { error: 'An incognito session keeps nothing on disk, so no key can be stored.' })
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.keyBefore', params: { way: 'gateway', id: 'openrouter' } })
  assert.deepEqual(g.shown, [], 'no key window opens')
  const silent = gate({ before: {} })
  assert.match((await silent.entry.request(silent.webContents, 'brave', null)).error, /Rebuild or reinstall/)
})

test('the key window shows what the bridge said, and a stored key is replaced because the bridge said one is stored', async () => {
  const g = gate({ before: { ready: true, stored: true, host: null, variable: null, records: false } })
  assert.deepEqual(await g.entry.request(g.webContents, 'brave', 'ignored'), { opened: true })
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.keyBefore', params: { way: 'brave' } })
  assert.deepEqual(g.shown, [{ way: 'brave', id: null, name: 'brave', host: null, replace: true, variable: null, records: false }])
  assert.deepEqual(g.told, [{ kind: 'opened', way: 'brave', id: null }])
  assert.deepEqual(await g.entry.save(KEY), { saved: true })
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.storeKey', params: { way: 'brave', key: KEY, replace: true } })

  const fresh = gate()
  await fresh.entry.request(fresh.webContents, 'gateway', 'openrouter')
  assert.equal(fresh.shown[0].replace, false)
  assert.equal(fresh.shown[0].host, 'openrouter.ai')
  assert.equal(fresh.shown[0].records, true)
  await fresh.entry.save(KEY)
  assert.deepEqual(fresh.sent.at(-1), { method: 'accounts.storeKey', params: { way: 'gateway', id: 'openrouter', key: KEY, replace: false } })
})

test('a key stored closes the window and tells the page the outcome and never the key', async () => {
  const g = gate({ stored: { outcome: 'stored', sentence: 'The key is stored, but your settings could not be changed.' } })
  await g.entry.request(g.webContents, 'gateway', 'openrouter')
  await g.entry.save(KEY)
  assert.equal(g.stub.closed, 1)
  assert.equal(g.stub.changed, 1, 'the model listing is dropped')
  assert.deepEqual(g.told.at(-1), { kind: 'ended', way: 'gateway', id: 'openrouter', outcome: 'stored', sentence: 'The key is stored, but your settings could not be changed.' })
  assert.equal(g.entry.windowState(), null)
  assert.ok(!JSON.stringify([g.told, g.shown, g.entry.windowState()]).includes(KEY), 'the key went nowhere but the bridge')
  assert.deepEqual(await g.entry.save(KEY), { refused: 'The key window is not waiting for a key.' })
  assert.equal(g.sent.filter((each) => each.method === 'accounts.storeKey').length, 1)
})

test('a refusal keeps the window open with the sentence, so the key can be typed again', async () => {
  let answers = [{ outcome: 'refused', sentence: 'No key was typed, so nothing was stored.' }, { outcome: 'stored', sentence: null }]
  const g = gate({ stored: () => answers.shift() })
  await g.entry.request(g.webContents, 'brave', null)
  assert.deepEqual(await g.entry.save('   '), { refused: 'No key was typed, so nothing was stored.' })
  assert.equal(g.stub.closed, 0)
  assert.equal(g.told.length, 1, 'the page is told nothing of a refusal')
  assert.notEqual(g.entry.windowState(), null)
  assert.deepEqual(await g.entry.save(KEY), { saved: true })

  const broken = gate()
  await broken.entry.request(broken.webContents, 'brave', null)
  broken.entry.deps.ask = async () => { throw new Error('The agent is not running.') }
  assert.deepEqual(await broken.entry.save(KEY), { refused: 'The agent is not running.' })
  assert.notEqual(broken.entry.windowState(), null)
  assert.deepEqual(await broken.entry.save(4), { refused: 'Type or paste the key.' })
})

test('one key window at a time, and closing it cancels and holds the next ask off', async () => {
  const g = gate()
  await g.entry.request(g.webContents, 'brave', null)
  assert.equal(await g.entry.request(g.webContents, 'gateway', 'openrouter'), null, 'one at a time')
  assert.equal(g.shown.length, 1)
  assert.equal(g.entry.show({}), false)
  assert.equal(g.entry.show(g.webContents), true)
  assert.equal(g.shown.length, 2, 'Show key window brings the same one back')
  assert.deepEqual(g.entry.current(g.webContents), { way: 'brave', id: null })
  assert.equal(g.entry.current({}), null)

  g.entry.cancel()
  assert.deepEqual(g.told.at(-1), { kind: 'ended', way: 'brave', id: null, outcome: 'cancelled', sentence: null })
  assert.ok(!methods(g.sent).includes('accounts.storeKey'), 'nothing is stored')
  assert.equal(await g.entry.request(g.webContents, 'brave', null), null, 'held off just after')
  g.stub.clock += KEY_COOLDOWN_MS
  assert.deepEqual(await g.entry.request(g.webContents, 'brave', null), { opened: true })
})

test('a window closed while its key is on its way ends as the bridge says', async () => {
  let release
  const g = gate({ stored: () => new Promise((resolve) => { release = resolve }) })
  await g.entry.request(g.webContents, 'brave', null)
  const saved = g.entry.save(KEY)
  g.entry.cancel()
  release({ outcome: 'stored', sentence: null })
  assert.deepEqual(await saved, { saved: true })
  assert.deepEqual(g.told.at(-1), { kind: 'ended', way: 'brave', id: null, outcome: 'stored', sentence: null })

  const refused = gate({ stored: () => new Promise((resolve) => { release = resolve }) })
  await refused.entry.request(refused.webContents, 'brave', null)
  const later = refused.entry.save(KEY)
  refused.entry.cancel()
  release({ outcome: 'refused', sentence: 'no' })
  await later
  assert.equal(refused.told.at(-1).outcome, 'cancelled')
  assert.equal(refused.entry.windowState(), null)
})

test('the main window closing or quitting takes the key window with it', async () => {
  const g = gate()
  await g.entry.request(g.webContents, 'brave', null)
  g.entry.abandon()
  assert.equal(g.entry.windowState(), null)
  assert.equal(g.stub.closed, 1)
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /on\('before-quit'[\s\S]{0,400}keyEntry\.abandon\(\)/)
  assert.match(index, /window\.on\('closed', \(\) => \{[\s\S]{0,100}keyEntry\.abandon\(\)/)
  assert.match(index, /new KeyEntry<BrowserWindow>\(\{[\s\S]*?changed: \(\) => \{ modelListing = null \}/)
})

test('the key window is a page of its own that reaches only its own channels', () => {
  const main = readFileSync('src/main/key-window.ts', 'utf8')
  for (const pattern of [/useContentSize: true/, /sandbox: true/, /contextIsolation: true/, /nodeIntegration: false/, /preload\/key-window\.js/,
    /setWindowOpenHandler\(\(\) => \(\{ action: 'deny' \}\)\)/, /on\('will-navigate', \(event\) => event\.preventDefault\(\)\)/, /if \(!closing\) cancel\(\)/]) {
    assert.match(main, pattern)
  }
  const handlers = main.split('ipcMain.handle(').slice(1).map((each) => each.slice(0, 160))
  assert.equal(handlers.length, 4)
  assert.match(main, /Math\.min\(KEY_WINDOW_HEIGHTS\.most, Math\.max\(KEY_WINDOW_HEIGHTS\.least/, 'the page cannot size its window past the bounds')
  for (const each of handlers) assert.match(each, /^'[^']+', \(event[^)]*\) => [\s\S]*?ours\(event\)/, 'each channel answers only the key window')
  const preload = readFileSync('src/preload/key-window.ts', 'utf8')
  assert.deepEqual([...new Set([...preload.matchAll(/'(bravebot:[^']+)'/g)].map((m) => m[1]))],
    ['bravebot:key-window:read', 'bravebot:key-window:save', 'bravebot:key-window:cancel', 'bravebot:key-window:fit', 'bravebot:key-window:changed'])
  assert.doesNotMatch(preload, /ipcRenderer\.send\(/, 'the page only asks and listens')
  const page = readFileSync('src/renderer/key-window.tsx', 'utf8')
  assert.match(page, /type="password"/)
  assert.doesNotMatch(page, /window\.bravebot\./, 'the key window has no way to the agent')
})

test('each way a key is entered for shows its mark', () => {
  assert.equal(accountLogo('brave', null), 'brave-icon-release-color')
  assert.equal(accountLogo('gateway', 'openrouter'), 'openrouter')
  assert.equal(accountLogo('gateway', 'somewhere'), 'key')
  assert.equal(accountLogo('bedrock', null), 'amazon-color')
})

test('the session the page names reaches the list, the check and the store, and the AWS rows are not checked', async () => {
  const g = gate()
  assert.deepEqual(await g.entry.request(g.webContents, 'gateway', 'openrouter', 's1'), { opened: true })
  assert.deepEqual(await g.entry.save(KEY), { saved: true })
  assert.deepEqual(g.sent.map((each) => [each.method, each.params.session]), [['accounts.list', 's1'], ['accounts.keyBefore', 's1'], ['accounts.storeKey', 's1']])
  assert.equal(g.sent[0].params.bedrock, false)
  assert.ok(!JSON.stringify(g.shown).includes('s1'), 'the key window is not told the session')
  const none = gate()
  await none.entry.request(none.webContents, 'brave', null)
  assert.deepEqual(none.sent[0].params, { bedrock: false })
  for (const session of ['', 7]) {
    const bad = gate()
    assert.equal(await bad.entry.request(bad.webContents, 'brave', null, session), null)
    assert.deepEqual(bad.sent, [])
  }
})

test('no key window is asked for while a sign-in is asked for or waits', async () => {
  const g = gate()
  g.stub.otherBusy = true
  assert.equal(await g.entry.request(g.webContents, 'brave', null), null)
  assert.deepEqual(g.sent, [])
  g.stub.otherBusy = false
  await g.entry.request(g.webContents, 'brave', null)
  assert.equal(g.entry.busy(), true, 'an open key window holds a sign-in off')
  g.entry.cancel()
  assert.equal(g.entry.busy(), false)
})
