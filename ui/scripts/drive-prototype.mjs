// The desktop as a client of `bravebot-host` (HOSTCLIENT-1), through the real host, the real app and
// a model service this script serves itself. Nothing is paid for and nothing leaves the machine.
//
// The host starts two clients: the desktop, as the person at the computer, and the host test client
// standing in for a phone, which may answer no trust. The script types into the second client
// through a named pipe and reads what it says from the host's standard error, and drives the
// desktop's window through the debugging port the host passes it. Each thing the plan's done-when
// names is read off the other client's side, so the window is not its own witness:
//
//   - both clients see the same session, and each view names the client that acted last;
//   - a message the other client sent is in the window with no local copy of it;
//   - each can send, answer a question and cancel;
//   - a send during a turn is refused as busy and the draft stays in the window;
//   - closing the window does not close the session or stop its turn.
//
// Needs `bravebot-host` built (`pnpm run bridge`), the app built (`electron-vite build`), and the
// shared client's scripts built (`pnpm --dir ../packages/agent-client run build`).
import assert from 'node:assert/strict'
import { execFileSync, spawn } from 'node:child_process'
import { closeSync, constants, existsSync, mkdirSync, openSync, readFileSync, realpathSync, writeFileSync, writeSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { makeWorld, startModelService } from './drive-world.mjs'
import { SHOTS_DIR } from './drive-support.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const ui = join(here, '..')
const electronPath = createRequire(import.meta.url)('electron')
const hostBinary = process.env.BRAVEBOT_HOST ?? join(ui, '../target/debug/bravebot-host')
const phoneScript = join(ui, '../packages/agent-client/dist/scripts/host-client.js')
for (const [what, path] of [['bravebot-host', hostBinary], ['the host test client', phoneScript], ['the app', join(ui, 'out/main/index.js')]]) {
  assert.ok(existsSync(path), `${what} is not built at ${path}`)
}

const within = (promise, what, ms = 20_000) =>
  Promise.race([promise, new Promise((_, reject) => setTimeout(() => reject(new Error(`timed out after ${ms}ms waiting for ${what}`)), ms))])
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

// The model service. A prompt names its plan. `plan:write` asks to write a file, and the plans that
// hold wait at a gate the script opens, so a turn is running for as long as the script says.
const gates = new Map()
const gate = (name) => {
  if (!gates.has(name)) {
    let release
    let arrive
    gates.set(name, { released: new Promise((resolve) => (release = resolve)), reached: new Promise((resolve) => (arrive = resolve)), release, arrive })
  }
  return gates.get(name)
}
const { service, rounds } = await startModelService(async ({ messages }) => {
  const last = messages.at(-1)
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  if (asked.includes('plan:write')) {
    return last?.role === 'tool' ? { content: 'wrote it' } : { tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'written after approval\n' } } }
  }
  const held = /plan:(hold\d)/.exec(asked)?.[1]
  if (held) {
    gate(held).arrive()
    await gate(held).released
    return { content: `${held} done` }
  }
  return { content: 'done' }
})

const world = makeWorld('bravebot-prototype-', { service })
const project = realpathSync(world.project)
const phoneInput = join(world.root, 'phone.input')
execFileSync('mkfifo', [phoneInput])
const debugPort = 9300 + Math.floor(Math.random() * 500)
const ci = process.env.CI ? ['--no-sandbox'] : []
const sessions = join(world.root, 'sessions')

const host = spawn(hostBinary, [
  '--sessions-root', sessions, '--workspace', project,
  '--client', 'desktop', '--answers', 'confirm,run,fetch,ask,trust', '--program', electronPath,
  '--arg', ui, '--arg', '--bravebot-host-client', '--arg', '--host-client-name', '--arg', 'desktop', '--arg', '--host-client-workspace', '--arg', project,
  '--arg', `--user-data-dir=${world.profile}`, '--arg', `--remote-debugging-port=${debugPort}`, ...ci.flatMap((flag) => ['--arg', flag]),
  '--client', 'phone', '--answers', 'confirm,run,fetch,ask', '--program', process.execPath,
  '--arg', phoneScript, '--arg', '--name', '--arg', 'phone', '--arg', '--workspace', '--arg', project, '--arg', '--input', '--arg', phoneInput,
], { env: world.env, stdio: ['pipe', 'pipe', 'pipe'] })

let said = ''
const waiting = []
host.stderr.setEncoding('utf8')
host.stderr.on('data', (chunk) => {
  said += chunk
  for (const wake of waiting.splice(0)) wake()
})
const hostEnded = new Promise((resolve) => host.once('exit', resolve))

/** What the phone has said, a line each. */
const phoneLines = () => said.split('\n').slice(0, -1).filter((line) => line.startsWith('host-client phone: ')).map((line) => line.slice('host-client phone: '.length))
let phoneSeen = 0
/** The next line the phone says that matches, after the last one asked for. */
const phoneSays = (pattern) =>
  within(new Promise((resolve) => {
    const look = () => {
      const lines = phoneLines()
      for (let at = phoneSeen; at < lines.length; at++) {
        if (pattern.test(lines[at])) {
          phoneSeen = at + 1
          return resolve(lines[at])
        }
      }
      waiting.push(look)
    }
    look()
  }), `the phone to say ${pattern}`)
let fifo
const phoneTypes = (line) => {
  fifo ??= openSync(phoneInput, constants.O_RDWR)
  writeSync(fifo, `${line}\n`)
}
const hostSays = (text) => within(new Promise((resolve) => {
  const look = () => (said.includes(text) ? resolve() : waiting.push(look))
  look()
}), `the host to say ${text}`)

// A no to the question about the folder, so the session asks the desktop about trust.
await within(new Promise((resolve) => {
  const look = () => {
    if (said.includes('trust this directory? [y/N] ')) {
      host.stdin.write('n\n')
      return resolve()
    }
    waiting.push(look)
  }
  look()
}), 'the host to ask about the folder')

let browser
let page
const errors = []
mkdirSync(SHOTS_DIR, { recursive: true })
try {
  const { chromium } = await import('playwright-core')
  const started = Date.now()
  while (!page) {
    if (Date.now() - started > 40_000) throw new Error(`the window never came up. The host said:\n${said}`)
    try {
      browser ??= await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`)
      page = browser.contexts().flatMap((context) => context.pages()).find((candidate) => candidate.url().includes('prototype.html'))
    } catch {
      browser = undefined
    }
    if (!page) await sleep(300)
  }
  page.setDefaultTimeout(20_000)
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('requestfailed', (request) => errors.push(`${request.url()} failed`))
  page.on('console', (message) => message.type() === 'error' && !/Failed to load resource/.test(message.text()) && errors.push(message.text()))
  const text = (selector) => page.locator(selector).first().innerText()
  /** A picture of the window at a step, kept beside the others so a person can see what was checked. */
  const shot = (name) => page.screenshot({ path: `${SHOTS_DIR}/prototype-${name}.png` })

  // 1. The desktop opens a session and answers the question about the folder, which only it may.
  await page.locator('[data-test="prototype"]').waitFor()
  assert.equal(await text('[data-test="prototype-name"]'), 'desktop')
  // It holds no session of its own: what the list shows is what the host holds, which is nothing yet.
  await page.getByText('The host holds no sessions.').waitFor()
  assert.equal(await page.locator('[data-test="prototype-held"]').count(), 0)
  await shot('1-no-sessions')
  await page.locator('[data-test="prototype-new"]').click()
  await page.getByRole('dialog', { name: 'Project trust', exact: true }).waitFor()
  await shot('2-trust-question')
  await page.locator('[data-test="trust-decline"]').click()
  await page.locator('[data-test="prototype-status"]').filter({ hasText: 'Ready' }).waitFor()
  assert.match(await text('[data-test="prototype-actor"]'), /Last action: desktop \(this window\)/)
  const handle = (await text('[data-test="prototype-held"] .prototype-id'))
  assert.match(handle, /^s\d+$/)

  // 2. The phone joins it and sees the same view, naming the desktop. It is not asked about trust.
  phoneTypes(`attach ${handle}`)
  await phoneSays(/^session s\d+ status idle/)
  await phoneSays(/view \d+ turn 0 status idle actor desktop/)
  assert.ok(!phoneLines().some((line) => line.startsWith('trust question')), 'the phone was asked about trust')

  // 3. The phone sends. Its message is in the window from the view alone, the window names the
  // phone, and the desktop answers the write the turn asks about.
  phoneTypes('send plan:write')
  await phoneSays(/^sent turn 1/)
  await page.getByText('plan:write', { exact: false }).first().waitFor()
  await page.locator('[data-test="prototype-actor"]').filter({ hasText: 'Last action: phone' }).waitFor()
  await page.locator('.confirm-actions .approve:not([aria-disabled="true"])').waitFor()
  await shot('3-phone-message-and-write-question')
  await page.locator('.confirm-actions .approve').click()
  await page.locator('[data-test="prototype-status"]').filter({ hasText: 'Done' }).waitFor()
  assert.equal(readFileSync(join(project, 'out.txt'), 'utf8'), 'written after approval\n')
  await phoneSays(/status completed actor desktop/)
  await shot('4-approved-from-the-desktop')

  // 4. A send during a turn is refused as busy, and the draft stays in the window.
  phoneTypes('send plan:hold1')
  await phoneSays(/^sent turn 2/)
  await within(gate('hold1').reached, 'the model to reach the first hold')
  await page.locator('.prototype-input').fill('and this too')
  await page.locator('[data-test="prototype-send"]').click()
  await page.locator('[data-test="prototype-notice"]').filter({ hasText: 'already running' }).waitFor()
  assert.equal(await page.locator('.prototype-input').inputValue(), 'and this too', 'the draft was kept')
  await shot('5-busy-send-keeps-the-draft')
  phoneTypes('send plan:hold1')
  await phoneSays(/failed: turn_in_flight/)
  gate('hold1').release()
  await page.locator('[data-test="prototype-status"]').filter({ hasText: 'Done' }).waitFor()
  assert.equal(rounds.filter((body) => body.includes('and this too')).length, 0, 'the refused send reached the model')

  // 5. The desktop sends the kept draft now, and the phone sees it. The phone cancels the next
  // turn, and the window names the phone and shows the turn cancelled.
  await page.locator('[data-test="prototype-send"]').click()
  await phoneSays(/status completed actor desktop/)
  phoneTypes('send plan:hold2')
  await phoneSays(/^sent turn 4/)
  await within(gate('hold2').reached, 'the model to reach the second hold')
  phoneTypes('cancel')
  await page.locator('[data-test="prototype-status"]').filter({ hasText: 'Cancelled' }).waitFor()
  await page.locator('[data-test="prototype-actor"]').filter({ hasText: 'Last action: phone' }).waitFor()
  await shot('6-cancelled-by-the-phone')
  gate('hold2').release()

  // 6. A message typed and not sent is kept when the view is closed and opened again. The desktop
  // then cancels a turn of its own, and the phone names it.
  await page.locator('.prototype-input').fill('an unsent note')
  await page.locator('[data-test="prototype-leave"]').click()
  await page.locator('[data-test="prototype-empty"]').waitFor()
  await page.locator('[data-test="prototype-held"]').click()
  await page.locator('.prototype-input').waitFor()
  assert.equal(await page.locator('.prototype-input').inputValue(), 'an unsent note', 'the draft was lost with the view')
  await page.locator('.prototype-input').fill('plan:hold3')
  await page.locator('[data-test="prototype-send"]').click()
  await within(gate('hold3').reached, 'the model to reach the third hold')
  await page.locator('[data-test="prototype-cancel"]').click()
  await phoneSays(/status cancelled actor desktop/)
  gate('hold3').release()

  // 7. Closing the window does not close the session or stop its turn.
  phoneTypes('send plan:hold4')
  await phoneSays(/^sent turn 6/)
  await within(gate('hold4').reached, 'the model to reach the fourth hold')
  await shot('7-turn-running-before-the-window-closes')
  await page.close()
  await hostSays('bravebot-host: client desktop ended')
  phoneTypes('list')
  await phoneSays(new RegExp(`^held ${handle} open attached true working true`))
  gate('hold4').release()
  await phoneSays(/status completed actor phone/)
  // What the other client said throughout, kept beside the pictures: it is the witness to what the window showed.
  writeFileSync(`${SHOTS_DIR}/prototype-other-client.txt`, `${phoneLines().join('\n')}\n`)
  assert.equal(host.exitCode, null, 'the host went on')
  assert.deepEqual(errors, [], 'the window raised errors')
  console.log(`The desktop and a second client shared ${handle}: each sent, answered and cancelled, the views named who acted, a busy send kept its draft, and closing the window left the session running.`)
} catch (error) {
  // What the window and the host were showing, which is what a failure here needs to be read by.
  console.error(`The host said:\n${said.split('\n').slice(-40).join('\n')}`)
  console.error(`The state:\n${JSON.stringify(await page?.evaluate(() => window.prototypeHost.state()).catch((failed) => String(failed))).slice(0, 2500)}`)
  await page?.screenshot({ path: `${SHOTS_DIR}/prototype-failure.png` }).catch(() => undefined)
  console.error(`The window raised: ${JSON.stringify(errors)}`)
  console.error(`The window said:\n${(await page?.locator('body').innerText().catch(() => '(no window)'))?.slice(0, 1500)}`)
  throw error
} finally {
  if (fifo !== undefined) closeSync(fifo)
  await browser?.close().catch(() => undefined)
  host.stdin.end('quit\n')
  await Promise.race([hostEnded, sleep(15_000).then(() => host.kill('SIGKILL'))])
  for (const open of gates.values()) open.release()
  service.closeAllConnections?.()
  service.close()
}
