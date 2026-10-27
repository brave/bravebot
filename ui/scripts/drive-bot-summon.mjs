// Summoning a bot for one turn (MEMORY-13): a bot picked from the `@` list answers one message in
// a project's chat and in another bot's conversation, under a label with its face and name, and the
// next message is answered as before. The summoned bot's own conversation shows a "Replied to"
// card for each, and pressing one opens the conversation it names.
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. Which definition a turn ran under is read off what the service was sent.
//
// Needs `bravebot-rpc` and `bravebot-ui-files` built (`npm run bridge`) and the app built
// (`electron-vite build`), which `npm run drive:bot-summon` does first. Screenshots go to
// BOT_SUMMON_OUTPUT, or /tmp/bravebot-ui.
import assert from 'node:assert/strict'
import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { ASSISTANT_BUBBLE, BOT_OPEN_BUTTON, COMPOSER_NAME, TRUST_APPROVE } from './drive-support.mjs'

// Every request lists each definition's purpose among the delegate kinds, so the sentence the
// agent adds to a turn addressed to a definition is what says which one the turn ran under. The
// service answers with that definition's name.
const LANTERN = 'SWEEP-THE-LANTERN-ROOM'
const HARBOUR = 'MIND-THE-HARBOUR-WALL'
const under = (body) =>
  body.includes('addressed this turn to lantern') ? 'lantern' : body.includes('addressed this turn to harbour') ? 'harbour' : 'plain'
const { service, rounds } = await startModelService(({ body }) => ({ content: under(body) }))
const ranUnder = (round) => under(rounds[round] ?? '')

const { root, project, profile, env } = makeWorld('bravebot-bot-summon-', { service })
const output = process.env.BOT_SUMMON_OUTPUT || '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
  const made = await page.evaluate(async ({ LANTERN, HARBOUR }) => [
    await window.bravebot.writeBot({ name: 'Lantern', purpose: `Answer every message. ${LANTERN}` }),
    await window.bravebot.writeBot({ name: 'Harbour', purpose: `Answer every message. ${HARBOUR}` }),
  ], { LANTERN, HARBOUR })
  assert.ok(made.every((bot) => bot?.definition), 'both bots were given definitions')
  await page.reload()
  await openProject(page, app, project)

  const field = page.getByRole('textbox', { name: COMPOSER_NAME })
  const list = page.locator('[data-test="mention-list"]')
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  const idle = () => page.locator('.composer .stop').waitFor({ state: 'hidden' })
  const roundsAre = async (count) => {
    for (let tries = 0; tries < 100 && rounds.length < count; tries++) await page.waitForTimeout(50)
    assert.equal(rounds.length, count)
  }
  const summon = async (typed, name) => {
    await field.click()
    await page.keyboard.type(typed)
    await list.locator('[data-test="mention-bot"]', { hasText: name }).waitFor()
    await page.keyboard.press('Enter')
    for (let tries = 0; tries < 100 && !(await field.inputValue()).includes(`[@${name}]`); tries++) await page.waitForTimeout(50)
    assert.ok((await field.inputValue()).includes(`[@${name}]`), 'the pick wrote the bot’s marker into the message')
    assert.equal(await page.locator('[data-test="summon-chip"], .attachment-chip.summoned').count(), 0, 'a summoned bot is not a chip above the message')
  }
  // A new conversation asks whether to trust its folder, and switching to the Bots tab opens the
  // first bot's conversation, so the question can be asked twice on the way.
  const answerTrust = async () => {
    await page.waitForTimeout(500)
    if (!(await trust.isVisible())) return
    await trust.locator(TRUST_APPROVE).click()
    await trust.waitFor({ state: 'hidden' })
  }
  const openBot = async (name) => {
    await page.locator('[data-test="sidebar-tabs"] [role="option"]').nth(1).click()
    await answerTrust()
    await page.locator('.bot').filter({ has: page.locator(`.bot-name:text-is("${name}")`) }).locator(BOT_OPEN_BUTTON).click()
    await answerTrust()
  }

  // ---- in a project's chat, the list offers the bots first and a pick answers that message ------
  await field.click()
  await page.keyboard.type('@')
  await list.locator('[data-test="mention-bot"]').first().waitFor()
  assert.deepEqual((await list.locator('[data-test="mention-bot"]').allInnerTexts()).map((text) => text.trim()), ['Harbour', 'Lantern'])
  await page.screenshot({ path: join(output, 'bot-summon-list.png') })
  await page.keyboard.press('Escape')
  await field.fill('')
  await summon('Ask @Lan', 'Lantern')
  assert.equal(await field.inputValue(), 'Ask [@Lantern] ', 'the half-typed name became the bot’s marker')
  await page.screenshot({ path: join(output, 'bot-summon-draft.png') })
  await page.keyboard.type('what changed?')
  await page.keyboard.press('Enter')
  await roundsAre(1)
  await idle()
  assert.equal(ranUnder(0), 'lantern', 'the summoned turn ran under the summoned bot’s definition')
  const mention = page.locator('.bubble.user [data-test="summon-mention"]')
  await mention.waitFor()
  assert.equal((await mention.innerText()).trim(), 'Lantern', 'the sent message shows the bot as a label where its marker was')
  await page.screenshot({ path: join(output, 'bot-summon-message.png') })
  assert.equal((await page.locator('.bubble.user .bubble-text').first().innerText()).replace(/\s+/g, ' '), 'Ask Lantern what changed?', 'the marker’s brackets are replaced by the label')
  const label = page.locator('[data-test="summon-label"]')
  await label.first().waitFor()
  assert.equal((await label.allInnerTexts()).map((text) => text.trim()).join(), 'Lantern', 'the reply is labelled with the summoned bot')

  await field.fill('And now a plain question')
  await field.press('Enter')
  await roundsAre(2)
  await idle()
  assert.equal(ranUnder(1), 'plain', 'the next turn was addressed to no bot')
  assert.equal(await label.count(), 1, 'only the summoned reply is labelled')
  await page.screenshot({ path: join(output, 'bot-summon-project.png') })

  // ---- in another bot's conversation, the summoned bot answers once and the host the rest -------
  await openBot('Harbour')
  await page.locator('[data-test="bot-greeting"]').waitFor()
  await page.screenshot({ path: join(output, 'bot-summon-greeting.png') })
  await field.click()
  await page.keyboard.type('@')
  await list.locator('[data-test="mention-bot"]').first().waitFor()
  assert.deepEqual((await list.locator('[data-test="mention-bot"]').allInnerTexts()).map((text) => text.trim()), ['Lantern'], 'a bot is not offered in its own conversation')
  await field.fill('')
  await summon('@La', 'Lantern')
  await page.keyboard.type('do you agree?')
  await page.keyboard.press('Enter')
  await roundsAre(3)
  await idle()
  assert.equal(ranUnder(2), 'lantern', 'the summoned turn in Harbour’s conversation ran as Lantern')
  await field.fill('Hello Harbour')
  await field.press('Enter')
  await roundsAre(4)
  await idle()
  assert.equal(ranUnder(3), 'harbour', 'the next turn ran as Harbour, the conversation’s own bot')
  assert.equal(await label.count(), 1)
  assert.equal((await page.locator(ASSISTANT_BUBBLE).allInnerTexts()).map((text) => text.trim()).join(), 'lantern,harbour')

  // ---- the summoned bot's own conversation shows a card for each reply, which opens it -----------
  await openBot('Lantern')
  const cards = page.locator('[data-test="replied-card"]')
  await cards.nth(1).waitFor()
  assert.equal(await cards.count(), 2)
  const where = (await cards.locator('.replied-where').allInnerTexts()).map((text) => text.trim())
  assert.deepEqual(where, [project.split('/').at(-1), 'Harbour'], 'one card names the project and one Harbour, oldest first')
  assert.deepEqual((await cards.locator('.replied-text').allInnerTexts()).map((text) => text.trim()), ['lantern', 'lantern'], 'each card holds the reply')
  await page.screenshot({ path: join(output, 'bot-summon-cards-light.png') })
  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'bot-summon-cards-dark.png') })
  await page.evaluate(() => window.bravebot.writeTheme('system'))
  await cards.filter({ has: page.locator('.replied-where', { hasText: 'Harbour' }) }).click()
  await page.locator('.entry-hit.focused .bubble.user', { hasText: 'do you agree?' }).waitFor()
  assert.equal((await label.allInnerTexts()).map((text) => text.trim()).join(), 'Lantern', 'the reply it opened carries its label')

  // ---- the earlier conversations are a page of their own, reached from the header ---------------
  await page.locator('[data-test="earlier-conversations"]').click()
  await page.getByRole('heading', { name: 'Earlier conversations' }).waitFor()
  assert.equal(await page.locator('[data-test="bot-page"] .composer').count(), 0, 'the page has no composer')
  await page.locator('[data-test="earlier-back"]').click()
  await page.locator('[data-test="bot-greeting"], .entries .bubble').first().waitFor()

  assert.deepEqual(errors, [])
  console.log(`PASS: the @ list offered bots before files and never a conversation's own; a summoned bot answered one turn in a project's chat and one in another bot's, under its label, and the next turn ran as before; the summoned bot's conversation showed a card for each reply, and one opened the reply. Screenshots in ${output}/bot-summon-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'bot-summon-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
