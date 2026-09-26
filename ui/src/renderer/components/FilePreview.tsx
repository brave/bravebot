import { useEffect, useState } from 'react'
import { cn, SCRIM } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import type { FilePreview as Preview } from '../../shared/files'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'

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
  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      {/* Wider than the window's other sheets, and for the same reason the expanded diff is: a
          line of somebody's source is as long as they wrote it, and a preview that folds it is
          not showing the file. */}
      <DialogContent className="modal file-preview max-h-[calc(100vh-64px)] w-[min(1000px,calc(100vw-48px))] overflow-y-auto p-7 sm:max-w-[min(1000px,calc(100vw-48px))]" showCloseButton={false} overlayClassName={SCRIM}>
        <DialogTitle className="sr-only">{`Preview ${path}`}</DialogTitle>
        <div className="code-toolbar flex items-center gap-2 border-b border-border bg-bubble-agent px-2.5 py-[7px] text-xs"><strong className="min-w-0 flex-1 wrap-anywhere">{path}</strong><Button variant="outline" size="sm" className="text-xs" onClick={onClose}>Done</Button></div>
        {/* Drawn as confined content is drawn, because that is what it is: bytes this window
            will show and not release. */}
        <p className="preview-boundary border-l-[3px] border-confine bg-confine/10 px-3 py-2.5 text-xs">For your review only. Previewing a file does not put its contents in the agent’s context.</p>
        <div className="preview-actions flex gap-2"><Button variant="outline" size="sm" className="text-xs" aria-pressed={wrap} onClick={() => setWrap(!wrap)}>Wrap lines</Button>
          <Button variant="outline" size="sm" className="text-xs" onClick={() => { void window.bravebot.openFile(session, path).then((outcome) => { if (outcome.status === 'failed') setProblem(outcome.message) }).catch(() => setProblem('The file could not be opened.')) }}>Open in default app</Button></div>
        {problem && <p role="alert">{problem}</p>}
        {loading ? <p role="status">Loading preview…</p> : preview ? <>
          {preview.truncated && <p role="status">Showing the first 128 KB. Open the file to review the rest.</p>}
          {/* Wrapping is the reader's choice. Unwrapped is what is actually in the file, which is
              the reading this window defaults to. */}
          <pre className={cn(
            'max-h-[60vh] overflow-auto bg-code p-3.5 font-mono text-xs/[1.6]',
            wrap && 'wrapped whitespace-pre-wrap wrap-anywhere',
          )}>{preview.text}</pre>
        </> : <p>This file is binary, unavailable, or outside the project. A text preview is not available.</p>}
      </DialogContent>
    </Dialog>
  )
}
