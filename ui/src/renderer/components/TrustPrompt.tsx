import { Modal } from './Modal'
import { Alert, Button } from '../nala'

interface Props {
  directory: string
  /** Where remembering the answer would write it, or null where remembering is not offered (TRUST-23). */
  keeping: string | null
  onAnswer: (trusted: boolean, remember?: boolean) => void
}

/**
 * The one question the agent asks before it will work in a directory.
 *
 * Modal, and with no way past it but an answer. There is deliberately no default and no
 * dismiss: defaulting to trusted vouches for a directory on behalf of somebody who was
 * never asked, and the bridge refuses a turn until this is answered anyway.
 *
 * Remembering is offered only where the bridge said it can be kept, and it says what it covers
 * and where it goes, since nobody can endorse a record they were not shown.
 */
export function TrustPrompt({ directory, keeping, onAnswer }: Props): React.JSX.Element {
  return (
    <Modal title="Project trust" className="trust">
        <h2 id="trust-title">Do you trust this directory?</h2>
        <code className="path">{directory}</code>
        <p>
          <strong>Trust it</strong> and files here are read normally, so ordinary work
          proceeds without a prompt for every edit.
        </p>
        <p>
          <strong>Decline</strong> and nothing here is trusted. The agent can still work on
          these files, but it never reads them: they go to an isolated processor, and you
          see every change before it is applied.
        </p>
        {keeping && (
          <>
            <p>
              <strong>Trust and remember</strong> also skips this question in later sessions
              started in exactly this directory. A session started inside or above this
              directory is still asked, and so is one started in a directory deleted and made
              again here.
            </p>
            <p className="aside">
              Permissions takes it back, and it is written down here: <code>{keeping}</code>
            </p>
          </>
        )}
        <Alert type="info" className="trust-aside">
          Trusted writes may apply directly. Changes involving untrusted content require review. This conversation keeps your answer{keeping ? ', and later sessions keep it only if you remember it' : ''}.
        </Alert>
        <div className="trust-actions">
          <Button kind="outline" size="small" onClick={() => onAnswer(false)} data-test="trust-decline">
            Don't trust
          </Button>
          {keeping && (
            <Button kind="plain" size="small" onClick={() => onAnswer(true, true)} data-test="trust-remember">
              Trust and remember
            </Button>
          )}
          <Button kind="filled" size="small" onClick={() => onAnswer(true)} data-test="trust-approve">
            Trust this directory
          </Button>
        </div>
    </Modal>
  )
}
