import { SCRIM } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

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
    <Dialog open disablePointerDismissal onOpenChange={() => { /* answer required */ }}>
      {/* The card's own rhythm is in the margins below rather than in a gap on the grid: the
          path sits closer to the question than the paragraphs do to each other. */}
      <DialogContent
        className="modal trust w-[460px] gap-0 sm:max-w-none"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogHeader className="gap-0 text-left">
          {/* The window's name for this question stays "Project trust". The sentence underneath
              is what the person is actually asked, and it is not the dialog's accessible name. */}
          <DialogTitle className="sr-only">Project trust</DialogTitle>
          <h2 id="trust-title" className="mb-2.5 text-[15px] font-semibold">
            Do you trust this directory?
          </h2>
        </DialogHeader>
        <code className="path mb-3 block rounded-sm bg-bubble-agent px-2.5 py-[7px] font-mono text-[11px] break-all">{directory}</code>
        <p className="m-0 mb-[9px] text-xs text-muted-foreground">
          <strong>Trust it</strong> and files here are read normally, so ordinary work
          proceeds without a prompt for every edit.
        </p>
        <p className="m-0 mb-[9px] text-xs text-muted-foreground">
          <strong>Decline</strong> and nothing here is trusted. The agent can still work on
          these files, but it never reads them: they go to an isolated processor, and you
          see every change before it is applied.
        </p>
        {keeping && (
          <>
            <p className="m-0 mb-[9px] text-xs text-muted-foreground">
              <strong>Trust and remember</strong> also skips this question in later sessions
              started in exactly this directory. A session started inside or above this
              directory is still asked, and so is one started in a directory deleted and made
              again here.
            </p>
            <p className="aside m-0 mb-[9px] text-[11px] text-muted-foreground/70">
              Permissions takes it back, and it is written down here: <code className="font-mono break-all">{keeping}</code>
            </p>
          </>
        )}
        <p className="aside m-0 mb-[9px] text-[11px] text-muted-foreground/70">
          Trusted writes may apply directly. Changes involving untrusted content require review. This conversation keeps your answer{keeping ? ', and later sessions keep it only if you remember it' : ''}.
        </p>
        <DialogFooter className="trust-actions -mx-0 -mb-0 mt-4 justify-end gap-2 rounded-none border-0 bg-transparent p-0 sm:justify-end">
          <Button
            variant="outline"
            className="decline"
            onClick={() => onAnswer(false)}
          >
            Don't trust
          </Button>
          {keeping && (
            <Button variant="outline" onClick={() => onAnswer(true, true)}>
              Trust and remember
            </Button>
          )}
          {/* Trusting is the one that grants something, so it is the filled one; declining
              leaves the directory as the agent already found it. */}
          <Button
            className="approve"
            onClick={() => onAnswer(true)}
          >
            Trust this directory
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
