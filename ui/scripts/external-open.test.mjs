// What the main process hands the system to open (SANDBOX-30): the variables the opener scripts
// read to choose a program are gone from the environment at the moment each call is made, and no
// other file in src/main calls the three opening functions of electron's `shell` directly.
//
// `xdg-open` runs the commands in `BROWSER` and reads its handler from the `XDG_` directories, and
// `gio open` executes the wrapper `GIO_LAUNCH_DESKTOP` names. A program a person approved with
// `run` may have written what those name. `shell` is stubbed here and records, for each call, which
// of the six names were still set.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const NAMES = ['BROWSER', 'GIO_LAUNCH_DESKTOP', 'XDG_DATA_HOME', 'XDG_DATA_DIRS', 'XDG_CONFIG_HOME', 'XDG_CONFIG_DIRS']

const calls = []
const electron = {
  shell: {
    openExternal: async (url) => { calls.push(['openExternal', url, still()]) },
    openPath: async (path) => { calls.push(['openPath', path, still()]); return '' },
    showItemInFolder: (path) => { calls.push(['showItemInFolder', path, still()]) },
  },
}
function still() { return NAMES.filter((name) => name in process.env) }

const source = buildSync({
  entryPoints: ['src/main/external-open.ts'],
  bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
}).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(
  (id) => (id === 'electron' ? electron : require(id)), module, module.exports)
const { OPENER_SELECTORS, removeOpenerSelectors, openExternal, openPath, showItemInFolder } = module.exports

function planted() {
  for (const name of NAMES) process.env[name] = '/stage/' + name
  process.env.EDITOR = 'vim'
  calls.length = 0
}

test('the six names are the ones the opener scripts read', () => {
  // The fault is a list that drops one, or that adds `PATH`, which the opener's own reduced search
  // path already covers and which every other program needs.
  assert.deepEqual([...OPENER_SELECTORS].sort(), [...NAMES].sort())
})

test('each of the three calls reaches electron with the names already deleted', async () => {
  // The fault is deleting once at start-up, which a later `process.env` write puts back before the
  // second call, or deleting after the call, when the script has already read them.
  planted()
  await openExternal('https://brave.com/')
  assert.deepEqual(calls.pop(), ['openExternal', 'https://brave.com/', []])
  process.env.BROWSER = '/stage/browser.sh'
  await openPath('/work/notes.txt')
  assert.deepEqual(calls.pop(), ['openPath', '/work/notes.txt', []])
  process.env.XDG_DATA_DIRS = '/stage/share'
  showItemInFolder('/work/notes.txt')
  assert.deepEqual(calls.pop(), ['showItemInFolder', '/work/notes.txt', []])
})

test('a variable that does not choose a program is left alone', () => {
  // The fault is clearing the environment, which would lose `HOME`, `DISPLAY` and the person's own.
  planted()
  assert.deepEqual(removeOpenerSelectors().sort(), [...NAMES].sort())
  assert.equal(process.env.EDITOR, 'vim')
  assert.ok(process.env.PATH)
  assert.deepEqual(removeOpenerSelectors(), [], 'nothing left to take off')
})

test('only external-open.ts calls the opening functions of electron shell', () => {
  // The fault is a new site that calls `shell.openExternal` directly, which skips the deletion.
  const offenders = []
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) walk(path)
      else if (/\.(ts|tsx)$/.test(entry.name) && entry.name !== 'external-open.ts') {
        const text = readFileSync(path, 'utf8')
        if (/\bshell\s*\.\s*(openExternal|openPath|showItemInFolder|trashItem)\b/.test(text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, ''))) offenders.push(path)
      }
    }
  }
  walk('src/main')
  assert.deepEqual(offenders, [])
})
