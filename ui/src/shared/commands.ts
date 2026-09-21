/**
 * Every action the interface offers from a menu, declared once.
 *
 * Three surfaces want this list: the native menu bar builds items from it, the accelerators
 * in that menu *are* the app's keyboard shortcuts, and an in-window menu draws rows from it.
 * Three hardcoded copies would drift — a menu that promises a shortcut nothing listens for
 * is worse than no menu — so the declaration lives here, in `shared`, which is the one place
 * the main process and the renderer can both reach. `layout.ts` next door plays the same
 * role for the remembered columns.
 *
 * Nothing here imports `electron` or `react`. That is what makes it shareable, and it is
 * why the accelerator is a string rather than anything that knows how to fire.
 *
 * ## What is deliberately absent
 *
 * There is no command that answers a question. The five the agent can ask — a write, a
 * command to run, whether the planner may read output, whether to vouch, and a series of
 * questions — are answered in the transcript, beside the evidence they are about, and
 * nowhere else. A keystroke that approved a write from across the window would be a
 * decision taken without looking at it, which is the one thing this whole app is arranged
 * to prevent. The absence is structural rather than a rule somebody has to remember: no
 * `CommandId` names an approval, so there is nothing for the menu layer to dispatch.
 */

export type CommandId =
  | 'session.new'
  | 'session.close'
  | 'session.export-tools'
  | 'session.export-text'
  | 'session.export-markdown'
  | 'session.export-pdf'
  | 'turn.send'
  | 'turn.cancel'
  | 'view.fold-left'
  | 'view.fold-right'
  | 'view.reset-columns'
  | 'view.theme'
  | 'app.about'
  | 'help.doctor'

/**
 * What has to be true of the window before a command means anything.
 *
 * Kept as a tag rather than a predicate closure so that it can cross the process boundary
 * with the rest of the declaration: the main process greys a menu item by it, and anything
 * drawing an in-window menu can grey a row by the same tag rather than by its own opinion.
 */
export type Requires = 'always' | 'session' | 'running' | 'sendable' | 'exportable' | 'forkable'

export interface Command {
  id: CommandId
  /** The resting label. Two commands override it with the state — see [`menuLabel`]. */
  label: string
  /** Electron's accelerator form, e.g. `CmdOrCtrl+N`. Absent where there is no shortcut. */
  accelerator?: string
  requires: Requires
  /**
   * A setting rather than an action: drawn with a tick when it is on.
   *
   * Declared here rather than left to each menu to decide, for the reason the whole file
   * exists — the native menu bar and the in-window menu draw the same row, and one of them
   * showing a tick the other does not would be two answers to "is this on".
   *
   * Which of them is on is [`isChecked`], reading the same [`WindowState`] the greying
   * reads. A checkbox command still dispatches on click; nothing here toggles anything, and
   * the renderer remains the only place the setting lives.
   */
  checkbox?: true
}

/**
 * As much of the window's state as a menu needs to know.
 *
 * Deliberately tiny. The renderer holds the session, the transcript and the columns; what
 * crosses to the main process is three booleans, because that is all that decides whether
 * an item is grey and what two of them are called. Sending more would make the main process
 * a second, stale copy of the renderer's state.
 */
export interface WindowState {
  hasSession: boolean
  running: boolean
  /**
   * There is a session, nothing is running, and the composer has something in it.
   *
   * Sent rather than derived from the first two, because whether the draft is empty is
   * known only to the renderer and it is the difference between a Send item that works and
   * one that is offered and then does nothing.
   */
  canSend: boolean
  /**
   * There is a session and somebody has said something in it.
   *
   * Sent rather than derived from `hasSession`, for the same reason `canSend` is: a session
   * that has been opened but not spoken in has nothing to write to a file, and an Export
   * item offered there would open a save sheet for a document the boundary is going to
   * refuse. Only the renderer can tell the difference — it holds the entries.
   */
  canExport: boolean
  /**
   * Whether an export should carry the tool calls as well as the conversation.
   *
   * A preference and not a capability, which makes it the one field here that does not
   * decide whether something is grey — it decides whether a row is ticked. It is sent for
   * the same reason the rest is: the renderer owns the setting, and a menu bar drawing its
   * own guess at it would be a second copy that could disagree.
   */
  includeTools: boolean
  folded: { left: boolean; right: boolean }
}

export const NOTHING_OPEN: WindowState = {
  hasSession: false,
  running: false,
  canSend: false,
  canExport: false,
  includeTools: false,
  folded: { left: false, right: false },
}

export const COMMANDS: readonly Command[] = [
  { id: 'session.new', label: 'New Session…', accelerator: 'CmdOrCtrl+N', requires: 'always' },
  {
    id: 'session.close',
    label: 'Close Session',
    accelerator: 'CmdOrCtrl+Shift+W',
    requires: 'session',
  },
  // Three items rather than one "Export…" that asks afterwards: the format *is* the choice,
  // and a menu that opened a picker somewhere else on screen would be putting the question
  // nowhere near the pointer that asked it. No accelerators — three formats cannot share one
  // and none of them is worth a key by itself.
  //
  // What goes *in* the file is a fourth row above those three rather than three more beside
  // them. It is not a format, and pairing it with each one would make six items where the
  // difference between two of them is a phrase at the end of a label. It sits with the
  // formats rather than in a preferences window because it is decided at the moment of
  // exporting, by somebody who knows who the file is for — and the native save sheet, which
  // is where the question really belongs, cannot be given a checkbox of our own.
  {
    id: 'session.export-tools',
    label: 'Include Tool Calls',
    requires: 'always',
    checkbox: true,
  },
  { id: 'session.export-text', label: 'Plain Text…', requires: 'exportable' },
  { id: 'session.export-markdown', label: 'Markdown…', requires: 'exportable' },
  { id: 'session.export-pdf', label: 'PDF…', requires: 'exportable' },
  { id: 'turn.send', label: 'Send', accelerator: 'CmdOrCtrl+Enter', requires: 'sendable' },
  { id: 'turn.cancel', label: 'Cancel Turn', accelerator: 'CmdOrCtrl+.', requires: 'running' },
  {
    id: 'view.fold-left',
    label: 'Hide Session List',
    accelerator: 'CmdOrCtrl+Alt+Left',
    requires: 'always',
  },
  {
    id: 'view.fold-right',
    label: 'Hide Context Panel',
    accelerator: 'CmdOrCtrl+Alt+Right',
    requires: 'always',
  },
  { id: 'view.reset-columns', label: 'Reset Columns', requires: 'always' },
  // Always available, including with nothing open: the window is painted whether or not there is
  // a session in it, and a person who has just launched the app is exactly who wants to change
  // how it looks. No accelerator — it is not a thing anybody does twice in a sitting, and the
  // shortcuts left are worth more to the folds.
  { id: 'view.theme', label: 'Theme…', requires: 'always' },
  { id: 'app.about', label: 'About Brave Bot', requires: 'always' },
  { id: 'help.doctor', label: 'Run Diagnostics…', requires: 'always' },
]

const BY_ID = new Map<CommandId, Command>(COMMANDS.map((command) => [command.id, command]))

/** The declaration for one id. Throws, because an unknown id is a typo, not a condition. */
export function command(id: CommandId): Command {
  const found = BY_ID.get(id)
  if (!found) throw new Error(`no such command: ${id}`)
  return found
}

/** Whether a command can be chosen, given what the window is currently doing. */
export function isEnabled(requires: Requires, state: WindowState): boolean {
  switch (requires) {
    case 'always':
      return true
    case 'session':
      return state.hasSession
    case 'running':
      return state.hasSession && state.running
    case 'sendable':
      return state.canSend
    case 'exportable':
      return state.canExport
    case 'forkable':
      // A fork cuts the conversation the session is holding, and a turn in flight is holding
      // it. The agent refuses one anyway; this is so the menu does not offer what it will
      // refuse.
      return state.hasSession && !state.running
  }
}

/**
 * Whether a checkbox command is currently on.
 *
 * One command, so this is a `switch` with one case and a `false` for everything else rather
 * than a field on the declaration: a `checked` that a non-checkbox command could carry would
 * be a state with two homes.
 */
export function isChecked(id: CommandId, state: WindowState): boolean {
  return id === 'session.export-tools' ? state.includeTools : false
}

/**
 * A window state, or nothing.
 *
 * The renderer is ours, but this value decides whether a menu item can be clicked, and the
 * main process has no other source for it. Nothing is coerced, for the reason `parseLayout`
 * gives next door: a half-understood message is not a state, and guessing at one is how a
 * menu ends up grey during a turn it should be able to cancel.
 *
 * `null` means "leave the menu as it was", which is the safe reading — a bad message can
 * never be the thing that *enables* an item.
 */
export function parseWindowState(value: unknown): WindowState | null {
  if (typeof value !== 'object' || value === null) return null
  const { hasSession, running, canSend, canExport, includeTools, folded } = value as Record<
    string,
    unknown
  >
  if (typeof hasSession !== 'boolean') return null
  if (typeof running !== 'boolean') return null
  if (typeof canSend !== 'boolean') return null
  if (typeof canExport !== 'boolean') return null
  if (typeof includeTools !== 'boolean') return null
  if (typeof folded !== 'object' || folded === null) return null
  const { left, right } = folded as Record<string, unknown>
  if (typeof left !== 'boolean' || typeof right !== 'boolean') return null
  return { hasSession, running, canSend, canExport, includeTools, folded: { left, right } }
}

/**
 * What a command is called right now.
 *
 * The two fold commands are the only ones that rename themselves, and they follow the
 * convention the fold buttons in the transcript header already use: the label says what
 * pressing it will do, not what the column currently is. A label that read "Session list"
 * and left the reader to infer the direction would be a state announced twice.
 */
export function menuLabel(item: Command, state: WindowState): string {
  if (item.id === 'view.fold-left') {
    return state.folded.left ? 'Show Session List' : 'Hide Session List'
  }
  if (item.id === 'view.fold-right') {
    return state.folded.right ? 'Show Context Panel' : 'Hide Context Panel'
  }
  return item.label
}

/** Whether an unknown string is a command this build declares. Used at the IPC boundary. */
export function isCommandId(value: unknown): value is CommandId {
  return typeof value === 'string' && BY_ID.has(value as CommandId)
}

// ------------------------------------------------------------------ context menus

/**
 * The kinds of thing that have a menu of their own when right-clicked.
 *
 * A closed union rather than a free string, because this is what the renderer is allowed to
 * say when it asks for a popup, and the main process builds the menu from it.
 */
export type ContextTarget = 'session' | 'entry' | 'entry-user' | 'directory'

export type ContextCommandId =
  /** Start a session in a project the main process named, from File > Open Recent. */
  | 'session.new-here'
  | 'context.session.open'
  | 'context.session.close'
  | 'context.session.copy-path'
  | 'context.entry.copy'
  | 'context.entry.fork'

/**
 * What appears on a right-click, decided here and built in the main process.
 *
 * The labels are compiled in. The renderer says *which kind of thing* was clicked and
 * *which one*, and nothing else — no path, no transcript text, no diff line. That matters
 * more here than anywhere else in the app: a transcript can hold content the agent read off
 * disk, and a native menu whose label came from that content, pointing at a path that came
 * from it too, is exactly the injection this whole program is arranged to prevent. So the
 * renderer cannot put a word on screen through this channel even if something has convinced
 * it to try.
 *
 * Note what an entry offers: copying, and nothing else. A confirm card, a command about to
 * be run and a quarantined blob all get the same one item as a plain message. Approving is
 * not on a context menu for the same reason it is not on an accelerator.
 *
 * A prompt the user typed offers one thing more, because it is the one row in a transcript that
 * came from the person reading it: forking. That is not an exception to the rule above. Forking
 * decides nothing — it opens a session holding what was said before that point and puts the
 * prompt in a composer to be edited — and every question the new session raises is still asked
 * in its transcript, beside the evidence. The renderer says a prompt was clicked; it still
 * cannot say what the menu reads.
 */
export const CONTEXT: Record<
  ContextTarget,
  readonly { id: ContextCommandId; label: string; requires?: Requires }[]
> = {
  // Not right-clickable: a directory reference only ever arrives from File > Open Recent,
  // which builds its own items. It is here so the union is total.
  directory: [],
  session: [
    { id: 'context.session.open', label: 'Open' },
    { id: 'context.session.close', label: 'Close Session' },
    { id: 'context.session.copy-path', label: 'Copy Project Path' },
  ],
  entry: [{ id: 'context.entry.copy', label: 'Copy' }],
  'entry-user': [
    { id: 'context.entry.copy', label: 'Copy' },
    { id: 'context.entry.fork', label: 'Fork From Here…', requires: 'forkable' },
  ],
}

/** Which thing was clicked. An identifier and a kind; never anything renderable. */
export interface ContextRef {
  target: ContextTarget
  id: string
}

export function parseContextRef(value: unknown): ContextRef | null {
  if (typeof value !== 'object' || value === null) return null
  const { target, id } = value as Record<string, unknown>
  // Checked against the table rather than against a list written out again here, so a target
  // added above cannot be one the boundary quietly refuses.
  if (typeof target !== 'string' || !isContextTarget(target)) return null
  if (typeof id !== 'string' || id.length === 0) return null
  return { target, id }
}

function isContextTarget(value: string): value is ContextTarget {
  return Object.hasOwn(CONTEXT, value)
}

/**
 * Every target's items, rather than a list of the targets that had any when this was written.
 * A new kind of thing to right-click should not need remembering here in order to work.
 */
export function isContextCommandId(value: unknown): value is ContextCommandId {
  return (
    typeof value === 'string' &&
    Object.values(CONTEXT).some((items) => items.some((item) => item.id === value))
  )
}
