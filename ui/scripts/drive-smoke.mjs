// Run against a real desktop (or Xvfb in CI), without backend credentials.
// Pass a packaged executable as the first argument to check its bundled bridge too.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { _electron as electron } from 'playwright-core'

const executablePath = process.argv[2]
const app = await electron.launch({
  ...(executablePath ? { executablePath, args: [] } : { args: ['.'] }),
  cwd: process.cwd(),
  timeout: 40000,
})

try {
  const page = await app.firstWindow()
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.locator('.sessions').waitFor({ state: 'visible' })
  await page.locator('.transcript').waitFor({ state: 'visible' })
  await page.waitForFunction(() => !!window.bravebot)
  const info = await page.evaluate(() => window.bravebot.request('agent.info'))
  assert.equal(info.error, undefined, JSON.stringify(info.error))
  assert.ok(info.ok?.version, 'Rust bridge must report its version')
  const windowState = await app.evaluate(({ BrowserWindow }) => {
    const window = BrowserWindow.getAllWindows()[0]
    return { visible: window.isVisible(), minimized: window.isMinimized() }
  })
  assert.equal(windowState.visible, true, 'Native window must be visible')
  assert.equal(windowState.minimized, false)
  assert.deepEqual(errors, [], 'Renderer must load without uncaught errors')
  mkdirSync('/tmp/bravebot-ui', { recursive: true })
  const screenshot = `/tmp/bravebot-ui/${executablePath ? 'packaged' : 'development'}-smoke.png`
  await page.screenshot({ path: screenshot })
  console.log(`Visible UI and Rust bridge ${info.ok.version} verified. Screenshot: ${screenshot}`)
} finally {
  await app.close()
}
