/**
 * The bots somebody has defined, and the one thing that decides whether a file on disk is that
 * list.
 *
 * A bot is a name, a purpose and a home folder of its own, with a history of conversations. Each
 * conversation runs in the bot's home folder or in a project somebody picked for it, and the bot
 * keeps one memory file in each folder it has worked in. Its identity persists when a new
 * conversation starts or an earlier one resumes.
 * Everything here is what the agent's own record cannot hold.
 *
 * It cannot hold it for the reason `forks.ts` states about lineage: `Record` has no field for any
 * of this, adding one is a change to a repository this app does not modify, and the agent rewrites
 * the whole record after every turn, so a key smuggled in beside it would not survive the first
 * reply.
 *
 * A separate file from the forks and the recents, for the reason each of those gives about the
 * other: a validator's job is to be the single judgement about *one* shape, and a hand-edited bot
 * should not cost somebody their fork banners.
 *
 * ## Which half of this the window may write
 *
 * Most of these fields are not the window's to set. `session` is the id the agent minted, read off
 * what it *answered* rather than off anything the renderer asked for, `archived` is a count the
 * agent reported about its own conversation, and `retired` is the moment somebody put the bot
 * away. All are main-process-written, the arrangement `recents` and `forks` already have and for
 * the same reason: a record of what happened is not a preference, and nothing that can be typed
 * into a window should be able to claim one. The form says four things; the rest is history.
 *
 * `retired` has a channel of its own rather than riding on the form, and that is the same rule
 * rather than an exception to it: the window may ask for a bot to be archived, and cannot say
 * what the field becomes.
 */

import { isProjectPath } from './recents'
import { isSessionId } from './forks'

export interface Bot {
  /**
   * The name every file belonging to this bot is named after.
   *
   * Restricted to `[a-z0-9-]` and composed from the name once, at creation. Both matter: this
   * becomes a path segment in three places — the ground file under `userData`, the home folder
   * under `userData`, and the memory file inside each folder the bot works in — and a path segment
   * that arrived as free text is a path segment that can be `..`.
   */
  slug: string
  /** What the bot is called, as somebody wrote it. Free text, and renamable. */
  name: string
  /** What it is for. Free text, and the whole of what makes it this bot rather than another. */
  purpose: string
  /**
   * The seed its icon is drawn from.
   *
   * Stored rather than derived from the slug or the name, because both of those can change and an
   * avatar that changes is not an avatar — the point of a face in a list is that it is the same
   * face tomorrow. Opaque here: what it means is `BotAvatar`'s business, and this only promises it
   * is a non-empty string that will be the same one next launch.
   */
  avatar: string
  /** Inference model; null uses the configured default for older bots. */
  model: string | null
  /**
   * The folder its conversations with no project run in, made by the main process under its own
   * data directory. Never one a window named. Main-written.
   */
  home: string
  /**
   * The name of the definition in `~/.bravebot/agents` that making this bot wrote, or `null` for a
   * bot made before that was done. The agent chose it (the slug, or the next free name after it),
   * so it is taken off the agent's answer and never off the form. Main-written.
   */
  definition: string | null
  /**
   * The durable id used by Continue, or `null` until it has spoken.
   *
   * Null is a real state rather than a missing value: the agent writes no record until a first
   * turn, so a bot made and not yet talked to genuinely has no session to name. Main-written.
   */
  session: string | null
  /**
   * Every recorded conversation for this bot, with the folder it ran in; retained when Continue
   * changes. The folders here are the projects the bot may be sent into again without the picker.
   * Main-written.
   */
  conversations: BotConversationRef[]
  /**
   * How much compaction had taken out of that session, last time anyone looked.
   *
   * The length of the conversation's archive, which the agent reports and which only ever rises.
   * A bot is re-grounded when this has gone up since the last turn, because that is the moment its
   * purpose stopped being in the conversation. Main-written.
   */
  archived: number
  /**
   * The modification time its memory file had, last time a turn of its finished.
   *
   * Not a copy of the memory and not a judgement about it — only the answer to "has it changed
   * since I last looked", which is the one question `quiet` below needs asked. Zero means nobody
   * has looked yet. Main-written, and read off the filesystem rather than off anything a model
   * said: the file's mtime is the only claim here nothing can be talked into.
   */
  remembered: number
  /**
   * How many of its turns have finished since that mtime last moved.
   *
   * The whole of the nudge: a bot keeping its memory current sits at zero forever and is never
   * reminded, and one that has stopped writing is handed its briefing again once this reaches
   * `QUIET_MAX` rather than waiting for a compaction. Main-written.
   */
  quiet: number
  /**
   * When it was archived, in milliseconds, or `0` for a bot still in use.
   *
   * A time rather than a flag because the only question the archive is ever asked is which of
   * these was put away most recently, and a boolean would need a date beside it to answer that.
   * Main-written, like the session id above: being archived is something that happened to a bot,
   * and a record of what happened is not a preference the window gets to claim.
   *
   * Nothing else about an archived bot differs. It keeps its slug, so it keeps its memory file;
   * it keeps its seed, so it keeps its face; it keeps its session, so bringing it back resumes
   * the same conversation. That is the whole difference between archiving one and forgetting it,
   * and it is why this is a field rather than a second list.
   */
  retired: number
  /** When it was made, in milliseconds. */
  created: number
  /** When anything about it last changed, in milliseconds. */
  updated: number
}

/** Model IDs are opaque provider identifiers, never paths or commands. */
export function isBotModel(value: unknown): value is string | null {
  return value === null || (
    typeof value === 'string' && value.trim().length > 0 && value.length <= 512 && !value.includes('\0')
  )
}

/** A conversation a bot has had, and the folder it ran in. */
export interface BotConversationRef {
  id: string
  directory: string
}

/** The folders a bot has worked in, home first. */
export function botFolders(bot: Bot): string[] {
  return [...new Set([bot.home, ...bot.conversations.map((each) => each.directory)])]
}

/** The projects a bot has worked in, newest conversation first, without its home folder. */
export function botProjects(bot: Bot): string[] {
  return [...new Set(bot.conversations.map((each) => each.directory).reverse())].filter((each) => each !== bot.home)
}

export interface StoredBots {
  bots: Bot[]
}

/**
 * Enough that nobody meets the ceiling, few enough that the file stays a file.
 *
 * Higher than `RECENTS_MAX` because this is not a menu, and far lower than `FORKS_MAX` because
 * unlike a fork a bot is made deliberately, one at a time, by somebody filling in a form.
 *
 * Both lists together, archived and not. An archive that did not count against the ceiling would
 * be a place bots could be put to get around it, and the file this all lives in is the thing the
 * ceiling is protecting.
 */
export const BOTS_MAX = 100

/** The characters a slug may be made of, which is the whole of why it is safe as a path segment. */
const SLUG = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

/**
 * Whether something is a slug.
 *
 * Deliberately stricter than "does not contain a separator". A name that cannot traverse is the
 * floor; what this asks for is a name that cannot surprise anybody — no dots, so `..` is
 * unreachable rather than merely excluded, no leading or trailing dash, and nothing that needs
 * escaping in a shell, a URL or a filesystem that folds case.
 */
export function isSlug(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 64 && SLUG.test(value)
}

/**
 * A slug for a name somebody typed.
 *
 * Lossy on purpose. It is not a rendering of the name and is never shown — the name is shown, and
 * this is what the files are called. A name with nothing usable in it at all still gets a slug,
 * because refusing to make a bot called `???` would be a validator making a product decision.
 */
export function slugFor(name: string, taken: ReadonlySet<string> = new Set()): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 48) || 'bot'
  if (!taken.has(base)) return base
  for (let n = 2; n < 1000; n++) {
    const tried = `${base}-${n}`
    if (!taken.has(tried)) return tried
  }
  return `${base}-${Date.now()}`
}

/** A non-empty line of text, with the whitespace-only case treated as the absence it is. */
function isText(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0 && !value.includes('\0')
}

/**
 * A bot list, always.
 *
 * Entries are filtered rather than the file refused, the way the recents and the forks are: one
 * unreadable line should not cost somebody every bot they have. Nothing is coerced, and a slug
 * appears once — it names files, and two bots claiming one would be two bots sharing a memory.
 *
 * Two fields may be missing. `avatar` is repaired from the slug rather than costing the bot its
 * row, because an icon is a smaller thing than a bot. `home` is missing from every bot written
 * before bots had one, and `homeFor` supplies it; without `homeFor` such a bot is dropped, because
 * a bot with nowhere to run cannot be shown as one that can.
 *
 * A bot written before then also has a `directory` it was pinned to. Its conversations are paired
 * with that folder, so they keep running where they always ran.
 */
export function parseBots(value: unknown, homeFor?: (slug: string) => string): StoredBots {
  if (typeof value !== 'object' || value === null) return { bots: [] }
  const { bots } = value as { bots?: unknown }
  if (!Array.isArray(bots)) return { bots: [] }

  const seen = new Set<string>()
  const kept: Bot[] = []
  for (const entry of bots) {
    if (typeof entry !== 'object' || entry === null) continue
    const {
      slug,
      name,
      purpose,
      avatar,
      model,
      home,
      directory,
      definition,
      session,
      conversations,
      archived,
      remembered,
      quiet,
      retired,
      created,
      updated,
    } = entry as Record<string, unknown>

    if (!isSlug(slug) || seen.has(slug)) continue
    if (!isText(name) || !isText(purpose)) continue
    const placed = isProjectPath(home) ? home : homeFor?.(slug)
    if (!isProjectPath(placed)) continue
    // Only bots written before homes existed carry this. It is where their conversations ran.
    const legacy = isProjectPath(directory) ? directory : null
    // Null and absent both mean "has not spoken yet". Anything else claiming to be an id has to
    // look like one, and a bot pointing at a session that cannot exist is dropped rather than
    // shown as openable.
    if (session !== null && session !== undefined && !isSessionId(session)) continue
    if (created !== undefined && (typeof created !== 'number' || !Number.isFinite(created))) continue
    if (updated !== undefined && (typeof updated !== 'number' || !Number.isFinite(updated))) continue

    const at = typeof created === 'number' ? created : Date.now()
    seen.add(slug)
    kept.push({
      slug,
      name,
      purpose,
      avatar: isText(avatar) ? avatar : slug,
      model: isBotModel(model) && model !== null ? model : null,
      home: placed,
      definition: isSlug(definition) ? definition : null,
      session: isSessionId(session) ? session : null,
      conversations: parseConversations(conversations, isSessionId(session) ? session : null, legacy ?? placed),
      // Clamped rather than refused. It is a watermark, and a nonsense one costs one needless
      // re-grounding, where dropping the bot over it costs the bot.
      archived:
        typeof archived === 'number' && Number.isInteger(archived) && archived >= 0 ? archived : 0,
      // Both clamped rather than refused, for the reason above them: these decide when a bot is
      // reminded to write, and a nonsense figure costs one needless briefing where dropping the
      // row over it costs the bot. Absent is the ordinary case — every bot written before this
      // existed has neither.
      remembered: typeof remembered === 'number' && Number.isFinite(remembered) && remembered >= 0
        ? remembered
        : 0,
      quiet: typeof quiet === 'number' && Number.isInteger(quiet) && quiet >= 0 ? quiet : 0,
      // Clamped rather than refused for the reason the three above it are, and absent is the
      // ordinary case: every bot written before the archive existed is a bot still in use.
      retired: typeof retired === 'number' && Number.isFinite(retired) && retired >= 0 ? retired : 0,
      created: at,
      updated: typeof updated === 'number' ? updated : at,
    })
    if (kept.length === BOTS_MAX) break
  }
  return { bots: kept }
}

/**
 * A bot's conversations, each id once.
 *
 * An entry is either a pair or, from a bot written before pairs existed, a bare id, which is
 * paired with `fallback`: the folder that bot was pinned to.
 */
function parseConversations(value: unknown, session: string | null, fallback: string): BotConversationRef[] {
  const seen = new Set<string>()
  const kept: BotConversationRef[] = []
  const add = (id: unknown, directory: unknown) => {
    if (!isSessionId(id) || !isProjectPath(directory)) return
    const key = `${directory}\u0000${id}`
    if (seen.has(key)) return
    seen.add(key)
    kept.push({ id, directory })
  }
  for (const entry of Array.isArray(value) ? value : []) {
    if (typeof entry === 'string') add(entry, fallback)
    else if (typeof entry === 'object' && entry !== null) {
      const { id, directory } = entry as Record<string, unknown>
      add(id, directory)
    }
  }
  if (session && !kept.some((each) => each.id === session)) add(session, fallback)
  return kept
}

/** The one with this slug, or `null`. */
export function botOf(bots: Bot[], slug: string): Bot | null {
  return bots.find((bot) => bot.slug === slug) ?? null
}

/**
 * The list with `bot` in it, replacing whatever shared its slug.
 *
 * Order is by slug rather than by recency, unlike every other list this app keeps. A bot is a
 * thing somebody named and expects to find in the same place twice; a list that reordered itself
 * as each one was spoken to would make the column a moving target.
 */
export function withBot(bots: Bot[], bot: Bot): Bot[] {
  return [...bots.filter((old) => old.slug !== bot.slug), bot]
    .sort((a, b) => a.slug.localeCompare(b.slug))
    .slice(0, BOTS_MAX)
}

/**
 * The bots still in use, in the order the list already keeps them.
 *
 * The split lives here rather than in the window because this is the file that owns the shape,
 * and because two callers want the same answer: the list draws one half and the archive draws the
 * other, and a disagreement between them would be a bot in neither.
 */
export function activeBots(bots: Bot[]): Bot[] {
  return bots.filter((bot) => bot.retired === 0)
}

/**
 * The ones put away, most recently archived first.
 *
 * A different order from the list above, deliberately. That one is alphabetical because a bot is
 * a thing somebody expects to find in the same place twice. An archive is not looked *through*,
 * it is looked *back at* — and what somebody is nearly always after is the one they just put
 * away, which a list sorted by slug would hide in the middle.
 */
export function retiredBots(bots: Bot[]): Bot[] {
  return bots.filter((bot) => bot.retired !== 0).sort((a, b) => b.retired - a.retired)
}

/** The list without the bot of this slug. The memory file and the session are not this list's. */
export function withoutBot(bots: Bot[], slug: string): Bot[] {
  return bots.filter((bot) => bot.slug !== slug)
}
