// A failure in the page ends in a screen that says so, a line in the diagnostic log, and a reload
// that works (DIAG-10).
//
// Real Electron renderer, real bridge, isolated state: a scratch `BRAVEBOT_HOME` and a temp
// `--user-data-dir`, never the real ones, and no provider request is made. The failure is the one
// React meets when a node it expects to remove is not there: the first `removeChild` after the
// patch throws the browser's own `NotFoundError`, which React meets in a commit.
//
//   pnpm run bridge && npx electron-vite build && node scripts/drive-window-fault.mjs
//
// Screenshots go to WINDOW_FAULT_OUTPUT, or a temp directory the run prints.
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { checker } from './drive-support.mjs'

const { check, problems } = checker()
const output = process.env.WINDOW_FAULT_OUTPUT || mkdtempSync(join(tmpdir(), 'bravebot-window-fault-'))
mkdirSync(output, { recursive: true })
const scratch = mkdtempSync(join(tmpdir(), 'bravebot-window-fault-home-'))
const home = join(scratch, 'home')
mkdirSync(home)
// The agent reads its settings from the state directory; an empty set is a valid one.
writeFileSync(join(home, 'settings.json'), '{}', { mode: 0o600 })

const app = await electron.launch({
  args: ['.', `--user-data-dir=${join(scratch, 'profile')}`],
  cwd: process.cwd(),
  env: { ...process.env, BRAVEBOT_HOME: home },
  timeout: 40000,
})

/** Every `window.fault` line the agent has written, read from the files in the scratch home. */
const faultLines = () => {
  try {
    return readdirSync(join(home, 'logs')).flatMap((file) =>
      readFileSync(join(home, 'logs', file), 'utf8').split('\n').filter((line) => line.includes(' window.fault ')))
  } catch {
    return []
  }
}

try {
  const page = await app.firstWindow()
  page.setDefaultTimeout(15000)
  await page.waitForLoadState('domcontentloaded')
  await page.getByText('Settings', { exact: true }).first().click()
  const back = page.getByText('Back to BraveBot')
  await back.waitFor()
  check(faultLines().length === 0, 'nothing is logged before anything fails')

  // The next removal of a node throws what the browser throws when the node is not a child.
  await page.evaluate(() => {
    const real = Node.prototype.removeChild
    Node.prototype.removeChild = function () {
      Node.prototype.removeChild = real
      throw new DOMException("Failed to execute 'removeChild' on 'Node': The node to be removed is not a child of this node.", 'NotFoundError')
    }
  })
  await back.click()

  const fault = page.locator('[data-test="window-fault"]')
  await fault.waitFor()
  check(true, 'the window shows the fault screen instead of going black')
  check(await page.locator('[data-test="window-fault"] leo-alert').getAttribute('role') === 'alert', 'the screen is announced as an alert')
  check(/error-card/.test(await page.locator('[data-test="window-fault"] leo-alert').getAttribute('class') ?? ''), 'it is drawn as the error card the transcript uses')
  const details = await page.locator('[data-test="window-fault"] pre').textContent()
  check(/NotFoundError/.test(details ?? '') && /not a child of this node/.test(details ?? ''), 'the details name the error and say what the browser said')
  check(/Component stack:\n\s+at /.test(details ?? ''), 'the details name the components React was drawing')
  await page.screenshot({ path: join(output, 'fault-light.png') })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.screenshot({ path: join(output, 'fault-dark.png') })
  await page.emulateMedia({ colorScheme: null })

  // The agent writes the line a moment after the main process forwards it.
  const deadline = Date.now() + 8000
  while (faultLines().length === 0 && Date.now() < deadline) await page.waitForTimeout(100)
  const lines = faultLines()
  check(lines.length === 1, `one line is logged for one failure (${lines.length})`)
  check(/ ERROR window\.fault kind=render name=NotFoundError$/.test(lines[0] ?? ''), `the line holds the kind and the error's name and nothing else: ${lines[0] ?? '(none)'}`)
  check(!/removeChild|not a child|Component/.test(lines.join('\n')), 'neither the message nor the components reached the log')

  await page.getByRole('button', { name: 'Reload' }).click()
  await page.locator('.sidebar-tabs [role="option"]').first().waitFor()
  check((await fault.count()) === 0, 'Reload brings the app back')
} catch (error) {
  check(false, `the run finished: ${error instanceof Error ? error.message : error}`)
} finally {
  await app.close().catch(() => undefined)
  rmSync(scratch, { recursive: true, force: true })
}

console.log(`screenshots in ${output}`)
if (problems.length) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok')
