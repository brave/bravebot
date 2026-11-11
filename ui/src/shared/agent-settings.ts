/**
 * One hook, as the agent read it. `firesForNothing` is its answer about an entry naming a tool on a
 * moment that has no tool call, and is not worked out again here: what a hook is has one reader.
 */
export interface Hook { on: 'turn-started' | 'tool-finished' | 'turn-finished' | 'prompt-shown' | 'checkout-made' | 'compacted' | 'session-started' | 'session-ended'; tool: string | null; run: string[]; timeout: number | null; firesForNothing: boolean }
/** `entire` is false where the agent passed over part of the file, so composing it back from
 * `hooks` alone would drop what it did not read. */
export interface HooksDocument { path: string; text: string | null; entire: boolean; timeoutSeconds: TimeoutRange; hooks: Hook[] }
/** The whole numbers of seconds the agent accepts for a hook's `timeout`, as it reported them, and
 * the bound it holds an entry to on each moment when none is stated. The range and the bounds are the
 * agent's; a form that held its own copy would disagree with the reader the day one changed. */
export interface TimeoutRange { min: number; max: number; defaultFor: Record<Hook['on'], number> }
/** A refusal that only ever narrows: `value` is null where nothing named it, `path` the weakest file that did. */
export interface Refusal { value: boolean | null; path: string | null; managed: boolean }
/** What the settings in force say about the two refusing keys and the run limits. A null limit is the agent's built-in figure. */
export interface Limits {
  readsStayInWorkspace: Refusal
  bypassUnreachable: Refusal
  unreadable: { name: string; path: string }[]
  run: { defaultSeconds: number | null; maxSeconds: number | null; maxOutput: number | null }
}
export interface AgentSettings {
  build: string; configured: boolean; problem: string | null; model: string | null
  brave: boolean; bedrock: boolean; providers: {
    name: string; credential: string
    /** The service key the account list names the provider by (`gateway:<id>`), which two providers do not share even when they share a name. */
    key: string
  }[]
  selected: string | null; layers: string[]; overrides: { name: string; path: string }[]
  /** Keys a project file named that the agent reads from the home directory only. */
  ignored?: { name: string; path: string }[]
  /** Absent when an older agent built the report. */
  limits?: Limits
  managed: { path: string | null; keys: string[] }
  network: { roots: string[]; problem: string | null; trustsNothing: boolean; proxy: string | null; authenticated: boolean; unusableProxy: string | null; noProxy: string | null }
  /** Absent when an older agent built the report. */
  retrieval?: Retrieval
}

/** The position of the first hook whose `timeout` the agent would drop the entry for, or null. An
 * empty `timeout` is not sent, and so is not one. */
export function unusableTimeout(hooks: Hook[], range: TimeoutRange): number | null {
  const index = hooks.findIndex(({ timeout }) => timeout != null && !(Number.isInteger(timeout) && timeout >= range.min && timeout <= range.max))
  return index < 0 ? null : index
}

/** The file the agent will read back, composed from the entries an editor holds. */
export function composeHooks(hooks: Hook[]): string {
  const entries = hooks.map(({ on, tool, run, timeout }) => ({ on, ...(tool ? { tool } : {}), run, ...(timeout != null ? { timeout } : {}) }))
  return JSON.stringify({ hooks: entries }, null, 2) + '\n'
}
/** Whether bots recall earlier messages, as `settings.inspect` and `retrieval.set` report it (RETRIEVE-24). */
export interface Retrieval {
  enabled: boolean
  /** The person's own settings file, which the switch writes. */
  path: string | null
  /** The file that decided `enabled`, or null where none stated it. */
  from: string | null
  /** What fixes the key, where something does: the administrator's file, or the run's `--settings` file. */
  locked: { by: 'managed' | 'run'; path: string | null } | null
  /** Whether the person was shown where recall sends, for the endpoint it sends to now. */
  disclosed: boolean
  /** Where recall sends, or null where the settings choose no embeddings service it can use. */
  endpoint: { service: 'openai' | 'brave' | 'openrouter' | 'custom'; url: string; host: string; model: string; dimensions: number; development: boolean } | null
  /** Why no endpoint is chosen, where none is. */
  unconfigured?: string | null
  /** The services recall can embed with and what is kept for each. Absent from an older agent. */
  embeddings?: EmbeddingsChoice
}

export type EmbeddingsService = 'openai' | 'brave' | 'openrouter' | 'custom'

/** What `retrieval.embeddings.set` and `settings.inspect` say of the embeddings choice (RETRIEVE-35). */
export interface EmbeddingsChoice {
  /** The service the settings name, or null where they name none. */
  selected: EmbeddingsService | null
  /** The service recall sends to: the one selected, or Brave where none is and this build reaches it. */
  inForce: EmbeddingsService | null
  /** What fixes the choice, where something does. */
  locked: { by: 'managed' | 'run'; path: string | null } | null
  /** Every service, in the order the window lists them. */
  options: EmbeddingsOption[]
}

/** One service as the window offers it. Values are kept whether or not it is selected. */
export interface EmbeddingsOption {
  service: EmbeddingsService
  /** Whether it can be chosen at all: configured for inference, Brave reachable, or the custom endpoint. */
  available: boolean
  /** Why it cannot be used as it stands, or null where it can. */
  reason: string | null
  /** Where it sends; for the custom endpoint, the URL as written. */
  url: string | null
  host: string | null
  model: string | null
  /** The length of its vectors, published or found by a test call. */
  dimensions: number | null
  queryPrefix: string | null
  passagePrefix: string | null
  /** The custom endpoint only: whether a key is stored. The key itself never crosses. */
  keyStored?: boolean
  /** OpenAI only: the models offered. */
  models?: string[]
}

/** What `retrieval.embeddings.set` is sent. */
export type EmbeddingsSetParams = {
  select?: EmbeddingsService
  /** Whose values are written; defaults to `select`. */
  service?: EmbeddingsService
  /** A string writes, null removes, absence leaves as it is. `url` and `key` are the custom endpoint's alone. */
  values?: { model?: string | null; url?: string | null; queryPrefix?: string | null; passagePrefix?: string | null; key?: string | null }
  /** The URL the disclosure dialog showed, after a `needsDisclosure` answer. */
  disclosed?: string
}

/** A test call's outcome: the length the reply had, or what went wrong. */
export type EmbeddingsTest =
  | { ok: true; dimensions: number; millis: number }
  | { ok: false; kind: 'key' | 'not-found' | 'shape' | 'network' | 'other'; message: string }

/** What `retrieval.embeddings.set` answers. With `needsDisclosure` nothing was written. */
export interface EmbeddingsSetAnswer {
  state: Retrieval
  /** The service touched and selected, tested; null where nothing could be. */
  test?: EmbeddingsTest | null
  /** Recall is on and would move to this URL, which the person has not been shown. */
  needsDisclosure?: string
}

/** One embedding model OpenRouter lists, as `retrieval.embeddings.models` answers it (RETRIEVE-37). */
export interface EmbeddingsModel {
  id: string
  name: string
  contextLength: number | null
  /** Dollars per input token, as a decimal string. */
  pricing: { prompt: string | null }
}
