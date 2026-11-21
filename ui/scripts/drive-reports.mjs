// Chat › Token Spend…, Context Breakdown… and Last Request… (CMD-15, CMD-19, TRACE-9), through the real menu,
// Electron IPC and the Rust bridge. Only the model server and the folder picker are substituted.
// Each report is opened before any turn, while a turn is held open by the model server (CMD-8),
// and after the turn ends.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust } from './drive-support.mjs'

const shots = process.env.REPORTS_SHOTS ?? join(tmpdir(), 'bravebot-reports')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-reports-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Reports\n')
const instructions = '\x1b[1mKeep answers short.\x1b[0m Ring \x07 once.\n'
writeFileSync(join(project, 'AGENTS.md'), instructions)
const token = 'reports-only-fake-token'
const failures = [], steps = []
let app, page
const server = createServer(async (req, res) => {
  try {
    for await (const _ of req);
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    const next = steps.shift()
    assert(next, 'Unexpected model request')
    const delta = await next()
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta, finish_reason: 'stop' }], usage: { prompt_tokens: 1200, completion_tokens: 30, total_tokens: 1230 } })}\n\ndata: [DONE]\n\n`)
  } catch (error) { failures.push(String(error)); res.writeHead(500); res.end('Fixture assertion failed') }
})
await listening(server)
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ provider: { local: { options: { baseURL: `http://127.0.0.1:${server.address().port}/v1`, apiKey: token }, models: { test: {} } } }, model: 'local/test' }))
const env = Object.assign(inheritedEnv(), { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost' })
async function until(predicate, label, ms = 20000) {
  const deadline = Date.now() + ms
  while (!await predicate()) { assert(Date.now() < deadline, `${label} timed out; fixture errors: ${failures}`); await delay(50) }
}

try {
  const world = await launchWorld({ profile, env, linuxArgs: ['--disable-gpu'], timeout: 15000, viewport: { width: 1350, height: 900 } })
  app = world.app; page = world.page
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await app.evaluate(({ dialog }, folder) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] }) }, project)
  await page.evaluate(() => { window.reportEvents = []; window.bravebot.onEvent(event => window.reportEvents.push(event)) })
  const ended = async () => (await page.evaluate(() => window.reportEvents)).filter(e => e.event === 'turn.done' || e.event === 'turn.error')
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled }
  }, id)
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  // Open a report from the menu, read it, optionally photograph it, and close it.
  const report = async (id, title, shot) => {
    assert.equal((await menuItem(id)).enabled, true, `${id} is offered`)
    await choose(id)
    const dialog = page.getByRole('dialog', { name: title, exact: true })
    await dialog.waitFor({ timeout: 5000 })
    const read = { body: await dialog.locator('pre.notice-body').innerText(), whole: await dialog.innerText() }
    if (shot) {
      for (const scheme of ['light', 'dark']) {
        await page.emulateMedia({ colorScheme: scheme })
        await page.waitForTimeout(150)
        await page.screenshot({ path: join(shots, `${shot}-${scheme}.png`) })
      }
      await page.emulateMedia({ colorScheme: 'light' })
    }
    await dialog.getByRole('button', { name: 'Done', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
    return read
  }
  // Open Last Request… (TRACE-9) and read its spans as { head, text } pairs.
  // `shown` names the span head to scroll to before the screenshot.
  const lastRequest = async (shot, shown) => {
    assert.equal((await menuItem('session.request')).enabled, true, 'Last Request is offered')
    await choose('session.request')
    const dialog = page.getByRole('dialog', { name: 'Last request', exact: true })
    await dialog.waitFor({ timeout: 5000 })
    const read = {
      whole: await dialog.innerText(),
      files: await dialog.locator('[data-test="request-files"]').innerText().catch(() => null),
      spans: await dialog.locator('[data-test="request-span"]').evaluateAll(spans => spans.map(span => ({
        head: span.querySelector('.request-span-head').textContent,
        text: span.querySelector('.request-span-text').textContent,
      }))),
    }
    if (shown) await dialog.locator('.request-span-head', { hasText: shown }).first().scrollIntoViewIfNeeded()
    if (shot) {
      for (const scheme of ['light', 'dark']) {
        await page.emulateMedia({ colorScheme: scheme })
        await page.waitForTimeout(150)
        await page.screenshot({ path: join(shots, `${shot}-${scheme}.png`) })
      }
      await page.emulateMedia({ colorScheme: 'light' })
    }
    await dialog.getByRole('button', { name: 'Done', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
    return read
  }

  await page.getByRole('button', { name: 'Open project', exact: true }).waitFor()
  assert.equal((await menuItem('session.cost')).enabled, false, 'Token Spend is grey with nothing open')
  assert.equal((await menuItem('session.context')).enabled, false, 'Context Breakdown is grey with nothing open')
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })
  await until(async () => (await menuItem('session.cost')).enabled, 'Token Spend offered on an open chat')

  // Before any turn: both name their empty state.
  assert.equal((await report('session.cost', 'Token spend')).body, 'Nothing spent yet.')
  assert.match((await report('session.context', 'Context breakdown', 'context-empty')).body, /has not been measured yet/)
  const none = await lastRequest('last-request-none')
  assert.match(none.whole, /No request has been sent to the model in this session yet\./)
  assert.deepEqual(none.spans, [])
  console.log('PASS: before a turn, Token Spend says nothing is spent, Context Breakdown says nothing is measured and Last Request says there is no request yet')

  // During a turn: the model server holds the answer until both reports have been read.
  let release
  steps.push(() => new Promise(resolve => { release = () => resolve({ content: 'Done.' }) }))
  const before = (await ended()).length
  await page.getByRole('textbox', { name: COMPOSER_NAME }).fill('Say done.')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => release !== undefined, 'the model asked')
  // The model has not answered yet, so the turn has spent nothing, as the terminal would say.
  assert.equal((await report('session.cost', 'Token spend', 'token-spend-mid-turn')).body, 'Nothing spent yet.')
  assert.match((await report('session.context', 'Context breakdown')).body, /has not been measured yet/)
  assert.ok((await lastRequest()).spans.some(span => span.text === 'Say done.'), 'mid-turn, Last Request shows the request the running turn sent')
  console.log('PASS: during a turn the reports open without waiting for it')
  release()
  await until(async () => (await ended()).length > before, 'turn end')
  assert.equal((await ended()).at(-1).event, 'turn.done')
  await page.getByText('Done.', { exact: true }).waitFor()

  // After the turn: spend is by turn, and the context is broken down by section and approximate.
  const cost = await report('session.cost', 'Token spend', 'token-spend')
  assert.match(cost.body, /^This session +1 turn · 1\.2k tokens/)
  assert.match(cost.body, /\nTurn 1 +1\.2k tokens +100%/)
  const context = await report('session.context', 'Context breakdown', 'context-breakdown')
  assert.match(context.whole, /Approximate/)
  assert.match(context.body, /^Last request +1\.2k tokens · context \d+%/)
  assert.match(context.body, /System prompt/)
  console.log(`PASS: after the turn, Token Spend lists turn 1 and Context Breakdown is approximate:\n${context.body}`)

  // After the turn: the instruction file with its bytes, then each span under its TRACE-9 label.
  // The project's AGENTS.md holds an escape and a bell, which the bridge sends as their pictures.
  const request = await lastRequest('last-request', 'trusted file AGENTS.md')
  // The bytes are of what was sent, which leaves out the file's last newline.
  assert.equal(request.files, `Instruction files loaded: AGENTS.md (${Buffer.byteLength(instructions.trimEnd())} bytes)`)
  const heads = request.spans.map(span => span.head)
  assert.ok(heads.includes('system · driver'), `a driver span: ${heads}`)
  assert.ok(heads.includes('system · trusted file AGENTS.md'), `the instruction file's span: ${heads}`)
  assert.ok(request.spans.some(span => span.head === 'user · typed' && span.text === 'Say done.'), `the typed line: ${heads}`)
  const file = request.spans.find(span => span.head === 'system · trusted file AGENTS.md')
  assert.match(file.text, /␛\[1mKeep answers short\.␛\[0m Ring ␇ once\./)
  assert.doesNotMatch(request.whole, /[\x1b\x07]/)
  // A span's line breaks are kept rather than pictured.
  assert.ok(request.spans.some(span => span.text.includes('\n')), 'a span keeps its line breaks')
  assert.doesNotMatch(request.whole, /␊/)
  console.log(`PASS: after the turn, Last Request lists AGENTS.md and labels each span: ${heads.join(' | ')}`)

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: Token Spend, Context Breakdown and Last Request against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'reports-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
