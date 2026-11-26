// The run card for a line the planner asked to run with no sandbox (SANDBOX-29), in the real
// renderer with a mocked bridge: the bridge's warning and not-remembered sentence are drawn, no
// standing answer is offered, and "Run once" sends a yes that remembers nothing. Screenshots of
// that card and of a run card with the other alerts, in light and dark, land in UNCONFINED_OUTPUT
// (default: a temp folder).
//
//   pnpm run build && xvfb-run -a node scripts/drive-unconfined-run.mjs
import assert from 'node:assert/strict'
import { mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { launchMocked } from './drive-mock.mjs'
import { eventsFor, installBridge, makeProfile } from './acceptance-fixture.mjs'
import { APPROVE_BUTTON, COMPOSER_NAME } from './drive-support.mjs'

const output = process.env.UNCONFINED_OUTPUT || join(tmpdir(), 'bravebot-unconfined-run')
mkdirSync(output, { recursive: true })
const { profile, directory, state } = makeProfile()
const { app, page, errors } = await launchMocked({ profile, x11: true, env: { BRAVEBOT_HOME: state }, viewport: { width: 1350, height: 900 }, timeout: 7000 })
const UNCONFINED_NOTE = 'the model asked for this one line to run with no sandbox: no profile holds it, and the next line is held to the session\'s mode again'
const NOT_REMEMBERED_NOTE = 'a line the model asked to run with no sandbox is asked about every time, so neither the line nor the request is remembered'
try {
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

  // "Run once" opens once the card has been on screen, and sends a yes that remembers nothing.
  const runOnce = card.locator(`${APPROVE_BUTTON}:not(.always)`)
  await page.waitForFunction((button) => !button.hasAttribute('disabled') && button.getAttribute('aria-disabled') !== 'true', await runOnce.elementHandle())
  await runOnce.click()
  let reply
  for (let tries = 0; tries < 50 && !reply; tries += 1) {
    reply = await app.evaluate(() => globalThis.ux.replies.find((r) => r.method === 'run.reply'))
    if (!reply) await page.waitForTimeout(100)
  }
  assert.ok(reply, 'Run once sent no run.reply')
  const { request, decision, remember } = reply.params
  assert.deepEqual({ request, decision, remember }, { request: 1, decision: 'approve', remember: false })

  // The other run-card alerts share `.card-alert`: a confined line that is lent a credential
  // scope and hands the reader's own data to the program.
  await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'))
  await emit('run.request', {
    request: 2, line: 'gh pr list', plan: 'gh pr list',
    stages: [{ program: 'gh', resolved: '/usr/bin/gh', shownBinary: '/usr/bin/gh', args: ['pr', 'list'], display: 'gh pr list' }],
    directory, shownDirectory: directory, releasesPrivate: true, ambient: [], canBeRemembered: false,
    requestedScopes: ['github'], confinement: null, notSandboxed: 'this is not sandboxed: it runs with the access your own shell has',
    unconfined: false, unconfinedNote: null, notRememberedNote: null, vouches: [], summary: 'List pull requests',
  })
  const lent = page.locator('.confirm.run').last()
  await lent.getByText('github', { exact: true }).waitFor()
  await lent.scrollIntoViewIfNeeded()
  for (const theme of ['light', 'dark']) {
    await page.evaluate((t) => document.documentElement.setAttribute('data-theme', t), theme)
    await page.waitForTimeout(450)
    await lent.screenshot({ path: join(output, `lent-run-${theme}.png`) })
    console.log('VISUAL', `lent-run-${theme}`)
  }
  assert.deepEqual(errors, [])
} finally {
  await app.close()
  for (const path of [profile, directory, state]) rmSync(path, { recursive: true, force: true })
}
