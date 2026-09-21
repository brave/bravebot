// Real OpenRouter inference: incurs usage on the configured account.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs'
import { _electron as electron } from 'playwright-core'
const profile = mkdtempSync('/tmp/bravebot-model-live-profile-')
const project = mkdtempSync('/tmp/bravebot-model-live-project-')
const shots = '/tmp/bravebot-model-live'
mkdirSync(shots, { recursive: true })
const app = await electron.launch({ args: ['.', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  await page.evaluate(() => {
    window.modelLiveTurns = []
    window.bravebot.onEvent((event) => {
      if (event.event === 'turn.done') window.modelLiveTurns.push(event.data.model)
    })
  })
  await app.evaluate(({ ipcMain }, directory) => {
    ipcMain.removeHandler('bravebot:choose-directory')
    ipcMain.handle('bravebot:choose-directory', () => directory)
  }, project)
  await page.getByRole('button', { name: /^\+ New session$/ }).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  const trigger = page.locator('.model-trigger')
  await trigger.waitFor()
  assert.match(await trigger.getAttribute('aria-label'), /claude-haiku-4.5/)
  const send = async (prompt, expected, shot) => {
    const before = await page.locator('.bubble.assistant').count()
    await page.locator('.composer textarea').fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.waitForFunction((count) => {
      return document.querySelector('.unconfigured') || document.querySelector('.bubble.failed') ||
        (document.querySelectorAll('.bubble.assistant').length > count && !document.querySelector('.model-trigger')?.disabled)
    }, before, { timeout: 180000 })
    await page.screenshot({ path: `${shots}/${shot}.png` })
    assert.equal(await page.locator('.unconfigured, .bubble.failed').count(), 0, 'inference must succeed')
    const reply = await page.locator('.bubble.assistant').last().innerText()
    assert.match(reply, expected)
    console.log(`${shot}: ${reply.trim()}`)
  }
  await send('Do not use tools. Reply with exactly: HAIKU_OK', /HAIKU_OK/, '01-haiku')
  await trigger.click()
  const search = page.getByRole('combobox', { name: 'Search models' })
  await search.fill('claude-sonnet-4.5')
  const option = page.getByRole('option').filter({ has: page.locator('.model-name', { hasText: /^anthropic\/claude-sonnet-4\.5$/ }) })
  await option.first().waitFor({ timeout: 70000 })
  await option.first().click()
  console.log('Selected:', await trigger.getAttribute('aria-label'))
  assert.match(await trigger.getAttribute('aria-label'), /Sonnet 4.5|claude-sonnet-4.5/i)
  await send('Do not use tools. Reply with exactly: SONNET_OK', /SONNET_OK/, '02-sonnet')
  const models = await page.evaluate(() => window.modelLiveTurns)
  assert.deepEqual(models, ['anthropic/claude-haiku-4.5', 'anthropic/claude-sonnet-4.5'])
  console.log('Agent-reported models:', models.join(' → '))
  await trigger.click()
  await search.fill('claude-sonnet-4.5')
  await page.getByRole('option', { selected: true }).waitFor({ timeout: 70000 })
  await page.screenshot({ path: `${shots}/03-selected.png` })
  console.log('PASS: real inference before and after changing models in the same conversation')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
}
