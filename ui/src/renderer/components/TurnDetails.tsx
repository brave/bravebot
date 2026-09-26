import { ChevronRightIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import type { TurnDetails as Details, TurnDisclosure } from '../turn-details'

export type OpenAudit = (turn: number | null, trigger: HTMLButtonElement) => void

export function TurnNotices({ details, onDisclosure }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
}): React.JSX.Element | null {
  if (!details?.notices.length) return null
  return (
    // A rule down the left rather than a box: this is said about the turn above it, and a
    // card would make it a third thing in the conversation.
    <Collapsible
      className="turn-notices mt-3 mb-4 border-l-2 border-border pl-3 text-xs text-muted-foreground"
      open={details.noticesOpen}
      onOpenChange={(open) => {
        if (open !== details.noticesOpen) onDisclosure(details.turn, 'noticesOpen', open)
      }}
    >
      {/* A finger is a blunter instrument than a pointer, so the row a touch has to find is
          given the height the platform asks for. */}
      <CollapsibleTrigger className="flex w-full cursor-pointer items-center gap-1 py-[5px] text-left pointer-coarse:min-h-11">
        <ChevronRightIcon className={cn('size-3! transition-transform motion-reduce:transition-none', details.noticesOpen && 'rotate-90')} />
        Turn notices · {details.notices.length}
      </CollapsibleTrigger>
      <CollapsibleContent keepMounted>
        <ul className="mt-1.5 list-disc pl-[18px]">
          {/* A notice is quoted, not paraphrased, so its own line breaks are kept and a long
              unbroken token is allowed to break anywhere rather than widen the column. */}
          {details.notices.map((notice, index) => <li key={index} className="my-1.5 whitespace-pre-wrap text-foreground wrap-anywhere">{notice}</li>)}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  )
}

const exact = (value?: number): string => value === undefined ? 'Unavailable' : value.toLocaleString()
const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

export function TurnFooter({ details, onDisclosure, onAudit }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  onAudit: OpenAudit
}): React.JSX.Element {
  return (
    <div className="turn-footer -mt-1 mb-[18px] flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs text-muted-foreground">
      {details?.status === 'complete' ? (
        <Collapsible
          className="turn-statistics max-w-full min-w-0"
          open={details.statsOpen}
          onOpenChange={(open) => {
            if (open !== details.statsOpen) onDisclosure(details.turn, 'statsOpen', open)
          }}
        >
          {/* A model's name has no spaces to break at and the footer is as narrow as the
              window is, so the summary is allowed to break mid-word rather than push the
              audit link off the end of the row. */}
          <CollapsibleTrigger className="flex cursor-pointer items-center gap-1 py-[5px] text-left wrap-anywhere pointer-coarse:min-h-11">
            <ChevronRightIcon className={cn('size-3! transition-transform motion-reduce:transition-none', details.statsOpen && 'rotate-90')} />
            {details.model ?? 'Model unavailable'} · {details.tokens === undefined ? 'Usage unavailable' : `${compact.format(details.tokens)} tokens`}
          </CollapsibleTrigger>
          <CollapsibleContent keepMounted>
            <div className="turn-statistics-body my-1.5 rounded-md bg-code px-3.5 py-3 text-foreground">
              <strong className="font-medium">Turn {details.turn}</strong>
              {/* Two equal columns with the figures against the right edge, so the numbers
                  line up as a column whatever the labels beside them are. */}
              <dl className="my-2.5 grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-x-[18px] gap-y-1.5 [&_dd]:m-0 [&_dd]:text-right [&_dd]:tabular-nums [&_dd]:wrap-anywhere [&_dt]:text-muted-foreground">
                <dt>Model used</dt><dd>{details.model ?? 'Unavailable'}</dd>
                <dt>Total tokens</dt><dd>{exact(details.tokens)}</dd>
                <dt>Output tokens</dt><dd>{exact(details.outputTokens)}</dd>
                <dt>Tool-calling rounds</dt><dd>{exact(details.steps)}</dd>
              </dl>
              <p className="mt-2 text-muted-foreground">Usage is summed across requests in this turn.</p>
            </div>
          </CollapsibleContent>
        </Collapsible>
      ) : details ? (
        <span>Final usage unavailable</span>
      ) : null}
      <Button
        variant="ghost"
        size="sm"
        // A word in a row of words rather than a button beside them: the footer is apparatus
        // under a reply, and a filled control down there would compete with the reply itself.
        // A refusal is the one thing here worth a colour, because it is the one thing that
        // happened rather than merely being available.
        className={cn(
          'turn-audit-link h-auto cursor-pointer px-0 py-[5px] text-left text-xs font-normal text-muted-foreground hover:bg-transparent hover:text-foreground hover:underline pointer-coarse:min-h-11',
          details?.clean === false && 'has-refusal text-warning',
        )}
        data-audit-turn={details?.turn ?? 'saved'}
        aria-controls="turn-audit-inspector"
        onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget)}
      >
        {details?.clean === false ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'}
        <span aria-hidden="true">↗</span>
      </Button>
    </div>
  )
}
