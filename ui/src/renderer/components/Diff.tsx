import type { Change } from '../../shared/protocol'
import { numberedDiffLines } from '../transcript'
import { useMemo, useState } from 'react'
import { Modal } from './Modal'
import { Button, Icon } from '../nala'
import { IconButton } from './IconButton'
import { highlightLine, languageOf } from '../highlight'

/**
 * A condensed diff, as the reviewer sees it.
 *
 * The complete file is never sent and is deliberately not shown: an approval the reviewer
 * cannot actually read is decorative, and a whole-file body asks them to spot the
 * difference themselves. Two lines of context either side, matching the terminal, so an
 * approval means the same thing in both interfaces.
 *
 * Coloured by the file's extension, except for a write nobody vouched for: that is a stranger's
 * patch, and it is read as the plain text it is.
 */
export function Diff({ changes, path, untrusted = false }: { changes: Change[]; path?: string; untrusted?: boolean }): React.JSX.Element {
  const [expanded, setExpanded] = useState(false)
  const [wrap, setWrap] = useState(true)
  const language = untrusted || !path ? null : languageOf(path)
  const lines = useMemo(() => numberedDiffLines(changes), [changes])
  const wrapToggle = (
    <Button kind="plain-faint" size="tiny" className="code-wrap" aria-pressed={wrap} aria-label="Wrap lines"
      data-tooltip={wrap ? 'Stop wrapping long lines' : 'Wrap long lines'} onClick={() => setWrap(!wrap)}>Wrap</Button>
  )
  const body = (
    <pre className={`diff ${wrap ? 'wrapped' : 'unwrapped'}`}>
      {lines.map((line, index) => (
        <div key={index} className={`line ${line.kind}`}>
          <span className="line-number" aria-label={line.before ? `Original line ${line.before}` : undefined}>{line.before}</span>
          <span className="line-number" aria-label={line.after ? `Proposed line ${line.after}` : undefined}>{line.after}</span>
          <span className="sign">{line.sign}</span>
          {line.kind === 'elided'
            ? <span className="text"><Icon name="more-horizontal" />{line.text}</span>
            : <span className="text">{line.spans
              ? line.spans.map((span, at) => <span key={at} className={span.changed ? 'changed' : undefined}>{highlightLine(span.text, language)}</span>)
              : highlightLine(line.text, language)}</span>}
        </div>
      ))}
    </pre>
  )
  return <div className="diff-review">
    <div className="code-toolbar">
      <span className="code-language">Proposed changes</span>
      {wrapToggle}
      <IconButton icon="fullscreen-on" label="Expand diff" tooltip="Review in a larger view" size="tiny" onClick={() => setExpanded(true)} />
    </div>
    {body}
    {expanded && <Modal title="Review proposed changes" size="xl" onClose={() => setExpanded(false)} className="expanded-diff"
      subtitle="Review the supplied changes here, then return to the approval card to decide."
      actions={<Button kind="filled" onClick={() => setExpanded(false)}>Done</Button>}>
      <div className="diff-review">
        <div className="code-toolbar"><span className="code-language">{path ?? 'Proposed changes'}</span>{wrapToggle}</div>
        {body}
      </div>
    </Modal>}
  </div>
}
