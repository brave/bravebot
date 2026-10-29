/**
 * Which bot answered which turn when a person summoned it, and the one judgement about whether a
 * file on disk is that list (MEMORY-14).
 *
 * The agent's record cannot hold this for the reason `forks.ts` gives about lineage: `Record` has
 * no field for it, and the agent rewrites the whole record after every turn. So the main process
 * keeps it under its own key in `state.ts`, written from the bot a person picked and from what the
 * agent answered when the turn ended. No channel lets a window write a line of it.
 *
 * The window draws two things from it: the label over a summoned reply, and the "Replied to" card
 * in the summoned bot's own conversation. Neither decides anything from the reply's text.
 */

import { isSlug, parseHomeTurn, type HomeTurn } from './bots'
import { isSessionId, keyOf, type SessionRef } from './forks'
import { isProjectPath } from './recents'

/** Whose conversation a bot was summoned into. */
export type SummonHost = { kind: 'bot'; slug: string } | { kind: 'project' }

export interface Summon {
  /** The conversation the bot was summoned into, which is where its reply is. */
  session: SessionRef
  /** The summoned prompt's ordinal among the things the person said, as the agent counts them. */
  prompt: number
  /** The bot that answered. */
  slug: string
  host: SummonHost
  /** Where the summoned bot's own conversation stood, which is where its card is drawn. */
  after: HomeTurn | null
  /** The reply as `turn.done` carried it for display, cut to `REPLY_MAX`. */
  reply: string
  /** When the turn ended, in milliseconds. */
  at: number
}

export interface StoredSummons {
  summons: Summon[]
}

/** Enough for a working history, few enough that the file stays a file. The list only grows. */
export const SUMMONS_MAX = 1000

/** How much of a reply a card keeps. It shows a few lines, and the link opens the rest. */
export const REPLY_MAX = 4000

function isOrdinal(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0
}

function parseHost(value: unknown): SummonHost | null {
  if (typeof value !== 'object' || value === null) return null
  const { kind, slug } = value as Record<string, unknown>
  if (kind === 'project') return { kind }
  if (kind === 'bot' && isSlug(slug)) return { kind, slug }
  return null
}

/** The key a record is unique under: one summon per prompt of a conversation. */
function keyOfSummon(summon: Pick<Summon, 'session' | 'prompt'>): string {
  return `${keyOf(summon.session.directory, summon.session.id)}#${summon.prompt}`
}

/**
 * A summons list, always.
 *
 * Entries are filtered rather than the file refused, the way the forks are: one unreadable line
 * should not cost every other reply its label.
 */
export function parseSummons(value: unknown): StoredSummons {
  if (typeof value !== 'object' || value === null) return { summons: [] }
  const { summons } = value as { summons?: unknown }
  if (!Array.isArray(summons)) return { summons: [] }
  const seen = new Set<string>()
  const kept: Summon[] = []
  for (const entry of summons) {
    if (typeof entry !== 'object' || entry === null) continue
    const { session, prompt, slug, host, after, reply, at } = entry as Record<string, unknown>
    if (typeof session !== 'object' || session === null) continue
    const { directory, id } = session as Record<string, unknown>
    if (!isProjectPath(directory) || !isSessionId(id)) continue
    if (!isOrdinal(prompt) || !isSlug(slug)) continue
    const parsedHost = parseHost(host)
    if (!parsedHost || typeof reply !== 'string') continue
    if (typeof at !== 'number' || !Number.isFinite(at)) continue
    const summon: Summon = {
      session: { directory, id },
      prompt,
      slug,
      host: parsedHost,
      after: parseHomeTurn(after),
      reply: reply.slice(0, REPLY_MAX),
      at,
    }
    const key = keyOfSummon(summon)
    if (seen.has(key)) continue
    seen.add(key)
    kept.push(summon)
    if (kept.length === SUMMONS_MAX) break
  }
  return { summons: kept }
}

/** The list with `summon` first, replacing whatever was recorded at its prompt. */
export function withSummon(summons: Summon[], summon: Summon): Summon[] {
  const key = keyOfSummon(summon)
  return [summon, ...summons.filter((old) => keyOfSummon(old) !== key)].slice(0, SUMMONS_MAX)
}

/** The list without what was recorded at one prompt of a conversation. */
export function withoutSummonAt(summons: Summon[], session: SessionRef, prompt: number): Summon[] {
  const key = keyOfSummon({ session, prompt })
  return summons.filter((old) => keyOfSummon(old) !== key)
}

/** The list without anything recorded in one conversation. */
export function withoutSession(summons: Summon[], session: SessionRef): Summon[] {
  const key = keyOf(session.directory, session.id)
  return summons.filter((old) => keyOf(old.session.directory, old.session.id) !== key)
}

/** Which bot answered each summoned prompt of one conversation, by ordinal. */
export function summonedIn(summons: Summon[], session: SessionRef | null): Map<number, string> {
  const found = new Map<number, string>()
  if (!session) return found
  for (const summon of summons) {
    if (summon.session.directory === session.directory && summon.session.id === session.id) found.set(summon.prompt, summon.slug)
  }
  return found
}

/** The rows of a transcript `placeCards` needs to know about. */
export interface PlacedRow {
  id: string
  kind: string
  prompt?: number
}

/** Where each card goes: before a row, or after every row. */
export interface Placement {
  before: Map<string, Summon[]>
  end: Summon[]
}

/**
 * Where the cards for `slug`'s replies elsewhere go in its own conversation `home`.
 *
 * A card follows the turn its record says the conversation had reached, so it goes before the next
 * prompt after that one, or at the end where there is none. A card from before the conversation
 * began, or from another home conversation, goes before the first row. Oldest first within a place.
 */
export function placeCards(summons: Summon[], slug: string, home: string | null, rows: PlacedRow[]): Placement {
  const before = new Map<string, Summon[]>()
  const end: Summon[] = []
  const prompts = rows.filter((row) => row.kind === 'user')
  const first = rows[0]?.id
  const mine = summons.filter((summon) => summon.slug === slug && !(summon.host.kind === 'bot' && summon.host.slug === slug))
    .sort((a, b) => a.at - b.at)
  const put = (id: string | undefined, summon: Summon) => {
    if (id === undefined) { end.push(summon); return }
    before.set(id, [...(before.get(id) ?? []), summon])
  }
  for (const summon of mine) {
    if (!summon.after || summon.after.id !== home) { put(first, summon); continue }
    const reached = summon.after.prompt
    let passed = false
    let next: PlacedRow | undefined
    for (const row of prompts) {
      if (row.prompt === undefined ? passed : row.prompt > reached) { next = row; break }
      if (row.prompt === reached) passed = true
    }
    put(next?.id, summon)
  }
  return { before, end }
}

/** The marker a summoned bot is written into a draft as. Never an `@` word, so never a file name. */
export function summonMarker(name: string, slug: string): string {
  const shown = name.replace(/[@[\]]/g, '').replace(/\s+/g, ' ').trim()
  return `[@${shown || slug}]`
}
