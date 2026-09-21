/**
 * The wire format, as TypeScript.
 *
 * Mirrors `docs/phase-0-rpc-protocol.md` §6 and the Rust in `crates/bravebot-bridge/src/wire.rs`.
 * Nothing here is generated, so the two can drift; the tags are the contract and they are
 * pinned by tests on the Rust side.
 *
 * Two rules carried over from the spec, because they matter as much here as there:
 *
 * 1. Match on the tag, never on prose. `Landing` and `Reach` have `describe()` methods
 *    upstream that return sentences meant for a screen. Those sentences are not sent, and
 *    a UI that matched on one would be matching on wording that will change.
 * 2. An unrecognised tag degrades toward *less* trust. Every helper below that maps a tag
 *    to a rendering decision defaults to the quarantined/untrusted reading.
 */

export type Intent = 'create' | 'overwrite' | 'edit'
export type Phase = 'planning' | 'thinking' | 'compacting' | 'reconnecting'
export type Reach = 'not_the_planner' | 'no_model'
export type Landing = 'context' | 'quarantined' | 'reserved'
export type TodoStatus = 'pending' | 'active' | 'done'
export type SaidKind = 'user' | 'assistant' | 'tool'

export type Change =
  | { kind: 'kept'; text: string }
  | { kind: 'added'; text: string }
  | { kind: 'removed'; text: string }
  | { kind: 'elided'; lines: number }

export interface Activity {
  verb: string
  target: string
  /** `null` while the call is still running. Absent-vs-null matters: see the Rust doc. */
  note: string | null
  failed: boolean
  untrusted: boolean
  changes: Change[]
}

export interface Shown {
  origin: string
  reach: Reach
  label: string
  /** Already trimmed by the kernel to 12 lines of at most 160 characters. */
  preview: string[]
  /** The true total, so a preview can say what it left out. */
  lines: number
}

export interface Said {
  kind: SaidKind
  text: string
}

export interface TodoRow {
  content: string
  status: TodoStatus
}

export interface SessionSummary {
  id: string
  directory: string
  project: string
  branch: string | null
  title: string
  updated: number
  bytes: number
}

export interface SessionRecord {
  id: string
  directory: string
  branch: string | null
  title: string
  started: number
  updated: number
  turns: number
  tokens: number
  build: string | null
}

export interface OpenedSession {
  contextTokens?: number
  session: string
  model: string | null
  record: SessionRecord
  said: Said[]
  context: string
  todos: Record<string, TodoRow[]>
  trust: { known: boolean; rules: { path: string; integrity: string }[] | null }
  branchNote: string | null
  buildNote: string | null
  /**
   * How many messages compaction has taken out of this conversation, in total.
   *
   * Only ever rises, and rises exactly when the session stopped carrying what was said before the
   * summary. Anything a front-end put at the top of a session — a bot's briefing, say — has to be
   * said again when this has gone up since it last looked.
   */
  archived: number
}

export interface ModelOption {
  id: string
  name: string
  provider: string
  premium: boolean
  contextWindow: number | null
  /** Provider-reported capabilities; absent when the backend does not report them. */
  capabilities?: string[]
}

export interface ModelCatalogue {
  defaultModel: string
  models: ModelOption[]
  warnings: string[]
}

/**
 * A session begun from part of another one.
 *
 * `said` is the child's own transcript rather than a slice of the parent's, so a window draws a
 * fork from what the conversation says instead of from what it happened to have on screen.
 * `prefill` is the prompt it was cut in front of — handed back rather than kept, because the
 * point of forking is to ask it differently.
 */
export interface ForkedSession {
  contextTokens?: number
  session: string
  /** The child's durable id. Real from here, though its record waits for the first turn. */
  id: string
  directory: string
  branch: string | null
  said: Said[]
  prefill: string
  context: string
  turns: number
  todos: Record<string, TodoRow[]>
  trust: { known: boolean; rules: { path: string; integrity: string }[] | null }
  parent: {
    id: string
    directory: string
    /** What the parent is called, for a banner that has not read the session list yet. */
    title: string | null
    /** Which of its prompts the cut was made in front of. */
    prompt: number
  }
}

export interface Vetting { verdict: string; reason?: string | null; detail?: string | null }
export interface VetRequest { request: number; origin: string; expects: string; content: string; lines: number; vetting: Vetting }

export interface ConfirmRequest {
  remark?: { preview: string[]; lines: number; label: string } | null
  request: number
  path: string
  intent: Intent
  untrusted: boolean
  existing: boolean
  added: number
  removed: number
  exact: boolean
  changes: Change[]
}

export interface TurnDone {
  contextTokens?: number
  /** Added by the desktop main process while memory maintenance reserves this session. */
  consolidating?: boolean
  turn: number
  reply: string
  model: string
  steps: number
  clean: boolean
  tokens: number
  outputTokens: number
  notices: string[]
  trust: { rules: { path: string; integrity: string }[] }
  /**
   * The session's durable id, real from this event onwards.
   *
   * `null` only where a record could not be written. It is *this* turn that created one — a
   * session writes nothing until it has something to say — so a front-end keeping a note of its
   * own about a session learns its name here rather than by guessing which row in the list is
   * the one it just made.
   */
  id: string | null
  /** As on `OpenedSession`. */
  archived: number
}

export interface TurnError {
  category?: string | null
  attempts?: number | null
  status?: number | null
  contextTokens?: number
  /** A failed turn may still have saved a recoverable conversation. */
  id?: string | null
  turn: number
  kind: 'cancelled' | 'precommit' | 'workspace' | 'chat'
  message: string
}

/** One stage of a pipeline awaiting a decision. */
export interface Stage {
  /** The name as the planner wrote it. */
  program: string
  /**
   * What that name resolved to on this machine, or `null` if it did not resolve.
   *
   * Shown *alongside* the name, never instead of it: `$PATH` decides what `grep` means, so
   * the binary and the word for it are two different claims and a reviewer needs both.
   */
  resolved: string | null
  args: string[]
  /** The agent's own rendering of the argv, so both front-ends show the same characters. */
  display: string
}

/** A pipeline the planner wants to run. */
export interface RunRequest {
  request: number
  /** Resolved execution plan, including conditional joins and redirections. */
  plan?: string
  writes?: string[]
  stdin?: string | null
  stages: Stage[]
  directory: string
  /**
   * Whether approving would hand the user's own data to a program.
   *
   * A second and independent reason to be careful, on confidentiality rather than
   * integrity: bytes going into a program are released somewhere the policy stops
   * governing.
   */
  releasesPrivate: boolean
  /** What approving *and remembering* would cover — the thing the second answer is about. */
  vouches: { program: string; args: string[]; display: string }[]
  summary: string
}

/**
 * A command's output the planner has asked to read.
 *
 * `output` is the full bytes, and that is the point of the question rather than a leak:
 * somebody deciding whether the model may read something has to be reading it themselves.
 * A front-end that truncates this is asking for an approval of what nobody saw. It is
 * released for a screen and stops there — approving is how it reaches the planner, and
 * that path runs through the agent, never through here.
 */
export interface OutputRequest {
  vetting?: Vetting
  request: number
  command: string
  reference: string
  lines: number
  output: string
  summary: string
}

/** A quarantined file the planner would like to read. */
export interface VouchRequest {
  vetting?: Vetting
  request: number
  path: string
  preview: string
  /** Whether the preview is only part of the file. Load-bearing: a preview that stops
   *  without saying so reads as the whole thing. */
  truncated: boolean
}

/** One option the person may pick. `index` is what a selection reports back. */
export interface AskRow {
  index: number
  label: string
  detail: string | null
}

/**
 * One question, already shaped for a screen by the agent.
 *
 * Every choice became exactly one row, in order — nothing filtered, reordered or
 * truncated. A front-end draws what it was handed: building rows itself would put the
 * decision about which options exist back where the model's words could reach it.
 *
 * A question with no rows is not an error. It is one that can only be answered in the
 * person's own words.
 */
export interface AskPrompt {
  /** A few words naming what this asks about, for telling several questions apart. */
  header: string
  question: string
  rows: AskRow[]
  /** Whether more than one row may be picked. */
  multiple: boolean
  /** A stable string standing for the whole question. */
  key: string
}

/** A series of questions the planner is putting to the person. */
export interface AskRequest {
  request: number
  prompts: AskPrompt[]
}

/**
 * One answer.
 *
 * Declining is a first-class answer rather than an error: a question nobody wants to answer
 * is still answered, and the turn continues. Sending neither `chosen` nor `typed` is how
 * that is said.
 */
export type AskAnswer = { chosen?: number[]; typed?: string }

/** Every event the bridge emits, keyed by name. */
export interface EventMap {
  'agent.ready': { build: string; version: string; home: string | null }
  'trust.request': { directory: string }
  'turn.started': { turn: number }
  'watch.fired': { number: number; path: string }
  'watch.ended': { number: number; reason: string; message?: string }
  phase: { phase: Phase }
  narration: { text: string }
  'tool.started': Activity
  'tool.finished': Activity
  landed: { landing: Landing }
  quarantined: Shown
  todos: { rows: TodoRow[] }
  tokens: { written: number }
  audit: { turn: number; event: Record<string, unknown> }
  'confirm.request': ConfirmRequest
  'run.request': RunRequest
  'output.request': OutputRequest
  'vouch.request': VouchRequest
  'vet.request': VetRequest
  'ask.request': AskRequest
  'turn.done': TurnDone
  'turn.error': TurnError
}

export type EventName = keyof EventMap

/**
 * One event, as a discriminated union over its name.
 *
 * Written as a mapped type rather than `{ event: EventName; data: EventMap[EventName] }`,
 * which looks equivalent and is not: that form pairs every name with every payload, so
 * narrowing on `event` tells the compiler nothing and every handler has to cast. This
 * form gives one member per name, so `switch (message.event)` narrows `data` with it.
 */
export type BridgeEvent = {
  [N in EventName]: { event: N; session?: string; data: EventMap[N] }
}[EventName]

export interface BridgeFailure {
  code: string
  message: string
}

// ---------------------------------------------------------------- reading tags

/**
 * Whether content at this landing reached the planner.
 *
 * Anything unrecognised reads as *not* reaching it, which is the safe direction: calling
 * quarantined content "read by the model" understates the confinement, and calling
 * context "quarantined" merely overstates it.
 */
export function reachedThePlanner(landing: Landing | string): boolean {
  return landing === 'context'
}

/** Whether a tag names something the planner was kept away from. */
export function isConfined(landing: Landing | string): boolean {
  return landing !== 'context'
}
