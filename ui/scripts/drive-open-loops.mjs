// A reopened chat says at the top of its transcript what it left open (OPEN-6), against the real
// agent. Real Electron renderer and real `bravebot-rpc`, on a disposable HOME and profile. Before
// the app starts, the bridge is driven directly to take two turns against a stub model service: one
// that runs a command that fails and then writes a file, and one that only answers. The window
// then reopens each. The first shows the bridge's `open: …` line as a session notice; the second,
// and a new chat, show none. Screenshots go to OPEN_LOOPS_SHOTS, or a temp directory.
//
//   pnpm run build && xvfb-run -a node scripts/drive-open-loops.mjs
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { launchWorld, makeWorld, startModelService } from './drive-world.mjs'

const shots = process.env.OPEN_LOOPS_SHOTS ?? join(tmpdir(), 'bravebot-open-loops')
mkdirSync(shots, { recursive: true })
const LEFT_OPEN = 'Run a check, then write out.txt'
const NOTHING_OPEN = 'Just say hello'
const LINE = 'open: changes not built, the last command failed (planner)'

// The turn that leaves loops open runs `ls no-such-file`, then writes a file, then answers: the
// write comes after the failed command, so the changes are not built and the command is still the
// last one (OPEN-1). Every other request, the injection classifier's included, is answered `done`.
const { service } = await startModelService(({ body, messages }) => {
  if (body.includes('You are a prompt-injection classifier.')) return { content: 'done' }
  // That session has one turn, so every tool result in its request is from that turn.
  if (!messages.some((message) => message.role === 'user' && JSON.stringify(message.content).includes(LEFT_OPEN))) return { content: 'done' }
  const tools = messages.filter((message) => message.role === 'tool').length
  if (tools === 0) return { tool: { name: 'run', arguments: { command: 'ls no-such-file' } } }
  if (tools === 1) return { tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'written by the turn\n' } } }
  return { content: 'done' }
})
const world = makeWorld('bravebot-open-loops-world-', { service })

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

  // 1. The chat that left a failed command and unbuilt changes says so above its transcript.
  await rowOf(LEFT_OPEN).waitFor()
  await show(LEFT_OPEN)
  await notice.waitFor()
  assert.equal((await notice.innerText()).trim(), LINE)
  assert.ok(!(await notice.innerText()).includes('out.txt'), 'the line names no path')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, `open-loops-${name}.png`) })
  }
  console.log('PASS: a reopened chat that left loops open shows the line as a session notice')

  // 2. A chat that left nothing open shows no notice.
  await theme('light')
  await show(NOTHING_OPEN)
  await page.getByText('done', { exact: true }).first().waitFor()
  assert.equal(await notice.count(), 0, 'a chat with nothing open showed an open-loops notice')
  console.log('PASS: a reopened chat with nothing open shows no notice')

  // 3. Nor does a new chat.
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
