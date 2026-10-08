// Exercise the actual About menu in an isolated Electron profile.
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const profile = mkdtempSync(join(tmpdir(), 'bravebot-about-'))
const app = await electron.launch({
  args: ['.', `--user-data-dir=${profile}`, '--disable-renderer-backgrounding'],
  cwd: process.cwd(), timeout: 40000,
})
try {
  const page = await app.firstWindow()
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.waitForLoadState('domcontentloaded')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.locator('.sidebar-tabs [role="option"]').first().waitFor()
  await app.evaluate(({ Menu }) => {
    Menu.getApplicationMenu().getMenuItemById('app.about').click()
  })
  const dialog = page.getByRole('dialog', { name: 'About Brave Bot' })
  await dialog.waitFor()
  assert.equal(await dialog.locator('svg.bot-avatar').count(), 0, 'the dialog shows no bot avatar')
  assert.equal(await dialog.getByText('Hello, there.', { exact: true }).count(), 0)
  const project = 'https://github.com/brave/bravebot'
  // Each launch icon sits on the same line as its link text.
  const links = dialog.locator('.about-links leo-link')
  assert.equal(await links.count(), 2)
  for (let i = 0; i < 2; i++) {
    const link = links.nth(i)
    const box = await link.evaluate(el => el.getBoundingClientRect().toJSON())
    const icon = await link.locator('leo-icon').evaluate(el => el.getBoundingClientRect().toJSON())
    assert.ok(icon.top >= box.top - 1 && icon.bottom <= box.bottom + 1, `link ${i}: icon is inside the link's line`)
    assert.ok(box.height < 32, `link ${i}: icon does not stack under the text`)
  }
  assert.equal(await dialog.getByRole('link', { name: 'GitHub' }).getAttribute('href'), project)
  await dialog.locator('summary').click()
  const build = await dialog.locator('dl > div').nth(1).locator('dd').textContent()
  assert.equal(await dialog.locator('.about-version').textContent(), `Version ${build.split(' ')[0]}`)
  await page.screenshot({ path: join(tmpdir(), 'bravebot-about.png') })
  await page.keyboard.press('Escape')
  await dialog.waitFor({ state: 'detached' })
  // A click on the backdrop closes it too; a keyboard click (no coordinates) above did not.
  await app.evaluate(({ Menu }) => { Menu.getApplicationMenu().getMenuItemById('app.about').click() })
  await dialog.waitFor()
  await page.mouse.click(4, 4)
  await dialog.waitFor({ state: 'detached' })
  await page.waitForTimeout(400)
  assert.deepEqual(errors, [])
  console.log('PASS: About menu, no avatar, version, links, close')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
}
