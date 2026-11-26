// The row beside the composer that says a session's loops are live (docs/specs/loop.md, LOOP-13).
//
// The real app and the real bridge, against a model service this script serves itself. A session
// with no loops shows no row. `/loop` typed into the composer starts a loop, and once its first
// tick has ended the row counts down to the next one with nothing done in the window. A second
// loop is counted, the session status lists both under their numbers, and once both are stopped
// the row is gone.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR, untilIn } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const { service } = await startModelService(async () => ({ content: 'Checked.' }))
const { project, profile, env } = makeWorld('bravebot-loop-status-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const row = page.locator('[data-test="loop-status"]')
  const until = untilIn(page, 400)
  const seconds = (text) => {
    const [, minutes = '0', secs = '0'] = /next in (?:(\d+)m)? ?(?:(\d+)s)?/.exec(text) ?? []
    return Number(minutes) * 60 + Number(secs)
  }

  // ---- no loops, no row --------------------------------------------------------------------------
  await page.waitForTimeout(2500)
  assert.equal(await row.count(), 0, 'a session with no loops shows no row')

  // ---- one loop counts down with nothing done in the window ---------------------------------------
  await composer.fill('/loop 5m check the deploy')
  await composer.press('Enter')
  await until(async () => (await row.count()) > 0 && /next in/.test(await row.innerText()), 'the row counts down to the next tick')
  await stop.waitFor({ state: 'hidden' })
  const first = await row.innerText()
  assert.match(first, /^Loop live · next in 4m \d+s$/)
  await page.waitForTimeout(3200)
  const later = await row.innerText()
  assert.ok(seconds(later) < seconds(first), `the row counted down with no input: "${first}" then "${later}"`)
  await page.screenshot({ path: join(output, `loop-status-one-${theme}.png`) })

  // ---- two loops are counted -----------------------------------------------------------------------
  await composer.fill('/loop 2m tidy the notes')
  await composer.press('Enter')
  await until(async () => /^2 loops live · next in 1m \d+s$/.test(await row.innerText()), 'two loops are counted, with the sooner moment')
  await stop.waitFor({ state: 'hidden' })
  await page.screenshot({ path: join(output, `loop-status-two-${theme}.png`) })

  // ---- the session status lists each loop ----------------------------------------------------------
  // Chat › Session Status…, the desktop's /status, by the route a menu click takes.
  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'session.status', null))
  const status = page.locator('.notice-grid dd', { hasText: 'tidy the notes' })
  await until(async () => (await status.count()) > 0, 'the status lists the loops')
  const body = await page.locator('.notice-grid').innerText()
  assert.match(body, /Loop 1\s+every 5m · next in \d+m \d+s\s+·\s+check the deploy/)
  assert.match(body, /Loop 2\s+every 2m · next in \d+m \d+s\s+·\s+tidy the notes/)
  // The label and the value sit side by side, with the line in the value's column.
  const [label, value] = await Promise.all(['dt', 'dd'].map((tag) => page.locator(`.notice-grid ${tag}`, { hasText: tag === 'dt' ? /^Loop 1$/ : 'check the deploy' }).boundingBox()))
  assert.ok(value.x > label.x + label.width && Math.abs(value.y - label.y) < 4, 'the loop row is two columns')
  await page.screenshot({ path: join(output, `loop-status-report-${theme}.png`) })
  await page.getByRole('button', { name: 'Done', exact: true }).click()
  await status.waitFor({ state: 'hidden' })

  // ---- stopped, the row is gone --------------------------------------------------------------------
  await composer.fill('/loop stop 1')
  await composer.press('Enter')
  await until(async () => /^Loop live/.test(await row.innerText()), 'one loop is left')
  await composer.fill('/loop stop 2')
  await composer.press('Enter')
  await until(async () => (await row.count()) === 0, 'the row is gone once no loop is live')

  assert.deepEqual(errors, [], 'the page reported no errors')
  console.log(`drive-loop-status: ok (${theme})`)
} finally {
  await app?.close()
  service.close()
}
