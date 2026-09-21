// The left column: everything under `~/.bravebot/sessions`, and the three things you can do
// to that list — search it, group it by checkout, and start a session from a heading.
//
// The `+` on a heading is pressed and then *declined* at the trust prompt, as `drive.mjs`
// does. The point of the shot is the question, not the answer: saying yes on camera would be
// trusting somebody's real checkout on their behalf, and would leave a session behind that
// nobody meant to start.
import { existsSync } from 'node:fs'

export default {
  id: '01-sessions',
  title: 'Sessions',

  async run(s) {
    const { page } = s
    const rows = page.locator('.session')
    const total = await rows.count()
    if (total === 0) s.skip('no sessions are stored — nothing to list')

    await s.say('Sessions', `Every session under ~/.bravebot, newest first — ${total} here.`, 1.6)
    await s.spotlight('.session-list', 1.4)
    await s.unspot()

    // A word out of the newest session's own title, so the filter visibly narrows to it.
    const first = (await page.locator('.session-title').first().textContent()) ?? ''
    const where = (await page.locator('.session-where').first().textContent()) ?? ''
    const word = first.split(/\s+/).find((w) => w.length > 4) ?? first.slice(0, 6)
    const project = where.split(' · ')[0] ?? ''

    await s.say('Find one', 'The box above the list filters as you type.')
    await s.slowType('.session-find', word)
    await s.say('Find one', `"${word}" — ${await rows.count()} of ${total} left.`, 1.6)

    // The same box against the second line: a session you remember by *where* it was, not by
    // what it was called, still has to be findable.
    if (project) {
      await page.locator('.session-find').fill('')
      await s.beat(0.4)
      await s.slowType('.session-find', project)
      await s.say('…or by project', `"${project}" — the checkout is searchable too.`, 1.6)
    }

    await s.pointAt('.session-find')
    await page.locator('.session-find').press('Escape')
    await s.say('Escape', 'Clears it, and the whole list is back.', 1.4)
    await s.beat(0.6)

    await s.say('Group by checkout', 'The toggle beside the box gathers sessions by project.')
    await s.click('.session-group')
    await s.beat(0.8)
    const heads = page.locator('.session-group-head')
    await s.say('Group by checkout', `${await heads.count()} checkouts, each with its count.`, 1.6)
    await s.shot('01-grouped')

    // The fullest group rather than the first: folding a group of one proves very little.
    const tallies = await page
      .locator('.session-group-section')
      .evaluateAll((sections) => sections.map((x) => x.querySelectorAll('.session').length))
    const biggest = tallies.reduce((best, n, i) => (n > (tallies[best] ?? 0) ? i : best), 0)
    const head = heads.nth(biggest)

    await s.say('Fold a group', 'Click a heading and its sessions fold away.')
    await s.click(head)
    await s.beat(1)
    await s.say('Fold a group', 'Click it again and they come back.')
    await s.click(head)
    await s.beat(1)

    // A checkout still on disk. The list remembers projects that have since moved, and the
    // bridge rightly refuses those — a real answer, but not the one this shot is about.
    const paths = await page
      .locator('.session-group-fold')
      .evaluateAll((all) => all.map((h) => h.getAttribute('title') ?? ''))
    const live = paths.find((p) => p && existsSync(p))
    if (live) {
      const owner = page
        .locator('.session-group-head')
        .filter({ has: page.locator(`.session-group-fold[title="${live}"]`) })
      await s.say('Start one here', 'The + beside a heading opens a session in that checkout.')
      await s.click(owner.locator('.session-group-new'))
      await s.beat(1.2)
      if (await page.locator('.trust').isVisible().catch(() => false)) {
        await s.say('Trust the directory', 'It asks before it works anywhere — once, per checkout.', 2.2)
        await s.shot('01-trust')
        await s.click('.trust-actions .decline')
      }
    }

    await s.say('Group by checkout', 'Back to a flat list.')
    await s.click('.session-group')
  },
}
