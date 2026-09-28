/**
 * The mark for a session that came out of another one.
 *
 * Leo's fork mark rather than a typed character: the obvious glyph, `⑂`, is a hairline at the
 * sizes it would be used at and reads as a smudge or a lowercase `y` — and this appears in three
 * places (the control a prompt offers, the banner, the session list), so whatever it is has to
 * survive being small.
 *
 * Always `aria-hidden`: every one of the three places says in words what it means, and a mark
 * that announced itself as well would say everything twice.
 */
import { Icon } from '../nala'

export function ForkIcon({ size = 12 }: { size?: number }): React.JSX.Element {
  return (
    <Icon
      className="fork-icon"
      name="fork-arrows"
      style={{ '--leo-icon-size': `${size}px` } as React.CSSProperties}
      aria-hidden="true"
    />
  )
}
