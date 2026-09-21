// That the three columns can be dragged, and that dragging them cannot break the layout.
//
// The positive case is easy to eyeball; the interesting assertions are the limits. A
// divider that can be dragged past its clamp, or that forgets its position on relaunch,
// looks fine in a screenshot and is wrong.
import { _electron as electron } from 'playwright-core'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const widths = async (page) => ({
  left: (await page.locator('.sessions').boundingBox()).width,
  right: (await page.locator('.context').boundingBox()).width,
  centre: (await page.locator('.transcript').boundingBox()).width,
})

/** Drag a divider by `dx`, in steps, the way a hand would. */
async function drag(page, which, dx) {
  const box = await page.locator('.gutter').nth(which).boundingBox()
  const y = box.y + box.height / 2
  const x = box.x + box.width / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + dx, y, { steps: 12 })
  await page.mouse.up()
  await page.waitForTimeout(150)
}

const launch = () => electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })

let app = await launch()
let page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
await page.waitForTimeout(1500)

check((await page.locator('.gutter').count()) === 2, 'two dividers are present')

const before = await widths(page)
console.log('start:', JSON.stringify(before))

// --- dragging widens and narrows -----------------------------------------------------
await drag(page, 0, 80)
let now = await widths(page)
check(Math.abs(now.left - (before.left + 80)) <= 2, `left divider widened the session list (${before.left} -> ${now.left})`)
check(Math.abs(now.centre - (before.centre - 80)) <= 2, 'the transcript gave up exactly what the session list took')

await drag(page, 1, -60)
const after = await widths(page)
check(Math.abs(after.right - (now.right + 60)) <= 2, `right divider widened the context panel (${now.right} -> ${after.right})`)

// --- the clamps ----------------------------------------------------------------------
await drag(page, 0, -900)
const squashed = await widths(page)
check(squashed.left >= 200, `the session list stops at its minimum (${squashed.left} >= 200)`)

await drag(page, 0, 1600)
const stretched = await widths(page)
check(stretched.left <= 460, `the session list stops at its maximum (${stretched.left} <= 460)`)
check(stretched.centre >= 380, `the transcript never drops below its floor (${stretched.centre} >= 380)`)

// --- double click resets -------------------------------------------------------------
const gutter = page.locator('.gutter').first()
await gutter.dblclick()
await page.waitForTimeout(150)
check(Math.abs((await widths(page)).left - 280) <= 2, 'double click restores the default width')

// --- keyboard ------------------------------------------------------------------------
await gutter.focus()
await page.keyboard.press('ArrowRight')
await page.keyboard.press('ArrowRight')
await page.waitForTimeout(120)
check(Math.abs((await widths(page)).left - 296) <= 2, 'arrow keys move the divider 8px at a time')

// --- what gets remembered ------------------------------------------------------------
await drag(page, 0, 60)
const remembered = (await widths(page)).left
await page.screenshot({ path: '/tmp/bravebot-ui/06-resize.png' })
await app.close()

app = await launch()
page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
await page.waitForTimeout(1500)
const reopened = (await widths(page)).left
check(Math.abs(reopened - remembered) <= 2, `the layout survives a relaunch (${remembered} -> ${reopened})`)
await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok — shot in /tmp/bravebot-ui/06-resize.png')
