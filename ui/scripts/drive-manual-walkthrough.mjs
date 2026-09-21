// The manual 0.9 walkthrough through real Electron IPC, Rust bridge and files.
// Only the model server and the OS file-picker result are substituted.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { _electron as electron } from 'playwright-core'

const root = mkdtempSync(join(tmpdir(), 'bravebot-walkthrough-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
const original = 'The release colour is blue.\n', changed = 'The release colour is green.\n'
writeFileSync(join(project, 'notes.txt'), original)
writeFileSync(join(project, 'watched.txt'), 'Waiting for changes.\n')
const override = join(root, 'override.json'), malformed = join(root, 'invalid.json')
writeFileSync(override, '{}\n'); writeFileSync(malformed, '{broken')
const hookLog = join(root, 'hook-ran'), hookScript = join(root, 'hook.cjs')
writeFileSync(hookScript, 'require("node:fs").appendFileSync(process.argv[2], "finished\\n")')
const fixtureToken = 'walkthrough-only-fake-token'
const requests = [], failures = [], steps = []
let app, page, toolId = 0
const content = text => ({ content: text })
const tool = (name, args) => ({ tool_calls: [{ index: 0, id: `call_${++toolId}`, type: 'function', function: { name, arguments: JSON.stringify(args) } }] })
const lastTool = body => body.messages.filter(m => m.role === 'tool').at(-1)?.content ?? ''
const reference = body => {
  const ref = lastTool(body).match(/ref:\d+/)?.[0]
  assert(ref, `Expected quarantined reference: ${lastTool(body)}`)
  return ref
}
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    const body = JSON.parse(raw); requests.push(body)
    assert.equal(req.headers.authorization, `Bearer ${fixtureToken}`)
    const next = steps.shift()
    assert(next, `Unexpected model request: ${JSON.stringify(body.messages.slice(-2))}`)
    const delta = await next(body)
    if (delta === null) { res.writeHead(200, { 'Content-Type': 'text/event-stream' }); res.flushHeaders(); return }
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta, finish_reason: delta.tool_calls ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
  } catch (error) { failures.push(String(error)); res.writeHead(500); res.end('Fixture assertion failed') }
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
function writeFixtureSettings(model) {
  writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ provider: { local: { options: { baseURL: `http://127.0.0.1:${server.address().port}/v1`, apiKey: fixtureToken }, models: { test: {} } } }, model }))
}
writeFixtureSettings('local/test')
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter(k => process.env[k]).map(k => [k, process.env[k]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost' })
async function until(predicate, label) {
  const deadline = Date.now() + 20000
  while (!await predicate()) { assert(Date.now() < deadline, `${label} timed out; fixture errors: ${failures}`); await delay(50) }
}
try {
  app = await electron.launch({ args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], env, timeout: 40000 })
  page = await app.firstWindow(); page.setDefaultTimeout(15000)
  await page.setViewportSize({ width: 1350, height: 900 })
  const uiErrors = []; page.on('pageerror', error => uiErrors.push(error.message))
  await app.evaluate(({ dialog }, project) => {
    globalThis.walkthroughPicker = { path: project, calls: [] }
    dialog.showOpenDialog = async (_window, options) => {
      globalThis.walkthroughPicker.calls.push(options)
      return { canceled: false, filePaths: [globalThis.walkthroughPicker.path] }
    }
  }, project)
  await page.evaluate(() => {
    window.walkthroughEvents = []
    window.bravebot.onEvent(event => window.walkthroughEvents.push(event))
  })
  const events = () => page.evaluate(() => window.walkthroughEvents)
  const completed = async () => (await events()).filter(e => e.event === 'turn.done' || e.event === 'turn.error').length
  const send = async prompt => {
    const before = await completed()
    await page.getByRole('dialog', { name: 'Project trust', exact: true }).waitFor({ state: 'hidden' })
    await page.getByRole('textbox', { name: 'Message the agent' }).fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    return before
  }
  const done = async before => {
    await until(async () => await completed() > before, 'turn completion')
    assert.deepEqual(failures, []); assert.equal(steps.length, 0, 'all scripted responses consumed')
    const terminal = (await events()).filter(e => e.event === 'turn.done' || e.event === 'turn.error').at(-1)
    assert.equal(terminal.event, 'turn.done', JSON.stringify(terminal))
    return terminal
  }
  const settings = page.getByRole('dialog', { name: 'Agent settings', exact: true })
  const openSettings = () => page.getByRole('button', { name: 'Agent settings', exact: true }).click()
  const closeSettings = async () => { await page.keyboard.press('Escape'); await settings.waitFor({ state: 'hidden' }) }
  const fits = async locator => assert(await locator.evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'content fits its width')
  const tabCycle = async dialog => {
    const buttons = dialog.locator('button:visible:enabled')
    await buttons.last().focus(); await page.keyboard.press('Tab')
    assert(await buttons.first().evaluate(el => el === document.activeElement), 'Tab wraps inside dialog')
    await page.keyboard.press('Shift+Tab')
    assert(await buttons.last().evaluate(el => el === document.activeElement), 'Shift+Tab wraps inside dialog')
  }

  // Sidebar children retain their open width during folding; button margins must fit too.
  const sidebar = page.locator('#sessions-column')
  const settingsButton = sidebar.getByRole('button', { name: 'Agent settings', exact: true })
  await settingsButton.waitFor()
  const divider = page.locator('.gutter').first()
  for (const [key, presses, expectedWidth] of [['Home', 1, 250], ['Shift+ArrowLeft', 2, 200], ['Shift+ArrowRight', 7, 400]]) {
    await divider.focus()
    for (let i = 0; i < presses; i++) await page.keyboard.press(key)
    await until(async () => Math.abs((await sidebar.boundingBox()).width - expectedWidth) < 1, 'sidebar resized')
    for (const tab of ['Sessions', 'Bots']) {
      await sidebar.getByRole('button', { name: tab, exact: true }).click()
      const panel = await sidebar.boundingBox(), button = await settingsButton.boundingBox()
      assert(button.x >= panel.x && button.x + button.width <= panel.x + panel.width,
        `Agent settings stays inside ${tab} sidebar at ${expectedWidth}px: ${JSON.stringify({ panel, button })}`)
      assert(await settingsButton.evaluate(el => el.scrollWidth <= el.clientWidth), 'settings label fits')
    }
  }
  await sidebar.getByRole('button', { name: 'Sessions', exact: true }).click()
  await divider.focus(); await page.keyboard.press('Home')
  console.log('PASS: Agent settings button fits both sidebar tabs at default/minimum/maximum widths')

  // 1–2: diagnostics, keyboard navigation, real override validation and clearing.
  await openSettings()
  await settings.getByText('Model service configured', { exact: true }).waitFor()
  assert.match(await settings.innerText(), /0\.9\.0/)
  await settings.getByRole('button', { name: 'Refresh diagnostics' }).click()
  await settings.getByRole('tab', { name: 'Connection', exact: true }).focus()
  for (const [key, name] of [['ArrowRight', 'Hooks'], ['ArrowRight', 'Run settings'], ['ArrowRight', 'Connection'], ['ArrowLeft', 'Run settings'], ['Home', 'Connection'], ['End', 'Run settings']]) {
    await page.keyboard.press(key)
    const tab = settings.getByRole('tab', { name, exact: true })
    assert.equal(await tab.getAttribute('aria-selected'), 'true')
    assert(await tab.evaluate(el => el === document.activeElement))
  }
  await app.evaluate((_, path) => { globalThis.walkthroughPicker.path = path }, override)
  await settings.getByRole('button', { name: 'Choose settings file…' }).click()
  await settings.getByText('Run override selected. Future turns and model discovery use it.', { exact: true }).waitFor()
  assert((await settings.innerText()).includes(override))
  await app.evaluate((_, path) => { globalThis.walkthroughPicker.path = path }, malformed)
  await settings.getByRole('button', { name: 'Choose settings file…' }).click()
  await settings.getByRole('alert').waitFor()
  assert((await settings.innerText()).includes(override), 'invalid selection preserves previous override')
  await settings.getByRole('button', { name: 'Clear override' }).click()
  await settings.getByText('No override selected', { exact: true }).waitFor()
  await tabCycle(settings)
  await closeSettings()
  assert(await page.getByRole('button', { name: 'Agent settings', exact: true }).evaluate(el => el === document.activeElement))
  console.log('PASS: real diagnostics, override selection/validation/clearing, keyboard tabs and focus')

  // Simulate a gateway settings file without a model selection: it retains the Brave default.
  writeFixtureSettings('automatic-brave-bot')
  await app.evaluate((_, path) => { globalThis.walkthroughPicker.path = path }, project)
  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  await page.getByText('Context not yet measured', { exact: true }).waitFor()
  const failedSetup = await send('Reply with manual check ready. Do not use tools.')
  await until(async () => await completed() > failedSetup, 'unconfigured selected model')
  await page.getByText('Choose a model from your configured service', { exact: true }).waitFor()
  assert.equal(requests.length, 0, 'wrong backend must not receive the gateway token')
  await page.getByRole('button', { name: /^Choose model:/ }).click()
  await page.getByRole('option').filter({ hasText: 'local ·' }).click()
  // Set the default for later fresh-session scenarios; the active session is fixed by the UI selection.
  writeFixtureSettings('local/test')
  steps.push(() => content('manual check ready'))
  await done(await send('Reply with manual check ready. Do not use tools.'))
  await page.getByText('12 context tokens at last request', { exact: false }).waitFor()
  steps.push(() => null)
  const beforeCancel = await send('Wait while I test Stop.')
  await until(() => steps.length === 0, 'held model request')
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await until(async () => await completed() > beforeCancel, 'cancelled turn')
  await page.getByText('Task stopped', { exact: true }).waitFor()
  console.log('PASS: file API key, wrong-backend diagnosis and model-picker recovery, context and cancellation')

  // 4: edit hooks in the UI, execute the saved hook, then remove and prove it stays removed.
  await openSettings(); await settings.getByRole('tab', { name: 'Hooks', exact: true }).click()
  await settings.getByRole('button', { name: 'Add hook', exact: true }).click()
  await settings.getByRole('combobox', { name: /^When/ }).selectOption('turn-finished')
  await settings.getByLabel('Program', { exact: true }).fill(process.execPath)
  for (const [i, arg] of [hookScript, hookLog].entries()) {
    await settings.getByRole('button', { name: 'Add argument', exact: true }).click()
    await settings.getByLabel(`Argument ${i + 1}`, { exact: true }).fill(arg)
  }
  await settings.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await settings.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  assert.deepEqual(JSON.parse(readFileSync(join(home, '.bravebot/hooks.json'), 'utf8')).hooks[0].run, [process.execPath, hookScript, hookLog])
  await closeSettings()
  steps.push(() => content('Hook verification finished.'))
  await done(await send('Run the hook verification.'))
  assert.equal(readFileSync(hookLog, 'utf8'), 'finished\n')
  await openSettings(); await settings.getByRole('tab', { name: 'Hooks', exact: true }).click()
  await settings.getByRole('button', { name: 'Remove hook', exact: true }).click()
  await settings.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await settings.getByText('Hooks saved. They apply when the next turn starts.').waitFor(); await closeSettings()
  steps.push(() => content('Hook removed.'))
  await done(await send('Check the next turn after removing the hook.'))
  assert.equal(readFileSync(hookLog, 'utf8'), 'finished\n')
  console.log('PASS: UI-saved hook executes; removed hook no longer executes')

  // 5: actual poller (not a synthetic watch event), missing file, per-watch stop and stop-all.
  await page.setViewportSize({ width: 560, height: 780 })
  await page.getByRole('button', { name: 'Watches', exact: true }).click()
  const watches = page.getByRole('dialog', { name: 'File watches', exact: true })
  await watches.getByLabel('Project file').fill('missing.txt')
  await watches.getByRole('button', { name: 'Watch file', exact: true }).click()
  await watches.getByRole('alert').waitFor()
  await watches.getByLabel('Project file').fill('watched.txt')
  await watches.getByRole('button', { name: 'Watch file', exact: true }).click()
  await watches.getByRole('button', { name: 'Stop watching watched.txt', exact: true }).waitFor()
  assert.match(await watches.innerText(), /Watching · Expires in/)
  await fits(watches); await tabCycle(watches)
  await page.keyboard.press('Escape')
  const beforeWatch = await completed()
  steps.push(body => { assert(JSON.stringify(body).includes('watched.txt')); assert(!JSON.stringify(body).includes('PRIVATE_WATCH_CONTENT')); return content('File watch observed.') })
  writeFileSync(join(project, 'watched.txt'), 'PRIVATE_WATCH_CONTENT changed')
  await done(beforeWatch)
  await page.getByText(/File watch \d+: watched.txt/).waitFor()
  await page.getByRole('button', { name: 'Watches', exact: true }).click()
  await watches.getByRole('button', { name: 'Stop watching watched.txt', exact: true }).click()
  await watches.getByText('No files watched.', { exact: false }).waitFor()
  const requestCount = requests.length
  writeFileSync(join(project, 'watched.txt'), 'Another PRIVATE_WATCH_CONTENT change')
  await delay(6500)
  assert.equal(requests.length, requestCount, 'stopped watch cannot start a turn')
  for (const path of ['notes.txt', 'watched.txt']) {
    await watches.getByLabel('Project file').fill(path)
    await watches.getByRole('button', { name: 'Watch file', exact: true }).click()
    await watches.getByRole('button', { name: `Stop watching ${path}`, exact: true }).waitFor()
  }
  await watches.getByRole('button', { name: 'Stop all watches', exact: true }).click()
  await watches.getByText('No files watched.', { exact: false }).waitFor()
  await page.keyboard.press('Escape'); await page.setViewportSize({ width: 1350, height: 900 })
  console.log('PASS: real file watch, missing-file error, narrow dialog, stop and stop-all')

  // 6: real quarantined read -> checker -> decision -> planner, across fresh sessions.
  const checker = () => content('{"verdict":"safe","reason":"Plain release notes."}')
  const leaveConfined = async () => { await page.locator('.confirm.vouch').getByRole('button', { name: 'Leave it confined', exact: true }).click() }
  const vetSteps = accepted => [
    () => tool('read_file', { path: 'notes.txt' }),
    checker,
    body => { assert(!lastTool(body).includes(original.trim())); return tool('vet_content', { ref: reference(body), expects: 'Release notes, to identify the release colour.' }) },
    body => { assert(JSON.stringify(body).includes(original.trim()), 'checker receives content'); return content('{"verdict":"safe","reason":"Plain release notes."}') },
    body => { assert.equal(lastTool(body).includes(original.trim()), accepted, 'only an approved read reaches planner'); return content(accepted ? 'The release colour is blue.' : 'I respected the refusal.') },
  ]
  for (const [caseIndex, accepted] of [false, true, false].entries()) {
    await page.getByRole('button', { name: /New session$/, exact: false }).click()
    await page.getByRole('button', { name: "Don't trust", exact: true }).click()
    steps.push(...vetSteps(accepted))
    const before = await send(`Read notes.txt using vet_content. Verification ${caseIndex}: ${accepted ? 'approve' : 'reject'}.`)
    await leaveConfined()
    const card = page.locator('.vetted-read')
    await card.getByText('No instructions detected', { exact: true }).waitFor()
    assert((await card.innerText()).includes(original.trim()))
    assert((await card.innerText()).includes('Release notes, to identify the release colour.'))
    assert((await card.innerText()).includes('Plain release notes.'))
    await page.setViewportSize({ width: 560, height: 780 }); await fits(card)
    const action = card.getByRole('button', { name: accepted ? 'Let the planner read once' : 'Keep it out', exact: true })
    await action.scrollIntoViewIfNeeded(); await action.click()
    const terminal = await done(before)
    assert.deepEqual(terminal.data.trust.rules, [], 'one-time approval does not create standing trust')
    await page.setViewportSize({ width: 1350, height: 900 })
  }
  console.log('PASS: vetted rejection/approval, no standing trust in fresh sessions, narrow approval actions')

  // Pending approvals stay with their session, and a real Stop invalidates the request.
  steps.push(...vetSteps(true).slice(0, 4))
  const beforePending = await send('Pending approval routing check for notes.txt.')
  await leaveConfined()
  await page.locator('.vetted-read').getByRole('button', { name: 'Let the planner read once', exact: true }).waitFor()
  const pendingSession = (await events()).filter(e => e.event === 'vet.request').at(-1).session
  await page.locator('.session').filter({ hasText: 'Reply with manual check ready.' }).click()
  assert.equal(await page.locator('.vetted-read').count(), 0, 'another session must not show pending evidence')
  // The active conversation was the last fresh session; find its sidebar row by its handle's persisted id.
  const sessionRows = await page.evaluate(async () => (await window.bravebot.request('session.list')).ok.sessions)
  const pendingDone = (await events()).filter(e => e.event === 'turn.done' && e.session === pendingSession).at(-1)
  const pendingRow = sessionRows.find(row => row.id === pendingDone.data.id)
  assert(pendingRow, 'originating session is persisted')
  await page.locator('.session').filter({ hasText: pendingRow.title }).last().click()
  await page.locator('.vetted-read').getByRole('button', { name: 'Let the planner read once', exact: true }).waitFor()
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await until(async () => await completed() > beforePending, 'pending approval cancellation')
  await until(async () => await page.getByRole('button', { name: 'Let the planner read once', exact: true }).count() === 0, 'approval invalidated')
  assert.deepEqual(failures, [])
  console.log('PASS: real background approval isolation and cancellation')

  // 7: real isolated processor -> write review -> decision, with filesystem assertions.
  for (const accepted of [false, true]) {
    steps.push(
      () => tool('read_file', { path: 'notes.txt' }),
      checker,
      body => tool('spawn_processor', { reads: [reference(body)], instruction: 'Change blue to green; return the complete document.' }),
      body => { assert(JSON.stringify(body).includes(original.trim())); return content(`Changed blue to green.\n===== the document starts here =====\n${changed}`) },
      body => tool('write_file', { path: 'notes.txt', contents_ref: reference(body) }),
      () => content(accepted ? 'Approved write finished.' : 'Rejected write finished.'),
    )
    const before = await send('Use an isolated processor to change blue to green in notes.txt and propose the write.')
    await leaveConfined()
    const card = page.locator('.confirm.untrusted').last()
    await card.getByText('Processor’s remark · untrusted', { exact: true }).waitFor()
    await card.getByText('Changed blue to green.', { exact: true }).waitFor()
    assert.equal(readFileSync(join(project, 'notes.txt'), 'utf8'), original, 'no write before approval')
    await page.setViewportSize({ width: 560, height: 780 }); await fits(card)
    await card.getByRole('button', { name: accepted ? 'Apply this change' : 'Don’t write', exact: true }).click()
    await done(before)
    assert.equal(readFileSync(join(project, 'notes.txt'), 'utf8'), accepted ? changed : original)
    await page.setViewportSize({ width: 1350, height: 900 })
  }
  console.log('PASS: processor remarks and real rejected/approved file writes')

  // 8: settings narrow layout and cleanup assertions.
  await page.setViewportSize({ width: 560, height: 780 })
  if (await page.locator('.app.left-folded').count()) await page.getByRole('button', { name: 'Session list', exact: true }).click()
  await openSettings(); await settings.getByText('Model service configured', { exact: true }).waitFor()
  await fits(settings); await tabCycle(settings); await closeSettings()
  assert.deepEqual(JSON.parse(readFileSync(join(home, '.bravebot/hooks.json'), 'utf8')).hooks, [])
  assert(existsSync(hookLog)); assert.deepEqual(uiErrors, []); assert.deepEqual(failures, [])
  const picks = await app.evaluate(() => globalThis.walkthroughPicker.calls)
  assert(picks.some(p => p.title === 'Choose run settings' && p.properties.includes('openFile')))
  assert(picks.some(p => p.properties.includes('openDirectory')))
  console.log('PASS: manual walkthrough against real application/backend; no paid model requests')
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(tmpdir(), 'bravebot-walkthrough-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText()).slice(-7000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
