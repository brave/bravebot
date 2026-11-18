/** The parts of an open session that decide whether the composer may start a turn. */
export interface SendState {
  running: boolean
  askingTrust: string | null
  /** The questions asked once trust is answered: about the rules a checkout proposes, and about each directory it names. */
  rulesQuestion?: unknown
  folderQuestion?: unknown
}

/**
 * Whether a session is waiting on one of the questions asked before its first turn: trust, then
 * the rules its settings propose, then each directory they name. The bridge refuses a turn while
 * any is out, so the composer holds it back and says why.
 */
export function askingFirst(live: SendState | null | undefined): boolean {
  return !!live && (!!live.askingTrust || !!live.rulesQuestion || !!live.folderQuestion)
}

/**
 * Whether a draft may be sent now: a session is open, it is idle and not waiting on a question
 * asked before its first turn, the backend is not known to be unready, and the draft is not blank.
 *
 * One function so the Send button's callback, the run-plan callback and the Send menu item
 * cannot disagree about it.
 */
export function canSend(live: SendState | null | undefined, backendReady: boolean | null | undefined, draft: string): boolean {
  return !!live && !live.running && !askingFirst(live) && backendReady !== false && draft.trim().length > 0
}
