/**
 * The only thing the renderer can reach.
 *
 * A handful of functions and two subscriptions. No child processes, no IPC surface beyond this: the
 * renderer asks the main process to call a named method, and the main process decides whether that
 * is a method at all. Nor any filesystem, with one measured exception below — the tree in the
 * context column can list a directory of the folder its own session is working in, and ask the
 * system to open a file there, naming both by a path relative to a root only the main process
 * holds. It cannot name a folder, and nothing here reads a file's contents.
 *
 * The layout and view pairs are the only things here that are not about the agent — they store
 * where the columns were and how the session list is arranged, and the main process checks that
 * that is all they are.
 *
 * The theme pair belongs with them: which palette the window is painted in is a fact about this
 * window, kept in the same file as the columns. What crosses is a name and never a colour, and the
 * one path that comes back is there to be printed in a sentence — nothing here takes a path, so
 * this side still cannot name a file.
 */

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { BridgeEvent, BridgeFailure } from '../shared/protocol'
import type { StoredLayout } from '../shared/layout'
import type { StoredView } from '../shared/view'
import type { StoredPanels } from '../shared/state'
import type { CommandId, ContextCommandId, ContextRef, WindowState } from '../shared/commands'
import type { ExportOutcome, ExportRequest } from '../shared/export'
import type { Fork } from '../shared/forks'
import type { Bot } from '../shared/bots'
import type { Listing, OpenOutcome, FilePreview, FileSearch, FileAttachment } from '../shared/files'
import type { Theme } from '../shared/theme'
import type { Experience } from '../shared/experience'

/** Everything the picker needs: what is on offer, which of them is chosen, and where to put one. */
export interface ThemeState {
  themes: Theme[]
  chosen: string
  /**
   * Where a palette somebody writes goes. Shown, and nothing more — the picker prints it in a
   * sentence so that "add your own" is an instruction rather than a hint. It is composed in the
   * main process and never travels back.
   */
  directory: string
}

export interface Answer<T> {
  ok?: T
  error?: BridgeFailure
}

const api = {
  readExperience(): Promise<Experience> { return ipcRenderer.invoke('bravebot:experience:read') as Promise<Experience> },
  writeExperience(key: string, value: unknown): Promise<Experience> {
    return ipcRenderer.invoke('bravebot:experience:write', key, value) as Promise<Experience>
  },
  /** Call one of the agent's methods. Never throws; failures come back in `error`. */
  request<T>(method: string, params?: Record<string, unknown>): Promise<Answer<T>> {
    return ipcRenderer.invoke('bravebot:request', method, params ?? {}) as Promise<Answer<T>>
  },

  /**
   * The columns from last launch — their widths and which were folded shut — or `null` for
   * a window that has never been arranged.
   *
   * Kept by the main process rather than in `localStorage`: the renderer runs on a
   * `file://` origin, whose storage Chromium discards between launches.
   */
  readLayout(): Promise<StoredLayout | null> {
    return ipcRenderer.invoke('bravebot:layout:read') as Promise<StoredLayout | null>
  },

  /** Remember where the columns were. Best-effort; the caller does not wait or check. */
  writeLayout(layout: StoredLayout): void {
    void ipcRenderer.invoke('bravebot:layout:write', layout)
  },

  /**
   * How the session list was arranged last launch — at the moment, whether it was grouped
   * by checkout. Never null: a column that has never been arranged is a flat one.
   *
   * Kept beside the layout and for the same measured reason, in a file of its own so that
   * one bad value cannot take the other down with it.
   */
  readView(): Promise<StoredView> {
    return ipcRenderer.invoke('bravebot:view:read') as Promise<StoredView>
  },

  /** Remember how the list was arranged. Best-effort; the caller does not wait or check. */
  writeView(view: StoredView): void {
    void ipcRenderer.invoke('bravebot:view:write', view)
  },

  /**
   * Which panels the context column was showing last launch. Never null: a column nobody has
   * arranged is one with every panel in it.
   *
   * Kept beside the layout and the view, in the one file the main process owns, and for the same
   * measured reason each of those gives.
   */
  readPanels(): Promise<StoredPanels> {
    return ipcRenderer.invoke('bravebot:panels:read') as Promise<StoredPanels>
  },

  /** Remember which panels are on. Best-effort; the caller does not wait or check. */
  writePanels(panels: StoredPanels): void {
    void ipcRenderer.invoke('bravebot:panels:write', panels)
  },

  /**
   * The palettes on offer and the one in force.
   *
   * The list is built in the main process from the set compiled into the app plus whatever JSON
   * somebody has written into its themes directory, because reading a directory is not something
   * this side does. The chosen name comes out of `bravebot-ui.json`, beside the columns.
   */
  readTheme(): Promise<ThemeState> {
    return ipcRenderer.invoke('bravebot:theme:read') as Promise<ThemeState>
  },

  /**
   * Choose a theme, by name.
   *
   * A name and nothing else, checked against the list on the way in — the renderer cannot hand
   * over a colour to paint with any more than it can name a file to read. Best-effort and
   * unanswered, like `writeLayout`: the window is already painted, and the round trip would only
   * confirm that the memory of it was written down.
   */
  writeTheme(name: string): void {
    void ipcRenderer.invoke('bravebot:theme:write', name)
  },

  /**
   * Listen for the palettes on disk changing. Returns an unsubscribe.
   *
   * The second subscription on this bridge, and the reason there is one: writing a palette is an
   * editing loop — save the file, look at the window, adjust a colour — and without this every
   * turn of it would mean quitting the app. Carries the same shape `readTheme` answers with, so
   * the renderer has one way of taking it in rather than two.
   */
  onThemeChanged(listener: (state: ThemeState) => void): () => void {
    const handler = (_event: IpcRendererEvent, state: ThemeState) => listener(state)
    ipcRenderer.on('bravebot:theme:changed', handler)
    return () => ipcRenderer.off('bravebot:theme:changed', handler)
  },

  /**
   * The projects opened before, newest first.
   *
   * There is no write half, deliberately: the main process records these when it hands out
   * a directory, so this list is something the renderer can show but not forge an entry in.
   */
  readRecents(): Promise<string[]> {
    return ipcRenderer.invoke('bravebot:recents:read') as Promise<string[]>
  },

  /**
   * Which session came out of which, newest first.
   *
   * No write half either, and for the same reason: the main process writes a line here from
   * what the agent answered when a fork was made, so a lineage on screen is one that happened.
   */
  readForks(): Promise<Fork[]> {
    return ipcRenderer.invoke('bravebot:forks:read') as Promise<Fork[]>
  },

  /** The bots defined here, by slug. */
  readBots(): Promise<Bot[]> {
    return ipcRenderer.invoke('bravebot:bots:read') as Promise<Bot[]>
  },

  /**
   * Define a bot, or change one that exists.
   *
   * The form fields and an optional creation avatar cross. The session id and compaction count
   * are both reports of what the *agent* did — the main process takes
   * them off its answers, the way it takes the fork lineage off one — so there is no way to claim
   * either from here. Neither is the slug: a name crosses, and the main process makes the thing
   * that becomes a filename out of it, so a path segment is never a string that arrived as one.
   */
  writeBotModel(slug: string, model: string): Promise<Bot | null> {
    return ipcRenderer.invoke('bravebot:bots:model', slug, model) as Promise<Bot | null>
  },

  writeBot(bot: {
    slug?: string
    /** The preview seed, used only when creating a bot. */
    avatar?: string
    model?: string | null
    name: string
    purpose: string
    directory: string
  }): Promise<Bot | null> {
    return ipcRenderer.invoke('bravebot:bots:write', bot) as Promise<Bot | null>
  },

  /**
   * Put a bot away, or bring it back out.
   *
   * The ordinary way a bot leaves the list. Nothing about it changes but the one field: its
   * session, its memory and its face are all still its own, which is what makes bringing it back
   * a restoration rather than a rebuild. Ask for it; what the field becomes is not this side's to
   * say, the same arrangement `releaseBotSession` below has.
   */
  retireBot(slug: string, retired: boolean): Promise<Bot | null> {
    return ipcRenderer.invoke('bravebot:bots:retire', slug, retired) as Promise<Bot | null>
  },

  /**
   * Forget a bot's definition. Its session and its memory file are left where they are.
   *
   * The second step rather than the first, and only offered from the archive. What it takes is
   * the thread tying a bot's pieces together — the slug that names its memory file, the seed its
   * face is drawn from, the id of its session — so a bot made again afterwards is a different bot
   * with the same name. `retireBot` above is what a row leaving the list ordinarily means.
   */
  removeBot(slug: string): Promise<string | null> {
    return ipcRenderer.invoke('bravebot:bots:remove', slug) as Promise<string | null>
  },

  /**
   * What a bot's memory currently says, or `null` if it has none to read yet.
   *
   * The words, never the path — which is what keeps this side of the wall unable to name a file.
   * The main process knows where a bot's memory lives because it put it there.
   */
  readBotMemory(slug: string): Promise<string | null> {
    return ipcRenderer.invoke('bravebot:bots:memory', slug) as Promise<string | null>
  },
  readMemoryHistory(slug: string): Promise<{ at: number; text: string; source: 'agent' | 'user' }[]> {
    return ipcRenderer.invoke('bravebot:bots:memory-history', slug) as Promise<{ at: number; text: string; source: 'agent' | 'user' }[]>
  },
  editBotMemory(slug: string, text: string, expected: string | null): Promise<string> {
    return ipcRenderer.invoke('bravebot:bots:edit-memory', slug, text, expected) as Promise<string>
  },

  /**
   * Let go of the session behind a bot, for a record the agent no longer has.
   *
   * Names a bot and nothing else: what its session becomes is decided over there, and the only
   * thing it can become is nothing.
   */
  releaseBotSession(slug: string): Promise<void> {
    return ipcRenderer.invoke('bravebot:bots:release', slug) as Promise<void>
  },

  /**
   * Send a turn as a bot.
   *
   * Separate from `request` above because a bot's turn carries files, and `turn.send` through that
   * channel has its file lists removed — a window that could name a file to read would be a window
   * that could have the planner read any file on the machine. So this names a *bot* and a moment:
   * `grounded` says whether this is the turn that has to carry the briefing, and the main process
   * decides what that means and which paths it is made of.
   */
  sendBotTurn(request: {
    session: string
    slug: string
    prompt: string
    grounded: boolean
    model?: string | null
    attachments?: string[]
  }): Promise<Answer<{ turn: number }>> {
    return ipcRenderer.invoke('bravebot:bots:send', request) as Promise<Answer<{ turn: number }>>
  },

  /**
   * One directory of the folder a session is working in, or `null` for anything it may not see.
   *
   * A session handle and a path *relative* to that session's directory — never a path. The main
   * process holds the roots, learned from what the agent answered when the session was opened, so
   * this cannot name a folder no session of this window is running in. That is the same promise
   * `chooseDirectory` below makes, and the reason there is no `readDirectory(path)` here.
   *
   * Names and kinds only. Nothing on this bridge reads a file's contents, so it adds no way for
   * something the agent was refused to reach the renderer regardless.
   */
  listFiles(session: string, path: string): Promise<Listing | null> {
    return ipcRenderer.invoke('bravebot:files:list', session, path) as Promise<Listing | null>
  },
  previewFile(session: string, path: string): Promise<FilePreview | null> {
    return ipcRenderer.invoke('bravebot:files:preview', session, path) as Promise<FilePreview | null>
  },
  chooseAttachments(session: string): Promise<FileAttachment[]> {
    return ipcRenderer.invoke('bravebot:files:choose-attachments', session) as Promise<FileAttachment[]>
  },
  searchFiles(session: string, query: string, hidden: boolean): Promise<FileSearch> {
    return ipcRenderer.invoke('bravebot:files:search', session, query, hidden) as Promise<FileSearch>
  },

  /**
   * Hand a file to whichever app the system assigns its type.
   *
   * The same pair, checked the same way, and only ever a regular file inside the session's own
   * folder. Never throws; a refusal comes back in `status`, because somebody double-clicked and
   * deserves to hear that nothing happened.
   */
  openFile(session: string, path: string): Promise<OpenOutcome> {
    return ipcRenderer.invoke('bravebot:files:open', session, path) as Promise<OpenOutcome>
  },

  /** A native picker grants this app run access to a configuration override. */
  selectSettings(clear = false): Promise<import('../shared/agent-settings').AgentSettings | null> {
    return ipcRenderer.invoke('bravebot:settings:select', clear)
  },
  readHooks(): Promise<import('../shared/agent-settings').HooksDocument> { return ipcRenderer.invoke('bravebot:hooks:read') },
  saveHooks(text: string, expected: string | null): Promise<import('../shared/agent-settings').HooksDocument> { return ipcRenderer.invoke('bravebot:hooks:save', text, expected) },
  /** Ask the user for a project directory, natively. */
  chooseDirectory(): Promise<string | null> {
    return ipcRenderer.invoke('bravebot:choose-directory') as Promise<string | null>
  },

  /**
   * Write the conversation to a file the user picks.
   *
   * The structured turns cross, not a finished document. The main process serializes them
   * and writes the result, so what lands on disk is something it composed from a value it
   * parsed rather than bytes this side handed over — the same discipline `writeLayout`
   * follows, for a value that matters rather more.
   *
   * There is no read half and no path argument: where the file goes is decided in a native
   * sheet, so the renderer cannot name a destination any more than it can name a project
   * directory. Never throws; a refusal comes back in `status`.
   */
  exportSession(request: ExportRequest): Promise<ExportOutcome> {
    return ipcRenderer.invoke('bravebot:export', request) as Promise<ExportOutcome>
  },

  /**
   * Listen for menu items being chosen. Returns an unsubscribe.
   *
   * One channel carrying one id, not a general "run this in the renderer" hook. The id is
   * checked against the command list on the way out and again on the way in, and that list
   * contains nothing that answers a question the agent asked — those are answered in the
   * transcript, beside the evidence, and a menu is not beside anything.
   */
  onCommand(
    listener: (id: CommandId | ContextCommandId, context: ContextRef | null) => void,
  ): () => void {
    const handler = (
      _event: IpcRendererEvent,
      id: CommandId | ContextCommandId,
      context: ContextRef | null,
    ) => listener(id, context)
    ipcRenderer.on('bravebot:command', handler)
    return () => ipcRenderer.off('bravebot:command', handler)
  },

  /**
   * Ask for the menu that belongs to a thing on screen.
   *
   * Carries which kind of thing and which one, and nothing else. What the menu says is
   * decided in the main process from labels compiled into it — the renderer cannot put a
   * word on screen this way, which is the point: a transcript can hold content the agent
   * read off disk.
   */
  popupContext(reference: ContextRef): void {
    ipcRenderer.send('bravebot:menu:popup', reference)
  },

  /**
   * Tell the menu what is currently possible.
   *
   * Without it "Cancel Turn" is black with nothing running and "Send" is black with an
   * empty composer — a menu offering what the window will refuse. Best-effort and
   * unanswered, like `writeLayout`: a menu item left momentarily wrong is not worth a
   * round trip.
   */
  publishState(state: WindowState): void {
    ipcRenderer.send('bravebot:menu:state', state)
  },

  /**
   * Listen for turns the main process sent on a bot's behalf, rather than a person.
   *
   * Two announcements over one channel pair: `consolidating` when such a turn goes out, and
   * `consolidated` when it ends however it ends. The window needs both — the first because a
   * transcript that drew a reply with no prompt above it would be a transcript with a hole in it,
   * and the second because such a turn carries the briefing, so by the time it is over the session
   * is grounded again and the next thing the user types must not carry it a second time.
   *
   * It carries a handle and a slug and no more. Nothing about what was said crosses here; the
   * words arrive as ordinary events like everything else.
   */
  onBotConsolidation(
    listener: (state: {
      session: string
      slug: string | null
      running: boolean
      delivered: boolean
    }) => void,
  ): () => void {
    const started = (_event: IpcRendererEvent, at: { session: string; slug: string }) =>
      listener({ ...at, running: true, delivered: false })
    const ended = (
      _event: IpcRendererEvent,
      at: { session: string; slug: string | null; delivered: boolean },
    ) => listener({ ...at, running: false })
    ipcRenderer.on('bravebot:bots:consolidating', started)
    ipcRenderer.on('bravebot:bots:consolidated', ended)
    return () => {
      ipcRenderer.off('bravebot:bots:consolidating', started)
      ipcRenderer.off('bravebot:bots:consolidated', ended)
    }
  },

  /** Listen for everything the agent announces. Returns an unsubscribe. */
  onEvent(listener: (event: BridgeEvent) => void): () => void {
    const handler = (_event: IpcRendererEvent, message: BridgeEvent) => listener(message)
    ipcRenderer.on('bravebot:event', handler)
    return () => ipcRenderer.off('bravebot:event', handler)
  },
}

contextBridge.exposeInMainWorld('bravebot', api)

export type BravebotApi = typeof api
