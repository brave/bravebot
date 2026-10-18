import { EFFORTS, EFFORT_HINTS, EFFORT_LABELS, type Effort } from '../../shared/effort'
import { ModePicker, type ModeChoice } from './ModePicker'

type Choice = Effort | 'default'

const CHOICES: readonly Choice[] = ['default', ...EFFORTS]

const LEVELS = Object.fromEntries(CHOICES.map((choice) => [choice, {
  label: EFFORT_LABELS[choice],
  icon: 'head-idea',
  says: EFFORT_HINTS[choice],
}])) as Record<Choice, ModeChoice>

/**
 * How hard the model thinks, kept per conversation like the model beside it.
 *
 * Default sends no level, which leaves it to settings and then the service. A model the catalogue
 * says ignores the parameter gets a grey trigger: a level would be sent and have no effect.
 */
export function EffortPicker({ effort, readsEffort, disabled, onChoose }: {
  effort: Effort | null
  /** `false` only when the catalogue says so; an unknown model is offered the menu. */
  readsEffort: boolean | null
  disabled: boolean
  onChoose: (effort: Effort | null) => void
}): React.JSX.Element {
  const ignored = readsEffort === false
  return (
    <ModePicker name="effort" testId="effort" noun="Reasoning effort" modes={LEVELS}
      choices={CHOICES} mode={effort ?? 'default'} disabled={disabled || ignored}
      explanation={ignored ? 'This model does not take a reasoning effort level' : undefined}
      onChoose={(choice) => onChoose(choice === 'default' ? null : choice)} />
  )
}
