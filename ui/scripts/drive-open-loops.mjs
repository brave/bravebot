// A reopened chat says at the top of its transcript what it left open (OPEN-6), against the real
// agent. Real Electron renderer and real `bravebot-rpc`, on a disposable HOME and profile. Before
// the app starts, the bridge is driven directly to take three turns against a stub model service:
// one that runs a command that fails, writes a file and answers at length, one that only answers,
// and one in a repository that leaves every kind of loop a resume can know open. The window then
// reopens each. The first shows the bridge's `Left open from earlier: …` sentence as a session
// notice that stays in view at the chat's end, the third names every kind and keeps the icon on
// the first line at the minimum window size, and the second, and a new chat, show none.
// Screenshots go to OPEN_LOOPS_SHOTS, or a temp directory.
//
//   pnpm run build && xvfb-run -a node scripts/drive-open-loops.mjs
import assert from 'node:assert/strict'
import { execFileSync, spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { launchWorld, makeWorld, startModelService } from './drive-world.mjs'

const shots = process.env.OPEN_LOOPS_SHOTS ?? join(tmpdir(), 'bravebot-open-loops')
mkdirSync(shots, { recursive: true })
const LEFT_OPEN = 'Run a check, then write out.txt'
const NOTHING_OPEN = 'Just say hello'
const EVERY = 'Leave every kind of loop open'
const WORKER = 'WORKER-WRITES-ONE'
const LINE = 'Left open from earlier: changes not built, the last command failed.'
const EVERY_LINE = 'Left open from earlier: changes not built, the last command failed, 2 todos; ' +
  '1 kept checkout, 1 file not committed, for you to act on.'
// The answer that ends the turn leaving loops open, long enough that the reopened chat is taller
// than the window and opens scrolled to its end.
const LONG_ANSWER = Array.from({ length: 40 }, (_, at) => `Paragraph ${at + 1} of what the turn found.`).join('\n\n')

const said = (messages, text) => messages.some((message) => message.role === 'user' && JSON.stringify(message.content).includes(text))
// The turn that leaves loops open runs `ls no-such-file`, then writes a file, then answers: the
// write comes after the failed command, so the changes are not built and the command is still the
// last one (OPEN-1). The turn that leaves every kind open starts a worker in a checkout that writes
// a file, writes two open todos, runs the failing command and writes a file in a repository. Each
// session has one turn, so every tool result in its request is from that turn. Every other
// request, the injection classifier's included, is answered `done`.
const { service } = await startModelService(({ body, messages }) => {
  if (body.includes('You are a prompt-injection classifier.')) return { content: 'done' }
  const tools = messages.filter((message) => message.role === 'tool').length
  if (said(messages, LEFT_OPEN)) {
    if (tools === 0) return { tool: { name: 'run', arguments: { command: 'ls no-such-file' } } }
    if (tools === 1) return { tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'written by the turn\n' } } }
    return { content: LONG_ANSWER }
  }
  if (said(messages, EVERY)) {
    if (tools === 0) return { tool: { name: 'spawn_agent', arguments: { kind: 'worker', task: WORKER, isolation: 'checkout' } } }
    if (tools === 1) return { tool: { name: 'todo_write', arguments: { todos: [{ content: 'check the parser', status: 'pending' }, { content: 'update the notes', status: 'in_progress' }] } } }
    if (tools === 2) return { tool: { name: 'run', arguments: { command: 'ls no-such-file' } } }
    if (tools === 3) return { tool: { name: 'write_file', arguments: { path: 'new.txt', contents: 'not committed\n' } } }
    return { content: 'done' }
  }
  // The worker, which is not shown the person's prompt.
  if (body.includes(WORKER)) {
    if (tools === 0) return { tool: { name: 'write_file', arguments: { path: 'one.txt', contents: 'from the worker\n' } } }
    return { content: 'done' }
  }
  return { content: 'done' }
})
const world = makeWorld('bravebot-open-loops-world-', { service, dirs: ['repository'] })
// A repository with one commit, so the worker can be given a checkout of it and a file the turn
// writes there is not committed.
const repository = join(world.root, 'repository')
writeFileSync(join(repository, 'README'), 'committed\n')
const git = (...args) => execFileSync('git', ['-c', 'user.name=Test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', ...args], { cwd: repository, stdio: 'ignore' })
git('init', '--quiet')
git('add', 'README')
git('commit', '--quiet', '-m', 'first')

/** The bridge on the world's HOME, for seeding the store before the app starts. */
function bridge() {
  const child = spawn(resolve('../target/debug/bravebot-rpc'), [], { env: world.env, stdio: ['pipe', 'pipe', 'ignore'] })
  const pending = new Map()
  const events = []
  const waiters = []
  let asked = 0
  createInterface({ input: child.stdout }).on('line', (line) => {
    const message = JSON.parse(line)
    if (message.id !== undefined) pending.get(message.id)?.(message)
    else {
      events.push(message)
      for (const waiter of [...waiters]) waiter()
    }
  })
  const call = (method, params) => new Promise((done, failed) => {
    const id = ++asked
    const timer = setTimeout(() => failed(new Error(`${method} was never answered`)), 30000)
    pending.set(id, (message) => {
      clearTimeout(timer)
      if (message.error) failed(new Error(`${method}: ${message.error.code} ${message.error.message}`))
      else done(message.ok)
    })
    child.stdin.write(`${JSON.stringify({ id, method, params })}\n`)
  })
  const event = (wanted) => new Promise((done, failed) => {
    const timer = setTimeout(() => failed(new Error('the turn never ended')), 30000)
    const look = () => {
      const at = events.findIndex(wanted)
      if (at < 0) return
      waiters.splice(waiters.indexOf(look), 1)
      clearTimeout(timer)
      done(events.splice(at, 1)[0])
    }
    waiters.push(look)
    look()
  })
  const ended = new Promise((done) => child.once('exit', done))
  return { call, event, close: async () => { child.stdin.end(); child.kill(); await ended } }
}

/** One turn in a new session of `directory`, approving every question; the session's record id. */
async function takeTurn(rpc, directory, prompt) {
  const { session } = await rpc.call('session.new', { directory })
  await rpc.call('trust.reply', { session, trusted: true }).catch((error) => {
    if (!/no_such_request/.test(error.message)) throw error
  })
  await rpc.call('turn.send', { session, prompt, messageId: randomUUID() })
  for (;;) {
    const next = await rpc.event((message) => message.session === session &&
      ['confirm.request', 'turn.done', 'turn.error'].includes(message.event))
    if (next.event === 'confirm.request') {
      await rpc.call('confirm.reply', { session, request: next.data.request, decision: 'approve' })
      continue
    }
    assert.equal(next.event, 'turn.done', JSON.stringify(next))
    await rpc.call('session.close', { session })
    return next.data.id
  }
}

const rpc = bridge()
await takeTurn(rpc, world.project, LEFT_OPEN)
await takeTurn(rpc, world.project, NOTHING_OPEN)
await takeTurn(rpc, repository, EVERY)
await rpc.close()

let app, page
try {
  const launched = await launchWorld({ profile: world.profile, env: world.env, viewport: { width: 1350, height: 900 } })
  app = launched.app; page = launched.page
  const list = page.locator('.session-list').first()
  const rowOf = (title) => list.locator('.session-row').filter({ hasText: title })
  const show = async (title) => {
    await rowOf(title).locator('button.session').click()
    await rowOf(title).and(page.locator('.current')).waitFor()
  }
  const notice = page.locator('[data-test="open-loops"]')
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }

  /** Whether the notice lies inside the transcript's scroll area as it is drawn now. */
  const inView = () => page.evaluate(() => {
    const notice = document.querySelector('[data-test="open-loops"]').getBoundingClientRect()
    const area = document.querySelector('.entries').getBoundingClientRect()
    return notice.top >= area.top && notice.bottom <= area.bottom
  })

  // 1. The chat that left a failed command and unbuilt changes says so above its transcript. Its
  // transcript is taller than the window and opens at its end, and the notice is still in view.
  await rowOf(LEFT_OPEN).waitFor()
  await show(LEFT_OPEN)
  await notice.waitFor()
  assert.equal((await notice.innerText()).trim(), LINE)
  assert.ok(!(await notice.innerText()).includes('out.txt'), 'the sentence names no path')
  await page.getByText('Paragraph 40 of what the turn found.').waitFor()
  await page.waitForFunction(() => {
    const entries = document.querySelector('.entries')
    return entries.scrollTop > 0 && entries.scrollHeight - entries.scrollTop - entries.clientHeight < 2
  }, null, { timeout: 10000 }).catch((error) => {
    throw new Error(`the chat did not reach its end within 10 s of opening: ${error.message}`)
  })
  assert.ok(await page.locator('.entries').evaluate((entries) => entries.scrollHeight > entries.clientHeight * 2), 'the transcript is not longer than the window')
  assert.ok(await page.locator('.entries').evaluate((entries) => entries.scrollTop > 0), 'the chat did not open at its end')
  assert.ok(await inView(), 'the notice is out of view when a long chat opens')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, `open-loops-long-${name}.png`) })
  }
  console.log('PASS: a reopened long chat that left loops open keeps the sentence in view')

  // 2. With every kind a resume can know open, at the minimum window size, the sentence wraps and
  // the icon sits on its first line.
  await page.setViewportSize({ width: 900, height: 560 })
  await rowOf(EVERY).waitFor()
  await show(EVERY)
  await notice.waitFor()
  assert.equal((await notice.innerText()).trim(), EVERY_LINE)
  const placed = await notice.evaluate((element) => {
    const icon = element.querySelector('leo-icon').getBoundingClientRect()
    const text = element.querySelector('.notice-text')
    const box = text.getBoundingClientRect()
    const line = parseFloat(getComputedStyle(text).lineHeight)
    return { lines: Math.round(box.height / line), iconMiddle: icon.top + icon.height / 2, firstMiddle: box.top + line / 2 }
  })
  assert.ok(placed.lines >= 2, `the sentence did not wrap at the minimum size: ${JSON.stringify(placed)}`)
  assert.ok(Math.abs(placed.iconMiddle - placed.firstMiddle) <= 1, `the icon is not on the first line: ${JSON.stringify(placed)}`)
  for (const name of ['light', 'dark']) {
    await theme(name)
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, `open-loops-every-kind-${name}.png`) })
  }
  await page.setViewportSize({ width: 1350, height: 900 })
  console.log('PASS: every kind of loop a resume knows is named, and the wrapped sentence keeps its icon on the first line')

  // 3. A chat that left nothing open shows no notice.
  await theme('light')
  await show(NOTHING_OPEN)
  await page.getByText('done', { exact: true }).first().waitFor()
  assert.equal(await notice.count(), 0, 'a chat with nothing open showed an open-loops notice')
  console.log('PASS: a reopened chat with nothing open shows no notice')

  // 4. Nor does a new chat.
  await page.getByRole('button', { name: 'New chat', exact: true }).click()
  await rowOf('New Chat').and(page.locator('.current')).waitFor()
  assert.equal(await notice.count(), 0, 'a new chat showed an open-loops notice')
  console.log('PASS: a new chat shows no notice')

  assert.deepEqual(launched.errors, [])
  console.log(`PASS: open loops on reopen against the real bridge; screenshots in ${shots}`)
} catch (error) {
  if (page) { await page.screenshot({ path: join(shots, 'open-loops-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  service.closeAllConnections()
  await new Promise((done) => service.close(done))
  rmSync(world.root, { recursive: true, force: true })
}
