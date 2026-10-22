/** How much of the model's thinking the window holds, counted from the end (VIEW-34). */
export const THINKING_KEPT = 4096

/** How many lines of it are drawn, counted from the end (VIEW-34). */
export const THINKING_ROWS = 8

/**
 * The end of what the model has thought, as far back as is kept.
 *
 * A model can think for minutes and the window holds what has arrived for every frame it redraws,
 * so what it holds is bounded. A cut that lands inside a character gives up that character's other
 * half rather than draw half of it.
 */
export function keepEnd(text: string): string {
  if (text.length <= THINKING_KEPT) return text
  const cut = text.slice(-THINKING_KEPT)
  const first = cut.charCodeAt(0)
  return first >= 0xdc00 && first <= 0xdfff ? cut.slice(1) : cut
}

/**
 * The lines drawn at the tail: the last few of what is held, with the blank space around it taken
 * off. Nothing is read out of the words, only where the lines end.
 */
export function lastLines(text: string): string {
  return text.trim().split(/\r\n|\r|\n/).slice(-THINKING_ROWS).join('\n')
}
