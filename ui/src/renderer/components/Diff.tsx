import type { Change } from '../../shared/protocol'
import { numberedDiffLines } from '../transcript'
import { useState } from 'react'
import { cn, SCRIM } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'

/**
 * The lid a reviewable block wears, written once.
 *
 * The inline diff and the expanded one are the same object seen at two sizes, and a toolbar
 * that drifted between them would tell the reviewer otherwise.
 */
const TOOLBAR = 'code-toolbar flex items-center gap-2 border-b border-border bg-bubble-agent px-2.5 py-[7px] text-xs'

/**
 * A condensed diff, as the reviewer sees it.
 *
 * The complete file is never sent and is deliberately not shown: an approval the reviewer
 * cannot actually read is decorative, and a whole-file body asks them to spot the
 * difference themselves. Two lines of context either side, matching the terminal, so an
 * approval means the same thing in both interfaces.
 */
export function Diff({ changes }: { changes: Change[] }): React.JSX.Element {
  const [expanded, setExpanded] = useState(false)
  const [wrap, setWrap] = useState(true)
  // The cap is the caller's, because the two places this is drawn are answering different
  // questions: inline it must leave the approval's buttons on screen, and expanded it is
  // the only thing on screen and may take most of the window.
  const body = (cap: string) => (
    <pre className={cn('diff m-0 overflow-auto font-mono text-[13px]/[1.65]', cap, wrap ? 'wrapped' : 'unwrapped')}>
      {numberedDiffLines(changes).map((line, index) => (
        <div
          key={index}
          className={cn(
            'line flex px-2',
            // Wrapping is a choice the reviewer makes: a wrapped line is easier to read whole,
            // an unwrapped one is what is actually in the file.
            wrap ? 'whitespace-pre-wrap' : 'whitespace-pre',
            line.kind,
            line.kind === 'added' && 'bg-success/10 text-success',
            line.kind === 'removed' && 'bg-destructive/10 text-destructive',
            line.kind === 'elided' && 'text-muted-foreground/70 italic',
          )}
        >
          {/* Fixed boxes and tabular figures, so the numbers form a column the eye can run
              down rather than a ragged edge that moves with every line's width. Unselectable:
              a drag across the diff is somebody copying the code, not the margin. */}
          <span className="line-number w-[3.5em] flex-[0_0_3.5em] pr-[0.7em] text-right text-muted-foreground tabular-nums select-none" aria-label={line.before ? `Original line ${line.before}` : undefined}>{line.before}</span>
          <span className="line-number w-[3.5em] flex-[0_0_3.5em] pr-[0.7em] text-right text-muted-foreground tabular-nums select-none" aria-label={line.after ? `Proposed line ${line.after}` : undefined}>{line.after}</span>
          <span className="sign w-[1.2em] shrink-0 text-muted-foreground/70">{line.sign}</span>
          <span className="text">{line.text}</span>
        </div>
      ))}
    </pre>
  )
  return <div className="diff-review my-3 overflow-hidden rounded-[9px] border border-border">
    <div className={TOOLBAR}><span className="flex-1 text-muted-foreground">Proposed changes</span><Button variant="outline" size="sm" className="text-xs" onClick={() => setWrap(!wrap)} aria-pressed={wrap}>Wrap lines</Button><Button variant="outline" size="sm" className="text-xs" onClick={() => setExpanded(true)}>Expand diff</Button></div>
    {body('max-h-80')}
    {expanded && (
      <Dialog open onOpenChange={(next) => { if (!next) setExpanded(false) }}>
        <DialogContent className="modal expanded-diff max-h-[calc(100vh-64px)] w-[min(1080px,calc(100vw-48px))] overflow-y-auto p-7 sm:max-w-[min(1080px,calc(100vw-48px))]" showCloseButton={false} overlayClassName={SCRIM}>
          <DialogTitle className="sr-only">Review proposed changes</DialogTitle>
          <div className={TOOLBAR}><h2 className="m-0 flex-1 text-base">Review proposed changes</h2><Button variant="outline" size="sm" className="text-xs" onClick={() => setWrap(!wrap)} aria-pressed={wrap}>Wrap lines</Button><Button variant="outline" size="sm" className="text-xs" onClick={() => setExpanded(false)}>Done</Button></div>
          <p>Review the supplied changes here, then return to the approval card to decide.</p>{body('max-h-[65vh]')}
        </DialogContent>
      </Dialog>
    )}
  </div>
}
