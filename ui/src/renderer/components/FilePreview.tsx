import { useEffect, useState } from 'react'
import type { FilePreview as Preview } from '../../shared/files'
import { Modal } from './Modal'
import { Alert, AlertDescription } from './ui/alert'
import { Button } from './ui/button'
import { DialogHeader, DialogTitle } from './ui/dialog'
import { Empty, EmptyDescription, EmptyHeader } from './ui/empty'
import { Spinner } from './ui/spinner'
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
  return <Modal title={`Preview ${path}`} onClose={onClose} className="file-preview w-[min(1000px,calc(100vw-48px))]">
    <DialogHeader className="code-toolbar flex-row items-center gap-2 text-left">
      <DialogTitle asChild><strong className="min-w-0 flex-1 break-anywhere">{path}</strong></DialogTitle>
      <Button variant="outline" size="sm" onClick={onClose}>Done</Button>
    </DialogHeader>
    <Alert className="preview-boundary border-l-[3px] border-l-confine bg-confine/10"><AlertDescription>For your review only. Previewing a file does not put its contents in the agent’s context.</AlertDescription></Alert>
    <div className="preview-actions flex gap-2">
      <Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle>
      <Button variant="outline" onClick={() => { void window.bravebot.openFile(session, path).then((outcome) => { if (outcome.status === 'failed') setProblem(outcome.message) }).catch(() => setProblem('The file could not be opened.')) }}>Open in default app</Button>
    </div>
    {problem && <Alert variant="destructive"><AlertDescription>{problem}</AlertDescription></Alert>}
    {loading ? <p role="status"><Spinner aria-hidden="true" />Loading preview…</p> : preview ? <>
      {preview.truncated && <p role="status">Showing the first 128 KB. Open the file to review the rest.</p>}
      <pre className={`max-h-[60vh] overflow-auto rounded-lg border border-border bg-code p-3.5 font-mono text-xs leading-relaxed ${wrap ? 'wrapped whitespace-pre-wrap break-anywhere' : ''}`}>{preview.text}</pre>
    </> : <Empty className="p-0 text-left md:p-0"><EmptyHeader className="items-start text-left"><EmptyDescription>This file is binary, unavailable, or outside the project. A text preview is not available.</EmptyDescription></EmptyHeader></Empty>}
  </Modal>
}
