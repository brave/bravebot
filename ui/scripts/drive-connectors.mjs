// That the Connectors dialog declares, approves and turns on an MCP server only as it was shown,
// and turns one off (docs/specs/mcp-servers.md SERVERS-2, SERVERS-3, SERVERS-5).
//
// The real app and the real bridge, against a home of this script's own. Nothing is started and
// nothing is paid for: connecting writes files, and the servers start with a conversation.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`), which
// `npm run drive:connectors` does first.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, chmodSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-connectors-')))
const home = join(root, 'home'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), join(home, 'github-mcp-server'), profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ model: 'local/test', provider: { local: { options: { baseURL: 'http://127.0.0.1:9/v1' }, models: { test: {} } } } }))
const program = join(home, 'github-mcp-server/github-mcp-server')
writeFileSync(program, '#!/bin/sh\n')
chmodSync(program, 0o755)
const file = (name) => { try { return readFileSync(join(home, '.bravebot', name), 'utf8') } catch { return '' } }

const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), BRAVEBOT_LOCALE: 'en-US' })

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))

  // ---- the button and the list ---------------------------------------------------------------
  await page.locator('[data-test="connectors"]').click()
  const dialog = page.locator('.connectors')
  await dialog.locator('[data-test="connectors-list"]').waitFor()
  for (const alias of ['github', 'gmail', 'calendar', 'brave-search']) {
    assert.equal((await dialog.locator(`[data-test="connector-${alias}"] [data-test="connector-standing"]`).innerText()).trim(), 'Not set up', alias)
  }
  await page.screenshot({ path: join(output, 'connectors-list.png') })

  // ---- a custom connector by URL ---------------------------------------------------------------
  await dialog.locator('[data-test="connector-add"]').click()
  await dialog.locator('[data-test="connector-alias"] input').fill('calc')
  await dialog.locator('[data-test="connector-url"] input').fill('http://localhost:3333/mcp')
  await page.screenshot({ path: join(output, 'connectors-add.png') })
  await dialog.locator('[data-test="connector-review"]').click()
  const review = dialog.locator('[data-test="connector-review-page"]')
  await review.waitFor()
  assert.match(await review.innerText(), /Reaches\s+http:\/\/localhost:3333\/mcp/)
  assert.equal(file('mcp.json'), '', 'reviewing wrote nothing')
  await page.screenshot({ path: join(output, 'connectors-review.png') })
  await page.locator('[data-test="connector-connect"]').click()
  await dialog.locator('[data-test="connectors-status"]').waitFor()
  assert.match(file('mcp.json'), /localhost:3333/)
  assert.deepEqual(JSON.parse(file('settings.json')).mcp.request, ['calc'])
  assert.equal((await dialog.locator('[data-test="connector-standing"]').first().innerText()).trim(), 'Connected')

  // ---- GitHub, from the catalog ------------------------------------------------------------------
  await page.locator('[data-test="connectors-back"]').click()
  await dialog.locator('[data-test="connector-calc"]').waitFor()
  assert.equal((await dialog.locator('[data-test="connector-calc"] [data-test="connector-standing"]').innerText()).trim(), 'Connected')
  await dialog.locator('[data-test="connector-github"]').click()
  await dialog.locator('[data-test="connector-field-token"] input').fill('github_pat_example')
  await page.screenshot({ path: join(output, 'connectors-github-setup.png') })
  await dialog.locator('[data-test="connector-review"]').click()
  await review.waitFor()
  const reviewed = await review.innerText()
  assert.ok(reviewed.includes(program), reviewed)
  assert.match(reviewed, /GITHUB_PERSONAL_ACCESS_TOKEN \(stored\)/)
  assert.ok(!(await page.content()).includes('github_pat_example'), 'the token is not drawn back')
  await page.locator('[data-test="connector-connect"]').click()
  await dialog.locator('[data-test="connectors-status"]').waitFor()
  assert.match(file('mcp.json'), /github_pat_example/)
  assert.deepEqual(JSON.parse(file('settings.json')).mcp.request, ['calc', 'github'])
  await page.screenshot({ path: join(output, 'connectors-github-connected.png') })

  // ---- its settings page, and disconnecting it ----------------------------------------------------
  assert.ok(!(await page.content()).includes('github_pat_example'), 'the token is not drawn on its page')
  await dialog.locator('[data-test="connector-disconnect"]').click()
  await dialog.locator('[data-test="connectors-status"]').filter({ hasText: 'disconnected' }).waitFor()
  assert.deepEqual(JSON.parse(file('settings.json')).mcp.request, ['calc'])
  assert.match(file('mcp.json'), /github/, 'the setup is kept')
  await page.locator('[data-test="connectors-back"]').click()
  assert.equal((await dialog.locator('[data-test="connector-github"] [data-test="connector-standing"]').innerText()).trim(), 'Not connected')
  await page.screenshot({ path: join(output, 'connectors-after.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: the Connectors dialog adds a custom connector and a catalog one only as reviewed, keeps a token out of the window, and disconnects one while keeping its setup. Screenshots in ${output}/connectors-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'connectors-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  rmSync(root, { recursive: true, force: true })
}
