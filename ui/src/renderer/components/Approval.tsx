import { useContext, useId, useMemo, useRef, type RefObject } from 'react'
import { ShownContext, leftWords, mayAnswer, useShown, type OnScreen } from '../shown'
import { Button } from '../nala'

/**
 * The row a waiting question's answers sit in. It counts the rows the question marks as deciding
 * that have not been on screen yet, says how many are left, and the approving buttons in it wait
 * until none are (PROMPT-4). A refusal never waits.
 *
 * A card finds its rows from this row, as the `.confirm` that holds it. A question drawn anywhere
 * else, such as the body of a dialog, names the element holding its rows in `rows`.
 */
export function Answers({ rows, className = 'confirm-actions', children }: {
  rows?: RefObject<HTMLElement | null>
  className?: string
  children: React.ReactNode
}): React.JSX.Element {
  const row = useRef<HTMLDivElement>(null)
  const counted = useShown(row, rows)
  const note = useId()
  const shown = useMemo<OnScreen>(() => ({ ...counted, note }), [counted, note])
  const words = leftWords(shown)
  return (
    <div className={className} ref={row}>
      {words && <span className="shown-left" id={note}>{words}</span>}
      <ShownContext.Provider value={shown}>{children}</ShownContext.Provider>
    </div>
  )
}

/**
 * Whether an approving button is open, the note saying why not and its id, and `press`, which
 * measures once more before it answers, so a row scrolled away since the last count stops it.
 */
function useApproval(standing: boolean, onClick: () => void): {
  open: boolean
  words: string | undefined
  reason: string | undefined
  press: () => void
} {
  const shown = useContext(ShownContext)
  const open = mayAnswer(shown, standing)
  const words = open ? undefined : leftWords(shown) ?? undefined
  return { open, words, reason: words && shown.note, press: () => { if (open && shown.now(standing)) onClick() } }
}

/** An approving Leo button. `standing` is an answer that also covers later questions. */
export function Approve({ kind = 'filled', size = 'small', className = 'approve', standing = false, tooltip, onClick, children, 'data-test': dataTest }: {
  kind?: 'filled' | 'outline'
  size?: 'small' | 'medium'
  className?: string
  standing?: boolean
  tooltip?: string
  onClick: () => void
  children: React.ReactNode
  'data-test'?: string
}): React.JSX.Element {
  const { open, words, press } = useApproval(standing, onClick)
  // The note's id would not resolve inside the button's shadow root, so its words go across instead.
  return (
    <Button kind={kind} size={size} className={className} data-tooltip={tooltip} isDisabled={!open} aria-disabled={!open}
      aria-description={words} onClick={press} data-test={dataTest}>
      {children}
    </Button>
  )
}
