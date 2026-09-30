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
  CutOff,
  ExposureRequest,
  FetchRequest,
  Landing,
  ManifestError,
  ManifestRequest,
  OutputRequest,
  RunRequest,
  Said,
  ServerRequest,
  SettingsRules,
  Shown,
  VouchRequest,
  VetRequest,
} from '../shared/protocol'
import type { ExportTurn } from '../shared/export'

export type Entry = (
  | { kind: 'turn-start'; id: string; number: number }
  /**
   * Something the person typed.
   *
   * `prompt` is the agent's own ordinal for it, which is what `session.fork` cuts on. Absent
   * for a prompt this window has added and not yet heard back about: such a prompt is in the
   * conversation but the window does not know where, and a fork is refused rather than guessed.
   */
  | { kind: 'user'; id: string; text: string; prompt?: number }
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
   * the prompt is boilerplate the main process composed, the reply that follows says what came of
   * it, and a bubble full of the app talking to itself would push the conversation off the screen.
   *
   * Which turns those are comes from the `consolidation` tag on the record, written by the send
   * that sent them. Nothing here reads a prompt's words to decide.
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
  /**
   * A URL awaiting a decision about whether to fetch it, or the record of one already made.
   *
   * No `remember`, because there is nothing to remember: an approval covers the one URL it was
   * given for and the next fetch asks again.
   */
  | { kind: 'fetch'; id: string; request: FetchRequest; decision: 'approve' | 'reject' | null }
  /**
   * A language server awaiting a decision about whether to start it, or the record of one
   * already made.
   *
   * No `remember`. An approval lasts for the conversation, and the agent keeps track of it: the
   * same language is not asked about again until the conversation closes.
   */
  | { kind: 'server'; id: string; request: ServerRequest; decision: 'approve' | 'reject' | null }
  /**
   * A frozen plan awaiting a decision about whether to run it, or the record of one already made.
   *
   * No `remember`. An approval covers this plan only, and does not approve the plan's writes.
   */
  | {
      kind: 'manifest'
      id: string
      request: ManifestRequest
      decision: 'approve' | 'reject' | null
      /**
       * The session's `deny` and `ask` rules, where it has any. The agent does not apply
       * permission rules to a manifest run, so the card names the rules the plan is not held to.
       */
      unheld?: string[]
    }
  /**
   * A file holding a credential, awaiting a decision about whether the model may read it, or
   * the record of one already made.
   *
   * No `remember`. An approval covers the file for the conversation, and the agent keeps track
   * of it.
   */
  | { kind: 'exposure'; id: string; request: ExposureRequest; decision: 'approve' | 'reject' | null }
  /**
   * The task a manifest run was asked to plan.
   *
   * Not a `user` entry. A run is not part of the conversation, so its task has no ordinal, cannot
   * be forked from, and is left out of an export.
   */
  | { kind: 'plan-task'; id: string; text: string }
  /**
   * What a manifest run's last step released for a screen, and the run's record where one was
   * written. The text can come from a file nobody vouched for.
   */
  | { kind: 'plan-reply'; id: string; text: string; record: string | null }
  /** A manifest run that stopped: declined, stopped by the person, or failed. */
  | { kind: 'plan-ended'; id: string; ended: ManifestError }
  | { kind: 'ask'; id: string; request: AskRequest; answers: AskAnswer[] | null }
  | { kind: 'error'; id: string; text: string; category?: string | null; attempts?: number | null; status?: number | null; cutOff?: CutOff | null }
  | { kind: 'watch'; id: string; text: string }
  /** A replayed tool line from a stored session: no outcome, because none was kept. */
  | { kind: 'replayed-tool'; id: string; text: string; why: string }
) & { interrupted?: boolean; turn?: number }

let counter = 0
const nextId = (): string => `e${++counter}`

/**
 * What a stored session looked like, as entries.
 *
 * A message somebody composed rather than typed arrives tagged, this app's own among them, so no
 * row here is chosen by reading a message's prose.
 */
export function fromSaid(said: Said[]): Entry[] {
  return said.map((entry) => {
    switch (entry.kind) {
      // Untagged, so somebody typed it, whatever it says. A prompt whose first line reads like
      // one of the rows below is still drawn with its words and its ordinal: the ordinal is what
      // a fork cuts on, and a prompt drawn as one of the interface's own rows loses both.
      case 'user':
        return { kind: 'user', id: nextId(), text: entry.text, prompt: entry.prompt } as const
      case 'assistant':
        return { kind: 'assistant', id: nextId(), text: entry.text } as const
      case 'tool':
        // The record does not store what came of a call, so this must not be drawn as
        // though it had an outcome. See docs/phase-0-rpc-protocol.md §7.1.
        return { kind: 'replayed-tool', id: nextId(), text: entry.text, why: entry.why } as const
      // The two the agent composed, and the one this app composes. Drawn from the fields rather
      // than from any text, which is the whole point of the tags: the words of an attached
      // message are the file's own, so a window that read them back to decide what row to draw
      // would let whoever wrote the file pick.
      case 'attached':
        return { kind: 'attached', id: nextId(), path: entry.path } as const
      case 'watch':
        return watchFired(entry.number, entry.path)
      case 'consolidation':
        return { kind: 'consolidation', id: nextId() } as const
      // The agent's own message carrying a file vet_content let through. A narration built from
      // the two fields, which are a reference name and a media type the agent chose.
      case 'vetted':
        return narrated(
          `The ${entry.media === 'application/pdf' ? 'PDF' : 'picture'} ${entry.reference} held was let through and attached for the model`,
        )
      default: {
        // A tag from a newer agent than this window. Drawn as a plain message and never as one of
        // the interface's own rows: those rows assert something about the conversation that this
        // build cannot check, and quoting asserts least. Drawn at all, because a message a
        // transcript leaves out silently is the failure the tags exist to prevent.
        const unknown = entry as { text?: string }
        return { kind: 'user', id: nextId(), text: unknown.text ?? '' } as const
      }
    }
  })
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
export const askedFetch = (request: FetchRequest): Entry => ({ kind: 'fetch', id: nextId(), request, decision: null })
export const askedServer = (request: ServerRequest): Entry => ({ kind: 'server', id: nextId(), request, decision: null })
export const askedManifest = (request: ManifestRequest, unheld: string[] = []): Entry => ({ kind: 'manifest', id: nextId(), request, decision: null, unheld })

/** The rules that narrow what a session does, which a manifest run is not held to. */
export const narrowing = (rules: SettingsRules | null | undefined): string[] => [...(rules?.deny ?? []), ...(rules?.ask ?? [])]

/** Whether a settings file wrote something that is not in force, which the person is told. */
export const notInForce = (rules: SettingsRules | null | undefined): boolean =>
  !!rules && (rules.unreadable.length > 0 || rules.proposed.length > 0 || rules.directories.length > 0)
export const askedExposure = (request: ExposureRequest): Entry => ({ kind: 'exposure', id: nextId(), request, decision: null })
export const planAsked = (text: string): Entry => ({ kind: 'plan-task', id: nextId(), text })
export const planReplied = (text: string, record: string | null): Entry => ({ kind: 'plan-reply', id: nextId(), text, record })
export const planEnded = (ended: ManifestError): Entry => ({ kind: 'plan-ended', id: nextId(), ended })
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

/**
 * The same entries, with `id`'s prompt numbered as the agent numbered it.
 *
 * The window draws a prompt the moment it is sent and learns where it landed when the turn ends,
 * because the turn adds user messages of its own on the way. Nothing is numbered where the agent
 * sent no ordinal: a fork of such a prompt is refused rather than cut in a guessed place.
 */
export function number(entries: Entry[], id: string, prompt: number | null | undefined): Entry[] {
  if (typeof prompt !== 'number') return entries
  return entries.map((entry) =>
    entry.id === id && entry.kind === 'user' ? { ...entry, prompt } : entry,
  )
}

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
/**
 * Every question answered with a yes or a no, and the method that carries the answer.
 *
 * Written down once. The kinds a card may answer, the entries that count as waiting, and the
 * method an answer is sent through all read this table, so a question added to it cannot be one
 * the transcript draws and the composer does not wait for, or one answered through a method
 * meant for another.
 *
 * A method per kind rather than one taking a kind, so an answer cannot be delivered to the wrong
 * question by getting a field wrong: the agent derives the kind from the method it was called on
 * and checks it against what is actually waiting.
 *
 * A series of questions is not here. Its reply is an answer per question and not a decision, so
 * it has a method and a callback of its own.
 */
export const REPLY = {
  confirm: 'confirm.reply',
  run: 'run.reply',
  output: 'output.reply',
  vouch: 'vouch.reply',
  vet: 'vet.reply',
  fetch: 'fetch.reply',
  server: 'server.reply',
  manifest: 'manifest.reply',
  exposure: 'exposure.reply',
} as const

/** Which kinds of question a person can answer with a yes or a no. */
export type Asked = keyof typeof REPLY

/** Every entry kind that puts something to the person. */
export type Asking = Extract<Entry, { kind: Asked | 'ask' }>

/** Whether an entry awaits a decision or an answer. */
const isAsking = (entry: Entry): entry is Asking =>
  entry.kind === 'ask' || Object.hasOwn(REPLY, entry.kind)

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
    case 'plan-task':
    case 'plan-reply':
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
    case 'tool': return [entry.activity.verb, entry.activity.target, entry.activity.why, entry.activity.note, ...entry.activity.changes.map((change) => 'text' in change ? change.text : '')].join(' ')
    case 'quarantined': return [entry.shown.origin, entry.shown.label, ...entry.shown.preview].join(' ')
    case 'confirm': return [entry.request.path, entry.request.intent, ...entry.request.changes.map((change) => 'text' in change ? change.text : '')].join(' ')
    case 'run': return [entry.request.summary, entry.request.directory, entry.request.line ?? '', ...entry.request.stages.map((stage) => stage.display)].join(' ')
    case 'output': return [entry.request.command, entry.request.summary, entry.request.output].join(' ')
    case 'vet': return [entry.request.origin, entry.request.expects, entry.request.content].join(' ')
    case 'fetch': return [entry.request.url, entry.request.host].join(' ')
    case 'server': return [entry.request.language, entry.request.program, entry.request.workspace].join(' ')
    case 'manifest': return [entry.request.task, ...entry.request.steps].join(' ')
    case 'exposure': return [entry.request.path, ...entry.request.credentials].join(' ')
    case 'plan-ended': return [entry.ended.problem ?? '', entry.ended.attempt?.plan ?? '', ...(entry.ended.attempt?.steps ?? [])].join(' ')
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

/**
 * What one ambient authority a command reaches costs, in the words a person reads.
 *
 * A sentence per kind rather than one with the name substituted into it, because what each of
 * them hands over is different: a container daemon is root on this machine, a logged-in tool is
 * an account somewhere else, the agent is a signature, a metadata service is the role this
 * machine runs as. The agent sends the kind and the word that named it and no sentence at all,
 * so this is where the desktop window says what it means.
 *
 * A kind this build does not know still says something. The list is the agent's and this window
 * may be older than it, and a row drawn with no sentence would be a grant with nothing said
 * about it, which is the one outcome the question exists to avoid.
 */
export function ambientSentence(authority: string): string {
  switch (authority) {
    case 'container-daemon':
      return 'the container daemon, which runs anything as root on this machine'
    case 'logged-in-tool':
      return 'already logged in, so it acts as you without asking you'
    case 'agent-socket':
      return 'your ssh agent, which signs with keys it never hands over'
    case 'metadata-service':
      return "this machine's metadata service, which hands out the credentials of the role it runs as"
    default:
      return 'access this window has no account of, which nobody is asked for and nothing here takes back'
  }
}
