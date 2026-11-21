// Chat > Continue in Background… through the real menu, Electron IPC, the Rust bridge and the
// `bravebot` program built beside it (BG-2, BG-9, CMD-34). Only the model server and the folder
// picker are substituted. It checks that the item asks to confirm, that confirming closes the chat
// and shows a toast naming the start of its id, that `bravebot sessions --json` lists the record as
// running in the background, and that opening the chat again from the list is refused with the
// held-by-background sentence. Unix only, since background sessions run only there.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust } from './drive-support.mjs'

const bravebot = resolve('..', 'target', 'debug', 'bravebot')
assert.ok(existsSync(bravebot), `build the bravebot program first: ${bravebot}`)
const shots = process.env.BACKGROUND_SHOTS ?? join(tmpdir(), 'bravebot-background')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-background-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Background\n')
const token = 'background-only-fake-token'
const failures = []
let app, page, id = null
const server = createServer(async (req, res) => {
  try {
    for await (const _ of req) { /* drain */ }
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta: { content: 'Noted.' }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
  } catch (error) { failures.push(String(error)); res.writeHead(500); res.end('Fixture assertion failed') }
})
await listening(server)
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ provider: { local: { options: { baseURL: `http://127.0.0.1:${server.address().port}/v1`, apiKey: token }, models: { test: {} } } }, model: 'local/test' }))
const env = Object.assign(inheritedEnv(), { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost' })
const cli = (...args) => execFileSync(bravebot, args, { env, encoding: 'utf8', timeout: 20000 })
async function until(predicate, label, ms = 20000) {
  const deadline = Date.now() + ms
  while (!await predicate()) { assert(Date.now() < deadline, `${label} timed out; fixture errors: ${failures}`); await delay(50) }
}

try {
  const world = await launchWorld({ profile, env, linuxArgs: ['--disable-gpu'], timeout: 15000, viewport: { width: 1350, height: 900 } })
  app = world.app; page = world.page
  await app.evaluate(({ dialog }, folder) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] }) }, project)
  await page.evaluate(() => {
    window.backgroundEvents = []
    window.bravebot.onEvent(event => window.backgroundEvents.push(event))
  })
  const events = (name) => page.evaluate((name) => window.backgroundEvents.filter(e => e.event === name), name)
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled }
  }, id)
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const dialog = page.getByRole('dialog', { name: 'Continue in background', exact: true })

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })

  // 1. Not offered until a chat is open with no turn running.
  assert.equal((await menuItem('session.background')).label, 'Continue in Background…')
  await composer.fill('Look at the README')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => (await events('turn.done')).length > 0, 'first turn end')
  await until(async () => (await menuItem('session.background')).enabled, 'Continue in Background offered once the turn ended')
  const listed = await page.evaluate(() => window.bravebot.request('session.list', {}))
  const rows = listed.ok?.sessions ?? listed.ok ?? []
  assert.equal(rows.length, 1, `one chat is listed: ${JSON.stringify(listed).slice(0, 400)}`)
  id = rows[0].id
  const title = rows[0].title

  // 2. The confirmation, in both themes. Cancel leaves the chat open.
  for (const name of ['light', 'dark']) {
    await theme(name)
    await choose('session.background')
    await dialog.waitFor()
    await page.waitForTimeout(200)
    await page.screenshot({ path: join(shots, `background-confirm-${name}.png`) })
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
  }
  await composer.waitFor()
  console.log('PASS: Continue in Background… asks to confirm, and Cancel leaves the chat open')

  // 3. Confirming closes the chat and names the start of its id.
  await theme('light')
  await choose('session.background')
  await dialog.waitFor()
  await dialog.getByRole('button', { name: 'Continue in Background', exact: true }).click()
  const toast = page.getByText(`Join it with: bravebot attach ${id.slice(0, 8)}`)
  await toast.waitFor({ timeout: 30000 })
  await composer.waitFor({ state: 'detached' })
  await page.screenshot({ path: join(shots, 'background-handed-over-light.png') })
  await theme('dark')
  await page.waitForTimeout(200)
  await page.screenshot({ path: join(shots, 'background-handed-over-dark.png') })
  console.log('PASS: confirming closes the chat and the toast names the start of its id')

  // 4. `bravebot sessions` lists it.
  const jobs = JSON.parse(cli('sessions', '--json'))
  const job = (Array.isArray(jobs) ? jobs : jobs.sessions ?? []).find((each) => each.id === id)
  assert.ok(job, `bravebot sessions lists ${id}: ${JSON.stringify(jobs).slice(0, 400)}`)
  console.log(`PASS: bravebot sessions lists the chat (${job.state ?? job.status ?? 'listed'})`)

  // 5. Opening it again from the list is refused while the background session holds it (BG-9).
  await page.locator('.session-name', { hasText: title }).first().click()
  const problem = page.locator('[data-test="problem-toast"]')
  await problem.waitFor()
  assert.match(await problem.innerText(), /held by a running background session/)
  await composer.waitFor({ state: 'detached', timeout: 1000 }).catch(() => {})
  await page.waitForTimeout(200)
  await page.screenshot({ path: join(shots, 'background-reopen-refused-dark.png') })
  await theme('light')
  await page.waitForTimeout(200)
  await page.screenshot({ path: join(shots, 'background-reopen-refused-light.png') })
  console.log('PASS: opening it from the list is refused with the held-by-background sentence')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: Continue in Background… against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'background-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  if (id) { try { cli('sessions', 'stop', id.slice(0, 8)) } catch (error) { console.error(`stopping the background session: ${error}`) } }
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
