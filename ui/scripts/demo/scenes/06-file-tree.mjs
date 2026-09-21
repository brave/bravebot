// The one panel that reads the disk rather than the transcript: a tree of the folder the
// session is working in.
//
// The demo does not double-click a file. That hands it to whatever app the machine assigns
// the type, which on a recording means an unrelated editor opening over the shot — and on
// somebody else's machine, who knows what. `drive-tree.mjs` leaves the same gesture out, for
// the same reason.
import { openNewest } from '../pick.mjs'

export default {
  id: '06-file-tree',
  title: 'The file tree',

  async run(s) {
    const { page } = s
    await openNewest(s, { hold: 1.2 })

    // The tree is the last of the five panels and may be switched off from a previous run.
    const pick = page.locator('.panel-pick[aria-controls="panel-files"]')
    if ((await pick.count()) && (await pick.getAttribute('aria-pressed')) === 'false') {
      await s.click(pick)
    }
    const tree = page.locator('.tree')
    if (!(await tree.count())) s.skip('this session has no folder to list')

    await s.glideTo(tree)
    const root = await page.locator('.tree-root').getAttribute('title')
    await s.say('The file tree', `The session's own folder — ${root ?? ''}`, 2)
    await s.spotlight(tree, 1.4)
    await s.unspot()

    await s.say('Type at a glance', 'Every file carries a two-letter badge for what it is.')
    await s.spotlight(page.locator('.tree-glyph').first(), 1.6)
    await s.unspot()

    // A folder with something in it. Directories list when opened rather than up front, so
    // an empty one is a perfectly good folder and a poor shot.
    const folders = page.locator('.tree-list[role="tree"] > li[aria-expanded]')
    const shut = await folders.count()
    let opened = null
    for (let i = 0; i < Math.min(shut, 6); i++) {
      const row = folders.nth(i).locator('.tree-row').first()
      await s.click(row)
      await s.beat(0.6)
      if ((await folders.nth(i).locator('[role="group"] > li').count()) > 0) {
        opened = { folder: folders.nth(i), row }
        break
      }
      await row.click() // shut the empty one again rather than leaving it hanging open
      await page.waitForTimeout(200 * s.speed)
    }

    if (opened) {
      await s.say('Open a folder', 'It is read when you open it, not all at once up front.', 2)
      await s.shot('06-tree')
    }

    await s.say('Dotfiles', 'Hidden entries sit behind a toggle.')
    await s.click(page.locator('.tree-tool').first())
    await s.beat(1.2)
    await s.click(page.locator('.tree-tool').first())
    await s.beat(0.8)

    // The filter, typed against something actually on screen — a made-up query narrowing to
    // nothing is a truthful shot of an empty tree.
    const names = await page.locator('.tree-list[role="tree"] .tree-name').allInnerTexts()
    const longest = names.reduce((a, b) => (b.length > a.length ? b : a), '')
    const query = longest.length > 6 ? longest.slice(2, 6) : longest.slice(0, 3)
    if (query) {
      await s.say('Filter by name', 'The box above the tree narrows it as you type.')
      await s.slowType('.tree-find', query)
      await s.beat(1)
      if (await page.locator('.tree-note').isVisible().catch(() => false)) {
        await s.say(
          'Filter by name',
          'It says so: the tree is listed a folder at a time, so it searches what has been read.',
          2.6,
        )
        await s.spotlight('.tree-note', 1.4)
        await s.unspot()
      }
      await s.pointAt('.tree-find')
      await page.locator('.tree-find').press('Escape')
      await s.beat(0.8)
    }

    if (opened) {
      await s.click(opened.row)
      await s.beat(0.6)
    }
  },
}
