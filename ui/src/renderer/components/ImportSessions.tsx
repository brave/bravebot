import { useEffect, useState } from 'react'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, Checkbox, Label, ProgressRing } from '../nala'
import { call } from '../request'
import { shortAgo } from '../time'
import { showToast } from '../toasts'
import { Modal } from './Modal'

/** One of Claude Code's sessions for the project, as `session.import` lists it. */
export interface Found {
  sourceId: string
  id: string
  /** The first thing the person asked there: another program's text, drawn as plain text. */
  title: string
  updated: number
  imported: boolean
}

interface Copied {
  copied: string[]
  already: string[]
  failed: { sourceId: string; message: string }[]
}

/** The list after a copy that partly failed: what went in is marked imported and no longer chosen. */
export function afterCopy(found: Found[], chosen: ReadonlySet<string>, answer: Copied): { found: Found[]; chosen: ReadonlySet<string> } {
  const done = new Set([...answer.copied, ...answer.already])
  const rows = found.map((one) => (done.has(one.id) ? { ...one, imported: true } : one))
  return { found: rows, chosen: new Set([...chosen].filter((sourceId) => !rows.some((one) => one.sourceId === sourceId && one.imported))) }
}

const plural = (count: number, one: string) => `${count} ${one}${count === 1 ? '' : 's'}`

/**
 * Copy chosen Claude Code sessions of one project into this app's chats (SESSION-32).
 *
 * The agent lists what Claude Code kept for the folder and copies the names sent back, which are
 * names it listed. A session copied before is shown ticked off rather than offered again.
 */
export function ImportSessions({ directory, onImported, onClose }: {
  directory: string
  onImported: () => void
  onClose: () => void
}): React.JSX.Element {
  const [found, setFound] = useState<Found[] | null>(null)
  const [problem, setProblem] = useState('')
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set())
  const [copying, setCopying] = useState(false)
  const showLoading = useDelayedFlag(found === null && !problem)

  useEffect(() => {
    let gone = false
    call<{ found: Found[] }>('session.import', { directory })
      .then((answer) => { if (!gone) setFound(answer.found) })
      .catch((error) => { if (!gone) setProblem(error instanceof Error ? error.message : String(error)) })
    return () => { gone = true }
  }, [directory])

  const pick = (sourceId: string, on: boolean) => {
    // Leo reports a `checked` the window set as well as a click, so only a change counts.
    if (on === chosen.has(sourceId)) return
    const next = new Set(chosen)
    if (on) next.add(sourceId)
    else next.delete(sourceId)
    setChosen(next)
  }

  const copy = async () => {
    setCopying(true)
    setProblem('')
    try {
      const answer = await call<Copied>('session.import', { directory, ids: [...chosen] })
      onImported()
      if (answer.failed.length > 0) {
        const after = afterCopy(found ?? [], chosen, answer)
        setFound(after.found)
        setChosen(after.chosen)
        setProblem(`${plural(answer.failed.length, 'session')} could not be copied: ${answer.failed[0]!.message}`)
        setCopying(false)
        return
      }
      showToast(`Imported ${plural(answer.copied.length, 'chat')}`, answer.already.length ? `${plural(answer.already.length, 'chat')} had been imported already` : undefined)
      onClose()
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
      setCopying(false)
    }
  }

  return (
    <Modal title="Import from Claude Code" size="md" onClose={onClose}
      subtitle={<span className="confirm-archive-name">{directory}</span>}
      actions={found && found.length > 0 ? <>
        <Button kind="plain-faint" onClick={onClose} data-test="import-cancel">Cancel</Button>
        <Button kind="filled" isDisabled={chosen.size === 0 || copying} onClick={() => void copy()} data-test="import-confirm">
          {chosen.size === 0 ? 'Import' : `Import ${chosen.size}`}
        </Button>
      </> : <Button kind="filled" onClick={onClose}>Close</Button>}>
      {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
      {found === null && !problem && (
        <p role="status" className="dialog-busy" data-delayed={!showLoading || undefined}><ProgressRing mode="indeterminate" /> Reading Claude Code’s sessions…</p>
      )}
      {found?.length === 0 && <p data-test="import-empty">No Claude Code sessions for this folder.</p>}
      {found && found.length > 0 && <ImportList found={found} chosen={chosen} copying={copying} onPick={pick} />}
    </Modal>
  )
}

/** The sessions on offer, one checkbox each. A title is drawn as text, whatever it holds. */
export function ImportList({ found, chosen, copying, onPick }: {
  found: Found[]
  chosen: ReadonlySet<string>
  copying: boolean
  onPick: (sourceId: string, on: boolean) => void
}): React.JSX.Element {
  return (
    <ul className="import-sessions" data-test="import-sessions">
      {found.map((one) => (
        <li key={one.sourceId} className="import-session" data-imported={one.imported || undefined}>
          <Checkbox checked={one.imported || chosen.has(one.sourceId)} isDisabled={one.imported || copying} size="small"
            onChange={({ checked }) => onPick(one.sourceId, checked)}>
            <span className="import-session-title" data-tooltip={one.title}>{one.title}</span>
          </Checkbox>
          {one.imported && <Label mode="outline" color="neutral">Imported</Label>}
          <time className="import-session-time" dateTime={new Date(one.updated * 1000).toISOString()}>{shortAgo(one.updated)}</time>
        </li>
      ))}
    </ul>
  )
}
