/**
 * The words Chat › Token Spend… and Chat › Context Breakdown… show, made from what
 * `session.report.cost` and `session.report.context` answer.
 *
 * The desktop form of `/cost` (CMD-15) and `/context` (CMD-19). The bridge works out the
 * figures, the same way the terminal does, and this module only words them. The wording is the
 * English the terminal's catalog (`crates/i18n/locales/en-US.ftl`) gives the same rows. Pure, so a
 * test can call it without a window.
 */

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

export type ContextSection = 'system' | 'instructions' | 'skills' | 'tools' | 'typed' | 'planner' | 'results' | 'other'

export interface ReportNotice {
  title: string
  subtitle?: string
  body: string
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
