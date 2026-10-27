/**
 * Keep the computer from sleeping while a turn runs, when the person asked for that (CMD-12).
 *
 * Held only while the setting is on and some session has a turn in flight, and let go the moment
 * either stops being true. It keeps the machine running and nothing else: the screen still dims
 * and locks. Electron is passed in rather than imported, so the rules can be tested without it.
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

  constructor(private readonly blocker: Blocker) {}

  /** Whether the person has the setting on. */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled
    this.apply()
  }

  /** Whether any session has a turn in flight. */
  setBusy(busy: boolean): void {
    this.busy = busy
    this.apply()
  }

  /** Let go for good, as the app quits. */
  release(): void {
    this.enabled = false
    this.busy = false
    this.apply()
  }

  /** Whether the computer is being kept awake now. */
  get holding(): boolean {
    return this.held !== null
  }

  private apply(): void {
    const want = this.enabled && this.busy
    if (want && this.held === null) this.held = this.blocker.start('prevent-app-suspension')
    if (!want && this.held !== null) {
      this.blocker.stop(this.held)
      this.held = null
    }
  }
}
