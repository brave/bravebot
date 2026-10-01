/**
 * `window.bravebot` for the Android app.
 *
 * The renderer was written against one object, and this is that object built over a WebView
 * message channel instead of Electron's IPC. Nothing in `src/renderer` knows which one it has.
 *
 * The other end is `BravebotHost`, which the app injects for this page's origin only. It plays the
 * part the main process plays on the desktop. Agent methods cross as the bridge's own
 * `{id, method, params}` and come back as its own `{id, ok | error}` and `{event, …}` lines.
 * The host checks the method against the same allowlist and strips the same fields from
 * `turn.send` that the main process strips, so nothing here is a security decision. Host-side
 * calls (a directory, the recents) cross as `{id, local, args}`.
 *
 * Preferences that say only how the window is arranged are kept in this page's own storage,
 * parsed on the way out by the same functions the main process uses. On the desktop they go
 * to the main process because a `file://` origin loses its storage between launches. The app
 * serves this page from an https origin, which keeps it.
 *
 * What the Android app does not do yet answers with the empty value the desktop gives for
 * "nothing here", rather than throwing: no bots, no forks, no file tree, no export.
 */

import type { BravebotApi, Answer, ThemeState } from '../preload/index'
import type { BridgeEvent } from '../shared/protocol'
import { parseExperience, type Experience } from '../shared/experience'
import { parseLayout } from '../shared/layout'
import { parseView } from '../shared/view'
import { parseAppearance, type Appearance } from '../shared/theme'
import { goBack } from '../renderer/back'

interface HostChannel {
  postMessage(message: string): void
  addEventListener(type: 'message', listener: (event: { data: string }) => void): void
}

declare global {
  interface Window {
    BravebotHost?: HostChannel
    /** Called by the app for each system Back press; false lets the press leave the app. */
    bravebotBack?: () => boolean
  }
}

const host = window.BravebotHost
if (!host) throw new Error('BravebotHost is missing: this page must be loaded by the Android app')

let nextId = 0
const waiting = new Map<number, (message: Record<string, unknown>) => void>()
const eventListeners = new Set<(event: BridgeEvent) => void>()
const themeListeners = new Set<(state: ThemeState) => void>()

host.addEventListener('message', ({ data }) => {
  let message: Record<string, unknown>
  try {
    message = JSON.parse(data) as Record<string, unknown>
  } catch {
    return
  }
  if (typeof message.event === 'string') {
    for (const listener of eventListeners) listener(message as unknown as BridgeEvent)
    return
  }
  if (typeof message.id !== 'number') return
  const settle = waiting.get(message.id)
  if (!settle) return
  waiting.delete(message.id)
  settle(message)
})

function post(body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const id = ++nextId
  return new Promise(resolve => {
    waiting.set(id, resolve)
    host!.postMessage(JSON.stringify({ id, ...body }))
  })
}

/** A host-side call whose failure is the given fallback, since none of these may throw. */
async function local<T>(name: string, fallback: T, ...args: unknown[]): Promise<T> {
  const answer = await post({ local: name, args })
  return 'ok' in answer ? (answer.ok as T) : fallback
}

function stored(key: string): unknown {
  try {
    const text = localStorage.getItem(`bravebot:${key}`)
    return text === null ? null : JSON.parse(text)
  } catch {
    return null
  }
}

function store(key: string, value: unknown): void {
  try {
    localStorage.setItem(`bravebot:${key}`, JSON.stringify(value))
  } catch {
    // Arrangement is best-effort on the desktop too.
  }
}

function subscribe<T>(set: Set<T>, listener: T): () => void {
  set.add(listener)
  return () => set.delete(listener)
}

const api: BravebotApi = {
  platform: 'android' as NodeJS.Platform,

  async request<T>(method: string, params?: Record<string, unknown>): Promise<Answer<T>> {
    return (await post({ method, params: params ?? {} })) as Answer<T>
  },
  onEvent: listener => subscribe(eventListeners, listener),

  async readExperience() {
    return parseExperience(stored('experience'))
  },
  async writeExperience(key, value) {
    const next: Experience = parseExperience({ ...parseExperience(stored('experience')), [key]: value })
    store('experience', next)
    return next
  },
  async readLayout() {
    return parseLayout(stored('layout'))
  },
  writeLayout: layout => store('layout', layout),
  async readView() {
    return parseView(stored('view'))
  },
  writeView: view => store('view', view),
  async readTheme() {
    return { chosen: parseAppearance(stored('theme')) }
  },
  writeTheme(name: Appearance) {
    store('theme', name)
    for (const listener of themeListeners) listener({ chosen: parseAppearance(name) })
  },
  onThemeChanged: listener => subscribe(themeListeners, listener),

  readRecents: () => local('recents.read', [] as string[]),
  chooseDirectory: () => local('directory.choose', null as string | null),

  async readForks() {
    return []
  },
  async readBots() {
    return []
  },
  async writeBotModel() {
    return null
  },
  async writeBot() {
    return null
  },
  async retireBot() {
    return null
  },
  async removeBot() {
    return null
  },
  async readBotMemory() {
    return null
  },
  async readMemoryHistory() {
    return []
  },
  async editBotMemory() {
    throw new Error('bots are not available on Android yet')
  },
  async releaseBotSession() {},
  async sendBotTurn() {
    return { error: { code: 'unsupported', message: 'bots are not available on Android yet' } }
  },

  async listFiles() {
    return null
  },
  async previewFile() {
    return null
  },
  async chooseAttachments() {
    return []
  },
  async searchFiles() {
    return { paths: [], incomplete: false }
  },
  async openFile() {
    return { status: 'failed', message: 'opening files is not available on Android yet' }
  },

  async selectSettings() {
    return null
  },
  async saveHooks() {
    throw new Error('hooks are not available on Android')
  },
  async exportSession() {
    return { status: 'failed', message: 'export is not available on Android yet' }
  },

  // There is no menu bar and no context menu to drive, so nothing is ever chosen.
  onCommand: () => () => {},
  popupContext() {},
  publishState() {},
  onBotConsolidation: () => () => {},
  onWindowActive(listener) {
    const handler = () => listener(document.visibilityState === 'visible')
    document.addEventListener('visibilitychange', handler)
    return () => document.removeEventListener('visibilitychange', handler)
  },
}

Object.defineProperty(window, 'bravebot', { value: Object.freeze(api), writable: false })
window.bravebotBack = goBack
