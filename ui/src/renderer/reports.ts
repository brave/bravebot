/**
 * The words Chat › Session Status…, Token Spend… and Context Breakdown… show, made from what
 * `session.report.status`, `session.report.cost` and `session.report.context` answer.
 *
 * The desktop form of `/status` (CMD-8), `/cost` (CMD-15) and `/context` (CMD-19). The bridge works out the
 * figures, the same way the terminal does, and this module only words them. The wording is the
 * English the terminal's catalog (`crates/i18n/locales/en-US.ftl`) gives the same rows. Pure, so a
 * test can call it without a window.
 */

import type { LoopRow } from '../shared/loop'

/** What `session.report.cost` answers. */
export interface CostReport {
  total: number
  turns: number
  rows: { turn: number; tokens: number; share: number }[]
  unattributed: number
  running: number
}

/** What `session.report.context` answers. */
export interface ContextReport {
  state: 'measured' | 'not-measured' | 'compacted' | 'no-request'
  total: number
  budget: number
  sections: { section: ContextSection; tokens: number; share: number }[]
}

/**
 * What `session.report.request` answers (TRACE-9). Every string is already drawn by the bridge's
 * `printable_spelled`, so a control character arrives as its picture; the window shows it as text.
 */
export interface RequestReport {
  none: boolean
  instructionFiles: { path: string; bytes: number }[]
  spans: { role: string; label: string; text: string }[]
}

export type ContextSection ='system' | 'instructions' | 'skills' | 'tools' | 'typed' | 'planner' | 'results' | 'other'

export interface ReportNotice {
  title: string
  subtitle?: string
  /** The report as plain text, for copying and for a notice drawn as text. */
  body: string
  /** The same report as label and value pairs, where it is drawn as a two-column list. */
  rows?: NoticeRow[]
}

/**
 * One pair of a notice drawn as a list. An empty label continues the row above. `value` is kept on
 * one line where it fits; `rest`, where set, follows it and is cut to the line.
 */
export interface NoticeRow {
  label: string
  value: string
  rest?: string
}

const SECTION_NAMES: Record<ContextSection, string> = {
  system: 'System prompt',
  instructions: 'Instruction files',
  skills: 'Skills',
  tools: 'Tool definitions',
  typed: 'What you typed',
  planner: 'What the planner wrote',
  results: 'Tool results',
  other: 'Other messages',
}

/** A count of tokens as the terminal writes one: whole below a thousand, then thousands to a point. */
export function tokens(count: number): string {
  if (count < 1000) return count === 1 ? '1 token' : `${count} tokens`
  return `${(count / 1000).toFixed(1)}k tokens`
}

function turns(count: number): string {
  return count === 1 ? '1 turn' : `${count} turns`
}

/** A row of a report: a label, a value, and a share of the total where there is one. */
type Row = [label: string, value: string, share?: string]

/**
 * The rows as plain text, padded so the values line up and so do the shares. Only rows with a
 * share set the width of the value column, so a long first row does not push the shares out.
 */
function table(rows: Row[]): string {
  const labels = Math.max(0, ...rows.map(([label]) => label.length))
  const values = Math.max(0, ...rows.filter((row) => row[2] !== undefined).map(([, value]) => value.length))
  return rows
    .map(([label, value, share]) => `${label.padEnd(labels)}  ${share === undefined ? value : `${value.padEnd(values)}  ${share}`}`)
    .join('\n')
}

/** The notice Token Spend… opens. */
export function costNotice(report: CostReport): ReportNotice {
  const title = 'Token spend'
  if (report.total === 0 && report.rows.length === 0) {
    return { title, body: 'Nothing spent yet.' }
  }
  const rows: Row[] = [['This session', `${turns(report.turns)} · ${tokens(report.total)}`]]
  for (const row of report.rows) {
    const label = row.turn === 0 ? 'Before turn 1' : `Turn ${row.turn}`
    rows.push([label, tokens(row.tokens), report.total === 0 ? undefined : `${row.share}%`])
  }
  if (report.unattributed > 0) {
    rows.push(['Not recorded against any turn', tokens(report.unattributed)])
  }
  return {
    title,
    subtitle: report.running > 0 ? 'Includes what the running turn has spent so far.' : undefined,
    body: table(rows),
  }
}

/** The notice Context Breakdown… opens. Every state but a measured one is a sentence saying why there is no breakdown. */
export function contextNotice(report: ContextReport): ReportNotice {
  const title = 'Context breakdown'
  switch (report.state) {
    case 'not-measured':
      return { title, body: 'The context has not been measured yet, so there is no breakdown.' }
    case 'compacted':
      return {
        title,
        body:
          'The conversation was compacted after the last request was measured, so that breakdown no longer describes it. ' +
          'The next request measures it again.',
      }
    case 'no-request':
      return {
        title,
        body: 'The context has been measured, but this session has sent no request of its own to break down yet.',
      }
    case 'measured': {
      const percent = report.budget > 0 ? Math.floor((report.total * 100) / report.budget) : 0
      const rows: Row[] = [['Last request', `${tokens(report.total)} · context ${percent}%`]]
      for (const section of report.sections) {
        rows.push([SECTION_NAMES[section.section], tokens(section.tokens), `${section.share}%`])
      }
      return {
        title,
        subtitle: 'Approximate: shares of the bytes sent, scaled to the measured total.',
        body: table(rows),
      }
    }
  }
}

/** What `session.report.status` answers. */
export interface StatusReport {
  project: string
  folders: string[]
  model: string | null
  definition: string | null
  permissionMode: string
  sandboxMode: string
  sessionId: string | null
  title: string | null
  turns: number
  tokens: number
  /** The goal the chat works towards, absent where none is set (GOAL-13). */
  goal?: StatusGoal
  /** Set while a turn runs: it holds the trust map and the vouched programs, so `rules` is null (CMD-8). */
  rulesHeldByTurn: boolean
  rules: {
    paths: { path: string; shownPath: string; integrity: string }[]
    commands: { display: string; shownDirectory: string }[]
  } | null
}

/**
 * The goal as `session.report.status` states it: the bridge's counts, from when the goal was set,
 * with the running turn's tokens in the spend (GOAL-13).
 */
export interface StatusGoal {
  condition: string
  rounds: number
  left: number
  paused: boolean
  stalled: boolean
  elapsedSeconds: number
  spentTokens: number
}

/** Whole seconds as the terminal's `/status` says them: `42s`, or `3m 05s`. */
export function elapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`
}

/**
 * The goal's rows under its condition: how many rounds have gone and are left, then how long it
 * has been set and what the chat has spent since.
 */
function goalRows(goal: StatusGoal): Row[] {
  const rounds = goal.rounds === 0
    ? 'not judged yet'
    : `sent back ${goal.rounds} ${goal.rounds === 1 ? 'time' : 'times'}`
  const state = goal.paused ? ' · paused, /goal resume arms it' : goal.stalled ? ' · stopped: rounds made no tool call' : ''
  return [
    ['Goal', goal.condition],
    // The same count the Goal panel gives, with its unit.
    ['', `${rounds} · ${goal.left} of ${goal.rounds + goal.left} rounds left${state}`],
    ['', `set ${elapsed(goal.elapsedSeconds)} ago · ${tokens(goal.spentTokens)} spent`],
  ]
}

const PERMISSION_NAMES: Record<string, string> = {
  ask: 'Ask',
  acceptEdits: 'Accept edits',
  plan: 'Plan',
  auto: 'Auto',
  bypass: 'Bypass',
}

const SANDBOX_NAMES: Record<string, string> = {
  'workspace-write': 'Workspace write',
  'read-only': 'Read only',
  strict: 'Strict',
  off: 'Off',
}

/**
 * What a running turn's rules are shown as. The terminal's `status-held-by-the-turn` says the same
 * in one row; here "shown once it ends" is the subtitle, so the rows fit the dialog.
 */
const HELD_BY_THE_TURN = 'held by the running turn'

/**
 * The notice Session Status… opens, the desktop's `/status` (CMD-8). While a turn runs, the trust
 * map and the vouched programs are said to be held by it, and neither is stated.
 */
export function statusNotice(report: StatusReport, loops: readonly LoopRow[] = []): ReportNotice {
  const rows: Row[] = [['Session', report.title || 'untitled, nothing sent yet']]
  if (report.sessionId) rows.push(['Session id', report.sessionId])
  rows.push(['Directory', report.project])
  for (const folder of report.folders.filter((folder) => folder !== report.project)) {
    rows.push(['Also open', folder])
  }
  if (report.model) rows.push(['Model', report.model])
  if (report.definition) rows.push(['Agent', report.definition])
  rows.push(['Permissions', PERMISSION_NAMES[report.permissionMode] ?? report.permissionMode])
  rows.push(['Sandbox', SANDBOX_NAMES[report.sandboxMode] ?? report.sandboxMode])
  rows.push(['Spent', `${turns(report.turns)} · ${tokens(report.tokens)}`])
  if (report.goal) rows.push(...goalRows(report.goal))
  // One row per loop under its number, from `loops.list` (LOOP-13); none where the session has none.
  // The times come first and the line last, so a long line is what the row cuts.
  const lines = new Map<number, string>()
  for (const loop of loops) {
    lines.set(rows.length, loop.prompt)
    rows.push([`Loop ${loop.number}`, `${loop.once ? 'sent once' : loop.pace} · ${loop.when}`])
  }
  const shown = (subtitle?: string): ReportNotice => ({
    title: 'Session status',
    ...(subtitle ? { subtitle } : {}),
    body: table(rows.map(([label, value], at) => [label, lines.has(at) ? `${value} · ${lines.get(at)}` : value])),
    rows: rows.map(([label, value], at) => ({ label, value, ...(lines.has(at) ? { rest: lines.get(at)! } : {}) })),
  })
  if (report.rulesHeldByTurn || !report.rules) {
    rows.push(['Trust', HELD_BY_THE_TURN], ['Programs', HELD_BY_THE_TURN])
    return shown('The running turn holds the trust and the vouched programs. They are shown once it ends.')
  }
  const { paths, commands } = report.rules
  if (paths.length === 0) rows.push(['Trust', 'nothing vouched for'])
  for (const [index, rule] of paths.entries()) {
    const name = rule.path === '' ? 'Project root' : rule.shownPath
    rows.push([index === 0 ? 'Trust' : '', `${name} · ${rule.integrity}`])
  }
  if (commands.length === 0) rows.push(['Programs', 'every run is put to you'])
  for (const [index, command] of commands.entries()) {
    rows.push([index === 0 ? 'Trusted commands' : '', `${command.display} · in ${command.shownDirectory}`])
  }
  return shown()
}

/** The figures `session.stats` answers for its total and for each model row (CLI-37). */
export interface StatsSpent {
  sessions: number
  turns: number
  tokens: number
  read: number
  written: number
  /** Tokens of sessions that kept no split into read and written. */
  unsplit: number
  /** What the service read out of its cache and wrote into it, over the sessions that carry the figures. */
  cacheRead: number
  cacheWritten: number
  /** Cache read over the tokens those sessions read, in tenths of a percent; null where they read nothing. */
  hitRatePermille: number | null
  /** The same as the terminal prints it, `-` where it is null. The bridge words it. */
  hitRate: string
}

/** The fixed word a record keeps for the backend that answered (SESSION-11), or null for none. */
export type StatsService = 'aichat' | 'bedrock' | 'gateway' | 'chatgpt' | 'xai'

/**
 * What `session.stats` answers: what the recorded sessions spent, as `bravebot stats --models`
 * totals it (CLI-37). The bridge has already grouped and cleaned the model rows. A row is one model
 * and the service that answered it; `model` is null for the sessions whose record names no model,
 * and `service` for those whose record names no service. `serviceWord` is the word the terminal
 * shows for the service, and `withoutCache.note` the terminal's note on the sessions the hit rate
 * leaves out, null where there are none; the bridge words both.
 */
export interface StatsReport {
  directory: string | null
  days: number | null
  total: StatsSpent
  models: (StatsSpent & { model: string | null; service: StatsService | null; serviceWord: string })[]
  unreadable: number
  /** The sessions the hit rate leaves out: those on a service that states none, and those naming no service. */
  withoutCache: { sessions: number; silent: number; older: number; note: string | null }
}

/** A row of the spend table: a label, the service word (empty on Total), its figures, and whether it is the Total row. */
export interface StatsRow extends StatsSpent {
  label: string
  service: string
  total: boolean
}

function spent(figures: StatsSpent): StatsSpent {
  const { sessions, turns, tokens, read, written, unsplit, cacheRead, cacheWritten, hitRatePermille, hitRate } = figures
  return { sessions, turns, tokens, read, written, unsplit, cacheRead, cacheWritten, hitRatePermille, hitRate }
}

/**
 * The rows of the spend table in the order the terminal prints them: each model and service in the
 * order the bridge sent, with the service's word as the bridge sent it, a model of null labelled
 * `unknown` (`stats-model-unknown`), then `Total` (`stats-row-total`).
 */
export function statsRows(report: StatsReport): StatsRow[] {
  const rows: StatsRow[] = report.models.map((row) => ({
    label: row.model ?? 'unknown', service: row.serviceWord, total: false, ...spent(row),
  }))
  rows.push({ label: 'Total', service: '', total: true, ...spent(report.total) })
  return rows
}

/**
 * The sentence saying which sessions the totals cover, the terminal's `stats-scope-*` strings.
 * `days` is null for no limit and `directory` is null for every project.
 */
export function statsScope(days: number | null, directory: string | null): string {
  const within = days === null ? null : days === 1 ? 'in the last day' : `in the last ${days} days`
  if (directory === null) {
    return within === null ? 'Every recorded session, in every project' : `Sessions last written ${within}, in every project`
  }
  return within === null ? `Every recorded session in ${directory}` : `Sessions last written ${within}, in ${directory}`
}
