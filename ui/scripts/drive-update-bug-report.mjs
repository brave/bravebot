// The update notice and the bug report in a real window (UPDATE-12, DIAG-9).
//
// Runs in a profile and a state directory of its own: `BRAVEBOT_HOME` names a scratch directory
// seeded with a release record newer than any build, so the agent's `update.status` says yes, and
// the agent writes its own diagnostic log there. Nothing is read from or written to the person's
// home. A second launch over the same profile checks that a dismissal outlived the window. The
// unavailable report is the main process's compose handler answered with the shape the bridge
// gives a session that keeps nothing.
//
// Screenshots go to $SHOTS, /tmp/bravebot-ui by default.
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { SIDEBAR_TAB } from './drive-support.mjs'

const shots = process.env.SHOTS ?? '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })
const scratch = mkdtempSync(join(tmpdir(), 'bravebot-update-'))
const profile = join(scratch, 'profile')
const home = join(scratch, 'home')
mkdirSync(home, { recursive: true })
writeFileSync(join(home, 'update-check'), `${Math.floor(Date.now() / 1000)}\treleases\t99.0.0\n`)
// A log is opened only when something is logged, so one an earlier run left is put there.
mkdirSync(join(home, 'logs'))
writeFileSync(join(home, 'logs', '20261027T101500Z-4242.log'), '2026-10-27T10:15:00Z ERROR example\n')

async function start() {
  const app = await electron.launch({
    args: ['.', `--user-data-dir=${profile}`, '--disable-renderer-backgrounding'],
    cwd: process.cwd(), timeout: 40000,
    env: { ...process.env, BRAVEBOT_HOME: home },
  })
  const page = await app.firstWindow()
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.waitForLoadState('domcontentloaded')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.locator(SIDEBAR_TAB).first().waitFor()
  return { app, page, errors }
}
const click = (app, id) => app.evaluate(({ Menu }, id) => Menu.getApplicationMenu().getMenuItemById(id).click(), id)

try {
  {
    const { app, page, errors } = await start()
    const banner = page.locator('[data-test="update-banner"]')
    await banner.waitFor()
    assert.match(await banner.textContent(), /A newer Brave Bot is available\. You're on \d+\.\d+\.\d+/)
    assert.doesNotMatch(await banner.textContent(), /99\.0\.0/, 'the recorded version is not shown')
    await page.screenshot({ path: join(shots, 'rxx7-1-banner.png') })

    await click(app, 'help.update')
    const about = page.getByRole('dialog', { name: 'About Brave Bot' })
    await about.waitFor()
    assert.equal(await about.locator('[data-test="about-update"]').getAttribute('data-state'), 'newer')
    assert.ok(await about.locator('[data-test="about-download"]').isVisible())
    await page.screenshot({ path: join(shots, 'rxx7-2-about.png') })

    await about.locator('[data-test="about-report-bug"]').click()
    const report = page.locator('.bug-report')
    await report.locator('[data-test="bug-report-text"]').waitFor()
    const text = await report.locator('[data-test="bug-report-text"]').textContent()
    assert.match(text, /^bravebot desktop\ninterface /)
    assert.ok(await report.locator('[data-test="bug-report-privacy"]').isVisible())
    assert.ok(await report.locator('[data-test="bug-report-log"]').isVisible(), 'a log is named, so Show Log is offered')
    assert.match(text, /20261027T101500Z-4242\.log/)
    await page.screenshot({ path: join(shots, 'rxx7-3-bug-report.png') })
    await page.keyboard.press('Escape')
    await report.waitFor({ state: 'detached' })

    await app.evaluate(({ ipcMain }) => {
      ipcMain.removeHandler('bravebot:bug-report:compose')
      ipcMain.handle('bravebot:bug-report:compose', () => ({ available: false, text: '', log: null }))
    })
    await click(app, 'help.bug-report')
    await page.locator('[data-test="bug-report-unavailable"]').waitFor()
    assert.equal(await page.locator('.bug-report').getByRole('button', { name: 'Save Report…', exact: true }).isDisabled(), true, 'nothing to save')
    await page.screenshot({ path: join(shots, 'rxx7-4-bug-report-unavailable.png') })
    await page.keyboard.press('Escape')

    await page.locator('[data-test="update-dismiss"]').click()
    await banner.waitFor({ state: 'detached' })
    await page.screenshot({ path: join(shots, 'rxx7-5-dismissed.png') })
    await page.waitForTimeout(400)
    assert.deepEqual(errors, [])
    await app.close()
  }
  {
    const { app, page, errors } = await start()
    await page.waitForTimeout(2500)
    assert.equal(await page.locator('[data-test="update-banner"]').count(), 0, 'a dismissal outlives the window')
    await click(app, 'help.update')
    const about = page.getByRole('dialog', { name: 'About Brave Bot' })
    await about.waitFor()
    assert.equal(await about.locator('[data-test="about-update"]').getAttribute('data-state'), 'newer', 'About still says it')
    assert.deepEqual(errors, [])
    await app.close()
  }
  console.log(`PASS: update banner, About line, bug report and its unavailable state, dismissal kept; shots in ${shots}`)
} finally {
  rmSync(scratch, { recursive: true, force: true })
}
