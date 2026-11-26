// Pictures dropped and pasted onto a `/btw` line in the desktop composer (docs/specs/commands.md,
// CMD-5, CMD-8; dropping.md DROP-4; pasting.md PASTE-2).
//
// The real app and the real bridge, against a model service this script serves itself. A /btw at
// rest with a dropped picture and a pasted one sends both in the question's own message, and the
// next turn's request holds neither. A /btw with a dropped picture while a turn is held is refused
// in the terminal's words, and its line goes back in the composer with the picture still staged.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`).
// Pass `--theme dark` or `--theme light` to take the screenshots in that theme.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { COMPOSER_NAME, SHOTS_DIR, copyImage, dragFiles, heldTurns, menuPaste, picture, untilIn } from './drive-support.mjs'

const theme = process.argv.includes('--theme') ? process.argv[process.argv.indexOf('--theme') + 1] : 'light'
const BTW = 'The first is orange and the second is blue.'

const { hold, release, waiting } = heldTurns()

const { service, rounds: requests } = await startModelService(async ({ messages }) => {
  const last = JSON.stringify(messages.at(-1) ?? '')
  if (last.includes('what colour are')) return { content: BTW }
  if (last.includes('Hold on')) { await hold(); return { content: 'Done holding.' } }
  return { content: 'I looked around the project.' }
})

const { root, project, profile, env } = makeWorld('bravebot-aside-staged-', { service, dirs: ['desktop'] })
const desktop = join(root, 'desktop')
writeFileSync(join(project, 'README.md'), '# scratch\n')
const output = process.env.BRAVEBOT_DRIVE_OUTPUT ?? SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1200, height: 800 } }))
  await app.evaluate(async ({ clipboard }) => { globalThis.keptClipboard = await clipboard.read().catch(() => []) })
  await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' })

  const dropped = join(desktop, 'shot.png')
  const orange = await picture(app, 48, [0xd0, 0x60, 0x20, 0xff])
  writeFileSync(dropped, Buffer.from(orange, 'base64'))
  const blue = await picture(app, 48, [0x20, 0x60, 0xd0, 0xff])

  await openProject(page, app, project)
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const stop = page.locator('.composer .stop')
  const transcript = () => page.locator('.entries').innerText()
  const until = untilIn(page)

  await composer.fill('Look around')
  await composer.press('Enter')
  await until(async () => (await transcript()).includes('I looked around the project.'), 'the first turn answered')
  await stop.waitFor({ state: 'hidden' })

  // ---- /btw at rest, with a dropped picture and a pasted one -----------------------------------
  await composer.fill('/btw what colour are ')
  await composer.press('End')
  await dragFiles(page, [dropped])
  await page.locator('[data-test="dropped-chip"]').first().waitFor()
  await composer.press('End')
  await composer.pressSequentially('and ')
  await copyImage(app, blue)
  await composer.focus()
  await menuPaste(app)
  await page.locator('[data-test="pasted-chip"]').first().waitFor()
  assert.equal(await composer.inputValue(), '/btw what colour are [Image #1] and [Image #2] ')
  await page.screenshot({ path: join(output, `aside-staged-composer-${theme}.png`) })
  const before = requests.length
  await composer.press('Enter')
  await until(async () => (await transcript()).includes(BTW), 'the /btw answer is drawn')
  const asked = JSON.parse(requests[before])
  const last = JSON.stringify(asked.messages.at(-1))
  assert.ok(last.includes('what colour are [Image #1] and [Image #2]'), 'the question went as typed')
  assert.equal((last.match(/data:image\/png;base64,/g) ?? []).length, 2, 'both pictures went in the question\'s message')
  assert.ok(last.includes('shot.png'), 'the dropped picture is named by its path')
  assert.equal(await page.locator('[data-test="dropped-chip"], [data-test="pasted-chip"]').count(), 0, 'what was sent left the composer')
  await page.screenshot({ path: join(output, `aside-staged-answered-${theme}.png`) })

  await composer.fill('Next step')
  await composer.press('Enter')
  await until(async () => requests.some((body) => body.includes('Next step')), 'the next turn reached the model')
  await stop.waitFor({ state: 'hidden' })
  const next = requests.findLast((body) => body.includes('Next step'))
  for (const held of ['data:image/png', 'what colour are', BTW]) assert.ok(!next.includes(held), `the next turn's request does not hold ${held}`)

  // ---- /btw with a dropped picture beside a held turn ------------------------------------------
  await composer.fill('Hold on')
  await composer.press('Enter')
  await stop.waitFor({ state: 'visible' })
  await until(async () => waiting.length > 0, 'the held turn reached the model')
  const during = requests.length
  await composer.fill('/btw what colour is ')
  await composer.press('End')
  await dragFiles(page, [dropped])
  await page.locator('[data-test="dropped-chip"]').first().waitFor()
  await composer.press('Enter')
  // An error note folds its sentence under its details, so it is read by textContent and unfolded
  // for the screenshot.
  await until(async () => (await page.locator('.entries').textContent()).includes('does not read dropped files while a turn runs'), 'the refusal is drawn')
  await page.locator('[data-test="error-details"]').last().evaluate((collapse) => { collapse.isOpen = true })
  await page.waitForTimeout(300)
  assert.equal(requests.length, during, 'the refused question reached nothing')
  assert.match(await composer.inputValue(), /^\/btw what colour is \[Image #\d+\]/, 'the line went back in the composer')
  assert.equal(await page.locator('[data-test="dropped-chip"]').count(), 1, 'the picture is still staged')
  await page.screenshot({ path: join(output, `aside-staged-refused-${theme}.png`) })
  release()
  await stop.waitFor({ state: 'hidden' })

  assert.deepEqual(errors, [])
  console.log(`PASS: a /btw carries what was staged on its line at rest, and a dropped file beside a turn is refused. Screenshots in ${output}/aside-staged-*-${theme}.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'aside-staged-failure.png') }).catch(() => undefined)
  throw error
} finally {
  release()
  await app?.evaluate(async ({ clipboard }) => {
    if (globalThis.keptClipboard?.length) await clipboard.write(globalThis.keptClipboard).catch(() => clipboard.writeText(''))
    else clipboard.writeText('')
  }).catch(() => undefined)
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
}
