// CHECK-11 in the window: a session opened with auto-vetting on says so above its transcript, and
// one opened without it says nothing. Real Electron and bridge; only the file picker is substituted.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { inheritedEnv, launchWorld, openProject } from './drive-world.mjs'

async function banner(auto) {
  const root = mkdtempSync(join(tmpdir(), 'bravebot-vetting-'))
  const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
  for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
  writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ vetting: { auto } }))
  const env = Object.assign(inheritedEnv(), { HOME: home, XDG_CONFIG_HOME: join(home, '.config') })
  let app
  try {
    const launched = await launchWorld({ profile, env, timeout: 15000 })
    app = launched.app
    const { page, errors } = launched
    await openProject(page, app, project)
    const notice = page.locator('.entries .session-banner')
    if (auto) await notice.waitFor()
    const shown = await notice.count() ? await notice.innerText() : null
    mkdirSync('/tmp/bravebot-ui', { recursive: true })
    await page.screenshot({ path: `/tmp/bravebot-ui/vetting-${auto ? 'on' : 'off'}.png` })
    assert.deepEqual(errors, [])
    return shown
  } finally {
    await app?.close()
    rmSync(root, { recursive: true, force: true })
  }
}

const on = await banner(true)
assert.match(on ?? '', /Auto-vetting is on/)
assert.match(on, /~\/\.bravebot\/vetting/, 'names the file the answer is kept in')
assert.equal(await banner(false), null, 'asking is the ordinary state and says nothing')
console.log('Auto-vetting notice verified on and off. Screenshots in /tmp/bravebot-ui/vetting-*.png')
