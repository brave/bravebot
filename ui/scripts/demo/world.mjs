// A world to film in, so nothing real ends up in the video.
//
// A demo of this app is a demo of somebody's session list, and a session list is a list of
// what they have actually been doing: real project names, real prompts, real paths, and — in
// the file tree — the real contents of a real directory. None of that belongs in a recording
// that leaves the machine. So the demo does not film the machine.
//
// `$HOME` is most of it, and not all of it. The agent finds `~/.bravebot` — sessions, history,
// standing instructions — by reading `HOME` and nothing else (`crates/agent/src/home.rs`), so
// pointing that at a directory of ours sanitises everything the *agent* holds. This app's own
// remembered state — the recents list especially, which is real project paths and is filmed in
// File ▸ Open Recent — is a separate matter: on macOS Electron derives `userData` from the
// password database rather than from the environment, so it needs `--user-data-dir` as well.
// Both are set, here and in `stage.mjs`. Point them at a directory of ours and the app comes up
// in a world containing exactly what this file put there: two invented checkouts, and the
// sessions the demo earned in them.
//
// **The sessions are earned rather than written.** Seeding them by hand would mean encoding
// the agent's own on-disk record format here — a format that is upstream, is not ours, and is
// rewritten after every turn. A demo that hard-coded it would break silently on a bump, months
// later, in a recording somebody was about to publish. So the world is built by driving the
// product: real prompts against the fixture checkouts, once, cached in the world for every run
// afterwards. It costs a few tokens the first time and nothing after that.
import { _electron as electron } from 'playwright-core'
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))

/** The checked-in templates. Copied into the world rather than filmed in place, so a turn
 *  that edits a file edits the copy and this checkout stays clean. */
const TEMPLATES = join(here, 'project')

/**
 * Outside the repository, so `git clean` does not throw away sessions that cost tokens — and
 * outside the home directory, because every path in this world ends up on screen. The
 * transcript header, the file tree's root and the session list's second line all show one, and
 * a home directory has somebody's name in it. `/Users/Shared` is a real, persistent, writable
 * macOS directory whose path says nothing about whose machine this is. `--world` overrides it.
 */
export const DEFAULT_WORLD = '/Users/Shared/bravebot-ui-demo'

/**
 * What the world is asked for, in the order the video wants it.
 *
 * Two checkouts, because grouping the session list by checkout is one of the things being
 * demonstrated and one heading is not a demonstration of grouping. Two prompts in the first,
 * because forking cuts in front of the second.
 *
 * The prompts are chosen to be cheap, to be safe, and to leave the right evidence behind: a
 * markdown table for the formatting scene, files read for the context panels, a command for
 * the approval card, and enough tool calls to make a run worth folding.
 */
const RECIPES = [
  {
    project: 'harbour-lights',
    prompts: [
      'Read src/lib/tides.ts and src/App.tsx, then describe what this app does in a short ' +
        'markdown table of its components and what each is for. Do not change anything.',
      'Which file under src/ is the longest, and what is in it? Do not change anything.',
    ],
  },
  {
    project: 'tide-tables',
    prompts: [
      'Run `wc -l src/*.ts` here and tell me the total number of lines. Do not change anything.',
    ],
  },
]

/** Where the world's copies of the fixture checkouts live. */
export const projectsIn = (world) =>
  RECIPES.map((recipe) => join(world, 'projects', recipe.project))

/**
 * The bots that live in the world before the bots scene makes one of its own.
 *
 * A list of one is not a list, and the scene's own bot is made and unmade every take. These two
 * stay, so the tab opens on a column with faces already in it and the one made on camera joins
 * them — three faces, told apart at a glance, which is the claim the faces make. Each in one of
 * the two checkouts, so the second line under a name differs too.
 *
 * One of them has been spoken to and the other has not, and that is the point of having two. A
 * bot with a session looks slowly about in the list where one that has never spoken faces forward
 * and waits; a bot that has been asked to remember something has a memory file with words in it
 * where a new one says "nothing remembered yet"; and the session behind it holds the record of the
 * write — the `Update` line naming its own memory file. All three are things the bots scene can
 * point at without spending a token, so long as the world has them, and none of them can be shown
 * with a bot made a moment ago.
 *
 * Written through the window's own channel rather than into `bravebot-ui.json`, so the slug, the
 * seed and the rest are made the way the app makes them; and by `ensureWorld` on every launch
 * rather than by the seeding run alone, since a definition is four fields and costs no turn, so
 * there is no reason to tie it to a `--rebuild`. The one turn Night Watch is owed is earned the
 * same way the sessions are — driven once, in the world, and kept — and only when it has no
 * session yet. It has to happen before the filmed window opens: the list is read when the window
 * mounts and again when a turn ends, and a bot written between the two is not in it until then.
 */
export const RESIDENT_BOTS = [
  {
    name: 'Release Notes',
    project: 'harbour-lights',
    purpose:
      'You write the release notes for the harbour-lights checkout. Keep them short, in the past ' +
      'tense, and grouped by what a user would notice.',
    // Never spoken to. The one that waits.
    prompts: [],
  },
  {
    name: 'Night Watch',
    project: 'tide-tables',
    purpose:
      'You look after the tide-tables checkout overnight. Say what changed since yesterday and ' +
      'whether any of it needs a person.',
    // One turn, and it names the memory file outright for the reason `13-bot-memory` gives: asked
    // to "remember", a bot may say it will and write nothing, which is a fine answer and no record.
    // Asked for a line in the file, it writes one, and the write is what the scene points at.
    prompts: [
      'Run `ls data` here, then add one line to your memory file: the tables under data/ are ' +
        'generated, and a failure there overnight is the one thing that needs a person by morning. ' +
        'Change nothing else.',
    ],
  },
]

/** Has anything been filmed here before? A world with no sessions is a world with no video. */
const seeded = (world) => {
  const sessions = join(world, '.bravebot', 'sessions')
  try {
    return readdirSync(sessions).some((name) => !name.startsWith('.'))
  } catch {
    return false
  }
}

/** Lay out the checkouts. Cheap, and idempotent, so it runs on every launch. */
function layOut(world, { rebuild }) {
  if (rebuild && existsSync(world)) rmSync(world, { recursive: true, force: true })
  mkdirSync(join(world, 'projects'), { recursive: true })
  for (const recipe of RECIPES) {
    const to = join(world, 'projects', recipe.project)
    if (!existsSync(to)) cpSync(join(TEMPLATES, recipe.project), to, { recursive: true })
  }
}

/**
 * Bring the world up to something worth filming, building the sessions if it has none.
 *
 * Returns `null` for the sessions when it could not build them — no credentials, a refusal, a
 * turn that never finished — rather than throwing. A world with checkouts and no sessions is
 * still a world the file-tree scene can be filmed in, and the scenes that need a transcript
 * already know how to bow out.
 */
export async function ensureWorld(world, opts) {
  layOut(world, opts)
  if (seeded(world) && !opts.rebuild) {
    await ensureResidents(world)
    return { world, built: false }
  }

  console.log(`building the demo world in ${world}`)
  console.log('  this drives real turns against the fixture checkouts, once — later runs reuse them')
  const built = await build(world)
  await ensureResidents(world)
  return { world, built }
}

/**
 * What the residents still lack: the ones not defined yet, and the ones owed a turn that have no
 * session. A read of this app's own state file, which is ours, not the agent's record format.
 */
function residentsWanting(world) {
  let bots = []
  try {
    const state = JSON.parse(readFileSync(join(world, 'userData', 'bravebot-ui.json'), 'utf8'))
    bots = state.bots ?? []
  } catch {
    // No state yet, or none that parses: everything is wanting, which is the right answer.
  }
  const missing = RESIDENT_BOTS.filter((bot) => !bots.some((have) => have.name === bot.name))
  const unspoken = RESIDENT_BOTS.filter(
    (bot) => bot.prompts.length && !bots.some((have) => have.name === bot.name && have.session),
  )
  return { missing, unspoken }
}

/**
 * Bring the resident bots up to what the scene expects: defined, and the one owed a session with
 * it. Launches the app to do both through the window, and closes it again. A few seconds when a
 * definition is missing, a turn's worth when the session is, and nothing at all otherwise.
 */
async function ensureResidents(world) {
  const { missing, unspoken } = residentsWanting(world)
  if (!missing.length && !unspoken.length) return
  if (missing.length) console.log(`  adding ${missing.map((bot) => bot.name).join(' and ')} to the world's bots`)

  const app = await electron.launch({
    args: ['.', `--user-data-dir=${join(world, 'userData')}`],
    cwd: process.cwd(),
    timeout: 40000,
    env: { ...process.env, HOME: world },
  })
  try {
    let page = await app.firstWindow()
    await page.waitForLoadState('domcontentloaded')
    page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
    await page.waitForTimeout(1500)

    if (missing.length) {
      const homes = Object.fromEntries(
        missing.map((bot) => [bot.name, join(world, 'projects', bot.project)]),
      )
      await page.evaluate(
        async ([bots, homes]) => {
          for (const bot of bots) {
            await window.bravebot.writeBot({ name: bot.name, purpose: bot.purpose, directory: homes[bot.name] })
          }
        },
        [missing, homes],
      )
      await page.waitForTimeout(500)
      if (unspoken.length) {
        // The list is read when the window mounts, so a bot written since is not in it to be
        // opened. Nothing is filmed here, so the window can simply start again.
        await page.reload()
        await page.waitForLoadState('domcontentloaded')
        await page.waitForTimeout(2000)
      }
    }

    if (unspoken.length) {
      if (await page.locator('.unconfigured').isVisible().catch(() => false)) {
        console.log('  the agent has no credentials built in, so the residents stay unspoken to')
        return
      }
      await page.locator('.sidebar-tab').nth(1).click()
      await page.waitForTimeout(600)
      for (const bot of unspoken) {
        const row = page
          .locator('.bot')
          .filter({ has: page.locator('.bot-name', { hasText: new RegExp(`^${bot.name}$`) }) })
        if (!(await row.count())) {
          console.log(`  ${bot.name} is not in the list; leaving it unspoken to`)
          continue
        }
        await row.locator('.bot-open-button').click()
        await page.waitForTimeout(1600)
        if (await page.locator('.trust').isVisible().catch(() => false)) {
          await page.locator('.trust-actions .approve').click()
          await page.waitForTimeout(800)
        }
        for (const prompt of bot.prompts) {
          console.log(`  ${bot.name}: ${prompt.slice(0, 62)}…`)
          await page.locator('.composer textarea').fill(prompt)
          await page.locator('.composer .send').click()
          if (!(await settle(page))) {
            console.log('    the turn did not finish in time; keeping what it managed')
            break
          }
        }
        // The bots tab again, for the next one — opening a bot shows its transcript and leaves
        // the sessions list in the column.
        await page.locator('.sidebar-tab').nth(1).click()
        await page.waitForTimeout(400)
      }
    }
  } finally {
    await app.close()
  }
}

/** Everything the seeding run needs to know about a turn that is still going. */
const busy = (page) => page.locator('.working').isVisible().catch(() => false)

/**
 * Drive a turn to a standstill, answering whatever it stops to ask.
 *
 * Approvals are given here without ceremony, which is the one place in this repository that
 * happens. It is defensible only because of where it happens: the agent is confined to a
 * checkout this file laid out itself, from templates in this repository, and the prompts it
 * was given ask it to read and to count. It is not a pattern to copy anywhere a real
 * directory is involved.
 */
async function settle(page, seconds = 180) {
  const until = Date.now() + seconds * 1000
  // `:has(.confirm-actions)` because a card that has been answered stays in the transcript —
  // that is the record of the decision — and `.confirm` alone would keep matching it forever.
  const asking = page.locator('.confirm:has(.confirm-actions)')
  while (Date.now() < until) {
    const card = asking.first()
    if (await card.isVisible().catch(() => false)) {
      // A series of questions has to be answered before it can be sent; everything else is a
      // single button. `:not(.always)` because vouching is a broader answer than was asked for.
      for (const block of await card.locator('.ask-question').all()) {
        const choice = block.locator('.choices .choice').first()
        if (await choice.count()) await choice.click()
        else await block.locator('.typed').first().fill('whatever you think best')
      }
      const approve = card.locator('.confirm-actions .approve:not(.always)').first()
      await (await approve.count() ? approve : card.locator('.confirm-actions .approve').first()).click()
      await page.waitForTimeout(800)
      continue
    }
    if (!(await busy(page))) {
      // Settled — but a turn pauses between steps, so this waits to be sure rather than
      // taking the first quiet frame as the end of it.
      await page.waitForTimeout(2500)
      if (!(await busy(page)) && !(await asking.first().isVisible().catch(() => false))) {
        return true
      }
      continue
    }
    await page.waitForTimeout(1000)
  }
  return false
}

/** Launch the app inside the world and earn its sessions. */
async function build(world) {
  // Both redirections, for the reason `stage.mjs` gives at length: `HOME` moves the agent's own
  // store, and only `--user-data-dir` moves this app's. Without the second, seeding a world would
  // write the real recents list — the checkouts it opens here would appear in somebody's File ▸
  // Open Recent afterwards, which is the opposite of what a sanitised world is for.
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${join(world, 'userData')}`],
    cwd: process.cwd(),
    timeout: 40000,
    env: { ...process.env, HOME: world },
  })
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
  await page.waitForTimeout(2500)

  if (await page.locator('.unconfigured').isVisible().catch(() => false)) {
    console.log('  the agent has no credentials built in, so there are no turns to drive')
    console.log('  the checkouts are there; the scenes needing a transcript will bow out')
    await app.close()
    return false
  }

  let made = 0
  try {
    for (const recipe of RECIPES) {
      const directory = join(world, 'projects', recipe.project)
      // The picker is a native sheet and would hang this, so it answers itself. The path is
      // one this file laid out, not one the window composed — the same rule the app keeps.
      await app.evaluate(({ dialog }, where) => {
        dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [where] })
      }, directory)

      await page.locator('.new').first().click()
      await page.waitForTimeout(1500)
      if (await page.locator('.trust').isVisible().catch(() => false)) {
        await page.locator('.trust-actions .approve').click()
        await page.waitForTimeout(800)
      }

      for (const prompt of recipe.prompts) {
        console.log(`  ${recipe.project}: ${prompt.slice(0, 62)}…`)
        await page.locator('.composer textarea').fill(prompt)
        await page.locator('.composer .send').click()
        if (!(await settle(page))) {
          console.log('    the turn did not finish in time; keeping what it managed')
          break
        }
        made++
      }
    }
  } finally {
    await app.close()
  }

  console.log(made ? `  built ${made} turn${made === 1 ? '' : 's'}` : '  nothing was built')
  return made > 0
}
