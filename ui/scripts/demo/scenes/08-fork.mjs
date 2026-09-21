// Cutting a conversation in two.
//
// The loop a person actually walks: hover a prompt, take the fork beside it, find that prompt
// waiting in the composer to be asked differently, read the banner, and click back to the
// session it came out of. All four marks — the control, the banner, the row in the list, the
// link back — are the same glyph, because they are the same idea.
//
// This one makes something. A fork is a real session from its first moment, so a take leaves
// a forked session in the store — it writes no record until it has something to say, but it
// is there in the window. The scene says which one, so it can be cleaned up after the shoot.
import { findSession, openSession } from '../pick.mjs'

export default {
  id: '08-fork',
  title: 'Forking',

  async run(s) {
    const { page } = s

    // Two prompts, so there is something in front of the second to cut at.
    const parent = await findSession(
      s,
      (said) => said.filter((line) => line.kind === 'user').length >= 2,
    )
    if (!parent) s.skip('no stored session has two prompts in it — nothing to fork')

    await s.say('Forking', 'A conversation that went somewhere unhelpful, and a wish to go back.', 2.2)
    await openSession(s, parent)

    const prompt = page.locator('.bubble.user').nth(1)
    await s.glideTo(prompt)
    await s.say('Hover a prompt', 'A fork appears beside it. Right-clicking offers the same thing.')
    await s.hover(prompt)
    await s.beat(0.8)

    const control = prompt.locator('.fork-here')
    if (!(await control.count())) s.skip('no fork control appeared on the prompt')
    await s.spotlight(control, 1.4)
    await s.unspot()

    await s.say('Fork from here', 'A new session holding everything said before this prompt.')
    await s.click(control)
    await page.waitForTimeout(1800 * s.speed)
    await s.shot('08-fork')

    const composer = page.locator('.composer textarea')
    await s.glideTo(composer)
    await s.say('The prompt is waiting', 'In the composer, to be edited and asked differently.', 2.4)
    await s.spotlight(composer, 1.6)
    await s.unspot()

    const banner = page.locator('.fork-banner')
    if (await banner.count()) {
      await s.glideTo(banner)
      await s.say('It says where it came from', 'And the link opens the parent at the prompt it was cut in front of.', 2.6)
      await s.spotlight(banner, 1.4)
      await s.unspot()
    }

    const mark = page.locator('.session:has(.fork-mark)').first()
    if (await mark.count()) {
      await s.say('And the list marks it', 'The same glyph, because it is the same idea.')
      await s.spotlight(mark, 1.8)
      await s.unspot()
    }

    if (await banner.locator('.link').count()) {
      await s.say('Back to the parent', 'Untouched — a fork takes nothing away from what it came from.')
      await s.click(banner.locator('.link'))
      await page.waitForTimeout(1600 * s.speed)
      await s.shot('08-parent')
      await s.beat(1.2)
    }

    console.log(`   note: this take left a fork of "${parent.title}" in the session store`)
  },
}
