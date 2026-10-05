// That a read which would expose a credential is put to the window, that the answer given there
// decides whether the model gets the file, and that the value is never drawn.
//
// The real app and the real bridge, against a model service this script serves itself. Nothing
// is paid for. The key is AWS's documented example, which is no credential.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`), which
// `npm run drive:exposure` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const KEY = 'AKIAIOSFODNN7EXAMPLE'

// The model service. A planner that reads `.env` whenever the last thing said was not a tool's
// result, and ends the turn when it was.
const rounds = []
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  rounds.push(body)
  const answered = (JSON.parse(body).messages ?? []).at(-1)?.role === 'tool'
  const delta = answered
    ? { role: 'assistant', content: 'done' }
    : { role: 'assistant', tool_calls: [{ index: 0, id: `call-${rounds.length}`, type: 'function', function: { name: 'read_file', arguments: JSON.stringify({ path: '.env' }) } }] }
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: answered ? 'stop' : 'tool_calls' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await new Promise((resolve) => service.listen(0, '127.0.0.1', resolve))

/** What the tools handed back in a request the planner was sent. */
const toolResults = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').map((message) => message.content).join('\n')

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-exposure-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, '.env'), `AWS_ACCESS_KEY_ID=${KEY}\n`)
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  const trustIt = async () => {
    await trust.getByRole('button', { name: 'Trust this directory' }).click()
    await trust.waitFor({ state: 'hidden' })
  }
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await trustIt()

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
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
