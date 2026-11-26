// `/loop` typed into the composer (docs/specs/loop.md, LOOP-3, LOOP-4, LOOP-6, LOOP-11, LOOP-13).
//
// The real app and the real bridge, against a model service this script serves itself. A typed
// `/loop 10s say hi` is answered as a row, its first tick follows at once under that answer, and
// the line goes to the model as a prompt while `/loop` itself never does. A `/loop` typed while a
// turn runs waits in the queue, and a `/loop stop` typed behind it waits behind it, so the stop
// reads two loops live and ends neither.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, heldTurns, SHOTS_DIR, untilIn } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const { hold, release, waiting } = heldTurns()

const { service, rounds } = await startModelService(async ({ messages }) => {
  const last = JSON.stringify(messages.at(-1) ?? '')
  if (last.includes('Hold on')) { await hold(); return { content: 'Done holding.' } }
  if (last.includes('second loop')) return { content: 'Second loop ran.' }
  return { content: 'Hi.' }
})
const prompts = () => rounds.filter((body) => !body.includes('You are a prompt-injection classifier.'))

const { project, profile, env } = makeWorld('bravebot-loop-command-', { service })
const output = SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const transcript = () => page.locator('.entries').innerText()
  const until = untilIn(page, 400)

  // ---- the answer and the first tick -------------------------------------------------------------
  const loopRow = page.locator('[data-test="loop-status"]')
  await composer.fill('/loop 10s say hi')
  await composer.press('Enter')
  await until(async () => /Loop 1, tick 1/.test(await transcript()), 'the first tick row')
  // The row beside the composer is read again on the tick's event, not on the next poll.
  await loopRow.waitFor({ timeout: 1500 })
  await until(async () => (await transcript()).includes('Hi.'), 'the first tick answered')
  await stop.waitFor({ state: 'hidden' })
  await until(async () => /^Loop live · next in \d+s$/.test(await loopRow.innerText()), 'the row counts down between ticks')
  const first = await transcript()
  assert.match(first, /Loop 1 repeats every 10s\. \/loop stop 1 ends it, and so does Stop or closing the chat\./)
  const answered = first.indexOf('Loop 1 repeats every 10s')
  assert.ok(answered < first.indexOf('Loop 1, tick 1'), 'the answer is above the tick it sent')
  assert.ok(first.indexOf('Loop 1, tick 1') < first.indexOf('Hi.'), 'the tick is above its reply')
  assert.equal((await page.locator('[data-test="loop-tick"]').first().innerText()).replace(/\s+/g, ' ').trim(), 'Loop 1, tick 1 · say hi')
  assert.equal(await page.locator('.loop-turn-prompt').first().getAttribute('data-tooltip'), 'say hi')
  assert.ok(!first.includes('sent again'), 'no fixed tail on the tick row')
  assert.equal(await page.locator('.bubble.user', { hasText: '/loop' }).count(), 0, 'no prompt bubble for /loop')
  assert.ok(prompts().some((body) => body.includes('say hi')), 'the line reached the model')
  assert.ok(!prompts().some((body) => body.includes('/loop')), 'the command word never reached the model')
  await page.screenshot({ path: join(output, `loop-command-${theme}.png`) })

  // ---- a /loop typed while a turn runs waits, and a /loop stop waits behind it ---------------------
  await composer.fill('Hold on')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await until(async () => waiting.length > 0, 'the held turn reached the model')
  await composer.fill('/loop 20m second loop')
  await composer.press('Enter')
  await composer.fill('/loop stop')
  await composer.press('Enter')
  await until(async () => (await page.evaluate(() => document.body.innerText)).includes('/loop stop'), 'the queued lines are shown')
  // Loop commands start something other than a turn, so the queue does not call them turns.
  assert.ok((await page.evaluate(() => document.body.innerText)).includes('Each starts after this one'), 'the queue caption names commands')
  await page.screenshot({ path: join(output, `loop-command-queued-${theme}.png`) })
  assert.ok(!/Loop 2/.test(await transcript()), 'nothing about a second loop while the turn runs')
  release()
  await until(async () => /Loop 2, tick 1/.test(await transcript()), 'the queued loop started')
  await until(async () => (await transcript()).includes('Second loop ran.'), 'its first tick answered')
  await stop.waitFor({ state: 'hidden' })
  // The stop is asked once the second loop has started, so it reads two live and ends neither. A
  // stop is answered at once while a turn runs (LOOP-11), so it may land beside the tick's turn.
  await until(async () => (await page.locator('.narration.refused').count()) > 0, 'the stop was answered')
  const after = await transcript()
  const refusal = (await page.locator('.narration.refused').last().innerText()).trim()
  assert.equal(refusal, 'Two loops are running. Type /loop stop 1 or /loop stop 2 to end one, or /loop to list them.')
  const at = after.lastIndexOf(refusal)
  assert.ok(at > after.indexOf('Loop 2 repeats every 20m'), `the stop was answered after the second loop started (${refusal} in ${after})`)
  // It was typed before loop 2's first tick, so it sits above that tick's row, which stays directly above its reply.
  assert.ok(at < after.indexOf('Loop 2, tick 1'), `the refusal is above the tick row (${after})`)
  assert.ok(!/Loop \d ended|Loop \d stopped|loops stopped/.test(after), 'the stop ended no loop')
  await page.locator('[data-test="loop-status"]').waitFor()
  await page.screenshot({ path: join(output, `loop-command-stop-${theme}.png`) })
  assert.deepEqual(errors, [], 'the page reported no errors')
  console.log('drive-loop-command: ok')
} finally {
  release()
  await app?.close()
  service.close()
}
