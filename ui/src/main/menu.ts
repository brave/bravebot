/**
 * The application menu.
 *
 * Until this existed the app shipped Electron's default menu, which is not a neutral
 * placeholder: it is titled after Electron, its Help item points at electronjs.org, and it
 * offers Reload and Toggle Developer Tools in a release build. In an app whose entire
 * premise is a renderer that cannot reach anything, a DevTools item anyone can pick is the
 * most expensive thing in that menu, and gating it is most of the reason to own one.
 *
 * The items come from [`COMMANDS`], so the menu, its accelerators and any in-window menu
 * are three views of one declaration rather than three lists that have to be kept in step.
 * What this file adds on top is the platform furniture the shared list has no business
 * knowing about: the roles.
 *
 * ## Why the name is written here rather than set on the app
 *
 * The obvious way to stop the menu saying "bravebot-ui" is `app.setName('Bravebot')`. It is
 * the wrong way: `app.getPath('userData')` is derived from `app.name`, so renaming the app
 * moves `~/Library/Application Support/bravebot-ui/` and every remembered layout in it stops
 * being found. The window would come back with default columns and nothing would say why.
 * So the name is a constant here and the roles that would otherwise interpolate `app.name`
 * — hide, quit — are given the label explicitly.
 *
 * The bold title beside the Apple menu is a third thing again: AppKit reads it from the
 * bundle's `CFBundleName`, before any of this has run, and no template can change it. It is
 * set instead by `scripts/name-dev-app.mjs`, which renames Electron's own bundle in
 * `node_modules` — the only lever there is until there is a packaging step with a
 * `productName`. If the menu bar ever says "Electron" again, that script has been undone by
 * an `pnpm install` and `pnpm run name-dev-app` puts it back.
 *
 * ## With no window
 *
 * On macOS closing the last window leaves the app and its menu bar running. The menu is told
 * when its window closes: the items that need a window go grey, and a command chosen anyway is
 * dropped. File > New Chat and Open Recent build a window and deliver their command once its
 * page has subscribed. Nothing here reads a window that has been destroyed.
 *
 * ## The roles are load-bearing
 *
 * `editMenu` and `windowMenu` are not decoration. The default menu already provided them,
 * so replacing it without them would silently take copy, paste and select-all away from the
 * composer — a regression nobody would attribute to a menu change. They are here to be
 * kept, and `scripts/drive-menu.mjs` asserts they survived.
 */

import { app, BrowserWindow, Menu, type MenuItemConstructorOptions } from 'electron'
import { openExternal as openExternally } from './external-open'
import { basename } from 'node:path'
import { recents } from './recents'
import { offerDirectories } from './opened'
import {
  COMMANDS,
  CONTEXT,
  NOTHING_OPEN,
  command,
  isChecked,
  isEnabled,
  menuLabel,
  type CommandId,
  type ContextCommandId,
  type ContextRef,
  type WindowState,
} from '../shared/commands'

/**
 * What this app is called on screen. See the note above on why it is not `app.setName`.
 */
const NAME = 'Brave Bot'

/**
 * The window's state as the menu last heard it.
 *
 * Held here rather than read from anywhere because the renderer is the only thing that
 * knows it, and it arrives by message. Before the first message nothing is open, which is
 * true: the window has not finished loading.
 */
let state: WindowState = NOTHING_OPEN

/** Where a chosen command is delivered. Set when a window is built and cleared when it closes. */
let target: BrowserWindow | null = null

/** Builds a window when a command that starts a chat is chosen with none open. */
let openWindow: (() => void) | null = null

type Chosen = { id: CommandId | ContextCommandId; context: ContextRef | null }

/**
 * The command that opened the current window, held until its page has subscribed. A page loads
 * after the window exists, and a command sent before it listens is lost.
 */
let waiting: Chosen | null = null

/**
 * The commands that make sense with no window: the ones that start a chat. Every other command
 * acts on something a window shows, so with none open it does nothing.
 */
const OPENS_A_WINDOW = new Set<CommandId | ContextCommandId>(['session.new', 'session.new-here'])

/**
 * Reading `webContents` of a destroyed window throws, so the window is asked first. A closed
 * window stays referenced by whatever held it, which is why the answer is checked at each use.
 */
function alive(window: BrowserWindow | null): window is BrowserWindow {
  return window !== null && !window.isDestroyed() && !window.webContents.isDestroyed()
}

function deliver(window: BrowserWindow, chosen: Chosen): void {
  window.webContents.send('bravebot:command', chosen.id, chosen.context)
}

/**
 * Tell the renderer a command was chosen.
 *
 * The id crosses and nothing else. The main process does not know what "new session" means
 * — the renderer owns every piece of state a command touches — so this is a notification,
 * not a call, and there is nothing to wait for.
 *
 * With no window, a command that starts a chat builds one and is delivered when its page has
 * subscribed (see [`commandsReady`]); any other command is dropped.
 */
function choose(id: CommandId | ContextCommandId, context?: ContextRef): void {
  const chosen: Chosen = { id, context: context ?? null }
  if (waiting) {
    if (OPENS_A_WINDOW.has(id)) waiting = chosen
    return
  }
  if (alive(target)) {
    deliver(target, chosen)
    return
  }
  if (!OPENS_A_WINDOW.has(id) || !openWindow) return
  waiting = chosen
  openWindow()
}

/** Register what builds a window, for a chat started from the menu while none is open. */
export function useWindowOpener(open: () => void): void {
  openWindow = open
}

/**
 * The page of the window the app built has subscribed to commands, so the one that asked for
 * the window can be delivered now. Carries nothing from the page.
 */
export function commandsReady(): void {
  const chosen = waiting
  waiting = null
  if (chosen && alive(target)) deliver(target, chosen)
}

/**
 * A window has closed. Items that need a window go grey, and the commands that were waiting for
 * it are dropped. A window that is not the current one changes nothing: a late close of the
 * previous window must not unset a window built since.
 */
export function windowClosed(window: BrowserWindow): void {
  if (target !== window) return
  target = null
  waiting = null
  apply(NOTHING_OPEN)
}

/** One menu item, built from the shared declaration. */
function item(id: CommandId): MenuItemConstructorOptions {
  const declared = command(id)
  return {
    // The id is what lets [`refreshMenu`] find this item again without rebuilding the menu.
    id: declared.id,
    label: menuLabel(declared, state),
    accelerator: declared.accelerator,
    enabled: isEnabled(declared.requires, state),
    // A tick, for the commands that are settings. AppKit would happily flip it on click by
    // itself; the state is set from `state` on every refresh instead, so the menu shows what
    // the window believes rather than what the menu last did.
    ...(declared.checkbox
      ? { type: 'checkbox' as const, checked: isChecked(declared.id, state) }
      : {}),
    click: (clicked) => {
      // Put the tick back to what the window believes: a setting can be refused, as keeping the
      // computer awake is when its first dialog is cancelled, and then no new state arrives.
      if (declared.checkbox) clicked.checked = isChecked(declared.id, state)
      choose(declared.id)
    },
  }
}

const SEPARATOR: MenuItemConstructorOptions = { type: 'separator' }

/**
 * File → Open Recent.
 *
 * The one part of the menu whose shape varies, which is why it is the one thing that makes
 * the menu get rebuilt rather than re-labelled. The path travels back as the command's
 * context, so the renderer opens a directory this process supplied rather than one it built.
 */
function openRecent(): MenuItemConstructorOptions {
  const directories = recents()
  offerDirectories(directories)
  return {
    label: 'Open Recent',
    submenu: directories.length
      ? directories.map((directory) => ({
          label: basename(directory) || directory,
          // The full path under the name, because two checkouts of the same project have
          // the same basename and picking the wrong one is a silent mistake.
          toolTip: directory,
          click: () => choose('session.new-here', { target: 'directory', id: directory }),
        }))
      : [{ label: 'No projects opened yet', enabled: false }],
  }
}

/**
 * Make a window the one chosen commands are delivered to, and build the menu.
 *
 * Called for each window the app builds. Afterwards the menu is mutated in place rather than
 * rebuilt; see [`refreshMenu`].
 */
export function installMenu(window: BrowserWindow): void {
  target = window
  buildMenu()
}

function buildMenu(): void {
  // Reload and DevTools only where there is a developer. `app.isPackaged` is the same test
  // `Bridge.binaryPath` already uses to tell a release from a checkout.
  const developing: MenuItemConstructorOptions[] = app.isPackaged
    ? []
    : [SEPARATOR, { role: 'reload' }, { role: 'toggleDevTools' }]

  const template: MenuItemConstructorOptions[] = [
    {
      // Written out rather than `role: 'appMenu'`, because the About item has to be ours:
      // the agent's build stamp is the first thing worth knowing when something is wrong,
      // and the stock About panel cannot show it.
      label: NAME,
      submenu: [
        item('app.about'),
        SEPARATOR,
        item('app.caffeinate'),
        SEPARATOR,
        { role: 'services' },
        SEPARATOR,
        // Labelled explicitly: left to the role, these interpolate `app.name`, which is the
        // package identifier `bravebot-ui`, and correcting that at the source would move
        // `userData`.
        { role: 'hide', label: `Hide ${NAME}` },
        { role: 'hideOthers' },
        { role: 'unhide' },
        SEPARATOR,
        { role: 'quit', label: `Quit ${NAME}` },
      ],
    },
    {
      label: 'File',
      submenu: [
        item('session.new'),
        openRecent(),
        SEPARATOR,
        {
          label: 'Export',
          submenu: [
            item('session.export-tools'),
            SEPARATOR,
            item('session.export-text'),
            item('session.export-markdown'),
            item('session.export-pdf'),
          ],
        },
        SEPARATOR,
        item('session.close'),
        { role: 'close' },
      ],
    },
    // Kept, not added. See the note at the top of this file.
    { role: 'editMenu' },
    {
      label: 'View',
      submenu: [
        item('view.fold-left'),
        item('view.fold-right'),
        item('view.reset-columns'),
        { type: 'separator' },
        item('view.find'),
        item('view.focus-composer'),
        { type: 'separator' },
        item('view.theme'),
        ...developing,
      ],
    },
    {
      label: 'Chat',
      submenu: [
        item('turn.send'),
        item('turn.cancel'),
        SEPARATOR,
        item('mode.cycle'),
        SEPARATOR,
        item('turn.rewind'),
        item('session.note'),
        SEPARATOR,
        item('session.status'),
        item('session.cost'),
        item('session.context'),
        item('session.request'),
      ],
    },
    { role: 'windowMenu' },
    {
      label: 'Help',
      submenu: [item('help.changelog'), SEPARATOR, item('help.doctor'), item('help.bug-report'), SEPARATOR, item('help.update')],
    },
  ]

  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

/**
 * Take in what the window is now doing, and re-label and re-grey the items that care.
 *
 * The menu is mutated rather than rebuilt. Rebuilding on every turn that starts or stops
 * would close a menu the user has open under their pointer, and it would throw away the
 * platform's own state in the Services and Window submenus. Each command's item is found by
 * the id it was built with, so nothing here has to know the shape of the template.
 */
export function rebuildMenu(): void {
  buildMenu()
}

/**
 * Heard only while a window is the target. A message from a window that is closing arrives after
 * the reset in [`windowClosed`], and applying it would enable items for a window that is gone.
 */
export function refreshMenu(next: WindowState): void {
  if (!alive(target)) return
  apply(next)
}

function apply(next: WindowState): void {
  state = next
  const menu = Menu.getApplicationMenu()
  if (!menu) return
  for (const declared of COMMANDS) {
    const found = menu.getMenuItemById(declared.id)
    if (!found) continue
    found.enabled = isEnabled(declared.requires, state)
    found.label = menuLabel(declared, state)
    if (declared.checkbox) found.checked = isChecked(declared.id, state)
  }
}

/** Open a link the app itself chose. Never a URL the renderer supplied. */
export function openExternal(url: string): void {
  void openExternally(url)
}

/**
 * Show the menu that belongs to a thing on screen.
 *
 * Built here, from [`CONTEXT`], out of labels compiled into this process. What arrived from
 * the renderer is a kind and an identifier, and neither is ever displayed: the reference is
 * handed straight back when an item is chosen, so the renderer can find the thing it already
 * had. Nothing the agent read off disk can reach a menu label through this path.
 *
 * Native rather than a panel in the window, because this is the one surface where people
 * have a pixel-exact expectation. The window is already `vibrancy: 'sidebar'` with inset
 * traffic lights; a right-click menu drawn in HTML would be the only thing on screen that
 * looked like a web page, and it would have to reimplement typeahead, edge flipping and
 * VoiceOver to get back to where AppKit starts.
 */
export function popupContext(reference: ContextRef): void {
  if (!alive(target)) return
  const items = CONTEXT[reference.target]
  if (items.length === 0) return
  const menu = Menu.buildFromTemplate(
    items.map((entry) => ({
      id: entry.id,
      label: entry.label,
      // Greyed from the same window state the menu bar is greyed from, so an item that the
      // window would refuse is not black in one menu and grey in the other.
      enabled: entry.requires ? isEnabled(entry.requires, state) : true,
      click: () => choose(entry.id, reference),
    })),
  )
  // No coordinates: with none, Electron pops at the cursor, which is where a context menu
  // belongs and is one fewer thing to get wrong near a screen edge.
  menu.popup({ window: target })
}
