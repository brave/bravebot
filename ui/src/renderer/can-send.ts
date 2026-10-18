/** The parts of an open session that decide whether the composer may start a turn. */
export interface SendState {
  running: boolean
  askingTrust: string | null
}

/**
 * Whether a draft may be sent now: a session is open, it is idle and not waiting on the trust
 * question, the backend is not known to be unready, and the draft is not blank.
 *
 * One function so the Send button's callback, the run-plan callback and the Send menu item
 * cannot disagree about it.
 */
export function canSend(live: SendState | null | undefined, backendReady: boolean | null | undefined, draft: string): boolean {
  return !!live && !live.running && !live.askingTrust && backendReady !== false && draft.trim().length > 0
}

/**
 * Whether the conversation may be summarised now: the session is idle, trusted, and somebody has
 * said something in it. The agent decides whether enough of it is old enough, and says so.
 */
export function canCompact(live: (SendState & { entries: readonly { kind: string }[] }) | null | undefined): boolean {
  return !!live && !live.running && !live.askingTrust && live.entries.some((entry) => entry.kind === 'user')
}
