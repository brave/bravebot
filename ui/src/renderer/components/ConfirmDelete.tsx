import { Modal } from './Modal'
import { Button } from '../nala'

const WORDS = {
  conversation: {
    title: 'Delete conversation',
    what: 'will be deleted from this computer.',
    cost: 'Its messages and the record of what the agent did are removed. This cannot be undone.',
  },
  bot: {
    title: 'Delete bot',
    what: 'will be deleted.',
    cost: 'Its local memory history is removed. Project files, conversations and the home folder stay. This cannot be undone.',
  },
} as const

/** The question asked before something is deleted for good. */
export function ConfirmDelete({ kind, name, onConfirm, onCancel }: {
  kind: keyof typeof WORDS
  name: string
  onConfirm: () => void
  onCancel: () => void
}): React.JSX.Element {
  const words = WORDS[kind]
  return (
    <Modal title={words.title} size="sm" compact className="confirm-archive" onClose={onCancel}
      subtitle={<><strong className="confirm-archive-name">{name}</strong> {words.what}</>}
      actions={<>
        <Button kind="plain-faint" onClick={onCancel} data-test="delete-cancel">Cancel</Button>
        <Button kind="filled" className="confirm-delete" onClick={onConfirm} data-test="delete-confirm">Delete</Button>
      </>}>
      <p>{words.cost}</p>
    </Modal>
  )
}
