// What the main process hands the system to open (SANDBOX-30): the variables the opener scripts
// read to choose a program are gone from the environment at the moment each call is made, and no
// other file in src/main calls the three opening functions of electron's `shell` directly.
//
// `xdg-open` runs the commands in `BROWSER` and reads its handler from the `XDG_` directories, and
// `gio open` executes the wrapper `GIO_LAUNCH_DESKTOP` names and loads modules from
// `GIO_EXTRA_MODULES` and `GIO_MODULE_DIR`, and `kde-open` loads Qt plug-ins and a service cache.
// A program a person approved with
// `run` may have written what those name. `shell` is stubbed here and records, for each call, which
// of the sixteen names were still set.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const NAMES = [
  'BROWSER', 'GIO_LAUNCH_DESKTOP', 'XDG_DATA_HOME', 'XDG_DATA_DIRS', 'XDG_CONFIG_HOME', 'XDG_CONFIG_DIRS',
  'XDG_CACHE_HOME', 'GIO_EXTRA_MODULES', 'GIO_MODULE_DIR', 'QT_PLUGIN_PATH', 'QT_QPA_PLATFORM_PLUGIN_PATH', 'QT_QPA_PLATFORMTHEME',
  'QT_QPA_GENERIC_PLUGINS', 'QT_QPA_PLATFORM', 'QT_STYLE_OVERRIDE', 'KDESYCOCA',
]

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
const { OPENER_SELECTORS, FILE_NAME_PREFIXES, removeOpenerSelectors, openExternal, openPath, showItemInFolder } = module.exports

function planted() {
  for (const name of NAMES) process.env[name] = '/stage/' + name
  process.env.EDITOR = 'vim'
  calls.length = 0
}

test('the sixteen names are the ones the opener scripts read', () => {
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
  // The fault is clearing the environment, which would lose `HOME`, `DISPLAY` and the person's own,
  // or listing `HOME`, which SANDBOX-30 leaves set for the opener.
  planted()
  const home = process.env.HOME
  process.env.HOME = '/stage/home'
  try {
    assert.deepEqual(removeOpenerSelectors().sort(), [...NAMES].sort())
    assert.equal(process.env.EDITOR, 'vim')
    assert.equal(process.env.HOME, '/stage/home')
    assert.ok(process.env.PATH)
    assert.deepEqual(removeOpenerSelectors(), [], 'nothing left to take off')
  } finally {
    if (home === undefined) delete process.env.HOME
    else process.env.HOME = home
  }
})

test('the home, runtime directory and session bus address are left set for the application', () => {
  // The fault is deleting them here, which takes the session bus from Electron and the runtime
  // directory from the agent it starts. The terminal client's opener judges them instead.
  const kept = ['HOME', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS']
  const before = kept.map((name) => process.env[name])
  try {
    planted()
    for (const name of kept) process.env[name] = '/stage/' + name
    removeOpenerSelectors()
    for (const name of kept) assert.equal(process.env[name], '/stage/' + name, `${name} was deleted`)
    assert.deepEqual(OPENER_SELECTORS.filter((name) => kept.includes(name)), [])
  } finally {
    kept.forEach((name, index) => {
      if (before[index] === undefined) delete process.env[name]
      else process.env[name] = before[index]
    })
  }
})

test('a file-name prefix is deleted only when it has a slash, by the pure function and before each call', async () => {
  // `xdg-mime` joins `XDG_CURRENT_DESKTOP` and `XDG_MENU_PREFIX` to a file name without checking
  // them, so `../../stage/evil` reads a file outside the judged directories. The fault is a list
  // that leaves them out, or one that deletes `XDG_CURRENT_DESKTOP` always, which hides the desktop
  // from `xdg-open`.
  assert.deepEqual([...FILE_NAME_PREFIXES].sort(), ['XDG_CURRENT_DESKTOP', 'XDG_MENU_PREFIX'])
  const env = { XDG_CURRENT_DESKTOP: '../../stage/evil', XDG_MENU_PREFIX: '../../../stage/', EDITOR: 'vim' }
  assert.deepEqual(removeOpenerSelectors(env).sort(), ['XDG_CURRENT_DESKTOP', 'XDG_MENU_PREFIX'])
  assert.deepEqual(env, { EDITOR: 'vim' })
  const kept = { XDG_CURRENT_DESKTOP: 'GNOME', XDG_MENU_PREFIX: 'gnome-' }
  assert.deepEqual(removeOpenerSelectors(kept), [])
  assert.deepEqual(kept, { XDG_CURRENT_DESKTOP: 'GNOME', XDG_MENU_PREFIX: 'gnome-' })
  const colon = { XDG_CURRENT_DESKTOP: 'GNOME:../x' }
  assert.deepEqual(removeOpenerSelectors(colon), ['XDG_CURRENT_DESKTOP'], 'one entry of a : list with a slash')

  // Through the three calls, on the real `process.env`.
  for (const name of FILE_NAME_PREFIXES) delete process.env[name]
  const seen = () => FILE_NAME_PREFIXES.filter((name) => name in process.env)
  for (const call of [() => openExternal('https://brave.com/'), () => openPath('/work/a'), async () => showItemInFolder('/work/a')]) {
    process.env.XDG_CURRENT_DESKTOP = '../../stage/evil'
    process.env.XDG_MENU_PREFIX = '../../../stage/'
    await call()
    assert.deepEqual(seen(), [])
  }
  process.env.XDG_CURRENT_DESKTOP = 'GNOME'
  await openExternal('https://brave.com/')
  assert.equal(process.env.XDG_CURRENT_DESKTOP, 'GNOME')
  delete process.env.XDG_CURRENT_DESKTOP
  calls.length = 0
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
