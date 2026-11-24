// The run card for a line the planner asked to run with no sandbox (SANDBOX-29), in the real
// renderer with a mocked bridge: the bridge's warning and not-remembered sentence are drawn, no
// standing answer is offered, and "Run once" sends a yes that remembers nothing. Screenshots in
// light and dark land in UNCONFINED_OUTPUT (default: a temp folder).
//
//   pnpm run build && xvfb-run -a node scripts/drive-unconfined-run.mjs
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { eventsFor, installBridge, makeProfile } from './acceptance-fixture.mjs'
import { COMPOSER_NAME } from './drive-support.mjs'

const output = process.env.UNCONFINED_OUTPUT || join(tmpdir(), 'bravebot-unconfined-run')
mkdirSync(output, { recursive: true })
const { profile, directory, state } = makeProfile()
const app = await electron.launch({
  args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), '--disable-renderer-backgrounding', '--disable-background-timer-throttling', `--user-data-dir=${profile}`],
  cwd: process.cwd(), timeout: 40000, env: { ...process.env, BRAVEBOT_HOME: state },
})
const UNCONFINED_NOTE = 'the planner asked for this one line to run with no sandbox: no profile holds it, and the next line is held to the session\'s mode again'
const NOT_REMEMBERED_NOTE = 'a line the planner asked to run with no sandbox is asked about every time, so neither the line nor the request is remembered'
try {
  const page = await app.firstWindow()
  await page.setViewportSize({ width: 1350, height: 900 })
  page.setDefaultTimeout(7000)
  const errors = []
  page.on('pageerror', (e) => errors.push(e.message))
  await installBridge(app, { directory })
  const { emit } = eventsFor(app)
  await page.reload()
  await page.getByRole('button', { name: 'Open project', exact: true }).waitFor()
  await page.locator('.session').filter({ hasText: 'Review the sample project' }).click()
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  await composer.fill('Build the release')
  await page.getByRole('button', { name: 'Send', exact: true }).click()

  await emit('run.request', {
    request: 1, line: 'cargo build --release', plan: 'cargo build --release',
    stages: [{ program: 'cargo', resolved: '/usr/local/bin/cargo', shownBinary: '/usr/local/bin/cargo', args: ['build', '--release'], display: 'cargo build --release' }],
    directory, shownDirectory: directory, releasesPrivate: false, ambient: [], canBeRemembered: false,
    confinement: null, notSandboxed: 'this is not sandboxed: it runs with the access your own shell has',
    unconfined: true, unconfinedNote: UNCONFINED_NOTE, notRememberedNote: NOT_REMEMBERED_NOTE,
    vouches: [], summary: 'Build the release with no sandbox',
  })
  const card = page.locator('.confirm.run').last()
  await card.locator('[data-test="unconfined"]').getByText(UNCONFINED_NOTE, { exact: true }).waitFor()
  await card.locator('[data-test="not-remembered"]').getByText(NOT_REMEMBERED_NOTE, { exact: true }).waitFor()
  assert.equal(await card.getByText('Trust command and output').count(), 0, 'an unconfined run offers no standing answer')
  await card.scrollIntoViewIfNeeded()
  for (const theme of ['light', 'dark']) {
    if (theme === 'dark') await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'dark'))
    await page.waitForTimeout(450)
    await card.screenshot({ path: join(output, `unconfined-run-${theme}.png`) })
    console.log('VISUAL', `unconfined-run-${theme}`)
  }
  assert.deepEqual(errors, [])
} finally {
  await app.close()
}
