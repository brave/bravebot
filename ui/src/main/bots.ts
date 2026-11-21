import { randomUUID } from 'node:crypto'
import { readProjectText } from './project-files'
/**
 * The desktop's bots: the bridge calls that read and write a row, where a memory file sits, and
 * the prompts this process composes.
 *
 * The session id and the compaction watermark are written from what the agent answered, never from
 * anything a window asked for. The form says a name, a purpose, a model and an avatar.
 */

import { type Bot, type BotServers, botFolders, botOf, isSlug, parseBotServers, withBot } from '../shared/bots'
import { newAvatarSeed } from '../shared/avatar'
import { chosenDirectories } from './opened'
import { type BotRow, type BridgeWrite, bridgeRequest, oneAtATime, readBots, updateBots, writeThroughBridge } from './bot-store'

/** Where a bot's files live inside the checkout it works in, relative to that checkout. */
const HOME = '.bravebot-ui'

/**
 * The largest memory this will read into the window.
 *
 * A cap rather than trust: the memory is the planner's own writing, and a runaway one would be a
 * panel this process built out of however much somebody's model chose to write. Well past any
 * memory worth keeping. Nothing copies it into a briefing any more, so this is the preview's bound
 * and not a turn's.
 */
const MEMORY_MAX = 64 * 1024

/** Every bot defined. Never throws; an unreadable file is no bots. */
export const bots = readBots

/** The one with this slug, or `null`. */
export async function bot(slug: unknown): Promise<Bot | null> {
  return isSlug(slug) ? botOf(await bots(), slug) : null
}

const FORM_FIELDS = ['name', 'purpose', 'model', 'avatar'] as const

async function writeBot(prepare: () => Promise<BridgeWrite> | BridgeWrite): Promise<Bot> {
  const answer = (await writeThroughBridge(prepare)) as { bot: BotRow }
  return answer.bot
}

export async function saveForm(value: unknown): Promise<Bot | null> {
  if (typeof value !== 'object' || value === null) return null
  const sent = value as Record<string, unknown>
  return writeBot(async () => {
    const fields = Object.fromEntries(FORM_FIELDS.filter((key) => sent[key] !== undefined).map((key) => [key, sent[key]]))
    if (isSlug(sent.slug) && (await bot(sent.slug))) return { method: 'bot.edit', params: { slug: sent.slug, ...fields } }
    return { method: 'bot.make', params: { avatar: newAvatarSeed(randomUUID()), ...fields } }
  })
}

export async function carryStillOwed(slug: string, folder: string): Promise<boolean> {
  const answer = (await bridgeRequest('bot.owes_carry_over', { slug, folder })) as { owes?: unknown } | null
  if (typeof answer?.owes !== 'boolean') throw new Error('the agent did not say whether a carry-over is owed')
  return answer.owes
}

export async function definitionForTurn(slug: string): Promise<Bot> {
  const answer = (await writeThroughBridge(() => ({ method: 'bot.migrate', params: { slug } }))) as { bot?: BotRow } | null
  const row = answer?.bot
  if (!row || !isSlug(row.definition)) throw new Error('The agent did not name the bot’s definition.')
  return row
}

export function saveBotModel(slug: string, model: string | null): Promise<Bot> {
  return writeBot(() => ({ method: 'bot.set_model', params: { slug, model } }))
}

/** The agent requests that read and rewrite a bot's `mcpServers:` line. */
export type ServersRequest = (method: 'bot.servers.read' | 'bot.servers.write', params: Record<string, unknown>) => Promise<unknown>

/**
 * The servers a bot's definition lets it use. The definition is named from the row, never by a
 * window, so a window can only ask about a bot's own file.
 */
export async function readBotServers(held: Bot, request: ServersRequest | null): Promise<BotServers> {
  if (held.definition === null) throw new Error('This bot has no definition yet. Open it once, then try again.')
  if (!request) throw new Error('The agent is not running, so this bot’s servers cannot be read.')
  const answer = parseBotServers(await request('bot.servers.read', { name: held.definition }))
  if (!answer) throw new Error('The agent did not say which servers this bot may use.')
  return answer
}

/**
 * Rewrite a bot's `mcpServers:` line, one save at a time with the form's saves, so it never races
 * a rewrite of the same file's purpose or model. `null` removes the line.
 */
export function saveBotServers(held: Bot, servers: string[] | null, request: ServersRequest | null): Promise<BotServers> {
  return oneAtATime(async () => {
    const definition = (await bot(held.slug))?.definition ?? null
    if (definition === null) throw new Error('This bot has no definition yet. Open it once, then try again.')
    if (!request) throw new Error('The agent is not running, so this bot cannot be edited.')
    const answer = (await request('bot.servers.write', { name: definition, servers })) as { name?: unknown } | null
    if (answer?.name !== definition) throw new Error('The agent did not rewrite the bot’s definition.')
    return readBotServers({ ...held, definition }, request)
  })
}

/** Write a bot down, replacing whatever shared its slug, and stamp when that happened. */
export async function saveBot(next: Bot): Promise<Bot> {
  const written = await updateBots((all) => withBot(all, { ...next, updated: Date.now() }))
  return written.find((each) => each.slug === next.slug) ?? next
}

export async function ensureBotHome(slug: string): Promise<boolean> {
  try {
    const answer = (await bridgeRequest('bot.ensure_home', { slug })) as { homeMade?: unknown }
    return answer.homeMade === true
  } catch {
    return false
  }
}

export async function worksIn(held: Bot, directory: string | undefined): Promise<boolean> {
  if (directory === undefined) return false
  const answer = (await bridgeRequest('bot.works_in', {
    slug: held.slug,
    directory,
    opened: chosenDirectories(),
  })) as { allowed?: unknown }
  return answer.allowed === true
}

/**
 * Record a durable conversation and the folder it ran in, without discarding any earlier ones.
 *
 * A turn that ended in the home folder at a prompt the agent numbered also moves `lastHome` there,
 * which is where a summon elsewhere places its card (MEMORY-14). The module records both.
 */
export async function noteBotSession(slug: string, id: string, directory: string, prompt?: number): Promise<void> {
  const numbered = typeof prompt === 'number' && Number.isInteger(prompt) && prompt >= 0 ? prompt : null
  await writeThroughBridge(() => ({ method: 'bot.record_conversation', params: { slug, id, directory, prompt: numbered } }))
}

/** Note how much compaction has taken out of a bot's session. The module raises the stored length only when the new figure is greater. */
export async function noteBotArchived(slug: string, archived: number): Promise<void> {
  await writeThroughBridge(() => ({ method: 'bot.record_archived', params: { slug, archived } }))
}

/**
 * Let go of the session behind a bot, so the next turn adopts a new one.
 *
 * For one case only: the record is gone from the agent's own store, which happens when somebody
 * deletes it or moves the checkout out from under it. Without this a bot would keep pointing at an
 * id nothing can open. Release the continuation pointer while retaining its history entry.
 *
 * The window may ask for this and cannot say what it becomes. Null is the only value it can lead
 * to, which keeps the promise the split is made of: an id is something the agent said.
 */
export async function releaseBotSession(slug: unknown): Promise<void> {
  if (!isSlug(slug)) return
  await writeThroughBridge(() => ({ method: 'bot.release_session', params: { slug } }))
}

/**
 * Forget a conversation that was deleted, in every bot that listed it. A bot whose continuation
 * pointer was that conversation starts a new one on its next turn.
 */
export async function forgetBotConversation(directory: string, id: string): Promise<void> {
  await writeThroughBridge(() => ({ method: 'bot.forget_conversation', params: { id, directory } }))
}

/**
 * Put a bot away, or take it back out.
 *
 * Nothing moves. The memory file stays where the agent left it and the session stays where the
 * agent keeps every session, which is why coming back is one field changing rather than a bot
 * being rebuilt. A rebuilt bot would be a different bot wearing the same name: a new slug, so a
 * different memory file, and a new seed, so a different face.
 *
 * The window may ask for this and cannot say what the field becomes, which is the arrangement
 * `releaseBotSession` above has and for the same reason.
 */
export async function retireBot(slug: unknown, retired: unknown): Promise<Bot | null> {
  if (typeof retired !== 'boolean' || !isSlug(slug)) return null
  const method = retired ? 'bot.archive' : 'bot.restore'
  const answer = (await writeThroughBridge(async () => {
    const held = await bot(slug)
    if (!held) return null
    return { method, params: { slug: held.slug } }
  })) as { bot: BotRow } | null
  return answer === null ? null : answer.bot
}

export async function removeBot(slug: unknown, turnInProgress: boolean): Promise<string | null> {
  if (!isSlug(slug)) return null
  if (turnInProgress) throw new Error('Stop this bot’s running conversations before deleting it.')
  const removed = await writeThroughBridge(async () => {
    const held = await bot(slug)
    if (!held) return null
    return { method: 'bot.delete', params: { slug: held.slug } }
  })
  return removed === null ? null : slug
}

/**
 * Where a bot's memory sits inside its checkout, as the agent would name it.
 *
 * A bot with a definition keeps its memory where the agent keeps every definition's, which the run
 * is told about and reads itself (MEMORY-4). A bot made before definitions existed has none until
 * it is given one (MEMORY-11), and until then is named at the file this app made for it.
 */
export function memoryPath(bot: Pick<Bot, 'slug' | 'definition'>): string {
  return bot.definition === null ? `${HOME}/bots/${bot.slug}.md` : definitionMemoryPath(bot.definition)
}

/**
 * Where the memory of a bot's definition is, relative to its checkout (MEMORY-2).
 *
 * Made from the definition's name, which the agent chose and this process judged to be a slug. Once
 * a bot has a definition this is the only memory path a composed turn names. The old path is never
 * named then, since the agent records it as untrusted and a run told to read it would ask a person
 * about notes they were already going to be asked about (MEMORY-11).
 */
export function definitionMemoryPath(definition: string): string {
  return `.bravebot/memory/${definition}.md`
}

/**
 * What this app says to a bot when it sends a turn nobody typed.
 *
 * Nothing in it says what it is. What tells it apart, in this run's transcript or in one reopened
 * next year, is the `composed` tag the send carries, which rides beside the message rather than
 * inside it: a sentence written here to be recognised later would be a sentence the backend was
 * sent, and one anybody could type into the composer to be drawn as this app's own row.
 *
 * `why` is the sentence that differs between the occasions this is sent, and is the only part a
 * caller supplies. Nothing here interpolates anything a model said.
 */
export function consolidationPrompt(bot: Bot, why: string): string {
  return [
    why,
    '',
    `Look back over this conversation and bring \`${memoryPath(bot)}\` up to date: add what`,
    'has turned out to be durable — a decision and why, a constraint, how something here is',
    'arranged — and prune whatever has stopped being true. If nothing in it needs changing, say so',
    'in one line and change nothing; an honest "no" is a better answer than an invented entry.',
    '',
    'Do not do any other work in this turn, and do not answer whatever was being discussed.',
    '',
  ].join('\n')
}

/**
 * What this app says to a migrated bot once its first turn has ended (MEMORY-11).
 *
 * The old notes are named in the words of the turn and never handed over as a file, since a file
 * handed to a turn is one a person is recorded as vouching for. The agent records them as
 * untrusted, so the run's read of them is quarantined and a person is asked before they reach it.
 * Only the slug and the definition's name go into it, both judged slugs when they were stored.
 */
export function carryOverPrompt(bot: Pick<Bot, 'slug' | 'definition'>): string {
  return [
    'This bot was made before bots had definitions, and its earlier notes are still in',
    `\`${HOME}/bots/${bot.slug}.md\` in this folder. Nobody has read them since they were recorded as`,
    'untrusted, so reading them asks the person first. If the read is refused or the file is',
    'withheld, say so and change nothing.',
    '',
    `Read that file, then bring \`${memoryPath(bot)}\` up to date with whatever in it still holds: a`,
    'decision and why, a constraint, how something here is arranged. Leave out anything that has',
    'stopped being true. If the file is absent or none of it still holds, say so in one line and',
    'change nothing.',
    '',
    'Do not do any other work in this turn, and do not answer whatever was being discussed.',
    '',
  ].join('\n')
}

/** Why a consolidation was sent after a compaction, in the one sentence that turn opens with. */
export const AFTER_COMPACTION =
  'Your conversation has just been summarised, and the detail behind that summary is now the only' +
  ' thing your memory can still be written from.'

/** What every memory file says about itself, at the top. */
const MEMORY_NOTE =
  'Written by the bot itself, and shown in the transcript each time it changes. Anything here is carried into every conversation it has; anything not here is forgotten when the conversation is compacted.'

/** The same note as earlier versions wrote it, broken at about 90 columns. */
const WRAPPED_MEMORY_NOTE = [
  'Written by the bot itself, and shown in the transcript each time it changes. Anything here',
  'is carried into every conversation it has; anything not here is forgotten when the',
  'conversation is compacted.',
].join('\n')

/**
 * The text with the note joined onto one line, or `null` when it does not hold the note exactly as
 * earlier versions wrapped it. Only that fixed block is replaced; the rest is carried as it is.
 */
export function unwrapMemoryNote(text: string): string | null {
  return text.includes(WRAPPED_MEMORY_NOTE) ? text.replace(WRAPPED_MEMORY_NOTE, () => MEMORY_NOTE) : null
}

/**
 * The folder a window asked about, if it is one this bot has worked in.
 *
 * Only folders this process recorded. A window may not point the memory reader at a folder of its
 * choosing.
 */
export function folderOf(held: Bot, directory: unknown): string | null {
  return typeof directory === 'string' && botFolders(held).includes(directory) ? directory : null
}

/**
 * What a bot's memory in one folder says, for showing it in the window. Never the path, only the
 * words.
 */
export async function memory(slug: unknown, directory: unknown): Promise<string | null> {
  const held = await bot(slug)
  if (!held) return null
  const folder = folderOf(held, directory)
  if (!folder) return null
  return readProjectText(folder, memoryPath(held), MEMORY_MAX)?.text ?? null
}
