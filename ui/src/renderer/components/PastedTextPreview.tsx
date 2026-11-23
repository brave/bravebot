import { useState } from 'react'
import type { PastedText } from '../../shared/pastes'
import { Modal } from './Modal'
import { Button } from '../nala'

/**
 * A folded paste, whole and read-only (PASTE-13, the window's form of the terminal's info panel).
 * `pasted` is a copy the caller took, so nothing here reaches the draft, the caret or the staging.
 * The title and the subtitle name the paste; the code box only holds the text and Wrap. The box
 * keeps its own maximum height and scrolls, so the dialog is as tall as a short paste needs.
 */
export function PastedTextPreview({ pasted, onClose }: {
  pasted: PastedText
  onClose: () => void
}): React.JSX.Element {
  const [wrap, setWrap] = useState(false)
  const lines = `${pasted.lines} ${pasted.lines === 1 ? 'line' : 'lines'}`
  return <Modal title={`Pasted text #${pasted.number}`} size="lg" snug onClose={onClose} className="pasted-text-preview"
    subtitle={`${lines} · Viewing sends nothing; the text goes with your message when you send it.`}
    actions={<Button kind="filled" onClick={onClose}>Done</Button>}>
    <div className="code-block preview-code" data-test="pasted-text-preview">
      <div className="code-toolbar">
        <Button kind="plain-faint" size="tiny" className="code-wrap" aria-pressed={wrap} onClick={() => setWrap(!wrap)}>Wrap</Button>
      </div>
      <pre className={wrap ? 'code-wrapped' : ''}>{pasted.text}</pre>
    </div>
  </Modal>
}
