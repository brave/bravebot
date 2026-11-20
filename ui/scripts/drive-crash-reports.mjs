// CRASH-11 and CRASH-12 in the window: the one-time crash reports notice and the switch in
// Settings, General, Privacy.
//
// Four launches, each on a temporary home and profile, against the real bridge: a first launch
// that is reporting says so under the welcome, with a More toggle, and a second launch (same home)
// does not; a launch told in another front end (the marker the terminal writes) says nothing; a
// launch with BRAVEBOT_CRASH_REPORTS=0 says nothing, writes no marker and shows the switch off and
// locked with the variable's name; and the switch writes `crashReports` into the person's own
// settings file and keeps the other keys. Screenshots in light and dark.
//
// A bridge built without a submission address reports nothing and says nothing (CRASH-3), so this
// needs `bravebot-rpc` built with one. `pnpm run drive:crash-reports` builds it that way, which
// `ui/scripts/build-bridge.sh` does not do (its check builds drop the address on purpose). Nothing
// is sent: no report is waiting, and the address is a closed local port. Run `pnpm run build`
// afterwards to put back the bridge a development build uses. Screenshots go to
// CRASH_REPORTS_SHOTS, or $TMPDIR/bravebot-crash-reports.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { checker } from './drive-support.mjs'
import { launchWorld, makeWorld, startModelService } from './drive-world.mjs'

const shots = process.env.CRASH_REPORTS_SHOTS ?? join(tmpdir(), 'bravebot-crash-reports')
mkdirSync(shots, { recursive: true })
const { check, problems } = checker()
const { service } = await startModelService(async () => ({ content: 'Done.' }))

const NOTICE = '[data-test="crash-reports-notice"]'
const world = makeWorld('bravebot-crash-reports-', { service })
delete world.env.BRAVEBOT_CRASH_REPORTS
const marker = join(world.home, '.bravebot/crash-reports/notice')
const settingsFile = join(world.home, '.bravebot/settings.json')

let current
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
const theme = async (value) => {
  await current.page.evaluate((chosen) => window.bravebot.writeTheme(chosen), value)
  await current.page.waitForFunction((chosen) => chosen === 'system' ? document.documentElement.getAttribute('data-theme') !== 'dark' : document.documentElement.getAttribute('data-theme') === chosen, value)
  await current.page.waitForTimeout(300)
}

async function launch(env = world.env) {
  current = await launchWorld({ profile: world.profile, env, timeout: 15000, viewport: { width: 1280, height: 860 } })
  await current.page.waitForSelector('.app')
  // The window asks the bridge once as it opens, so give that answer time to arrive.
  await current.page.waitForTimeout(1200)
  return current
}

async function openGeneral({ app, page }) {
  await app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0].webContents.send('bravebot:command', 'view.theme', null)
  })
  const row = page.locator('[data-test="settings-view"] [data-test="crash-reports-row"]')
  await row.waitFor()
  await page.waitForFunction(() => {
    const control = document.querySelector('[data-test="crash-reports-control"]')
    return control && (control.hasAttribute('disabled') || control.checked !== undefined)
  })
  await page.waitForTimeout(400)
  return page.locator('[data-test="crash-reports-control"]')
}

/** That the notice's text and buttons stay inside it, and it does not make the page scroll sideways. */
async function laidOut(label) {
  const found = await current.page.evaluate((selector) => {
    const out = []
    const notice = document.querySelector(selector)
    if (!notice) return ['no notice']
    const frame = notice.getBoundingClientRect()
    for (const element of notice.querySelectorAll('.notice-text, .notice-actions, leo-button, button')) {
      const rect = element.getBoundingClientRect()
      if (rect.width === 0 && rect.height === 0) continue
      if (rect.left < frame.left - 1 || rect.right > frame.right + 1 || rect.top < frame.top - 1 || rect.bottom > frame.bottom + 1) out.push(`${element.className || element.tagName} spills out of the notice`)
    }
    // The More link sits inside the sentence, as the usage ping notice's does, so it is held to
    // that notice's size, and only the standalone buttons are held to a hit target.
    for (const target of notice.querySelectorAll('leo-button, button:not(.notice-more)')) {
      const rect = target.getBoundingClientRect()
      if (rect.width > 0 && Math.min(rect.width, rect.height) < 20) out.push(`${target.tagName} is ${Math.round(rect.width)}x${Math.round(rect.height)}`)
    }
    const more = notice.querySelector('.notice-more')
    const ping = document.querySelector('[data-test="usage-ping-notice"] .notice-more')
    if (more && ping && Math.abs(more.getBoundingClientRect().height - ping.getBoundingClientRect().height) > 1) out.push('the More link is not the height of the usage ping notice\u2019s')
    if (document.documentElement.scrollWidth > document.documentElement.clientWidth + 1) out.push('the page scrolls sideways')
    return out
  }, NOTICE)
  check(found.length === 0, `${label}: laid out (${found.join('; ')})`)
}

const settings = () => JSON.parse(readFileSync(settingsFile, 'utf8'))

/**
 * That the last row of Privacy, the crash reports switch, can be reached: the settings page scrolls to
 * it, and once it is scrolled to the whole row and its switch are inside both the scrolling area and
 * the window. The row is below the fold at this window's height, so the screenshots after it are taken
 * with the row scrolled into view.
 */
async function lastPrivacyRowReachable(label) {
  const { found, metrics } = await current.page.evaluate(async () => {
    const row = document.querySelector('[data-test="settings-view"] [data-test="crash-reports-row"]')
    const scroller = document.querySelector('[data-test="settings-view"] .settings-scroll')
    if (!row || !scroller) return { found: ['the last Privacy row or the scrolling area is missing'], metrics: null }
    const control = row.querySelector('[data-test="crash-reports-control"]')
    const before = scroller.scrollTop
    row.scrollIntoView({ block: 'end' })
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))
    const area = scroller.getBoundingClientRect()
    const inside = (rect) => rect.height > 0 && rect.top >= area.top - 1 && rect.bottom <= area.bottom + 1 && rect.bottom <= innerHeight + 1
    const out = []
    if (getComputedStyle(scroller).overflowY !== 'auto' && getComputedStyle(scroller).overflowY !== 'scroll') out.push('the settings page does not scroll')
    if (area.bottom > innerHeight + 1) out.push('the scrolling area runs below the window')
    if (!inside(row.getBoundingClientRect())) out.push('the row is not inside the window once scrolled to')
    if (!control || !inside(control.getBoundingClientRect())) out.push('the switch is not inside the window once scrolled to')
    return { found: out, metrics: { scrolledBy: Math.round(scroller.scrollTop - before), scrollable: scroller.scrollHeight - scroller.clientHeight, windowHeight: innerHeight } }
  })
  check(found.length === 0, `${label}: the last Privacy row is reachable (${found.join('; ')})`)
  if (metrics) console.log(`${label}: the page scrolls ${metrics.scrollable}px and bringing the last row into view moved it ${metrics.scrolledBy}px in a ${metrics.windowHeight}px window`)
}

/** That the notice's text is dark ink on a light surface in the light theme and the reverse in the dark one. */
async function inkFollowsTheme(label, dark) {
  const lightness = await current.page.evaluate((selector) => {
    const text = document.querySelector(`${selector} .notice-text`)
    const [red, green, blue] = getComputedStyle(text).color.match(/[\d.]+/g).map(Number)
    return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255
  }, NOTICE)
  check(dark ? lightness > 0.5 : lightness < 0.5, `${label}: the notice text is ${dark ? 'light' : 'dark'} ink (${lightness.toFixed(2)})`)
}

try {
  // ---- first launch that reports: said once, under the welcome -------------------------------
  {
    const { page, errors, app } = await launch()
    const notice = page.locator(NOTICE)
    check(await notice.count() === 1, 'the first launch that is reporting says so under the welcome (needs a bridge built with an address)')
    check(/sends Brave a report/.test(await notice.innerText()), 'it says that a crash sends Brave a report')
    check(!/operating system/.test(await notice.innerText()), 'the details wait for More')
    check(existsSync(marker), 'the window recorded it in the marker the terminal reads')
    check(await page.locator('[data-test="usage-ping-notice"]').count() <= 1, 'the usage ping notice is drawn at most once beside it')
    await laidOut('first launch')
    await inkFollowsTheme('light', false)
    check(await page.getByText('sends Brave a report').count() === 1, 'the sentence is on the page once')
    check(await notice.getAttribute('role') === 'status', 'the notice is announced as a status')
    check(await page.getByRole('button', { name: 'Dismiss crash reports notice' }).count() === 1, 'the dismiss button has an accessible name')
    check(await page.getByRole('button', { name: 'Open settings' }).count() >= 1, 'and so does the settings button')
    await shot('01-notice-light')
    // The More link works from the keyboard and reports its state.
    await page.locator('[data-test="crash-reports-notice-more"]').focus()
    check(await page.evaluate(() => document.activeElement?.getAttribute('data-test')) === 'crash-reports-notice-more', 'More takes keyboard focus')
    await page.keyboard.press('Enter')
    check(await page.locator('[data-test="crash-reports-notice-more"]').getAttribute('aria-expanded') === 'true', 'Enter on More expands the notice')
    await page.keyboard.press('Enter')
    check(await page.locator('[data-test="crash-reports-notice-more"]').getAttribute('aria-expanded') === 'false', 'and Enter again folds it')
    await page.locator('[data-test="crash-reports-notice-more"]').click()
    const text = await notice.innerText()
    check(/operating system/.test(text) && /never holds your prompts, files or keys/.test(text), 'More says what a report holds')
    check(/Settings, under General/.test(text) && /BRAVEBOT_CRASH_REPORTS=0/.test(text), 'and both ways to turn reports off')
    check(await page.locator('[data-test="crash-reports-notice-more"]').getAttribute('aria-expanded') === 'true', 'More reports its state to assistive technology')
    await laidOut('expanded')
    await shot('02-notice-expanded-light')
    await theme('dark')
    await laidOut('expanded, dark')
    await inkFollowsTheme('dark', true)
    await shot('03-notice-expanded-dark')
    await theme('system')
    await page.locator('[data-test="crash-reports-notice-dismiss"]').click()
    await notice.waitFor({ state: 'detached' })
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }

  // ---- the same home again: not said again -----------------------------------------------------
  {
    const { page, errors, app } = await launch()
    check(await page.locator(NOTICE).count() === 0, 'a second launch of the same home says nothing')
    // ---- the switch: on, then off, written into the person's own file --------------------------
    const control = await openGeneral(current)
    check(await control.evaluate((element) => element.checked) === true, 'the switch reads on')
    check(await page.locator('[data-test="crash-reports-locked"]').count() === 0, 'and says nothing is holding it')
    check(await page.getByRole('switch', { name: 'Send crash reports to Brave' }).count() === 1, 'the switch is a switch named for what it does')
    // The same switch from the keyboard, before any pointer use: focus it, Space turns it off, Space turns it on.
    const inner = page.locator('[data-test="crash-reports-control"] [role="switch"]')
    const enabled = async () => { for (let tries = 0; tries < 50 && !(await inner.isEnabled()); tries++) await page.waitForTimeout(100) }
    await enabled()
    await inner.focus()
    check(await page.evaluate(() => document.querySelector('[data-test="crash-reports-control"]')?.matches(':focus-within') || document.activeElement?.getAttribute('data-test') === 'crash-reports-control'), 'the switch takes keyboard focus')
    check(await inner.getAttribute('aria-checked') === 'true', 'it reads on to assistive technology')
    await inner.press('Space')
    for (let tries = 0; tries < 30 && settings().crashReports !== false; tries++) await page.waitForTimeout(100)
    check(settings().crashReports === false, 'Space on the focused switch turns reports off')
    await enabled()
    check(await inner.getAttribute('aria-checked') === 'false', 'and it reads off to assistive technology')
    check(await page.evaluate(() => document.querySelector('[data-test="crash-reports-control"]')?.matches(':focus-within') || document.activeElement?.getAttribute('data-test') === 'crash-reports-control'), 'the switch keeps keyboard focus')
    await inner.press('Space')
    for (let tries = 0; tries < 30 && settings().crashReports !== true; tries++) await page.waitForTimeout(100)
    check(settings().crashReports === true && await inner.getAttribute('aria-checked') === 'true', 'and Space again turns them on')
    await lastPrivacyRowReachable('light')
    await shot('04-switch-on-light')
    await control.click()
    await page.waitForFunction(() => document.querySelector('[data-test="crash-reports-control"]')?.checked === false)
    for (let tries = 0; tries < 30 && !(existsSync(settingsFile) && settings().crashReports === false); tries++) await page.waitForTimeout(100)
    check(existsSync(settingsFile) && settings().crashReports === false, 'turning it off writes crashReports: false into the settings file')
    check(settings().model === 'local/test' && settings().provider?.local !== undefined, 'and keeps the file\u2019s other keys')
    check(await page.locator('[data-test="crash-reports-error"]').count() === 0, 'with no error shown')
    await lastPrivacyRowReachable('light, off')
    await shot('05-switch-off-light')
    await theme('dark')
    await lastPrivacyRowReachable('dark, off')
    await shot('06-switch-off-dark')
    await theme('system')
    await control.click()
    for (let tries = 0; tries < 30 && settings().crashReports !== true; tries++) await page.waitForTimeout(100)
    check(settings().crashReports === true, 'turning it on writes true, and does not remove the key')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }

  // ---- told by another front end: not said here ------------------------------------------------
  {
    rmSync(marker)
    writeFileSync(marker, '')
    const { page, errors, app } = await launch()
    check(await page.locator(NOTICE).count() === 0, 'a person the terminal already told is not told by the window')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }

  // ---- the variable turns reports off: nothing said, nothing recorded, switch locked ------------
  {
    rmSync(join(world.home, '.bravebot/crash-reports'), { recursive: true, force: true })
    const { page, errors, app } = await launch({ ...world.env, BRAVEBOT_CRASH_REPORTS: '0' })
    check(await page.locator(NOTICE).count() === 0, 'with BRAVEBOT_CRASH_REPORTS=0 the window says nothing about reports')
    check(!existsSync(marker), 'and records nothing')
    const control = await openGeneral(current)
    check(await control.evaluate((element) => element.checked) === false, 'the switch reads off')
    check(await control.evaluate((element) => element.hasAttribute('disabled') || element.disabled === true), 'and cannot be changed')
    const locked = page.locator('[data-test="crash-reports-locked"]')
    check(/BRAVEBOT_CRASH_REPORTS/.test(await locked.innerText()), 'the row names the variable that holds it off')
    const before = readFileSync(settingsFile, 'utf8')
    await control.click({ force: true }).catch(() => {})
    await page.waitForTimeout(400)
    check(readFileSync(settingsFile, 'utf8') === before, 'a click on it writes nothing')
    await lastPrivacyRowReachable('light, locked')
    await shot('07-switch-locked-light')
    await theme('dark')
    await lastPrivacyRowReachable('dark, locked')
    await shot('08-switch-locked-dark')
    await theme('system')
    check(errors.length === 0, `no page errors: ${errors.join('; ')}`)
    await app.close()
  }
} catch (error) {
  if (current?.page) await shot('failure').catch(() => undefined)
  throw error
} finally {
  await current?.app.close().catch(() => undefined)
  service.close()
  rmSync(world.root, { recursive: true, force: true })
}

if (problems.length) {
  console.error(`\n${problems.length} check(s) failed`)
  process.exit(1)
}
console.log(`\nall crash reports checks passed (screenshots in ${shots})`)
