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
const buses = []
const electron = {
  shell: {
    openExternal: async (url) => { calls.push(['openExternal', url, still()]); buses.push(bus()) },
    openPath: async (path) => { calls.push(['openPath', path, still()]); buses.push(bus()); return '' },
    showItemInFolder: (path) => { calls.push(['showItemInFolder', path, still()]); buses.push(bus()) },
  },
}
function still() { return NAMES.filter((name) => name in process.env) }
/** The session bus names still set, with their values, at the moment of a call. */
function bus() { return Object.fromEntries(SESSION_BUS.filter((name) => name in process.env).map((name) => [name, process.env[name]])) }
const SESSION_BUS = ['XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS']

const source = buildSync({
  entryPoints: ['src/main/external-open.ts'],
  bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
}).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(
  (id) => (id === 'electron' ? electron : require(id)), module, module.exports)
const { OPENER_SELECTORS, FILE_NAME_PREFIXES, SESSION_BUS_NAMES, removeOpenerSelectors, useOpenerJudge, openExternal, openPath, showItemInFolder } = module.exports

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
  await showItemInFolder('/work/notes.txt')
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

test('the home is left set, and the session bus names are not in the list deleted whatever they hold', () => {
  // The fault is deleting `HOME` here, which takes it from the agent this process starts, or listing
  // the two session bus names with the others, which deletes the person's own bus whatever it holds
  // and takes it from Electron. They are judged by the agent instead (the tests below).
  const before = ['HOME', ...SESSION_BUS].map((name) => process.env[name])
  try {
    planted()
    for (const name of ['HOME', ...SESSION_BUS]) process.env[name] = '/stage/' + name
    removeOpenerSelectors()
    for (const name of ['HOME', ...SESSION_BUS]) assert.equal(process.env[name], '/stage/' + name, `${name} was deleted`)
    assert.deepEqual(OPENER_SELECTORS.filter((name) => ['HOME', ...SESSION_BUS].includes(name)), [])
    assert.deepEqual([...SESSION_BUS_NAMES], SESSION_BUS)
  } finally {
    ;['HOME', ...SESSION_BUS].forEach((name, index) => {
      if (before[index] === undefined) delete process.env[name]
      else process.env[name] = before[index]
    })
  }
})

/** Run `body` with the two session bus names set to `values`, and put the environment back. */
async function withBus(values, body) {
  const before = SESSION_BUS.map((name) => process.env[name])
  try {
    for (const name of SESSION_BUS) delete process.env[name]
    Object.assign(process.env, values)
    calls.length = 0
    buses.length = 0
    await body()
  } finally {
    SESSION_BUS.forEach((name, index) => {
      if (before[index] === undefined) delete process.env[name]
      else process.env[name] = before[index]
    })
    useOpenerJudge(null)
  }
}

const bothSet = { XDG_RUNTIME_DIR: '/stage/run', DBUS_SESSION_BUS_ADDRESS: 'unix:path=/stage/run/bus' }
const everyCall = [
  () => openExternal('https://brave.com/'),
  () => openPath('/work/notes.txt'),
  () => showItemInFolder('/work/notes.txt'),
]

test('a session bus name the agent lists is deleted before each of the three calls and one it does not list is kept', async () => {
  // The faults are a deletion that ignores the answer (both gone whatever they hold, or both kept),
  // one that deletes after the call, and one that handles only `openExternal`.
  await withBus(bothSet, async () => {
    for (const call of everyCall) {
      Object.assign(process.env, bothSet)
      useOpenerJudge(async () => ({ remove: ['DBUS_SESSION_BUS_ADDRESS'] }))
      await call()
      assert.deepEqual(buses.pop(), { XDG_RUNTIME_DIR: '/stage/run' })
      Object.assign(process.env, bothSet)
      useOpenerJudge(async () => ({ remove: ['XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS'] }))
      await call()
      assert.deepEqual(buses.pop(), {})
      Object.assign(process.env, bothSet)
      useOpenerJudge(async () => ({ remove: [] }))
      await call()
      assert.deepEqual(buses.pop(), bothSet)
    }
  })
})

test('the agent is asked about the two session bus names and nothing else, and only when one is set', async () => {
  // The fault is sending the whole environment, which hands the agent every variable the person
  // has, or asking when nothing is held.
  await withBus({ XDG_RUNTIME_DIR: '/run/user/1' }, async () => {
    const asked = []
    useOpenerJudge(async (method, params) => { asked.push([method, params]); return { remove: [] } })
    process.env.EDITOR = 'vim'
    await openExternal('https://brave.com/')
    assert.deepEqual(asked, [['opener.judge', { XDG_RUNTIME_DIR: '/run/user/1' }]])
    delete process.env.XDG_RUNTIME_DIR
    await openExternal('https://brave.com/')
    assert.equal(asked.length, 1, 'asked with nothing held')
  })
})

test('a session bus name is deleted when the agent cannot be asked or its answer cannot be read', async () => {
  // The fault is keeping a name nothing has judged, which is the case the check exists for.
  const answers = [
    null,
    async () => { throw new Error('the agent is not running') },
    () => { throw new Error('sync') },
    async () => null,
    async () => ({}),
    async () => ({ remove: 'XDG_RUNTIME_DIR' }),
    async () => ({ remove: [7] }),
    async () => ({ remove: ['HOME'] }),
  ]
  await withBus(bothSet, async () => {
    for (const answer of answers) {
      Object.assign(process.env, bothSet)
      useOpenerJudge(answer)
      await openExternal('https://brave.com/')
      assert.deepEqual(buses.pop(), {}, String(answer))
    }
  })
})

test('a session bus name is deleted when the agent does not answer in time, and the call still goes through', async () => {
  // The fault is waiting on the agent without a bound, so one request that never answers stops every
  // link and every show-in-folder behind it. The call must reach electron with both names deleted,
  // and an answer that arrives after the bound must not put them back or throw.
  await withBus(bothSet, async () => {
    let late
    useOpenerJudge(
      () => new Promise((resolve) => { late = () => resolve({ remove: [] }) }),
      20,
    )
    const started = Date.now()
    for (const call of everyCall) {
      Object.assign(process.env, bothSet)
      // A call that is still waiting after a second fails the test and does not hang it.
      let guard
      const stuck = new Promise((_, reject) => {
        guard = setTimeout(() => reject(new Error('a call waited for an agent that never answered')), 1000)
      })
      try {
        await Promise.race([call(), stuck])
      } finally {
        clearTimeout(guard)
      }
      assert.deepEqual(buses.pop(), {})
    }
    assert.ok(Date.now() - started < 1000, 'a call waited for an agent that never answered')
    late()
    await new Promise((resolve) => setTimeout(resolve, 10))
    assert.deepEqual(buses.length, 0)
  })
})

test('an answer inside the bound is used and the bound is not left running', async () => {
  // The fault is a bound that fires after a prompt answer and deletes the names it kept, or that
  // leaves a timer that holds the process open.
  await withBus(bothSet, async () => {
    useOpenerJudge(async () => ({ remove: [] }), 20)
    await openExternal('https://brave.com/')
    assert.deepEqual(buses.pop(), bothSet)
    await new Promise((resolve) => setTimeout(resolve, 60))
    assert.deepEqual(bus(), bothSet)
  })
})

test('a name the agent lists that is not a session bus name is not deleted from the environment', async () => {
  // `HOME` stays set whatever the agent says, so a bad answer cannot take what the agent and
  // Electron read.
  const home = process.env.HOME
  try {
    await withBus({}, async () => {
      process.env.HOME = '/stage/home'
      useOpenerJudge(async () => ({ remove: ['HOME'] }))
      await openExternal('https://brave.com/')
      assert.equal(process.env.HOME, '/stage/home')
    })
  } finally {
    if (home === undefined) delete process.env.HOME
    else process.env.HOME = home
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
