// CHECK-11 in the window: a session opened with auto-vetting on says so above its transcript, and
// one opened without it says nothing. Real Electron and bridge; only the file picker is substituted.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

async function banner(auto) {
  const root = mkdtempSync(join(tmpdir(), 'bravebot-vetting-'))
  const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
  for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
  writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ vetting: { auto } }))
  const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter(k => process.env[k]).map(k => [k, process.env[k]]))
  Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config') })
  const app = await electron.launch({ args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], env, timeout: 40000 })
  try {
    const page = await app.firstWindow(); page.setDefaultTimeout(15000)
    const errors = []; page.on('pageerror', error => errors.push(error.message))
    await app.evaluate(({ dialog }, path) => {
      dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
    }, project)
    await page.getByRole('button', { name: 'Open project', exact: true }).click()
    const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
    await trust.getByRole('button', { name: 'Trust this directory' }).click()
    await trust.waitFor({ state: 'hidden' })
    const notice = page.locator('.transcript-head .vetting-banner')
    if (auto) await notice.waitFor()
    const shown = await notice.count() ? await notice.innerText() : null
    mkdirSync('/tmp/bravebot-ui', { recursive: true })
    await page.screenshot({ path: `/tmp/bravebot-ui/vetting-${auto ? 'on' : 'off'}.png` })
    assert.deepEqual(errors, [])
    return shown
  } finally {
    await app.close()
    rmSync(root, { recursive: true, force: true })
  }
}

const on = await banner(true)
assert.match(on ?? '', /Auto-vetting is on/)
assert.match(on, /~\/\.bravebot\/vetting/, 'names the file the answer is kept in')
assert.equal(await banner(false), null, 'asking is the ordinary state and says nothing')
console.log('Auto-vetting notice verified on and off. Screenshots in /tmp/bravebot-ui/vetting-*.png')
