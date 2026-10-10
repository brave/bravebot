/**
 * The wire format, as TypeScript.
 *
 * Mirrors `docs/phase-0-rpc-protocol.md` §6 and the Rust in `crates/ui-bridge/src/wire.rs`.
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
/** A wait the window shows: the agent's phase, or a first turn starting the session's MCP servers. */
export type Waiting = Phase | 'starting-servers'

/** What a running confined check was given: lines of text, or one picture or PDF, which has none. */
/** A hook holding the turn open: the moment's word and the program as the person's file spells it. */
export type Hook = { moment: string; program: string }
export type Checking = { lines: number } | { file: 'picture' | 'pdf' }
export type Reach = 'not_the_planner' | 'no_model'
export type Landing = 'context' | 'quarantined' | 'reserved'
export type TodoStatus = 'pending' | 'active' | 'done'

export type Change =
  | { kind: 'kept'; text: string }
  | { kind: 'added'; text: string }
  | { kind: 'removed'; text: string }
  | { kind: 'elided'; lines: number }

export interface Activity {
  verb: string
  target: string
  /** Why the planner made the call, in its own words. Empty where it gave no reason. */
  why: string
  /** `null` while the call is still running. Absent-vs-null matters: see the Rust doc. */
  note: string | null
  failed: boolean
  untrusted: boolean
  changes: Change[]
  /**
   * Whole seconds this call spent at a model of its own, `null` where it asked none. The window
   * cannot time this itself, so without it a slow model and a slow program look the same.
   */
  waitedSeconds: number | null
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

/**
 * One thing said, from a conversation nobody watched happen.
 *
 * The last three were composed rather than typed, two by the agent and one by this app, and they
 * carry no text on purpose. A window writes its own sentence from the fields, because the text of
 * one of them is a file's own bytes and drawing that as anything but a plain message would let
 * whoever wrote the file pick which row it appears as. Rule 1 above, applied to the messages whose
 * prose is nobody's to read back. A `user` is therefore what somebody typed, whatever it says.
 */
export type Said =
  | {
      kind: 'user'
      text: string
      /**
       * Which of the user's messages this is, counted by the agent.
       *
       * The coordinate `session.fork` cuts on. It is here rather than counted in the window
       * because it is the agent's own numbering, and a window holding a second copy of the rule
       * agrees with it only for as long as nobody adds a kind of user message: a turn nudged for
       * spending its tool budget adds one, no event tells a window about it, and a fork of
       * anything after it is refused.
       */
      prompt?: number
    }
  | { kind: 'assistant'; text: string }
  | { kind: 'tool'; text: string; why: string }
  | { kind: 'attached'; path: string }
  | { kind: 'watch'; number: number; path: string }
  /**
   * A turn this app sent to bring a bot's memory up to date, rather than a person typing it.
   *
   * No text and no ordinal, for the same reason the two above carry neither: what a transcript
   * draws for this is a row about the turn, and the words are a prompt the main process composed.
   * The tag is written by the `composed` field of the `turn.send` that sent it, which is the only
   * tag a request may claim.
   */
  | { kind: 'consolidation' }
  /**
   * A picture or a PDF `vet_content` let through and attached for the model. Both fields are the
   * agent's own: a reference name and a media type from its table of extensions.
   */
  | { kind: 'vetted'; reference: string; media: string }

export interface TodoRow {
  content: string
  status: TodoStatus
}

/**
 * The permission rules a session opened under, read from the settings files when it opened.
 *
 * `deny`, `ask` and `allow` are the rules in force, as the files spelled them. The rest is what
 * a file wrote that is not in force: an entry that is not a rule, an `allow` rule a project's
 * file wrote, and a directory a file asked to have opened. Rule text is what a person wrote in a
 * settings file.
 */
export interface SettingsRules {
  deny: string[]
  ask: string[]
  allow: string[]
  /** An entry nothing could read as a rule, and the agent's sentence about why. */
  unreadable: { rule: string; said: string }[]
  /** An `allow` rule a project's settings file wrote. It answers no question here. */
  proposed: { rule: string; file: string }[]
  /** A directory a settings file asked to have opened. This app opens none of them. */
  directories: string[]
  /**
   * Every entry of the four `sandbox.filesystem` lists a confined program is held to, with the
   * file that wrote it (null for the command line) and, where it is not in force, the agent's
   * sentence about why. Absent from an older bridge.
   */
  filesystem?: { key: FilesystemKey; path: string; file: string | null; pinned: boolean; refused: string | null }[]
  /** `allowRead` and `allowWrite` a project's file wrote, which are not in force. Absent from an older bridge. */
  filesystemIgnored?: { key: FilesystemKey; file: string }[]
}

export type FilesystemKey = 'allowRead' | 'denyRead' | 'allowWrite' | 'denyWrite'

export interface SessionSummary {
  id: string
  directory: string
  project: string
  branch: string | null
  title: string
  updated: number
  bytes: number
  /**
   * Whether the record is a manifest run. A run has no conversation, so it is read with
   * `manifest.read` and cannot be opened or continued. Absent from an older bridge.
   */
  manifest?: boolean
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
  /** Which front end wrote the record: `terminal`, `desktop`, or null for one written before this was kept. */
  front: string | null
}

/**
 * A yes kept about a directory: when it was given, in seconds since the epoch, the file it is kept
 * in, and the directory it is about, which is the session's own or the root of the git worktree around it.
 */
export interface KeptTrust {
  at: number
  path: string
  root?: string
}

/** Why file backups may not account for everything a turn did (§7.1 `session.rewind`). */
export type RewindGap = 'command' | 'hook' | 'scratch' | 'language-server' | 'desktop' | 'backup-unavailable' | 'unknown'

/** One point `session.rewind` can put the session back to, as `turn.done` lists them (§8.2). */
export interface RewindPoint {
  /** The `steps` that reaches this point. */
  steps: number
  /** The turn it undoes. */
  turn: number
  /** That turn's prompt in the `Said.prompt` coordinate, or `null` where the conversation no longer holds it. */
  prompt: number | null
  text: string
  /** What the turn wrote over. */
  paths: string[]
  /** A string rather than `RewindGap`: a newer agent may name a gap this build has no words for. */
  gaps: string[]
}

export interface OpenedSession {
  /** Absent from an older bridge, which kept no rewind points. Newest first. */
  rewind?: RewindPoint[]
  contextTokens?: number
  session: string
  model: string | null
  record: SessionRecord
  said: Said[]
  context: string
  todos: Record<string, TodoRow[]>
  trust: { known: boolean; rules: { path: string; integrity: string }[] | null }
  /** The yes an earlier session here was told to remember, where it and not `trust` settled the question (TRUST-23). */
  remembered?: KeptTrust | null
  /** Where a remembered yes would be written, where the question is asked and remembering is offered. */
  keeping?: string | null
  branchNote: string | null
  buildNote: string | null
  /** Said when the other front end wrote the transcript above: it drew it, and this one will not draw it the same way. */
  frontNote: string | null
  /**
   * How many messages compaction has taken out of this conversation, in total.
   *
   * Only ever rises, and rises exactly when the session stopped carrying what was said before the
   * summary. Anything a front-end put at the top of a session — a bot's briefing, say — has to be
   * said again when this has gone up since it last looked.
   */
  archived: number
  /**
   * Whether a check that finds nothing reads quarantined content to the model with nobody asked.
   *
   * Settled by the agent when the session opened and not changed while it is open, so a window
   * says it once at the top of the transcript and goes on showing it (CHECK-11).
   */
  autoVetting: boolean
  /** Always `ask`: a mode is not read from the record (MODE-10). */
  permissionMode: PermissionMode
  /** Absent from an older bridge, which read no rules. */
  settingsRules?: SettingsRules | null
}

/**
 * How much a session's next turn asks before it acts, as `session.mode` names it.
 *
 * Three of the agent's four. Bypassing every check is reachable only through the command-line
 * flag, and the bridge refuses it from a window (MODE-11).
 */
export type PermissionMode = 'ask' | 'acceptEdits' | 'plan'

/** The order the mode shortcut walks, ending back at the first. */
export const PERMISSION_MODES: readonly PermissionMode[] = ['ask', 'acceptEdits', 'plan']

export function nextPermissionMode(mode: PermissionMode): PermissionMode {
  return PERMISSION_MODES[(PERMISSION_MODES.indexOf(mode) + 1) % PERMISSION_MODES.length]!
}

/**
 * The session put back `steps` turns, on disk and in the conversation (§7.1 `session.rewind`).
 *
 * `said` and the fields after it are `session.open`'s, read off the session as it now stands.
 */
export interface RewoundSession {
  session: string
  /** The turn the session now stands before. */
  turn: number
  /** The prompt that began that turn. */
  text: string
  /** Each path that did not go back. */
  refused: string[]
  gaps: string[]
  said: Said[]
  context: string
  contextTokens?: number
  archived: number
  todos: Record<string, TodoRow[]>
  trust: { rules: { path: string; integrity: string }[] | null }
  rewind: RewindPoint[]
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
  remembered?: KeptTrust | null
  keeping?: string | null
  /** The parent's, as it opened: the child carries on its conversation. */
  autoVetting: boolean
  /** Always `ask`, whatever the parent was in: a fork opens as any session does (MODE-11). */
  permissionMode: PermissionMode
  /** Absent from an older bridge, which read no rules. */
  settingsRules?: SettingsRules | null
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
/**
 * A picture or a PDF put to the person as a copy to open, since a transcript row cannot show what
 * the model would be shown. The copy is deleted once the question is answered.
 */
export interface VetPicture { path: string; media: string; bytes: number }

export interface VetRequest { request: number; origin: string; summary?: string; expects: string; content: string; lines: number; picture?: VetPicture | null; vetting: Vetting }

export interface ConfirmRequest {
  remark?: { preview: string[]; lines: number; label: string } | null
  /**
   * What the credential scan inferred about this body, where it inferred anything: the reason a
   * write the path's own rule would have let through unasked is being asked about. Each entry is
   * already a kind, a location and a masked preview, so drawing one repeats no part of the value.
   */
  credentials?: string[]
  /**
   * Whether the agent's own record holds a write to this path in the working directory after the
   * checkout the body comes from was made. Names only: it says nothing of whether the bytes differ.
   */
  writtenSinceCheckout?: boolean
  /**
   * What the write does to the file's line terminators, in the agent's words, or null where both
   * sides end in LF. The changes below are compared without terminators, so this is the only place
   * a swap of CRLF for LF, or a kept CRLF, is stated.
   */
  lineEndings?: string | null
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
  /**
   * Where this turn's prompt landed among the things the user said, or `null` where the
   * conversation does not hold it.
   *
   * The same coordinate `Said.prompt` carries, for the one prompt a window has just added
   * itself and so has no `Said` for. A turn adds a user message for every file named in the
   * prompt, and another if it spends its tool budget, so this is the only honest source.
   */
  prompt?: number | null
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
  /** As on `OpenedSession`. */
  rewind?: RewindPoint[]
}

/**
 * What a reply the output ceiling stopped was doing. `tool` is the request's own name for a tool
 * it offered, and `null` for a call to one it did not.
 */
export interface CutOff {
  ceiling: number
  call: { tool: string | null } | null
  thought: boolean
}

export interface TurnError {
  /** As on `TurnDone`. */
  prompt?: number | null
  /** As on `TurnDone`. */
  rewind?: RewindPoint[]
  category?: string | null
  attempts?: number | null
  status?: number | null
  /** Where the output ceiling ended the turn, and `null` otherwise. */
  cutOff?: CutOff | null
  contextTokens?: number
  /** A failed turn may still have saved a recoverable conversation. */
  id?: string | null
  /** What the turn said as it ran. A turn that failed has no reply to carry them on. */
  notices?: string[]
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
  /** The path the program is started by, which is `resolved` unless the name reached it through a link. */
  startedAs?: string
  /** The file the step runs, which is what the terminal shows under the step. */
  binary?: string
  args: string[]
  /** The agent's own rendering of the argv, so both front-ends show the same characters. */
  display: string
}

/**
 * Access a request reaches that nothing in the agent holds: the kind, and the word that named it.
 *
 * `authority` is what to match on. The sentence a person reads is the front end's, because the
 * words belong to the surface drawing them.
 */
export interface Ambient {
  authority: string
  named: string
}

/**
 * A URL the planner wants fetched.
 *
 * `host` is the agent's own reading of the URL and is drawn as sent, never worked out here from
 * `url`: `https://example.com@evil.test/` reads as one site and reaches the other, and the host
 * is what a yes agrees to talk to. Nothing of a reply is here, since nothing has been fetched,
 * and what comes back is confined whatever is answered.
 *
 * `ambient` is empty for every host but a machine's metadata service, which hands out the
 * credentials of the role the machine runs as to whatever reaches it.
 */
export interface FetchRequest {
  request: number
  url: string
  host: string
  ambient?: Ambient[]
  summary: string
}

/**
 * A language server the planner wants started.
 *
 * `program` is the absolute path the server's name resolved to, drawn as sent. The agent picks
 * the name from its own table, so nothing a turn read chooses what runs.
 *
 * When `runsBuildTooling` is true, starting the server runs code from the dependency tree with
 * the person's own access, as a build does. The card must say so. When `declared` is true the
 * person declared the server themselves, so what starting it runs is not known and the card
 * says that in place of either sentence (LSP-11). `args` are the arguments it is started with.
 */
export interface ServerRequest {
  request: number
  language: string
  program: string
  args: string[]
  /** The arguments as one line, quoted where one holds a space, so it reads back into the list. */
  argumentsLine: string
  workspace: string
  runsBuildTooling: boolean
  declared: boolean
  summary: string
}

/**
 * A frozen plan a manifest run is about to walk.
 *
 * `task` is what the person typed. `steps` has one line per step, in order, as the agent
 * rendered it. Every line is drawn, because the answer covers the whole plan.
 */
export interface ManifestRequest {
  request: number
  task: string
  steps: string[]
}

/**
 * What a manifest run produced, whether or not it finished.
 *
 * All of it came from a planner that was shown the task and nothing else, or from the agent's
 * own account of what it did. `proposed` is the planner's manifest verbatim, which is all there
 * is to read when it could not be made into a plan.
 */
export interface PlanAttempt {
  goal: string | null
  proposed: string | null
  plan: string | null
  steps: string[]
}

/**
 * A saved manifest run, read back with `manifest.read`.
 *
 * `failure` is the agent's sentence about why the run stopped, and is null for a run that
 * finished. What the run released for a screen is not saved, so it is not here.
 */
export interface RunRecord {
  record: SessionRecord
  model: string | null
  manifest: PlanAttempt & { failure: string | null }
}

/** A manifest run that finished. `record` names the run's own record, where one was written. */
export interface ManifestDone {
  run: number
  /**
   * What the plan's last step released for a screen. This can be the text of a file nobody
   * vouched for, so it is drawn as plain text in a marked container and never formatted.
   */
  reply: string
  model: string
  steps: number
  clean: boolean
  tokens: number
  outputTokens: number
  notices?: string[]
  attempt: PlanAttempt | null
  record: string | null
}

/** A manifest run that stopped: declined, stopped by the person, or failed. */
export interface ManifestError {
  run: number
  kind: string
  message: string
  category: string | null
  attempts?: number | null
  status?: number | null
  /** Whether the person stopped it. A stopped run is not recorded. */
  stopped: boolean
  /** Whether the plan was put to the person and declined. Read this, not the wording of `problem`. */
  declined: boolean
  /** The agent's own sentence about why, where it wrote one. Absent for a service failure. */
  problem: string | null
  attempt: PlanAttempt | null
  record: string | null
  notices?: string[]
}

/**
 * A vouched file the planner asked to read, which the scan found a credential in.
 *
 * `credentials` has one line per finding, as the agent wrote it: the kind, where it is, and a
 * mask of the value. No line holds any part of a value, and the file's text is not sent.
 */
export interface ExposureRequest {
  request: number
  path: string
  credentials: string[]
  summary: string
}

/** One variable an MCP server receives: its name, and whether the declaration stores its value. */
export interface McpVariable { name: string; stored: boolean }

/**
 * Whether to use an MCP server a project requests, asked before it is started (SERVERS-4).
 *
 * Everything the declaration says, as fields. A local server has `command`, the words it is
 * started with, and `program` where that resolved somewhere other than its first word; a remote
 * one has `url`. A stored value is named and never sent. `fetching` is the agent's own lines
 * about a runner that fetches what it runs when it starts (SERVERS-6), empty for any other
 * program. `digest` is what an approval binds to, so editing the declaration asks again.
 *
 * Three answers: no, yes, and yes for every server a checkout in this project requests from now
 * on, which is `remember` on the reply.
 */
export interface McpServerRequest {
  request: number
  alias: string
  transport: 'stdio' | 'http'
  command: string[] | null
  url: string | null
  program: string | null
  variables: McpVariable[]
  reads: string[]
  directory: string | null
  digest: string
  requestedBy: string
  changed: boolean
  fetching: string[]
  /** The question as the agent words it, one string per line. */
  lines?: string[]
}

/** One tool on an MCP server's list, as the client drew it. `description` is the server's text. */
export interface McpTool { name: string; arguments: string[]; description: string | null }

/**
 * The tools an MCP server lists, put to the person before any is offered to the model
 * (SERVERS-8). `refused` counts tools the client would not draw, and `changed` says a different
 * list was approved under this declaration before. A yes is remembered for this exact list.
 */
export interface McpToolsRequest {
  request: number
  alias: string
  tools: McpTool[]
  refused: number
  changed: boolean
  vetting: Vetting
}

/**
 * One call to an MCP server's tool (SERVERS-7). `name` is `alias:tool`. Each argument's value is
 * JSON, as the model wrote it. Three answers: no, yes, and yes without asking again about this
 * tool in this project, which is `remember` on the reply and offered only where `mayStand`.
 */
export interface McpCallRequest {
  request: number
  alias: string
  tool: string
  name: string
  arguments: { name: string; value: string }[]
  description: string | null
  mayStand: boolean
}

/**
 * A remote MCP server whose reply pointed somewhere it is not declared (SERVERS-11). Nothing was
 * sent there. `authority` is the host and port the destination reaches, taken from it by the
 * agent, and is what a yes agrees to. `mayRecord` says whether a yes outlasts this session.
 */
export interface McpMoveRequest {
  request: number
  alias: string
  declared: string
  destination: string
  authority: string
  mayRecord: boolean
}

/** What starting a session's MCP servers came to: the ones started, and why each other was not. */
export interface McpStarted { servers: string[]; confined: boolean; notes: string[] }

/** A pipeline the planner wants to run. */
export interface RunRequest {
  request: number
  /**
   * The line as the planner spelled it, shown above the plan as context.
   *
   * Not what an approval binds to: two spellings that compile to the same plan are one
   * endorsement. It is drawn all the same, because a reader given only the plan has nothing
   * to compare it against. Empty for a call spelled as argv stages rather than as a line.
   */
  line?: string
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
  /** What the stages are confined to, or `null` where the turn does not confine them. */
  confinement?: { heading: string; directories: string[]; sentences: string[] } | null
  /**
   * Credential scopes and toolchain lists the planner asked this line to be lent, by name.
   *
   * Empty for nearly every command.
   */
  requestedScopes?: string[]
  /**
   * Whether a standing answer exists for this line.
   *
   * False for a line that is asked about every time whatever is remembered: private input, an
   * assignment, a file to write, a reference as input, or a requested scope. A front end offers
   * "Trust" only when this is true and does not work the reasons out for itself.
   */
  canBeRemembered: boolean
  /**
   * Access the command reaches that nothing in the agent holds.
   *
   * A container daemon, a tool that is already logged in, the ssh agent, the metadata service
   * of this machine: nothing is handed over when one is used, nobody is asked at that moment,
   * and the agent cannot take the access back afterwards. So the one thing available is that
   * whoever approves the command is told what they are approving.
   *
   * `authority` is the kind, which is what to match on, and `named` is the word that named it:
   * a program, an address, a socket or a variable. The sentence is the front end's, because the
   * words a person reads belong to the surface drawing them. Empty for nearly every command.
   */
  ambient?: Ambient[]
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
  'trust.request': { directory: string; keeping?: string | null }
  'turn.started': { turn: number }
  'watch.fired': { number: number; path: string }
  'watch.ended': { number: number; reason: string; message?: string }
  phase: { phase: Phase }
  composing: { call: string | null }
  narration: { text: string }
  'tool.started': Activity
  'tool.finished': Activity
  'check.started': Checking
  'check.finished': Record<string, never>
  'hook.started': Hook
  'hook.finished': Record<string, never>
  landed: { landing: Landing }
  quarantined: Shown
  todos: { rows: TodoRow[] }
  tokens: { written: number }
  /** Numbered by `turn`, or by `run` for a manifest run, which is not one of the turns. */
  audit: { turn?: number; run?: number; event: Record<string, unknown> }
  'confirm.request': ConfirmRequest
  'run.request': RunRequest
  'output.request': OutputRequest
  'vouch.request': VouchRequest
  'vet.request': VetRequest
  'fetch.request': FetchRequest
  'server.request': ServerRequest
  'manifest.request': ManifestRequest
  'exposure.request': ExposureRequest
  'mcp-server.request': McpServerRequest
  'mcp-tools.request': McpToolsRequest
  'mcp-call.request': McpCallRequest
  'mcp-move.request': McpMoveRequest
  /** Sent as a session's first turn starts the MCP servers its project requests. */
  'mcp.starting': { servers: string[] }
  'mcp.started': McpStarted
  'manifest.started': { run: number }
  'manifest.done': ManifestDone
  'manifest.error': ManifestError
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
