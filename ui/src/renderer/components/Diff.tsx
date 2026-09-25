import type { Change } from '../../shared/protocol'
import { numberedDiffLines } from '../transcript'
import { useState } from 'react'
import { Modal } from './Modal'
import { Button } from './ui/button'
import { Card, CardContent, CardHeader } from './ui/card'
import { Toggle } from './ui/toggle'

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
  const body = (
    <pre className={`diff ${wrap ? 'wrapped' : 'unwrapped'}`}>
      {numberedDiffLines(changes).map((line, index) => (
        <div key={index} className={`line ${line.kind}`}>
          <span className="line-number" aria-label={line.before ? `Original line ${line.before}` : undefined}>{line.before}</span>
          <span className="line-number" aria-label={line.after ? `Proposed line ${line.after}` : undefined}>{line.after}</span>
          <span className="sign">{line.sign}</span>
          <span className="text">{line.text}</span>
        </div>
      ))}
    </pre>
  )
  return <Card className="diff-review gap-0 overflow-hidden py-0">
    <CardHeader className="code-toolbar flex-row items-center gap-2 border-b px-2.5 py-1.5 [.border-b]:pb-1.5">
      <span className="min-w-0 flex-1 text-[11px] font-medium text-ink-dim">Proposed changes</span>
      <Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle>
      <Button variant="outline" size="sm" onClick={() => setExpanded(true)}>Expand diff</Button>
    </CardHeader>
    <CardContent className="p-0">{body}</CardContent>
    {expanded && <Modal title="Review proposed changes" onClose={() => setExpanded(false)} className="expanded-diff w-[min(1080px,calc(100vw-48px))]">
      <div className="code-toolbar mb-3 flex items-center gap-2">
        <h2 className="m-0 min-w-0 flex-1 text-lg font-semibold">Review proposed changes</h2>
        <Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle>
        <Button variant="outline" size="sm" onClick={() => setExpanded(false)}>Done</Button>
      </div>
      <p>Review the supplied changes here, then return to the approval card to decide.</p>{body}
    </Modal>}
  </Card>
}
