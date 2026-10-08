import { createInterface } from 'node:readline/promises'
import type { Readable, Writable } from 'node:stream'

/**
 * Yes or no questions on one terminal. When the input ends (Ctrl-D) the pending question and every
 * later one is answered no, because nobody is left to say yes. One listener watches for that end, so
 * any number of questions can be asked.
 */
export function yesNoAsker(input: Readable, output: Writable): { ask(text: string): Promise<boolean>; close(): void } {
  const terminal = createInterface({ input, output })
  let ended = false
  const inputEnded = new Promise<string>((resolve) =>
    terminal.once('close', () => {
      ended = true
      resolve('n')
    }),
  )
  return {
    async ask(text) {
      if (ended) return false
      // A question the interface abandons because its input ended counts as no, however it is reported.
      const answer = await Promise.race([terminal.question(text).catch(() => 'n'), inputEnded])
      return answer.trim().toLowerCase().startsWith('y')
    },
    close: () => terminal.close(),
  }
}
