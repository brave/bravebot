// That the permission mode chosen in the composer governs the next turn: accepting edits writes a
// file with no card and still puts a command to the window, planning writes nothing and asks
// nothing about a write, the control stays usable while a turn runs, and the Session menu's
// shortcut walks the modes.
//
// The real app and the real bridge, against a model service this script serves itself, in a
// directory the person did not trust, so every write would be asked about. Nothing is paid for.
// Whether a write landed is read off the disk.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`), which
// `npm run drive:permission-mode` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const listening = (server) => new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))

// A planner that makes the one call the last prompt names, and ends the turn once a tool answered.
const rounds = []
const service = createServer(async (request, response) => {
  let body = ''
  for await (const chunk of request) body += chunk
  if (request.method !== 'POST') {
    response.writeHead(200, { 'Content-Type': 'application/json' })
    return response.end(JSON.stringify({ data: [{ id: 'test' }] }))
  }
  rounds.push(body)
  const messages = JSON.parse(body).messages ?? []
  const answered = messages.at(-1)?.role === 'tool'
  const asked = JSON.stringify(messages.filter((message) => message.role === 'user').at(-1) ?? '')
  const file = asked.match(/Write ([a-z]+\.md)/)?.[1]
  const call = answered ? null
    : file ? { name: 'write_file', arguments: JSON.stringify({ path: file, contents: '# Notes\n' }) }
      : asked.includes('Run') ? { name: 'run', arguments: JSON.stringify({ command: '/usr/bin/touch ran.txt' }) }
        : null
  const delta = call
    ? { role: 'assistant', tool_calls: [{ index: 0, id: `call-${rounds.length}`, type: 'function', function: call }] }
    : { role: 'assistant', content: 'done' }
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: call ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await listening(service)

const toolResults = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').map((message) => message.content).join('\n')

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-mode-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({
  provider: { local: { options: { baseURL: `http://127.0.0.1:${service.address().port}/v1` }, models: { test: {} } } },
  model: 'local/test',
}))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter((key) => process.env[key]).map((key) => [key, process.env[key]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost', BRAVEBOT_LOCALE: 'en-US' })

const output = '/tmp/bravebot-ui'
mkdirSync(output, { recursive: true })
const app = await electron.launch({
  args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`],
  env,
  timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(30000)
  await page.setViewportSize({ width: 1400, height: 1000 })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] })
  }, project)
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.getByRole('button', { name: "Don't trust", exact: true }).click()
  await trust.waitFor({ state: 'hidden' })

  const composer = page.locator('.composer textarea')
  const stop = page.locator('.composer .stop')
  const replies = page.locator('.bubble.assistant')
  const mode = page.locator('[data-test="mode-trigger"]')
  const writeCards = page.locator('.confirm:not(.run):not(.ask)')
  const send = async (prompt) => {
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
  }
  // A turn the fixture answers at once can show and hide Stop between two polls, so a turn is
  // waited out by its reply.
  const ask = async (prompt) => {
    const before = await replies.count()
    await send(prompt)
    await replies.nth(before).waitFor()
    await stop.waitFor({ state: 'hidden' })
  }
  // The Leo host has no box of its own, so the mode is read off its attribute, not its visibility.
  const inMode = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="mode-trigger"]')?.getAttribute('data-mode') === wanted, wanted)
  const cycle = () => app.evaluate(({ Menu }) => Menu.getApplicationMenu().getMenuItemById('mode.cycle').click())

  assert.equal(await mode.getAttribute('data-mode'), 'ask', 'a session opens asking')
  await page.screenshot({ path: join(output, 'mode-ask.png') })
  await mode.click()
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).waitFor()
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'mode-menu.png') })
  await page.getByRole('menuitemradio', { name: /^Accept edits/ }).click()
  await inMode('acceptEdits')
  await ask('Write notes.md')
  assert.equal(await writeCards.count(), 0, 'accepting edits put a write to the window')
  assert.equal(readFileSync(join(project, 'notes.md'), 'utf8'), '# Notes\n', 'the write landed')
  await page.screenshot({ path: join(output, 'mode-accept-edits.png') })

  await send('Run a command')
  const run = page.locator('.confirm.run')
  await run.waitFor()
  assert.equal(await mode.locator('button').isDisabled(), false, 'the mode can be changed while a turn runs')
  assert.match(await mode.getAttribute('data-tooltip'), /next turn/, 'and says a change applies from the next turn')
  await page.screenshot({ path: join(output, 'mode-command-asked.png') })
  await run.locator('.reject').click()
  await stop.waitFor({ state: 'hidden' })
  assert.equal(existsSync(join(project, 'ran.txt')), false, 'a refused command ran')

  await cycle()
  await inMode('plan')
  const sentBefore = rounds.length
  await ask('Write plan.md')
  assert.equal(await writeCards.count(), 0, 'planning put a write to the window')
  assert.equal(existsSync(join(project, 'plan.md')), false, 'planning wrote a file')
  assert.ok(rounds[sentBefore].includes('Plan mode.'), 'the planner was told it is planning')
  assert.match(toolResults(rounds.at(-1)), /refused|declined|not/, 'the planner was told the write did not happen')
  await page.screenshot({ path: join(output, 'mode-plan.png') })

  await cycle()
  await inMode('ask')

  assert.deepEqual(errors, [])
  console.log(`PASS: accepting edits writes with no card and still asks about a command, the control works while a turn runs, planning writes nothing and asks nothing, and the menu shortcut walks back to asking. Screenshots in ${output}/mode-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'mode-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
