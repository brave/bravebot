// That the sandbox mode chosen in the composer governs the session's next turn: a session opens
// in standard, the menu offers strict and standard and no way to turn the sandbox off, a program
// the agent runs is held to the mode chosen, and the choice is the session's own.
//
// The real app and the real bridge, against a model service this script serves itself, in a
// directory the person did not trust. Nothing is paid for. Whether a program was held to its
// profile is read off the disk and off what the tool handed back to the model.
//
// Needs `bravebot-rpc` built (`npm run bridge`) and the app built (`electron-vite build`), which
// `npm run drive:sandbox-mode` does first.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, rmSync, realpathSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-sandbox-')))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
// Both outside the project: standard reads the first and refuses a write to the second, strict
// refuses both. Both are under the real home, because a session may use the temporary directory,
// so a file there would prove nothing. They are removed at the end.
const outside = join(homedir(), `.bravebot-sandbox-drive-${process.pid}-outside`)
const planted = join(homedir(), `.bravebot-sandbox-drive-${process.pid}-planted`)
writeFileSync(outside, 'outside the project\n')

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
  const command = answered ? null
    : asked.includes('Plant') ? `/bin/sh -c '/usr/bin/touch ${planted} > /dev/null'`
      : asked.includes('Read') ? `/bin/sh -c '/bin/cat ${outside} > /dev/null'`
        : null
  const call = command ? { name: 'run', arguments: JSON.stringify({ command }) } : null
  const delta = call
    ? { role: 'assistant', tool_calls: [{ index: 0, id: `call-${rounds.length}`, type: 'function', function: call }] }
    : { role: 'assistant', content: 'done' }
  const chunk = { id: 'c1', object: 'chat.completion.chunk', model: 'test', choices: [{ index: 0, delta, finish_reason: call ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 1, total_tokens: 11 } }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' })
  response.end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
})
await listening(service)

const lastToolResult = (round) => (JSON.parse(round).messages ?? []).filter((message) => message.role === 'tool').at(-1)?.content

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
  const sandbox = page.locator('[data-test="sandbox-trigger"]')
  // The Leo host has no box of its own, so the mode is read off its attribute, not its visibility.
  const inMode = (wanted) => page.waitForFunction((wanted) =>
    document.querySelector('[data-test="sandbox-trigger"]')?.getAttribute('data-mode') === wanted, wanted)
  // Ask for a program to be run, approve it once, and return what the tool handed the model.
  const run = async (prompt) => {
    const before = await replies.count()
    await composer.fill(prompt)
    await page.locator('.composer .send').click()
    const card = page.locator('.confirm.run:not([data-folded="true"])')
    await card.waitFor()
    await card.locator('.confirm-actions .approve:not(.always)').click()
    await replies.nth(before).waitFor()
    await stop.waitFor({ state: 'hidden' })
    return lastToolResult(rounds.at(-1))
  }

  assert.equal(await sandbox.getAttribute('data-mode'), 'standard', 'a session opens in standard')
  assert.equal(await page.locator('[data-test="mode-trigger"]').getAttribute('data-mode'), 'ask', 'the permission mode is a control of its own')
  await page.screenshot({ path: join(output, 'sandbox-standard.png') })

  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).waitFor()
  await page.waitForTimeout(300)
  const offered = await page.locator('[data-test^="sandbox-option-"]').evaluateAll((items) => items.map((item) => item.getAttribute('data-test')))
  assert.deepEqual(offered, ['sandbox-option-standard', 'sandbox-option-strict'], 'the menu offers strict and standard, and no way to turn the sandbox off')
  assert.equal(await page.getByRole('menuitemradio', { name: /^Standard/ }).getAttribute('aria-checked'), 'true', 'the mode in force is checked')
  await page.screenshot({ path: join(output, 'sandbox-menu.png') })
  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).waitFor({ state: 'hidden' })

  // The control only means something where this platform confines a program at all.
  const control = await run('Plant a file')
  const confines = !existsSync(planted)
  if (!confines) {
    console.log(`  --   this platform did not confine the control program (${String(control).slice(0, 60)}), so what the modes hold a program to is not checked here`)
  } else {
    assert.match(String(control), /exited 1/, 'standard refused a write outside the session')
    const readable = await run('Read a file')
    assert.match(String(readable), /It exited 0\./, 'standard reads a file outside the session')

    await sandbox.click()
    await page.getByRole('menuitemradio', { name: /^Strict/ }).click()
    await inMode('strict')
    await page.screenshot({ path: join(output, 'sandbox-strict.png') })
    const refused = await run('Read a file')
    assert.match(String(refused), /exited 1/, 'strict refused a read outside the session')
    assert.match(String(refused), /Confinement:/, 'and the planner was told what it ran under')
    assert.equal(await sandbox.getAttribute('data-mode'), 'strict', 'the choice holds for the next turn')

    await sandbox.click()
    await page.getByRole('menuitemradio', { name: /^Standard/ }).click()
    await inMode('standard')
    const again = await run('Read a file')
    assert.match(String(again), /It exited 0\./, 'standard reads it again from the turn after the change')
  }

  await page.emulateMedia({ colorScheme: 'dark' })
  await sandbox.click()
  await page.getByRole('menuitemradio', { name: /^Strict/ }).click()
  await inMode('strict')
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(output, 'sandbox-strict-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })

  assert.deepEqual(errors, [])
  console.log(`PASS: a session opens in standard, the menu offers strict and standard and no way to turn the sandbox off, and a program is held to the mode chosen for its turn. Screenshots in ${output}/sandbox-*.png`)
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'sandbox-failure.png') }).catch(() => undefined)
  throw error
} finally {
  await app.close()
  await new Promise((resolve) => service.close(resolve))
  rmSync(planted, { force: true })
  rmSync(outside, { force: true })
  rmSync(root, { recursive: true, force: true })
}
