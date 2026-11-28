/**
 * A sentence the bridge wrote about a failure, with the words that are a command or a setting's name
 * drawn as code.
 *
 * The words arrive beside the sentence, as `command` for one or `codes` for several in the order the
 * sentence holds them, so nothing here reads the sentence to decide what to mark: each is found where
 * it stands, after the one before it, and wrapped, and one the sentence does not hold is skipped. Both
 * are text, drawn as text, so nothing in either is read as markup.
 */
export function FailureSentence({ text, command, codes }: { text: string; command?: string | null; codes?: readonly string[] | null }): React.JSX.Element {
  const words = codes ?? (command ? [command] : [])
  const parts: React.ReactNode[] = []
  let from = 0
  for (const word of words) {
    const at = word ? text.indexOf(word, from) : -1
    if (at < 0) continue
    parts.push(text.slice(from, at), <code key={parts.length}>{word}</code>)
    from = at + word.length
  }
  if (parts.length === 0) return <>{text}</>
  return <>{parts}{text.slice(from)}</>
}
