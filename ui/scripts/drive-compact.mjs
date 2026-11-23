// Chat > Compact Conversation… through the real menu, Electron IPC and the Rust bridge (CMD-34).
// Only the model server and the folder picker are substituted. It checks that the item is greyed
// while a turn runs, that compacting an idle chat shows the compacting row and then a notice with
// what was summarised and kept, and that Cancel Turn stops a compaction.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust } from './drive-support.mjs'

const shots = process.env.COMPACT_SHOTS ?? join(tmpdir(), 'bravebot-compact')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-compact-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Compact\n')
const token = 'compact-only-fake-token'
const failures = [], steps = [], asked = []
let app, page
const content = text => ({ content: text })
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    asked.push(JSON.parse(raw))
    const next = steps.shift()
    assert(next, `Unexpected model request: ${raw.slice(-400)}`)
    const delta = await next()
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
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
    window.compactEvents = []
    window.bravebot.onEvent(event => window.compactEvents.push(event))
  })
  const events = (name) => page.evaluate((name) => window.compactEvents.filter(e => e.event === name), name)
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled }
  }, id)
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const ended = async () => (await events('turn.done')).length + (await events('turn.error')).length
  const say = async (prompt, reply = () => content('Noted.')) => {
    steps.push(reply)
    const before = await ended()
    await composer.fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await until(async () => await ended() > before, 'turn end')
    assert.equal((await events('turn.done')).length > 0, true)
  }
  const long = `Early detail: ${'the parser drops a token. '.repeat(200)}`
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }
  const dialog = page.getByRole('dialog', { name: 'Compact conversation', exact: true })
  const toast = page.locator('[data-test="status-toast"]')

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })

  // 1. Greyed while a turn runs.
  assert.equal((await menuItem('session.compact')).label, 'Compact Conversation…')
  let release
  steps.push(() => new Promise(resolve => { release = () => resolve(content('Noted.')) }))
  const before = await ended()
  await composer.fill(long)
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => release !== undefined, 'the turn reaches the model')
  await until(async () => !(await menuItem('session.compact')).enabled, 'Compact greyed while the turn runs')
  release()
  await until(async () => await ended() > before, 'turn end')
  await until(async () => (await menuItem('session.compact')).enabled, 'Compact offered once the turn ended')
  console.log('PASS: Compact Conversation… is greyed while a turn runs and offered once it ends')

  // 2. Compacting an idle chat: the dialog, the compacting row, and the notice.
  await say(long)
  await say('How does fetch_page handle errors?')
  await say('And the tests?')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await choose('session.compact')
    await dialog.waitFor()
    await dialog.getByText('Summarises earlier messages so less is sent to the model.', { exact: true }).waitFor()
    await dialog.locator('textarea').fill('the parser')
    await page.waitForTimeout(200)
    await page.screenshot({ path: join(shots, `compact-dialog-${name}.png`) })
    if (name === 'dark') break
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
  }
  await theme('light')
  let summarise
  steps.push(() => new Promise(resolve => { summarise = () => resolve(content('Summary: the parser drops a token.')) }))
  await dialog.getByRole('button', { name: 'Compact', exact: true }).click()
  await dialog.waitFor({ state: 'detached' })
  await until(async () => summarise !== undefined, 'the summary request reaches the model')
  const request = asked.at(-1)
  assert.ok(!request.tools || request.tools.length === 0, 'the summary request offers no tools')
  assert.match(JSON.stringify(request.messages), /the parser/, 'the focus is in the request')
  await page.getByText('Compacting…', { exact: true }).first().waitFor()
  assert.equal((await menuItem('session.compact')).enabled, false, 'greyed while it compacts')
  assert.equal((await menuItem('turn.cancel')).enabled, true, 'Cancel Turn offered while it compacts')
  await page.screenshot({ path: join(shots, 'compact-running-light.png') })
  summarise()
  await toast.filter({ hasText: 'Conversation compacted' }).waitFor()
  const said = await toast.innerText()
  assert.match(said, /Summarised \d+ earlier messages?, kept the last \d+ as they are/)
  await page.waitForTimeout(700)
  await page.screenshot({ path: join(shots, 'compact-done-light.png') })
  const [done] = await events('compact.done')
  assert.equal(done.data.status, 'compacted')
  await until(async () => (await menuItem('session.compact')).enabled, 'offered again once it ended')
  console.log(`PASS: compacting an idle chat shows the compacting row, then "${said.replace(/\s+/g, ' ').trim()}"`)

  // 3. Cancel Turn stops a compaction.
  await theme('dark')
  await say(long)
  await say(long)
  await say('One more question.')
  await toast.first().waitFor({ state: 'detached', timeout: 10000 }).catch(() => {})
  let never
  steps.push(() => new Promise(resolve => { never = resolve }))
  await choose('session.compact')
  await dialog.waitFor()
  await dialog.getByRole('button', { name: 'Compact', exact: true }).click()
  await dialog.waitFor({ state: 'detached' })
  await until(async () => never !== undefined, 'the second summary request reaches the model')
  await page.getByText('Compacting…', { exact: true }).first().waitFor()
  await page.screenshot({ path: join(shots, 'compact-running-dark.png') })
  await choose('turn.cancel')
  await toast.filter({ hasText: 'Compaction stopped' }).waitFor()
  await page.waitForTimeout(700)
  await page.screenshot({ path: join(shots, 'compact-stopped-dark.png') })
  assert.equal((await events('compact.done')).at(-1).data.status, 'stopped')
  await until(async () => (await menuItem('session.compact')).enabled, 'offered again once stopped')
  never(content('too late'))
  console.log('PASS: Cancel Turn stops a compaction, and the notice says so')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: Compact Conversation… against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'compact-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
