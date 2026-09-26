import { ChevronRightIcon } from 'lucide-react'
import { SCRIM } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

export function Unconfigured({ detail, onClose }: { detail: string; onClose: () => void }): React.JSX.Element {
  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      {/* Narrower than the other dialogs share, and deliberately: this is three short paragraphs
          somebody reads once, not a form or a diff. The padding is the one every dialog keeps. */}
      <DialogContent className="modal max-w-lg p-7" showCloseButton={false} overlayClassName={SCRIM}>
        <DialogHeader>
          <DialogTitle>Connect the agent backend</DialogTitle>
        </DialogHeader>
        {/* A measure rather than the width of the card: these are sentences somebody reads once,
            and a line that runs the full width of a dialog is a line the eye loses its place in. */}
        <div className="flex flex-col gap-3 [&_code]:rounded-[4px] [&_code]:bg-bubble-agent [&_code]:px-[5px] [&_code]:py-px [&_code]:font-mono [&_code]:text-[11px] [&_p]:max-w-[60ch] [&_p]:text-muted-foreground">
          <p>This build cannot load its backend credentials. You can continue browsing conversations and writing drafts.</p>
          <p>If you installed Brave Bot, obtain a configured build from its distributor. Credentials are currently provided when the agent is built.</p>
          <Collapsible className="group/setup mt-[18px] text-[11px] text-muted-foreground/70">
            <CollapsibleTrigger className="flex items-center gap-1 text-left">
              <ChevronRightIcon className="size-3! transition-transform motion-reduce:transition-none group-data-open/setup:rotate-90" />
              Development setup
            </CollapsibleTrigger>
            <CollapsibleContent>
              <ol className="flex max-w-[60ch] list-decimal flex-col gap-2 pl-5 text-muted-foreground">
                <li>Provide the backend credentials through the project’s approved environment configuration.</li>
                <li>Run <code>npm run bridge</code> from the interface checkout.</li>
                <li>Restart Brave Bot, then use <strong>Check again</strong>.</li>
              </ol>
              <p className="mt-2">See <code>docs/setup.md</code> for the configuration layout. Keep credentials outside the repository.</p>
            </CollapsibleContent>
          </Collapsible>
          <Collapsible className="group/tech mt-[18px] text-[11px] text-muted-foreground/70">
            <CollapsibleTrigger className="flex items-center gap-1 text-left">
              <ChevronRightIcon className="size-3! transition-transform motion-reduce:transition-none group-data-open/tech:rotate-90" />
              Technical details
            </CollapsibleTrigger>
            <CollapsibleContent>
              <pre className="overflow-auto rounded-sm bg-bubble-agent px-2.5 py-2 font-mono text-xs whitespace-pre-wrap">{detail}</pre>
            </CollapsibleContent>
          </Collapsible>
        </div>
        <DialogFooter className="mx-0 mb-0 rounded-none border-0 bg-transparent p-0 sm:justify-end">
          <Button onClick={onClose}>Continue browsing</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
