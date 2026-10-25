import { FailureSentence } from './FailureSentence'

/**
 * What the model list could not ask of a service, one sentence each, in the picker's status type.
 * A warning that names a command to run has it beside the sentence, in the same order, and it is
 * drawn as code.
 */
export function ModelWarnings({ warnings, commands }: { warnings: string[]; commands?: (string | null)[] }): React.JSX.Element {
  return <>{warnings.map((warning, index) => <p className="model-status" key={warning}><FailureSentence text={warning} command={commands?.[index]} /></p>)}</>
}
