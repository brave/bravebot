/**
 * Plugins: the payloads of `plugins.list`, `plugins.question` and `plugins.ended` (docs/specs/plugins.md).
 *
 * The bridge runs every check and puts every question; these only read what it sent. A payload of
 * the wrong shape parses to `null`, so the page draws no question it cannot draw whole and never
 * offers a yes to a listing it could not read (PLUGIN-4).
 *
 * Every string under an `untrusted` key is text the plugin wrote, and is drawn inside the margin
 * that marks untrusted content. `shadowedBy` names another source's origin and is drawn there too.
 */

/** One installed plugin, as its record says. Every field but `enabled` is text a plugin or a person wrote. */
export interface PluginRow {
  name: string
  version: string
  source: string
  /** The ref named at install, for a git source. */
  requestedRef: string | null
  /** The full commit installed, for a git source. */
  commit: string | null
  digest: string
  enabled: boolean
}

export interface PluginList {
  state: string
  writable: boolean
  plugins: PluginRow[]
}

/** Something a plugin's hook or server would reach: a program, a host, a file or a directory. */
export interface Reach {
  kind: 'command' | 'host' | 'file' | 'directory'
  untrusted: string
}

export interface SkillOffer {
  untrusted: { name: string; description: string }
  /** The origin of the skill that takes the name instead, where one does. */
  shadowedBy: string | null
}

export interface DefinitionOffer {
  kind: string
  untrusted: { name: string }
  shadowedBy: string | null
}

export interface HookRow {
  timeoutSecs: number | null
  /** `argv` is every word as it will run, `${PLUGIN_ROOT}` already expanded (PLUGIN-13). Never cut. */
  untrusted: { moment: string; tool: string | null; watches: string | null; argv: string[] }
  reaches: Reach[]
}

export interface PassedOver {
  entries: number
  keys: number
}

/** A hooks file as read: its rows, or the fault that kept it from being read. */
export interface HookPart {
  rows: HookRow[]
  fault: string | null
  passedOver: PassedOver
}

export interface ServerRow {
  usable: boolean
  reachable: boolean
  /** `<plugin>_<alias>` is longer than an alias may be. */
  tooLong: boolean
  neverStarted: boolean
  untrusted: {
    alias: string
    transport: string
    argv: string[]
    url: string | null
    variables: string[]
    stored: string[]
  }
  reaches: Reach[]
}

export interface ServerPart {
  rows: ServerRow[]
  fault: string | null
}

/** What the install question lists (PLUGIN-4). */
export interface InstallListing {
  git: boolean
  untrusted: {
    name: string
    version: string
    source: string
    requestedRef: string | null
    commit: string | null
    skipped: string[]
  }
  skills: SkillOffer[]
  definitions: DefinitionOffer[]
  /** `null` where the manifest names no hooks file. */
  hooks: HookPart | null
  servers: ServerPart | null
  /** The shadow marks were checked against the home directory's own skills and definitions only. */
  homeSourcesOnly: boolean
}

/** One part added, removed or changed between the installed copy and the new one. */
export type Delta<T> =
  | { change: 'added'; now: T }
  | { change: 'removed'; was: T }
  | { change: 'changed'; was: T; now: T }

export interface PartDeltas<T> {
  deltas: Delta<T>[]
  installedFault: string | null
  newFault: string | null
  passedOver: PassedOver | null
}

export interface FileChange {
  change: 'added' | 'removed' | 'changed'
  /** The path, decoded for display. */
  untrusted: string
}

/** What the update question lists (PLUGIN-9). */
export interface UpdateListing {
  git: boolean
  enabled: boolean
  installedDigest: string
  newDigest: string
  untrusted: {
    name: string
    installedVersion: string
    newVersion: string
    source: string
    installedSource: string
    requestedRef: string | null
    installedCommit: string | null
    newCommit: string | null
    skipped: string[]
  }
  /** What the installed copy held. `readable` is false where it could not be read. */
  before: {
    readable: boolean
    skills: SkillOffer[]
    definitions: DefinitionOffer[]
    hooks: HookPart | null
    servers: ServerPart | null
  }
  skills: Delta<SkillOffer>[]
  definitions: Delta<DefinitionOffer>[]
  hooks: PartDeltas<HookRow> | null
  servers: PartDeltas<ServerRow> | null
  /** Every file that differs, or `null` where the installed copy could not be compared. */
  files: FileChange[] | null
  homeSourcesOnly: boolean
}

/** The host question a git source puts before any `git` starts (PLUGIN-3). */
export interface HostListing {
  host: string
  url: string
}

/** One question a job puts. A reply names `job` and `question`, and only a yes to that number counts. */
export type PluginQuestion = { job: string; question: number } & (
  | { kind: 'host'; listing: HostListing }
  | { kind: 'install'; listing: InstallListing }
  | { kind: 'update'; listing: UpdateListing }
)

export type PluginOutcome = 'installed' | 'updated' | 'unchanged' | 'declined' | 'nobody' | 'refused'

/** How a job ended, the sentence to show, and what is installed afterwards. */
export interface PluginsEnded {
  job: string
  outcome: PluginOutcome
  sentence: string
  plugins: PluginRow[]
}

type Parse<T> = (value: unknown) => T | null

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)
const isString = (value: unknown): value is string => typeof value === 'string'
const isCount = (value: unknown): value is number => Number.isInteger(value) && (value as number) >= 0
const isNullableString = (value: unknown): value is string | null => value === null || isString(value)
const isStrings = (value: unknown): value is string[] => Array.isArray(value) && value.every(isString)

/** Every element of `value` parsed, or `null` where it is not a list or one element is malformed. */
function list<T>(value: unknown, parse: Parse<T>): T[] | null {
  if (!Array.isArray(value)) return null
  const parsed = value.map(parse)
  return parsed.every((each): each is T => each !== null) ? parsed : null
}

/** `null` stays `null`; anything else must parse. `undefined` is a malformed payload. */
function nullable<T>(value: unknown, parse: Parse<T>): T | null | undefined {
  if (value === null) return null
  const parsed = parse(value)
  return parsed === null ? undefined : parsed
}

const pluginRow: Parse<PluginRow> = (value) => {
  if (!isObject(value)) return null
  const { name, version, source, requestedRef, commit, digest, enabled } = value
  if (!isString(name) || !isString(version) || !isString(source) || !isString(digest)) return null
  if (!isNullableString(requestedRef) || !isNullableString(commit) || typeof enabled !== 'boolean') return null
  return { name, version, source, requestedRef, commit, digest, enabled }
}

/** The answer to `plugins.list`, `plugins.enable`, `plugins.disable` and `plugins.remove`. */
export function parsePluginList(value: unknown): PluginList | null {
  if (!isObject(value) || !isString(value.state) || typeof value.writable !== 'boolean') return null
  const plugins = list(value.plugins, pluginRow)
  return plugins ? { state: value.state, writable: value.writable, plugins } : null
}

/** The job number `plugins.add` and `plugins.update` answer with. */
export function parseJob(value: unknown): string | null {
  return isObject(value) && isString(value.job) && value.job !== '' ? value.job : null
}

const REACHES = new Set(['command', 'host', 'file', 'directory'])
const reach: Parse<Reach> = (value) =>
  isObject(value) && isString(value.kind) && REACHES.has(value.kind) && isString(value.untrusted)
    ? { kind: value.kind as Reach['kind'], untrusted: value.untrusted }
    : null

const skillOffer: Parse<SkillOffer> = (value) => {
  if (!isObject(value) || !isObject(value.untrusted) || !isNullableString(value.shadowedBy)) return null
  const { name, description } = value.untrusted
  return isString(name) && isString(description) ? { untrusted: { name, description }, shadowedBy: value.shadowedBy } : null
}

const definitionOffer: Parse<DefinitionOffer> = (value) => {
  if (!isObject(value) || !isString(value.kind) || !isObject(value.untrusted) || !isNullableString(value.shadowedBy)) return null
  const { name } = value.untrusted
  return isString(name) ? { kind: value.kind, untrusted: { name }, shadowedBy: value.shadowedBy } : null
}

const passedOver: Parse<PassedOver> = (value) =>
  isObject(value) && isCount(value.entries) && isCount(value.keys) ? { entries: value.entries, keys: value.keys } : null

const hookRow: Parse<HookRow> = (value) => {
  if (!isObject(value) || !isObject(value.untrusted)) return null
  const { timeoutSecs } = value
  if (timeoutSecs !== null && !isCount(timeoutSecs)) return null
  const { moment, tool, watches, argv } = value.untrusted
  if (!isString(moment) || !isNullableString(tool) || !isNullableString(watches) || !isStrings(argv)) return null
  const reaches = list(value.reaches, reach)
  return reaches ? { timeoutSecs, untrusted: { moment, tool, watches, argv }, reaches } : null
}

const hookPart: Parse<HookPart> = (value) => {
  if (!isObject(value) || !isNullableString(value.fault)) return null
  const rows = list(value.rows, hookRow)
  const over = passedOver(value.passedOver)
  return rows && over ? { rows, fault: value.fault, passedOver: over } : null
}

const serverRow: Parse<ServerRow> = (value) => {
  if (!isObject(value) || !isObject(value.untrusted)) return null
  const { usable, reachable, tooLong, neverStarted } = value
  if (typeof usable !== 'boolean' || typeof reachable !== 'boolean' || typeof tooLong !== 'boolean' || typeof neverStarted !== 'boolean') return null
  const { alias, transport, argv, url, variables, stored } = value.untrusted
  if (!isString(alias) || !isString(transport) || !isStrings(argv) || !isNullableString(url) || !isStrings(variables) || !isStrings(stored)) return null
  const reaches = list(value.reaches, reach)
  return reaches
    ? { usable, reachable, tooLong, neverStarted, untrusted: { alias, transport, argv, url, variables, stored }, reaches }
    : null
}

const serverPart: Parse<ServerPart> = (value) => {
  if (!isObject(value) || !isNullableString(value.fault)) return null
  const rows = list(value.rows, serverRow)
  return rows ? { rows, fault: value.fault } : null
}

function delta<T>(parse: Parse<T>): Parse<Delta<T>> {
  return (value) => {
    if (!isObject(value)) return null
    if (value.change === 'added') {
      const now = parse(value.now)
      return now ? { change: 'added', now } : null
    }
    if (value.change === 'removed') {
      const was = parse(value.was)
      return was ? { change: 'removed', was } : null
    }
    if (value.change === 'changed') {
      const was = parse(value.was)
      const now = parse(value.now)
      return was && now ? { change: 'changed', was, now } : null
    }
    return null
  }
}

function partDeltas<T>(parse: Parse<T>): Parse<PartDeltas<T>> {
  return (value) => {
    if (!isObject(value) || !isNullableString(value.installedFault) || !isNullableString(value.newFault)) return null
    const deltas = list(value.deltas, delta(parse))
    // The new side's count of what was passed over; a servers file and an absent hooks file have none.
    const over = value.passedOver === undefined ? null : nullable(value.passedOver, passedOver)
    if (!deltas || over === undefined) return null
    return { deltas, installedFault: value.installedFault, newFault: value.newFault, passedOver: over }
  }
}

const CHANGES = new Set(['added', 'removed', 'changed'])
const fileChange: Parse<FileChange> = (value) =>
  isObject(value) && isString(value.change) && CHANGES.has(value.change) && isString(value.untrusted)
    ? { change: value.change as FileChange['change'], untrusted: value.untrusted }
    : null

/** `homeSourcesOnly` is added by the bridge after the projection; absent means not said. */
const homeSourcesOnly = (value: Record<string, unknown>): boolean | null =>
  value.homeSourcesOnly === undefined ? false : typeof value.homeSourcesOnly === 'boolean' ? value.homeSourcesOnly : null

export function parseInstallListing(value: unknown): InstallListing | null {
  if (!isObject(value) || typeof value.git !== 'boolean' || !isObject(value.untrusted)) return null
  const { name, version, source, requestedRef, commit, skipped } = value.untrusted
  if (!isString(name) || !isString(version) || !isString(source)) return null
  if (!isNullableString(requestedRef) || !isNullableString(commit) || !isStrings(skipped)) return null
  const skills = list(value.skills, skillOffer)
  const definitions = list(value.definitions, definitionOffer)
  const hooks = nullable(value.hooks, hookPart)
  const servers = nullable(value.servers, serverPart)
  const local = homeSourcesOnly(value)
  if (!skills || !definitions || hooks === undefined || servers === undefined || local === null) return null
  return {
    git: value.git,
    untrusted: { name, version, source, requestedRef, commit, skipped },
    skills, definitions, hooks, servers,
    homeSourcesOnly: local,
  }
}

export function parseUpdateListing(value: unknown): UpdateListing | null {
  if (!isObject(value) || typeof value.git !== 'boolean' || typeof value.enabled !== 'boolean') return null
  if (!isString(value.installedDigest) || !isString(value.newDigest) || !isObject(value.untrusted) || !isObject(value.before)) return null
  const u = value.untrusted
  if (!isString(u.name) || !isString(u.installedVersion) || !isString(u.newVersion) || !isString(u.source) || !isString(u.installedSource)) return null
  if (!isNullableString(u.requestedRef) || !isNullableString(u.installedCommit) || !isNullableString(u.newCommit) || !isStrings(u.skipped)) return null
  const b = value.before
  if (typeof b.readable !== 'boolean') return null
  const beforeSkills = list(b.skills, skillOffer)
  const beforeDefinitions = list(b.definitions, definitionOffer)
  const beforeHooks = nullable(b.hooks, hookPart)
  const beforeServers = nullable(b.servers, serverPart)
  const skills = list(value.skills, delta(skillOffer))
  const definitions = list(value.definitions, delta(definitionOffer))
  const hooks = nullable(value.hooks, partDeltas(hookRow))
  const servers = nullable(value.servers, partDeltas(serverRow))
  const files = nullable(value.files, (each) => list(each, fileChange))
  const local = homeSourcesOnly(value)
  if (!beforeSkills || !beforeDefinitions || beforeHooks === undefined || beforeServers === undefined) return null
  if (!skills || !definitions || hooks === undefined || servers === undefined || files === undefined || local === null) return null
  return {
    git: value.git,
    enabled: value.enabled,
    installedDigest: value.installedDigest,
    newDigest: value.newDigest,
    untrusted: {
      name: u.name, installedVersion: u.installedVersion, newVersion: u.newVersion,
      source: u.source, installedSource: u.installedSource, requestedRef: u.requestedRef,
      installedCommit: u.installedCommit, newCommit: u.newCommit, skipped: u.skipped,
    },
    before: { readable: b.readable, skills: beforeSkills, definitions: beforeDefinitions, hooks: beforeHooks, servers: beforeServers },
    skills, definitions, hooks, servers, files,
    homeSourcesOnly: local,
  }
}

export function parseHostListing(value: unknown): HostListing | null {
  return isObject(value) && isString(value.host) && isString(value.url) ? { host: value.host, url: value.url } : null
}

/** The data of a `plugins.question` event, or `null` where any part of it is malformed. */
export function parsePluginQuestion(value: unknown): PluginQuestion | null {
  if (!isObject(value) || !isString(value.job) || !Number.isInteger(value.question) || (value.question as number) < 1) return null
  const { job } = value
  const question = value.question as number
  switch (value.kind) {
    case 'host': {
      const listing = parseHostListing(value.listing)
      return listing ? { job, question, kind: 'host', listing } : null
    }
    case 'install': {
      const listing = parseInstallListing(value.listing)
      return listing ? { job, question, kind: 'install', listing } : null
    }
    case 'update': {
      const listing = parseUpdateListing(value.listing)
      return listing ? { job, question, kind: 'update', listing } : null
    }
    default: return null
  }
}

const OUTCOMES = new Set<string>(['installed', 'updated', 'unchanged', 'declined', 'nobody', 'refused'])

/** The data of a `plugins.ended` event, or `null` where it is malformed. */
export function parsePluginsEnded(value: unknown): PluginsEnded | null {
  if (!isObject(value) || !isString(value.job) || !isString(value.outcome) || !OUTCOMES.has(value.outcome) || !isString(value.sentence)) return null
  const plugins = list(value.plugins, pluginRow)
  return plugins ? { job: value.job, outcome: value.outcome as PluginOutcome, sentence: value.sentence, plugins } : null
}
