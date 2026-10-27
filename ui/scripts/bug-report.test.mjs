// The bug report's main-process half (DIAG-9): a save writes what the bridge composed and nothing
// the window said, readable by its owner alone; a page is opened by name; the log is shown only
// where it is a log.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)

/** The module with electron stubbed, and a record of what it asked electron for. */
function load({ saveTo = null, documents = '/nowhere' } = {}) {
  const seen = { opened: [], shown: [], sheets: [] }
  const electron = {
    app: { getVersion: () => '0.9.1', getPath: (name) => { assert.equal(name, 'documents'); return documents } },
    dialog: {
      showSaveDialog: async (window, options) => {
        seen.sheets.push(options)
        return saveTo ? { canceled: false, filePath: saveTo } : { canceled: true, filePath: '' }
      },
    },
    shell: {
      openExternal: async (url) => { seen.opened.push(url) },
      showItemInFolder: (path) => { seen.shown.push(path) },
    },
  }
  const source = buildSync({ entryPoints: ['src/main/bug-report.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => id === 'electron' ? electron : require(id), module, module.exports)
  return { main: module.exports, seen }
}

/** A bridge that answers compose with `answer`, and records what it was asked. */
function bridge(answer) {
  const asked = []
  const ask = async (method, params) => { asked.push({ method, params }); return answer }
  return { ask, asked }
}

const scratch = (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'bravebot-bug-report-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  return dir
}

test('compose states the app\'s own version as the interface and reads only the answer\'s shape', async () => {
  const { main } = load()
  const { ask, asked } = bridge({ available: true, text: 'report', log: null })
  assert.deepEqual(await main.compose(ask), { available: true, text: 'report', log: null })
  assert.deepEqual(asked, [{ method: 'bug-report.compose', params: { interface: '0.9.1' } }])
  assert.equal((await main.compose(async () => { throw new Error('gone') })).available, false)
  assert.equal(main.parseComposed({ available: true, text: 3 }).available, false)
  assert.equal(main.parseComposed({ available: true, text: 'x', log: '/etc/passwd' }).log, null, 'a log path that is not a log is dropped')
})

test('a save writes the bridge\'s text, owner-only, even over a file that was readable by others', async (t) => {
  const dir = scratch(t)
  const target = join(dir, 'report.txt')
  writeFileSync(target, 'old')
  chmodSync(target, 0o644)
  const { main, seen } = load({ saveTo: target, documents: dir })
  const { ask, asked } = bridge({ available: true, text: 'composed by the bridge\n', log: null })
  // The window cannot hand text in: the function takes a window and a bridge and nothing else.
  assert.equal(main.save.length, 2)
  assert.deepEqual(await main.save({}, ask), { status: 'saved', where: target })
  assert.equal(readFileSync(target, 'utf8'), 'composed by the bridge\n')
  if (process.platform !== 'win32') assert.equal(statSync(target).mode & 0o777, 0o600)
  assert.equal(asked.length, 1, 'composed again after the sheet closed')
  assert.equal(seen.sheets[0].defaultPath, join(dir, 'bravebot-bug-report.txt'))
})

test('a cancelled sheet writes nothing, and a session that keeps nothing fails', async (t) => {
  const dir = scratch(t)
  const target = join(dir, 'report.txt')
  {
    const { main } = load()
    const { ask, asked } = bridge({ available: true, text: 'x', log: null })
    assert.deepEqual(await main.save({}, ask), { status: 'cancelled' })
    assert.equal(asked.length, 0)
  }
  {
    const { main } = load({ saveTo: target })
    const outcome = await main.save({}, bridge({ available: false, text: '', log: null }).ask)
    assert.equal(outcome.status, 'failed')
    assert.match(outcome.message, /keeps nothing/)
    assert.equal(existsSync(target), false)
  }
  {
    const { main } = load({ saveTo: target })
    assert.equal((await main.save(null, bridge({ available: true, text: 'x' }).ask)).status, 'failed', 'no window, no sheet')
  }
})

test('only the two named pages open, at their literal URLs', () => {
  const { main, seen } = load()
  assert.equal(main.openPage('releases'), true)
  assert.equal(main.openPage('new-issue'), true)
  for (const bad of ['https://example.com', 'toString', '__proto__', 'constructor', undefined, 7]) {
    assert.equal(main.openPage(bad), false, String(bad))
  }
  assert.deepEqual(seen.opened, [
    'https://github.com/brave/bravebot/releases/latest',
    'https://github.com/brave/bravebot/issues/new',
  ])
})

test('the log is shown only where it is a plain file named as a log in a logs directory', async (t) => {
  const dir = scratch(t)
  const logs = join(dir, 'logs')
  mkdirSync(logs)
  const log = join(logs, '20261027T101500Z-4242.log')
  writeFileSync(log, 'line\n')
  assert.equal(load().main.isLogPath(log), true)
  for (const bad of ['logs/20261027T101500Z-4242.log', join(dir, '20261027T101500Z-4242.log'),
    join(logs, 'notes.txt'), join(logs, '20261027T101500Z-4242.log.sh'), '/etc/passwd']) {
    assert.equal(load().main.isLogPath(bad), false, bad)
  }

  const { main, seen } = load()
  assert.equal(await main.showLog(bridge({ available: true, text: 'x', log }).ask), true)
  assert.deepEqual(seen.shown, [log])

  const linked = join(logs, '20261027T101600Z-4243.log')
  symlinkSync('/etc/hosts', linked)
  const absent = join(logs, '20261027T101700Z-4244.log')
  const other = join(logs, '20261027T101800Z-4245.log')
  mkdirSync(other)
  for (const path of [linked, absent, other, '/etc/hosts']) {
    const { main: again, seen: none } = load()
    assert.equal(await again.showLog(bridge({ available: true, text: 'x', log: path }).ask), false, path)
    assert.deepEqual(none.shown, [])
  }
  assert.equal(await main.showLog(bridge({ available: false, text: '', log }).ask), false, 'nothing shown without a report')
})

test('the modal shows the privacy note and hands the main process no text, URL or path', () => {
  const modal = readFileSync('src/renderer/components/BugReport.tsx', 'utf8')
  for (const hook of ['bug-report-privacy', 'bug-report-text', 'bug-report-save', 'bug-report-copy', 'bug-report-issue', 'bug-report-log', 'bug-report-unavailable']) {
    assert.ok(modal.includes(`data-test="${hook}"`), hook)
  }
  assert.match(modal, /PRIVACY_NOTE = '[^']*Read it before you post it/)
  assert.match(modal, /saveBugReport\(\)/, 'a save carries nothing')
  assert.match(modal, /showBugReportLog\(\)/, 'nor does showing the log')
  assert.deepEqual([...modal.matchAll(/openPage\(([^)]*)\)/g)].map((m) => m[1]), ["'new-issue'"], 'and a page is named, not given')
})

test('the main process wires the window\'s asks to this module and to nothing that takes text', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /'bravebot:bug-report:save', \(\) => saveReport\(window, askBridge\)\)/)
  assert.match(index, /'bravebot:open-page', \(_event, name: unknown\) => openPage\(name\)\)/)
  const allowed = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)[1]
  assert.ok(!allowed.includes("'bug-report.compose'"), 'the window cannot state the interface version itself')
})
