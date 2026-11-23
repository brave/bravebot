// That the session spend limit set in the composer holds the next turn (TURN-8): a small limit is
// set through the composer's spend control, a prompt is sent, the limit question comes up in the
// window before the second request, and answering "go on without a limit" clears the control.
//
// The real app and the real bridge, against a model service this script serves itself, which
// reports 11 tokens a request. Nothing is paid for.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run build` does.
import assert from 'node:assert/strict'
import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_INPUT, COMPOSER_SEND, APPROVE_BUTTON, SHOTS_DIR } from './drive-support.mjs'

// The first answer asks for a listing, so the turn needs a second request, which the limit holds.
const { service, rounds } = await startModelService(({ messages }) =>
  messages.at(-1)?.role === 'tool'
    ? { content: 'done' }
    : { tool: { name: 'list_files', arguments: { directory: '.' } } })

const { root, project, profile, env } = makeWorld('bravebot-spend-limit-', { service })

mkdirSync(SHOTS_DIR, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  await openProject(page, app, project)

  const control = page.locator('[data-test="spend-limit"]')
  // The Leo host has no box of its own, so the limit is read off its attribute.
  const limitIs = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="spend-limit"]')?.getAttribute('data-limit') === wanted, wanted)
  await control.waitFor({ state: 'attached' })
  assert.equal(await control.getAttribute('data-limit'), 'none', 'a session with no limit setting opens with none')

  await control.click()
  const field = page.locator('[data-test="spend-limit-field"] input')
  await field.waitFor()
  await field.fill('5')
  await page.locator('[data-test="spend-limit-set"]').click()
  await limitIs('5 tokens')
  await page.screenshot({ path: join(SHOTS_DIR, 'spend-limit-set.png') })

  await page.locator(COMPOSER_INPUT).fill('List the files')
  await page.locator(COMPOSER_SEND).click()
  const card = page.locator('.confirm.ask:not([data-folded="true"])')
  await card.waitFor({ timeout: 60000 })
  const question = (await card.locator('.ask-question .question').first().textContent())?.trim() ?? ''
  assert.match(question, /11/, `the question says what was spent: ${question}`)
  assert.equal(rounds.length, 1, 'no request was sent past the limit before the person answered')
  await page.screenshot({ path: join(SHOTS_DIR, 'spend-limit-question.png') })

  await card.locator('.choices .choice', { hasText: /without a limit/i }).click()
  await card.locator(APPROVE_BUTTON).click()
  await limitIs('none')
  await page.locator('.composer .stop').waitFor({ state: 'hidden' })
  assert.equal(rounds.length, 2, 'going on without a limit sent the held request')
  assert.equal(await control.getAttribute('data-limit'), 'none', 'the control is clear after the turn')
  await page.screenshot({ path: join(SHOTS_DIR, 'spend-limit-cleared.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: a limit set in the composer brings up the limit question before the next request, and going on without a limit clears the control. Screenshots in ${SHOTS_DIR}/spend-limit-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(SHOTS_DIR, 'spend-limit-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
