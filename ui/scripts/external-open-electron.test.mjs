// What `xdg-open` is started with when the real Electron on Linux opens an address or a file
// through `src/main/external-open.ts` (SANDBOX-30).
//
// `external-open.test.mjs` stubs `shell`, so it shows the six names are gone at the moment of each
// call and cannot show that Electron hands its environment to `xdg-open`. Here Electron runs under
// Xvfb with a script named `xdg-open` first on `PATH` that records which of the names it was
// started with. The same two calls on Electron's own `shell`, with no deletion, are the control:
// they must reach the script with all six names, or an empty record from the first run would say
// nothing about Electron.
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
const NAMES = ['BROWSER', 'GIO_LAUNCH_DESKTOP', 'XDG_DATA_HOME', 'XDG_DATA_DIRS', 'XDG_CONFIG_HOME', 'XDG_CONFIG_DIRS']

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

/** Run `calls` (a JavaScript body using `open`, which is `external-open.ts` or Electron's `shell`) in Electron and return, per call, the names `xdg-open` was started with. */
function started(root, mode) {
  const bin = join(root, `bin-${mode}`)
  const log = join(root, `${mode}.log`)
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
  const main = join(root, `main-${mode}.cjs`)
  writeFileSync(main, `
const { app, shell } = require('electron')
const open = ${mode === 'through' ? "require('./external-open.cjs')" : 'shell'}
app.whenReady().then(async () => {
  // Set before each call: a deletion by the first call would otherwise hide a missing one in the second.
  const plant = () => { for (const [name, value] of ${JSON.stringify(Object.entries(values))}) process.env[name] = value }
  plant()
  await open.openExternal('https://example.invalid/page')
  plant()
  await open.openPath(${JSON.stringify(text)})
  const until = Date.now() + 20000
  const done = () => require('fs').readdirSync(${JSON.stringify(log)}).length
  while (done() < 2 && Date.now() < until) await new Promise((resolve) => setTimeout(resolve, 100))
  await new Promise((resolve) => setTimeout(resolve, 500))
  app.quit()
})
`)

  const env = { ...process.env, PATH: `${bin}${delimiter}${process.env.PATH}`, HOME: join(root, 'home') }
  delete env.ELECTRON_RUN_AS_NODE
  for (const name of NAMES) delete env[name]
  const result = spawnSync('xvfb-run', ['-a', require('electron'), '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', `--user-data-dir=${join(root, `data-${mode}`)}`, main], {
    cwd: root, env, encoding: 'utf8', timeout: 90000,
  })
  assert.equal(result.status, 0, `${result.stderr}\n${result.error ?? ''}`)
  return readdirSync(log).map((file) => {
    const [call, ...names] = readFileSync(join(log, file), 'utf8').trim().split('\n')
    return { call: call.replace(/^call /, ''), names }
  }).sort((a, b) => a.call.localeCompare(b.call))
}

test('electron on linux starts xdg-open without the six names through external-open.ts and with them without it', { timeout: 240000 }, (context) => {
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

    // The fault is a deletion that leaves a name set, or one that Electron's own start of the
    // launcher does not see, so the launcher is still handed the person's value.
    const through = started(root, 'through')
    assert.deepEqual(through.map((record) => record.call.startsWith('https://') ? 'address' : 'file').sort(), ['address', 'file'])
    for (const record of through) assert.deepEqual(record.names, [], `${record.call} was started with ${record.names}`)

    // The control: Electron's `shell` starts it with all six, so the empty record above is the
    // deletion's work and not something Electron does by itself.
    const control = started(root, 'direct')
    assert.equal(control.length, 2)
    for (const record of control) assert.deepEqual(record.names.sort(), [...NAMES].sort(), `${record.call} was started with ${record.names}`)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
})
