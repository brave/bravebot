import { useState } from 'react'
import { CircleAlertIcon, ChevronRightIcon } from 'lucide-react'
import { failureSummary } from '../failure'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'

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
  const [detailsOpen, setDetailsOpen] = useState(false)
  const technical = [
    detail.trim(),
    attempts != null ? `Requests attempted: ${attempts}` : '',
    status != null ? `HTTP status: ${status}` : '',
  ].filter(Boolean).join('\n')
  return (
    <Alert variant="destructive" className="error-card" role="alert">
      <CircleAlertIcon />
      <AlertTitle><strong>{classified.title}</strong></AlertTitle>
      <AlertDescription>
        <p>{classified.description}</p>
        {(onRetry || onModel) && (
          <div className="error-actions mt-2.5 flex flex-wrap gap-2">
            {onRetry && <Button variant="outline" size="sm" onClick={onRetry}>Draft continuation</Button>}
            {onModel && <Button variant="outline" size="sm" onClick={onModel}>Choose another model</Button>}
          </div>
        )}
        {!!technical && (
          <div className="mt-2.5 text-xs">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-auto gap-1 px-0 py-0 text-xs font-normal text-inherit hover:bg-transparent hover:text-inherit"
              aria-expanded={detailsOpen}
              onClick={() => setDetailsOpen((open) => !open)}
            >
              <ChevronRightIcon
                className={cn(
                  'size-3! transition-transform motion-reduce:transition-none',
                  detailsOpen && 'rotate-90',
                )}
              />
              Technical details
            </Button>
            {/* Kept mounted while shut so the diagnostic stays in the document for copy and for
                the marking tests; capped and scrollable so a long backend log cannot bury the
                conversation it interrupted. */}
            <pre
              hidden={!detailsOpen}
              className="mt-1.5 max-h-50 overflow-auto text-xs whitespace-pre-wrap wrap-anywhere"
            >
              {technical}
            </pre>
          </div>
        )}
      </AlertDescription>
    </Alert>
  )
}
