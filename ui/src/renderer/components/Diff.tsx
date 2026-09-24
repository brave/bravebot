import type { Change } from '../../shared/protocol'
import { numberedDiffLines } from '../transcript'
import { useState } from 'react'
import { Modal } from './Modal'
import { Button } from './ui/button'
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
  return <div className="diff-review">
    <div className="code-toolbar"><span>Proposed changes</span><Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle><Button variant="outline" size="sm" onClick={() => setExpanded(true)}>Expand diff</Button></div>
    {body}
    {expanded && <Modal title="Review proposed changes" onClose={() => setExpanded(false)} className="expanded-diff">
      <div className="code-toolbar"><h2>Review proposed changes</h2><Toggle variant="outline" pressed={wrap} onPressedChange={setWrap}>Wrap lines</Toggle><Button variant="outline" size="sm" onClick={() => setExpanded(false)}>Done</Button></div>
      <p>Review the supplied changes here, then return to the approval card to decide.</p>{body}
    </Modal>}
  </div>
}
