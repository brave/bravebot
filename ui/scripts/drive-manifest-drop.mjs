// A picture dropped onto a typed `/manifest` line goes to the planner with the task (DROP-4,
// DROP-11), both when the line is sent at once and when it waits behind a running turn.
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. The drop is a trusted drag driven through Chromium's input pipeline, as in
// drive-drop.mjs.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync, readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR } from './drive-support.mjs'

// A run asks the planner twice: once for the plan in words, then for its steps. An ordinary turn
// answers slowly, so a `/manifest` line can be typed while it runs.
let planning = 0
const { service, rounds: requests } = await startModelService(async ({ body }) => {
  if (body.includes('Hold the turn')) {
    await new Promise((resolve) => setTimeout(resolve, 3000))
    return { content: 'done' }
  }
  planning += 1
  const target = planning <= 2 ? 'first.md' : 'second.md'
  return planning % 2 === 1
    ? { content: `1. Write ${target} with a fixed body.` }
    : { content: JSON.stringify({ steps: [{ capability: 'FILE_WRITE', args: { path: target, contents: '# Notes from the plan\n' } }] }) }
})

// The desktop is outside the project, which is where a drop usually comes from (DROP-3).
const { root, project, profile, env } = makeWorld('bravebot-manifest-drop-', { service, dirs: ['desktop'] })
const desktop = join(root, 'desktop')
writeFileSync(join(project, 'README.md'), '# scratch\n')

const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))

  const pictureBytes = await app.evaluate(({ nativeImage }) => {
    const size = 48, pixels = Buffer.alloc(size * size * 4)
    for (let i = 0; i < size * size; i++) pixels.set([0x20, 0x80, 0xd0, 0xff], i * 4)
    return nativeImage.createFromBitmap(pixels, { width: size, height: size }).toPNG().toString('base64')
  })
  const png = join(desktop, 'sketch.png')
  writeFileSync(png, Buffer.from(pictureBytes, 'base64'))

  await openProject(page, app, project)

  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const chips = page.locator('[data-test="dropped-chip"]')
  const stop = page.locator('.composer .stop')
  const card = page.locator('.confirm.manifest')
  const cdp = await page.context().newCDPSession(page)
  const drag = async (files) => {
    const box = await page.locator('.composer textarea').first().boundingBox()
    const x = Math.round(box.x + box.width / 2), y = Math.round(box.y + box.height / 2)
    const data = { items: [], files, dragOperationsMask: 1 }
    await cdp.send('Input.dispatchDragEvent', { type: 'dragEnter', x, y, data })
    for (let i = 0; i < 3; i++) {
      await cdp.send('Input.dispatchDragEvent', { type: 'dragOver', x, y, data })
      await page.waitForTimeout(100)
    }
    await cdp.send('Input.dispatchDragEvent', { type: 'drop', x, y, data })
  }
  // A shot is taken once what appeared has finished moving in (the card's rise, the composer
  // docking), with the pointer off every control, so nothing is caught mid-move or hovered.
  const settled = async () => {
    await page.mouse.move(1380, 700)
    await page.waitForFunction(() => document.getAnimations().every((animation) =>
      animation.playState !== 'running' || animation.effect?.getComputedTiming().iterations === Infinity))
    await page.waitForTimeout(300)
  }
  const shoot = async (name) => {
    await page.emulateMedia({ colorScheme: 'light' })
    await settled()
    await page.screenshot({ path: join(output, `${name}-light.png`) })
    await page.emulateMedia({ colorScheme: 'dark' })
    await settled()
    await page.screenshot({ path: join(output, `${name}-dark.png`) })
    await page.emulateMedia({ colorScheme: 'light' })
  }
  const planned = (task) => requests.find((body) => body.includes(task) && !body.includes('Hold the turn'))
  const carriesPicture = (body) => body.includes(`Dropped file: ${png}`) && body.includes('data:image/png;base64,')

  // ---- a /manifest line with a dropped picture, sent at once ------------------------------------
  await composer.fill('/manifest build the first notes from ')
  await drag([png])
  await chips.first().waitFor()
  assert.equal(await composer.inputValue(), '/manifest build the first notes from [Image #1] ')
  await settled()
  await page.screenshot({ path: join(output, 'manifest-drop-staged.png') })
  await composer.press('Enter')
  await card.first().waitFor({ timeout: 60000 })
  assert.equal(await chips.count(), 0, 'starting the run clears what was staged')
  const first = planned('build the first notes from [Image #1]')
  assert.ok(first, 'the task reached the planner')
  assert.ok(carriesPicture(first), 'with the dropped picture, named by its path before its bytes (DROP-11)')
  const shown = card.first().locator('[data-test="sent-dropped-chip"]')
  assert.equal(await shown.count(), 1, 'the plan card shows the picture dropped onto the task')
  assert.match(await shown.innerText(), /sketch\.png[\s\S]*\[Image #1\]/)
  await page.locator('.working').waitFor({ state: 'detached' })
  await shoot('manifest-drop-plan')
  const approve = card.first().getByRole('button', { name: 'Run this plan' })
  assert.equal(await approve.isDisabled(), false, 'the plan can be approved')
  await approve.click()
  // Once the plan is approved the run is past planning. It may already be over.
  const word = await page.locator('.working .working-word').innerText({ timeout: 500 }).catch(() => null)
  assert.ok(word === null || !word.startsWith('Planning'), `the approved run says it is planning: ${word}`)
  await stop.waitFor({ state: 'hidden', timeout: 60000 })
  await page.waitForFunction(() => document.querySelector('.composer .stop') === null, null, { timeout: 60000 })
  const written = join(project, 'first.md')
  const until = Date.now() + 10000
  while (!existsSync(written) && Date.now() < until) await page.waitForTimeout(100)
  assert.ok(existsSync(written), 'the approved plan did not write first.md')
  assert.equal(readFileSync(join(project, 'first.md'), 'utf8'), '# Notes from the plan\n', 'the approved plan ran')

  // ---- the same line typed while a turn runs waits, and goes with its picture ---------------------
  await composer.fill('Hold the turn')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await composer.fill('/manifest build the second notes from ')
  await drag([png])
  await chips.first().waitFor()
  await composer.press('Enter')
  await page.locator('.queued-messages').waitFor()
  assert.equal(await chips.count(), 0, 'queuing takes the drop with the line')
  assert.equal(planned('build the second notes from'), undefined, 'the run waits for the turn')
  const queuedChip = page.locator('.queued-messages [data-test="sent-dropped-chip"]')
  assert.match(await queuedChip.innerText(), /^sketch\.png\s*\[Image #\d+\]$/, 'the queued line shows the picture that waits with it')
  const textBox = await page.locator('.queued-messages .tray-text').boundingBox()
  const chipBox = await queuedChip.boundingBox()
  assert.ok(chipBox.x - (textBox.x + textBox.width) < 12, 'the chip follows the queued text, not the remove button')
  // Folded, the approved plan names the file as the record title does.
  assert.match(await card.first().innerText(), /build the first notes from sketch\.png/)
  assert.doesNotMatch(await card.first().innerText(), /\[Image #1\]/)
  // The first run's record is titled with the file's name, not its marker.
  await page.locator('.session', { hasText: 'build the first notes from sketch.png' }).waitFor()
  await shoot('manifest-drop-queued')
  await page.waitForFunction(() => document.querySelector('.queued-messages') === null, null, { timeout: 60000 })
  await card.nth(1).waitFor({ timeout: 60000 })
  const second = planned('build the second notes from [Image #')
  assert.ok(second, 'the queued task reached the planner when the turn ended')
  assert.ok(carriesPicture(second), 'with the picture it waited with (DROP-4)')
  assert.ok(!requests.some((body) => body.includes('Hold the turn') && body.includes('/manifest')), 'the line was never sent as a prompt')

  assert.deepEqual(errors, [])
  console.log(`PASS: a picture dropped onto a /manifest line reaches the planner as "Dropped file:" and its bytes, the plan card is shown and runs, and a line typed during a turn waits and goes with its picture. Screenshots in ${output}/manifest-drop-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'manifest-drop-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
