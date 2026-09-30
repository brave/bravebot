import { useEffect, useState } from 'react'
import type { FilePreview as Preview } from '../../shared/files'
import { Modal } from './Modal'
import { Alert, Button, Icon, ProgressRing } from '../nala'
import { FileGlyph } from './FileGlyph'

export function FilePreview({ session, path, onClose }: { session: string; path: string; onClose: () => void }): React.JSX.Element {
  const [preview, setPreview] = useState<Preview | null>(null)
  const [loading, setLoading] = useState(true)
  const [problem, setProblem] = useState('')
  const [wrap, setWrap] = useState(false)
  useEffect(() => {
    let gone = false
    setLoading(true); setPreview(null); setProblem('')
    void window.bravebot.previewFile(session, path).then((value) => { if (!gone) setPreview(value) })
      .catch(() => { if (!gone) setProblem('Preview unavailable.') }).finally(() => { if (!gone) setLoading(false) })
    return () => { gone = true }
  }, [session, path])
  const open = () => {
    void window.bravebot.openFile(session, path).then((outcome) => { if (outcome.status === 'failed') setProblem(outcome.message) }).catch(() => setProblem('The file could not be opened.'))
  }
  return <Modal title={`Preview ${path}`} size="xl" onClose={onClose} className="file-preview"
    subtitle="For your review only. Previewing a file does not put its contents in the agent’s context."
    actions={<Button kind="filled" size="small" onClick={onClose}>Done</Button>}>
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    <div className="code-block preview-code">
      <div className="code-toolbar">
        <FileGlyph name={path} />
        <span className="code-language" data-tooltip={path}>{path}</span>
        <Button kind="plain-faint" size="tiny" className="code-wrap" aria-pressed={wrap} onClick={() => setWrap(!wrap)}>Wrap</Button>
        <Button kind="plain-faint" size="tiny" onClick={open}><Icon name="launch" slot="icon-before" />Open in default app</Button>
      </div>
      {loading ? <p role="status" className="preview-status"><ProgressRing mode="indeterminate" /> Loading preview…</p> : preview ? <>
        {preview.truncated && <p role="status" className="preview-status">Showing the first 128 KB. Open the file to review the rest.</p>}
        <pre className={wrap ? 'code-wrapped' : ''}>{preview.text}</pre>
      </> : <p className="preview-status">This file is binary, unavailable, or outside the project. A text preview is not available.</p>}
    </div>
  </Modal>
}
