import { useState } from 'react'
import { Modal } from './Modal'
import { Button, TextArea } from '../nala'

/**
 * A dialog that asks for one piece of text about the open chat and hands it to `onConfirm`. The text
 * is kept here until the action, so typing re-draws only this dialog.
 *
 * `optional` lets the action go with the field empty, as the focus of Compact Conversation… may be
 * (CMD-34). `test` names the hooks: `<test>-text`, `<test>-cancel` and `<test>-<action>`, with the
 * action's label in lower case with its spaces as hyphens.
 */
export function AskDialog({ title, subtitle, label, action, optional = false, rows = 3, test, className, running, onCancel, onConfirm }: {
  title: string
  subtitle: string
  /** The field's label. */
  label: string
  /** The primary button's label. */
  action: string
  optional?: boolean
  rows?: number
  test: string
  className?: string
  /** A turn is running in the chat, so the action waits. */
  running: boolean
  onCancel: () => void
  /** Resolves true when the dialog can close. */
  onConfirm: (text: string) => Promise<boolean>
}): React.JSX.Element {
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const blocked = busy || running || (!optional && !text.trim())
  const confirm = async () => {
    if (blocked) return
    setBusy(true)
    const done = await onConfirm(text)
    setBusy(false)
    if (done) onCancel()
  }
  return (
    <Modal title={title} size="md" className={className} subtitle={subtitle}
      onClose={busy ? undefined : onCancel} actions={<>
        <Button kind="plain-faint" size="small" isDisabled={busy} onClick={onCancel} data-test={`${test}-cancel`}>
          Cancel
        </Button>
        <Button kind="filled" size="small" isDisabled={blocked} onClick={() => void confirm()} data-test={`${test}-${action.toLowerCase().replace(/\s+/g, '-')}`}>
          {action}
        </Button>
      </>}>
      <TextArea
        autofocus
        value={text}
        minRows={rows}
        maxRows={10}
        onInput={({ value }) => setText(value)}
        onChange={({ value }) => setText(value)}
        data-test={`${test}-text`}
      >{label}</TextArea>
    </Modal>
  )
}
