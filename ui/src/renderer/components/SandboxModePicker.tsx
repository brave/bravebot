import { SANDBOX_MODES, type SandboxMode } from '../../shared/protocol'
import { ModePicker, type ModeChoice } from './ModePicker'

const MODES: Record<SandboxMode, ModeChoice> = {
  'workspace-write': {
    label: 'Workspace write',
    icon: 'shield',
    says: 'Programs can read the machine except credential locations, and write only in the session',
  },
  'read-only': {
    label: 'Read only',
    icon: 'eye-on',
    says: 'Programs can read the machine except credential locations, and write nothing in the session',
  },
  strict: {
    label: 'Strict',
    icon: 'shield-done',
    says: 'Programs read only what their command names. A program that needs more fails',
  },
}

/**
 * What a program the agent runs is held to.
 *
 * Workspace write is the quiet resting state. Read only and Strict are drawn by name for as long as they
 * hold. There is no entry for turning the sandbox off: this window offers none, and the bridge refuses
 * it.
 *
 * A change applies from the session's next turn, so it is not disabled while a turn runs: the
 * turn on screen keeps the mode it began with.
 */
export function SandboxModePicker({ mode, onChoose }: {
  mode: SandboxMode
  onChoose: (mode: SandboxMode) => void
}): React.JSX.Element {
  return (
    <ModePicker name="sandbox" testId="sandbox" noun="Sandbox mode" modes={MODES}
      choices={SANDBOX_MODES} mode={mode} onChoose={onChoose} />
  )
}
