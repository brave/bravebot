import { useEffect, useRef, useState } from 'react'
import type { SessionVariant } from '../../shared/protocol'
import { Button, ButtonMenu, Icon } from '../nala'
import { useFocusReturn, useMenuItemRoles } from '../hooks'
import { call } from '../rpc'

/** What the trigger says: the pick, or that the model's own options are sent. */
export function variantLabel({ variant }: SessionVariant): string {
  return variant ?? 'Default options'
}

/** The sentence the tooltip and the accessible name give. */
export function variantSentence({ model, variant }: SessionVariant): string {
  return variant
    ? `Options variant: ${variant}. Its options are added to ${model}'s from the next turn`
    : `Options variant: off. ${model} is sent its default options`
}

/**
 * The model's named option variant (BACKEND-66), in the composer toolbar beside the model picker.
 *
 * What it lists is what `session.variant` answers for the model the composer has chosen, read
 * again when the session, the model or the running flag changes. A model that offers no variants
 * shows nothing. A pick applies from the next turn, so the control is not disabled while one runs.
 * What the bridge refuses is said beside the control until the next answer.
 */
export function VariantPicker({ session, model, running }: {
  session: string
  model: string | null
  running: boolean
}): React.JSX.Element | null {
  const [read, setRead] = useState<SessionVariant | null>(null)
  const [refused, setRefused] = useState<string | null>(null)

  useEffect(() => {
    let current = true
    call<SessionVariant>('session.variant', { session, model })
      .then((answer) => { if (current) { setRead(answer); setRefused(null) } })
      // A session the bridge does not hold yet (a bot's first page) has no variant to pick.
      .catch(() => { if (current) setRead(null) })
    return () => { current = false }
  }, [session, model, running])

  const pick = async (variant: string | null) => {
    try {
      setRead(await call<SessionVariant>('session.variant', { session, model, variant }))
      setRefused(null)
    } catch (error) {
      setRefused(error instanceof Error ? error.message : String(error))
    }
  }

  if (!read) return null
  return <VariantMenu read={read} refused={refused} onPick={(variant) => void pick(variant)} />
}

/** The trigger and the menu for what `session.variant` answered; nothing when no name is offered. */
export function VariantMenu({ read, refused, onPick }: {
  read: SessionVariant
  refused: string | null
  onPick: (variant: string | null) => void
}): React.JSX.Element | null {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const { onClose, restoreFocus } = useFocusReturn(trigger)
  const label = variantLabel(read)
  const sentence = variantSentence(read)

  // Leo's React wrapper sets these once, when the host is made.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', sentence)
    button.setAttribute('aria-expanded', String(open))
    button.setAttribute('data-tooltip', sentence)
    button.setAttribute('data-variant', read.variant ?? 'off')
  }, [sentence, open, read.variant])

  // Leo stamps `role="menuitem"` on every item when the menu opens. These are one radio group.
  useMenuItemRoles(trigger)

  if (read.offered.length === 0) return null

  const row = (name: string | null) => {
    const checked = name === read.variant
    return (
      <leo-menu-item key={name ?? ''} data-role="menuitemradio" aria-checked={checked ? 'true' : 'false'}
        data-test={`variant-option-${name ?? 'off'}`}
        onClick={() => { if (!checked) onPick(name) }}>
        <span className="menu-icon-row">
          <span className="menu-text">
            <span className="menu-title">{name ?? 'Off'}</span>
            {name === null && <span className="menu-subtitle">The model’s default options</span>}
          </span>
          <span className="menu-check" aria-hidden="true">{checked && <Icon name="check-normal" />}</span>
        </span>
      </leo-menu-item>
    )
  }

  return (
    <span className="variant-picker">
      <ButtonMenu className="variant-menu" isOpen={open} placement="top-end" positionStrategy="fixed"
        onClose={onClose}
        onChange={({ isOpen }) => { setOpen(isOpen); if (!isOpen) restoreFocus() }}>
        <Button ref={trigger} slot="anchor-content" kind="plain-faint" size="tiny" className="variant-trigger"
          aria-label={sentence} aria-haspopup="menu" aria-expanded={open} data-tooltip={sentence}
          data-test="variant-trigger" data-variant={read.variant ?? 'off'}>
          <Icon name="tune" slot="icon-before" />
          <span className="variant-current">{label}</span>
          <Icon name="carat-down" slot="icon-after" />
        </Button>
        {read.offered.map((name) => row(name))}
        {row(null)}
      </ButtonMenu>
      {refused && <span className="variant-refused" role="alert" data-test="variant-refused">{refused}</span>}
    </span>
  )
}
