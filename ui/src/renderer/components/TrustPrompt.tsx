import { cn, SCRIM } from '@/lib/utils'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'

interface Props {
  directory: string
  onAnswer: (trusted: boolean) => void
}

/** The box both answers are drawn in, so the pair differ only where they are meant to. */
const ANSWER =
  'min-h-8 rounded-[7px] border px-3.5 py-1.5 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary'

/**
 * The one question the agent asks before it will work in a directory.
 *
 * Modal, and with no way past it but an answer. There is deliberately no default and no
 * dismiss: defaulting to trusted vouches for a directory on behalf of somebody who was
 * never asked, and the bridge refuses a turn until this is answered anyway.
 */
export function TrustPrompt({ directory, onAnswer }: Props): React.JSX.Element {
  return (
    <Dialog open disablePointerDismissal onOpenChange={() => { /* answer required */ }}>
      {/* The card's own rhythm is in the margins below rather than in a gap on the grid: the
          path sits closer to the question than the paragraphs do to each other. */}
      <DialogContent
        className="modal trust w-[460px] gap-0 sm:max-w-none"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogTitle className="sr-only">Project trust</DialogTitle>
        <h2 id="trust-title" className="m-0 mb-2.5 text-[15px] font-semibold">Do you trust this directory?</h2>
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
        <p className="aside m-0 mb-[9px] text-[11px] text-muted-foreground/70">
          Trusted writes may apply directly. Changes involving untrusted content require review. Your trust choice is saved with this conversation.
        </p>
        <div className="trust-actions mt-4 flex justify-end gap-2">
          <button className={cn(ANSWER, 'decline border-border bg-transparent hover:bg-destructive/10 hover:text-destructive')} onClick={() => onAnswer(false)}>
            Don't trust
          </button>
          {/* The two answers are not drawn alike. Trusting is the one that grants something, so
              it is the filled one; declining leaves the directory as the agent already found it. */}
          <button className={cn(ANSWER, 'approve border-transparent bg-primary font-medium text-primary-foreground')} onClick={() => onAnswer(true)}>
            Trust this directory
          </button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
