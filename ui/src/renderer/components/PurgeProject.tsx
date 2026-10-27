import { useEffect, useState } from 'react'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, ProgressRing } from '../nala'
import { call } from '../request'
import { showToast } from '../toasts'
import { ConfirmDelete } from './ConfirmDelete'
import { Modal } from './Modal'
import { plural } from '../plural'

/** What `session.purge` with `dryRun` answers: the records it would remove, and the ids it holds. */
interface Plan {
  delete: { id: string; title: string; updated: number }[]
  held: string[]
}


/**
 * The question asked before every chat of one project is deleted.
 *
 * The agent says which records the project wrote and which it holds. A held id this window has
 * open is deleted with the rest, since the window closes it first; one held by a background
 * session is kept, and the question says how many.
 */
export function PurgeProject({ directory, opened, onConfirm, onClose }: {
  directory: string
  /** The ids of this project's conversations the window has open. */
  opened: ReadonlySet<string>
  onConfirm: (ids: string[]) => void
  onClose: () => void
}): React.JSX.Element | null {
  const [plan, setPlan] = useState<Plan | null>(null)
  const [problem, setProblem] = useState('')
  const showLoading = useDelayedFlag(plan === null && !problem)

  useEffect(() => {
    let gone = false
    call<Plan>('session.purge', { directory, dryRun: true })
      .then((answer) => { if (!gone) setPlan(answer) })
      .catch((error) => { if (!gone) setProblem(error instanceof Error ? error.message : String(error)) })
    return () => { gone = true }
  }, [directory])

  const ids = plan ? [...plan.delete.map((row) => row.id), ...plan.held.filter((id) => opened.has(id))] : []
  const kept = plan ? plan.held.filter((id) => !opened.has(id)).length : 0
  const empty = plan !== null && ids.length === 0

  useEffect(() => {
    if (!empty) return
    showToast('Nothing to delete', kept ? `${plural(kept, 'chat')} in use by a background session` : directory, 'note')
    onClose()
  }, [empty, kept, directory, onClose])

  if (empty) return null
  if (plan) {
    const detail = `${plural(ids.length, 'conversation')} will be deleted.`
      + (kept ? ` ${plural(kept, 'conversation')} in use by a background session will be kept.` : '')
    return <ConfirmDelete kind="project" name={directory} detail={detail} onCancel={onClose} onConfirm={() => onConfirm(ids)} />
  }
  return (
    <Modal title="Delete all chats" size="sm" compact className="confirm-archive" onClose={onClose}
      actions={problem ? <Button kind="filled" onClick={onClose}>Close</Button> : undefined}>
      {problem
        ? <Alert type="error" size="small" role="alert">{problem}</Alert>
        : <p role="status" className="dialog-busy" data-delayed={!showLoading || undefined}><ProgressRing mode="indeterminate" /> Finding the chats…</p>}
    </Modal>
  )
}
