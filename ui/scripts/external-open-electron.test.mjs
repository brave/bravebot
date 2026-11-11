// What `xdg-open` is started with when the real Electron on Linux opens an address, opens a file
// or reveals a file through `src/main/external-open.ts` (SANDBOX-30).
//
// `external-open.test.mjs` stubs `shell`, so it shows the fifteen names are gone at the moment of each
// call and cannot show that Electron hands its environment to `xdg-open`. Here Electron runs under
// Xvfb with a script named `xdg-open` first on `PATH` that records which of the names it was
// started with. The same calls on Electron's own `shell`, with no deletion, are the control: they
// must reach the script with all fifteen names, or an empty record from the first run would say
// nothing about Electron.
//
// `openExternal` and `openPath` are one test and `showItemInFolder` is another, each with its own
// Electron run. `showItemInFolder` asks the file manager over D-Bus and may run `xdg-open` on the
// folder that holds the file when that fails, so the run points `DBUS_SESSION_BUS_ADDRESS` at a
// socket that does not exist.
//
// Needs Linux, `xvfb-run` and the Electron binary `pnpm install` fetches. Anywhere else it prints
// `skipped` and passes, and under `CI` on Linux a missing `xvfb-run` or Electron fails it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
// The same names as `OPENER_SELECTORS` in `src/main/external-open.ts`, which `make check-code-selectors`
// compares with this list and with `CODE_SELECTORS` in `crates/tui/src/opener.rs`.
const NAMES = [
  'BROWSER', 'GIO_LAUNCH_DESKTOP', 'XDG_DATA_HOME', 'XDG_DATA_DIRS', 'XDG_CONFIG_HOME', 'XDG_CONFIG_DIRS',
  'GIO_EXTRA_MODULES', 'GIO_MODULE_DIR', 'QT_PLUGIN_PATH', 'QT_QPA_PLATFORM_PLUGIN_PATH', 'QT_QPA_PLATFORMTHEME',
  'QT_QPA_GENERIC_PLUGINS', 'QT_QPA_PLATFORM', 'QT_STYLE_OVERRIDE', 'KDESYCOCA',
]

function unavailable() {
  if (process.platform !== 'linux') return 'not Linux'
  const missing = []
  if (spawnSync('xvfb-run', ['--help'], { stdio: 'ignore' }).error) missing.push('xvfb-run')
  let electron = ''
  try { electron = require('electron') } catch { /* not installed */ }
  if (typeof electron !== 'string' || !existsSync(electron)) missing.push('the electron binary')
  if (missing.length === 0) return ''
  if (process.env.CI) throw new Error(`missing ${missing.join(' and ')}`)
  return `no ${missing.join(' and ')}`
}

/** Run the calls named by `calls` (`external`: `openExternal` then `openPath`; `folder`: `showItemInFolder`) on `open` (`external-open.ts` or Electron's `shell`) in Electron and return, per start of `xdg-open`, its argument and the names it was started with. */
function started(root, mode, calls) {
  const bin = join(root, `bin-${mode}-${calls}`)
  const log = join(root, `${mode}-${calls}.log`)
  mkdirSync(bin)
  mkdirSync(log)
  const script = join(bin, 'xdg-open')
  // One file per start: the two starts overlap, and a shared file would interleave their lines.
  writeFileSync(script, `#!/bin/sh\n{ printf 'call %s\\n' "$1"; env | grep -E '^(${NAMES.join('|')})=' | cut -d= -f1; } > '${log}/'$$\n`)
  chmodSync(script, 0o755)

  const text = join(root, 'notes.txt')
  writeFileSync(text, 'a text file\n')
  const values = {}
  for (const name of NAMES) {
    values[name] = join(root, `value-${name}`)
    mkdirSync(values[name], { recursive: true })
  }
  const main = join(root, `main-${mode}-${calls}.cjs`)
  const body = calls === 'external'
    ? `await open.openExternal('https://example.invalid/page')
  plant()
  await open.openPath(${JSON.stringify(text)})`
    : `open.showItemInFolder(${JSON.stringify(text)})`
  const expected = calls === 'external' ? 2 : 1
  const patience = calls === 'external' ? 20000 : 8000
  writeFileSync(main, `
const { app, shell } = require('electron')
const open = ${mode === 'through' ? "require('./external-open.cjs')" : 'shell'}
app.whenReady().then(async () => {
  // Set before each call: a deletion by the first call would otherwise hide a missing one in the second.
  const plant = () => { for (const [name, value] of ${JSON.stringify(Object.entries(values))}) process.env[name] = value }
  plant()
  ${body}
  const until = Date.now() + ${patience}
  const done = () => require('fs').readdirSync(${JSON.stringify(log)}).length
  while (done() < ${expected} && Date.now() < until) await new Promise((resolve) => setTimeout(resolve, 100))
  await new Promise((resolve) => setTimeout(resolve, 500))
  app.quit()
})
`)

  const env = { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, HOME: join(root, 'home'), DBUS_SESSION_BUS_ADDRESS: `unix:path=${join(root, 'no-bus')}` }
  delete env.ELECTRON_RUN_AS_NODE
  for (const name of NAMES) delete env[name]
  const result = spawnSync('xvfb-run', ['-a', require('electron'), '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', `--user-data-dir=${join(root, `data-${mode}-${calls}`)}`, main], {
    cwd: root, env, encoding: 'utf8', timeout: 90000,
  })
  assert.equal(result.status, 0, `${result.stderr}\n${result.error ?? ''}`)
  return readdirSync(log).map((file) => {
    const [call, ...names] = readFileSync(join(log, file), 'utf8').trim().split('\n')
    return { call: call.replace(/^call /, ''), names }
  }).sort((a, b) => a.call.localeCompare(b.call))
}

/** What a start of `xdg-open` was asked to open: an address or the file `openPath` was given. */
function kind(record) {
  return record.call.startsWith('https://') ? 'address' : 'file'
}

/** Run `body` with a scratch root holding the bundled `external-open.ts`, or skip when Electron cannot run here. */
function inElectronRoot(context, body) {
  const why = unavailable()
  if (why) {
    console.log(`skipped: ${why}`)
    context.skip(why)
    return
  }
  const root = mkdtempSync(join(tmpdir(), 'external-open-electron-'))
  try {
    mkdirSync(join(root, 'home'))
    writeFileSync(join(root, 'external-open.cjs'), buildSync({
      entryPoints: ['src/main/external-open.ts'],
      bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
    }).outputFiles[0].text)
    body(root)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

test('electron on linux starts xdg-open without the fifteen names for openExternal and openPath through external-open.ts and with them without it', { timeout: 240000 }, (context) => {
  inElectronRoot(context, (root) => {
    // The fault is a deletion that leaves a name set, or one that Electron's own start of the
    // launcher does not see, so the launcher is still handed the person's value.
    const through = started(root, 'through', 'external')
    assert.deepEqual(through.map(kind).sort(), ['address', 'file'])
    for (const record of through) assert.deepEqual(record.names, [], `${record.call} was started with ${record.names}`)

    // The control: Electron's `shell` starts it with all fifteen, so the empty record above is the
    // deletion's work and nothing Electron does by itself.
    const control = started(root, 'direct', 'external')
    assert.deepEqual(control.map(kind).sort(), ['address', 'file'])
    for (const record of control) assert.deepEqual(record.names.sort(), [...NAMES].sort(), `${record.call} was started with ${record.names}`)
  })
})

// How many times Electron starts `xdg-open` for `showItemInFolder` is Electron's choice, so no count
// is asserted, but each run must start it at least once: a run with no start would pass the checks
// below with nothing in them. Every start through `external-open.ts` must carry none of the names,
// and every start of the control must carry all fifteen.
test('electron on linux starts any xdg-open for showItemInFolder without the fifteen names through external-open.ts and with them without it', { timeout: 240000 }, (context) => {
  inElectronRoot(context, (root) => {
    const control = started(root, 'direct', 'folder')
    assert.notEqual(control.length, 0, 'showItemInFolder started no xdg-open on Electron\'s own shell, so the run says nothing about the deletion')
    for (const record of control) assert.deepEqual(record.names.sort(), [...NAMES].sort(), `${record.call} was started with ${record.names}`)

    const through = started(root, 'through', 'folder')
    assert.notEqual(through.length, 0, 'showItemInFolder through external-open.ts started no xdg-open')
    for (const record of through) assert.deepEqual(record.names, [], `${record.call} was started with ${record.names}`)
  })
})
