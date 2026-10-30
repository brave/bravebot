import { Modal } from './Modal'
import { Button } from '../nala'

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

export function Notice({ title, subtitle, body, onClose }: { title: string; subtitle?: string; body: string; onClose: () => void }): React.JSX.Element {
  return (
    <Modal title={title} subtitle={subtitle} size="md" onClose={onClose} className="notice"
      actions={<Button kind="filled" onClick={onClose} data-test="notice-done">Done</Button>}>
      <pre className="notice-body">{body}</pre>
    </Modal>
  )
}
