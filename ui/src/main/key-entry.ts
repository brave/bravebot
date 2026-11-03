/**
 * The window's entry of a Brave Premium key or a gateway key (PREM-16, CRED-24).
 *
 * The page may only ask, from a row that says a key can be entered for it. This process asks the
 * bridge whether one can be kept before anything is typed, then shows a key window of its own. The
 * key is typed there, comes to this process, and goes to the bridge in `accounts.storeKey`; it is
 * never sent to the main window's page and never kept here. Whether a stored key is replaced is
 * what the bridge said is stored, not what the page said. Neither `accounts.keyBefore` nor
 * `accounts.storeKey` is a method the page may call through `bravebot:request`.
 *
 * Nothing here imports Electron, so the gate can be tested with the bridge and the window stubbed.
 */

import { isKeyWay, parseAccounts, type KeyAnswer, type KeyNotice, type KeySaveAnswer, type KeyWay, type KeyWindowState } from '../shared/accounts'
import { COOLDOWN_MS, fromWindow, sessionOf, withSession, type Asker } from './sign-in'

export interface KeyEntryDeps<W extends Asker = Asker> {
  /** The bridge. */
  ask: (method: string, params: Record<string, unknown>) => Promise<unknown>
  /** The main window, or `null` while there is none. */
  window: () => W | null
  /** Tell the main window's page. */
  tell: (notice: KeyNotice) => void
  /** Show the key window for `state`, or focus it. */
  showWindow: (state: KeyWindowState) => void
  closeWindow: () => void
  /** A key was stored: what was listed for the models is stale. */
  changed: () => void
  /** A sign-in is asked for or waits: one window of the two at a time. */
  otherBusy: () => boolean
  now: () => number
}

const text = (value: unknown): string | null => (typeof value === 'string' && value ? value : null)

export class KeyEntry<W extends Asker = Asker> {
  private asking = false
  private open: KeyWindowState | null = null
  /** The session the open window's key is checked against, as the page named it. */
  private session: string | undefined = undefined
  private saving = false
  /** Cancelled while a save was on its way: that save decides how the window ended. */
  private cancelledWhileSaving = false
  private coolUntil = 0

  constructor(private readonly deps: KeyEntryDeps<W>) {}

  private fromWindow(sender: unknown): W | null {
    return fromWindow(this.deps.window(), sender)
  }

  /** `bravebot:enterKey`. Dropped, saying nothing, unless every condition of the gate holds. */
  async request(sender: unknown, way: unknown, id: unknown, session?: unknown): Promise<KeyAnswer> {
    const window = this.fromWindow(sender)
    if (!window || !window.isFocused()) return null
    if (!isKeyWay(way)) return null
    const named = way === 'gateway' ? text(id) : null
    if (way === 'gateway' && !named) return null
    const inSession = sessionOf(session)
    if (inSession === null) return null
    if (this.busy() || this.deps.otherBusy()) return null
    if (this.deps.now() < this.coolUntil) return null
    this.asking = true
    try {
      // No key row is read by asking the AWS command-line tool, so the AWS rows are not checked.
      const rows = parseAccounts(await this.deps.ask('accounts.list', withSession(inSession, { bedrock: false })))
      if (!rows) return { error: 'The agent did not say which keys are stored. Rebuild or reinstall the app, then retry.' }
      const row = rows.find((r) => r.way === way && (way === 'brave' || r.id === named))
      if (!row?.canEnterKey) return null
      const params = withSession(inSession, way === 'gateway' ? { way, id: named } : { way })
      const before = (await this.deps.ask('accounts.keyBefore', params)) as Record<string, unknown> | null
      if (before?.ready !== true) {
        return { error: text(before?.sentence) ?? 'The agent did not say whether a key can be stored. Rebuild or reinstall the app, then retry.' }
      }
      if (!this.fromWindow(sender)) return null
      this.open = {
        way, id: named, name: row.name,
        host: text(before.host),
        replace: before.stored === true,
        variable: text(before.variable),
        records: before.records === true,
      }
      this.session = inSession
      this.deps.showWindow(this.open)
      this.deps.tell({ kind: 'opened', way, id: named })
      return { opened: true }
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) }
    } finally {
      this.asking = false
    }
  }

  /** The key window is asked for or open. */
  busy(): boolean {
    return this.asking || this.open !== null
  }

  /** `bravebot:enterKey:current`: the key window open, for a page that reloaded meanwhile. */
  current(sender: unknown): { way: KeyWay; id: string | null } | null {
    if (!this.fromWindow(sender) || !this.open) return null
    return { way: this.open.way, id: this.open.id }
  }

  /** `bravebot:enterKey:show`: bring the key window back. */
  show(sender: unknown): boolean {
    if (!this.fromWindow(sender) || !this.open) return false
    this.deps.showWindow(this.open)
    return true
  }

  /** What the key window draws. */
  windowState(): KeyWindowState | null {
    return this.open
  }

  /**
   * The key window's Save. A refusal is said in that window, which stays open so the key can be
   * typed again; a key stored closes it and tells the page.
   */
  async save(key: unknown): Promise<KeySaveAnswer> {
    const open = this.open
    if (!open || this.saving) return { refused: 'The key window is not waiting for a key.' }
    if (typeof key !== 'string') return { refused: 'Type or paste the key.' }
    this.saving = true
    this.cancelledWhileSaving = false
    try {
      const params: Record<string, unknown> = { way: open.way, key, replace: open.replace }
      if (open.id !== null) params.id = open.id
      const answer = (await this.deps.ask('accounts.storeKey', withSession(this.session, params))) as Record<string, unknown> | null
      if (this.open !== open) return { refused: 'The key window is not waiting for a key.' }
      if (answer?.outcome === 'stored') {
        this.finish('stored', text(answer.sentence))
        this.deps.changed()
        return { saved: true }
      }
      if (answer?.outcome === 'refused') return this.refused(text(answer.sentence) ?? 'The key was not stored.')
      return this.refused('The agent did not say whether the key was stored. Rebuild or reinstall the app, then retry.')
    } catch (error) {
      return this.refused(error instanceof Error ? error.message : String(error))
    } finally {
      this.saving = false
      this.cancelledWhileSaving = false
    }
  }

  private refused(sentence: string): KeySaveAnswer {
    if (this.cancelledWhileSaving) this.cancel()
    return { refused: sentence }
  }

  /** The key window's Cancel, or a person closing it. Nothing is stored. */
  cancel(): void {
    if (!this.open) return
    if (this.saving && !this.cancelledWhileSaving) { this.cancelledWhileSaving = true; return }
    this.coolUntil = this.deps.now() + COOLDOWN_MS
    this.finish('cancelled', null)
  }

  private finish(outcome: 'stored' | 'cancelled', sentence: string | null): void {
    const open = this.open
    if (!open) return
    this.open = null
    this.session = undefined
    this.deps.closeWindow()
    this.deps.tell({ kind: 'ended', way: open.way, id: open.id, outcome, sentence })
  }

  /** The main window closing or the app quitting: the key window goes and nothing is stored. */
  abandon(): void {
    this.open = null
    this.session = undefined
    this.asking = false
    this.deps.closeWindow()
  }
}
