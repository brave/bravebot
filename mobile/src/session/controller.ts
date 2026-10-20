import { RpcError, StaleActionError, UnsupportedError, type AgentSession, type AskAnswer } from '@brave/agent-client'
import { displayString } from '../text/displayText'

/** What the person answered a question with, held against its row because the view does not say. */
export type Decision = 'approved' | 'rejected'

export function describeError(error: unknown): string {
  if (error instanceof StaleActionError) return 'That question is not waiting any more.'
  if (error instanceof RpcError) {
    if (error.code === 'turn_in_flight') return 'A turn is already running.'
    return `The request was refused (${displayString(error.code)}).`
  }
  if (error instanceof UnsupportedError) return displayString(error.message)
  return 'Something went wrong. Try again.'
}

/**
 * Client-local state for one open session: the draft, what is being sent, the person's own answers
 * (the view does not say whether a resolved question was approved), and the last error. Session
 * state itself stays in the view; nothing here decides whether a turn is busy or a question valid.
 */
export class SessionController {
  draft = ''
  error: string | null = null
  sending = false
  replying = false
  private cancelling = false
  /** Replaced, never changed in place, so a component can tell by identity that it has changed. */
  decisions: ReadonlyMap<number, Decision> = new Map()
  private version = 0
  private readonly listeners = new Set<() => void>()

  constructor(
    readonly session: AgentSession,
    private readonly onActivity: () => void,
  ) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getVersion = (): number => this.version

  private changed(): void {
    this.version++
    for (const listener of [...this.listeners]) listener()
  }

  setDraft(text: string): void {
    this.draft = text
    this.changed()
  }

  /** Show a failure that did not come from one of this controller's own actions. */
  report(error: unknown): void {
    this.error = describeError(error)
    this.changed()
  }

  /** Run an action, and say how it ended: no failure, or what it threw, for a caller that must tell a refusal from a loss. */
  private async attempt(work: () => Promise<void>, flag: 'sending' | 'replying'): Promise<{ failed: false } | { failed: true; error: unknown }> {
    this[flag] = true
    this.error = null
    this.changed()
    try {
      await work()
      return { failed: false }
    } catch (error) {
      this.error = describeError(error)
      return { failed: true, error }
    } finally {
      this[flag] = false
      this.changed()
      this.onActivity()
    }
  }

  /** Send the draft, or the given text, such as a suggestion. The draft is changed only for a send of the draft, and only once it was accepted. */
  async send(given?: string): Promise<void> {
    const fromDraft = given === undefined
    const text = given ?? this.draft
    const prompt = text.trim()
    if (prompt === '' || this.sending) return
    const outcome = await this.attempt(async () => {
      await this.session.send(prompt)
    }, 'sending')
    // Text typed while the send was in flight is kept; only what was sent is taken out of the draft.
    if (!outcome.failed && fromDraft && this.draft.startsWith(text)) {
      this.draft = this.draft.slice(text.length)
      this.changed()
    }
  }

  /** Reply to the question the card showed. The client refuses it if that question is no longer on screen. */
  async decide(request: number, decision: 'approve' | 'reject'): Promise<void> {
    if (this.replying) return
    const pending = this.session.view.pending
    const row = pending !== null && pending.request === request ? pending.row : null
    // The view marks the question resolved before the reply returns, so the answer is held first.
    if (row !== null) this.decisions = new Map(this.decisions).set(row, decision === 'approve' ? 'approved' : 'rejected')
    const outcome = await this.attempt(() => this.session.decide(request, decision), 'replying')
    // A reply that was refused, or never sent, took no effect, so the answer goes. One whose outcome is unknown may
    // have taken effect, so the answer stays when the view already shows the question resolved.
    const failure = outcome.failed ? outcome.error : null
    const refused =
      failure instanceof StaleActionError || failure instanceof UnsupportedError || (failure instanceof RpcError && failure.outcome === 'rejected')
    const stillWaiting = this.session.view.pending?.row === row
    if (outcome.failed && row !== null && (refused || stillWaiting)) {
      const kept = new Map(this.decisions)
      kept.delete(row)
      this.decisions = kept
      this.changed()
    }
  }

  async answer(request: number, answers: AskAnswer[]): Promise<void> {
    if (this.replying) return
    const pending = this.session.view.pending
    const row = pending !== null && pending.request === request ? pending.row : null
    const outcome = await this.attempt(() => this.session.answer(request, answers), 'replying')
    // Answering nothing to every question is declining, and the transcript says so.
    if (!outcome.failed && row !== null && answers.length > 0 && answers.every((answer) => answer === null)) {
      this.decisions = new Map(this.decisions).set(row, 'rejected')
      this.changed()
    }
  }

  async trust(trusted: boolean): Promise<void> {
    if (this.replying) return
    await this.attempt(() => this.session.answerTrust(trusted), 'replying')
  }

  /** Never held back by a reply in flight: stopping work is always available. */
  async cancel(): Promise<void> {
    if (this.cancelling) return
    this.cancelling = true
    // Cancel refuses the question on screen, so that is the answer to record for it.
    const refused = this.session.view.pending?.row ?? null
    try {
      const result = await this.session.cancel()
      if (refused !== null && result.cancelled !== false && !this.decisions.has(refused)) this.decisions = new Map(this.decisions).set(refused, 'rejected')
      // The runtime stops a named turn only if it is still the one running; otherwise the person is told nothing was stopped.
      this.error = result.cancelled === false ? 'That turn had already ended, so nothing was stopped.' : null
      this.changed()
    } catch (error) {
      this.error = describeError(error)
      this.changed()
    } finally {
      this.cancelling = false
    }
    this.onActivity()
  }
}
