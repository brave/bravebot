import { readProjectText, seedProjectMemory } from './project-files'
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
 * - **The ground file**, `<userData>/bots/<slug>/ground.md`, is composed here from the bot's name
 *   and its purpose. It is handed to a turn as `dropped`, which is the read that is deliberately
 *   *not* confined to the workspace. It lives outside the checkout precisely so the planner cannot
 *   rewrite the thing that defines it: the agent may write inside the workspace and nowhere else,
 *   and this is nowhere else.
 * - **The memory file**, `<directory>/.bravebot-ui/bots/<slug>.md`, is inside the checkout because
 *   that is the only place the agent can write. That is the whole mechanism by which memory is
 *   appended: the bot is told where its memory is and asked to keep it current, and it edits the
 *   file with its ordinary write tool. Nothing here parses what a model said; the change the agent
 *   applied is the record. What that write is *gated* on is below, and is not what it looks like.
 *
 * Only the ground file is handed over, and it quotes no byte of the memory. `ground` below says
 * why: a path this app names is a path the agent records as vouched for, so the only ones it may
 * name are ones whose every byte it wrote.
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
 * the person vouched for the checkout the memory sits in, and a turn that only read that checkout
 * has observed nothing untrusted. So a bot exploring its project and writing down what it found
 * does so silently, and the record of it is the `Update` line in the transcript and the row in the
 * Writes panel rather than a card somebody pressed.
 *
 * The prompt appears exactly where it matters: a turn that *has* touched untrusted content — a
 * fetched page, a command's output, a quarantined file — is asked before it may write to memory,
 * because that write would turn a trusted path untrusted. The gate is on prompt injection reaching
 * the memory, not on the memory changing.
 *
 * And the path stays untrusted afterwards, which is the half this app used to undo: it named the
 * memory on every grounded turn, and naming is what `policy.vouch_for_named_path` records, so the
 * rule the write had written was overwritten by a grant nobody was asked for. It no longer names
 * it. See `ground`.
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
 * One file goes to the turn rather than two, and the trust argument in `ground` is not the only
 * reason. Every attached file is pushed into the conversation as its own user message, and the
 * agent's compaction keeps only the last two of those verbatim — so handing over two would mean
 * the window a compaction preserves is spent entirely on this app's own injections. The memory is
 * read by the bot instead, which costs a call and spends none of that window.
 *
 * ## Why the files are re-made before every turn
 *
 * A memory file removed by a `git clean`, or a checkout switched to a branch that never had one,
 * leaves the bot with nothing where its memory should be. So `ground` runs on the way into every
 * send and makes the missing file, rather than once when the bot was made, and the making is the
 * helper's rather than this process's: see `seedProjectMemory`.
 *
 * Only the missing file. A memory that is there is left exactly as it is, including one an editor
 * saved over with bytes that are not text: that used to fail the turn, because the memory was a
 * path the turn *named* and a named file that cannot be read is a `?` all the way out, and now it
 * is a read the bot makes and a paragraph it does without. Replacing somebody's file to save a
 * paragraph is not a trade worth making.
 */

import { app } from 'electron'
import { randomUUID } from 'node:crypto'
import { lstatSync, mkdirSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { type Bot, botOf, isBotModel, isSlug, slugFor, withBot } from '../shared/bots'
import { newAvatarSeed } from '../shared/avatar'
import { isProjectPath } from '../shared/recents'
import { isOpenedDirectory } from './opened'
import { putBots, readState } from './state'

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

/**
 * The bot a window's form describes, or `null` if what it sent is not a bot this app may keep.
 *
 * Four fields cross from a window (a name, a purpose, a model and a face), and none of them is
 * a path. The fifth is, and it is the one field that decides where on the disk this app writes:
 * a bot's memory is made under its project folder by a helper pinned to that folder, and no
 * session and no prompt stands between a saved bot and that write. So the folder is checked
 * against the ones the picker handed out rather than for the shape of a path. `isProjectPath`
 * says a string is absolute and holds no NUL, which every folder on the account satisfies.
 *
 * An existing bot keeps everything this road cannot say: its id, its watermark, its seed, its
 * folder, when it was made. Its folder is fixed for the reason the form gives, to keep a bot's
 * memory and its conversations together, and this never reads the one it was sent. A new one is
 * given a slug composed here from the name, so the thing that becomes a path segment is never a
 * string that arrived as one.
 */
export function botFromForm(value: unknown): Bot | null {
  if (typeof value !== 'object' || value === null) return null
  const { slug, avatar, model, name, purpose, directory } = value as Record<string, unknown>
  if (model !== undefined && !isBotModel(model)) return null
  if (typeof name !== 'string' || typeof purpose !== 'string') return null
  if (!name.trim() || !purpose.trim()) return null
  if (!isProjectPath(directory)) return null
  if (avatar !== undefined && (typeof avatar !== 'string' || !avatar.trim() || avatar.length > 128)) {
    return null
  }

  const held = isSlug(slug) ? bot(slug) : null
  if (held) return { ...held, name, purpose, model: model === undefined ? held.model : model }
  if (!isOpenedDirectory(directory)) return null
  return {
    slug: slugFor(name, new Set(bots().map((each) => each.slug))),
    name,
    purpose,
    model: typeof model === 'string' ? model : null,
    // Use the draft's preview seed so creation keeps the face already shown. Older callers may
    // omit it; either way it is stored and survives a rename.
    avatar: typeof avatar === 'string' ? avatar : newAvatarSeed(randomUUID()),
    directory,
    session: null,
    conversations: [],
    archived: 0,
    // Nothing has been remembered and nothing has gone unremembered, so a new bot starts owing no
    // nudge. See `noteBotMemory`, which takes its first reading when its first turn ends.
    remembered: 0,
    quiet: 0,
    // In use, which is what a bot somebody just filled in a form for is.
    retired: 0,
    created: Date.now(),
    updated: Date.now(),
  }
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
    // `lstat`, so a link at the memory path reports on itself rather than on whatever it aims at.
    // Nothing here would act on the answer, but a figure about a file outside the checkout has no
    // business being read at all, and the difference is one letter.
    return lstatSync(memoryFile(bot.directory, bot.slug)).mtimeMs
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
 * alongside it and saying the same thing twice in one turn is how a model learns to skim both.
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

/**
 * The longest a bot's name may be inside a file this composes.
 *
 * The form takes free text of any length, and the seed the helper writes is bounded at 64 KB, so
 * without this a name nobody would type is a bot whose every turn is refused. Far past a name and
 * far short of the bound.
 */
const NAME_MAX = 200

/** What a memory file says before anything has been remembered in it. */
function emptyMemory(bot: Bot): string {
  return [
    `# ${bot.name.slice(0, NAME_MAX)} — memory`,
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
function groundText(bot: Bot, nudge: boolean, fresh: boolean): string {
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
    // What this document deliberately does not do is quote the memory. The words in it are the
    // model's own writing, and a copy of them inside a file the app vouches for would be the app
    // answering, on the user's behalf and without asking, a question the agent is there to decide.
    // So the file is read rather than quoted, and what the read comes back as is the trust map's
    // answer about that path.
    //
    // `fresh` is the one case where there is nothing to read: the file was made a moment ago by
    // the walk that prepared this briefing, so sending the model to open a template it would
    // learn nothing from costs a call for no answer.
    ...(fresh
      ? ['It is new and holds nothing yet, so there is nothing to read back.']
      : [
          'Read that file now, before anything else. It is not quoted here: what is in it is your',
          'own writing rather than anything this window wrote, so you read it on the same terms as',
          'any other file in this checkout. If it comes back withheld, say so and carry on without',
          'it rather than guessing at what it used to say.',
        ]),
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
  ].join('\n')
}

/** What a checkout's `.bravebot-ui` says about itself, so it does not become somebody's diff. */
const GITIGNORE = [
  '# Written by bravebot-ui. This folder holds the memory of the bots that work in this',
  '# checkout. It ignores itself so it never becomes a change nobody made.',
  '*',
  '',
].join('\n')

/**
 * The one path a grounded turn names, with the file behind it known to exist.
 *
 * One path and not two. The memory file is deliberately absent: see `ground` below.
 */
export interface Grounding {
  /** The ground file, absolute, for `dropped`. */
  ground: string
}

/**
 * Make a bot's files current, and say where the one a turn may name is.
 *
 * Returns `null` when the checkout cannot be prepared: a volume that is not mounted, a directory
 * somebody deleted, or a link where the memory file belongs. That is a refusal rather than a
 * repair: sending the turn anyway would fail inside the agent with a message about a path, where
 * this can say the bot's checkout is gone.
 *
 * ## What this hands over, and what it refuses to
 *
 * The briefing, and nothing else. `turn.send` admits every path it is given as *trusted* context,
 * which is the agent recording that a person named the file in their own line, so the only paths
 * this may name are ones whose every byte this process wrote. The briefing is one: it is
 * composed from the bot's name and purpose, which somebody typed into this window, and from the
 * memory's *path*, which is a string this file builds out of a slug it judged.
 *
 * The memory is not one, and it was handed over twice. It was named in `files`, and its body was
 * copied into the briefing, a fresh path under this app's own data directory that the trust map
 * has never heard of and that a copy therefore laundered. Both of those vouched, on a
 * person's behalf and without asking them, for text the model itself wrote. That undid the gate
 * the write went through: a turn that has touched untrusted content is asked before it may write
 * to the memory *because that write leaves the path untrusted*, and the next grounded turn
 * vouched for it again regardless, so a fetched page's bytes came back as trusted context and
 * stayed there.
 *
 * So the memory reaches a turn the way any other file in the checkout does: the briefing says
 * where it is, and the model reads it with its ordinary read tool, under whatever the agent's
 * trust map says about that path. A memory a write left untrusted comes back quarantined, which is
 * the outcome the gate was for. Whether it may be trusted is the agent's question, and this is how
 * it gets asked.
 */
export function ground(bot: Bot, nudge = false): Grounding | null {
  try {
    // Through the confined helper rather than `node:fs`. A concatenated path handed to `node:fs`
    // follows a link at every component, so a link at the memory file was read through and
    // written through; the helper opens each component relative to a pinned directory and follows
    // nothing. It answers only whether it wrote, because what the memory *says* has no business
    // in a file this process composes.
    const fresh = seedProjectMemory(bot.directory, memoryPath(bot.slug), emptyMemory(bot), GITIGNORE)

    const ground = join(ownDirectory(bot.slug), 'ground.md')
    mkdirSync(ownDirectory(bot.slug), { recursive: true })
    // Written to a name of its own and renamed into place, never opened by the name the turn will
    // name. `writeFileSync` on the briefing's own path follows a link sitting there and writes the
    // target instead; a rename replaces whatever is at the name, so a link there is displaced
    // rather than written through, and what the turn reads is what this process wrote.
    // A name nothing else will pick, so two sends for one bot cannot collide, and an exclusive
    // create so a link left at that name is refused rather than written through. `rm` first
    // because `wx` on a leftover of our own, from a process that died between the write and the
    // rename, would otherwise refuse this bot's turns for good; it unlinks a name and never
    // follows one.
    const pending = `${ground}.${randomUUID()}.tmp`
    rmSync(pending, { force: true })
    writeFileSync(pending, groundText(bot, nudge, fresh), { encoding: 'utf8', mode: 0o600, flag: 'wx' })
    try {
      renameSync(pending, ground)
    } catch (error) {
      rmSync(pending, { force: true })
      throw error
    }
    return { ground }
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
