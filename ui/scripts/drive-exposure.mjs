// That a read which would expose a credential is put to the window, that the answer given there
// decides whether the model gets the file, and that the value is never drawn.
//
// The real app and the real bridge, against a model service this script serves itself. Nothing
// is paid for. The key is AWS's documented example, which is no credential.
//
// Needs `bravebot-rpc` built (`pnpm run bridge`) and the app built (`electron-vite build`), which
// `pnpm run drive:exposure` does first.
import assert from 'node:assert/strict'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'

const KEY = 'AKIAIOSFODNN7EXAMPLE'

// The model service. A planner that reads `.env` whenever the last thing said was not a tool's
// result, and ends the turn when it was.
const { service, rounds } = await startModelService(({ messages }) =>
  messages.at(-1)?.role === 'tool' ? { content: 'done' } : { tool: { name: 'read_file', arguments: { path: '.env' } } })

/** What the tools handed back in a request the planner was sent. */
const toolResults = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').map((message) => message.content).join('\n')

const { root, project, profile, env } = makeWorld('bravebot-exposure-', { service })
writeFileSync(join(project, '.env'), `AWS_ACCESS_KEY_ID=${KEY}\n`)

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
let app, page, errors
try {
  ;({ app, page, errors } = await launchWorld({ profile, env, viewport: { width: 1400, height: 1000 } }))
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  const trustIt = async () => {
    await trust.locator('[data-test="trust-approve"]').click()
    await trust.waitFor({ state: 'hidden' })
  }
  await openProject(page, app, project)

  const composer = page.locator('.composer textarea')
  const stop = page.locator('.composer .stop')
  const cards = page.locator('.confirm.exposure')
  const ask = async (prompt) => {
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    await stop.waitFor({ state: 'visible' })
  }
  const ended = () => stop.waitFor({ state: 'hidden' })
  /** Whether the value is anywhere in the window, in the text or in the markup. */
  const drawn = async () => (await page.content()).includes(KEY)

  // ---- the question, and a yes ---------------------------------------------------------------
  await ask('What is in .env?')
  const card = cards.first()
  await card.waitFor()
  assert.equal((await card.locator('.confirm-head .path').innerText()).trim(), '.env')
  const findings = (await card.locator('.exposure-findings li').allInnerTexts()).map((text) => text.trim())
  assert.equal(findings.length, 1)
  assert.match(findings[0], /an AWS access key id/)
  assert.match(findings[0], /\.env:1/)
  assert.match(await card.locator('.warn').innerText(), /Sending the file discloses that value/)
  assert.deepEqual(await card.locator('.confirm-actions button').allInnerTexts(), ['Keep it back', 'Send it anyway'])
  assert.match(await page.locator('.pending-jump').innerText(), /Approval needed/)
  assert.equal(await page.locator('.pending-jump').getAttribute('data-tooltip'), 'Answer the file with a credential')
  assert.equal(rounds.length, 1, 'no model was asked again before the answer')
  assert.ok(!rounds[0].includes(KEY), 'the value did not reach the planner before the answer')
  assert.ok(!(await drawn()), 'the value is not drawn on the question')
  await page.screenshot({ path: join(output, 'exposure-asked.png') })

  await card.getByRole('button', { name: 'Send it anyway', exact: true }).click()
  await card.locator('.decided.approve').waitFor()
  assert.match(await card.locator('.decided').innerText(), /You sent this file to the model/)
  await ended()
  assert.ok(toolResults(rounds.at(-1)).includes(KEY), 'a yes handed the planner the file')
  assert.ok(!(await drawn()), 'and the value is still not drawn in the window')
  await page.screenshot({ path: join(output, 'exposure-sent.png') })

  // ---- the next message, which asks nobody ---------------------------------------------------
  await ask('And once more?')
  await ended()
  assert.equal(await cards.count(), 1, 'the same file was not asked about again in this conversation')
  assert.ok(toolResults(rounds.at(-1)).includes(KEY))

  // ---- a new conversation, which asks again, and a no ------------------------------------------
  await page.locator('[data-test="new-session"]').click()
  // The new conversation is on screen once the first one's rows are gone.
  await page.locator('.bubble').first().waitFor({ state: 'detached' })
  if (await trust.isVisible().catch(() => false)) await trustIt()
  assert.equal(await cards.count(), 0)
  const before = rounds.length
  await ask('What is in .env?')
  const second = cards.first()
  await second.waitFor()
  assert.equal(await second.locator('.confirm-actions button').count(), 2, 'a new conversation asked for itself')
  await second.getByRole('button', { name: 'Keep it back', exact: true }).click()
  await second.locator('.decided.reject').waitFor()
  assert.match(await second.locator('.decided').innerText(), /You kept this file back/)
  await ended()
  const after = rounds.slice(before)
  assert.ok(after.every((round) => !round.includes(KEY)), 'a no kept the value from the planner')
  assert.match(toolResults(after.at(-1)), /refused/, 'and the planner was told the read did not happen')
  assert.ok(!toolResults(after.at(-1)).includes('AWS access key'), 'and was not told what the scan found')
  assert.ok(!(await drawn()))
  await page.screenshot({ path: join(output, 'exposure-kept.png') })

  assert.deepEqual(errors, [])
  console.log(`PASS: a read that would expose a credential is put to the window with what was found, a yes hands the model the file for the conversation, a no keeps it back, a new conversation asks again, and the value is never drawn. Screenshots in ${output}/exposure-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'exposure-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app?.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
