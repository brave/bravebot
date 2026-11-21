// `/recap` and `/btw <question>` typed into the composer (docs/specs/commands.md, CMD-8, CMD-16).
//
// The real app and the real bridge, against a model service this script serves itself. A recap at
// rest and a /btw asked while a turn is held are each answered as a row of the transcript, and the
// next turn's request carries neither the question nor the answer.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const RECAP = 'We looked around the project and found three files.'
const BTW = 'It is waiting for the model to answer.'

let waiting = []
const hold = () => new Promise((resolve) => waiting.push(resolve))
const release = () => waiting.splice(0).forEach((resolve) => resolve())

const { service, rounds: requests } = await startModelService(async ({ messages }) => {
  const last = JSON.stringify(messages.at(-1) ?? '')
  if (last.includes('why is it slow')) return { content: BTW }
  if (last.includes('Hold on')) { await hold(); return { content: 'Done holding.' } }
  if (last.includes('Look around') || last.includes('Next step')) return { content: 'I looked around the project.' }
  return { content: RECAP }
})

const { project, profile, env } = makeWorld('bravebot-aside-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const transcript = () => page.locator('.entries').innerText()
  const until = async (check, what) => {
    for (let tries = 0; tries < 200; tries++) { if (await check()) return; await page.waitForTimeout(50) }
    assert.fail(what)
  }

  await composer.fill('Look around')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes('I looked around the project.'), 'the first turn answered')
  await stop.waitFor({ state: 'hidden' })

  // ---- /recap at rest ----------------------------------------------------------------------------
  await composer.fill('/recap')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes(RECAP), 'the recap is drawn')
  assert.equal(await page.locator('.bubble.user', { hasText: '/recap' }).count(), 0, 'no prompt bubble for /recap')
  await page.screenshot({ path: join(output, `aside-recap-${theme}.png`) })

  // ---- /btw beside a held turn -------------------------------------------------------------------
  await composer.fill('Hold on')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await until(async () => waiting.length > 0, 'the held turn reached the model')
  await composer.fill('/btw why is it slow')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes(BTW), 'the /btw answer is drawn while the turn runs')
  assert.ok(await stop.isVisible(), 'the turn is still running')
  assert.match(await transcript(), /asked beside the turn/)
  await page.screenshot({ path: join(output, `aside-btw-${theme}.png`) })
  release()
  await stop.waitFor({ state: 'hidden' })

  // ---- the next turn carries neither -------------------------------------------------------------
  await composer.fill('Next step')
  await composer.press('Enter')
  await until(async () => requests.some((body) => body.includes('Next step')), 'the next turn reached the model')
  await stop.waitFor({ state: 'hidden' })
  const next = requests.findLast((body) => body.includes('Next step'))
  for (const held of [RECAP, BTW, 'why is it slow', '/recap']) assert.ok(!next.includes(held), `the next turn's request does not hold ${held}`)

  assert.deepEqual(errors, [])
  console.log(`PASS: /recap and /btw are answered as rows and the next turn holds neither. Screenshots in ${output}/aside-*-${theme}.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'aside-failure.png') }).catch(() => undefined)
  throw error
} finally {
  release()
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
}
