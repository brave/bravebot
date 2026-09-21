/**
 * The mark for a session that came out of another one.
 *
 * Drawn rather than typed. The obvious character for this, `⑂`, is a hairline at the sizes it
 * would be used at and reads as a smudge or a lowercase `y` — and this appears in three places
 * (the control a prompt offers, the banner, the session list), so whatever it is has to survive
 * being small. A path at `currentColor` inherits whatever the thing around it is doing.
 *
 * Always `aria-hidden`: every one of the three places says in words what it means, and a mark
 * that announced itself as well would say everything twice.
 */
export function ForkIcon({ size = 12 }: { size?: number }): React.JSX.Element {
  return (
    <svg
      className="fork-icon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {/* A trunk that splits in two. The asymmetric version — a line carrying on with another
          leaving it — is the truer picture of what a fork does, and at 12px it draws a
          lowercase `r`. This one is unmistakably two paths out of one. */}
      <path d="M8 14V9" />
      <path d="M8 9 3.5 4.5" />
      <path d="M8 9l4.5-4.5" />
    </svg>
  )
}
