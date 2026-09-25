// The column beside the conversation: what the session touched, and the file tree.
//
// Panel visibility is now Overview vs Files tabs rather than five independent picks.
import { openNewest } from '../pick.mjs'

const LABELS = {
  overview: ['Overview', 'Plan, reads, writes, and confined content for this turn.'],
  files: ['Files', 'The project folder itself, listed from disk.'],
}

export default {
  id: '05-context',
  title: 'Context',

  async run(s) {
    const { page } = s
    await openNewest(s, { hold: 1.2 })

    await s.say('Context', "Everything the session touched, beside the conversation that caused it.", 2)
    await s.spotlight('.context', 1.6)
    await s.unspot()
    await s.shot('05-context')

    const tabs = page.locator('.inspector-tabs [role="tab"]')
    for (let i = 0; i < (await tabs.count()); i++) {
      const tab = tabs.nth(i)
      const name = ((await tab.textContent()) ?? '').trim().toLowerCase()
      const [title, line] = LABELS[name] ?? [name, '']
      await s.click(tab)
      await s.say(title, line, 1.8)
    }

    await s.shot('05-panels')
    await s.say('Context', 'A panel folds from its own heading, too.')
    const overview = page.locator('.inspector-tabs [role="tab"]').filter({ hasText: /^Overview$/ })
    if ((await overview.getAttribute('aria-selected')) !== 'true') await s.click(overview)
    const head = page.locator('.panel-head').first()
    if (await head.count()) {
      await s.click(head)
      await s.beat(1)
      await s.click(head)
      await s.beat(0.8)
    }
  },
}
