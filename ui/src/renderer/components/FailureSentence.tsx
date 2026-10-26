/**
 * A sentence the bridge wrote about a failure, with the words that are a command drawn as code.
 *
 * The command arrives beside the sentence, so nothing here reads the sentence to decide what to
 * mark: the words are found where they stand and wrapped, and a sentence that does not hold them is
 * drawn as it is. Both are text, drawn as text, so nothing in either is read as markup.
 */
export function FailureSentence({ text, command }: { text: string; command?: string | null }): React.JSX.Element {
  const at = command ? text.indexOf(command) : -1
  if (!command || at < 0) return <>{text}</>
  return <>{text.slice(0, at)}<code>{command}</code>{text.slice(at + command.length)}</>
}
