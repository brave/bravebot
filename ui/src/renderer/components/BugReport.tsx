import { useEffect, useState } from 'react'
import { Modal } from './Modal'
import { Alert, Button, Icon, ProgressRing } from '../nala'
import { useDelayedFlag } from '../hooks'
import { showToast } from '../toasts'
import type { Composed } from '../../shared/bug-report'

/** What the privacy note says, close to DIAG-8's known cost. */
export const PRIVACY_NOTE = 'This names paths, model services and the names of environment variables. Read it before you post it. It holds no conversation, file contents or credentials.'

/**
 * The bug report (DIAG-9): what a saved one would hold, shown before anything is written.
 *
 * Nothing is sent anywhere. Save writes a file where the person chooses, Copy puts the text on the
 * clipboard, Open an Issue opens a new issue's page with nothing filled in, and Show Log shows the
 * newest diagnostic log in the file manager so it can be attached by hand.
 */
export function BugReport({ onClose }: { onClose: () => void }): React.JSX.Element {
  const [report, setReport] = useState<Composed | null>(null)
  const [failure, setFailure] = useState('')
  const [busy, setBusy] = useState(false)
  const loading = report === null
  const showLoading = useDelayedFlag(loading)

  useEffect(() => {
    let live = true
    void window.bravebot.composeBugReport()
      .then((composed) => { if (live) setReport(composed) })
      .catch(() => { if (live) setReport({ available: false, text: '', log: null }) })
    return () => { live = false }
  }, [])

  const ready = report?.available === true

  async function save() {
    setBusy(true)
    setFailure('')
    try {
      const outcome = await window.bravebot.saveBugReport()
      if (outcome.status === 'saved') showToast('Bug report saved', outcome.where)
      else if (outcome.status === 'failed') setFailure(outcome.message)
    } catch (error) {
      setFailure(`The report could not be saved: ${String(error)}`)
    } finally {
      setBusy(false)
    }
  }

  async function copy() {
    if (!report?.available) return
    try {
      await navigator.clipboard.writeText(report.text)
      showToast('Bug report copied')
    } catch {
      setFailure('Could not copy the report. Select the text and copy it instead.')
    }
  }

  return (
    <Modal title="Report a bug" subtitle="Nothing is sent. Save or copy the report, then attach it to an issue." size="lg" onClose={onClose} className="bug-report"
      actions={<>
        {report?.log && <Button kind="plain-faint" className="modal-leading" onClick={() => void window.bravebot.showBugReportLog()} data-test="bug-report-log">
          <Icon name="folder-open" slot="icon-before" />Show Log
        </Button>}
        <Button kind="outline" isDisabled={!ready} onClick={() => void window.bravebot.openPage('new-issue')} data-test="bug-report-issue">
          Open an Issue<Icon name="launch" slot="icon-after" />
        </Button>
        <Button kind="outline" isDisabled={!ready} onClick={() => void copy()} data-test="bug-report-copy">Copy</Button>
        <Button kind="filled" isDisabled={!ready || busy} onClick={() => void save()} data-test="bug-report-save">Save Report…</Button>
      </>}>
      {loading ? (
        <p role="status" className="preview-status" data-delayed={!showLoading || undefined}><ProgressRing mode="indeterminate" /> Composing the report…</p>
      ) : ready ? <>
        <Alert type="warning" size="small" className="bug-report-privacy" data-test="bug-report-privacy">{PRIVACY_NOTE}</Alert>
        <pre className="bug-report-text" tabIndex={0} aria-label="Report text" data-test="bug-report-text">{report.text}</pre>
        {report.log && <p className="bug-report-log">Attach the newest diagnostic log separately: <code>{report.log}</code></p>}
      </> : (
        <p role="status" className="preview-status" data-test="bug-report-unavailable">Bug reports aren't available in a session that keeps nothing.</p>
      )}
      {failure && <Alert type="error" size="small" role="alert" data-test="bug-report-failure">{failure}</Alert>}
    </Modal>
  )
}
