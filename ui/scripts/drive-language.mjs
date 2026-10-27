// LOCALE-8 in the window: a language picked in Settings ▸ General is saved where `/language` keeps
// it, the window is read in it from the next start, and Automatic removes the saved choice. Real
// Electron and bridge on a temporary home and profile.
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { checker } from './drive-support.mjs'
import { launchWorld, makeWorld } from './drive-world.mjs'

const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })
const { check, problems } = checker()

const world = makeWorld('bravebot-language-', { baseURL: 'http://127.0.0.1:9/v1', project: false })
delete world.env.BRAVEBOT_LOCALE
Object.assign(world.env, { LANG: 'en_US.UTF-8' })
const saved = join(world.home, '.bravebot/language')

async function settings() {
  const launched = await launchWorld({ profile: world.profile, env: world.env, timeout: 15000 })
  const { app, page } = launched
  await page.waitForSelector('.app')
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
  })
  const control = page.locator('[data-test="settings-view"] [data-test="language-control"]')
  await control.waitFor()
  await page.waitForFunction(() => !document.querySelector('[data-test="language-control"]')?.hasAttribute('disabled'))
  return { ...launched, control }
}

const until = async (page, done) => {
  for (let tries = 0; tries < 50 && !done(); tries++) await page.waitForTimeout(100)
  return done()
}

try {
  {
    const { app, page, control, errors } = await settings()
    check(await page.locator('[data-test="settings-view"] h3', { hasText: 'Theme' }).count() === 1, 'the window starts in English')
    await control.click()
    await page.getByRole('option', { name: 'Français', exact: true }).click()
    check(await until(page, () => existsSync(saved) && readFileSync(saved, 'utf8') === 'fr\n'), 'choosing Français saves fr where /language keeps it')
    check(await page.locator('[data-test="language-restart"]').count() === 1, 'a Restart button is offered')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }
  {
    const { app, page, control, errors } = await settings()
    check(await page.locator('[data-test="settings-view"] h3', { hasText: 'Thème' }).count() === 1, 'after a new start the Theme heading is in French')
    check(await page.evaluate(() => document.documentElement.lang) === 'fr', 'and the document says it is French')
    await page.screenshot({ path: join(shots, 'language-fr.png') })
    await control.click()
    await page.getByRole('option', { name: 'Automatique', exact: true }).click()
    check(await until(page, () => !existsSync(saved)), 'choosing Automatic removes the saved choice')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }
} finally {
  rmSync(world.root, { recursive: true, force: true })
}

if (problems.length) {
  console.error(`\n${problems.length} check(s) failed`)
  process.exit(1)
}
console.log('\nall language checks passed')
