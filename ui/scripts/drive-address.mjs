// Addressing a definition for one message from the composer's `@` list (ADDRESS-2, ADDRESS-3,
// ADDRESS-5): the definition row with its kind and description ahead of the files, Enter staging a
// `[@name]` marker, Shift-Enter keeping it, a line holding only the marker refused with nothing
// sent, a send running one turn under the definition with its reply labelled with the definition,
// and the next message going unaddressed and unlabelled.
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. What the model was sent is read off that service.
//
// Needs `bravebot-rpc` and `bravebot-ui-files` built (`pnpm run bridge`) and the app built
// (`electron-vite build`), which `pnpm run drive:address` does first. Screenshots of the open list
// in light and dark and of the refusal go to ADDRESS_OUTPUT, or /tmp/bravebot-ui.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { ASSISTANT_BUBBLE, COMPOSER_NAME, SHOTS_DIR } from './drive-support.mjs'

const PURPOSE = 'You are lantern, and you keep the lights on in this project.'

const { service, rounds } = await startModelService(async () => ({ content: 'done' }))
const { root, home, project, profile, env } = makeWorld('bravebot-address-', { service, dirs: ['home/.bravebot/agents'] })
writeFileSync(join(home, '.bravebot/agents/lantern.md'), `---\nname: lantern\ndescription: Keeps the lights.\nkind: reader\n---\n\n${PURPOSE}\n`)
writeFileSync(join(project, 'lantern-notes.md'), '# Notes\n')

const output = process.env.ADDRESS_OUTPUT || SHOTS_DIR
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
  await openProject(page, app, project)

  const field = page.getByRole('textbox', { name: COMPOSER_NAME })
  const list = page.locator('[data-test="mention-list"]')
  const refusal = page.locator('[data-test="send-refused"]')
  const value = () => field.inputValue()
  const valueIs = async (want) => {
    for (let tries = 0; tries < 100 && (await value()) !== want; tries++) await page.waitForTimeout(50)
    assert.equal(await value(), want)
  }
  const settled = async (count) => {
    await page.locator(ASSISTANT_BUBBLE).nth(count - 1).waitFor()
    await page.locator('.composer .stop').waitFor({ state: 'hidden' })
    assert.equal(rounds.length, count)
  }

  // ---- `@` and part of the name lists the definition, with its description, ahead of the files --
  await field.click()
  await page.keyboard.type('@lan')
  await list.locator('[data-test="mention-definition"]').waitFor()
  const options = (await list.getByRole('option').allInnerTexts()).map((text) => text.replace(/\s+/g, ' ').trim())
  assert.deepEqual(options, ['lantern Keeps the lights. Definition', 'lantern-notes.md'], 'the definition row comes first and says what it is')
  await page.screenshot({ path: join(output, 'address-list-light.png') })
  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'address-list-dark.png') })
  await page.evaluate(() => window.bravebot.writeTheme('system'))

  // ---- Enter stages the marker and sends nothing; Shift-Enter keeps it ---------------------------
  await page.keyboard.press('Enter')
  await valueIs('[@lantern] ')
  assert.equal(rounds.length, 0, 'Enter on the row picked it rather than sending')
  await page.keyboard.press('Shift+Enter')
  await valueIs('[@lantern] \n')
  assert.equal(rounds.length, 0)
  await page.keyboard.press('Backspace')
  await valueIs('[@lantern] ')

  // ---- a line holding only the marker says a task is needed and sends nothing --------------------
  await page.keyboard.press('Enter')
  await refusal.waitFor()
  assert.match(await refusal.innerText(), /lantern needs a task/)
  await page.screenshot({ path: join(output, 'address-needs-a-task-light.png') })
  // The line is put back as it was sent, trimmed, with the marker and the pick kept.
  await valueIs('[@lantern]')
  assert.equal(rounds.length, 0, 'the model was sent nothing')

  // ---- a task after the marker runs one turn under the definition --------------------------------
  await field.click()
  await page.keyboard.press('End')
  await page.keyboard.type(' say hello')
  await page.keyboard.press('Enter')
  await settled(1)
  const addressed = JSON.parse(rounds[0])
  assert.ok(JSON.stringify(addressed.messages).includes(PURPOSE), 'the turn ran under lantern')
  assert.ok(JSON.stringify(addressed.messages).includes('say hello'))
  assert.ok(!JSON.stringify(addressed.messages).includes('[@lantern]'), 'the marker is no part of the task')
  await valueIs('')

  // ---- the next message is the planner's -------------------------------------------------------
  await page.keyboard.type('and now plainly')
  await page.keyboard.press('Enter')
  await settled(2)
  assert.ok(!JSON.stringify(JSON.parse(rounds[1]).messages).includes(PURPOSE), 'the next turn is not addressed')

  // ---- the addressed reply is labelled lantern, and the planner's reply after it is not -----------
  const labels = page.locator('[data-test="address-label"]')
  assert.equal(await labels.count(), 1, 'one reply carries the definition label')
  assert.equal((await labels.innerText()).trim(), 'lantern')
  const labelledReply = page.locator('.entry-hit', { has: labels }).locator(ASSISTANT_BUBBLE)
  assert.equal(await labelledReply.count(), 1, 'the label sits over a reply')
  await page.screenshot({ path: join(output, 'address-label-light.png') })
  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'address-label-dark.png') })
  await page.evaluate(() => window.bravebot.writeTheme('system'))

  assert.deepEqual(errors, [])
  console.log(`PASS: @lan listed lantern as a definition with its description ahead of the file; Enter wrote [@lantern] and Shift-Enter kept it; the marker alone was refused and nothing was sent; a task after it ran one turn under lantern without the marker and its reply was labelled lantern; the next message was the planner's, unlabelled. Screenshots in ${output}/address-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'address-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
