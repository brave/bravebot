// TRUST-23 and TRUST-24 in the window: a yes remembered at the trust question settles the next
// session started in that directory, which says so above its transcript, and Forget in
// Permissions makes the next one ask again. Real Electron and bridge, across two launches with one
// home; only the file picker is substituted.
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-remembered-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })

async function launch(drive) {
  const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter(k => process.env[k]).map(k => [k, process.env[k]]))
  Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config') })
  const app = await electron.launch({ args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], env, timeout: 40000 })
  try {
    const page = await app.firstWindow(); page.setDefaultTimeout(15000)
    const errors = []; page.on('pageerror', error => errors.push(error.message))
    await app.evaluate(({ dialog }, path) => {
      dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
    }, project)
    await drive(page)
    assert.deepEqual(errors, [])
  } finally {
    await app.close()
  }
}

const trust = (page) => page.getByRole('dialog', { name: 'Project trust', exact: true })
const banner = (page) => page.locator('.entries .session-banner', { hasText: 'Trust is remembered' })
const openProject = (page) => page.getByRole('button', { name: 'Open project', exact: true }).click()

try {
  let record = null
  await launch(async (page) => {
    await openProject(page)
    const asked = trust(page)
    const remember = asked.getByRole('button', { name: 'Trust and remember', exact: true })
    await remember.waitFor()
    record = (await asked.locator('.aside code').innerText()).trim()
    assert.ok(record.startsWith(join(home, '.bravebot', 'trusted')), `the offer names the record: ${record}`)
    await page.screenshot({ path: `${shots}/remembered-1-offered.png` })
    await remember.click()
    await asked.waitFor({ state: 'hidden' })
    await banner(page).waitFor()
    assert.ok(existsSync(record), 'remembering wrote the record it named')
    await page.screenshot({ path: `${shots}/remembered-2-kept.png` })
  })

  await launch(async (page) => {
    await openProject(page)
    const shown = banner(page)
    await shown.waitFor()
    assert.match(await shown.innerText(), /sessions started here are not asked/)
    assert.equal(await trust(page).count(), 0, 'a remembered yes asks nothing')
    await page.screenshot({ path: `${shots}/remembered-3-not-asked.png` })

    await shown.getByRole('button', { name: 'Manage', exact: true }).click()
    const forget = page.getByRole('button', { name: 'Forget', exact: true })
    await forget.waitFor()
    await page.screenshot({ path: `${shots}/remembered-4-permissions.png` })
    await forget.click()
    await page.getByText('No answer is remembered for this directory.', { exact: true }).waitFor()
    assert.equal(existsSync(record), false, 'forgetting removed the record')
    assert.equal(await shown.count(), 0, 'the banner went with the answer')
    await page.getByRole('button', { name: 'Done', exact: true }).click()

    await page.getByRole('button', { name: 'New session', exact: true }).click()
    await trust(page).waitFor()
    await page.screenshot({ path: `${shots}/remembered-5-asked-again.png` })
  })
  console.log(`Remembered trust verified across two launches. Screenshots in ${shots}/remembered-*.png`)
} finally {
  rmSync(root, { recursive: true, force: true })
}
