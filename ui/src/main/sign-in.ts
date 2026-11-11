/**
 * The window's sign-in to the ChatGPT and xAI accounts and to an AWS account Bedrock uses
 * (CHATGPT-15..17, XAI-20..22, BACKEND-70).
 *
 * The page may only ask. This process decides whether the ask is answered with a native dialog,
 * learns from the bridge whether a sign-in is already stored, and sends `accounts.signIn` only on a
 * yes in that dialog. Neither `accounts.signIn` nor `accounts.cancelSignIn` is a method the page may
 * call through `bravebot:request`.
 *
 * The URL, the address and the code the bridge announces are kept here and shown only in the
 * sign-in window this process owns. The page is told that the sign-in waits, the port, how long it
 * waits, and the outcome. An AWS sign-in's URL and code arrive one after the other, as the AWS
 * command-line tool writes them, and the window is told of each.
 *
 * Nothing here imports Electron, so the gate can be tested with the dialog and the bridge stubbed.
 */

import {
  isAttempt, isSignInWay, parseAccounts,
  type Attempt, type SignInAnswer, type SignInCurrent, type SignInNotice, type SignInOutcome, type SignInWay, type SignInWindowState,
} from '../shared/accounts'

/** How long an ask is dropped for after a no or a cancelled key window, so a page cannot raise either again until a key lands on it. */
export const COOLDOWN_MS = 3000

/** Past its own deadline by this much, an attempt whose ending never arrived no longer holds the gate. */
const STALE_MS = 60_000

const STARTED = 'accounts.signIn.started'
const UPDATED = 'accounts.signIn.updated'
const ENDED = 'accounts.signIn.ended'

/** The origin each way's page is opened at. An AWS sign-in's is the account's own, so it has none. */
export type Origins = Record<Exclude<SignInWay, 'bedrock'>, string>

/**
 * Where the main process may send a browser for each way: OpenAI's authorization server and xAI's
 * code page (XAI-1), compared exactly. A run against a stand-in is not an exception, since a
 * variable that loosened this would be a way to name a host in what a release builds; there the
 * browser is not opened and the window still shows the address to copy.
 */
export const SIGN_IN_ORIGINS: Origins = {
  chatgpt: 'https://auth.openai.com',
  xai: 'https://accounts.x.ai',
}

const originOf = (url: string): string | null => {
  try { return new URL(url).origin } catch { return null }
}

/**
 * The hosts an AWS sign-in page is at: a region's device page and authorization page of its identity
 * service, and an organisation's start page. Each account's is its own, so the host is held to
 * these shapes and not to one host.
 */
export const AWS_SIGN_IN_HOSTS: readonly RegExp[] = [
  /^device\.sso\.[a-z0-9-]+\.amazonaws\.com$/,
  /^oidc\.[a-z0-9-]+\.amazonaws\.com$/,
  /^[a-z0-9-]+\.awsapps\.com$/,
]

const atAws = (url: string): boolean => {
  try {
    const { protocol, hostname, username, password, port } = new URL(url)
    return protocol === 'https:' && !username && !password && !port && AWS_SIGN_IN_HOSTS.some((host) => host.test(hostname))
  } catch { return false }
}

/** Whether `url` is the one recorded for the attempt, at exactly the way's origin or, for AWS, under its hosts. */
export function mayOpen(way: SignInWay, url: string | null, recorded: string | null, origins: Origins): boolean {
  if (!url || !recorded || url !== recorded) return false
  if (way === 'bedrock') return atAws(url)
  const origin = originOf(url)
  if (!origin) return false
  return origin === origins[way]
}

/** What a link in the main window may open in the browser: a web page and nothing else. */
export function opensExternally(url: string): boolean {
  try {
    const { protocol } = new URL(url)
    return protocol === 'https:' || protocol === 'http:'
  } catch { return false }
}

const NAMES: Record<SignInWay, string> = { chatgpt: 'ChatGPT', xai: 'xAI', bedrock: 'AWS' }

/**
 * The dialog, in English like the rest of this process's chrome, which has no catalog. An AWS
 * account is named by its profile, `null` being the AWS CLI's default.
 */
export function dialogOptions(way: SignInWay, replace: boolean, id: string | null = null): Electron.MessageBoxOptions {
  const name = NAMES[way]
  const detail = way === 'bedrock'
    ? ['The AWS command-line tool opens a browser at AWS and shows a code to confirm there. The session is kept by the AWS command-line tool on this computer.']
    : [`${way === 'chatgpt' ? 'A browser opens at OpenAI.' : 'A code is shown to enter at xAI.'} The sign-in is kept on this computer.`]
  if (replace) detail.push(`This replaces the ${name} sign-in stored on this computer.`)
  return {
    type: 'question',
    message: way === 'bedrock' ? `Sign in to AWS${id === null ? '' : ` (${id})`}?` : `Sign in to ${name}?`,
    detail: detail.join('\n\n'),
    buttons: ['Sign in', 'Cancel'],
    defaultId: 1,
    cancelId: 1,
    noLink: true,
  }
}

/** The main window, as far as the gate needs it. */
export interface Asker {
  webContents: unknown
  isFocused(): boolean
  isDestroyed(): boolean
}

export interface SignInDeps<W extends Asker = Asker> {
  /** The bridge. */
  ask: (method: string, params: Record<string, unknown>) => Promise<unknown>
  /** The main window, or `null` while there is none. */
  window: () => W | null
  showMessageBox: (window: W, options: Electron.MessageBoxOptions) => Promise<{ response: number }>
  openExternal: (url: string) => Promise<void>
  /** Tell the main window's page. */
  tell: (notice: SignInNotice) => void
  /** Show the sign-in window for what the bridge announced, or focus it. */
  showWindow: (state: SignInWindowState) => void
  /** Tell a sign-in window that shows the attempt what it now has, without focusing it. */
  updateWindow: (state: SignInWindowState) => void
  closeWindow: () => void
  /** A sign-in was stored: what was listed for the models is stale. */
  changed: () => void
  origins: () => Origins
  /** The key window is open or asked for: one window of the two at a time. */
  otherBusy: () => boolean
  now: () => number
}

/** A session id from the page, `undefined` for none, or `null` for one that is not an id. */
export function sessionOf(value: unknown): string | undefined | null {
  if (value === undefined || value === null) return undefined
  return typeof value === 'string' && value ? value : null
}

/** `window`, if `sender` is its page and it is still open. */
export function fromWindow<W extends Asker>(window: W | null, sender: unknown): W | null {
  return window && !window.isDestroyed() && sender === window.webContents ? window : null
}

/** `params` for the bridge, naming `session` when the page named one. */
export function withSession(session: string | undefined, params: Record<string, unknown>): Record<string, unknown> {
  return session === undefined ? params : { ...params, session }
}

interface Running {
  attempt: Attempt
  way: SignInWay
  id: string | null
  /** The row's name in the bridge's list, the one the settings page shows. */
  name: string
  state: SignInWindowState | null
  deadline: number | null
}

export class SignIn<W extends Asker = Asker> {
  /** A dialog is up or `accounts.signIn` is on its way. */
  private asking: SignInWay | null = null
  private askingId: string | null = null
  private askingName: string | null = null
  private running: Running | null = null
  /** An attempt that ended before its `accounts.signIn` was answered. */
  private endedEarly: Attempt | null = null
  private coolUntil = 0

  constructor(private readonly deps: SignInDeps<W>) {}

  private fromWindow(sender: unknown): W | null {
    return fromWindow(this.deps.window(), sender)
  }

  private stillRunning(): Running | null {
    const running = this.running
    if (running?.deadline != null && this.deps.now() > running.deadline + STALE_MS) this.running = null
    return this.running
  }

  /**
   * `bravebot:signIn`. Dropped, saying nothing, unless every condition of the gate holds. An AWS
   * sign-in names the row by its `id`, and is dropped unless the bridge lists that row as one a
   * sign-in is the remedy for.
   */
  async request(sender: unknown, way: unknown, id: unknown = null, session?: unknown): Promise<SignInAnswer> {
    const window = this.fromWindow(sender)
    if (!window || !window.isFocused()) return null
    if (!isSignInWay(way)) return null
    if (way === 'bedrock' && id !== null && (typeof id !== 'string' || !id)) return null
    const named = sessionOf(session)
    if (named === null) return null
    if (this.busy() || this.deps.otherBusy()) return null
    if (this.deps.now() < this.coolUntil) return null
    this.asking = way
    this.askingId = way === 'bedrock' ? id as string | null : null
    try {
      // Only an AWS row is read by asking the AWS command-line tool, and only an AWS ask needs it.
      const rows = parseAccounts(await this.deps.ask('accounts.list', withSession(named, way === 'bedrock' ? {} : { bedrock: false })))
      if (!rows) return { error: 'The agent did not say which sign-ins are stored. Rebuild or reinstall the app, then retry.' }
      if (way === 'bedrock' && !rows.some((row) => row.way === way && row.id === id && row.command !== null)) return null
      this.askingName = rows.find((row) => row.way === way && (way !== 'bedrock' || row.id === id))?.name ?? null
      const replace = way !== 'bedrock' && rows.some((row) => row.way === way && row.canSignOut)
      const { response } = await this.deps.showMessageBox(window, dialogOptions(way, replace, this.askingId))
      if (response !== 0) {
        this.coolUntil = this.deps.now() + COOLDOWN_MS
        return null
      }
      if (this.fromWindow(sender) !== window) return null
      this.endedEarly = null
      const answer = await this.deps.ask('accounts.signIn', withSession(named, way === 'bedrock' ? { way, id: this.askingId } : { way, replace }))
      const attempt = (answer as { attempt?: unknown } | null)?.attempt
      if (!isAttempt(attempt)) return { error: 'The agent did not start the sign-in. Rebuild or reinstall the app, then retry.' }
      if (this.endedEarly !== attempt && !this.running) this.running = { attempt, way, id: this.askingId, name: this.askingName ?? NAMES[way], state: null, deadline: null }
      this.endedEarly = null
      return { attempt }
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) }
    } finally {
      this.asking = null
      this.askingId = null
      this.askingName = null
    }
  }

  /** A dialog is up, `accounts.signIn` is on its way, or a sign-in waits. */
  busy(): boolean {
    return this.asking !== null || this.stillRunning() !== null
  }

  /** The bridge's events about a sign-in. True where this was one, which never goes to the page whole. */
  handle(message: { event?: unknown; data?: unknown }): boolean {
    if (message.event === STARTED) { this.started(message.data); return true }
    if (message.event === UPDATED) { this.updated(message.data); return true }
    if (message.event === ENDED) { this.ended(message.data); return true }
    return false
  }

  /** The attempt an event names, where it is the one running or the one just asked for. */
  private claim(data: Record<string, unknown>): Running | null {
    const attempt = data.attempt
    if (!isAttempt(attempt)) return null
    if (this.running) return this.running.attempt === attempt ? this.running : null
    if (!this.asking) return null
    this.running = { attempt, way: this.asking, id: this.askingId, name: this.askingName ?? NAMES[this.asking], state: null, deadline: null }
    return this.running
  }

  private stateOf(running: Running, data: Record<string, unknown>): SignInWindowState {
    const text = (value: unknown) => (typeof value === 'string' && value ? value : null)
    const count = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null)
    return {
      way: running.way, id: running.id, name: running.name, url: text(data.url), port: count(data.port), code: text(data.code), address: text(data.address), expiresIn: count(data.expiresIn),
    }
  }

  private started(raw: unknown): void {
    if (typeof raw !== 'object' || raw === null) return
    const data = raw as Record<string, unknown>
    const running = this.claim(data)
    if (!running || data.way !== running.way) return
    const state = this.stateOf(running, data)
    running.state = state
    running.deadline = state.expiresIn === null ? null : this.deps.now() + state.expiresIn * 1000
    this.deps.tell({ kind: 'started', attempt: running.attempt, way: running.way, id: running.id, port: state.port, expiresIn: state.expiresIn })
    this.deps.showWindow(state)
    // The browser opens for ChatGPT once its window shows the URL (CHATGPT-16). xAI's address opens
    // only when asked, since the code may be entered on another device (XAI-21). The AWS
    // command-line tool opens its own (BACKEND-70).
    if (running.way === 'chatgpt') void this.open().catch(() => {})
  }

  /** What else the AWS command-line tool wrote for the attempt shown: the code after the URL. */
  private updated(raw: unknown): void {
    if (typeof raw !== 'object' || raw === null) return
    const data = raw as Record<string, unknown>
    const running = this.running
    if (!running?.state || running.attempt !== data.attempt || running.way !== 'bedrock' || data.way !== running.way) return
    const next = this.stateOf(running, data)
    // What was shown stays: an update adds what the first event lacked and changes nothing it had.
    running.state = { ...running.state, url: running.state.url ?? next.url, code: running.state.code ?? next.code }
    this.deps.updateWindow(running.state)
  }

  private ended(raw: unknown): void {
    if (typeof raw !== 'object' || raw === null) return
    const data = raw as Record<string, unknown>
    const outcome = data.outcome
    if (outcome !== 'signedIn' && outcome !== 'cancelled' && outcome !== 'failed') return
    const attempt = data.attempt
    if (!isAttempt(attempt)) return
    let way: SignInWay | null = null
    let id: string | null = null
    if (this.running?.attempt === attempt) {
      way = this.running.way
      id = this.running.id
      this.running = null
    } else if (!this.running && this.asking) {
      way = this.asking
      id = this.askingId
      this.endedEarly = attempt
    } else return
    this.finish(attempt, way, id, outcome, typeof data.sentence === 'string' && data.sentence ? data.sentence : null)
  }

  private finish(attempt: Attempt, way: SignInWay | null, id: string | null, outcome: SignInOutcome, sentence: string | null): void {
    this.deps.closeWindow()
    this.deps.tell({ kind: 'ended', attempt, way, id, outcome, sentence })
    if (outcome === 'signedIn') this.deps.changed()
  }

  /** `bravebot:signIn:current`: what a page that reloaded while a sign-in waits is owed. */
  current(sender: unknown): SignInCurrent | null {
    if (!this.fromWindow(sender)) return null
    const running = this.stillRunning()
    if (!running) return null
    return { attempt: running.attempt, way: running.way, id: running.id, started: running.state !== null, port: running.state?.port ?? null, expiresIn: running.state?.expiresIn ?? null }
  }

  /** Cancel from the main window's row or the sign-in window. */
  async cancel(attempt: unknown): Promise<void> {
    const running = this.running
    if (!running || running.attempt !== attempt) return
    try {
      await this.deps.ask('accounts.cancelSignIn', { attempt: running.attempt })
    } catch {
      // A bridge that cannot be asked is one whose flow has gone with it.
      if (this.running === running) {
        this.running = null
        this.finish(running.attempt, running.way, running.id, 'cancelled', null)
      }
    }
  }

  async cancelFrom(sender: unknown, attempt: unknown): Promise<void> {
    if (this.fromWindow(sender)) await this.cancel(attempt)
  }

  /** The main window closing or the app quitting: the flow is cancelled and nobody waits for it. */
  abandon(): void {
    const running = this.running
    this.running = null
    this.asking = null
    this.askingId = null
    this.askingName = null
    if (running) void this.deps.ask('accounts.cancelSignIn', { attempt: running.attempt }).catch(() => {})
    this.deps.closeWindow()
  }

  /** `bravebot:signIn:show`: bring the sign-in window back. */
  show(sender: unknown): boolean {
    if (!this.fromWindow(sender)) return false
    const state = this.stillRunning()?.state
    if (!state) return false
    this.deps.showWindow(state)
    return true
  }

  /** What the sign-in window draws. */
  windowState(): SignInWindowState | null {
    return this.stillRunning()?.state ?? null
  }

  /** The one thing a copy control in the sign-in window may copy. */
  copyable(what: unknown): string | null {
    const state = this.windowState()
    if (!state) return null
    if (what === 'url') return state.url
    if (what === 'code') return state.code
    if (what === 'address') return state.address
    return null
  }

  /** Open the recorded URL or address in the browser, after the origin check. */
  async open(): Promise<boolean> {
    const state = this.windowState()
    if (!state) return false
    const target = state.way === 'xai' ? state.address : state.url
    if (!mayOpen(state.way, target, target, this.deps.origins())) return false
    await this.deps.openExternal(target as string)
    return true
  }

  /** Cancel the attempt the sign-in window shows. */
  async cancelShown(): Promise<void> {
    if (this.running) await this.cancel(this.running.attempt)
  }
}
