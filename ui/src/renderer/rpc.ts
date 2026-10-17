import { RequestFailed } from './rewind'
import * as t from './transcript'

/** Raised for the one failure that needs its own screen rather than a line of text. */
export class Unconfigurable extends Error {}

/** Which method answers which question. See [`t.REPLY`], which is where they are written down. */
export const METHOD: Record<t.Asked, string> = t.REPLY

/** Throws what an answer that failed is, and returns for one that did not. */
function unwrap(answer: { error?: { code: string; message: string } | null }): void {
  if (!answer.error) return
  // `config` is `Config::from_env` failing, which for a packaged app or one launched from a checkout
  // means the credentials were not baked in at compile time. It is not recoverable
  // from here and needs saying properly.
  if (answer.error.code === 'config') throw new Unconfigurable(answer.error.message)
  throw new RequestFailed(answer.error.code, answer.error.message)
}

export async function call<T>(method: string, params?: Record<string, unknown>): Promise<T> {
  const answer = await window.bravebot.request<T>(method, params)
  unwrap(answer)
  return answer.ok as T
}

/**
 * Send a turn as a bot.
 *
 * The same failure handling as [`call`], over a different channel — and a different channel
 * because this is the one kind of turn that carries files. See `bravebot:bots:send`.
 */
export async function callBot(request: {
  session: string
  slug: string
  prompt: string
  grounded: boolean
  model: string | null
  attachments?: string[]
}): Promise<void> {
  unwrap(await window.bravebot.sendBotTurn(request))
}
