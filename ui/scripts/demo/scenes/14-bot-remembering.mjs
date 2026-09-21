// The half of a bot's memory nobody presses a key for.
//
// `13-bot-memory` films the memory being written because somebody asked for it in so many words,
// which is the version of this feature that is easy to show and was, for a while, the only version
// there was. The instruction to write lives in the briefing; the briefing reaches a turn only when
// that turn is grounded; and a session spends almost all of its life ungrounded. So a bot was told
// once, at the start, and then left alone with it.
//
// Two things close that, and this scene is about the awkward fact that neither of them looks like
// anything:
//
//  - A bot that has gone several turns without writing is handed its briefing again, early, with a
//    line asking whether anything since is worth keeping. What a viewer sees is a turn.
//  - A compaction is answered by a turn this app sends itself, asking the bot to bring its memory
//    up to date before the detail behind the summary is gone. What a viewer sees is a line in the
//    transcript that nobody typed — and only if a compaction actually happened, which needs a
//    conversation far longer than a take.
//
// So this films the evidence rather than the mechanism: the memory as it stands, and the line drawn
// for a turn the app sent, if the world it inherited has one. It bows out where it has nothing —
// `s.skip` is for a scene with nothing to show, and a bot that has not yet needed reminding is
// exactly that. Filmed after `13-bot-memory`, whose session it reads.
const NAME = 'Harbour Watch'

export default {
  id: '14-bot-remembering',
  title: 'Remembering without being asked',
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

    // --- what it has kept ---------------------------------------------------------------------

    await s.say('What a bot has kept', 'The memory is a file, and the form shows it as it stands.', 2.6)
    await s.click(row.locator('.bot-edit'))
    await page.waitForTimeout(700)

    const shown = page.locator('.bot-memory')
    if (!(await shown.count())) s.skip('this build shows no memory in the form')
    await s.glideTo(shown)
    await s.spotlight(shown, 2.4)
    await s.say(
      'Nobody typed any of this',
      'It is what the bot decided was worth carrying past the end of a conversation.',
      3.2,
    )
    await s.unspot()
    await s.shot('14-bot-memory-kept')
    await s.beat(1.0)

    // Leave the form the way it was found. A driver can afford to end anywhere; a take cannot,
    // because the scene after this one opens on whatever this one left on screen.
    const close = page.locator('.bot-form button').filter({ hasText: /^Cancel$/ }).first()
    if (await close.count()) await s.click(close)
    await page.waitForTimeout(500)

    // --- and the turn nobody asked for ---------------------------------------------------------

    await s.click(row.locator('.bot-open-button'))
    await page.waitForTimeout(2200)

    const asked = page.locator('.consolidation').first()
    if (!(await asked.count())) {
      // The ordinary outcome for a short take, and not a failure: the app asks after a compaction,
      // and a conversation this size has had nothing worth compacting. Said out loud rather than
      // filmed as an empty spotlight.
      console.log('   note: no consolidation in this session yet — nothing has been compacted')
      await s.say(
        'And when a conversation is summarised',
        'The app asks the bot to write down what the summary is about to take. There has been nothing to take here yet.',
        3.4,
      )
      return
    }

    await s.glideTo(asked)
    await s.say('This one the app asked for', 'A compaction was about to take the detail, so the bot was asked to keep it.', 3.4)
    await s.spotlight(asked, 2.6)
    await s.unspot()
    await s.say(
      'Drawn as house-keeping',
      'Not as a prompt — because saying somebody typed it would say something that is not true.',
      3.2,
    )
    await s.shot('14-bot-consolidation')
    await s.beat(1.0)
  },
}
