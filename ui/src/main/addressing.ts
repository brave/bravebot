/**
 * The send behind `bravebot:definitions:address` (ADDRESS-3): one turn addressed to a definition a
 * person picked from the composer's `@` list. Kept apart from the Electron entry point, as
 * `summon-turn.ts` is, so that a test can call it with fake services. A window can invoke the
 * channel directly, so the name it sends is checked here against the definitions the bridge
 * resolves for the session now, and is never trusted as it came.
 */

import { randomUUID } from 'node:crypto'
import { isSlug, type Bot } from '../shared/bots'
import { type Answer, type Services } from './summon-turn'

/** The definition names among the rows of a `mentions.offer` answer. */
function definitionRows(offer: unknown): string[] {
  const entries = (offer as { entries?: unknown } | null)?.entries
  if (!Array.isArray(entries)) return []
  return entries
    .filter((entry): entry is { kind: 'definition'; path: string } =>
      typeof entry === 'object' && entry !== null && entry.kind === 'definition' && typeof entry.path === 'string')
    .map((entry) => entry.path)
}

/**
 * Send one turn addressed to the definition `name`.
 *
 * A prompt that is empty once trimmed is refused with `needs_a_task`, as `/agent` refuses one
 * (ADDRESS-5). The name has to be a slug, the rule the bridge's `address` applies, and has to be
 * among the definition rows `mentions.offer` returns for this session, which is the set `/agent`
 * accepts, less any whose name is not a slug. The bridge lists a definition named exactly as typed
 * first, so its cap on rows never leaves the name out. A file or directory row of the same name
 * does not count, and a miss lists the definitions a bare `@` offers. In a bot's conversation the
 * bot's own definition goes as `definition`, which the bridge keeps standing, as
 * `sendSummonedTurn` sends it. Nothing is recorded in `Summoning`, since no bot was summoned.
 *
 * A model that needs a sign-in or that the machine refuses is not checked here: the agent's turn
 * refuses it and sends nothing (ADDRESS-11).
 */
export async function sendAddressedTurn(
  services: Services,
  session: string,
  name: unknown,
  prompt: string,
  options: { model?: string | null; attachments?: unknown; drops?: unknown; pastes?: unknown; shown?: unknown },
): Promise<Answer> {
  const { request } = services
  if (!request) return { error: { code: 'no_bridge', message: 'the agent is not running' } }
  if (!isSlug(name)) return { error: { code: 'bad_request', message: 'not a definition name' } }
  if (prompt.trim() === '') return { error: { code: 'needs_a_task', message: `/agent ${name} needs a task` } }
  const folder = services.folderOf(session)
  if (folder === undefined) return { error: { code: 'bad_request', message: 'that session is not open' } }

  let offered: string[]
  try {
    offered = definitionRows(await request('mentions.offer', { session, line: `@${name}`, cursor: 0, definitions: true }))
  } catch (error) {
    return { error: services.failure(error) }
  }
  if (!offered.includes(name)) {
    // The rows for the name hold only definitions whose names hold it, so the refusal lists the
    // ones a bare `@` offers.
    let all: string[]
    try {
      all = definitionRows(await request('mentions.offer', { session, line: '@', cursor: 0, definitions: true }))
    } catch (error) {
      return { error: services.failure(error) }
    }
    const listed = all.length === 0 ? 'none' : all.join(', ')
    return { error: { code: 'no_such_definition', message: `no definition named ${name}; definitions: ${listed}` } }
  }

  const owner = await services.ownerOf(session, folder)
  let hosting: Bot | null = null
  if (owner !== undefined) {
    try {
      hosting = await services.host(session, owner, folder, request)
    } catch (error) {
      return { error: { code: 'no_definition', message: `the bot this is the conversation of could not be given a definition: ${String(error)}` } }
    }
    services.adopt(session, owner)
  }

  // A new id for this send, which the agent requires on every send (RPCVIEW-10).
  const params: Record<string, unknown> = { session, prompt, address: name, messageId: randomUUID() }
  if (hosting?.definition) params.definition = hosting.definition
  // The line as the window's box showed it, where the bridge reads the `@` names (NAME-12).
  if (typeof options.shown === 'string') params.shown = options.shown
  const selectedModel = hosting?.model ?? options.model
  if (selectedModel !== undefined) params.model = selectedModel
  try {
    return { ok: await request('turn.send', services.carry(session, params, options)) }
  } catch (error) {
    return { error: services.failure(error) }
  }
}
