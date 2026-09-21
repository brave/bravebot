// The three columns, and the two ways they move: dragged from the 1px divider between them,
// or folded away entirely from the chevrons at either end of the transcript's header.
//
// A drag is the one gesture in the app that a synthetic pointer cannot fake by teleporting —
// the column has to be seen following the mouse — so this moves the real cursor in steps and
// keeps the drawn pointer alongside it, frame for frame.
import { openNewest } from '../pick.mjs'

/** Drag a divider `dx` pixels, with the drawn pointer travelling with the real one. */
async function drag(s, selector, dx) {
  const { page } = s
  const box = await page.locator(selector).boundingBox()
  if (!box) s.skip(`no ${selector} on screen to drag`)
  const y = box.y + box.height / 2
  const x = box.x + box.width / 2

  await s.pointAt(selector)
  await page.mouse.move(x, y)
  await page.mouse.down()
  const steps = 26
  for (let i = 1; i <= steps; i++) {
    const to = x + (dx * i) / steps
    await page.mouse.move(to, y)
    await page.evaluate(([px, py]) => window.__demo.cursor(px, py, 0), [to, y])
    await page.waitForTimeout(Math.round(11 * s.speed))
  }
  await page.mouse.up()
  await s.beat(0.8)
}

export default {
  id: '04-columns',
  title: 'The layout',

  async run(s) {
    const { page } = s
    await openNewest(s, { hold: 1.2 })

    await s.say('Three columns', 'Sessions, the conversation, and what the session has touched.', 2)
    await s.shot('04-columns')

    await s.say('Drag a divider', 'Either side resizes from the hairline between the columns.')
    await drag(s, '.gutter:not(.inert) >> nth=0', 110)
    await drag(s, '.gutter:not(.inert) >> nth=1', -90)

    await s.say('Double-click to reset', 'Both go back to where they started.')
    const first = await s.pointAt('.gutter:not(.inert) >> nth=0')
    await page.evaluate(() => window.__demo.tap())
    await first.dblclick()
    await s.beat(1)
    const second = await s.pointAt('.gutter:not(.inert) >> nth=1')
    await page.evaluate(() => window.__demo.tap())
    await second.dblclick()
    await s.beat(1.2)

    await s.say('Fold a column away', 'The chevrons at either end of the header.')
    await s.click('.fold-toggle.left')
    await s.beat(1.2)
    await s.say('Fold a column away', 'The conversation takes the room.', 1.4)
    await s.click('.fold-toggle.right')
    await s.beat(1.4)
    await s.shot('04-folded')

    await s.say('And back', 'Each returns to the width it left at — and would after a relaunch.')
    await s.click('.fold-toggle.right')
    await s.beat(1)
    await s.click('.fold-toggle.left')
    await s.beat(1.2)
  },
}
