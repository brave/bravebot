// Bots: the other list in the left column.
//
// Every scene before this one is about a *chat*, an occasion, named after whatever was asked
// first, and finished with. A bot is somebody who has them: a name, a purpose and a memory, with
// conversations started in any project or in none. So the shot is the tab first, because the
// column having two lists is the thing a viewer has to be told before anything else here makes
// sense.
//
// It films what a bot *is* and no more: the tab, the form, the face, and what the form holds. The
// face has more to it than this scene can show — it takes up a posture for what its bot is doing,
// and a bot made a moment ago is doing nothing — so the rest of that is in `13-bot-memory`, where
// there is a turn for it to react to.
//
// **It makes a bot on camera.** A bot needs no folder to be made: the app gives it a home of its
// own, and a project is picked for each conversation in the composer on the bot's page.
//
// The bot is removed before it is made, off camera, so every take films the same thing: a run
// against a world that already has one would otherwise skip the part worth watching. Nothing
// accumulates — but the world has two resident bots of its own (`RESIDENT_BOTS` in `world.mjs`),
// so the list the tab opens on is a list, and the bot made on camera is the third face in it
// rather than the only one.
import { rmSync } from 'node:fs'
import { join } from 'node:path'

const NAME = 'Harbour Watch'
const PURPOSE =
  'You look after the harbour-lights checkout. Keep an eye on what changes in it, ' +
  'and say what you would want to be told about it a week from now.'

export default {
  id: '12-bots',
  title: 'Bots',

  async run(s) {
    const { page } = s

    // Off camera: take away anything this pair of scenes left behind last time, so every take
    // films the same thing.
    //
    // The definition goes through the window's own channel — `removeBot` forgets a bot and leaves
    // its session and memory where they are, which is what the control in the window does too. The
    // *memory file* then has to be taken away here, from the disk, and it is the one that matters:
    // `13-bot-memory` asks the bot to remember two things, and a bot whose memory already says them
    // rightly writes nothing and answers that it knew. That is the bot behaving well and the take
    // being wasted, and the run reports it as "no memory write was offered" — which reads like a
    // broken feature and is not one.
    const gone = await page.evaluate(async (name) => {
      const bots = await window.bravebot.readBots()
      const taken = []
      for (const bot of bots) {
        if (bot.name !== name) continue
        taken.push({ slug: bot.slug, folders: [bot.home, ...bot.conversations.map((each) => each.directory)] })
        await window.bravebot.removeBot(bot.slug)
      }
      return taken
    }, NAME)
    for (const bot of gone) {
      for (const folder of bot.folders) rmSync(join(folder, '.bravebot-ui', 'bots', `${bot.slug}.md`), { force: true })
    }

    const tab = page.locator('.sidebar-tabs [role="option"]').nth(1)
    if (!(await tab.count())) s.skip('this build has no bots tab')

    await s.say('Bots', 'The left column holds two lists. This is the other one.', 2.2)
    await s.glideTo(tab)
    await s.click(tab)
    await page.waitForTimeout(700)
    await s.say('Bots', 'A chat is an occasion. A bot is somebody who has them.', 2.4)
    // Both lists are in the column and only one is shown; the one with bots in it is this one.
    const list = page.locator('.session-list:has(.bot)')
    const residents = page.locator('.bot')
    // The resident with a session, found by what the app recorded rather than by name, so a world
    // with a different resident works.
    const spokenName = await page.evaluate(async () => (await window.bravebot.readBots()).find((bot) => bot.session && bot.retired === 0)?.name ?? null)
    const spoken = residents
      .filter({ has: page.locator('.bot-name', { hasText: spokenName ?? '\u0000' }) })
      .first()
    if ((await residents.count()) > 1) {
      await s.spotlight(list, 1.6)
      await s.say('Each one', 'A face, a name, and what it is for.', 2.4)
      await s.unspot()
    }
    await s.shot('12-bots')

    // --- making one -----------------------------------------------------------------------

    await s.say('Make one', 'A name, and what it is for.')
    await s.click(page.locator('[data-test="new-bot"]'))
    await page.waitForTimeout(600)

    const form = page.locator('.bot-form')
    if (!(await form.count())) s.skip('the new-bot form did not open')

    await s.slowType(form.getByRole('textbox', { name: 'Name', exact: true }), NAME)
    await s.beat(0.5)
    await s.slowType(form.getByRole('textbox', { name: 'Purpose', exact: true }), PURPOSE)
    await s.beat(0.8)

    await s.say('No folder to choose', 'Each conversation picks its project. A folder in it holds the bot’s memory there, and is nobody’s diff.', 3.0)
    const note = page.locator('.bot-note')
    if (await note.count()) {
      await s.spotlight(note, 1.6)
      await s.unspot()
    }

    await s.click(form.locator('.bot-save'))
    await page.waitForTimeout(1200)

    const row = page
      .locator('.bot')
      .filter({ has: page.locator('.bot-name', { hasText: new RegExp(`^${NAME}$`) }) })
    if (!(await row.count())) s.skip('the bot was not added to the list')

    await s.say('And there it is', 'No conversations yet, a bot writes no session until it has something to say.', 2.6)
    await s.spotlight(row, 1.8)
    await s.unspot()
    await s.shot('12-bot-made')

    // --- the face -------------------------------------------------------------------------

    const face = row.locator('.bot-avatar')
    if (await face.count()) {
      await s.glideTo(face)
      await s.say('Every bot has a face', 'Built from a seed it is given once, so it is the same face tomorrow.', 2.6)
      await s.spotlight(face, 2.4)
      await s.unspot()
      // The whole column of faces, since telling them apart is what the faces are for and one
      // face proves nothing about that.
      const faces = page.locator('.bot .bot-avatar')
      if ((await faces.count()) > 1) {
        await s.spotlight(list, 2.0)
        await s.say('And no two alike', 'The head, what sits on it, the ears, the eyes, the body — and the colour.', 2.8)
        await s.unspot()
      }
      await s.say('One colour each', 'The parts told apart by shade of it, with a drawn edge so it reads on any theme.', 2.8)
      // The face knows what its bot is doing, and this one is doing nothing yet: a bot nobody has
      // spoken to faces forward and only blinks, where one with a session looks slowly about. Held
      // long enough for a blink to land — the one thing it does — so the stillness reads as a
      // posture rather than as a frozen frame. Then the resident that *has* a session, for the
      // contrast, if the world has one. `13-bot-memory` films the rest of what the face does,
      // because the rest needs a turn.
      await s.say('And it waits', 'A bot nobody has spoken to faces forward, and only blinks.', 2.8)
      await s.beat(1.4)
      const spokenFace = spoken.locator('.bot-avatar')
      if (await spoken.count()) {
        await s.glideTo(spokenFace)
        await s.say('This one has been spoken to', 'It has a session, so it looks slowly about. At work, it would look down.', 3.2)
        await s.beat(1.4)
      }
    }

    // --- what is in one -------------------------------------------------------------------

    await s.say('What a bot holds', 'Its page: its conversations, and beside them its purpose and its memory.')
    await s.click(row.locator('.bot-open-button'))
    await page.locator('[data-test="bot-details"]').waitFor()

    const purpose = page.locator('[data-test="bot-details"]').getByRole('textbox', { name: 'Purpose', exact: true })
    if (await purpose.count()) {
      await s.spotlight(purpose, 2.0)
      await s.unspot()
    }
    const memory = page.locator('[data-test="bot-details"] [data-test="bot-memory"]')
    if (await memory.count()) {
      await s.glideTo(memory)
      await s.say('Its memory', 'A real file in each folder it works in, empty until the bot has something to keep.', 3.0)
      await s.spotlight(memory, 2.2)
      await s.unspot()
    }

    // --- one that remembers ---------------------------------------------------------------

    // The resident with a session, which was asked once — off camera, when the world was built —
    // to put a line in its memory file. Its memory is a file with words in it, and its session
    // holds the record of the write: the `Update` line naming the file, drawn like any other
    // change. Both can be pointed at without a turn, which is why the world keeps a bot like this.
    // Bows out where the world has none: a `--real` run, or a build with nothing to drive turns.
    if (await spoken.count()) {
      await s.say('And one that remembers', 'Asked once to keep something. What it kept is in the file.', 2.6)
      await s.click(spoken.locator('.bot-open-button'))
      await page.locator('[data-test="bot-details"]').waitFor()
      // Its memory is kept in the project it worked in, so that folder's memory is shown.
      const folder = page.locator('[data-test="memory-folder"]')
      if (await folder.count()) {
        await s.click(folder)
        const project = page.locator('leo-option').filter({ hasNotText: 'No project' }).first()
        if (await project.count()) await s.click(project)
        await page.waitForTimeout(600)
      }
      const kept = page.locator('[data-test="bot-details"] [data-test="bot-memory"]')
      if (await kept.count()) {
        await s.glideTo(kept)
        // The file opens with the standing note about what it is for, and what the bot added is
        // under it; the box shows the top. Scrolled to the end, where the bot's line is.
        await kept.locator('.memory-readable, .bot-memory').first().evaluate((el) => {
          el.scrollTop = el.scrollHeight
        })
        await s.say('In its own words', 'The bot wrote this. Nobody typed it, and nothing here parsed what the model said.', 3.2)
        await s.spotlight(kept, 2.4)
        await s.unspot()
        await s.shot('12-bot-memory')
      }

      await s.say('One conversation, resumed', 'Opening it from the bot’s page resumes it, not a new one each time.', 2.6)
      await s.click(page.locator('[data-test="bot-conversations"] .bot-history-row').first())
      await page.waitForTimeout(1800)
      const wrote = page.locator('.tool').filter({ hasText: '.bravebot-ui/bots/' }).last()
      if (await wrote.count()) {
        await wrote.scrollIntoViewIfNeeded()
        await s.glideTo(wrote)
        await s.say('And the write is on the record', 'The memory was saved here, in the transcript, named like any other file it touched.', 3.2)
        await s.spotlight(wrote, 2.4)
        await s.unspot()
        await s.shot('12-bot-memory-noted')
      } else {
        await s.shot('12-bot-resumed')
      }
      await s.beat(0.8)
    } else {
      await s.say('One session, resumed', 'Not a new one each time — which is what makes a bot persistent.', 2.6)
      await s.beat(0.8)
    }
  },
}
