// LOCALE-8 in the window: a language picked in Settings ▸ General is saved where `/language` keeps
// it, the window is read in it from the next start, and Automatic removes the saved choice. A
// BRAVEBOT_LOCALE in the environment wins and the page says so, and Restart is held back while a
// turn runs. Real Electron and bridge on a temporary home and profile; only the model service is
// substituted. Screenshots go to $TMPDIR/bravebot-language, or LANGUAGE_SHOTS.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { checker, COMPOSER_INPUT, COMPOSER_SEND, COMPOSER_IDLE } from './drive-support.mjs'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

const shots = process.env.LANGUAGE_SHOTS ?? join(tmpdir(), 'bravebot-language')
mkdirSync(shots, { recursive: true })
const { check, problems } = checker()

let release = () => {}
const held = new Promise((resolve) => { release = resolve })
const { service, rounds } = await startModelService(async () => {
  await held
  return { content: 'Done.' }
})

const world = makeWorld('bravebot-language-', { service })
delete world.env.BRAVEBOT_LOCALE
Object.assign(world.env, { LANG: 'en_US.UTF-8' })
const saved = join(world.home, '.bravebot/language')

let current
// A control that has just been enabled fades into its colours, and a capture taken mid-fade shows
// it looking disabled: wait out every finite animation first.
const shot = async (name) => {
  await current.page.evaluate(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const roots = [document, ...[...document.querySelectorAll('*')].flatMap((element) => element.shadowRoot ? [element.shadowRoot] : [])]
    await Promise.all(roots.flatMap((root) => root.getAnimations())
      .filter((animation) => Number.isFinite(animation.effect?.getComputedTiming().endTime))
      .map((animation) => animation.finished.catch(() => {})))
  })
  await current.page.screenshot({ path: join(shots, `${name}.png`) })
}
const head = (page) => page.evaluate(() => ({
  title: document.querySelector('#settings-title')?.textContent,
  subtitle: document.querySelector('[data-test="settings-subtitle"]')?.textContent,
  crumbs: [...document.querySelectorAll('.settings-crumb')].map((crumb) => crumb.textContent),
}))

async function launch(env = world.env) {
  current = await launchWorld({ profile: world.profile, env, timeout: 15000, viewport: { width: 1280, height: 860 } })
  await current.page.waitForSelector('.app')
  return current
}

async function openGeneral({ app, page }) {
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
  })
  const control = page.locator('[data-test="settings-view"] [data-test="language-control"]')
  await control.waitFor()
  await page.waitForFunction(() => !document.querySelector('[data-test="language-control"]')?.hasAttribute('disabled'))
  return control
}

/** That each row of the page keeps its text and controls inside it, and nothing is cut short. */
async function laidOut(label) {
  const found = await current.page.evaluate(() => {
    const out = []
    const name = (element) => `${element.tagName.toLowerCase()} "${element.textContent.trim().slice(0, 40)}"`
    const main = document.querySelector('[data-test="settings-view"] .settings-main')
    if (!main) return ['no settings page']
    if (main.scrollWidth > main.clientWidth + 1) out.push(`the page scrolls sideways (${main.scrollWidth} > ${main.clientWidth})`)
    for (const row of main.querySelectorAll('.settings-row')) {
      const frame = row.getBoundingClientRect()
      for (const element of row.querySelectorAll('h3, p, leo-dropdown, leo-button, leo-alert')) {
        const rect = element.getBoundingClientRect()
        if (rect.width === 0 && rect.height === 0) continue
        if (rect.left < frame.left - 1 || rect.right > frame.right + 1 || rect.top < frame.top - 1 || rect.bottom > frame.bottom + 1) out.push(`${name(element)} spills out of its row`)
        if (element.scrollWidth > element.clientWidth + 1) out.push(`${name(element)} is cut short (${element.scrollWidth} > ${element.clientWidth})`)
        const inner = element.shadowRoot?.querySelector('button, .click-target, [part~="label"]')
        if (inner && inner.scrollWidth > inner.clientWidth + 1) out.push(`${name(element)}'s label is cut short (${inner.scrollWidth} > ${inner.clientWidth})`)
      }
      const style = getComputedStyle(row)
      const left = frame.left + parseFloat(style.paddingLeft)
      const right = frame.right - parseFloat(style.paddingRight)
      for (const alert of row.querySelectorAll('leo-alert')) {
        const rect = alert.getBoundingClientRect()
        if (Math.abs(rect.left - left) > 1 || Math.abs(rect.right - right) > 1) out.push(`the alert does not span its row (${Math.round(rect.left)}–${Math.round(rect.right)}, row ${Math.round(left)}–${Math.round(right)})`)
      }
      const text = row.querySelector('.settings-row-text')?.getBoundingClientRect()
      const control = row.querySelector('.settings-row-control')?.getBoundingClientRect()
      if (text && control && text.right > control.left + 1) out.push(`row "${row.querySelector('h3')?.textContent}" runs its text under its controls`)
    }
    return out
  })
  check(found.length === 0, `${label}: laid out (${found.join('; ')})`)
}

const until = async (page, done) => {
  for (let tries = 0; tries < 50 && !(await done()); tries++) await page.waitForTimeout(100)
  return done()
}
const heading = (page, text) => page.locator('[data-test="settings-view"] h3', { hasText: text })

try {
  // ---- English, then pick French -------------------------------------------------------------
  {
    const { app, page, errors } = await launch()
    const control = await openGeneral(current)
    check(await heading(page, 'Theme').count() === 1, 'the window starts in English')
    const english = await head(page)
    check(english.title === 'General' && english.subtitle === 'How this window looks and which language it uses'
      && english.crumbs.join(' / ') === 'Settings / General', `the page is headed in English (${JSON.stringify(english)})`)
    check(await heading(page, 'Display language').count() === 1 && await heading(page, /^Language$/).count() === 0,
      'the row is named Display language under the Language heading, not Language twice')
    check(!(await page.locator('[data-test="language-restart"] button').isDisabled()), 'Restart is offered once the languages are read')
    check(/next time/i.test(await page.locator('[data-test="language-note"]').innerText()), 'the note says it applies from the next start')
    await laidOut('English')
    await shot('01-general-en')
    await control.click()
    await page.getByRole('option', { name: 'Français', exact: true }).waitFor()
    await page.waitForTimeout(400)
    await shot('02-language-options-en')
    await page.getByRole('option', { name: 'Français', exact: true }).click()
    check(await until(page, () => existsSync(saved) && readFileSync(saved, 'utf8') === 'fr\n'), 'choosing Français saves fr where /language keeps it')
    check(await page.locator('[data-test="language-restart"]').count() === 1, 'a Restart button is offered')
    check(await heading(page, 'Theme').count() === 1, 'the page stays in English until the next start')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }

  // ---- French from the next start, a choice that cannot be saved, dark -------------------------
  {
    const { app, page, errors } = await launch()
    const control = await openGeneral(current)
    check(await heading(page, 'Thème').count() === 1, 'after a new start the Theme heading is in French')
    check(await page.evaluate(() => document.documentElement.lang) === 'fr', 'and the document says it is French')
    const french = await head(page)
    check(french.title === 'Général' && french.subtitle === "L'apparence de cette fenêtre et la langue qu'elle utilise"
      && french.crumbs.join(' / ') === 'Réglages / Général', `the page is headed in French (${JSON.stringify(french)})`)
    check(await heading(page, "Langue d'affichage").count() === 1, 'the row is named Langue d’affichage')
    check(!(await page.locator('[data-test="language-restart"] button').isDisabled()), 'Restart is offered after a new start')
    await laidOut('French')
    await shot('03-general-fr')

    // A directory where the file should be: the choice cannot be saved, and the page says so and
    // keeps showing the one that is.
    rmSync(saved)
    mkdirSync(join(saved, 'kept'), { recursive: true })
    await control.click()
    await page.getByRole('option', { name: 'English (US)', exact: true }).click()
    const alert = page.locator('[data-test="language-error"]')
    await alert.waitFor({ timeout: 5000 }).catch(() => {})
    check(await alert.count() === 1, 'a choice that could not be saved shows the alert')
    check(await control.evaluate((element) => element.value) === 'fr', 'and the picker goes back to the choice in force')
    check(existsSync(join(saved, 'kept')), 'and nothing was disturbed')
    await laidOut('French, not saved')
    await shot('04-unsaved-fr')
    rmSync(saved, { recursive: true, force: true })
    writeFileSync(saved, 'fr\n')

    await page.evaluate(() => window.bravebot.writeTheme('dark'))
    await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
    await page.waitForTimeout(400)
    await laidOut('French, dark')
    await shot('05-general-fr-dark')
    await page.evaluate(() => window.bravebot.writeTheme('system'))
    await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') !== 'dark')

    await control.click()
    await page.getByRole('option', { name: 'Automatique', exact: true }).click()
    check(await until(page, () => !existsSync(saved)), 'choosing Automatic removes the saved choice')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }

  // ---- BRAVEBOT_LOCALE wins over the saved choice ----------------------------------------------
  {
    writeFileSync(saved, 'fr\n')
    const { app, page, errors } = await launch({ ...world.env, BRAVEBOT_LOCALE: 'en-US' })
    const control = await openGeneral(current)
    check(await heading(page, 'Theme').count() === 1, 'BRAVEBOT_LOCALE=en-US reads the window in English over a saved fr')
    check(await control.evaluate((element) => element.value) === 'fr', 'the picker still shows the saved choice')
    check(/BRAVEBOT_LOCALE/.test(await page.locator('[data-test="language-note"]').innerText()), 'the note says BRAVEBOT_LOCALE wins')
    await laidOut('overridden')
    await shot('06-overridden-en')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
    rmSync(saved)
  }

  // ---- Restart is held back while a turn runs --------------------------------------------------
  {
    const { app, page, errors } = await launch()
    await openProject(page, app, world.project)
    await page.locator(COMPOSER_INPUT).fill('Take your time.')
    await page.locator(COMPOSER_SEND).click()
    check(await until(page, () => rounds.length > 0), 'the turn reached the model and is held there')
    await openGeneral(current)
    const restart = page.locator('[data-test="language-restart"]')
    const running = page.locator('[data-test="language-restart-running"]')
    await running.waitFor({ timeout: 5000 }).catch(() => {})
    check(await running.count() === 1, 'the page says a restart waits for the running turn')
    check(await restart.locator('button').isDisabled(), 'Restart is disabled while the turn runs')
    await laidOut('turn running')
    await shot('07-restart-running-en')
    release()
    check(await until(page, async () => (await running.count()) === 0), 'the note goes once the turn ends')
    check(await until(page, async () => !(await restart.locator('button').isDisabled())), 'and Restart is offered again')
    await page.locator('[data-test="settings-back"]').click()
    await page.locator(COMPOSER_IDLE).waitFor()
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }
} catch (error) {
  if (current?.page) await shot('failure').catch(() => undefined)
  throw error
} finally {
  await current?.app.close().catch(() => undefined)
  release()
  service.close()
  rmSync(world.root, { recursive: true, force: true })
}

if (problems.length) {
  console.error(`\n${problems.length} check(s) failed`)
  process.exit(1)
}
console.log(`\nall language checks passed (screenshots in ${shots})`)
