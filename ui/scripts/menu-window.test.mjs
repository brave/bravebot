// The application menu when the window it delivers to has closed. On macOS closing the last
// window leaves the app and its menu bar running, so every item stays wired to a window that is
// gone. `src/main/menu.ts` is bundled with `electron` replaced by a fake whose destroyed window
// throws what Electron throws ("Object has been destroyed") when its `webContents` is read, so a
// command sent to a dead window fails here as it does in the app.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { build } from 'esbuild'

const require = createRequire(import.meta.url)

const bundled = await build({
  entryPoints: ['src/main/menu.ts'],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'cjs',
  external: ['electron'],
  plugins: [
    {
      name: 'recents-from-the-fake',
      setup(built) {
        built.onResolve({ filter: /^\.\/recents$/ }, () => ({ path: 'recents', namespace: 'fake-recents' }))
        built.onLoad({ filter: /.*/, namespace: 'fake-recents' }, () => ({
          loader: 'js',
          contents: "module.exports = { recents: () => require('electron').recentList }",
        }))
      },
    },
  ],
}).then((result) => result.outputFiles[0].text)

class FakeWindow {
  constructor() {
    this.destroyed = false
    this.sent = []
    this.shown = false
  }

  isDestroyed() {
    return this.destroyed
  }

  get webContents() {
    if (this.destroyed) throw new TypeError('Object has been destroyed')
    return { isDestroyed: () => false, send: (...args) => this.sent.push(args) }
  }

  destroy() {
    this.destroyed = true
  }
}

function fakeMenu(template) {
  const make = (options) => ({
    ...options,
    enabled: options.enabled ?? true,
    click: options.click ? () => options.click(menu_item(options)) : undefined,
    submenu: options.submenu ? { items: options.submenu.map(make) } : undefined,
  })
  const menu_item = (options) => options
  const items = template.map(make)
  const everything = (list) => list.flatMap((each) => [each, ...everything(each.submenu?.items ?? [])])
  return {
    items,
    getMenuItemById: (id) => everything(items).find((each) => each.id === id) ?? null,
    everything: () => everything(items),
    popup(options) {
      electron.popups.push(options?.window)
    },
  }
}

let electron

/** A fresh copy of the module, so each test starts with the menu as the app first has it. */
function load() {
  let installed = null
  electron = {
    app: { isPackaged: false },
    BrowserWindow: FakeWindow,
    dialog: {},
    shell: {},
    recentList: ['/work/project-a'],
    popups: [],
    Menu: {
      setApplicationMenu: (menu) => { installed = menu },
      getApplicationMenu: () => installed,
      buildFromTemplate: fakeMenu,
    },
  }
  const module = { exports: {} }
  new Function('require', 'module', 'exports', bundled)(
    (name) => (name === 'electron' ? electron : require(name)),
    module,
    module.exports,
  )
  const menu = module.exports
  const windows = []
  // What the app's own window builder does: a window is made, and the menu is installed on it.
  const open = () => {
    const window = new FakeWindow()
    windows.push(window)
    menu.installMenu(window)
  }
  menu.useWindowOpener(open)
  open()
  const click = (id) => electron.Menu.getApplicationMenu().getMenuItemById(id).click()
  const item = (id) => electron.Menu.getApplicationMenu().getMenuItemById(id)
  const close = (window) => { window.destroy(); menu.windowClosed(window) }
  return { menu, windows, click, item, close }
}

const BUSY = {
  hasSession: true,
  running: true,
  canSend: true,
  canStart: true,
  canExport: true,
  includeTools: false,
  canRewind: true,
  caffeinated: false,
  folded: { left: false, right: false },
}

const NEEDS_A_WINDOW = [
  'session.close', 'turn.send', 'turn.cancel', 'view.find', 'view.focus-composer', 'mode.cycle',
  'turn.rewind', 'session.note', 'session.export-text', 'session.export-markdown',
  'session.export-pdf', 'session.export-tools', 'view.fold-left', 'view.fold-right',
  'view.reset-columns', 'view.theme', 'app.about', 'help.doctor', 'app.sections',
]

/**
 * The reported failure: File > New Chat after the window was closed threw "Object has been
 * destroyed" from the menu's click handler. A fix that only swallows the throw leaves New Chat
 * doing nothing, so the chat has to start in a window built for it.
 */
test('New Chat with every window closed builds one window and delivers the command once its page has subscribed', () => {
  const { menu, windows, click, close } = load()
  close(windows[0])

  assert.doesNotThrow(() => click('session.new'))
  assert.equal(windows.length, 2, 'one new window was built')
  assert.deepEqual(windows[0].sent, [], 'nothing went to the closed window')
  assert.deepEqual(windows[1].sent, [], 'the new page has not subscribed, so the command is held')

  menu.commandsReady()
  assert.deepEqual(windows[1].sent, [['bravebot:command', 'session.new', null]])

  menu.commandsReady()
  assert.equal(windows[1].sent.length, 1, 'a second subscription does not repeat the command')
})

/**
 * Sending the command as soon as the window exists loses it, because the page loads and
 * subscribes later. Choosing New Chat twice before the page is ready must not build two windows.
 */
test('a second New Chat while the new window is loading builds no further window', () => {
  const { menu, windows, click, close } = load()
  close(windows[0])
  click('session.new')
  click('session.new')
  assert.equal(windows.length, 2)
  menu.commandsReady()
  assert.equal(windows[1].sent.length, 1)
})

/** The check has to be on the window itself: the app had not yet been told it closed. */
test('a command chosen for a destroyed window that the menu was not told about throws nothing and is not delivered to it', () => {
  const { windows, click } = load()
  windows[0].destroy()
  assert.doesNotThrow(() => click('turn.send'))
  assert.doesNotThrow(() => click('session.new'))
  assert.deepEqual(windows[0].sent, [])
})

test('commands that act on a window do nothing when no window is open, and build no window', () => {
  const { windows, click, close } = load()
  close(windows[0])
  for (const id of NEEDS_A_WINDOW) assert.doesNotThrow(() => click(id), id)
  assert.equal(windows.length, 1, 'none of them built a window')
  assert.deepEqual(windows[0].sent, [])
})

test('closing the window greys the items that need one and keeps the always-available ones enabled', () => {
  const { menu, windows, item, close } = load()
  menu.refreshMenu(BUSY)
  for (const id of ['session.close', 'turn.send', 'turn.cancel', 'turn.rewind', 'session.export-pdf']) {
    assert.equal(item(id).enabled, true, `${id} is enabled while a chat runs`)
  }

  close(windows[0])
  for (const id of ['session.close', 'turn.send', 'turn.cancel', 'turn.rewind', 'session.export-pdf', 'view.find', 'mode.cycle', 'session.note']) {
    assert.equal(item(id).enabled, false, `${id} is grey with no window`)
  }
  for (const id of ['session.new', 'app.about', 'view.theme']) {
    assert.equal(item(id).enabled, true, `${id} stays enabled with no window`)
  }
})

/** The closing page can publish one last state after the close, which must not undo the reset. */
test('a state published after the window closed does not enable items again', () => {
  const { menu, windows, item, close } = load()
  close(windows[0])
  menu.refreshMenu(BUSY)
  assert.equal(item('turn.send').enabled, false)
  assert.equal(item('session.close').enabled, false)
})

test('a window built after the last one closed receives later commands and Open Recent choices, through a second close and reopen', () => {
  const { menu, windows, click, item, close } = load()
  const recent = () => electron.Menu.getApplicationMenu().everything().find((each) => each.label === 'project-a')

  for (const cycle of [1, 2]) {
    const closing = windows.at(-1)
    close(closing)
    click('session.new')
    menu.commandsReady()
    const fresh = windows.at(-1)
    assert.equal(windows.length, cycle + 1)
    assert.notEqual(fresh, closing)
    assert.deepEqual(fresh.sent, [['bravebot:command', 'session.new', null]], `cycle ${cycle}: New Chat reached the new window`)

    menu.refreshMenu(BUSY)
    click('turn.send')
    recent().click()
    assert.deepEqual(fresh.sent.slice(1), [
      ['bravebot:command', 'turn.send', null],
      ['bravebot:command', 'session.new-here', { target: 'directory', id: '/work/project-a' }],
    ], `cycle ${cycle}: later commands and Open Recent reached the new window`)
    assert.equal(item('turn.send').enabled, true)
  }
  assert.deepEqual(windows[0].sent, [], 'the first window was never sent to after it closed')
  assert.equal(windows[1].sent.length, 3, 'the second window was sent to only while it was the current one')
})

test('Open Recent with every window closed builds a window and delivers the folder the menu offered', () => {
  const { menu, windows, close } = load()
  const recent = electron.Menu.getApplicationMenu().everything().find((each) => each.label === 'project-a')
  close(windows[0])
  assert.doesNotThrow(() => recent.click())
  menu.commandsReady()
  assert.deepEqual(windows[1].sent, [['bravebot:command', 'session.new-here', { target: 'directory', id: '/work/project-a' }]])
})

/** Windows close in an order the menu does not control: the old one's close can come late. */
test('the close of an earlier window does not unset the window built after it', () => {
  const { menu, windows, click, close } = load()
  close(windows[0])
  click('session.new')
  menu.commandsReady()
  menu.windowClosed(windows[0])
  assert.doesNotThrow(() => click('view.find'))
  menu.refreshMenu(BUSY)
  click('view.find')
  assert.deepEqual(windows[1].sent.at(-1), ['bravebot:command', 'view.find', null])
})

test('a context menu and a rebuild with no window or a destroyed one throw nothing and never pop up on a destroyed window', () => {
  const { menu, windows, close } = load()
  const reference = { target: 'session', id: 'abc' }

  windows[0].destroy()
  assert.doesNotThrow(() => menu.popupContext(reference))
  assert.doesNotThrow(() => menu.rebuildMenu())
  assert.deepEqual(electron.popups, [])

  menu.windowClosed(windows[0])
  assert.doesNotThrow(() => menu.popupContext(reference))
  assert.doesNotThrow(() => menu.rebuildMenu())
  assert.deepEqual(electron.popups, [])

  const fresh = new FakeWindow()
  menu.installMenu(fresh)
  menu.popupContext(reference)
  assert.deepEqual(electron.popups, [fresh], 'a live window still gets its context menu')
  close(fresh)
})

test('a close reported for a window the menu is delivering to stops delivery to it', () => {
  const { menu, windows, click } = load()
  menu.windowClosed(windows[0])
  assert.doesNotThrow(() => click('turn.send'))
  assert.deepEqual(windows[0].sent, [])
})

test('a rebuild with no window keeps the items grey', () => {
  const { menu, windows, item, close } = load()
  menu.refreshMenu(BUSY)
  close(windows[0])
  menu.rebuildMenu()
  assert.equal(item('session.close').enabled, false)
  assert.equal(item('session.new').enabled, true)
})

// The wiring in `src/main/index.ts` and `src/preload/index.ts` needs a running app, so it is read as
// text, as `single-instance.test.mjs` reads the lock. Each pin fails when its line is removed.
const index = readFileSync('src/main/index.ts', 'utf8')
const preload = readFileSync('src/preload/index.ts', 'utf8')

/** A close the menu is not told about leaves it delivering to a window that is gone. */
test('the window closed handler tells the menu which window closed, before anything else', () => {
  assert.match(index, /window\.on\('closed', \(\) => \{\n\s*menuWindowClosed\(created\)\n/)
  assert.match(index, /import \{[^}]*\bwindowClosed as menuWindowClosed\b[^}]*\} from '\.\/menu'/)
})

/** A late close of an older window must not clear the globals of a window built after it. */
test('the window closed handlers reset the globals only for the window that is still current', () => {
  assert.match(index, /window\.on\('closed', \(\) => \{\n\s*if \(window !== created\) return\n\s*if \(watchClock\) clearInterval\(watchClock\)/)
  assert.match(index, /menuWindowClosed\(created\)\n\s*if \(window !== created\) return\n\s*signIn\.abandon\(\)/)
})

/** A message from a window that is closing arrives after the reset, and must not enable items. */
test('the menu state and commands-ready messages are heard only from the current window\'s own top frame', () => {
  assert.match(index, /ipcMain\.on\('bravebot:menu:state', \(event, value: unknown\) => \{\n(?:\s*\/\/[^\n]*\n)*\s*if \(!window \|\| event\.senderFrame !== window\.webContents\.mainFrame\) return\n/)
  assert.match(index, /ipcMain\.on\('bravebot:commands:ready', \(event\) => \{\n\s*if \(window && event\.senderFrame === window\.webContents\.mainFrame\) commandsReady\(\)\n/)
  assert.equal(index.match(/ipcMain\.on\('bravebot:commands:ready'/g).length, 1)
})

/** Without the opener, New Chat with no window does nothing, and the page never subscribes without the message. */
test('the menu is given the window builder before the first window is built, and the page tells the main process it has subscribed', () => {
  assert.match(index, /\n\s*useWindowOpener\(createWindow\)\n\s*createWindow\(\)\n/)
  assert.match(preload, /ipcRenderer\.on\('bravebot:command', handler\)\n(?:\s*\/\/[^\n]*\n)*\s*ipcRenderer\.send\('bravebot:commands:ready'\)\n/)
})
