/**
 * The main process: one window, one agent, and a narrow channel between them.
 *
 * The security posture here is not boilerplate. This app drives a tool whose whole point
 * is that untrusted content cannot reach anything that decides, and a renderer with
 * filesystem access would undo that from the outside. So the renderer gets no Node, no
 * remote origins, and no navigation.
 */

import { readHooks, saveHooks } from './agent-settings'
import { app, BrowserWindow, ipcMain, dialog, shell } from 'electron'
import { randomUUID } from 'node:crypto'
import { newAvatarSeed } from '../shared/avatar'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { Bridge, BridgeError } from './bridge'
import { parseLayout } from '../shared/layout'
import { parseView } from '../shared/view'
import { parseContextRef, parseWindowState } from '../shared/commands'
import { installMenu, popupContext, rebuildMenu, refreshMenu } from './menu'
import { noteProject, recents } from './recents'
import { putBots, putLayout, putPanels, putTheme, putView, readState } from './state'
import { parsePanels } from '../shared/state'
import { isProjectPath } from '../shared/recents'
import { forks, noteFork } from './forks'
import {
  bot,
  bots,
  ground,
  memory,
  noteBotArchived,
  noteBotMemory,
  noteBotNudged,
  noteBotSession,
  nudgeDue,
  releaseBotSession,
  retireBot,
  saveBot,
  consolidationPrompt,
  AFTER_COMPACTION,
} from './bots'
import { isBotModel, isSlug, slugFor, withoutBot, type Bot } from '../shared/bots'
import { isSessionId, parseForkResult } from '../shared/forks'
import { rootForSession, forgetRoot, list, noteRoot, open as openInApp, preview, search, chooseAttachments, attachmentPaths } from './files'
import { isSubpath } from '../shared/files'
import {
  parseExportRequest,
  suggestedFilename,
  toMarkdown,
  toPlainText,
  withExtension,
  type ExportFormat,
  type ExportOutcome,
} from '../shared/export'
import { printToPdf } from './export'
import { readThemes, themesDirectory, watchThemes } from './theme'
import { parseChosenTheme } from '../shared/theme'
import { readExperience, writeExperience } from './experience'
import { editMemory, memoryHistory, snapshotMemory, removeMemoryHistory } from './memory'

/**
 * Which live session belongs to which bot, for the length of this run.
 *
 * A handle is per-process and means nothing on disk, so this is not written down. It exists to
 * answer one question at the moment the agent answers it: a `turn.done` names a handle and carries
 * the session's durable id and its compaction count, and this is what turns that into a bot.
 *
 * Keyed by handle rather than by bot, because the events arrive keyed that way and because a bot
 * whose session was closed and reopened is a new handle for the same bot.
 */
const botHandles = new Map<string, string>()
const runningHandles = new Set<string>()

/**
 * The handles currently running a turn this app sent rather than a person.
 *
 * One job: a consolidation ends in a `turn.done` like any other, and the hook that decides to send
 * one is reading `turn.done`. Without this a compaction would be answered by a turn, whose own
 * completion would be answered by another, forever — and each would be grounded, so the loop would
 * also be the most expensive one available.
 *
 * Not written down, for the same reason `botHandles` is not: a handle means nothing across runs,
 * and a consolidation left running when the process died is not one this process can finish.
 */
const consolidating = new Set<string>()

/** Why a bot's turn could not be sent, in the shape `bravebot:request` already answers with. */
interface BotFailure {
  code: string
  message: string
}

let window: BrowserWindow | null = null
let bridge: Bridge | null = null
// Model discovery uses its own process so a slow listing never blocks live turn replies.
let selectedSettings: string | null = null
let watchClock: ReturnType<typeof setInterval> | null = null
let pollingWatches = false
let modelListingGeneration = 0
let modelListingDirectory: string | undefined
let modelListing: Promise<unknown> | null = null
function listModels(directory?: string): Promise<unknown> {
  if (modelListing && modelListingDirectory === directory) return modelListing
  modelListingDirectory = directory
  const generation = ++modelListingGeneration
  const discovery = new Bridge(() => {}, selectedSettings)
  let timer: ReturnType<typeof setTimeout>
  modelListing = Promise.race([
    discovery.request('models.list', { directory }),
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Model discovery timed out. Try again.')), 60000)
    }),
  ]).finally(() => {
    clearTimeout(timer)
    discovery.dispose()
    if (generation === modelListingGeneration) modelListing = null
  })
  return modelListing
}

let stopWatchingThemes: (() => void) | null = null

/**
 * Send a turn as a bot, which is the only path that may name a file.
 *
 * Two callers and one composition on purpose. `turn.send` takes two lists of paths and admits both
 * to the planner as trusted context, so *where* those paths come from is the whole security story
 * of this feature — a window that could name one would be a window that could have the planner read
 * anything on the machine. Both callers hand over a bot; neither hands over a path.
 *
 * `grounded` decides how much is attached. `nudge` decides only what the briefing says once it has
 * been decided to attach it, and is meaningless without it.
 */
async function sendBotTurn(
  session: string,
  held: Bot,
  prompt: string,
  grounded: boolean,
  nudge = false,
  recall = true,
  model?: string | null,
  attachments?: unknown,
): Promise<{ ok?: unknown; error?: BotFailure }> {
  if (!bridge) return { error: { code: 'no_bridge', message: 'the agent is not running' } }

  botHandles.set(session, held.slug)

  // `recall` is left off entirely in the ordinary case rather than sent as `true`. The agent
  // defaults it that way, and a parameter that only ever appears when it is doing something is a
  // parameter somebody reading the wire can see the point of.
  const params: Record<string, unknown> = recall ? { session, prompt } : { session, prompt, recall }
  const selectedModel = held.model ?? model
  if (selectedModel !== undefined) params.model = selectedModel
  if (grounded) {
    // Made afresh on the way into every send rather than once when the bot was created. A file a
    // turn names and cannot read is not a smaller turn, it is a failed one — so a memory deleted
    // by a `git clean`, or a branch switched to one that never had it, is repaired here instead of
    // ending the turn inside the agent with a message about a path.
    const paths = ground(held, nudge)
    if (!paths) {
      return {
        error: {
          code: 'no_checkout',
          message: `${held.name} works in ${held.directory}, which cannot be written to`,
        },
      }
    }
    params.dropped = [paths.ground]
    params.files = [paths.memory]
  }

  try {
    params.files = [...((params.files as string[] | undefined) ?? []), ...attachmentPaths(session, attachments)]
    return { ok: await bridge.request('turn.send', params) }
  } catch (error) {
    if (error instanceof BridgeError) {
      return { error: { code: error.code, message: error.message } }
    }
    return { error: { code: 'internal', message: String(error) } }
  }
}

/**
 * Answer a compaction with a turn asking the bot to bring its memory up to date.
 *
 * Sent grounded, which is not an extra cost: the compaction has just made the briefing due, so the
 * user's next prompt would have carried it anyway. This spends the round trip and the attachment
 * together, and the window is told so it can stop expecting to send one.
 *
 * Everything about this is best-effort. A failure to consolidate is a memory that stays as it was,
 * which is exactly where it would have stayed had none of this existed — so nothing here surfaces
 * an error of its own, and the `turn.error` the window is about to receive is the whole report.
 */
/**
 * Tell the window a consolidation is over, and whether the briefing went with it.
 *
 * `delivered` is the whole of what the window does with this. A consolidation that ran carried the
 * briefing, so the session is grounded again and the next thing somebody types must not carry it a
 * second time — but one that never left, because the checkout is gone or a turn was already in
 * flight, delivered nothing, and saying otherwise would cost the bot its briefing over a turn that
 * did not happen.
 */
function ended(session: string, slug: string | null, delivered: boolean): void {
  window?.webContents.send('bravebot:bots:consolidated', { session, slug, delivered })
}

async function consolidate(session: string, slug: string): Promise<void> {
  const held = bot(slug)
  if (!held) return
  // Set before the send rather than after it, because the answer can arrive before an `await`
  // resumes and a flag set late is a flag that was never set.
  consolidating.add(session)
  window?.webContents.send('bravebot:bots:consolidating', { session, slug })
  const answer = await sendBotTurn(
    session,
    held,
    consolidationPrompt(held, AFTER_COMPACTION),
    true,
    false,
    // Nobody typed this, so it is not something anybody should find by pressing up — in this
    // window or in the terminal front-end, which shares the same history file. It does not name
    // the session either. See `recall` in `bridge.rs`.
    false,
  ).catch(() => ({ error: { code: 'internal', message: 'consolidation failed' } }))
  if (answer.error) {
    consolidating.delete(session)
    ended(session, slug, false)
  }
}

function createWindow(): void {
  window = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 900,
    minHeight: 560,
    show: false,
    // Keep native window controls on Linux; inset traffic lights and vibrancy are macOS-only.
    ...(process.platform === 'darwin'
      ? {
          titleBarStyle: 'hiddenInset' as const,
          trafficLightPosition: { x: 16, y: 18 },
          vibrancy: 'sidebar' as const,
          backgroundColor: '#00000000',
        }
      : { backgroundColor: '#181818' }),
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webviewTag: false,
    },
  })

  window.once('ready-to-show', () => window?.show())

  // Nothing in this app navigates anywhere. A link opens in the user's browser, and an
  // in-window navigation is refused outright rather than sandboxed.
  window.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url)
    return { action: 'deny' }
  })
  window.webContents.on('will-navigate', (event) => event.preventDefault())

  bridge = new Bridge((message) => {
    if (message.session && message.event === 'turn.started') runningHandles.add(message.session)
    if (message.session && (message.event === 'turn.done' || message.event === 'turn.error')) runningHandles.delete(message.session)
    // What this app has to say about the event, held until the event itself has been sent. See
    // the note where it is called.
    let after: (() => void) | null = null
    // Two things a bot needs to remember are only ever said here, and only by the agent: the
    // durable id of the session behind it — which becomes real on the turn that first writes a
    // record, not before — and how much compaction has taken out of that session, which is what
    // decides when its briefing has to be given again.
    //
    // Read off the event and never off anything a window asked for, the same promise the recents
    // list and the fork lineage each make one screen up. The window can ask a bot to speak; it
    // cannot tell this process what the answer was.
    if (message.event === 'turn.done' && typeof message.session === 'string') {
      const handle = message.session
      const slug = botHandles.get(handle)
      if (slug) {
        if (message.data.id) noteBotSession(slug, message.data.id)
        // Read before `noteBotArchived` moves it, because the comparison *is* the signal: the
        // archive rises exactly once per compaction that actually happened, which is the only
        // reliable way to learn that one did. See the note on `Bot.archived`.
        const before = bot(slug)?.archived ?? 0
        noteBotArchived(slug, message.data.archived)
        // Whether the bot wrote anything down during the turn that has just ended. Asked of every
        // turn including a consolidation's own, so a consolidation that worked is what resets the
        // count that would otherwise have nudged.
        noteBotMemory(slug)
        try { snapshotMemory(slug) } catch { /* Memory itself remains available if history storage fails. */ }

        // A consolidation ending is the end of it. Answering it with another would be a loop.
        if (consolidating.delete(handle)) {
          // Keep the queue held until the ordered completion announcement releases it.
          message.data.consolidating = true
          after = () => ended(handle, slug, true)
        }
        else if (message.data.archived > before) {
          message.data.consolidating = true
          after = () => void consolidate(handle, slug)
        }
      }
    }
    // A turn this app sent can fail like any other, and a flag left set would mean the next
    // compaction went unanswered in silence. Cleared without a word to the window beyond the
    // ordinary `turn.error` it is about to receive.
    if (message.event === 'turn.error' && typeof message.session === 'string') {
      const handle = message.session
      const slug = botHandles.get(handle)
      if (slug && message.data.id) noteBotSession(slug, message.data.id)
      if (consolidating.delete(handle)) {
        after = () => ended(handle, botHandles.get(handle) ?? null, true)
      }
    }

    // The event first, and this app's own announcement after it. Both arrive as messages to the
    // same window in the order they are sent, and the wrong order here is visible: a consolidation
    // announced before the `turn.done` that provoked it draws its line above the reply it comes
    // after, which reads as though the app interrupted rather than followed.
    window?.webContents.send('bravebot:event', message)
    after?.()
  }, selectedSettings)

  if (watchClock) clearInterval(watchClock)
  watchClock = setInterval(() => {
    if (!bridge || !window || pollingWatches) return
    pollingWatches = true
    void bridge.request('watches.poll').catch(() => {}).finally(() => { pollingWatches = false })
  }, 1000)
  window.on('closed', () => { if (watchClock) clearInterval(watchClock); watchClock = null })

  // Watching the palettes directory, so that editing one is an editing loop rather than a relaunch
  // each time. Torn down with the window rather than at quit: on macOS the last window can close
  // and a new one be built from the dock, and a watcher left holding a `webContents` that is gone
  // would be one more thing keeping it alive.
  stopWatchingThemes?.()
  stopWatchingThemes = watchThemes(() => {
    window?.webContents.send('bravebot:theme:changed', {
      themes: readThemes(),
      chosen: readState().theme,
      directory: themesDirectory(),
    })
  })

  if (process.env.ELECTRON_RENDERER_URL) {
    void window.loadURL(process.env.ELECTRON_RENDERER_URL)
  } else {
    void window.loadFile(join(__dirname, '../renderer/index.html'))
  }

  window.on('closed', () => {
    stopWatchingThemes?.()
    stopWatchingThemes = null
    bridge?.dispose()
    bridge = null
    runningHandles.clear()
    botHandles.clear()
    window = null
  })

  // Built here rather than once at startup, because the menu holds the window it delivers a
  // chosen item to. On macOS closing the last window does not quit the app, and clicking the
  // dock icon builds a new one — with the menu installed once, every item would still be
  // pointing at the window that was closed, and the whole menu would go quiet with nothing
  // on screen to say why.
  installMenu(window)
}

/**
 * The methods the renderer may call.
 *
 * An allow-list rather than a pass-through: the renderer names a method and the main
 * process decides whether that is a method at all. A generic "send anything to the
 * agent" channel would make the preload's narrowness decorative.
 */
const ALLOWED = new Set([
  'agent.info',
  'models.list',
  'session.list',
  'session.open',
  'session.new',
  'session.fork',
  'session.close',
  'turn.send',
  'turn.cancel',
  'confirm.reply',
  'run.reply',
  'output.reply',
  'vouch.reply',
  'vet.reply',
  'ask.reply',
  'trust.reply',
  'permissions.list',
  'permissions.revoke',
  'doctor',
  'settings.inspect',
  'watches.list',
  'watches.add',
  'watches.stop',
])

/** What the save sheet offers per format. */
const FILTERS: Record<ExportFormat, Electron.FileFilter> = {
  txt: { name: 'Plain Text', extensions: ['txt'] },
  md: { name: 'Markdown', extensions: ['md', 'markdown'] },
  pdf: { name: 'PDF', extensions: ['pdf'] },
}

/**
 * Remember where a session that has just started is working, for the file tree.
 *
 * The handle comes off the answer in every case. The directory comes off the answer too where
 * there is one — an opened session carries its record, a forked one its own directory — and off
 * the request for `session.new`, which answers with a handle and a branch and nothing else. That
 * one path is the same value this handler already trusts enough to write to the recents list a few
 * lines up, it arrived from a native picker or a list the main process itself keeps, and it is
 * checked as a project path before it becomes a root.
 */
function noteOpenedRoot(method: string, params: unknown, ok: unknown): void {
  if (method !== 'session.open' && method !== 'session.new' && method !== 'session.fork') return
  if (typeof ok !== 'object' || ok === null) return
  const answer = ok as Record<string, unknown>
  if (!isSessionId(answer.session)) return

  if (method === 'session.new') {
    noteRoot(answer.session, (params as { directory?: unknown } | null)?.directory)
    return
  }
  const record = answer.record
  const directory =
    method === 'session.open' && typeof record === 'object' && record !== null
      ? (record as { directory?: unknown }).directory
      : answer.directory
  noteRoot(answer.session, directory)
}

/**
 * The params a method is allowed to have arrived with.
 *
 * `turn.send` takes two lists of file paths — `files`, read inside the workspace, and `dropped`,
 * read anywhere on the disk — and both are admitted to the planner as *trusted* context. Nothing
 * else the renderer can say has that reach: the file tree is confined to roots this process learnt
 * from the agent, the folder picker is native, and the preload has never carried a file's contents
 * in either direction. A window that could name either list would be a window that could read any
 * file on the machine and have the planner read it too, which is a larger change than any feature
 * is worth.
 *
 * So they are removed here rather than trusted here. A bot's turn needs both, and gets them from
 * `bravebot:bots:send` below — which composes the paths itself, from a definition this process
 * holds, and never from anything that crossed the bridge from a window.
 *
 * Stripped silently. There is no legitimate caller to warn, and a message saying which key was
 * removed would be a message telling a compromised renderer what to try next.
 */
function sanitised(method: string, params: unknown): Record<string, unknown> {
  const held = (params ?? {}) as Record<string, unknown>
  if (method !== 'turn.send') return held
  // `recall` joins the two lists for a smaller reason than theirs. It decides whether a prompt is
  // one a person can find again, and that is a claim about who asked — which this process makes
  // and a window does not get to. Nothing worse than a lost history entry is at stake; it is here
  // because the answer to "may the renderer say this?" is the same either way.
  const { files: _files, dropped: _dropped, recall: _recall, attachments, ...rest } = held
  return { ...rest, files: attachmentPaths(typeof rest.session === 'string' ? rest.session : '', attachments) }
}

app.whenReady().then(() => {
  ipcMain.handle('bravebot:request', async (_event, method: unknown, params: unknown) => {
    if (typeof method !== 'string' || !ALLOWED.has(method)) {
      return { error: { code: 'bad_request', message: `not a permitted method: ${String(method)}` } }
    }
    if (!bridge) {
      return { error: { code: 'no_bridge', message: 'the agent is not running' } }
    }
    try {
      const session = (params as { session?: unknown } | null)?.session
      const ok = method === 'models.list'
        ? await listModels(rootForSession(typeof session === 'string' ? session : ''))
        : await bridge.request(method, sanitised(method, params))
      // Opening a session is the other way a project becomes recent, and this handler is
      // already the choke point that sees it. Reading one field it is forwarding anyway is
      // a smaller thing than a channel that would let the renderer write the list itself.
      if (method === 'session.open' || method === 'session.new') {
        const directory = (params as { directory?: unknown } | null)?.directory
        if (isProjectPath(directory) && noteProject(directory)) rebuildMenu()
      }
      // A fork is the one call whose *answer* is worth writing down: which session it made and
      // which one it came out of. Read off the agent's reply and never off `params`, so the
      // renderer cannot compose a lineage it was not given — the same promise the recents list
      // makes one line up.
      if (method === 'session.fork') {
        const fork = parseForkResult(ok)
        if (fork) {
          noteFork(fork)
          if (noteProject(fork.child.directory)) rebuildMenu()
        }
      }
      // Which directory each live session is working in, for the file tree in the context
      // column. Recorded at this one choke point because it is the only place that sees both
      // halves at once, and read off the *answer* wherever the answer carries it — the handle
      // always, the directory for an opened or forked session — so a root the tree can browse is
      // one the agent confirmed rather than one the renderer asserted. See `files.ts`.
      noteOpenedRoot(method, params, ok)
      if (method === 'session.close') {
        const closing = (params as { session?: unknown } | null)?.session
        if (isSessionId(closing)) {
          forgetRoot(closing)
          botHandles.delete(closing)
          // A session being released takes any turn of its with it, this app's own included.
          consolidating.delete(closing)
        }
      }
      return { ok }
    } catch (error) {
      if (error instanceof BridgeError) {
        return { error: { code: error.code, message: error.message } }
      }
      return { error: { code: 'internal', message: String(error) } }
    }
  })

  /**
   * Write the conversation to a file the user picks.
   *
   * Note what this does not touch: `ALLOWED` above. An export reaches no agent method — it
   * is made entirely of things the window already had — so the list of things the renderer
   * may ask the agent to do is exactly as long as it was. Anyone checking whether this
   * feature widened the app's reach should find that it did not.
   *
   * The renderer sends turns; this composes the file. See `shared/export.ts` for why that
   * way round.
   */
  ipcMain.handle('bravebot:export', async (_event, value: unknown): Promise<ExportOutcome> => {
    const request = parseExportRequest(value)
    // Deliberately does not echo what arrived. A message about a malformed export is for
    // the person, and the payload is the one thing they cannot act on.
    if (!request) {
      return { status: 'failed', message: 'that is not a conversation this build can export' }
    }
    if (!window) return { status: 'failed', message: 'there is no window to ask in' }

    // Stamped here rather than sent from the renderer: one fewer value crossing, and the
    // process that writes the file is the one with an opinion about when it was written.
    const at = Date.now()
    const result = await dialog.showSaveDialog(window, {
      title: 'Export session',
      defaultPath: join(
        app.getPath('documents'),
        suggestedFilename(request.document.title, request.format),
      ),
      filters: [FILTERS[request.format]],
      properties: ['createDirectory'],
    })
    // A cancelled sheet is not a failure and says nothing on screen.
    if (result.canceled || !result.filePath) return { status: 'cancelled' }

    const target = withExtension(result.filePath, request.format)
    try {
      if (request.format === 'pdf') {
        writeFileSync(target, await printToPdf(request.document, at))
      } else {
        const text =
          request.format === 'md'
            ? toMarkdown(request.document, at)
            : toPlainText(request.document, at)
        writeFileSync(target, text, 'utf8')
      }
      return { status: 'saved', where: target }
    } catch (error) {
      // Unlike a layout that cannot be written, this one is worth saying out loud: somebody
      // asked for a file and does not have one.
      return { status: 'failed', message: String(error) }
    }
  })

  // What the window remembers between launches: the column widths and folds, how the session
  // list is arranged, and which panels the context column is showing.
  //
  // A file rather than `localStorage`, because the renderer is loaded from `file://` and
  // Chromium does not keep storage for that origin across launches — writes work for the
  // life of the window and are gone by the next one. Measured, not assumed. `state.ts` owns
  // that file and replaces one key per write, so a preference crossing here cannot disturb
  // another one, and the two lists in it that the renderer may read have no channel that
  // writes them.
  //
  // The renderer is ours, but this still validates what it sends: the value is written to
  // disk and read back on the next launch, so a bad write would be a bug that outlives the
  // session that caused it. One validator per shape is the whole of the judgement, on the way
  // in and on the way out — so what lands on disk is the parsed value and never the object
  // the renderer happened to pass.

  ipcMain.handle('bravebot:experience:read', () => readExperience())
  ipcMain.handle('bravebot:experience:write', (_event, key: unknown, value: unknown) => writeExperience(key, value))
  ipcMain.handle('bravebot:layout:read', () => readState().layout)

  ipcMain.handle('bravebot:layout:write', (_event, value: unknown) => {
    const layout = parseLayout(value)
    if (!layout) return
    putLayout(layout)
  })

  ipcMain.handle('bravebot:view:read', () => readState().view)

  ipcMain.handle('bravebot:view:write', (_event, value: unknown) => putView(parseView(value)))

  // The bots. Three channels rather than the usual read-and-write pair, and the extra one is the
  // point of the arrangement: a bot's turn cannot go through `bravebot:request` above, because a
  // bot needs `files` and `dropped` on `turn.send` and `sanitised` removes those from anything a
  // window sends. So the window names a *bot*, and this process — which holds the definitions and
  // composes every path from a slug it has judged — names the files.
  //
  // The split inside `bravebot:bots:write` is the same idea one field down. The form fields and
  // a creation seed cross freely; the session id, the compaction watermark and the
  // two figures that decide when a bot is reminded to write are reports of what the agent did, are
  // taken off its answers and off the filesystem below, and have no way in from here.

  ipcMain.handle('bravebot:bots:read', () => bots())

  ipcMain.handle('bravebot:bots:model', (_event, slug: unknown, model: unknown) => {
    const held = bot(slug)
    if (!held || !isBotModel(model)) return null
    const next = { ...held, model }
    saveBot(next)
    return next
  })

  ipcMain.handle('bravebot:bots:write', (_event, value: unknown) => {
    if (typeof value !== 'object' || value === null) return null
    const { slug, avatar, model, name, purpose, directory } = value as Record<string, unknown>
    if (model !== undefined && !isBotModel(model)) return null
    if (typeof name !== 'string' || typeof purpose !== 'string') return null
    if (!name.trim() || !purpose.trim()) return null
    if (!isProjectPath(directory)) return null
    if (avatar !== undefined && (typeof avatar !== 'string' || !avatar.trim() || avatar.length > 128)) {
      return null
    }

    // An existing bot keeps everything this channel cannot say — its id, its watermark, its seed,
    // when it was made. A new one is given a slug composed here from the name, so the thing that
    // becomes a path segment is never a string that arrived as one.
    const held = isSlug(slug) ? bot(slug) : null
    const next: Bot = held
      ? { ...held, name, purpose, model: model === undefined ? held.model : model }
      : {
          slug: slugFor(name, new Set(bots().map((each) => each.slug))),
          name,
          purpose,
          model: typeof model === 'string' ? model : null,
          // Use the draft's preview seed so creation keeps the face already shown. Older
          // callers may omit it; either way it is stored and survives a rename.
          avatar: typeof avatar === 'string' ? avatar : newAvatarSeed(randomUUID()),
          directory,
          session: null,
          conversations: [],
          archived: 0,
          // Nothing has been remembered and nothing has gone unremembered, so a new bot starts
          // owing no nudge. See `noteBotMemory`, which takes its first reading when its first
          // turn ends.
          remembered: 0,
          quiet: 0,
          // In use, which is what a bot somebody just filled in a form for is.
          retired: 0,
          created: Date.now(),
          updated: Date.now(),
        }
    saveBot(next)
    if (noteProject(next.directory)) rebuildMenu()
    return next
  })

  // Archiving is the ordinary way a bot leaves the list, and it is not a removal at all: the
  // definition stays exactly as it was, which is what lets the same session, the same memory and
  // the same face come back together when somebody brings it out again.
  ipcMain.handle('bravebot:bots:retire', (_event, slug: unknown, retired: unknown) => {
    if (typeof retired !== 'boolean') return null
    return retireBot(slug, retired)
  })

  ipcMain.handle('bravebot:bots:remove', (_event, slug: unknown) => {
    const held = bot(slug)
    if (!held) return null
    if ([...botHandles].some(([handle, owner]) => owner === held.slug && runningHandles.has(handle))) throw new Error('Stop this bot’s running conversations before deleting it.')
    removeMemoryHistory(held.slug)
    // The definition and app-owned memory copies go. Its session is a session like any other and stays
    // in the agent's own store, and its memory is a file in somebody's checkout that this app did
    // not put there on its own account. Deleting either would make a bot's removal a destructive
    // act, which is not what removing a row from a list looks like.
    //
    // What it *does* take is the thread tying those pieces together — the slug that names the
    // memory file, the seed the face is drawn from, the id of the session. So this is no longer
    // the first thing offered: the window puts a bot in the archive, and only offers this from
    // there, a second deliberate step. The guarantee is the interface's rather than this
    // channel's, and it stays that way on purpose — the demo tears its own bots down through
    // here, and a bot made by a script is a bot a script should be able to take away.
    putBots(withoutBot(bots(), held.slug))
    return held.slug
  })

  ipcMain.handle('bravebot:bots:memory', (_event, slug: unknown) => memory(slug))
  ipcMain.handle('bravebot:bots:memory-history', (_event, slug: unknown) => memoryHistory(slug))
  ipcMain.handle('bravebot:bots:edit-memory', (_event, slug: unknown, text: unknown, expected: unknown) => {
    if ([...botHandles].some(([handle, owner]) => owner === slug && runningHandles.has(handle))) throw new Error('Stop this bot’s running conversations before editing its memory.')
    return editMemory(slug, text, expected)
  })

  // Asked for when the window finds a bot pointing at a session the agent no longer lists. What it
  // becomes is not the window's to say — see `releaseBotSession`.
  ipcMain.handle('bravebot:bots:release', (_event, slug: unknown) => releaseBotSession(slug))

  /**
   * Send a turn as a bot.
   *
   * `grounded` is the window's claim that this turn is the one that has to carry the briefing —
   * the first of a session, or the first since a compaction. It decides how much is attached and
   * nothing else; it cannot name what is attached, which is the whole reason this channel exists.
   */
  ipcMain.handle(
    'bravebot:bots:send',
    async (_event, value: unknown): Promise<{ ok?: unknown; error?: BotFailure }> => {
      if (typeof value !== 'object' || value === null) {
        return { error: { code: 'bad_request', message: 'not a request' } }
      }
      const { session, slug, prompt, grounded, model, attachments } = value as Record<string, unknown>
      if (!isSessionId(session) || typeof prompt !== 'string') {
        return { error: { code: 'bad_request', message: 'not a request' } }
      }
      if (model !== undefined && model !== null && typeof model !== 'string') {
        return { error: { code: 'bad_request', message: 'invalid model' } }
      }
      const held = bot(slug)
      if (!held) return { error: { code: 'no_such_bot', message: 'no bot by that name' } }
      // Said separately from the line above rather than folded into it. A bot that has been put
      // away is not a bot that does not exist, and a refusal that named the wrong reason would
      // send whoever read it looking for a bot that is sitting in the archive. No window offers
      // this — an archived bot has no row that sends — but the channel should not lean on that.
      if (held.retired !== 0) {
        return { error: { code: 'bot_archived', message: 'that bot is archived' } }
      }

      // The window's claim is that the briefing is due; this may decide it is due when the window
      // did not. It is never the other way round — a window saying "grounded" is answering a
      // question about *its* session, which this process cannot see, so that answer stands.
      const nudge = grounded !== true && nudgeDue(held)
      if (nudge) noteBotNudged(held.slug)
      return sendBotTurn(session, held, prompt, grounded === true || nudge, nudge, true, model as string | null | undefined, attachments)
    },
  )

  ipcMain.handle('bravebot:panels:read', () => readState().panels)

  ipcMain.handle('bravebot:panels:write', (_event, value: unknown) => putPanels(parsePanels(value)))

  // The theme. The chosen name is a key in the same file as the three above; the list it is chosen
  // from is built here, because reading a directory of palettes is not something the renderer does.
  //
  // What crosses is a *name*. Not a path — the renderer cannot see the filesystem and this does not
  // become the first place it can reach one — and not a colour either, so nothing painted in this
  // window is something the renderer composed. Validated on the way in like the rest, and checked
  // against the list as well: a name nobody is offering is a bug on this side rather than a
  // preference, and writing it down would outlive the session that caused it.

  ipcMain.handle('bravebot:theme:read', () => ({
    themes: readThemes(),
    chosen: readState().theme,
    directory: themesDirectory(),
  }))

  ipcMain.handle('bravebot:theme:write', (_event, value: unknown) => {
    const name = parseChosenTheme(value)
    if (!readThemes().some((theme) => theme.name === name)) return
    putTheme(name)
  })

  // Choosing a project is a native affair: the renderer cannot see the filesystem and
  // should not be handed a path it invented.
  ipcMain.handle('bravebot:settings:select', async (_event, clear: unknown) => {
    if (!bridge || !window) throw new Error('The agent is unavailable.')
    let path: string | null = null
    if (clear !== true) {
      const picked = await dialog.showOpenDialog(window, { title: 'Choose run settings', properties: ['openFile'], filters: [{ name: 'JSON settings', extensions: ['json'] }] })
      if (picked.canceled) return null
      path = picked.filePaths[0] ?? null
      if (!path) return null
    }
    const report = await bridge.selectSettings(path)
    selectedSettings = path
    modelListing = null
    return report
  })
  const agentHome = async () => {
    const info = await bridge?.request<{ home: string | null }>('agent.info')
    if (!info?.home) throw new Error('The agent state directory is unavailable.')
    return info.home
  }
  ipcMain.handle('bravebot:hooks:read', async () => readHooks(await agentHome()))
  ipcMain.handle('bravebot:hooks:save', async (_event, text: unknown, expected: unknown) => {
    if (typeof text !== 'string' || (expected !== null && typeof expected !== 'string')) throw new Error('Invalid hooks document.')
    return saveHooks(await agentHome(), text, expected)
  })

  ipcMain.handle('bravebot:choose-directory', async () => {
    if (!window) return null
    const result = await dialog.showOpenDialog(window, {
      title: 'Open a project',
      properties: ['openDirectory', 'createDirectory'],
    })
    if (result.canceled) return null
    const directory = result.filePaths[0] ?? null
    // Recorded here, where a real directory has just been chosen, rather than being taken
    // from the renderer later. The recents list is the main process's own record; the
    // renderer can read it and ask to open something on it, and cannot write to it.
    if (directory && noteProject(directory)) rebuildMenu()
    return directory
  })

  /** The projects opened before, newest first. Read-only on purpose. */
  ipcMain.handle('bravebot:recents:read', () => recents())

  /** Which session came out of which. Read-only for the same reason the recents list is. */
  ipcMain.handle('bravebot:forks:read', () => forks())

  // Looking at the folder a session is working in.
  //
  // The pair that crosses is a session handle and a path relative to that session's directory,
  // and both are refused here before anything becomes a syscall. The roots are the main process's
  // own record of what the agent answered, so there is no channel on which the renderer can name a
  // folder — the promise `choose-directory` makes, kept for a second feature. `files.ts` has the
  // resolution and the reason a lexical check is only half of it.
  //
  // Note what this does not touch: `ALLOWED` above. The tree reaches no agent method, so the list
  // of things the renderer may ask the agent to do is exactly as long as it was.
  ipcMain.handle('bravebot:files:preview', (_event, session: unknown, path: unknown) => {
    return typeof session === 'string' && isSubpath(path) ? preview(session, path) : null
  })
  ipcMain.handle('bravebot:files:choose-attachments', (_event, session: unknown) => {
    if (!window || typeof session !== 'string') throw new Error('No active project.')
    return chooseAttachments(window, session)
  })
  ipcMain.handle('bravebot:files:search', (_event, session: unknown, query: unknown, hidden: unknown) => {
    return typeof session === 'string' && typeof query === 'string' && query.length < 1000 ? search(session, query, hidden === true) : { paths: [], incomplete: true }
  })
  ipcMain.handle('bravebot:files:list', (_event, session: unknown, path: unknown) => {
    if (!isSessionId(session) || !isSubpath(path)) return null
    return list(session, path)
  })

  ipcMain.handle('bravebot:files:open', async (_event, session: unknown, path: unknown) => {
    if (!isSessionId(session) || !isSubpath(path)) {
      // Deliberately not an echo of what arrived, the way the export refusal is not: the message
      // is for the person, and the payload is the one thing they cannot act on.
      return { status: 'failed', message: 'that is not a file in this project' }
    }
    return openInApp(session, path)
  })

  // What the window can currently do, so the menu can grey what it cannot. The renderer is
  // the only thing that knows this, and it says so rather than being asked: a menu that has
  // to poll would be a second copy of the transcript's state, arriving late.
  ipcMain.on('bravebot:menu:state', (_event, value: unknown) => {
    const state = parseWindowState(value)
    if (state) refreshMenu(state)
  })

  // A right-click on something in the window. The reference is validated rather than
  // trusted: it decides which menu is built, and an unrecognised one gets no menu at all.
  ipcMain.on('bravebot:menu:popup', (_event, value: unknown) => {
    const reference = parseContextRef(value)
    if (reference) popupContext(reference)
  })

  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})

/**
 * Closing stdin tells the agent the front-end has gone, which refuses anything waiting on
 * an approval. Doing it here rather than letting the process die means the refusal is
 * deliberate rather than a consequence of a dropped pipe.
 */
app.on('before-quit', () => {
  bridge?.dispose()
  bridge = null
})
