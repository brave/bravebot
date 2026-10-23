import type { JsonValue } from '@brave/agent-client'

// Readers for the bridge payloads the screens draw (ui/docs/phase-0-rpc-protocol.md, section 8).
// A reader returns null when a required field is missing or has the wrong type. An approval that
// reads as null is shown as unanswerable, never as a partial card: every field a person decides on
// is drawn or the question is not offered.

type Obj = { [key: string]: JsonValue }

function obj(value: JsonValue | undefined): Obj | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value : null
}

function text(source: Obj, key: string): string | null {
  const value = source[key]
  return typeof value === 'string' ? value : null
}

/** A string that may be absent or null, which reads as null. Any other type makes the payload unreadable: undefined. */
function optionalText(source: Obj, key: string): string | null | undefined {
  const value = source[key]
  if (value === undefined || value === null) return null
  return typeof value === 'string' ? value : undefined
}

function count(source: Obj, key: string): number | null {
  const value = source[key]
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null
}

function flag(source: Obj, key: string): boolean | null {
  const value = source[key]
  return typeof value === 'boolean' ? value : null
}

function texts(source: Obj, key: string): string[] | null {
  const value = source[key]
  if (!Array.isArray(value)) return null
  return value.every((item) => typeof item === 'string') ? (value as string[]) : null
}

function list<T>(source: Obj, key: string, read: (item: JsonValue) => T | null): T[] | null {
  const value = source[key]
  if (!Array.isArray(value)) return null
  const items: T[] = []
  for (const item of value) {
    const got = read(item)
    if (got === null) return null
    items.push(got)
  }
  return items
}

export type Change =
  | { kind: 'kept' | 'added' | 'removed'; text: string }
  | { kind: 'elided'; lines: number }

function readChange(value: JsonValue): Change | null {
  const source = obj(value)
  if (source === null) return null
  const kind = text(source, 'kind')
  if (kind === 'elided') {
    const lines = count(source, 'lines')
    return lines === null ? null : { kind, lines }
  }
  const body = text(source, 'text')
  return (kind === 'kept' || kind === 'added' || kind === 'removed') && body !== null ? { kind, text: body } : null
}

export interface ConfirmView {
  request: number
  path: string
  /** `path` spelled for a person (PROMPT-1). Drawn in its place, so a name that would read as another is marked. */
  shownPath: string
  intent: 'create' | 'overwrite' | 'edit'
  untrusted: boolean
  existing: boolean
  added: number
  removed: number
  exact: boolean
  changes: Change[]
  remark: { preview: string[]; lines: number; label: string } | null
  credentials: string[]
  writtenSinceCheckout: boolean
  lineEndings: string | null
}

export function readConfirm(data: JsonValue): ConfirmView | null {
  const source = obj(data)
  if (source === null) return null
  const request = count(source, 'request')
  const path = text(source, 'path')
  const shownPath = text(source, 'shownPath')
  const intent = text(source, 'intent')
  const untrusted = flag(source, 'untrusted')
  const existing = flag(source, 'existing')
  const added = count(source, 'added')
  const removed = count(source, 'removed')
  const exact = flag(source, 'exact')
  const changes = list(source, 'changes', readChange)
  if (request === null || path === null || shownPath === null || untrusted === null || existing === null) return null
  if (added === null || removed === null || exact === null || changes === null) return null
  if (intent !== 'create' && intent !== 'overwrite' && intent !== 'edit') return null
  let remark: ConfirmView['remark'] = null
  if (source.remark !== undefined && source.remark !== null) {
    const found = obj(source.remark)
    const preview = found && texts(found, 'preview')
    const lines = found && count(found, 'lines')
    const label = found && text(found, 'label')
    if (!found || preview === null || lines === null || label === null) return null
    remark = { preview, lines, label }
  }
  const credentials = texts(source, 'credentials')
  const writtenSinceCheckout = flag(source, 'writtenSinceCheckout')
  if (credentials === null || writtenSinceCheckout === null) return null
  const lineEndings = optionalText(source, 'lineEndings')
  if (lineEndings === undefined) return null
  return { request, path, shownPath, intent, untrusted, existing, added, removed, exact, changes, remark, credentials, writtenSinceCheckout, lineEndings }
}

export interface Stage {
  program: string
  resolved: string | null
  startedAs: string | null
  binary: string | null
  /** The file the step runs, and the path that starts it where that is another, spelled for a person. Drawn in place of `resolved`, `startedAs` and `binary`. */
  shownBinary: string
  args: string[]
  display: string
}

function readStage(value: JsonValue): Stage | null {
  const source = obj(value)
  if (source === null) return null
  const program = text(source, 'program')
  const args = texts(source, 'args')
  const display = text(source, 'display')
  const shownBinary = text(source, 'shownBinary')
  if (program === null || args === null || display === null || shownBinary === null) return null
  const resolved = optionalText(source, 'resolved')
  const startedAs = optionalText(source, 'startedAs')
  const binary = optionalText(source, 'binary')
  if (resolved === undefined || startedAs === undefined || binary === undefined) return null
  return { program, resolved, startedAs, binary, shownBinary, args, display }
}

export interface Ambient {
  authority: string
  named: string
}

function readAmbient(value: JsonValue): Ambient | null {
  const source = obj(value)
  if (source === null) return null
  const authority = text(source, 'authority')
  const named = text(source, 'named')
  return authority === null || named === null ? null : { authority, named }
}

export interface RunView {
  request: number
  line: string
  plan: string
  stages: Stage[]
  directory: string
  /** `directory` spelled for a person (PROMPT-1). */
  shownDirectory: string
  writes: string[]
  /** `writes` spelled for a person, in the same order. */
  shownWrites: string[]
  stdin: string | null
  releasesPrivate: boolean
  /** `directories` is the paths as held and `shownDirectories` the same paths spelled for a person. */
  confinement: { heading: string; directories: string[]; shownDirectories: string[]; sentences: string[] } | null
  /** The bridge's sentence for a command nothing confines. Present exactly when `confinement` is null. */
  notSandboxed: string | null
  requestedScopes: string[]
  ambient: Ambient[]
  summary: string
}

export function readRun(data: JsonValue): RunView | null {
  const source = obj(data)
  if (source === null) return null
  const request = count(source, 'request')
  const directory = text(source, 'directory')
  const shownDirectory = text(source, 'shownDirectory')
  const releasesPrivate = flag(source, 'releasesPrivate')
  const stages = list(source, 'stages', readStage)
  if (request === null || directory === null || shownDirectory === null || releasesPrivate === null || stages === null) return null
  const writes = texts(source, 'writes')
  const shownWrites = texts(source, 'shownWrites')
  const requestedScopes = texts(source, 'requestedScopes')
  const ambient = list(source, 'ambient', readAmbient)
  if (writes === null || shownWrites === null || requestedScopes === null || ambient === null) return null
  const stdin = optionalText(source, 'stdin')
  if (stdin === undefined) return null
  let confinement: RunView['confinement'] = null
  if (source.confinement !== undefined && source.confinement !== null) {
    const found = obj(source.confinement)
    const heading = found && text(found, 'heading')
    const directories = found && texts(found, 'directories')
    const shownDirectories = found && texts(found, 'shownDirectories')
    const sentences = found && texts(found, 'sentences')
    if (!found || heading === null || directories === null || shownDirectories === null || sentences === null) return null
    confinement = { heading, directories, shownDirectories, sentences }
  }
  const notSandboxed = optionalText(source, 'notSandboxed')
  if (notSandboxed === undefined || (confinement === null && notSandboxed === null)) return null
  const line = text(source, 'line')
  const plan = text(source, 'plan')
  const summary = source.summary === undefined ? '' : text(source, 'summary')
  if (line === null || plan === null || summary === null) return null
  return { request, line, plan, stages, directory, shownDirectory, writes, shownWrites, stdin, releasesPrivate, confinement, notSandboxed, requestedScopes, ambient, summary }
}

export interface FetchView {
  request: number
  url: string
  host: string
  ambient: Ambient[]
  summary: string
}

export function readFetch(data: JsonValue): FetchView | null {
  const source = obj(data)
  if (source === null) return null
  const request = count(source, 'request')
  const url = text(source, 'url')
  const host = text(source, 'host')
  const ambient = list(source, 'ambient', readAmbient)
  const summary = source.summary === undefined ? '' : text(source, 'summary')
  if (request === null || url === null || host === null || ambient === null || summary === null) return null
  return { request, url, host, ambient, summary }
}

export interface AskRow {
  index: number
  label: string
  detail: string | null
}

export interface AskPrompt {
  header: string
  question: string
  rows: AskRow[]
  multiple: boolean
  key: string
}

function readAskRow(value: JsonValue): AskRow | null {
  const source = obj(value)
  if (source === null) return null
  const index = count(source, 'index')
  const label = text(source, 'label')
  const detail = optionalText(source, 'detail')
  if (index === null || label === null || detail === undefined) return null
  return { index, label, detail }
}

function readAskPrompt(value: JsonValue): AskPrompt | null {
  const source = obj(value)
  if (source === null) return null
  const header = text(source, 'header')
  const question = text(source, 'question')
  const rows = list(source, 'rows', readAskRow)
  const multiple = flag(source, 'multiple')
  const key = text(source, 'key')
  if (header === null || question === null || rows === null || multiple === null || key === null) return null
  return { header, question, rows, multiple, key }
}

export interface AskView {
  request: number
  prompts: AskPrompt[]
}

export function readAsk(data: JsonValue): AskView | null {
  const source = obj(data)
  if (source === null) return null
  const request = count(source, 'request')
  const prompts = list(source, 'prompts', readAskPrompt)
  return request === null || prompts === null ? null : { request, prompts }
}

export interface TrustView {
  directory: string
  /** `directory` spelled for a person (PROMPT-1). Drawn in its place. */
  shownDirectory: string
  keeping: string | null
  /** `keeping` spelled for a person. Null exactly when `keeping` is. */
  shownKeeping: string | null
}

export function readTrust(data: JsonValue): TrustView | null {
  const source = obj(data)
  if (source === null) return null
  const directory = text(source, 'directory')
  const shownDirectory = text(source, 'shownDirectory')
  const keeping = optionalText(source, 'keeping')
  const shownKeeping = optionalText(source, 'shownKeeping')
  if (directory === null || shownDirectory === null || keeping === undefined || shownKeeping === undefined) return null
  if ((keeping === null) !== (shownKeeping === null)) return null
  return { directory, shownDirectory, keeping, shownKeeping }
}

export interface ShownView {
  origin: string
  reach: 'not_the_planner' | 'no_model'
  label: string
  preview: string[]
  lines: number
}

export function readShown(data: JsonValue): ShownView | null {
  const source = obj(data)
  if (source === null) return null
  const origin = text(source, 'origin')
  const reach = text(source, 'reach')
  const label = text(source, 'label')
  const preview = texts(source, 'preview')
  const lines = count(source, 'lines')
  if (origin === null || label === null || preview === null || lines === null) return null
  if (reach !== 'not_the_planner' && reach !== 'no_model') return null
  return { origin, reach, label, preview, lines }
}

export interface ActivityView {
  verb: string
  target: string
  /** `target` spelled so a file's name that is blank or ends in whitespace is visible (PROMPT-1). Draw this and not `target`. */
  shownTarget: string
  why: string
  note: string | null
  failed: boolean
  untrusted: boolean
}

export function readActivity(data: JsonValue): ActivityView | null {
  const source = obj(data)
  if (source === null) return null
  const verb = text(source, 'verb')
  const target = text(source, 'target')
  // A bridge that sends no spelling sends a target that needs none, so the target is drawn as it is.
  const shownTarget = source.shownTarget === undefined ? target : text(source, 'shownTarget')
  const why = source.why === undefined ? '' : text(source, 'why')
  const note = optionalText(source, 'note')
  const failed = flag(source, 'failed')
  const untrusted = flag(source, 'untrusted')
  if (verb === null || target === null || shownTarget === null || why === null || note === undefined || failed === null || untrusted === null) return null
  return { verb, target, shownTarget, why, note, failed, untrusted }
}

export function readPromptText(data: JsonValue): string | null {
  const source = obj(data)
  return source === null ? null : text(source, 'text')
}

export function readNarration(data: JsonValue): string | null {
  const source = obj(data)
  return source === null ? null : text(source, 'text')
}

export function readReply(data: JsonValue): string | null {
  const source = obj(data)
  return source === null ? null : text(source, 'reply')
}

export interface FailureView {
  kind: string
  message: string
}

export function readFailure(data: JsonValue): FailureView | null {
  const source = obj(data)
  if (source === null) return null
  const kind = text(source, 'kind')
  const message = text(source, 'message')
  return kind === null || message === null ? null : { kind, message }
}
