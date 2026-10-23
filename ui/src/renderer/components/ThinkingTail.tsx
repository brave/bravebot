import { Icon } from '../nala'
import { lastLines } from '../thinking'

/**
 * The end of what the model has thought in this round, under a head of its own (VIEW-34).
 *
 * It is the model's output, so it reaches the tree as a text child and nowhere else: markup in it is
 * drawn as the characters it is, and the head above it is the one that says what it is. It is not an
 * entry of the transcript, since it is no part of the conversation, and it is drawn only between a
 * round's start and its words, so a session that is shown none draws nothing.
 */
export function ThinkingTail({ text }: { text: string }): React.JSX.Element | null {
  const shown = lastLines(text)
  if (shown === '') return null
  return (
    <div className="thinking" data-test="thinking">
      <div className="thinking-head"><Icon name="idea" />Thinking</div>
      <div className="thinking-text">{shown}</div>
    </div>
  )
}
