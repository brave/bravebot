import { memo, useState, type ReactNode } from 'react'
import { Icon } from '../nala'
import { IconButton } from './IconButton'

/**
 * A one-time notice in the composer tray: a short sentence with a More/Less toggle that swaps in the
 * long text, then any actions and a dismiss button. `prefix` names its class, the id the toggle
 * controls and every `data-test` (`<prefix>`, `<prefix>-more`, `<prefix>-dismiss`), so drivers match
 * each notice by the name it had before it shared this shell.
 */
export const ComposerNotice = memo(function ComposerNotice({ prefix, short, long, more, less, dismissLabel, onDismiss, actions, expanded: startExpanded = false }: {
  prefix: string
  short: string
  long: string
  more: string
  less: string
  dismissLabel: string
  onDismiss: () => void
  actions?: ReactNode
  expanded?: boolean
}): React.JSX.Element {
  const [expanded, setExpanded] = useState(startExpanded)
  return (
    <div className={`composer-tray composer-notice ${prefix}${expanded ? ' expanded' : ''}`} role="status" data-test={prefix}>
      <Icon name="info-outline" />
      <span className="notice-text" id={`${prefix}-text`}>
        {expanded ? long : short}{' '}
        {/* A bare button rather than a Leo one, so it sits in the sentence without raising the line. */}
        <button
          type="button"
          className="notice-more"
          aria-expanded={expanded}
          aria-controls={`${prefix}-text`}
          onClick={() => setExpanded((open) => !open)}
          data-test={`${prefix}-more`}
        >
          {expanded ? less : more}
        </button>
      </span>
      <span className="notice-actions">
        {actions}
        <IconButton icon="close" size="tiny" label={dismissLabel} tooltip={dismissLabel} onClick={onDismiss} data-test={`${prefix}-dismiss`} />
      </span>
    </div>
  )
})
