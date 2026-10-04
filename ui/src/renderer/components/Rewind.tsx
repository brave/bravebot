import { Modal } from './Modal'
import { Alert, Button } from '../nala'
import type { RewindPlan } from '../rewind'

/**
 * The question asked before a session is put back.
 *
 * It names every file that will be overwritten with what it held before, because that is the
 * part of a rewind nobody can take back from here, and says plainly what the backups do not
 * cover so nothing on disk is assumed restored that was not.
 */
export function RewindConfirm({ plan, bot, running, busy, onCancel, onConfirm }: {
  plan: RewindPlan
  /** A bot's session: its memory file is its own and is not rolled back. */
  bot: boolean
  running: boolean
  busy: boolean
  onCancel: () => void
  onConfirm: () => void
}): React.JSX.Element {
  const one = plan.steps === 1
  const warnings = bot
    ? [...plan.warnings, 'The bot’s memory isn’t rolled back. Anything it saved stays.']
    : plan.warnings
  return (
    <Modal title={one ? 'Undo turn' : `Rewind ${plan.steps} turns`} size="md" className="rewind-confirm"
      subtitle={`Puts the files and the conversation back to before turn ${plan.turn}.`}
      onClose={busy ? undefined : onCancel} actions={<>
        <Button kind="plain-faint" size="small" className="modal-leading" isDisabled={busy} onClick={onCancel} data-test="rewind-cancel">
          Cancel
        </Button>
        <Button kind="filled" size="small" isDisabled={busy || running} onClick={onConfirm} data-test="rewind-confirm">
          {one ? 'Undo' : `Rewind ${plan.steps} turns`}
        </Button>
      </>}>
      {plan.paths.length ? <>
        <h4>{plan.paths.length === 1 ? 'This file is put back' : `These ${plan.paths.length} files are put back`}</h4>
        <ul className="rewind-paths" data-test="rewind-paths">
          {plan.paths.map((path) => <li key={path}><code>{path}</code></li>)}
        </ul>
      </> : <p>No files were written, so only the conversation goes back.</p>}
      {warnings.length > 0 && (
        <Alert type="warning" size="small" className="rewind-warnings" data-test="rewind-warnings">
          {warnings.length === 1 ? warnings[0] : <ul>{warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>}
        </Alert>
      )}
      <p className="rewind-aside">The prompt that began turn {plan.turn} goes back in the composer to edit.</p>
      {running && <p className="rewind-aside">Wait for the turn to finish.</p>}
    </Modal>
  )
}
