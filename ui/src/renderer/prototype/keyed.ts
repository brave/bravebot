/**
 * What the window keeps for each session, as values that are replaced and not changed, so the
 * window holds them above the session on screen. A session's view is drawn afresh each time it is
 * shown, and what was typed in it, and what was sent from it and not yet answered, is not lost with
 * the view. The functions are the whole of how those are kept, so a test can say what survives.
 */

/** The sessions with a message out. */
export function withSending(all: ReadonlySet<string>, session: string, on: boolean): ReadonlySet<string> {
  const next = new Set(all)
  if (on) next.add(session)
  else next.delete(session)
  return next
}

/** The questions with an answer out, by session. */
export function withAnswerOut(all: ReadonlyMap<string, ReadonlySet<number>>, session: string, request: number, on: boolean): ReadonlyMap<string, ReadonlySet<number>> {
  const mine = new Set(all.get(session) ?? [])
  if (on) mine.add(request)
  else mine.delete(request)
  return new Map(all).set(session, mine)
}

/** What was typed and not sent, by session, after `change` is made to the session's own. */
export function withDraft(all: ReadonlyMap<string, string>, session: string, change: (now: string) => string): ReadonlyMap<string, string> {
  return new Map(all).set(session, change(all.get(session) ?? ''))
}
