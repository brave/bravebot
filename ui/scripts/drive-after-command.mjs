// `/after` typed into the composer (docs/specs/loop.md, LOOP-18, LOOP-6, LOOP-13).
//
// The real app and the real bridge, against a model service this script serves itself. A typed
// `/after 5s say hi` is answered as a row and sends nothing then, and the row beside the composer
// says it is sending in the time left. One typed while a turn runs is taken at once, and once its
// delay has passed while the turn still runs the row says it is sending later. When the turn ends
// the line is sent once as a prompt, the ending says it was sent, and the row is gone.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, heldTurns, SHOTS_DIR, untilIn } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const { hold, release } = heldTurns()

const { service, rounds } = await startModelService(async ({ messages }) => {
  // The last two: a typed turn may carry a note after the person's line.
  const last = JSON.stringify(messages.slice(-2))
  if (last.includes('Hold on') && !last.includes('say hi')) { await hold(); return { content: 'Done holding.' } }
  return { content: 'Hi.' }
})
const prompts = () => rounds.filter((body) => !body.includes('You are a prompt-injection classifier.'))

const { project, profile, env } = makeWorld('bravebot-after-command-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const row = page.locator('[data-test="loop-status"]')
  const transcript = () => page.locator('.entries').innerText()
  const until = untilIn(page, 400)

  // ---- answered at once, sent later ----------------------------------------------------------------
  await composer.fill('/after 20m check the deploy')
  await composer.press('Enter')
  // Read again on the answer, before the next poll could have come.
  await row.waitFor({ timeout: 1500 })
  await until(async () => (await row.count()) > 0 && /^Sending in 19m \d+s$/.test(await row.innerText()), 'the row says when the line goes')
  assert.match(await transcript(), /Sending once in 20m as loop 1\. \/loop stop 1 cancels it, and so does Stop or closing the chat\./, 'the answer names the delay and the number')
  assert.equal(await page.locator('.bubble.user', { hasText: '/after' }).count(), 0, 'no prompt bubble for /after')
  assert.ok(!prompts().some((body) => body.includes('check the deploy')), 'nothing was sent yet')
  await page.screenshot({ path: join(output, `after-command-${theme}.png`) })
  await composer.fill('/loop stop 1')
  await composer.press('Enter')
  await until(async () => (await row.count()) === 0, '/loop stop ends an /after line')

  // ---- typed during a turn, due while it runs, sent once when it ends ------------------------------
  await composer.fill('Hold on')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('/after 5s say hi')
  await composer.press('Enter')
  await until(async () => (await row.count()) > 0 && /^Sending in/.test(await row.innerText()), 'taken while the turn runs')
  await until(async () => (await row.innerText()) === 'Sending after this turn', 'once its delay passed, the row says it waits for the turn')
  assert.ok(!prompts().some((body) => body.includes('say hi')), 'it waits for the turn')
  await page.screenshot({ path: join(output, `after-command-later-${theme}.png`) })
  release()
  await until(async () => /Loop 2 · sent once\s*·\s*say hi/.test(await transcript()), 'the line was sent once, in one row')
  await until(async () => (await row.count()) === 0, 'nothing is left')
  await stop.waitFor({ state: 'hidden' })
  assert.ok(prompts().some((body) => body.includes('say hi')), 'the line reached the model')
  assert.ok(!prompts().some((body) => body.includes('/after')), 'the command word never reached the model')
  // A send that does not happen has no event to wait on. Only the main process's clock sends a
  // held line, calling `loops.poll` every 1000 ms (ui/src/main/index.ts), so 2.5 s covers at least
  // two polls that could have sent it again; the row gone above already says no loop 2 is held.
  await page.waitForTimeout(2500)
  const sent = await transcript()
  assert.equal(sent.match(/Loop 2 · sent once/g)?.length, 1, 'no second send')
  assert.ok(!/Loop 2 ended/.test(sent), 'no separate ending row for a line sent once')
  assert.ok(!/Loop 2, tick/.test(sent), 'no tick row for a line sent once')
  await page.screenshot({ path: join(output, `after-command-sent-${theme}.png`) })

  assert.deepEqual(errors, [], 'the page reported no errors')
  console.log(`drive-after-command: ok (${theme})`)
} finally {
  await app?.close()
  service.close()
}
