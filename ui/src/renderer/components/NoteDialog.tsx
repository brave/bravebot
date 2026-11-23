import { AskDialog } from './AskDialog'

/** A note on the open chat (SESSION-36). */
export function NoteDialog({ running, onCancel, onAdd }: {
  running: boolean
  onCancel: () => void
  /** Resolves true when the dialog can close. */
  onAdd: (text: string) => Promise<boolean>
}): React.JSX.Element {
  return (
    <AskDialog title="Add note" subtitle="Kept on this chat record. Never sent to the model." label="Note" action="Add"
      test="note" className="note-dialog" running={running} onCancel={onCancel} onConfirm={onAdd} />
  )
}
