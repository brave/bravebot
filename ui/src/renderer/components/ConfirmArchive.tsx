import { Modal } from './Modal'
import { Button } from '../nala'

const WORDS = {
  bot: {
    title: 'Archive bot',
    where: 'moves to the Archived section of the bots list.',
    kept: 'Its memory and conversations are kept, and you can restore it from there at any time.',
  },
  conversation: {
    title: 'Archive conversation',
    where: 'is hidden from the chat list until you show archived chats.',
    kept: 'Nothing in it is deleted, and you can restore it from View options.',
  },
} as const

/**
 * The question asked before a bot or a conversation is archived. Archiving can be undone, so
 * the dialog says where the thing goes and how it comes back, and Cancel is the way out.
 */
export function ConfirmArchive({ kind, name, onConfirm, onCancel }: {
  kind: keyof typeof WORDS
  name: string
  onConfirm: () => void
  onCancel: () => void
}): React.JSX.Element {
  const words = WORDS[kind]
  return (
    <Modal title={words.title} size="sm" compact className="confirm-archive" onClose={onCancel}
      subtitle={<><strong className="confirm-archive-name">{name}</strong> {words.where}</>}
      actions={<>
        <Button kind="plain-faint" onClick={onCancel} data-test="archive-cancel">Cancel</Button>
        <Button kind="filled" onClick={onConfirm} data-test="archive-confirm">Archive</Button>
      </>}>
      <p>{words.kept}</p>
    </Modal>
  )
}
