// OPEN-3's line on the desktop, through Electron IPC and the real Rust bridge. Only the model server
// and the folder picker are substituted. A turn that writes a file and builds nothing ends with a
// muted `open: changes not built (planner)` line under the reply, and the next idle turn draws none.
// Screenshots in light and dark go to OPEN_LOOPS_SHOTS, or /tmp/bravebot-open-loops.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust } from './drive-support.mjs'

const shots = process.env.OPEN_LOOPS_SHOTS ?? join(tmpdir(), 'bravebot-open-loops')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-open-loops-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Open loops\n')
const token = 'open-loops-only-fake-token'
const failures = [], steps = []
let app, page, toolId = 0
const content = text => ({ content: text })
const write = (path, contents) => ({ tool_calls: [{ index: 0, id: `call_${++toolId}`, type: 'function', function: { name: 'write_file', arguments: JSON.stringify({ path, contents }) } }] })
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    const next = steps.shift()
    assert(next, `Unexpected model request: ${raw.slice(-400)}`)
    const delta = next()
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta, finish_reason: delta.tool_calls ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
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
  await app.evaluate(({ dialog }, folder) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] }) }, project)
  await page.evaluate(() => {
    window.openLoopsEvents = []
    window.bravebot.onEvent(event => window.openLoopsEvents.push(event))
  })
  const events = (name) => page.evaluate((name) => window.openLoopsEvents.filter(e => e.event === name), name)
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const ended = async () => (await events('turn.done')).length + (await events('turn.error')).length
  const say = async (prompt, ...replies) => {
    steps.push(...replies)
    const before = await ended()
    await composer.fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await until(async () => {
      const approve = page.locator('.confirm .approve:not([data-clicked])').filter({ visible: true })
      if (await approve.count() > 0) await approve.first().evaluate(el => { el.dataset.clicked = '1'; el.click() }).catch(() => {})
      return await ended() > before
    }, 'turn end')
  }
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }
  const lines = page.locator('[data-test="open-loops"]')

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })

  // 1. A write with no build after it leaves the changes open for the planner.
  await say('Write notes.md saying hello', () => write('notes.md', 'hello\n'), () => content('I wrote notes.md.'))
  const first = (await events('turn.done')).at(-1)
  assert.equal(first?.data.openLoops, 'open: changes not built (planner)', `turn.done carried ${JSON.stringify(first?.data.openLoops)}`)
  await lines.first().waitFor()
  assert.equal(await lines.count(), 1)
  assert.equal(await lines.first().innerText(), 'open: changes not built (planner)')
  const below = await page.locator('.entries').evaluate((el) => {
    const reply = [...el.querySelectorAll('.bubble.assistant')].pop().getBoundingClientRect().bottom
    return el.querySelector('[data-test="open-loops"]').getBoundingClientRect().top >= reply
  })
  assert.ok(below, 'the open-loops line is not under the reply')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, `open-loops-${name}.png`) })
  }
  console.log('PASS: a turn that writes a file ends with the open-loops line under the reply')

  // 2. A turn that changes nothing has no line of its own.
  await say('Say hi', () => content('Hi.'))
  assert.equal((await events('turn.done')).at(-1).data.openLoops, null)
  await page.getByText('Hi.', { exact: true }).waitFor()
  assert.equal(await lines.count(), 1, 'the idle turn drew an open-loops line')
  await page.screenshot({ path: join(shots, 'open-loops-idle-turn-dark.png') })
  console.log('PASS: the next idle turn draws no line')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: the open-loops line against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'open-loops-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
