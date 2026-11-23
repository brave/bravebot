// One instance of the app per profile. `src/main/single-instance.ts` is bundled and driven with a
// fake `app` and fake windows, and `src/main/index.ts` is read as text for the wiring a fake cannot
// reach: that the lock is taken before anything that makes a window or starts the agent, and that
// nothing runs for a launch that did not get it. The processes themselves are started twice on one
// profile by `single-instance-electron.test.mjs`, which runs the real Electron.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { build } from 'esbuild'

const bundled = await build({
  entryPoints: ['src/main/single-instance.ts'],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
}).then((result) => result.outputFiles[0].text)

function load() {
  const module = { exports: {} }
  new Function('module', 'exports', bundled)(module, module.exports)
  return module.exports
}
const { takeSingleInstance, raiseMainWindow } = load()

/** An `app` that records what it was asked, with the lock held by another process when `held`. */
function fakeApp(held, ready = true) {
  const calls = []
  const listeners = []
  return {
    calls,
    listeners,
    requestSingleInstanceLock: (...args) => { calls.push(['lock', args.length]); return !held },
    quit: () => calls.push(['quit']),
    isReady: () => ready,
    on: (event, listener) => { calls.push(['on', event]); listeners.push(listener) },
    show: () => calls.push(['app.show']),
    focus: (options) => calls.push(['app.focus', options]),
  }
}

class FakeWindow {
  constructor(order, { minimized = false, destroyed = false } = {}) {
    this.order = order
    this.minimized = minimized
    this.destroyed = destroyed
  }
  isDestroyed() { return this.destroyed }
  isMinimized() { return this.minimized }
  restore() { this.minimized = false; this.order.push('restore') }
  show() { this.order.push('show') }
  focus() { this.order.push('focus') }
}

/**
 * A launch that finds the lock held has to end and start nothing. The caller starts the window and
 * the agent only when this answers true, so the answer is what is asserted, with the quit.
 */
test('a launch that does not get the lock quits, answers false, and listens for nothing', () => {
  const app = fakeApp(true)
  let activated = 0
  assert.equal(takeSingleInstance(app, () => { activated += 1 }), false)
  assert.deepEqual(app.calls, [['lock', 0], ['quit']])
  assert.equal(activated, 0)
})

test('a launch that gets the lock does not quit, answers true, and listens for a second launch', () => {
  const app = fakeApp(false)
  assert.equal(takeSingleInstance(app, () => {}), true)
  assert.deepEqual(app.calls, [['lock', 0], ['on', 'second-instance']])
})

/**
 * What the second process passed is untrusted: nothing may take it as a parameter. The listener
 * is called the way Electron calls it, with an event, the second process's argv, its working
 * directory and its additional data, and `activate` must be called with no argument at all.
 */
test('a second launch activates with none of what the second process passed', () => {
  const app = fakeApp(false)
  const seen = []
  takeSingleInstance(app, (...args) => seen.push(args))
  const hostile = ['/a/folder/to/open', '--user-data-dir=/etc', 'paste this into the composer']
  app.listeners[0]({ preventDefault() {} }, hostile, '/a/folder/to/open', { directory: '/a/folder/to/open', text: 'paste this' })
  assert.deepEqual(seen, [[]])
})

test('two second launches activate twice, once each', () => {
  const app = fakeApp(false)
  let activated = 0
  takeSingleInstance(app, () => { activated += 1 })
  app.listeners[0]({}, [], '', undefined)
  app.listeners[0]({}, [], '', undefined)
  assert.equal(activated, 2)
})

/**
 * Electron documents `second-instance` as emitted after `ready`, but a launch heard earlier would
 * build a window before the IPC handlers and the window opener exist. The first instance builds
 * its own window when it is ready, so an early one is dropped.
 */
test('a second launch heard before the app is ready activates nothing', () => {
  const app = fakeApp(false, false)
  let activated = 0
  takeSingleInstance(app, () => { activated += 1 })
  app.listeners[0]({}, [], '', undefined)
  assert.equal(activated, 0)
})

/** A window that is minimised and only focused stays minimised, so it is restored first. */
test('a minimised main window is restored, shown and focused, in that order', () => {
  const order = []
  const window = new FakeWindow(order, { minimized: true })
  let created = 0
  raiseMainWindow(fakeApp(false), window, () => { created += 1 }, 'linux')
  assert.deepEqual(order, ['restore', 'show', 'focus'])
  assert.equal(window.minimized, false)
  assert.equal(created, 0, 'a main window that exists is not replaced')
})

test('a hidden or background main window is shown and focused without being restored', () => {
  const order = []
  raiseMainWindow(fakeApp(false), new FakeWindow(order), () => {}, 'linux')
  assert.deepEqual(order, ['show', 'focus'])
})

test('on macOS the app is made active so a window behind another app comes forward', () => {
  const app = fakeApp(false)
  raiseMainWindow(app, new FakeWindow([]), () => {}, 'darwin')
  assert.deepEqual(app.calls, [['app.show'], ['app.focus', { steal: true }]])
})

/** Off macOS the app-level focus picks a window of its own choosing, which can be a small one. */
test('off macOS only the main window is touched, not the app', () => {
  for (const platform of ['linux', 'win32']) {
    const app = fakeApp(false)
    raiseMainWindow(app, new FakeWindow([]), () => {}, platform)
    assert.deepEqual(app.calls, [], platform)
  }
})

/** The macOS state after the last window closed: the app is alive and a second launch must give a window. */
test('with no main window a second launch builds one and touches nothing else', () => {
  for (const window of [null, new FakeWindow([], { destroyed: true })]) {
    const app = fakeApp(false)
    let created = 0
    raiseMainWindow(app, window, () => { created += 1 }, 'darwin')
    assert.equal(created, 1)
    assert.deepEqual(app.calls, [])
  }
})

test('repeated second launches with a window open build no further window', () => {
  const window = new FakeWindow([])
  let created = 0
  for (let launch = 0; launch < 3; launch += 1) raiseMainWindow(fakeApp(false), window, () => { created += 1 }, 'darwin')
  assert.equal(created, 0)
})

const index = readFileSync('src/main/index.ts', 'utf8')
const helper = readFileSync('src/main/single-instance.ts', 'utf8')

/**
 * A lock requested inside `whenReady`, or after the window is built, lets the losing process make
 * a window and start `bravebot-rpc` before it quits. The lock is the first thing the module does
 * after its imports, and `whenReady` is not waited on without it.
 */
test('the lock is taken before any window, agent or state is started, and startup waits on it', () => {
  const lock = index.indexOf('takeSingleInstance(app,')
  assert.notEqual(lock, -1)
  for (const later of ['new SignIn', 'new KeyEntry', 'function createWindow', 'new BrowserWindow', 'new Bridge(', 'readState(', 'createWindow()', 'app.whenReady()']) {
    const at = index.indexOf(later)
    assert.ok(at === -1 || at > lock, `${later} comes after the lock`)
  }
  assert.equal(index.match(/app\.whenReady\(\)/g).length, 1)
  assert.match(index, /if \(primary\) app\.whenReady\(\)\.then\(/)
})

/**
 * `BrowserWindow.getAllWindows()[0]` raises whichever window Electron lists first, which can be a
 * sign-in, key or print window. The second launch raises the window `index.ts` holds as its main
 * one, and the helper reads nothing the second process sent.
 */
test('a second launch raises the main window held by index.ts and reads no window list or launch argument', () => {
  assert.match(index, /takeSingleInstance\(app, \(\) => raiseMainWindow\(app, window, createWindow\)\)/)
  assert.doesNotMatch(helper, /getAllWindows|getFocusedWindow|argv|commandLine|workingDirectory|additionalData/)
  assert.doesNotMatch(index, /second-instance/)
})

/** An exception for an unpackaged run would leave `pnpm run dev` and `electron .` open twice. */
test('the lock does not depend on whether the app is packaged', () => {
  assert.doesNotMatch(helper, /isPackaged/)
  const around = index.slice(index.indexOf('const primary'), index.indexOf('const primary') + 200)
  assert.doesNotMatch(around, /isPackaged/)
})
