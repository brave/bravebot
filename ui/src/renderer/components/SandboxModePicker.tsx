import { SANDBOX_MODES, type SandboxMode } from '../../shared/protocol'
import { ModePicker, type ModeChoice } from './ModePicker'

const MODES: Record<SandboxMode, ModeChoice> = {
  standard: {
    label: 'Standard',
    icon: 'shield',
    says: 'Programs can read the machine except credential locations, and write only in the session',
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
 * Standard is the quiet resting state. Strict is drawn marked for as long as it holds. There is
 * no entry for turning the sandbox off: this window offers none, and the bridge refuses it.
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
