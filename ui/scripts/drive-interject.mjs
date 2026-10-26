// Enter while a turn runs sends the message into that turn (docs/specs/terminal-input.md, INPUT-46).
//
// The real app and the real bridge, against a model service this script serves itself. The first
// answer of every turn is held until the script lets it go, so what the window shows while a turn
// is mid-round, and what the next request carries, are observed and not raced.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME } from './drive-support.mjs'

let waiting = []
const t0 = Date.now()
const note = (what) => process.env.DRIVE_DEBUG && console.error(`${((Date.now() - t0) / 1000).toFixed(1)}s ${what}`)
const hold = () => { note('model asked, answer held'); return new Promise((resolve) => waiting.push(resolve)) }
const release = () => { if (waiting.length) note(`letting go of ${waiting.length}`); waiting.splice(0).forEach((resolve) => resolve()) }
// Lets go of the answer the model service is holding, once it is holding one: the request takes a
// moment to arrive after the window sends.
const letGo = async () => {
  for (let tries = 0; tries < 200 && waiting.length === 0; tries++) await new Promise((resolve) => setTimeout(resolve, 50))
  assert.ok(waiting.length > 0, 'the model was never asked')
  release()
}

// A turn's first answer asks for a listing and is held; the answer to that listing, and an answer
// to a message that says "quick", is plain text. The held answers are what leave a turn mid-round.
const { service, rounds: requests } = await startModelService(async ({ messages }) => {
  const last = messages.at(-1)
  // The answer to a listing, and to a message the person put in after the listing came back.
  if (last?.role === 'tool' || (last?.role === 'user' && messages.at(-2)?.role === 'tool')) return { content: 'done' }
  await hold()
  return JSON.stringify(last).includes('quick') ? { content: 'quick answer' } : { tool: { name: 'list_files', arguments: { directory: '.' } } }
})

const { project, profile, env } = makeWorld('bravebot-interject-', { service })
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const sent = page.locator('[data-test="interjected-messages"]')
  const queued = page.locator('.queued-messages')
  const transcript = () => page.locator('.entries').innerText()
  const settle = async () => {
    for (let tries = 0; tries < 100 && !(await stop.isHidden()); tries++) { release(); await page.waitForTimeout(100) }
    await stop.waitFor({ state: 'hidden' })
  }
  const mentioning = (text) => requests.filter((body) => body.includes(text))
  // The requests that begin a turn on `text`: its words are the last thing the request carries, which
  // a later round of the same turn, holding them in its history, is not.
  const beginning = (text) => requests.filter((body) => {
    const last = JSON.parse(body).messages?.at(-1)
    return last?.role === 'user' && JSON.stringify(last).includes(text)
  })
  // A request takes a moment to reach the model service after the window starts a turn.
  const reached = async (text) => {
    for (let tries = 0; tries < 200 && beginning(text).length === 0; tries++) await page.waitForTimeout(50)
    return beginning(text).length
  }

  // ---- Enter sends the message into the turn, which reads it at its next round -----------------
  await composer.fill('Look around')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('no, the other one')
  await composer.press('Enter')
  await sent.waitFor()
  assert.match(await sent.innerText(), /Sent to this turn/)
  assert.match(await sent.innerText(), /no, the other one/)
  assert.equal(await queued.count(), 0, 'it is not in the queue')
  assert.doesNotMatch(await transcript(), /no, the other one/, 'it is in the transcript only once the planner has it')
  assert.equal(await composer.inputValue(), '', 'it left the box')
  console.log('--- while the first round runs ---\n' + await sent.innerText())
  await page.screenshot({ path: join(output, 'interject-sent.png') })
  await letGo()
  await sent.waitFor({ state: 'hidden' })
  await settle()
  assert.match(await transcript(), /no, the other one/, 'the planner read it and it is in the transcript')
  assert.equal(mentioning('no, the other one').length, 1, 'exactly one request carried it')
  assert.equal(requests.length, 2, 'it joined the turn and did not start another')
  assert.ok(requests[1].includes('no, the other one') && !requests[0].includes('no, the other one'), 'the second round carried it and the first could not have')
  await page.screenshot({ path: join(output, 'interject-read.png') })

  // ---- Alt+Enter holds the message until the turn ends, as a turn of its own ------------------
  const before = requests.length
  await composer.fill('Again')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('for later')
  await composer.press('Alt+Enter')
  await queued.waitFor()
  assert.equal(await sent.count(), 0, 'it was queued and not sent into the turn')
  console.log('--- queued for after ---\n' + await queued.innerText())
  await letGo()
  await queued.waitFor({ state: 'hidden' })
  await stop.waitFor({ state: 'visible' })
  assert.equal(await reached('for later'), 1, 'it went as a turn of its own, once')
  assert.ok(requests.slice(before).findIndex((body) => body.includes('for later')) >= 2, 'and after the first turn answered')
  await settle()

  // ---- a message the turn does not read goes as a turn of its own -------------------------------
  const start = requests.length
  await composer.fill('quick question')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('and one more thing')
  await composer.press('Enter')
  await sent.waitFor()
  await letGo()
  await sent.waitFor({ state: 'hidden' })
  await stop.waitFor({ state: 'visible' })
  assert.equal(await reached('and one more thing'), 1, 'the turn ended without another round, so it went as a turn of its own')
  await settle()
  assert.equal((await transcript()).split('and one more thing').length - 1, 1, 'and it is in the transcript once')
  assert.ok(requests.length >= start + 2)

  // ---- Stop does not hold back what was sent into the turn --------------------------------------
  const stopped = requests.length
  note('stop scenario begins')
  await composer.fill('Stop me')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('after the stop')
  await composer.press('Enter')
  await sent.waitFor()
  note('clicking stop')
  await stop.click()
  await letGo()
  await page.getByText('Task stopped', { exact: true }).waitFor()
  await stop.waitFor({ state: 'visible' })
  assert.equal(await reached('after the stop'), 1, 'a message sent into a turn that was stopped goes as a turn of its own, past the paused queue')
  await settle()
  assert.ok(requests.length > stopped)
  console.log('--- after stop ---\n' + (await transcript()).split('\n').slice(-6).join('\n'))

  assert.deepEqual(errors, [])
  console.log(`PASS: Enter sends into the running turn and the planner reads it at the next round, Alt+Enter queues for after, a message the turn did not read goes as a turn of its own, and Stop does not hold it back. Screenshots in ${output}/interject-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'interject-failure.png') }).catch(() => undefined)
  console.error(`the model was asked ${requests.length} times; the last carried: ${JSON.stringify(JSON.parse(requests.at(-1) ?? '{}').messages?.slice(-3))}`)
  throw error
} finally {
  release()
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
}
