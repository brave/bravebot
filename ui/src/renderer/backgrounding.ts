import type { BackgroundedSession, StoppedBackground } from '../shared/protocol'
import { RequestFailed } from './rewind'

/**
 * What the window does with the answer to `session.background` (BG-2, CMD-34).
 *
 * A refusal (`bad_request`) leaves the chat open, because the bridge refuses before it closes
 * anything, and the bridge's sentence is shown. Any other failure comes after the bridge closed
 * the chat, so the window lets go of it too and shows the sentence. A hand-over closes the chat and
 * shows a toast naming the start of the record's id, as `/bg` does in the terminal.
 */
export type Backgrounded =
  | { closed: true; toast: { title: string; body: string } }
  | { closed: boolean; problem: string }

/** How many characters of an id the terminal shows (`ID_SHOWN` in `crates/cli/src/background.rs`). */
export const ID_SHOWN = 8

export function shortId(id: string): string {
  return [...id].slice(0, ID_SHOWN).join('')
}

export function handedOver(answer: BackgroundedSession): Backgrounded {
  return {
    closed: true,
    toast: {
      title: 'Continued in the background',
      // The terminal's `bg-handed-over` line, and the mode it runs in.
      body: `It runs in ${answer.mode} mode. Join it with: bravebot attach ${shortId(answer.handedOver)}`,
    },
  }
}

export function notHandedOver(error: unknown): Backgrounded {
  if (error instanceof RequestFailed) return { closed: error.code !== 'bad_request', problem: error.reason }
  return { closed: false, problem: String(error) }
}

/**
 * What the window shows for the answer to `session.background.stop` (BG-11): a toast when the
 * process was stopped or had already ended, or the bridge's sentence when it could not be stopped.
 */
export type StoppedOutcome = { toast: { title: string; body: string } } | { problem: string }

/** Ask the bridge to stop the background session holding the chat `id`, and say what came of it. */
export async function stopBackground(
  ask: (id: string) => Promise<StoppedBackground>,
  id: string,
): Promise<StoppedOutcome> {
  try {
    const answer = await ask(id)
    if (answer.stopped === 'not-running') {
      return { toast: { title: 'Not running', body: 'It had already ended. Its chat is kept.' } }
    }
    return {
      // One line in the toast, so it is no longer than the hand-over's.
      toast: { title: 'Stopped', body: `Its chat is kept. Join it with: bravebot attach ${shortId(id)}` },
    }
  } catch (error) {
    return { problem: error instanceof RequestFailed ? error.reason : String(error) }
  }
}

/** Ask the bridge, and say what came of it. */
export async function continueInBackground(
  ask: (session: string) => Promise<BackgroundedSession>,
  session: string,
): Promise<Backgrounded> {
  try {
    return handedOver(await ask(session))
  } catch (error) {
    return notHandedOver(error)
  }
}
