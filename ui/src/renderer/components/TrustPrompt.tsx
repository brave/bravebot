import { useRef } from 'react'
import { Modal } from './Modal'
import { Answers, Approve } from './Approval'
import { Alert, Button, Icon } from '../nala'

interface Props {
  /** The directory the question is about, spelled for a person (PROMPT-1). */
  directory: string
  /** The file a yes would be kept in, spelled for a person, or null where the answer cannot be kept (TRUST-23). */
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
 *
 * The yes takes no press until the directory, each of the rows saying what an answer does and the
 * file a yes is kept in have been on screen (PROMPT-4, TRUST-23), as an approval on a card does.
 * The directory is a box that scrolls inside itself, so a long path cannot push the rest out of
 * the dialog. The no answers at every draw.
 */
export function TrustPrompt({ directory, keeping, confinedLines, onAnswer }: Props): React.JSX.Element {
  const rows = useRef<HTMLDivElement>(null)
  return (
    <Modal title="Project trust" size="sm" className="trust" subtitle="Do you trust this directory?" subtitleId="trust-title" actions={
      <Answers rows={rows} className="trust-answers">
        <Button kind="plain-faint" onClick={() => onAnswer(false)} data-test="trust-decline">
          Don't trust
        </Button>
        <Approve size="medium" onClick={() => onAnswer(true)} data-test="trust-approve">
          {keeping ? 'Trust and remember' : 'Trust for this session'}
        </Approve>
      </Answers>
    }>
      <div ref={rows} className="trust-rows">
        <code className="path" tabIndex={0} aria-label="Directory" data-deciding="all" data-test="trust-path">{directory}</code>
        <ul className="trust-options">
          <li data-deciding="all">
            <Icon name="shield-done" />
            <p><strong>Trust it</strong> and files here are read normally, so ordinary work
              proceeds without a prompt for every edit.</p>
          </li>
          {confinedLines && (
            <li data-test="trust-confined-lines" data-deciding="all">
              <Icon name="shield-done" />
              <p>{confinedLines}</p>
            </li>
          )}
          <li data-deciding="all">
            <Icon name="eye-off" />
            <p><strong>Decline</strong> and nothing here is trusted. The agent can still work on
              these files, but it never reads them: they go to an isolated processor, and you
              see every change before it is applied. You are asked again the next time a chat
              starts here.</p>
          </li>
          <li data-deciding="all">
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
          <p className="aside" data-deciding="all">
            Permissions takes it back, and it is written down here: <code>{keeping}</code>
          </p>
        )}
        <Alert type="info" size="small" className="trust-aside">
          Trusted writes may apply directly. Changes involving untrusted content require review.
        </Alert>
      </div>
    </Modal>
  )
}
