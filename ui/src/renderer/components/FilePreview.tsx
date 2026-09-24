import { useEffect, useState } from 'react'
import type { FilePreview as Preview } from '../../shared/files'
import { Modal } from './Modal'
import { Button } from './ui/button'
import { Toggle } from './ui/toggle'

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
  return <Modal title={`Preview ${path}`} onClose={onClose} className="file-preview">
    <div className="code-toolbar"><strong>{path}</strong><Button variant="outline" size="sm" onClick={onClose}>Done</Button></div>
    <p className="preview-boundary">For your review only. Previewing a file does not put its contents in the agent’s context.</p>
    <div className="preview-actions"><Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle>
      <Button variant="outline" onClick={() => { void window.bravebot.openFile(session, path).then((outcome) => { if (outcome.status === 'failed') setProblem(outcome.message) }).catch(() => setProblem('The file could not be opened.')) }}>Open in default app</Button></div>
    {problem && <p role="alert">{problem}</p>}
    {loading ? <p role="status">Loading preview…</p> : preview ? <>
      {preview.truncated && <p role="status">Showing the first 128 KB. Open the file to review the rest.</p>}
      <pre className={wrap ? 'wrapped' : ''}>{preview.text}</pre>
    </> : <p>This file is binary, unavailable, or outside the project. A text preview is not available.</p>}
  </Modal>
}
