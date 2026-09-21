/**
 * Turning the event stream into something a transcript can draw.
 *
 * The agent reports what it is doing as a sequence of unrelated announcements. A reader
 * wants a single ordered column: what was asked, what was done on the way, and what came
 * back. This is where one becomes the other.
 *
 * The ordering rule is that everything is appended in arrival order and nothing is
 * reordered afterwards. A tool line is *mutated* when it finishes rather than appended
 * again, so a call occupies one row for its whole life.
 */

import type {
  Activity,
  AskAnswer,
  AskRequest,
  Change,
  ConfirmRequest,
  Landing,
  OutputRequest,
  RunRequest,
  Said,
  Shown,
  VouchRequest,
  VetRequest,
} from '../shared/protocol'
import type { ExportTurn } from '../shared/export'
import { CONSOLIDATION_MARK } from '../shared/bots'

export type Entry = (
  | { kind: 'turn-start'; id: string; number: number }
  | { kind: 'user'; id: string; text: string }
  | { kind: 'assistant'; id: string; text: string }
  | { kind: 'narration'; id: string; text: string }
  /**
   * A file that was put in front of the planner because somebody named it.
   *
   * The path and not the contents: a person reading a transcript wants to know the file was there,
   * and the file itself is on their disk. Drawing the whole of it would bury the exchange the
   * transcript is for — which is what happens without this.
   */
  | { kind: 'attached'; id: string; path: string }
  /**
   * A turn this app sent on the bot's behalf, drawn as the house-keeping it is.
   *
   * It carries no text, because there is no version of showing the words that is an improvement:
   * the prompt is boilerplate this file composed, the reply that follows says what came of it, and
   * a bubble full of the app talking to itself would push the conversation off the screen.
   */
  | { kind: 'consolidation'; id: string }
  /** A tool call. `landing` arrives after the call finishes, so it fills in late. */
  | { kind: 'tool'; id: string; activity: Activity; landing: Landing | null }
  | { kind: 'quarantined'; id: string; shown: Shown }
  /** A write awaiting a decision, or the record of one already made. */
  | {
      kind: 'confirm'
      id: string
      request: ConfirmRequest
      decision: 'approve' | 'reject' | null
    }
  /**
   * A pipeline awaiting a decision, or the record of one already made.
   *
   * `remember` is kept next to the decision because the two together are the answer: it
   * says an approval also vouched for the programs, which is why a run already decided
   * still reads differently from one merely allowed once.
   */
  | {
      kind: 'run'
      id: string
      request: RunRequest
      decision: 'approve' | 'reject' | null
      remember: boolean
    }
  /** A command's output awaiting a decision about whether the planner may read it. */
  | {
      kind: 'output'
      id: string
      request: OutputRequest
      decision: 'approve' | 'reject' | null
    }
  /** A quarantined path awaiting a decision about vouching for it. */
  | {
      kind: 'vouch'
      id: string
      request: VouchRequest
      decision: 'approve' | 'reject' | null
    }
  /**
   * A series of questions awaiting answers, or the record of ones already given.
   *
   * `answers` rather than a decision, because this is the one question that is not a yes or
   * a no. `null` means it still stands; an empty array is a real reply that declined
   * everything.
   */
  | { kind: 'vet'; id: string; request: VetRequest; decision: 'approve' | 'reject' | null }
  | { kind: 'ask'; id: string; request: AskRequest; answers: AskAnswer[] | null }
  | { kind: 'error'; id: string; text: string; category?: string | null; attempts?: number | null; status?: number | null }
  | { kind: 'watch'; id: string; text: string }
  /** A replayed tool line from a stored session: no outcome, because none was kept. */
  | { kind: 'replayed-tool'; id: string; text: string }
) & { interrupted?: boolean; turn?: number }

let counter = 0
const nextId = (): string => `e${++counter}`

/** What a stored session looked like, as entries. */
export function fromSaid(said: Said[]): Entry[] {
  return said.map((entry) => {
    switch (entry.kind) {
      case 'user': {
        // A file the *user* named is put into the conversation as a user message, under a line
        // saying whose contents follow. That is right for the planner and wrong for a person: the
        // agent filters its own house-keeping messages out of a replayed transcript and does not
        // filter this one, so a reopened session draws the file as though somebody had typed the
        // whole of it into the composer.
        //
        // Drawn as an attachment instead. Matching on the wording is what this file is otherwise
        // careful never to do, and it is a poor tool — but the alternative is drawing a lie, and
        // an attachment shown as a prompt is the kind of lie that matters here: it says a person
        // said something they did not.
        const watch = /^Watch (\d+) fired: ([^\n]+) looks written to since the last look\.\n\nNothing has been read\./.exec(entry.text)
        if (watch) return watchFired(Number(watch[1]), watch[2]!)
        const named = attached(entry.text)
        if (named) return { kind: 'attached', id: nextId(), path: named } as const
        // The same judgement one line up, for the same reason, on a string with none of that
        // one's difficulty: this app composed it, so the prefix is exact by construction rather
        // than a guess at somebody else's wording. A consolidation drawn as a prompt would say a
        // person asked for it, and nobody did.
        //
        // It errs in the other direction if somebody types those words into the composer
        // themselves, which is the same hazard the line above carries and is answered the same
        // way: the mark is long, dull and bracketed, so typing it is a thing somebody does on
        // purpose, and what they get for it is their prompt drawn as the house-keeping they were
        // imitating.
        if (entry.text.startsWith(CONSOLIDATION_MARK)) {
          return { kind: 'consolidation', id: nextId() } as const
        }
        return { kind: 'user', id: nextId(), text: entry.text } as const
      }
      case 'assistant':
        return { kind: 'assistant', id: nextId(), text: entry.text } as const
      case 'tool':
        // The record does not store what came of a call, so this must not be drawn as
        // though it had an outcome. See docs/phase-0-rpc-protocol.md §7.1.
        return { kind: 'replayed-tool', id: nextId(), text: entry.text } as const
    }
  })
}

/** The prefix the agent puts in front of a file it was handed. Its wording, not ours. */
const CONTENTS = 'Contents of '

/** The path a message is the contents of, or `null` if it is not one. */
function attached(text: string): string | null {
  if (!text.startsWith(CONTENTS)) return null
  const end = text.indexOf(':\n')
  if (end === -1) return null
  return text.slice(CONTENTS.length, end)
}

export const userSaid = (text: string): Entry => ({ kind: 'user', id: nextId(), text })
export const consolidating = (): Entry => ({ kind: 'consolidation', id: nextId() })
export const narrated = (text: string): Entry => ({ kind: 'narration', id: nextId(), text })
export const watchFired = (number: number, path: string): Entry => ({ kind: 'watch', id: nextId(), text: `File watch ${number}: ${path}` })
export const errored = (text: string): Entry => ({ kind: 'error', id: nextId(), text })
export const quarantined = (shown: Shown): Entry => ({ kind: 'quarantined', id: nextId(), shown })
export const started = (activity: Activity): Entry => ({
  kind: 'tool',
  id: nextId(),
  activity,
  landing: null,
})
export const asked = (request: ConfirmRequest): Entry => ({
  kind: 'confirm',
  id: nextId(),
  request,
  decision: null,
})
export const askedRun = (request: RunRequest): Entry => ({
  kind: 'run',
  id: nextId(),
  request,
  decision: null,
  remember: false,
})
export const askedOutput = (request: OutputRequest): Entry => ({
  kind: 'output',
  id: nextId(),
  request,
  decision: null,
})
export const askedVet = (request: VetRequest): Entry => ({ kind: 'vet', id: nextId(), request, decision: null })
export const askedVouch = (request: VouchRequest): Entry => ({
  kind: 'vouch',
  id: nextId(),
  request,
  decision: null,
})
export const askedQuestions = (request: AskRequest): Entry => ({
  kind: 'ask',
  id: nextId(),
  request,
  answers: null,
})
export const replied = (text: string, turn?: number): Entry => ({ kind: 'assistant', id: nextId(), text, turn })
export const turnStarted = (number: number): Entry => ({ kind: 'turn-start', id: nextId(), number })

/** Keep notices below the prompt even if a worker reports activity before turn.started. */
export function beginTurn(entries: Entry[], number: number): Entry[] {
  if (entries.some((entry) => entry.kind === 'turn-start' && entry.number === number)) return entries
  let at = entries.length
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index]!
    if (entry.kind === 'assistant' || entry.kind === 'turn-start') break
    if (entry.kind === 'user' || entry.kind === 'consolidation' || entry.kind === 'watch') { at = index + 1; break }
  }
  return [...entries.slice(0, at), turnStarted(number), ...entries.slice(at)]
}

/**
 * Attach a finished call to the row it started in.
 *
 * Matched on the last still-running tool row rather than by an id, because the engine
 * does not give calls one: it announces a start and later a finish, and the pairing is
 * positional. A finish with nothing running is appended as its own row rather than
 * dropped — an unexplained line is better than a missing one.
 */
export function finish(entries: Entry[], activity: Activity): Entry[] {
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index]
    if (entry?.kind === 'tool' && entry.activity.note === null) {
      const updated = [...entries]
      updated[index] = { ...entry, activity }
      return updated
    }
  }
  return [...entries, { kind: 'tool', id: nextId(), activity, landing: null }]
}

/** Where the last finished call's result went. */
export function land(entries: Entry[], landing: Landing): Entry[] {
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index]
    if (entry?.kind === 'tool' && entry.landing === null) {
      const updated = [...entries]
      updated[index] = { ...entry, landing }
      return updated
    }
  }
  return entries
}

/** Record what the user decided about a write. */
/** Every entry kind that puts something to the person. */
export type Asking = Extract<
  Entry,
  { kind: 'confirm' | 'run' | 'output' | 'vouch' | 'vet' | 'ask' }
>

/** Whether an entry awaits a decision or an answer. */
const isAsking = (entry: Entry): entry is Asking =>
  entry.kind === 'confirm' ||
  entry.kind === 'run' ||
  entry.kind === 'output' ||
  entry.kind === 'vouch' ||
  entry.kind === 'vet' ||
  entry.kind === 'ask'

/**
 * Whether it is still waiting.
 *
 * Approval requests carry a decision and a user question carries answers, so "unanswered" is not
 * one field. Note the asymmetry that matters: `answers: []` is *answered* — somebody
 * declined every question — where `null` means nobody has replied at all.
 */
const unanswered = (entry: Asking): boolean =>
  !entry.interrupted && (entry.kind === 'ask' ? entry.answers === null : entry.decision === null)

/** A decision-carrying question, which is every kind but `ask`. */
type Decided = Exclude<Asking, { kind: 'ask' }>

/**
 * Record what was answered.
 *
 * Matched on kind as well as id. The agent numbers its questions in one sequence per turn,
 * so ids do not collide — but a mismatch here would draw the answer on the wrong card, and
 * a card that says a person approved something they did not is the worst kind of wrong this
 * interface can be.
 */
export function decide(
  entries: Entry[],
  kind: Decided['kind'],
  request: number,
  decision: 'approve' | 'reject',
  remember = false,
): Entry[] {
  return entries.map((entry) =>
    isAsking(entry) && unanswered(entry) && entry.kind === kind && entry.request.request === request
      ? entry.kind === 'run'
        ? { ...entry, decision, remember }
        : { ...entry, decision }
      : entry,
  )
}

/**
 * The question still waiting on somebody, if there is one.
 *
 * At most one is ever outstanding: the turn blocks on the answer, so it cannot get as far
 * as asking a second thing. Searched from the end anyway, because that is where it is.
 */
export function outstanding(entries: Entry[]): Asking | null {
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index]
    if (entry && isAsking(entry) && unanswered(entry)) return entry
  }
  return null
}

/** Record the answers somebody gave to a series of questions. */
export function answered(
  entries: Entry[],
  request: number,
  answers: AskAnswer[],
): Entry[] {
  return entries.map((entry) =>
    entry.kind === 'ask' && unanswered(entry) && entry.request.request === request ? { ...entry, answers } : entry,
  )
}

/** A diff, condensed, as lines a reader can scan. */
export function diffLines(changes: Change[]): { sign: string; text: string; kind: string }[] {
  return changes.map((change) => {
    switch (change.kind) {
      case 'added':
        return { sign: '+', text: change.text, kind: 'added' }
      case 'removed':
        return { sign: '-', text: change.text, kind: 'removed' }
      case 'kept':
        return { sign: ' ', text: change.text, kind: 'kept' }
      case 'elided':
        return {
          sign: ' ',
          text: `⋯ ${change.lines} unchanged line${change.lines === 1 ? '' : 's'}`,
          kind: 'elided',
        }
    }
  })
}

/**
 * An entry as plain text, for the clipboard.
 *
 * Only the kinds that are text return any. A confirm card, a run, an output and a vouch all
 * come back `null` deliberately: what is on screen for those is a diff, an argv, some bytes
 * and a path — evidence, laid out to be read in place — and a "Copy" that flattened one of
 * them into a paragraph would produce something that reads like a record of the exchange
 * without being one. A quarantined blob does copy, because it is exactly text and copying it
 * to your own clipboard decides nothing.
 */
export function plainText(entry: Entry): string | null {
  switch (entry.kind) {
    case 'user':
    case 'assistant':
    case 'narration':
    case 'watch':
    case 'error':
    case 'replayed-tool':
      return entry.text
    case 'quarantined':
      // The preview, which is all the interface ever had: the kernel trimmed it before it
      // arrived and the full text was never sent here to copy.
      return entry.shown.preview.join('\n')
    default:
      return null
  }
}

/**
 * The conversation, for an export: what was asked and what came back.
 *
 * Filtered on `kind` first and only then passed through [`plainText`], which matters more
 * than it looks. `plainText` answers a question about the *clipboard* — it returns text for
 * narration, errors and replayed tool lines too — so "every entry it will speak for" is a
 * different set from "the conversation", and defining one in terms of the other would make
 * this quietly follow that function the next time it changes. Naming the two kinds here means
 * a twelfth `Entry` is left out of exports until somebody decides otherwise, which is the
 * safe direction for a file somebody keeps.
 *
 * What is left out is the same argument `plainText` makes about the five decision cards, one
 * level up: a diff, an argv, a confined blob and an approval are evidence laid out to be read
 * in place. Narration and errors go too — those are the machinery talking, not the exchange.
 *
 * `tools` adds the calls back, for somebody who asked for them in the Export menu. Only the
 * tool rows, and only the line the row already draws: a verb, a target and an outcome the app
 * wrote itself. The decision cards stay out at either setting, because widening "what was
 * done" to "what was approved" is the step that would turn a file into a claim about what
 * somebody looked at. A replayed call has no outcome to give — the record does not keep one —
 * so it crosses with a null note and the export says so rather than inventing one.
 */
export function conversation(entries: Entry[], tools = false): ExportTurn[] {
  const turns: ExportTurn[] = []
  for (const entry of entries) {
    if (entry.kind === 'user' || entry.kind === 'assistant') {
      const text = plainText(entry)?.trim()
      if (text) turns.push({ role: entry.kind, text })
      continue
    }
    if (!tools) continue
    if (entry.kind === 'tool') {
      const { verb, target, note, failed } = entry.activity
      turns.push({ role: 'tool', verb, target, note, failed })
    } else if (entry.kind === 'replayed-tool') {
      turns.push({ role: 'tool', verb: entry.text, target: '', note: null, failed: false })
    }
  }
  return turns
}


/** Search user-visible content, excluding protocol identifiers and internal metadata. */
export function searchableText(entry: Entry): string {
  if ('text' in entry) return entry.text
  switch (entry.kind) {
    case 'turn-start': return ''
    case 'attached': return entry.path
    case 'consolidation': return 'Updating persistent memory'
    case 'tool': return [entry.activity.verb, entry.activity.target, entry.activity.note, ...entry.activity.changes.map((change) => 'text' in change ? change.text : '')].join(' ')
    case 'quarantined': return [entry.shown.origin, entry.shown.label, ...entry.shown.preview].join(' ')
    case 'confirm': return [entry.request.path, entry.request.intent, ...entry.request.changes.map((change) => 'text' in change ? change.text : '')].join(' ')
    case 'run': return [entry.request.summary, entry.request.directory, ...entry.request.stages.map((stage) => stage.display)].join(' ')
    case 'output': return [entry.request.command, entry.request.summary, entry.request.output].join(' ')
    case 'vet': return [entry.request.origin, entry.request.expects, entry.request.content].join(' ')
    case 'vouch': return [entry.request.path, entry.request.preview].join(' ')
    case 'ask': return entry.request.prompts.map((prompt) => [prompt.header, prompt.question, ...prompt.rows.map((row) => `${row.label} ${row.detail ?? ''}`)].join(' ')).join(' ')
  }
}

/** Elided spans retain their lengths, so both source and proposed line numbers remain exact. */
export function numberedDiffLines(changes: Change[]): (ReturnType<typeof diffLines>[number] & { before: number | null; after: number | null })[] {
  let before = 1, after = 1
  return diffLines(changes).map((line, index) => {
    const change = changes[index]!
    if (change.kind === 'elided') {
      before += change.lines; after += change.lines
      return { ...line, before: null, after: null }
    }
    return { ...line, before: change.kind === 'added' ? null : before++, after: change.kind === 'removed' ? null : after++ }
  })
}


/** Ending a turn invalidates its pending approval channels; keep evidence without live controls. */
export function interruptPending(entries: Entry[]): Entry[] {
  return entries.map((entry) => isAsking(entry) && unanswered(entry) ? { ...entry, interrupted: true } : entry)
}
