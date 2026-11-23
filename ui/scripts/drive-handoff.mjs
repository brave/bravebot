// Chat > Hand Off to New Chat… through the real menu, Electron IPC and the Rust bridge (SESSION-34,
// CMD-34). Only the model server and the folder picker are substituted. It checks that the brief is
// put in the box with a banner, that Escape drops it and puts back the draft it replaced, and that
// Send on an edited brief opens a new chat whose first prompt is the edited text, whose notices
// name the source chat, and whose record lists the source as the chat it was handed off from.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust, approveTrustIfAsked } from './drive-support.mjs'

const shots = process.env.HANDOFF_SHOTS ?? join(tmpdir(), 'bravebot-handoff')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-handoff-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Handoff\n')
const token = 'handoff-only-fake-token'
const BRIEF = 'We added a retry with backoff to fetch_page in src/net.rs.'
const GOAL = 'write the tests for the retry'
const EDIT = ' Use the fake clock.'
const failures = [], asked = []
let app, page
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    asked.push(raw)
    const text = raw.includes('I am starting a new session to carry this work on') ? BRIEF : 'Noted.'
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta: { content: text }, finish_reason: 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
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
    window.handoffEvents = []
    window.bravebot.onEvent(event => window.handoffEvents.push(event))
  })
  const events = (name) => page.evaluate((name) => window.handoffEvents.filter(e => e.event === name), name)
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled }
  }, id)
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const boxText = () => composer.inputValue()
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }
  const dialog = page.getByRole('dialog', { name: 'Hand off to a new chat', exact: true })
  const banner = page.locator('[data-test="handoff-armed"]')
  const ask = async () => {
    await choose('session.handoff')
    await dialog.waitFor()
    await dialog.locator('textarea').fill(GOAL)
    await dialog.getByRole('button', { name: 'Write brief', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
    await banner.waitFor()
  }

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })

  // 1. A first turn, so there is something to write a brief from.
  assert.equal((await menuItem('session.handoff')).label, 'Hand Off to New Chat…')
  await composer.fill('Add a retry to fetch_page')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => (await events('turn.done')).length > 0, 'first turn end')
  await until(async () => (await menuItem('session.handoff')).enabled, 'Hand Off offered once the turn ended')
  const source = (await events('turn.done'))[0].session

  // 2. The dialog, in both themes.
  for (const name of ['light', 'dark']) {
    await theme(name)
    await choose('session.handoff')
    await dialog.waitFor()
    await dialog.locator('textarea').fill(GOAL)
    await page.waitForTimeout(200)
    await page.screenshot({ path: join(shots, `handoff-dialog-${name}.png`) })
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
  }

  // 3. A ready brief goes in the box over the draft, and Escape puts the draft back.
  await composer.fill('my half-written draft')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await ask()
    const text = await boxText()
    assert.ok(text.startsWith(BRIEF), `the box starts with the brief: ${text}`)
    assert.ok(text.trimEnd().endsWith(GOAL), `the box ends with the goal: ${text}`)
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, `handoff-armed-${name}.png`) })
    await composer.focus()
    await page.keyboard.press('Escape')
    await banner.waitFor({ state: 'detached' })
    assert.equal(await boxText(), 'my half-written draft', 'Escape puts back the draft the brief replaced')
  }
  console.log('PASS: a ready brief goes in the box with a banner, and Escape puts back the earlier draft')

  // 4. Emptying the box drops the handoff.
  await ask()
  await composer.fill('')
  await banner.waitFor({ state: 'detached' })
  console.log('PASS: emptying the box drops the handoff')

  // 5. Send on an edited brief opens a new chat with that text as its first prompt.
  await theme('light')
  await composer.fill('my half-written draft')
  await ask()
  const edited = `${(await boxText()).trim()}${EDIT}`
  await composer.fill(edited)
  const requestsBefore = asked.length
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await approveTrustIfAsked(page)
  await until(async () => (await events('turn.done')).some((e) => e.session !== source), 'the new chat\'s first turn ends', 30000)
  const firstPrompt = asked.slice(requestsBefore).find((raw) => raw.includes('Use the fake clock.'))
  assert.ok(firstPrompt, 'the new chat\'s first request holds the edited brief')
  assert.ok(firstPrompt.includes('retry with backoff to fetch_page'), 'with the brief in it')
  const notice = page.locator('[data-test="handed-off-from"]')
  await notice.waitFor()
  assert.match(await notice.innerText(), /Handed off from/)
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(shots, 'handoff-new-chat-light.png') })
  await theme('dark')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(shots, 'handoff-new-chat-dark.png') })
  const listed = await page.evaluate(() => window.bravebot.request('session.list', {}))
  const rows = listed.ok?.sessions ?? listed.ok ?? []
  const child = rows.find((row) => row.handedOffFrom)
  assert.ok(child, `a listed chat names the chat it was handed off from: ${JSON.stringify(listed).slice(0, 400)}`)
  assert.ok(rows.some((row) => row.id === child.handedOffFrom), 'and that is the source chat\'s record')
  console.log('PASS: Send opens a new chat with the edited brief as its first prompt, linked to the source chat')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: Hand Off to New Chat… against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'handoff-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
