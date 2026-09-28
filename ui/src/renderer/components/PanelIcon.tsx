/**
 * The marks for the panels in the context column.
 *
 * Leo icons rather than hand-drawn paths: five of them sit in one row of connected buttons,
 * and they have to be told apart at a glance and at twelve pixels — which is why each is one
 * idea (a list, a page, a pencil, a lock, a folder) and none is a scene.
 *
 * Always `aria-hidden`: every button carries the panel's name in its `title` and its accessible
 * label, and a mark that announced itself as well would say it twice.
 */

import type { PanelName } from '../../shared/state'
import { Icon, type IconName } from '../nala'

const NAMES: Record<PanelName, IconName> = {
  plan: 'list-checks',
  read: 'file-code',
  writes: 'edit-pencil',
  confined: 'lock',
  files: 'folder-open',
}

export function PanelIcon({
  panel,
  size = 13,
}: {
  panel: PanelName
  size?: number
}): React.JSX.Element {
  return (
    <Icon
      className="panel-icon"
      name={NAMES[panel]}
      style={{ '--leo-icon-size': `${size}px` } as React.CSSProperties}
      aria-hidden="true"
    />
  )
}
