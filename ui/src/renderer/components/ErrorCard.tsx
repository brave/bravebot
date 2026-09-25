import { cn } from 'cn'
import { failureSummary } from '../failure'
import { Alert, AlertDescription, AlertTitle } from './ui/alert'
import { Button } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'

/**
 * A failure, titled from the category the agent sent and never from the words in the detail.
 *
 * The detail is prose: a backend's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. Matching on it is what the wire protocol forbids in as many words,
 * and it is why every enum here arrives with a tag. A failure that carried no category gets
 * `failureSummary`'s unknown-category answer, which is the same one an unrecognised tag gets.
 */
export function ErrorCard({ detail, onRetry, onModel, category, attempts, status, className }: {
  category?: string | null; attempts?: number | null; status?: number | null
  detail: string; onRetry?: () => void; onModel?: () => void; className?: string
}): React.JSX.Element {
  const classified = failureSummary(category ?? '')
  return (
    <Alert variant="destructive" className={cn('error-card mx-5 mt-2.5 border-l-[3px] border-l-destructive bg-destructive/10 text-foreground', className)}>
      <AlertTitle><strong>{classified.title}</strong></AlertTitle>
      <AlertDescription><p>{classified.description}</p></AlertDescription>
      {(onRetry || onModel) && (onRetry && onModel ? <ButtonGroup className="error-actions mt-2.5">
        <Button variant="outline" size="sm" onClick={onRetry}>Draft continuation</Button>
        <Button variant="outline" size="sm" onClick={onModel}>Choose another model</Button>
      </ButtonGroup> : <div className="error-actions mt-2.5">
        {onRetry && <Button variant="outline" size="sm" onClick={onRetry}>Draft continuation</Button>}
        {onModel && <Button variant="outline" size="sm" onClick={onModel}>Choose another model</Button>}
      </div>)}
      <Collapsible className="error-details mt-2.5">
        <CollapsibleTrigger asChild><Button variant="ghost" size="sm">Technical details</Button></CollapsibleTrigger>
        <CollapsibleContent forceMount><pre className="max-h-[200px] overflow-auto whitespace-pre-wrap break-words rounded-md border border-border bg-muted p-2.5 font-mono text-xs">{detail}{attempts != null ? `\nRequests attempted: ${attempts}` : ''}{status != null ? `\nHTTP status: ${status}` : ''}</pre></CollapsibleContent>
      </Collapsible>
    </Alert>
  )
}
