// Consulting bots from the window: the `@` list offering a bot, a line naming two bots consulting
// them one after another (CONSULT-9), and a consult the planner proposes drawn as a card that
// sends nothing until it is answered yes (CONSULT-12). A line naming a bot is drafted into a brief,
// which a host holding nothing untrusted or private sends at once (CONSULT-5).
//
// The real app and the real bridge, against a model service this script serves itself, so nothing
// is paid for. Which bot a request was for is read off the system prompt the service was sent.
//
// Needs `bravebot-rpc` and `bravebot-ui-files` built (`npm run bridge`) and the app built
// (`electron-vite build`), which `npm run drive:consult` does first.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

const BRIEF = 'zqk brief: is the retry loop bounded'

const { service, rounds } = await startModelService(async ({ body, messages }) => {
  // The reviewer narrates and reads before it replies, so what a consult streams is drawn as well as its reply.
  if (body.includes('REVIEW-BY-DEFINITION')) {
    return messages.some((message) => message.role === 'tool')
      ? { content: 'reviewer says blue' }
      : { content: 'zqk reviewer looks first', tool: { name: 'read_file', arguments: { path: 'release.txt' } } }
  }
  if (body.includes('TEST-BY-DEFINITION')) return { content: 'tester says green' }
  // The host drafting a brief for the bot a line named (CONSULT-5).
  const drafting = /I have asked the agent ([a-z-]+) about my next line/.exec(body)
  if (drafting) return { content: `zqk drafted for ${drafting[1]}` }
  const asked = messages.findLast((message) => message.role === 'user')
  const proposing = JSON.stringify(asked?.content ?? '').includes('propose one')
  if (proposing && !messages.some((message) => message.role === 'tool')) {
    return { tool: { name: 'propose_consult', arguments: { bot: 'reviewer', brief: BRIEF } } }
  }
  return { content: 'host done' }
})

const { root, home, project, profile, env } = makeWorld('bravebot-consult-', { service })
mkdirSync(join(home, '.bravebot/agents'), { recursive: true })
const bot = (name, body) =>
  writeFileSync(join(home, `.bravebot/agents/${name}.md`), `---\nname: ${name}\ndescription: ${name} bot\nkind: reader\n---\n\n${body}\n`)
bot('reviewer', 'REVIEW-BY-DEFINITION')
bot('tester', 'TEST-BY-DEFINITION')
writeFileSync(join(project, 'release.txt'), 'the release is blue\n')

const output = process.env.CONSULT_OUTPUT || '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 900 } }))
  await openProject(page, app, project)

  const field = page.getByRole('textbox', { name: 'Message the agent' })
  const list = page.locator('[data-test="mention-list"]')
  const shows = (text) => page.waitForFunction((want) => document.body.innerText.includes(want), text, { timeout: 30000 })
  const forBot = (marker) => rounds.findIndex((body) => body.includes(marker))

  // ---- the @ list offers a bot -------------------------------------------------------------
  await field.click()
  await page.keyboard.type('ask @rev')
  await list.waitFor()
  const offered = (await list.getByRole('option').allInnerTexts()).map((text) => text.trim())
  assert.ok(offered.some((text) => text.includes('reviewer')), `the list offers the bot: ${offered}`)
  await page.keyboard.press('Escape')
  await field.fill('')

  // ---- a line naming two bots consults them in its order, one after the other ---------------
  // Each bot's brief is drafted from the conversation and, holding nothing untrusted or private,
  // is sent at once: one Enter on the line is all the person presses.
  await field.fill('ask @reviewer and @tester what colour the release is')
  await page.keyboard.press('Enter')
  await shows('reviewer says blue')
  await shows('zqk drafted for reviewer')
  await shows('tester says green')
  await shows('zqk drafted for tester')
  assert.equal(await page.locator('[data-test="consult-brief"]').count(), 0, 'no brief is left waiting in the box')
  assert.equal(await field.inputValue(), '', 'the box is empty once the briefs have gone')
  assert.ok(forBot('REVIEW-BY-DEFINITION') >= 0 && forBot('TEST-BY-DEFINITION') > forBot('REVIEW-BY-DEFINITION'), 'the reviewer is consulted first, the tester next')
  // Everything the reviewer streamed, its narration, its read and its reply, sits under one header
  // naming it, and the tester has its own (CONSULT-11).
  const groups = await page.locator('.consult-group').evaluateAll((all) => all.map((group) => ({
    bot: group.dataset.bot,
    headers: group.querySelectorAll('.consulted-by').length,
    text: group.innerText,
  })))
  assert.deepEqual(groups.map((group) => group.bot), ['reviewer', 'tester'], `one group for each bot: ${JSON.stringify(groups)}`)
  for (const group of groups) assert.equal(group.headers, 1, `one header in the ${group.bot} group`)
  const [reviewer, tester] = groups
  for (const want of ['zqk reviewer looks first', 'release.txt', 'reviewer says blue']) {
    assert.ok(reviewer.text.includes(want), `the reviewer's group holds ${want}: ${reviewer.text}`)
  }
  assert.ok(!tester.text.includes('reviewer'), `the tester's group holds none of the reviewer's: ${tester.text}`)
  assert.ok(tester.text.includes('tester says green'), `the tester's group holds its reply: ${tester.text}`)
  const outside = await page.evaluate(() => [...document.querySelectorAll('.entries > *:not(.consult-group)')].map((node) => node.textContent).join('\n'))
  assert.ok(!outside.includes('zqk reviewer looks first'), 'the reviewer narration is not drawn as the host')
  await page.screenshot({ path: join(output, 'consult-several.png') })

  // ---- a consult the planner proposes is a card, and is sent on its yes ---------------------
  const before = rounds.length
  await field.fill('please propose one to the reviewer')
  await page.keyboard.press('Enter')
  const card = page.locator('.confirm.consult')
  await card.waitFor({ timeout: 30000 })
  const drawn = await card.innerText()
  for (const want of ['reviewer', BRIEF, 'Nothing is sent unless you say yes']) assert.ok(drawn.includes(want), `the card shows ${want}: ${drawn}`)
  assert.ok(!rounds.slice(before).some((body) => body.includes('REVIEW-BY-DEFINITION')), 'nothing reaches the bot before the yes')
  await page.screenshot({ path: join(output, 'consult-card.png') })
  await card.getByRole('button', { name: 'Yes, send it', exact: true }).click()
  await shows('You sent this consult')
  for (let tries = 0; tries < 300 && !rounds.slice(before).some((body) => body.includes('REVIEW-BY-DEFINITION')); tries++) await page.waitForTimeout(100)
  assert.ok(rounds.slice(before).some((body) => body.includes('REVIEW-BY-DEFINITION')), 'the yes sends the brief to the bot')
  await shows('host done')

  assert.deepEqual(errors, [], 'no page errors')
  console.log('drive-consult: ok')
} finally {
  await app?.close().catch(() => undefined)
  service.close()
  rmSync(root, { recursive: true, force: true })
}
