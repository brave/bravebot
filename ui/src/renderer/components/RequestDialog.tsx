import { Modal } from './Modal'
import { Button } from '../nala'
import type { RequestReport } from '../reports'

/**
 * Chat › Last Request…, the desktop form of `/request` (TRACE-9): the instruction files the last
 * request's system prompt was built from, with their bytes, then each span under the label of
 * where its words came from.
 *
 * Read-only. The spans hold tool results and other untrusted text, so each is drawn as React text
 * in a `pre` and never as Markdown or HTML. The bridge has already drawn every string with
 * `printable_spelled`, so a control character arrives as its picture.
 */
export function RequestDialog({ report, onClose }: { report: RequestReport; onClose: () => void }): React.JSX.Element {
  return (
    <Modal title="Last request" subtitle={report.none ? undefined : 'What the last turn sent the model, read from the request itself'}
      size="lg" onClose={onClose} className="request-view"
      actions={<Button kind="filled" onClick={onClose} data-test="request-done">Done</Button>}>
      {report.none ? (
        <p data-test="request-none">No request has been sent to the model in this session yet.</p>
      ) : (
        <>
          <p className="request-files" data-test="request-files">
            {report.instructionFiles.length === 0
              ? 'Instruction files loaded: none'
              : `Instruction files loaded: ${report.instructionFiles.map((file) => `${file.path} (${file.bytes} bytes)`).join(', ')}`}
          </p>
          <ol className="request-spans">
            {report.spans.map((span, index) => (
              <li key={index} className="request-span" data-test="request-span">
                <div className="request-span-head">{span.role} · {span.label}</div>
                <pre className="request-span-text">{span.text}</pre>
              </li>
            ))}
          </ol>
        </>
      )}
    </Modal>
  )
}
