import { readProjectText } from './project-files'
/**
 * Where a bot's definition is kept, and where the two files it speaks through are made.
 *
 * The same arrangement as the forks and the recents next door: the list lives under its own key in
 * `state.ts`, and the half of it that reports what the agent did — the session id, the compaction
 * watermark — is written here from what the agent *answered*, never from anything a window asked
 * for. What is different is that the other half is a preference somebody types, so unlike those
 * two this module does take dictation. It takes it four fields at a time.
 *
 * ## Why there are two files, and why only one of them is handed to a turn
 *
 * The agent has no persona field. `Task` offers a prompt, some files, and a home directory, and
 * the system prompt belongs to the build — so the only way to put a standing purpose in front of
 * it is to give it something to read. Two candidates exist and this uses both, for different
 * halves of the job:
 *
 * - **The ground file**, `<userData>/bots/<slug>/ground.md`, is composed here from the bot's name,
 *   its purpose, and whatever its memory currently says. It is handed to a turn as `dropped`,
 *   which is the read that is deliberately *not* confined to the workspace. It lives outside the
 *   checkout precisely so the planner cannot rewrite the thing that defines it: the agent may
 *   write inside the workspace and nowhere else, and this is nowhere else.
 * - **The memory file**, `<directory>/.bravebot-ui/bots/<slug>.md`, is inside the checkout because
 *   that is the only place the agent can write. That is the whole mechanism by which memory is
 *   appended: the bot is told where its memory is and asked to keep it current, and it edits the
 *   file with its ordinary write tool. Nothing here parses what a model said; the change the agent
 *   applied is the record. What that write is *gated* on is below, and is not what it looks like.
 *
 * ## What a memory write is actually gated on
 *
 * A memory write does **not** always stop and ask. It goes through the agent's ordinary write gate
 * (`Policy::write_needs_approval` upstream), whose rule is about *integrity* rather than about
 * which file it is: trusted data going to a trusted path is written without a prompt, because for
 * data to be trusted the turn must have observed nothing untrusted, and the destination only gains
 * trust by it.
 *
 * Both halves are true of a bot's memory in the ordinary case. The destination is trusted because
 * this app *names* the file, and naming it is what `policy.vouch_for_named_path` does; a turn that
 * only read the checkout has observed nothing untrusted. So a bot exploring its project and writing
 * down what it found does so silently, and the record of it is the `Update` line in the transcript
 * and the row in the Writes panel rather than a card somebody pressed.
 *
 * The prompt appears exactly where it matters: a turn that *has* touched untrusted content — a
 * fetched page, a command's output, a quarantined file — is asked before it may write to memory,
 * because that write would turn a trusted path untrusted. The gate is on prompt injection reaching
 * the memory, not on the memory changing.
 *
 * This was written the other way round first, and the briefing handed to the model said every edit
 * would be shown as a diff before it happened. That was false, and a false promise in a briefing is
 * worse than none: it is the model telling somebody something the app does not do. Tightening it is
 * not available from here — there is no "always ask about this path" upstream, and adding one would
 * be a change to a repository this app does not modify. So the briefing now says what is true: the
 * edit is on the record rather than in front of a card.
 *
 * ## When a bot is asked to write
 *
 * Nothing above makes a bot *remember*; it only makes remembering cheap once it decides to. The
 * instruction to do it lives in one place — the ground file — and the ground file reaches a turn
 * only when that turn is grounded, which is the first of a session and the first after each
 * compaction. Everything between those carried no reminder at all, and in practice a memory only
 * changed when somebody asked for it in so many words.
 *
 * Two things close that, and neither of them attaches anything to an ordinary turn:
 *
 * - **A compaction is answered with a turn of this app's own.** A rise in `archived` is the one
 *   moment memory is unambiguously *for*, since it is the only thing that survived it. The main
 *   process sends a grounded turn saying so — see `consolidationPrompt` — instead of waiting for
 *   the user's next prompt to carry the briefing. That prompt would have carried it anyway, so
 *   what this costs is a round trip and not an extra attachment.
 * - **A bot that has stopped writing is grounded early.** `quiet` counts turns since the memory
 *   file's mtime last moved, and at `QUIET_MAX` the next turn is grounded whether the window
 *   thought so or not, with one extra paragraph in the briefing. It resets on the nudge as well as
 *   on a write, so ignoring it buys silence rather than a briefing on everything.
 *
 * Both are honest about what they can do. Neither checks that a model wrote anything, because
 * checking would mean parsing what it said, and the one rule this file has is that the change the
 * agent applied is the record.
 *
 * One file goes to the turn rather than two, and that is not tidiness. Every attached file is
 * pushed into the conversation as its own user message, and the agent's compaction keeps only the
 * last two of those verbatim — so handing over two would mean the window a compaction preserves is
 * spent entirely on this app's own injections. The memory is therefore *copied into* the ground
 * file rather than attached beside it.
 *
 * ## Why the files are re-made before every turn
 *
 * A file a turn names and cannot read does not degrade: the read is a `?` all the way out to a
 * failed turn. A memory file removed by a `git clean`, a checkout switched to a branch that never
 * had one, an editor saving over it with something that is not text — each of those would brick
 * the bot rather than cost it a paragraph. So `ground` runs on the way into every send and repairs
 * what is missing, rather than once when the bot was made.
 */

import { app } from 'electron'
import { mkdirSync, readFileSync, writeFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { type Bot, botOf, CONSOLIDATION_MARK, isSlug, withBot } from '../shared/bots'
import { putBots, readState } from './state'

/** Where a bot's files live inside the checkout it works in, relative to that checkout. */
const HOME = '.bravebot-ui'

/**
 * The largest memory this will copy into a ground file.
 *
 * A cap rather than trust, because the memory is the one part of the ground file the planner
 * wrote, and a runaway one would be re-read at the top of every re-grounded turn — paid for in
 * context, forever, by the person whose session it is. Well past any memory worth keeping, and far
 * short of anything that could crowd out a conversation.
 */
const MEMORY_MAX = 64 * 1024

/**
 * How many of a bot's turns may finish without its memory changing before it is reminded.
 *
 * A briefing costs one attachment, and every attachment stays in the conversation and is counted
 * against the two a compaction keeps verbatim — so this cannot be one. It also cannot be very
 * large, because the thing it guards against is a bot that has quietly stopped writing anything
 * down, and by the time a conversation is thirty turns old the material worth keeping has already
 * gone past. Six is roughly a working exchange: long enough that a bot doing the job is never
 * interrupted, short enough that one that has stopped is caught inside the same sitting.
 *
 * It is a nudge and not a demand. Nothing here can make a model write, and nothing here checks
 * that it did — what the reminder buys is the instruction being in front of it again.
 */
const QUIET_MAX = 6

/** Every bot defined. Never throws; an unreadable file is no bots. */
export function bots(): Bot[] {
  return readState().bots
}

/** The one with this slug, or `null`. */
export function bot(slug: unknown): Bot | null {
  return isSlug(slug) ? botOf(bots(), slug) : null
}

/** Write a bot down, replacing whatever shared its slug, and stamp when that happened. */
export function saveBot(next: Bot): void {
  putBots(withBot(bots(), { ...next, updated: Date.now() }))
}

/** Record the latest durable conversation without discarding any earlier IDs. */
export function noteBotSession(slug: string, id: string): void {
  const held = bot(slug)
  if (!held) return
  saveBot({ ...held, session: id, conversations: [...new Set([...held.conversations, id])] })
}

/**
 * Note how much compaction has taken out of a bot's session.
 *
 * Monotonic, and treated as such: a lower figure than the one already stored describes a
 * conversation this is not looking at, and adopting it would ask for a re-grounding that nothing
 * happened to justify.
 */
export function noteBotArchived(slug: string, archived: number): void {
  const held = bot(slug)
  if (!held || !Number.isInteger(archived) || archived <= held.archived) return
  saveBot({ ...held, archived })
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
export function releaseBotSession(slug: unknown): void {
  const held = bot(slug)
  if (!held || held.session === null) return
  saveBot({ ...held, session: null, archived: 0 })
}

/**
 * Put a bot away, or take it back out.
 *
 * Nothing moves. The ground file is where this process left it, the memory file is where the
 * agent left it, the session is where the agent keeps every session — and that is the whole
 * reason coming back is one field changing rather than a bot being rebuilt. A rebuilt bot would
 * be a different bot wearing the same name: a new slug, so a different memory file, and a new
 * seed, so a different face.
 *
 * The window may ask for this and cannot say what the field becomes, which is the arrangement
 * `releaseBotSession` above has and for the same reason.
 */
export function retireBot(slug: unknown, retired: boolean): Bot | null {
  const held = bot(slug)
  if (!held) return null
  const next = { ...held, retired: retired ? Date.now() : 0 }
  saveBot(next)
  return next
}

/** Where this app keeps its own files for a bot. Composed from a slug that has been judged. */
function ownDirectory(slug: string): string {
  return join(app.getPath('userData'), 'bots', slug)
}

/** Where a bot's memory sits inside its checkout, as the agent would name it. */
export function memoryPath(slug: string): string {
  return `${HOME}/bots/${slug}.md`
}

/** The same, absolutely, for this process to read and seed. */
function memoryFile(directory: string, slug: string): string {
  return join(directory, HOME, 'bots', `${slug}.md`)
}

/**
 * When a bot's memory file last changed, in milliseconds, or `0` if there is nothing to look at.
 *
 * Deliberately the mtime and not the contents. Whether the memory has moved is a question about
 * the file, and answering it by reading and comparing 64K of text on the end of every turn would
 * be paying a great deal to learn something the filesystem already knows. A file that has been
 * rewritten with the same words counts as changed, which is the harmless direction to be wrong in:
 * it costs one nudge that was not needed.
 */
function memoryStamp(bot: Bot): number {
  try {
    return statSync(memoryFile(bot.directory, bot.slug)).mtimeMs
  } catch {
    return 0
  }
}

/**
 * Note whether a bot's memory moved during the turn that has just finished.
 *
 * Called from the one place that sees a bot's turn end. Two outcomes and no third: the file is
 * newer than the mark, so the bot wrote and the count goes back to nothing — or it is not, and one
 * more turn has gone by without it.
 *
 * A stamp that has gone *backwards* is adopted rather than ignored, unlike `archived` above. That
 * is a different kind of figure: an archive only rises, so a fall means somebody is describing
 * another conversation, where an mtime falls perfectly ordinarily when a file is restored from a
 * checkout or a branch is switched. What matters is only that it differs from the mark.
 */
export function noteBotMemory(slug: string): void {
  const held = bot(slug)
  if (!held) return
  const stamp = memoryStamp(held)
  if (stamp !== held.remembered) saveBot({ ...held, remembered: stamp, quiet: 0 })
  else saveBot({ ...held, quiet: held.quiet + 1 })
}

/**
 * Whether this bot has gone long enough without writing to be handed its briefing again.
 *
 * Asked on the way into a send, of a turn the window did not think needed grounding.
 */
export function nudgeDue(bot: Bot): boolean {
  return bot.quiet >= QUIET_MAX
}

/**
 * Note that a bot has just been reminded, so it is not reminded again on the next turn.
 *
 * Reset on the *nudge* rather than on a write, which is what makes this self-quieting in both
 * directions. A bot that takes the hint resets through `noteBotMemory` and never comes back here;
 * one that ignores it gets another `QUIET_MAX` turns of peace rather than a briefing attached to
 * everything it is ever asked, which is the failure that would make this worse than nothing.
 */
export function noteBotNudged(slug: string): void {
  const held = bot(slug)
  if (!held || held.quiet === 0) return
  saveBot({ ...held, quiet: 0 })
}

/**
 * What this app says to a bot when it sends a turn nobody typed.
 *
 * Written as one short instruction rather than as a briefing, because the briefing is attached
 * alongside it and saying the same thing twice in one turn is how a model learns to skim both. It
 * opens with `CONSOLIDATION_MARK` so that a transcript — this run's, or one reopened next year —
 * can tell it from something a person asked for.
 *
 * `why` is the sentence that differs between the occasions this is sent, and is the only part a
 * caller supplies. Nothing here interpolates anything a model said.
 */
export function consolidationPrompt(bot: Bot, why: string): string {
  return [
    CONSOLIDATION_MARK,
    '',
    why,
    '',
    `Look back over this conversation and bring \`${memoryPath(bot.slug)}\` up to date: add what`,
    'has turned out to be durable — a decision and why, a constraint, how something here is',
    'arranged — and prune whatever has stopped being true. If nothing in it needs changing, say so',
    'in one line and change nothing; an honest "no" is a better answer than an invented entry.',
    '',
    'Do not do any other work in this turn, and do not answer whatever was being discussed.',
    '',
  ].join('\n')
}

/** Why a consolidation was sent after a compaction, in the one sentence that turn opens with. */
export const AFTER_COMPACTION =
  'Your conversation has just been summarised, and the detail behind that summary is now the only' +
  ' thing your memory can still be written from.'

/** What a memory file says before anything has been remembered in it. */
function emptyMemory(bot: Bot): string {
  return [
    `# ${bot.name} — memory`,
    '',
    'Written by the bot itself, and shown in the transcript each time it changes. Anything here',
    'is carried into every conversation it has; anything not here is forgotten when the',
    'conversation is compacted.',
    '',
    'Nothing remembered yet.',
    '',
  ].join('\n')
}

/**
 * What the bot is handed at the top of a grounded turn.
 *
 * Written as a briefing rather than as a set of commands, because that is what it will arrive as.
 * The agent shows an attached file to the planner under a line saying whose contents these are, so
 * this is read as a document somebody handed over — which is the strongest framing available
 * without changing the agent, and an honest one: it *is* a document somebody handed over.
 */
function groundText(bot: Bot, memory: string, nudge: boolean): string {
  return [
    `# ${bot.name}`,
    '',
    'You are working as this bot for the whole of this session. What follows is who that is.',
    '',
    '## Purpose',
    '',
    bot.purpose.trim(),
    '',
    '## Memory',
    '',
    `Your memory is the file \`${memoryPath(bot.slug)}\` in this checkout. It is the only thing`,
    'about you that survives a compaction, so when you learn something durable — a decision and',
    'why, a constraint, how something here is arranged — edit that file to say so as you go, in',
    'the same turn you learnt it, rather than waiting to be asked. Keep it short enough to stay',
    'worth reading: prune what has stopped being true rather than only appending.',
    '',
    // Said plainly because it is true, where the sentence this replaced — that every edit would be
    // shown as a diff first — was not. The write gate is about integrity rather than about which
    // file it is, so an ordinary memory write goes through without a card. A briefing that tells a
    // model the app will ask first is the model telling somebody something the app does not do.
    'Editing it does not usually stop to ask, though every edit is on the record: the transcript',
    'draws it and the Writes panel lists it. Write it as something the user would want to read',
    'back, because they will.',
    ...(nudge
      ? [
          '',
          'You have not changed it in a while. Before going further, consider whether anything you',
          'have learnt since is worth keeping — and leave it alone if it is not.',
        ]
      : []),
    '',
    'It currently says:',
    '',
    '---',
    '',
    memory.trim(),
    '',
  ].join('\n')
}

/** What a checkout's `.bravebot-ui` says about itself, so it does not become somebody's diff. */
const GITIGNORE = [
  '# Written by bravebot-ui. This folder holds the memory of the bots that work in this',
  '# checkout. It ignores itself so it never becomes a change nobody made.',
  '*',
  '',
].join('\n')

/** Whether a path is a file this process can read as text, which is what a turn will need of it. */
function readable(path: string): string | null {
  try {
    if (!statSync(path).isFile()) return null
    const text = readFileSync(path, 'utf8')
    // A file whose bytes are not text comes back with replacement characters rather than an error,
    // and the agent would refuse it where this did not. Cheaper to notice here, where the answer
    // is to write a fresh one, than to spend a turn finding out.
    return text.includes('�') ? null : text
  } catch {
    return null
  }
}

/** Both paths a grounded turn names, with both files known to exist and to be readable. */
export interface Grounding {
  /** The ground file, absolute, for `dropped`. */
  ground: string
  /** The memory file, relative to the checkout, for `files`. */
  memory: string
}

/**
 * Make a bot's files current, and say where they are.
 *
 * Returns `null` when the checkout cannot be written to at all — a volume that is not mounted, a
 * directory somebody deleted. That is a refusal rather than a repair: sending the turn anyway
 * would fail inside the agent with a message about a path, where this can say the bot's checkout
 * is gone.
 */
export function ground(bot: Bot, nudge = false): Grounding | null {
  const memory = memoryFile(bot.directory, bot.slug)
  try {
    mkdirSync(join(bot.directory, HOME, 'bots'), { recursive: true })
    if (readable(join(bot.directory, HOME, '.gitignore')) === null) {
      writeFileSync(join(bot.directory, HOME, '.gitignore'), GITIGNORE, 'utf8')
    }
    let held = readable(memory)
    if (held === null) {
      held = emptyMemory(bot)
      writeFileSync(memory, held, 'utf8')
    }

    const ground = join(ownDirectory(bot.slug), 'ground.md')
    mkdirSync(ownDirectory(bot.slug), { recursive: true })
    writeFileSync(ground, groundText(bot, held.slice(0, MEMORY_MAX), nudge), 'utf8')
    return { ground, memory: memoryPath(bot.slug) }
  } catch {
    return null
  }
}

/** What a bot's memory says, for showing it in the window. Never the path, only the words. */
export function memory(slug: unknown): string | null {
  const held = bot(slug)
  if (!held) return null
  return readProjectText(held.directory, memoryPath(held.slug), MEMORY_MAX)?.text ?? null
}
