import { useState } from 'react'
import { Modal } from './Modal'
import { Button, TextArea } from '../nala'

/**
 * A note on the open chat (SESSION-36). The text is kept here until Add, so typing re-draws only
 * this dialog.
 */
export function NoteDialog({ running, onCancel, onAdd }: {
  running: boolean
  onCancel: () => void
  /** Resolves true when the dialog can close. */
  onAdd: (text: string) => Promise<boolean>
}): React.JSX.Element {
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const add = async () => {
    if (busy || running || !text.trim()) return
    setBusy(true)
    const done = await onAdd(text)
    setBusy(false)
    if (done) onCancel()
  }
  return (
    <Modal title="Add note" size="md" className="note-dialog"
      subtitle="Kept on this chat record. Never sent to the model."
      onClose={busy ? undefined : onCancel} actions={<>
        <Button kind="plain-faint" size="small" className="modal-leading" isDisabled={busy} onClick={onCancel} data-test="note-cancel">
          Cancel
        </Button>
        <Button kind="filled" size="small" isDisabled={busy || running || !text.trim()} onClick={() => void add()} data-test="note-add">
          Add
        </Button>
      </>}>
      <TextArea
        autofocus
        value={text}
        minRows={3}
        maxRows={10}
        onInput={({ value }) => setText(value)}
        onChange={({ value }) => setText(value)}
        data-test="note-text"
      >Note</TextArea>
    </Modal>
  )
}
