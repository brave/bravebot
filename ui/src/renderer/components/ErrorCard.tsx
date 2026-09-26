import { ChevronRightIcon } from 'lucide-react'
import { failureSummary } from '../failure'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'

/**
 * A failure, titled from the category the agent sent and never from the words in the detail.
 *
 * The detail is prose: a backend's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. Matching on it is what the wire protocol forbids in as many words,
 * and it is why every enum here arrives with a tag. A failure that carried no category gets
 * `failureSummary`'s unknown-category answer, which is the same one an unrecognised tag gets.
 */
export function ErrorCard({ detail, onRetry, onModel, category, attempts, status }: {
  category?: string | null; attempts?: number | null; status?: number | null
  detail: string; onRetry?: () => void; onModel?: () => void
}): React.JSX.Element {
  const classified = failureSummary(category ?? '')
  return (
    // The left edge carries the failure on its own, so the card reads as one at a glance and
    // without relying on the tint alone — which on a light theme is nearly the page.
    <Alert
      variant="destructive"
      className="error-card rounded-[10px] border-[color-mix(in_srgb,var(--destructive)_25%,var(--border))] border-l-[3px] border-l-destructive bg-destructive/10 px-4 py-3.5 text-[13px]"
      role="alert"
    >
      <AlertTitle>
        <strong>{classified.title}</strong>
      </AlertTitle>
      <AlertDescription className="[&_p:not(:last-child)]:mt-1.5 [&_p:not(:last-child)]:mb-2.5">
        <p className="leading-normal">{classified.description}</p>
        <div className="error-actions flex flex-wrap gap-2">
          {onRetry && <Button variant="outline" size="sm" onClick={onRetry}>Draft continuation</Button>}
          {onModel && <Button variant="outline" size="sm" onClick={onModel}>Choose another model</Button>}
        </div>
        <Collapsible className="mt-2.5 text-xs group/details">
          <CollapsibleTrigger className="flex items-center gap-1 text-left">
            <ChevronRightIcon className="size-3! transition-transform motion-reduce:transition-none group-data-open/details:rotate-90" />
            Technical details
          </CollapsibleTrigger>
          <CollapsibleContent>
            {/* Capped and scrollable. A backend's diagnostic can be a page long, and a card
                that grew to fit one would bury the conversation it interrupted. */}
            <pre className="max-h-50 overflow-auto text-xs whitespace-pre-wrap wrap-anywhere">
              {detail}
              {attempts != null ? `\nRequests attempted: ${attempts}` : ''}
              {status != null ? `\nHTTP status: ${status}` : ''}
            </pre>
          </CollapsibleContent>
        </Collapsible>
      </AlertDescription>
    </Alert>
  )
}
