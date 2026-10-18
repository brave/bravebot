import { PERMISSION_MODES, type PermissionMode } from '../../shared/protocol'
import { ModePicker, type ModeChoice } from './ModePicker'

/** As the Chat menu writes `mode.cycle`'s accelerator. */
const SHORTCUT = '⌘⇧M'

const MODES: Record<PermissionMode, ModeChoice> = {
  ask: {
    label: 'Ask',
    icon: 'hand-raised',
    says: 'Ask before every write and command',
  },
  acceptEdits: {
    label: 'Accept edits',
    icon: 'edit-pencil',
    says: 'Write files without asking. Commands and credential writes still ask',
  },
  plan: {
    label: 'Plan',
    icon: 'clipboard',
    says: 'Write nothing, only read and plan. Commands still ask',
  },
}

/**
 * How much the session asks before it acts.
 *
 * Asking is the quiet resting state, drawn like the model beside it. The other two are drawn
 * marked for as long as they hold, since each changes what the session does without a card to
 * say so.
 *
 * Not disabled while a turn runs. The running turn follows the mode chosen last (MODE-8), so a
 * change applies to it from its next question.
 */
export function PermissionModePicker({ mode, onChoose }: {
  mode: PermissionMode
  onChoose: (mode: PermissionMode) => void
}): React.JSX.Element {
  return (
    <ModePicker name="permission" testId="mode" noun="Permission mode" modes={MODES}
      choices={PERMISSION_MODES} mode={mode} shortcut={SHORTCUT} onChoose={onChoose} />
  )
}
