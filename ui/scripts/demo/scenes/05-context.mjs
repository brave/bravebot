// The right column: what the session has touched. Five panels — the plan, files read, writes
// and how far each got, anything confined, and the folder itself — each turned on and off
// from the row of icons at the top.
//
// Filmed by walking the row rather than by opening one panel, because the argument the column
// makes is cumulative: it is the whole of what the agent did, in one place, beside the
// conversation that caused it.
import { findSession, openSession, openNewest } from '../pick.mjs'

const LABELS = {
  plan: ['The plan', 'What the agent said it was going to do.'],
  read: ['Files read', 'Everything it looked at, and what it was only told the name of.'],
  writes: ['Writes', 'What it changed, and how far each one got.'],
  confined: ['Confined', 'Anything from outside, kept labelled as such.'],
  files: ['Files', "The folder the session is working in."],
}

export default {
  id: '05-context',
  title: 'Context',

  async run(s) {
    const { page } = s

    // A session that actually did something, so the panels have contents rather than counts
    // of zero — an empty panel is a truthful shot and a useless one.
    const busy = await findSession(s, (said) => said.filter((l) => l.kind === 'tool').length >= 2)
    if (busy) await openSession(s, busy, { hold: 1.2 })
    else await openNewest(s, { hold: 1.2 })

    if (!(await page.locator('.context').isVisible().catch(() => false))) {
      s.skip('the context column is not on screen')
    }

    await s.say('Context', "Everything the session touched, beside the conversation that caused it.", 2)
    await s.spotlight('.context', 1.6)
    await s.unspot()
    await s.shot('05-context')

    // Everything off first, so each panel arrives on an empty column and is the only thing
    // moving when its turn comes.
    const picks = page.locator('.panel-pick')
    const count = await picks.count()
    for (let i = 0; i < count; i++) {
      const pick = picks.nth(i)
      if ((await pick.getAttribute('aria-pressed')) === 'true') {
        await pick.click()
        await page.waitForTimeout(160 * s.speed)
      }
    }
    await s.beat(0.8)

    for (let i = 0; i < count; i++) {
      const pick = picks.nth(i)
      const id = (await pick.getAttribute('aria-controls')) ?? ''
      const key = Object.keys(LABELS).find((k) => id.includes(k)) ?? id
      const [title, line] = LABELS[key] ?? [key, '']
      await s.click(pick)
      await s.say(title, line, 1.8)
    }

    await s.shot('05-panels')
    await s.say('Context', 'A panel folds from its own heading, too.')
    const head = page.locator('.panel-head').first()
    if (await head.count()) {
      await s.click(head)
      await s.beat(1)
      await s.click(head)
      await s.beat(0.8)
    }
  },
}
