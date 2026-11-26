import { Modal } from './Modal'
import { Button } from '../nala'
import type { NoticeRow } from '../reports'

/** What `changelog.read` returns. */
export interface ReleaseNotes { version: string; running: boolean; notes: string | null }

/** The notice Help › Release Notes shows (CMD-25). The notes are plain text, not Markdown. */
export function releaseNotesNotice(read: ReleaseNotes): { title: string; subtitle?: string; body: string } {
  if (read.notes === null) return { title: 'Release notes', body: 'No release notes in this build.' }
  return {
    title: `Release notes for ${read.version}`,
    subtitle: read.running ? undefined : 'This build has no notes of its own. These are the newest release’s.',
    body: read.notes,
  }
}

/**
 * A report in a dialog: plain text, or where `rows` is set, a list of labels beside their values
 * in two columns, so a value that wraps stays in its own column.
 */
export function Notice({ title, subtitle, body, rows, onClose }: { title: string; subtitle?: string; body: string; rows?: NoticeRow[]; onClose: () => void }): React.JSX.Element {
  return (
    <Modal title={title} subtitle={subtitle} size="md" onClose={onClose} className="notice"
      actions={<Button kind="filled" onClick={onClose} data-test="notice-done">Done</Button>}>
      {rows
        ? (
            <dl className="notice-body notice-grid">
              {rows.map((row, at) => (
                <div key={at}>
                  <dt>{row.label}</dt>
                  <dd>
                    <span className="notice-value">{row.value}</span>
                    {row.rest !== undefined && <><span aria-hidden="true"> · </span><span className="notice-rest" data-tooltip={row.rest}>{row.rest}</span></>}
                  </dd>
                </div>
              ))}
            </dl>
          )
        : <pre className="notice-body">{body}</pre>}
    </Modal>
  )
}
