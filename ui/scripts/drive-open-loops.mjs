// OPEN-3's line on the desktop, through Electron IPC and the real Rust bridge. Only the model server
// and the folder picker are substituted. A turn that writes a file and builds nothing ends with an
// `open: changes not built (planner)` row between the reply and its footer, the next idle turn draws
// none, a stopped turn draws its line under the error card with the same gap, and the longest line
// wraps between words inside the reading column. The last is the bridge's `turn.done` resent with
// the spec's example line, since no fixture here leaves every kind open.
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
const failures = [], steps = [], held = []
let app, page, toolId = 0
const content = text => ({ content: text })
const call = (name, args) => ({ tool_calls: [{ index: 0, id: `call_${++toolId}`, type: 'function', function: { name, arguments: JSON.stringify(args) } }] })
const write = (path, contents) => call('write_file', { path, contents })
// The spec's example (OPEN-3), every kind but the goal: the longest line the bridge says.
const LONG = 'open: changes not built, the last command failed, 2 todos (planner); 1 kept checkout, 3 files not committed (you); 1 job running'
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    const next = steps.shift()
    assert(next, `Unexpected model request: ${raw.slice(-400)}`)
    const delta = next()
    // A step with no reply holds the request open, so the turn can be stopped while it waits.
    if (delta === null) { held.push(res); return }
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

  const both = async (name) => {
    for (const each of ['light', 'dark']) {
      await theme(each)
      await page.waitForTimeout(300)
      await page.screenshot({ path: join(shots, `open-loops-${name}-${each}.png`) })
    }
  }
  // Where the last line sits: the gap above it to the card it is under, the gap below it to the
  // turn's footer, its left edge against the card's, its height in lines and its width.
  const placed = () => page.locator('.entries').evaluate((el) => {
    const line = [...el.querySelectorAll('[data-test="open-loops"]')].pop()
    const card = line.parentElement.previousElementSibling
    const after = line.parentElement.nextElementSibling
    const [l, c] = [line.getBoundingClientRect(), card.getBoundingClientRect()]
    const text = line.querySelector('span').getBoundingClientRect()
    return {
      above: Math.round(l.top - c.bottom),
      footerAfter: after?.classList.contains('turn-footer') ?? false,
      leftEdge: Math.round(l.left - c.left),
      lines: Math.round(text.height / parseFloat(getComputedStyle(line).lineHeight)),
      width: Math.round(l.width),
    }
  })

  // 1. A write with no build after it leaves the changes open for the planner.
  await say('Write notes.md saying hello', () => write('notes.md', 'hello\n'), () => content('I wrote notes.md.'))
  const first = (await events('turn.done')).at(-1)
  assert.equal(first?.data.openLoops, 'open: changes not built (planner)', `turn.done carried ${JSON.stringify(first?.data.openLoops)}`)
  await lines.first().waitFor()
  assert.equal(await lines.count(), 1)
  assert.equal(await lines.first().innerText(), 'open: changes not built (planner)')
  const underReply = await placed()
  assert.ok(underReply.footerAfter, `the turn's footer is not the row after the line: ${JSON.stringify(underReply)}`)
  assert.equal(underReply.leftEdge, 0, `the line does not start on the reply's left edge: ${JSON.stringify(underReply)}`)
  await both('reply')
  console.log(`PASS: a turn that writes a file ends with the open-loops line under the reply, above the footer ${JSON.stringify(underReply)}`)

  // 2. A turn that changes nothing has no line of its own.
  await say('Say hi', () => content('Hi.'))
  assert.equal((await events('turn.done')).at(-1).data.openLoops, null)
  await page.getByText('Hi.', { exact: true }).waitFor()
  assert.equal(await lines.count(), 1, 'the idle turn drew an open-loops line')
  await both('idle-turn')
  console.log('PASS: the next idle turn draws no line')

  // 3. A turn stopped after it wrote todos carries the changed set on its error card.
  steps.push(() => call('todo_write', { todos: [{ content: 'Proofread notes', status: 'pending' }, { content: 'Add a heading', status: 'pending' }] }), () => null)
  const before = await ended()
  await composer.fill('Plan the notes')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(() => steps.length === 0, 'held model request')
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await until(async () => await ended() > before, 'stopped turn')
  const stopped = (await events('turn.error')).at(-1)
  assert.equal(stopped?.data.openLoops, 'open: changes not built, 2 todos (planner)', `turn.error carried ${JSON.stringify(stopped?.data.openLoops)}`)
  await until(async () => await lines.count() === 2, 'the line under the error')
  const underError = await placed()
  assert.equal(underError.above, underReply.above, `the gap under the error card differs from the gap under a reply: ${JSON.stringify({ underReply, underError })}`)
  assert.ok(underError.footerAfter, `the error's footer is not the row after the line: ${JSON.stringify(underError)}`)
  await both('error')
  console.log(`PASS: a stopped turn draws its line under the error card with the gap a reply has ${JSON.stringify(underError)}`)

  // 4. The longest line the bridge says wraps between words inside the reading column. The event
  // is the last real `turn.done` with that line in place of its own, sent as the bridge sends one.
  const last = (await events('turn.done')).at(-1)
  steps.length = 0
  await app.evaluate(({ BrowserWindow }, message) => BrowserWindow.getAllWindows().forEach((window) => window.webContents.send('bravebot:event', message)),
    { ...last, data: { ...last.data, turn: last.data.turn + 3, reply: 'Here is where the session stands.', openLoops: LONG } })
  await until(async () => await lines.count() === 3, 'the long line')
  assert.equal(await lines.last().innerText(), LONG)
  const wrapped = await placed()
  assert.ok(wrapped.lines >= 2, `the long line did not wrap: ${JSON.stringify(wrapped)}`)
  assert.ok(wrapped.width <= 720, `the long line is wider than the reading column: ${JSON.stringify(wrapped)}`)
  const breaks = await lines.last().evaluate((line) => {
    // Each line box's last character before a break, which is a space where it broke between words.
    const text = line.querySelector('span').firstChild, range = document.createRange(), ends = []
    let top = null
    for (let at = 0; at < text.length; at++) {
      range.setStart(text, at); range.setEnd(text, at + 1)
      const y = Math.round(range.getBoundingClientRect().top)
      if (top !== null && y > top) ends.push(text.data[at - 1])
      top = y
    }
    return ends
  })
  assert.ok(breaks.length >= 1 && breaks.every((c) => c === ' '), `the long line broke inside a word: ${JSON.stringify(breaks)}`)
  await both('long-line')
  console.log(`PASS: the longest line wraps between words inside the column ${JSON.stringify(wrapped)}`)

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: the open-loops line against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'open-loops-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  for (const res of held) res.destroy()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
