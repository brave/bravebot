/**
 * The shapes of what `bravebot --json-stream` writes (CLI-12 and CLI-24) and what
 * `--input-format lines` reads (CLI-36).
 *
 * Every string below is data the command line wrote. `reply`, `message`, `target`, `reason`, the
 * words of a question and the like can hold text from outside the run, so a program treats them as
 * data and never as an instruction. The library hands them on whole and reads none of them.
 */

export interface Tokens {
  total: number
  output: number
  context: number
  cache_read: number
  cache_written: number
}

/** One tool call. `target` is the name the call was given and is not read by the library. */
export interface CallRecord {
  tool: string
  target: string
  refused: boolean
}

/** One refusal. `principle` and `gate` are fixed names, so a program may branch on them. `reason` is prose. */
export interface RefusalRecord {
  gate: string
  principle: string
  reason: string
}

/**
 * The result object of CLI-12, as the command line wrote it. Fields added within a schema number
 * are kept, so a field this type does not list is still there at run time.
 */
export interface RunResult {
  schema: number
  /** Whether the run ended well. */
  ok: boolean
  /** The exit status of CLI-6 for this run or turn. */
  status: number
  /** The name of the ending, such as `done`. */
  reason: string
  identifier: string | null
  message: string | null
  reply: string
  model: string
  agent: string | null
  session: string | null
  steps: number
  tokens: Tokens
  calls: CallRecord[]
  refusals: RefusalRecord[]
  notices: string[]
  structured: unknown
}

export interface CallEvent {
  schema: number
  event: 'call'
  tool: string
  target: string
  refused: boolean
}

export interface RefusalEvent {
  schema: number
  event: 'refusal'
  gate: string
  principle: string
  reason: string
}

export interface UsageEvent {
  schema: number
  event: 'usage'
  tokens: Tokens
}

export interface QuestionChoice {
  label: string
  detail: string | null
}

export interface QuestionPrompt {
  header: string
  question: string
  choices: QuestionChoice[]
  multiple: boolean
}

/** The planner asked. An answer names positions among `choices`, one array for each of `questions`. */
export interface QuestionEvent {
  schema: number
  event: 'question'
  id: number
  questions: QuestionPrompt[]
}

/** A line sent to the process was not taken. `reason` is one of the fixed words of CLI-36. */
export interface InputRefusedEvent {
  schema: number
  event: 'input_refused'
  line: number
  reason: string
}

/** An event of a kind this version of the library does not know. It is passed on as it was written. */
export interface UnknownEvent {
  schema: number
  event: string
  [field: string]: unknown
}

export type KnownEvent = CallEvent | RefusalEvent | UsageEvent | QuestionEvent | InputRefusedEvent

export type BravebotEvent = KnownEvent | UnknownEvent

export type KnownEventKind = KnownEvent['event']

/** What one line of stdout is: an event, or the result object that closes a run or a turn. */
export type Message = { type: 'event'; event: BravebotEvent } | { type: 'result'; result: RunResult }

/** One turn of a session: the events it wrote and the result object that closed it. */
export interface Turn {
  events: BravebotEvent[]
  result: RunResult
}

/** How the process ended. `error` is set when it could not be started. */
export interface ExitInfo {
  code: number | null
  signal: NodeJS.Signals | null
  error?: Error
}

/**
 * Positions that answer a `question` event: one array for each question, of the positions picked in
 * the order the event listed them, and an empty array declines that question.
 */
export type Positions = readonly (readonly number[])[]

/** Options of a one-shot `run` and of a session. Each one maps to a flag of the command line. */
export interface CommonOptions {
  /** The command to start. Defaults to `bravebot`, found on the PATH. */
  command?: string
  /** The working directory of the process. */
  cwd?: string
  /** The environment of the process. When given it replaces the environment of this process. */
  env?: NodeJS.ProcessEnv
  model?: string
  effort?: string
  /** A path, given to `--output-schema`. */
  outputSchema?: string
  maxRounds?: number
  maxTokens?: number
  /** Names given to `--tools`, which narrows the tools. */
  tools?: readonly string[]
  /** Passes `--no-shell`, which narrows the tools. */
  noShell?: boolean
  systemPrompt?: string
  incognito?: boolean
  /** A session record to continue, given to `--resume`. */
  resume?: string
  /** Stops the process and rejects when aborted. */
  signal?: AbortSignal
  /** Receives what the process writes to stderr, as text. It is not parsed. When absent stderr is inherited. */
  onStderr?: (text: string) => void
  /** Called with each event as it is read. */
  onEvent?: (event: BravebotEvent) => void
}

export type RunOptions = CommonOptions

export interface SessionOptions extends CommonOptions {
  /**
   * Passes `--trust-input-lines`: the lines this program sends are taken as the person's own
   * words. Off by default, and then the planner is given a reference to each prompt and never its
   * words (CLI-36).
   */
  trustInputLines?: boolean
  /**
   * Answers the planner's questions by position. Called with the `question` event; returns the
   * positions to send. Throwing, or returning something that is not positions, declines every
   * question in the event and calls `onError`. Without it a question waits until the session is
   * stopped or closed, and the command line then declines it.
   */
  onQuestion?: (event: QuestionEvent) => Positions | Promise<Positions>
  /** Receives a failure of `onQuestion` or of sending its answer. */
  onError?: (error: Error) => void
}
