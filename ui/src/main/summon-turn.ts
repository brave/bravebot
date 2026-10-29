/**
 * The send behind `bravebot:bots:summon` (MEMORY-14), kept apart from the Electron entry point so
 * that a test can call it with the services it needs in hand. A window can invoke the channel
 * directly, so every refusal here has to hold without the window's own `@` list having filtered
 * the pick first.
 */

import { type Bot } from '../shared/bots'
import { type Pending } from './summons'

/** Why a bot's turn could not be sent, in the shape `bravebot:request` already answers with. */
export interface Failure {
  code: string
  message: string
}

export interface Answer {
  ok?: unknown
  error?: Failure
}

export type Request = (method: string, params: Record<string, unknown>) => Promise<unknown>

/**
 * The handles running a turn a person summoned a bot for, and what the record of it will say once
 * the turn ends.
 *
 * Not written down, for the reason `consolidating` is not. Set before the send, because the answer
 * can arrive before an `await` resumes. A turn that fails or a conversation that closes abandons
 * it, so the next turn on that handle is not recorded as a summoned one.
 */
export class Summoning {
  private readonly held = new Map<string, Pending>()

  begin(session: string, pending: Pending): void {
    this.held.set(session, pending)
  }

  /** The turn ended: what its send left, and nothing after. */
  finish(session: string): Pending | null {
    const pending = this.held.get(session) ?? null
    this.held.delete(session)
    return pending
  }

  /** The turn failed or the conversation closed. */
  abandon(session: string): void {
    this.held.delete(session)
  }

  has(session: string): boolean {
    return this.held.has(session)
  }
}

/** What the send reads and writes outside this file, so that none of it needs Electron. */
export interface Services {
  /** The agent's request, or null where it is not running. */
  request: Request | null
  summoning: Summoning
  /** The folder the agent confirmed the session works in, or undefined where it is not open. */
  folderOf(session: string): string | undefined
  /** The bot whose conversation this is: the one whose handle it is, or whose home it runs in. */
  ownerOf(session: string, folder: string): string | undefined
  /** Record that a handle is a bot's conversation. */
  adopt(session: string, slug: string): void
  /** Give a bot made before definitions one. Throws where it cannot. */
  migrate(held: Bot, request: Request | null): Promise<Bot>
  /**
   * The bot whose conversation this is, given a definition if it has none, with what a turn of its
   * own owes noted against the handle. Null where the row is gone. Throws where it cannot be given
   * one.
   */
  host(session: string, slug: string, folder: string, request: Request | null): Promise<Bot | null>
  /** Add the files, drops and pasted pictures a send names to its parameters. Throws on a refusal. */
  carry(
    session: string,
    params: Record<string, unknown>,
    options: { attachments?: unknown; drops?: unknown; pastes?: unknown },
  ): Record<string, unknown>
  /** What a thrown error is to the window. */
  failure(error: unknown): Failure
}

/**
 * Send one turn addressed to a bot a person picked in the composer.
 *
 * The turn runs in the folder the agent confirmed the session works in and under the session's
 * history, so it holds nothing a turn of that conversation did not. The bot's definition name is
 * the row's, judged a slug when it was stored, and is sent as `address`, which the bridge applies
 * to this turn alone. In a bot's conversation the bot's own definition goes with it as `definition`,
 * which the bridge keeps standing, so a turn that follows without one (a watch, a consolidation) is
 * the bot's own. The summoned bot's model is its definition's, so only the conversation's goes as
 * `model`. No briefing is attached: a bot without a definition is given one first.
 *
 * Whose conversation this is comes from this process's own records: the bot whose handle this is,
 * or the bot whose home the session runs in. Anything else is a project's.
 */
export async function sendSummonedTurn(
  services: Services,
  session: string,
  summoned: Bot,
  prompt: string,
  options: { model?: string | null; attachments?: unknown; drops?: unknown; pastes?: unknown },
): Promise<Answer> {
  const { request } = services
  if (!request) return { error: { code: 'no_bridge', message: 'the agent is not running' } }
  const folder = services.folderOf(session)
  if (folder === undefined) return { error: { code: 'bad_request', message: 'that session is not open' } }
  const owner = services.ownerOf(session, folder)
  if (owner === summoned.slug) {
    return { error: { code: 'bad_request', message: `this is ${summoned.name}’s own conversation` } }
  }
  try {
    summoned = await services.migrate(summoned, request)
  } catch (error) {
    return { error: { code: 'no_definition', message: `${summoned.name} could not be given a definition: ${String(error)}` } }
  }
  if (summoned.definition === null) {
    return { error: { code: 'no_definition', message: `${summoned.name} has no definition` } }
  }
  // A bot's home conversation is that bot's from its first turn, summoned or not, so the turn's end
  // is recorded against it as one of its own would be, and the turn is sent as one of its own would
  // be: with its definition standing, which the summoned bot's address does not replace.
  let hosting: Bot | null = null
  if (owner !== undefined) {
    try {
      hosting = await services.host(session, owner, folder, request)
    } catch (error) {
      return { error: { code: 'no_definition', message: `the bot this is the conversation of could not be given a definition: ${String(error)}` } }
    }
    services.adopt(session, owner)
  }

  const params: Record<string, unknown> = { session, prompt, address: summoned.definition }
  if (hosting?.definition) params.definition = hosting.definition
  const selectedModel = hosting?.model ?? options.model
  if (selectedModel !== undefined) params.model = selectedModel
  services.summoning.begin(session, {
    slug: summoned.slug,
    host: owner === undefined ? { kind: 'project' } : { kind: 'bot', slug: owner },
    after: summoned.lastHome,
  })
  try {
    return { ok: await request('turn.send', services.carry(session, params, options)) }
  } catch (error) {
    services.summoning.abandon(session)
    return { error: services.failure(error) }
  }
}
