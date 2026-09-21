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
 * Whoever opens and closes it owns `open`; this knows nothing about why.
 */
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
    <div className={`fold ${open ? 'open' : ''}`}>
      <div className="fold-clip">
        <div className={className}>{children}</div>
      </div>
    </div>
  )
}
