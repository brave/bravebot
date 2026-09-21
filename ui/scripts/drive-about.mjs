// Exercise the actual About menu and rendered eyes in an isolated Electron profile.
// Reduced motion freezes idle blinks so before/after comparisons measure the interaction.
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

for (const fallback of [false, true]) {
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
    if (fallback) {
      // Simulate a refused GPU context; command-line flags can still allow software WebGL.
      await page.addInitScript(() => {
        const getContext = HTMLCanvasElement.prototype.getContext
        HTMLCanvasElement.prototype.getContext = function (kind, ...args) {
          return kind.includes('webgl') ? null : getContext.call(this, kind, ...args)
        }
      })
      await page.reload()
    }
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.locator('.sidebar-tab').first().waitFor()
    await app.evaluate(({ Menu }) => {
      Menu.getApplicationMenu().getMenuItemById('app.about').click()
    })
    const dialog = page.getByRole('dialog', { name: 'About Brave Bot' })
    await dialog.waitFor()
    const mascot = dialog.getByRole('button', { name: 'Make Brave Bot wink' })
    const face = mascot.locator(fallback ? 'svg.bot-avatar' : 'canvas.bot-avatar')
    await face.waitFor()
    const picture = () => face.evaluate(el => el instanceof HTMLCanvasElement ? el.toDataURL() : el.outerHTML)
    await page.waitForTimeout(100)
    const neutral = await picture()
    await mascot.hover()
    await page.waitForTimeout(100)
    const curious = await picture()
    assert.notEqual(curious, neutral, 'hover changes the rendered eyes')
    assert.equal(await mascot.locator('.about-figure').evaluate(el => getComputedStyle(el).transform), 'none')
    await mascot.click()
    await page.waitForTimeout(90)
    assert.notEqual(await picture(), curious, 'click closes one eye')
    // A second click must not toggle it open or extend the cycle indefinitely.
    await mascot.evaluate(el => el.click())
    assert.notEqual(await picture(), curious, 'a repeated click lets the wink finish')
    await page.waitForTimeout(350)
    assert.equal(await picture(), curious, 'the eye reopens automatically')
    await page.keyboard.press('Space')
    await page.waitForTimeout(90)
    assert.notEqual(await picture(), curious, 'keyboard activation also winks')
    await page.waitForTimeout(350)
    assert.equal(await picture(), curious, 'keyboard wink completes')
    await page.mouse.move(0, 0)
    await page.keyboard.press('Tab')
    await page.waitForTimeout(100)
    assert.equal(await picture(), neutral, 'leaving the mascot restores its neutral expression')
    assert.equal(await dialog.getByText('Hello, there.', { exact: true }).count(), 0)
    const project = 'https://github.com/brave/bravebot'
    assert.equal(await dialog.getByRole('link', { name: 'GitHub' }).getAttribute('href'), project)
    await dialog.locator('summary').click()
    const build = await dialog.locator('dl > div').nth(1).locator('dd').textContent()
    assert.equal(await dialog.locator('.about-version').textContent(), `Version ${build.split(' ')[0]}`)
    await page.screenshot({ path: join(tmpdir(), `bravebot-about-${fallback ? 'svg' : 'webgl'}.png`) })
    // Closing mid-wink must safely release the timer and avatar.
    await mascot.click()
    await page.keyboard.press('Escape')
    await dialog.waitFor({ state: 'detached' })
    await page.waitForTimeout(400)
    assert.deepEqual(errors, [])
    console.log(`PASS: ${fallback ? 'SVG fallback' : 'WebGL'} About menu, hover, full wink, repeat click, keyboard, version, links, close`)
  } finally {
    await app.close()
    rmSync(profile, { recursive: true, force: true })
  }
}
