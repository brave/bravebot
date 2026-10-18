import { Modal } from './Modal'
import { Alert, Button, Icon } from '../nala'

interface Props {
  directory: string
  /** The file a yes would be kept in, or null where the answer cannot be kept (TRUST-23). */
  keeping: string | null
  /** What a yes also grants beyond the files (RUN-5), in the bridge's words, or null where the question says nothing of it. */
  confinedLines: string | null
  onAnswer: (trusted: boolean) => void
}

/**
 * The one question the agent asks before it will work in a directory.
 *
 * Modal, and with no way past it but an answer. There is deliberately no default and no
 * dismiss: defaulting to trusted vouches for a directory on behalf of somebody who was
 * never asked, and the bridge refuses a turn until this is answered anyway.
 *
 * A yes is kept for later sessions wherever the bridge said it can be, and the question says what
 * that covers and where it is written, since nobody can endorse a record they were not shown. A no
 * is never kept.
 */
export function TrustPrompt({ directory, keeping, confinedLines, onAnswer }: Props): React.JSX.Element {
  return (
    <Modal title="Project trust" size="sm" className="trust" subtitle="Do you trust this directory?" subtitleId="trust-title" actions={<>
      <Button kind="plain-faint" className="modal-leading" onClick={() => onAnswer(false)} data-test="trust-decline">
        Don't trust
      </Button>
      <Button kind="filled" onClick={() => onAnswer(true)} data-test="trust-approve">
        {keeping ? 'Trust and remember' : 'Trust for this session'}
      </Button>
    </>}>
      <code className="path">{directory}</code>
      <ul className="trust-options">
        <li>
          <Icon name="shield-done" />
          <p><strong>Trust it</strong> and files here are read normally, so ordinary work
            proceeds without a prompt for every edit.</p>
        </li>
        {confinedLines && (
          <li data-test="trust-confined-lines">
            <Icon name="shield-done" />
            <p>{confinedLines}</p>
          </li>
        )}
        <li>
          <Icon name="eye-off" />
          <p><strong>Decline</strong> and nothing here is trusted. The agent can still work on
            these files, but it never reads them: they go to an isolated processor, and you
            see every change before it is applied. You are asked again the next time a chat
            starts here.</p>
        </li>
        <li>
          <Icon name="pin" />
          {keeping ? (
            <p><strong>A yes is kept</strong>, so later chats started in this directory, or below
              it where this directory is the root of a git repository, are not asked. A chat
              started above this directory is still asked, and so is one started below it where
              this is no repository root, in a repository nested inside it, or in a directory
              deleted and made again here.</p>
          ) : (
            <p><strong>A yes is not kept here</strong>, so the next chat started in this
              directory asks again.</p>
          )}
        </li>
      </ul>
      {keeping && (
        <p className="aside">
          Permissions takes it back, and it is written down here: <code>{keeping}</code>
        </p>
      )}
      <Alert type="info" size="small" className="trust-aside">
        Trusted writes may apply directly. Changes involving untrusted content require review.
      </Alert>
    </Modal>
  )
}
