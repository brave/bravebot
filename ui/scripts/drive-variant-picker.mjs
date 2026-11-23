// That a model's named option variant picked in the composer changes the next turn's request
// (BACKEND-66): the settings give the model two variants, the composer shows the control, `terse`
// is picked, and the request the turn sends carries terse's options. Off drops them again.
//
// The real app and the real bridge, against a model service this script serves itself. Nothing is
// paid for. Screenshots of the control and its menu are taken in the light and the dark theme.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run build` does.
import assert from 'node:assert/strict'
import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_INPUT, COMPOSER_SEND, SHOTS_DIR } from './drive-support.mjs'

const { service, rounds } = await startModelService(() => ({ content: 'done' }))
const baseURL = `http://127.0.0.1:${service.address().port}/v1`
const { root, project, profile, env } = makeWorld('bravebot-variant-picker-', {
  service,
  settings: {
    provider: { local: { options: { baseURL }, models: { test: { variants: {
      terse: { verbosity: 'low' },
      wide: { verbosity: 'high' },
      unused: { disabled: true },
    } } } } },
  },
})

mkdirSync(SHOTS_DIR, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
  await openProject(page, app, project)

  const trigger = page.locator('[data-test="variant-trigger"]')
  const variantIs = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="variant-trigger"]')?.getAttribute('data-variant') === wanted, wanted)
  await trigger.waitFor({ state: 'attached' })
  await variantIs('off')

  const shoot = async (name) => {
    for (const scheme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme: scheme })
      await page.waitForTimeout(150)
      await page.screenshot({ path: join(SHOTS_DIR, `variant-picker-${name}-${scheme}.png`) })
    }
  }

  await trigger.click()
  const option = (name) => page.locator(`[data-test="variant-option-${name}"]`)
  await option('terse').waitFor()
  assert.equal(await option('unused').count(), 0, 'a disabled variant is not offered')
  await shoot('menu')
  await option('terse').click()
  await variantIs('terse')
  await page.waitForTimeout(200)
  await shoot('picked')

  const send = async (text) => {
    const before = rounds.length
    await page.locator(COMPOSER_INPUT).fill(text)
    await page.locator(COMPOSER_SEND).click()
    for (let waited = 0; rounds.length === before; waited += 100) {
      assert.ok(waited < 60000, 'the turn sent a request')
      await page.waitForTimeout(100)
    }
    await page.locator('.composer .stop').waitFor({ state: 'hidden', timeout: 60000 })
    return JSON.parse(rounds.at(-1))
  }

  const picked = await send('Say done')
  assert.equal(picked.verbosity, 'low', `terse's option reaches the request: ${JSON.stringify(picked)}`)

  await trigger.click()
  await option('off').click()
  await variantIs('off')
  const dropped = await send('Say done again')
  assert.ok(!('verbosity' in dropped), `with Off the request carries no variant option: ${JSON.stringify(dropped)}`)

  assert.deepEqual(errors, [])
  console.log(`PASS: a variant picked in the composer reaches the next request, and Off drops it. Screenshots in ${SHOTS_DIR}/variant-picker-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(SHOTS_DIR, 'variant-picker-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
