// A bot, working — which is the only way to show the part that matters.
//
// `12-bots` can show the tab, the faces and the form without spending a token. It cannot show the
// three things the feature is actually for, because each of them needs a model to answer:
//
//  1. **A purpose reaches the planner without anybody typing it.** The bot is asked something
//     ordinary and answers as the thing it was defined to be.
//  2. **The memory is a file the bot edits, and the edit is on the record.** The transcript draws
//     an `Update` line and the Writes panel lists the file — which is the whole mechanism, and the
//     reason nothing here parses what a model said.
//
//     It is *not* an approval card, and this scene waited for one at first and reported that no
//     write had been offered while the file on disk said otherwise. The agent's write gate is about
//     integrity rather than about which file it is: trusted data to a trusted path is written
//     without asking, and a bot that has only read its own checkout is exactly that case. So what
//     is filmed is the record of the write, which is what actually happens.
//  3. **The briefing is a file, not a prompt.** Reopened, the transcript draws it as a file that
//     was read rather than as something somebody typed into the composer.
//
// And, on the way, the rest of what a bot's face does — which `12-bots` could not show because a
// bot made a moment ago is doing nothing: it looks at the reader once it is the one on screen, and
// while a turn runs it stands where the spinner would at the foot of the transcript, looking down.
//
// So this is `live: true`, like `02-live`, and is filmed only when a run asks for it.
//
// It leaves a session behind — the bot's own, which is the point of a bot — in the demo world
// rather than on anybody's machine.
const NAME = 'Harbour Watch'

export default {
  id: '13-bot-memory',
  title: 'A bot at work',
  live: true,

  async run(s) {
    const { page } = s

    const tab = page.locator('.sidebar-tab').nth(1)
    if (!(await tab.count())) s.skip('this build has no bots tab')
    await s.click(tab)
    await page.waitForTimeout(600)

    const row = page
      .locator('.bot')
      .filter({ has: page.locator('.bot-name', { hasText: new RegExp(`^${NAME}$`) }) })
    if (!(await row.count())) s.skip(`"${NAME}" is not in the list — 12-bots makes it`)

    await s.say('Open it', 'A bot with no session yet begins one, in the checkout it is pinned to.', 2.4)
    await s.click(row.locator('.bot-open-button'))
    await page.waitForTimeout(1600)

    if (await page.locator('.trust').isVisible().catch(() => false)) {
      await s.say('Trust the directory', 'The same question any new session asks, once per checkout.', 2.2)
      await s.click('.trust-actions .approve')
      await page.waitForTimeout(800)
    }

    // The header names the bot rather than the session's title, which is the small thing that
    // says whose conversation this is. The face beside the name is the same one as in the list,
    // and now that this is the bot on screen it looks at the reader rather than about the room.
    const head = page.locator('.transcript-head h1')
    if (await head.count()) {
      await s.glideTo(head)
      await s.say('It is the bot speaking', 'The header says who, where a session would say what it was asked first.', 2.6)
      await s.spotlight(head, 1.8)
      await s.unspot()
      if (await head.locator('.bot-avatar').count()) {
        await s.say('And it looks at you', 'The one on screen faces the reader. The others, in the list, look about.', 2.6)
      }
    }

    // --- the purpose, which nobody typed ----------------------------------------------------

    await s.say('Ask it something ordinary', 'Nothing in the prompt says what this bot is for.')
    await s.slowType('.composer textarea', 'What should I know about this checkout?')
    await s.click('.composer .send')

    // While the turn runs, the bot's own face stands where the spinner would, at the foot of the
    // transcript, looking down at the page — the one place in the conversation its face says
    // something the header does not. Filmed now, because a first turn is the slowest one in the
    // scene and this is the only moment the row is on screen. Bows out quietly if the turn was
    // quicker than the glide: a fast answer is not a broken feature.
    const working = page.locator('.working .bot-avatar')
    if (await working.isVisible().catch(() => false)) {
      await s.glideTo(working)
      await s.say('It is working, and looks it', 'Its face stands in for the spinner, looking down at the page.', 2.8)
      await s.spotlight(page.locator('.working'), 2.0)
      await s.unspot()
      await s.shot('13-bot-working')
    }
    await s.say('The briefing goes with it', 'Its purpose and its memory, as a file the planner is handed.', 2.6)

    // A bot's first turn reads its briefing and its memory before it does anything else, so it is
    // the slowest one in the scene. Bowing out rather than failing: a turn that ran long is not a
    // broken video, it is a take to run again.
    if (!(await settle(s))) s.skip('the first turn did not finish in time')
    // The nod on finishing is half a second long and over before `settle` has noticed the turn
    // end, so it is said rather than pointed at. The face is back to looking at the reader by now,
    // which is the thing that can be seen.
    await s.beat(1.2)
    await s.shot('13-bot-reply')
    await s.say('And it answers as itself', 'The purpose was never in the composer. It nodded, once, on finishing.', 2.8)
    await s.beat(1.0)

    // --- the memory, written and approved ----------------------------------------------------

    // Named outright rather than hinted at. "Remember that…" leaves the bot free to say it will
    // and write nothing, which is a fine answer and a wasted take — the shot is the approval card,
    // and the approval card only exists if a write is actually proposed. Asking for the file by
    // name is also what a person does once they know their bot keeps one.
    await s.say('Now ask it to remember', 'Memory is a file in the checkout, and the bot edits it.')
    await s.slowType(
      '.composer textarea',
      'Add two things to your memory file: the tide tables are regenerated every Thursday, ' +
        'and nothing under src/data should be edited by hand.',
    )
    await s.click('.composer .send')

    // A card only where one is due — a turn that has touched untrusted content is asked before it
    // may write to a trusted path. The ordinary case here has touched nothing untrusted, so there
    // is no card and the write simply happens; both are filmed, and neither is treated as the
    // failure of the other.
    const card = page.locator('.confirm:has(.confirm-actions)').first()
    if (await waitForCard(s, card)) {
      await s.glideTo(card)
      await s.say('This one it asks about', 'A turn that touched untrusted content, writing to a trusted path.', 3.0)
      await s.spotlight(card, 2.6)
      await s.unspot()
      await s.shot('13-bot-memory-diff')
      const approve = card.locator('.confirm-actions .approve:not(.always)').first()
      await s.click((await approve.count()) ? approve : card.locator('.confirm-actions .approve').first())
      await page.waitForTimeout(900)
    }

    await settle(s)
    await s.beat(1.0)

    // The record of the write, which is the thing that is always there. The `Update` line in the
    // transcript names the file and the lines it changed; the Writes panel lists it as applied.
    const wrote = page.locator('.tool').filter({ hasText: '.bravebot-ui/bots/' }).last()
    if (await wrote.count()) {
      await s.glideTo(wrote)
      await s.say('It edited its own memory', 'An ordinary write, named in the transcript like any other.', 2.8)
      await s.spotlight(wrote, 2.2)
      await s.unspot()
    }
    const written = page.locator('#panel-writes .files li').filter({ hasText: 'bots/' }).first()
    if (await written.count()) {
      await s.glideTo(written)
      await s.say('And listed under Writes', 'Every change to a bot’s memory is on the record, whether or not it stopped to ask.', 3.2)
      await s.spotlight(written, 2.2)
      await s.unspot()
      await s.shot('13-bot-memory-written')
    }

    // --- resumed -----------------------------------------------------------------------------

    await s.say('Leave, and come back', 'A bot is one session, resumed — not a new one each time.', 2.6)
    await s.click(page.locator('.sidebar-tab').first())
    await page.waitForTimeout(600)
    await s.click(page.locator('.sidebar-tab').nth(1))
    await page.waitForTimeout(500)
    await s.click(row.locator('.bot-open-button'))
    await page.waitForTimeout(2000)

    await s.say('The same conversation', 'With its purpose and its memory read back in at the top.', 2.6)
    const attached = page.locator('.attached').first()
    if (await attached.count()) {
      await s.glideTo(attached)
      await s.say('Drawn as a file that was read', 'Rather than as a prompt, because nobody typed it.', 2.8)
      await s.spotlight(attached, 2.0)
      await s.unspot()
    }
    await s.shot('13-bot-resumed')
    await s.beat(1.0)

    console.log(`   note: this take left a session on "${NAME}" in the demo world`)
  },
}

/**
 * Wait until a write approval is on screen, or until the turn ends without one.
 *
 * `.working` is the turn; the card is the question. Whichever happens first is the answer, which is
 * why this is a poll rather than a race against a timeout — a fixed one was tried and gave up a few
 * seconds before the answer arrived, which is the failure mode that looks like a working feature
 * being broken. The seconds here are a backstop for a turn that never ends at all.
 *
 * Answering `false` is an ordinary outcome rather than a miss: most memory writes are not asked
 * about at all. See the note at the top of this file.
 */
async function waitForCard(s, card, seconds = 300) {
  const until = Date.now() + seconds * 1000
  let quiet = 0
  while (Date.now() < until) {
    if (await card.isVisible().catch(() => false)) return true
    const working = await s.page.locator('.working').isVisible().catch(() => false)
    // Two quiet samples in a row, because a turn pauses between steps and the first still frame
    // is not the end of it.
    quiet = working ? 0 : quiet + 1
    if (quiet >= 3) return false
    await s.page.waitForTimeout(700)
  }
  return false
}

/**
 * Wait for a turn to stop, answering nothing.
 *
 * Unlike the seeding run in `world.mjs`, this approves nothing on its own — the one approval this
 * scene gives is the memory write, and it is given on camera because being given on camera is the
 * point of it.
 */
async function settle(s, seconds = 420) {
  const until = Date.now() + seconds * 1000
  while (Date.now() < until) {
    const working = await s.page.locator('.working').isVisible().catch(() => false)
    if (!working) {
      await s.page.waitForTimeout(1800)
      if (!(await s.page.locator('.working').isVisible().catch(() => false))) return true
      continue
    }
    await s.page.waitForTimeout(800)
  }
  return false
}
