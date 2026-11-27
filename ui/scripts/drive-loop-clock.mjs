// A loop's ticks are sent by the main process's clock (docs/specs/loop.md, LOOP-4, LOOP-5, LOOP-6).
//
// The real app and the real bridge, against a model service this script serves itself. A loop is
// started through the bridge, and its next tick reaches the model with nothing done in the window.
// While the window says the session has a queue, no tick goes, and one goes once it says the queue
// is empty. The window cannot send a tick itself: `loops.poll` is refused.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`).
import assert from 'node:assert/strict'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

const LINE = 'ping the clock'
const { service, rounds } = await startModelService(async () => ({ content: 'Pinged.' }))
const ticks = () => rounds.filter((body) => body.includes(LINE) && !body.includes('You are a prompt-injection classifier.')).length

const { project, profile, env } = makeWorld('bravebot-loop-clock-', { service })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await openProject(page, app, project)
  const wait = async (until, what, tries = 400) => {
    for (let tried = 0; tried < tries; tried++) {
      if (await until()) return
      await page.waitForTimeout(50)
    }
    assert.fail(`timed out waiting for ${what}`)
  }

  const session = await page.evaluate(async (directory) => {
    const made = await window.bravebot.request('session.new', { directory })
    await window.bravebot.request('trust.reply', { session: made.ok.session, trusted: true })
    return made.ok.session
  }, project)

  // ---- the window cannot send a tick ------------------------------------------------------------
  const refused = await page.evaluate(() => window.bravebot.request('loops.poll', {}))
  assert.equal(refused.error?.code, 'bad_request', `loops.poll is refused to the window (${JSON.stringify(refused)})`)

  // ---- a loop started through the bridge ticks with nothing done in the window -------------------
  const started = await page.evaluate(([session, argument]) => window.bravebot.request('loop.command', { session, argument }), [session, `5s ${LINE}`])
  assert.equal(started.ok?.number, 1, `the loop started (${JSON.stringify(started)})`)
  await wait(() => ticks() >= 1, 'the first tick')
  await wait(() => ticks() >= 2, 'a tick sent by the clock')
  const listed = await page.evaluate((session) => window.bravebot.request('loops.list', { session }), session)
  assert.equal(listed.ok?.loops?.[0]?.number, 1, `the loop is listed (${JSON.stringify(listed)})`)

  // ---- a session the window says has a queue gets no tick until the queue is empty ---------------
  await page.evaluate((session) => window.bravebot.publishQueued([session]), session)
  const held = ticks()
  await page.waitForTimeout(8000)
  assert.equal(ticks(), held, 'no tick while the window holds a queue for the session')
  await page.evaluate(() => window.bravebot.publishQueued([]))
  await wait(() => ticks() > held, 'the tick held for the queue')

  await page.evaluate((session) => window.bravebot.request('session.close', { session }), session)
  assert.deepEqual(errors, [], 'the page reported no errors')
  console.log(`drive-loop-clock: ok (${ticks()} ticks sent by the clock)`)
} finally {
  await app?.close()
  service.close()
}
