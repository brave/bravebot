import { useCallback, useEffect, useState } from 'react'
import type { TurnDetails as Details, TurnDisclosure } from '../turn-details'
import type { RecalledRow } from '../recalled'
import { blocksFor, readExcerpts, type ExcerptBlock, type ExcerptsKey } from '../excerpts'
import { Alert, Button, Collapse, ProgressRing } from '../nala'
import { t } from '../i18n'
import { CopyButton } from './CopyButton'
import { IconButton } from './IconButton'

export type OpenAudit = (turn: number | null, trigger: HTMLButtonElement) => void

export function TurnNotices({ details, onDisclosure }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
}): React.JSX.Element | null {
  if (!details?.notices.length) return null
  return <Collapse className="turn-notices" isOpen={details.noticesOpen}
    title={`Turn notices · ${details.notices.length}`}
    onToggle={({ open }) => { if (open !== details.noticesOpen) onDisclosure(details.turn, 'noticesOpen', open) }}
    data-test="turn-notices">
    <ul>{details.notices.map((notice, index) => <li key={index}>{notice}</li>)}</ul>
  </Collapse>
}

const whoSaid = (row: RecalledRow): string => {
  switch (row.who) {
    case 'you': return t.uiRecalledYou()
    case 'reply': return t.uiRecalledReply()
    case 'tool': return t.uiRecalledTool()
    case 'exchange': return t.uiRecalledExchange()
    case 'missing': return t.uiRecalledMissing()
  }
}

/** Who a block names as its speaker, in the reader's words. The record's own word goes in a tooltip. */
const speaker = (from: string): string => {
  switch (from) {
    case 'user': return t.uiRecalledYou()
    case 'you': return t.uiRecalledReply()
    case 'tool': return t.uiRecalledTool()
    case 'file': return t.uiRecalledFromFile()
    case 'skill': return t.uiRecalledFromSkill()
    case 'session': return t.uiRecalledFromSession()
    default: return t.uiRecalledFromOther()
  }
}

/**
 * "Used N earlier messages" under a reply whose turn recalled them (RETRIEVE-27). Collapsed at
 * first. Each row is drawn from this window's transcript and opens to the words its turn sent
 * (RETRIEVE-32); the button beside it scrolls to the row.
 *
 * A turn this window ran keeps its disclosure with the turn's details; one read back from the
 * record has none, so it is kept here.
 */
export function TurnRecalled({ count, rows, details, asked, onDisclosure }: {
  count: number
  rows: RecalledRow[]
  details?: Details
  /** Where the exchange's sends are asked for, or `undefined` while nothing can say (no saved record or prompt number yet). */
  asked?: ExcerptsKey
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
}): React.JSX.Element {
  const [kept, setKept] = useState(false)
  const open = details ? details.recalledOpen : kept
  // The record is written when a turn ends, so until then there are no sends to read.
  const running = details?.status === 'running'
  return <Collapse className="turn-notices turn-recalled" isOpen={open}
    title={t.uiRecalledTitle({ count })}
    onToggle={({ open: next }) => {
      if (next === open) return
      if (details) onDisclosure(details.turn, 'recalledOpen', next)
      else setKept(next)
    }}
    data-test="turn-recalled">
    <ul>{rows.map((row, index) => <li key={row.ordinal !== undefined ? `n${row.ordinal}` : row.id ?? `missing-${index}`} className="turn-recalled-item">
      <RecalledExcerpt row={row} asked={running ? undefined : asked} running={running} />
      {row.id !== null
        ? <IconButton icon="arrow-diagonal-up-right" label={t.uiRecalledReveal()} size="tiny"
          className="turn-recalled-reveal" data-test="turn-recalled-reveal"
          onClick={() => document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: row.id }))} />
        // Nothing to go to, but the room is kept so every row's chevron lines up.
        : <span className="turn-recalled-reveal" aria-hidden="true" />}
    </li>)}</ul>
  </Collapse>
}

type Reading = { state: 'reading' } | { state: 'failed' } | { state: 'read'; blocks: ExcerptBlock[]; contextBytes: number }

/** One recalled row, which reads its exchange's sends the first time it is opened. */
function RecalledExcerpt({ row, asked, running }: { row: RecalledRow; asked?: ExcerptsKey; running: boolean }): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [reading, setReading] = useState<Reading>({ state: 'reading' })
  const [attempt, setAttempt] = useState(0)
  const { directory, id, prompt } = asked ?? {}
  const ordinal = row.ordinal
  useEffect(() => {
    if (!open || running || directory === undefined || id === undefined || prompt === undefined || ordinal === undefined) return
    let gone = false
    setReading({ state: 'reading' })
    readExcerpts({ directory, id, prompt }).then(
      (read) => { if (!gone) setReading({ state: 'read', blocks: blocksFor(read, ordinal), contextBytes: read.contextBytes }) },
      () => { if (!gone) setReading({ state: 'failed' }) })
    return () => { gone = true }
  }, [open, running, directory, id, prompt, ordinal, attempt])
  const retry = useCallback(() => setAttempt((at) => at + 1), [])
  // Nothing can be asked for before the record holds the exchange: say when it will be.
  const waiting = running || asked === undefined || ordinal === undefined
  return <Collapse className="turn-recalled-excerpt" isOpen={open}
    onToggle={({ open: next }) => { if (next !== open) setOpen(next) }}
    data-test="turn-recalled-row">
    <span slot="title" className="turn-recalled-row">
      <span className="turn-recalled-who">{whoSaid(row)}</span>
      {row.line && <span className="turn-recalled-line">{row.line}</span>}
    </span>
    {open && <div className="turn-recalled-body">
      {waiting ? <p className="turn-recalled-note" data-test="turn-recalled-running">{t.uiRecalledRunning()}</p>
        : reading.state === 'reading' ? <p className="turn-recalled-note" role="status" data-test="turn-recalled-reading"><ProgressRing mode="indeterminate" /> {t.uiRecalledReading()}</p>
        : reading.state === 'failed' ? <div className="turn-recalled-failed">
          <Alert type="error" size="small" role="alert" data-test="turn-recalled-failed">{t.uiRecalledUnread()}</Alert>
          <Button size="small" kind="outline" onClick={retry} data-test="turn-recalled-retry">{t.uiRecalledRetry()}</Button>
        </div>
        : reading.blocks.length === 0 ? <p className="turn-recalled-note" data-test="turn-recalled-nothing">{t.uiRecalledNothing()}</p>
        : reading.blocks.map((block, at) => <SentBlock key={at} block={block} contextBytes={reading.contextBytes} />)}
    </div>}
  </Collapse>
}

/** The words one block carried, captioned with who said them, how they were trusted and which part they are. */
function SentBlock({ block, contextBytes }: { block: ExcerptBlock; contextBytes: number }): React.JSX.Element {
  const { send, excerpt } = block
  return <figure className="turn-recalled-block" data-test="turn-recalled-block">
    <figcaption className="turn-recalled-caption">
      <span className="turn-recalled-facts">
        <span title={excerpt.from}>{speaker(excerpt.from)}</span>
        {send.label && <span>{send.label}</span>}
        <span>{excerpt.context ? t.uiRecalledContext({ bytes: contextBytes }) : t.uiRecalledPart({ part: excerpt.chunk + 1 })}</span>
        {send.replacedInTurn && <span className="turn-recalled-replaced" data-test="turn-recalled-replaced">{t.uiRecalledReplaced()}</span>}
      </span>
      <CopyButton text={() => excerpt.text} label={t.uiRecalledCopy()} />
    </figcaption>
    <pre className="turn-recalled-text" tabIndex={0} data-test="turn-recalled-text">{excerpt.text}</pre>
  </figure>
}

const exact = (value?: number): string => value === undefined ? 'Unavailable' : value.toLocaleString()
const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

/**
 * The line under a reply: copy it, the way into its audit, and what it cost.
 *
 * Quiet until the row is hovered or is the last reply, except when a policy refused something in
 * the turn, which stays in view because it is the one thing here a reader has to know. The model
 * line follows the audit control. Its tooltip holds the exact counts.
 */
export function TurnFooter({ details, onAudit, copy, undo }: {
  details?: Details
  onAudit: OpenAudit
  /** The reply's own text, for the Copy button. Absent under an error, which has its own details. */
  copy?: string
  /** Offered on the latest turn's footer while the session can be put back to before it. */
  undo?: { running: boolean; onUndo: () => void }
}): React.JSX.Element {
  const blocked = details?.clean === false
  const label = blocked ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'
  return <div className={`turn-footer${blocked ? ' has-refusal' : ''}`}>
    {copy !== undefined && <CopyButton text={() => copy} label="Copy message" data-test="copy-message" />}
    {undo && <IconButton icon="arrow-undo" label="Undo turn" size="tiny" disabled={undo.running}
      tooltip={undo.running ? 'Wait for the turn to finish' : 'Undo turn · Put back its files and conversation'}
      data-test="turn-undo" onClick={undo.onUndo} />}
    {blocked && <span className="turn-refusal" aria-hidden="true">Policy blocked an action</span>}
    <IconButton icon={blocked ? 'shield-alert' : 'shield-done'} label={label}
      tooltip={blocked ? 'Policy blocked an action · Open the audit' : details ? 'Audit this turn' : 'Audit unavailable'}
      size="tiny"
      className={`turn-audit-link${blocked ? ' has-refusal' : ''}`}
      dataset={{ 'audit-turn': details?.turn ?? 'saved' }}
      data-test="turn-audit"
      controls="turn-audit-inspector"
      onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget as HTMLButtonElement)} />
    {details?.status === 'complete' ? <Usage details={details} /> : details ? <span className="turn-usage-missing">Final usage unavailable</span> : null}
  </div>
}

/** The model and a compact total. A click opens the exact counts; a screen reader can read them without one. */
function Usage({ details }: { details: Details }): React.JSX.Element {
  const shown = `${details.model ?? 'Model unavailable'} · ${details.tokens === undefined ? 'Usage unavailable' : `${compact.format(details.tokens)} tokens`}`
  const tooltip = [
    `Turn ${details.turn}`,
    `Model used · ${details.model ?? 'Unavailable'}`,
    ...(details.service ? [`Answered by · ${details.service}`] : []),
    `Total tokens · ${exact(details.tokens)}`,
    `Output tokens · ${exact(details.outputTokens)}`,
    `Tool-calling rounds · ${exact(details.steps)}`,
    'Usage is summed across requests in this turn',
  ].join('\n')
  return <button type="button" className="turn-usage" data-tooltip={tooltip} data-tooltip-trigger="click" data-test="turn-usage">
    <span className="turn-usage-line">{shown}</span>
    <span className="visually-hidden">{tooltip}</span>
  </button>
}
