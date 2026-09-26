/**
 * Something that opens and closes vertically.
 *
 * The height is animated as a grid track from `0fr` to `1fr`, which lets the browser
 * interpolate to the content's own height without anyone measuring it first: a pixel
 * height would have to be read back on every change to what is inside, and would be wrong
 * for the frame after that content grew.
 *
 * The children stay mounted while closed, so a collapse has something to animate away
 * from — unmounting them made the close a snap under an open that was not. They are hidden
 * from the reader and from the tab order instead, in CSS, once the fold has finished.
 *
 * Whoever opens and closes it owns `open`; this knows nothing about why. Collapsible is
 * available elsewhere for disclosure controls that do not need this track animation; the
 * driver scripts assert intermediate heights, which only the `0fr`/`1fr` grid can provide.
 */
import { cn } from '@/lib/utils'

export function Fold({
  open,
  className,
  children,
}: {
  open: boolean
  /** Where to put the padding. It cannot go on the clip, which would keep a closed fold as
   *  tall as its own padding. */
  className?: string
  children: React.ReactNode
}): React.JSX.Element {
  return (
    <div
      className={cn(
        'fold grid transition-[grid-template-rows] duration-[var(--panel-duration)] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
        open ? 'open grid-rows-[1fr]' : 'grid-rows-[0fr]',
      )}
    >
      <div
        className={cn(
          'fold-clip min-h-0 overflow-hidden',
          // A closed fold should not be tabbable or read out, and clipping alone does neither.
          // Hiding waits for the fold to finish so there is something to watch on the way
          // down; on the way back it lifts at once.
          open
            ? 'visible'
            : 'invisible [transition:visibility_0s_linear_var(--panel-duration)]',
          'motion-reduce:[transition:none]',
        )}
      >
        <div className={className}>{children}</div>
      </div>
    </div>
  )
}
