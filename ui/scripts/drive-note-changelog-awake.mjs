// Add Note, Release Notes and Keep Computer Awake During Turns, through the real menu, Electron IPC
// and the Rust bridge. Only the model server and the folder picker are substituted, and Electron's
// powerSaveBlocker is watched rather than replaced.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, TRUST_APPROVE } from './drive-support.mjs'

const shots = process.env.EXTRAS_SHOTS ?? join(tmpdir(), 'bravebot-extras')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-extras-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Extras\n')
const token = 'extras-only-fake-token'
const failures = [], steps = []
let app, page
const content = text => ({ content: text })
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
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
  await app.evaluate(({ dialog, powerSaveBlocker }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] })
    // Watched, not replaced: the real blocker is still taken and let go.
    globalThis.__blocks = []
    const start = powerSaveBlocker.start.bind(powerSaveBlocker), stop = powerSaveBlocker.stop.bind(powerSaveBlocker)
    powerSaveBlocker.start = (type) => { const id = start(type); globalThis.__blocks.push(['start', type, id]); return id }
    powerSaveBlocker.stop = (id) => { globalThis.__blocks.push(['stop', id]); return stop(id) }
  }, project)
  await page.evaluate(() => {
    window.extrasEvents = []
    window.bravebot.onEvent(event => window.extrasEvents.push(event))
  })
  const ended = async () => (await page.evaluate(() => window.extrasEvents)).filter(e => e.event === 'turn.done' || e.event === 'turn.error')
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled, checked: item.checked }
  }, id)
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  const held = async () => {
    const blocks = await app.evaluate(() => globalThis.__blocks)
    return blocks.filter(([kind]) => kind === 'start').length - blocks.filter(([kind]) => kind === 'stop').length
  }
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const turn = async (prompt, reply) => {
    steps.push(reply)
    const before = (await ended()).length
    await composer.fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    return before
  }
  const turnEnded = async (before) => {
    await until(async () => (await ended()).length > before, 'turn end')
    assert.equal((await ended()).at(-1).event, 'turn.done')
  }

  // 1. Release notes, from the Help menu, with nothing open.
  await page.getByRole('button', { name: 'Open project', exact: true }).waitFor()
  const changelog = await menuItem('help.changelog')
  assert.equal(changelog.enabled, true)
  await choose('help.changelog')
  const notes = page.getByRole('dialog', { name: /^Release notes/ })
  await notes.waitFor()
  const body = await notes.locator('pre.notice-body').innerText()
  assert.ok(body.trim().length > 0, 'the notice has a body')
  assert.equal(await notes.locator('pre.notice-body a, pre.notice-body strong, pre.notice-body h2').count(), 0, 'drawn as plain text')
  await page.screenshot({ path: join(shots, 'release-notes.png') })
  await notes.getByRole('button', { name: 'Done', exact: true }).click()
  await notes.waitFor({ state: 'detached' })
  console.log(`PASS: Help > Release Notes shows "${body.split('\n')[0].slice(0, 60)}" as plain text`)

  // 2. A note on a chat's record.
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await page.locator(TRUST_APPROVE).click()
  await page.getByRole('dialog', { name: 'Project trust', exact: true }).waitFor({ state: 'hidden' })
  assert.equal((await menuItem('session.note')).label, 'Add Note…')
  await turnEnded(await turn('Say hello.', () => content('Hello.')))
  await page.getByText('Hello.', { exact: true }).waitFor()
  await until(async () => (await menuItem('session.note')).enabled, 'Add Note enabled')
  await choose('session.note')
  const dialog = page.getByRole('dialog', { name: 'Add note', exact: true })
  await dialog.waitFor()
  await dialog.getByText('Kept on this chat record. Never sent to the model.', { exact: true }).waitFor()
  const text = 'Check <b>this</b> & **that** later.'
  await dialog.locator('textarea').fill(text)
  await page.waitForTimeout(200)
  await page.screenshot({ path: join(shots, 'note-dialog.png') })
  await dialog.getByRole('button', { name: 'Add', exact: true }).click()
  await dialog.waitFor({ state: 'detached' })
  const listed = page.locator('[data-test="notes"] li')
  await until(async () => await listed.count() === 1, 'the note listed')
  assert.equal(await listed.locator('.note-text').innerText(), text, 'the note is drawn as typed')
  assert.equal(await listed.locator('b').count(), 0)
  assert.match(await listed.locator('.note-turn').innerText(), /Turn 1/)
  assert.equal(await page.locator('.bubble.user').filter({ hasText: 'Check' }).count(), 0, 'the note is not a message')
  console.log('PASS: Chat > Add Note keeps a note in the Notes panel, drawn as typed, and sends nothing')

  // 3. Keeping the computer awake: Cancel leaves it off, Enable turns it on, and it is held only during a turn.
  assert.equal((await menuItem('app.caffeinate')).checked, false, 'off at launch')
  await choose('app.caffeinate')
  const awake = page.getByRole('dialog', { name: 'Keep computer awake during turns?', exact: true })
  await awake.waitFor()
  await page.screenshot({ path: join(shots, 'keep-awake-dialog.png') })
  await awake.getByRole('button', { name: 'Cancel', exact: true }).click()
  await awake.waitFor({ state: 'detached' })
  await page.waitForTimeout(200)
  assert.equal((await menuItem('app.caffeinate')).checked, false, 'Cancel leaves the tick off')
  await choose('app.caffeinate')
  await awake.waitFor()
  await awake.getByRole('button', { name: 'Enable', exact: true }).click()
  await awake.waitFor({ state: 'detached' })
  await until(async () => (await menuItem('app.caffeinate')).checked, 'the tick on')
  assert.equal(await held(), 0, 'nothing held between turns')

  let release
  const before = await turn('Take your time.', () => new Promise(resolve => { release = () => resolve(content('Done slowly.')) }))
  await until(async () => release !== undefined && await held() === 1, 'held while the model is answering')
  assert.deepEqual((await app.evaluate(() => globalThis.__blocks))[0].slice(0, 2), ['start', 'prevent-app-suspension'])
  release()
  await turnEnded(before)
  await until(async () => await held() === 0, 'let go when the turn ended')
  console.log('PASS: Keep Computer Awake asks, Cancel leaves it off, Enable holds the blocker only during a turn')

  await choose('app.caffeinate')
  await until(async () => !(await menuItem('app.caffeinate')).checked, 'the tick off')
  await choose('app.caffeinate')
  await until(async () => (await menuItem('app.caffeinate')).checked, 'on again')
  assert.equal(await awake.count(), 0, 'not asked a second time')
  console.log('PASS: once agreed, it is not asked again')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: note, release notes and keep-awake against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'extras-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
