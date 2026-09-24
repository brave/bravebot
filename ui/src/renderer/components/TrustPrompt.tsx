import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from './ui/alert-dialog'
interface Props {
  directory: string
  onAnswer: (trusted: boolean) => void
}

/**
 * The one question the agent asks before it will work in a directory.
 *
 * Modal, and with no way past it but an answer. There is deliberately no default and no
 * dismiss: defaulting to trusted vouches for a directory on behalf of somebody who was
 * never asked, and the bridge refuses a turn until this is answered anyway.
 */
export function TrustPrompt({ directory, onAnswer }: Props): React.JSX.Element {
  return (
    <AlertDialog open>
      <AlertDialogContent className="modal trust" overlayClassName="scrim">
        <AlertDialogHeader>
          <AlertDialogTitle id="trust-title">Do you trust this directory?</AlertDialogTitle>
          <AlertDialogDescription>Choose how Brave Bot may handle files in this project.</AlertDialogDescription>
        </AlertDialogHeader>
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
        <p className="aside">
          Trusted writes may apply directly. Changes involving untrusted content require review. Your trust choice is saved with this conversation.
        </p>
        <AlertDialogFooter className="trust-actions">
          <AlertDialogCancel className="decline" onClick={() => onAnswer(false)}>
            Don't trust
          </AlertDialogCancel>
          <AlertDialogAction className="approve" onClick={() => onAnswer(true)}>
            Trust this directory
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
