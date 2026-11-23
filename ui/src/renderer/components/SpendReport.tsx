import { useEffect, useState } from 'react'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, ControlItem, SegmentedControl, Toggle } from '../nala'
import { call } from '../request'
import { statsRows, statsScope, type StatsReport } from '../reports'
import { Modal } from './Modal'
import { Skeleton } from './Skeleton'

/** The "last n days" choices. `null` is no limit, as `bravebot stats` without `--days`. */
const DAYS: readonly (number | null)[] = [null, 1, 7, 30]

const COLUMNS = ['Sessions', 'Turns', 'Tokens', 'Read', 'Written', 'Unsplit'] as const

/**
 * Token Spend Across Chats…, the desktop form of `bravebot stats` (CLI-37): what the recorded
 * sessions spent, overall or in one project, optionally for the last n days, with a row for each
 * model when asked.
 *
 * Tokens only. No model listing carries a price, so nothing is shown in money (CLI-37 Known
 * costs). The bridge reads every record whole on each call, so only a new days choice asks it
 * again; the by-model toggle changes how the answer already held is drawn. The bridge has cleaned
 * every model name and the directory, and they are drawn as React text.
 */
export function SpendReport({ directory, onClose }: {
  /** The project to total, or null for every project. */
  directory: string | null
  onClose: () => void
}): React.JSX.Element {
  const [days, setDays] = useState<number | null>(null)
  const [byModel, setByModel] = useState(false)
  const [report, setReport] = useState<StatsReport | null>(null)
  const [problem, setProblem] = useState('')
  const showLoading = useDelayedFlag(report === null && !problem)

  useEffect(() => {
    let gone = false
    setReport(null)
    setProblem('')
    call<StatsReport>('session.stats', { ...(directory === null ? {} : { directory }), ...(days === null ? {} : { days }) })
      .then((answer) => { if (!gone) setReport(answer) })
      .catch((error) => { if (!gone) setProblem(error instanceof Error ? error.message : String(error)) })
    return () => { gone = true }
  }, [directory, days])

  const rows = report ? statsRows(byModel ? report : { ...report, models: [] }) : []
  const unreadable = report?.unreadable ?? 0

  return (
    <Modal title="Token spend across chats" subtitle={statsScope(days, report?.directory ?? directory)}
      size="lg" className="spend-report" onClose={onClose}
      actions={<Button kind="filled" onClick={onClose} data-test="spend-report-done">Done</Button>}>
      <div className="spend-report-controls">
        <SegmentedControl size="small" value={days === null ? 'all' : String(days)} aria-label="Sessions last written"
          data-test="spend-report-days"
          onChange={({ value }) => {
            const chosen = DAYS.find((choice) => (choice === null ? 'all' : String(choice)) === value)
            if (chosen !== undefined) setDays(chosen)
          }}>
          {DAYS.map((choice) => (
            <ControlItem key={choice ?? 'all'} value={choice === null ? 'all' : String(choice)}>
              {choice === null ? 'All time' : choice === 1 ? 'Last day' : `Last ${choice} days`}
            </ControlItem>
          ))}
        </SegmentedControl>
        <div className="spend-report-by-model">
          <Toggle size="small" checked={byModel} aria-label="By model" data-test="spend-report-by-model"
            onChange={({ checked }) => setByModel(checked)} />
          <span aria-hidden="true" onClick={() => setByModel((on) => !on)}>By model</span>
        </div>
      </div>
      {problem ? (
        <Alert type="error" size="small" role="alert" data-test="spend-report-error">{problem}</Alert>
      ) : report === null ? (
        <Skeleton label="Totalling the chats…" count={2} delayed={!showLoading} data-test="spend-report-loading" />
      ) : (
        <>
          <table className="spend-report-table" data-test="spend-report-table">
            <thead>
              <tr>
                <th scope="col">{byModel ? 'Model' : <span className="visually-hidden">Row</span>}</th>
                {COLUMNS.map((column) => <th scope="col" key={column}>{column}</th>)}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, index) => (
                <tr key={index} className={row.total ? 'spend-report-total' : undefined} data-test="spend-report-row">
                  <th scope="row" className="spend-report-label" data-tooltip={row.total ? undefined : row.label}>{row.label}</th>
                  <td>{row.sessions}</td>
                  <td>{row.turns}</td>
                  <td>{row.tokens}</td>
                  <td>{row.read}</td>
                  <td>{row.written}</td>
                  <td>{row.unsplit}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {unreadable > 0 && (
            <p className="spend-report-note" data-test="spend-report-unreadable">
              {unreadable === 1
                ? '1 entry in the sessions directory could not be read as a record and is not counted.'
                : `${unreadable} entries in the sessions directory could not be read as records and are not counted.`}
            </p>
          )}
        </>
      )}
      <p className="spend-report-note" data-test="spend-report-incognito">
        An incognito session leaves no record, so none is counted.
      </p>
    </Modal>
  )
}
