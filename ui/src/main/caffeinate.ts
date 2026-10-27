/**
 * Keep the computer from sleeping while a turn runs, when the person asked for that (CMD-12).
 *
 * Held only while the setting is on, the dialog explaining it has been agreed to, and some session
 * has a turn in flight, and let go the moment any of that stops being true. It keeps the machine
 * running and nothing else: the screen still dims and locks. Electron is passed in rather than
 * imported, so the rules can be tested without it.
 */

/** The part of Electron's `powerSaveBlocker` this uses. */
export interface Blocker {
  start(type: 'prevent-app-suspension'): number
  stop(id: number): void
}

export class Caffeinate {
  private enabled = false
  private busy = false
  private held: number | null = null

  /** `confirmed` is whether the dialog was agreed to in an earlier run. */
  constructor(private readonly blocker: Blocker, private confirmed = false) {}

  /** Whether the person has the setting on. Ignored until the dialog has been agreed to. */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled
    this.apply()
  }

  /** Whether any session has a turn in flight. */
  setBusy(busy: boolean): void {
    this.busy = busy
    this.apply()
  }

  /** The person agreed to the dialog. */
  confirm(): void {
    this.confirmed = true
  }

  get isConfirmed(): boolean {
    return this.confirmed
  }

  /** The window went away, and its setting and its turns with it. */
  windowClosed(): void {
    this.enabled = false
    this.busy = false
    this.apply()
  }

  /** Let go for good, as the app quits. */
  release(): void {
    this.windowClosed()
  }

  /** Whether the computer is being kept awake now. */
  get holding(): boolean {
    return this.held !== null
  }

  private apply(): void {
    const want = this.enabled && this.confirmed && this.busy
    if (want && this.held === null) this.held = this.blocker.start('prevent-app-suspension')
    if (!want && this.held !== null) {
      this.blocker.stop(this.held)
      this.held = null
    }
  }
}

/** Keep `running` to the sessions with a turn in flight, from one bridge event. */
export function trackTurns(running: Set<string>, message: { event?: string; session?: string | null }): void {
  if (!message.session) return
  if (message.event === 'turn.started') running.add(message.session)
  if (message.event === 'turn.done' || message.event === 'turn.error') running.delete(message.session)
}
