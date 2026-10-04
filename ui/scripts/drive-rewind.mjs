// Undo and rewind through real Electron IPC, the Rust bridge and files on disk.
// Only the model server and the OS folder picker are substituted, as in drive-manual-walkthrough.
//
// Each entry point is walked once: the Undo turn button on the latest footer, Rewind to Before
// This on a prompt's right-click, and Chat > Undo Last Turn. Every one has to put the file
// back, take the turn out of the transcript and leave its prompt in the composer.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { _electron as electron } from 'playwright-core'

const shots = process.env.REWIND_SHOTS ?? join(tmpdir(), 'bravebot-rewind')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-rewind-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
const notes = join(project, 'notes.txt')
const original = 'The release colour is blue.\n'
writeFileSync(notes, original)
const token = 'rewind-only-fake-token'
const failures = [], steps = []
let app, page, toolId = 0
const content = text => ({ content: text })
const run = command => ({ tool_calls: [{ index: 0, id: `call_${++toolId}`, type: 'function', function: { name: 'run', arguments: JSON.stringify({ command }) } }] })
const write = contents => ({ tool_calls: [{ index: 0, id: `call_${++toolId}`, type: 'function', function: { name: 'write_file', arguments: JSON.stringify({ path: 'notes.txt', contents }) } }] })
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    const next = steps.shift()
    assert(next, `Unexpected model request: ${raw.slice(-400)}`)
    const delta = next()
    res.writeHead(200, { 'Content-Type': 'text/event-stream' })
    if (delta === null) { res.flushHeaders(); return }
    res.end(`data: ${JSON.stringify({ model: 'test', choices: [{ index: 0, delta, finish_reason: delta.tool_calls ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 12, completion_tokens: 9, total_tokens: 21 } })}\n\ndata: [DONE]\n\n`)
  } catch (error) { failures.push(String(error)); res.writeHead(500); res.end('Fixture assertion failed') }
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
writeFileSync(join(home, '.bravebot/settings.json'), JSON.stringify({ provider: { local: { options: { baseURL: `http://127.0.0.1:${server.address().port}/v1`, apiKey: token }, models: { test: {} } } }, model: 'local/test' }))
const env = Object.fromEntries(['PATH', 'DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'LANG'].filter(k => process.env[k]).map(k => [k, process.env[k]]))
Object.assign(env, { HOME: home, XDG_CONFIG_HOME: join(home, '.config'), NO_PROXY: '127.0.0.1,localhost' })
async function until(predicate, label) {
  const deadline = Date.now() + 20000
  while (!await predicate()) { assert(Date.now() < deadline, `${label} timed out; fixture errors: ${failures}`); await delay(50) }
}

try {
  app = await electron.launch({ args: ['.', ...(process.env.CI ? ['--no-sandbox'] : []), ...(process.platform === 'linux' ? ['--ozone-platform=x11', '--disable-gpu'] : []), `--user-data-dir=${profile}`], env, timeout: 40000 })
  page = await app.firstWindow(); page.setDefaultTimeout(15000)
  await page.setViewportSize({ width: 1350, height: 900 })
  const uiErrors = []; page.on('pageerror', error => uiErrors.push(error.message))
  await app.evaluate(({ dialog, Menu }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] })
    // A native context menu is modal. Recorded, and the item named in `__choose` is clicked.
    globalThis.__pops = []
    globalThis.__choose = null
    Menu.prototype.popup = function popup() {
      globalThis.__pops.push(this.items.map(i => ({ id: i.id, label: i.label, enabled: i.enabled })))
      const chosen = this.items.find(i => i.id === globalThis.__choose)
      globalThis.__choose = null
      chosen?.click()
    }
  }, project)
  await page.evaluate(() => {
    window.rewindEvents = []
    window.bravebot.onEvent(event => window.rewindEvents.push(event))
  })
  const ended = async () => (await page.evaluate(() => window.rewindEvents)).filter(e => e.event === 'turn.done' || e.event === 'turn.error')
  const composer = page.getByRole('textbox', { name: 'Message the agent' })
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled, accelerator: item.accelerator ?? null }
  }, id)
  const dialog = page.getByRole('dialog', { name: 'Undo turn', exact: true })
  const prompt = 'Change blue to green in notes.txt.'

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  await page.getByRole('button', { name: 'Trust this directory', exact: true }).click()
  await page.getByRole('dialog', { name: 'Project trust', exact: true }).waitFor({ state: 'hidden' })

  /** One turn that writes `colour` into notes.txt, after `first` if given, approving whatever it asks. */
  const writeTurn = async (colour, first = []) => {
    steps.push(...first, () => write(`The release colour is ${colour}.\n`), () => content(`Made it ${colour}.`))
    const before = (await ended()).length
    await composer.fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    const approve = page.locator('.confirm .approve:not([data-clicked])').filter({ visible: true })
    await until(async () => {
      if (await approve.count() > 0) await approve.first().evaluate(el => { el.dataset.clicked = '1'; el.click() }).catch(() => {})
      return (await ended()).length > before
    }, 'turn end')
    const end = (await ended()).at(-1)
    assert.equal(end.event, 'turn.done', JSON.stringify(end))
    assert.deepEqual(failures, []); assert.equal(steps.length, 0, 'every scripted reply consumed')
    assert.equal(readFileSync(notes, 'utf8'), `The release colour is ${colour}.\n`)
    assert.deepEqual(end.data.rewind?.[0]?.paths, ['notes.txt'], JSON.stringify(end.data.rewind))
    await page.getByText(`Made it ${colour}.`, { exact: true }).waitFor()
    return end.data.turn
  }
  /** What a rewind has to leave behind, whichever entry point asked for it. */
  const rewound = async (colour, turn) => {
    await dialog.waitFor({ state: 'hidden' })
    await page.getByText(`Back to before turn ${turn}`, { exact: true }).waitFor()
    assert.equal(readFileSync(notes, 'utf8'), original, 'the file is back on disk')
    assert.equal(await page.getByText(`Made it ${colour}.`, { exact: true }).count(), 0, 'the reply is gone')
    assert.equal(await page.locator('.bubble.user').filter({ hasText: prompt }).count(), 0, 'the prompt is gone from the transcript')
    assert.equal(await composer.inputValue(), prompt, 'the prompt is back in the composer')
    assert.equal(await page.locator('[data-test="turn-undo"]').count(), 0, 'nothing left to undo')
    assert.equal((await menuItem('turn.rewind')).enabled, false, 'the menu item greys with no points')
  }

  // Before any turn there is nothing to undo, and Cmd+Z is left to the text being edited.
  const item = await menuItem('turn.rewind')
  assert.equal(item.label, 'Undo Last Turn…')
  assert.equal(item.accelerator, null)
  assert.equal(item.enabled, false)
  console.log('PASS: Undo Last Turn has no shortcut and is greyed with nothing to undo')

  // 1. The footer.
  let turn = await writeTurn('green')
  const undo = page.locator('[data-test="turn-undo"]')
  assert.equal(await undo.count(), 1, 'one Undo turn, on the latest footer')
  assert.equal((await menuItem('turn.rewind')).enabled, true)
  await undo.click()
  await dialog.waitFor()
  await dialog.getByText('notes.txt', { exact: true }).waitFor()
  await page.screenshot({ path: join(shots, 'rewind-confirm.png') })
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.match(readFileSync(notes, 'utf8'), /green/, 'Cancel changes nothing')
  await undo.click()
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  await rewound('green', turn)
  await page.screenshot({ path: join(shots, 'rewind-after.png') })
  console.log('PASS: Undo turn on the footer confirms, restores the file, redraws and refills the composer')

  // 2. The prompt's right-click.
  turn = await writeTurn('red')
  const bubble = page.locator('.bubble.user').filter({ hasText: prompt }).last()
  await app.evaluate(() => { globalThis.__choose = 'context.entry.rewind' })
  await bubble.click({ button: 'right' })
  const offered = await app.evaluate(() => globalThis.__pops.at(-1))
  assert.deepEqual(offered.map(i => i.label), ['Copy', 'Fork From Here…', 'Rewind to Before This…'])
  await dialog.waitFor()
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  await rewound('red', turn)
  console.log('PASS: Rewind to Before This on a prompt rewinds to before its turn')

  // 3. Chat > Undo Last Turn, and every entry point greyed while a turn runs.
  turn = await writeTurn('yellow')
  steps.push(() => null)
  const before = (await ended()).length
  await composer.fill('Wait while the entry points are checked.')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => steps.length === 0, 'held model request')
  assert.equal(await undo.locator('button').isDisabled(), true, 'Undo turn greys while a turn runs')
  assert.equal((await menuItem('turn.rewind')).enabled, false, 'the menu item greys while a turn runs')
  await page.locator('.bubble.user').filter({ hasText: prompt }).last().click({ button: 'right' })
  const running = (await app.evaluate(() => globalThis.__pops.at(-1))).find(i => i.label === 'Rewind to Before This…')
  assert.equal(running.enabled, false, 'the right-click item greys while a turn runs')
  await page.getByRole('button', { name: 'Stop', exact: true }).click()
  await until(async () => (await ended()).length > before, 'cancelled turn')
  const cancelled = (await ended()).at(-1)
  const steps2 = cancelled.data.rewind.find(point => point.turn === turn)?.steps
  assert.equal(steps2, 2, JSON.stringify(cancelled.data.rewind))
  await composer.fill('')
  // The cancelled turn wrote nothing; going back past it to before the write takes two steps.
  await app.evaluate(() => { globalThis.__choose = 'context.entry.rewind' })
  await page.locator('.bubble.user').filter({ hasText: prompt }).last().click({ button: 'right' })
  const two = page.getByRole('dialog', { name: 'Rewind 2 turns', exact: true })
  await two.waitFor()
  await two.getByText('notes.txt', { exact: true }).waitFor()
  await two.getByRole('button', { name: 'Cancel', exact: true }).click()
  await two.waitFor({ state: 'hidden' })
  await app.evaluate(({ Menu }) => Menu.getApplicationMenu().getMenuItemById('turn.rewind').click())
  await dialog.waitFor()
  await dialog.getByText('No files were written, so only the conversation goes back.', { exact: true }).waitFor()
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(await composer.inputValue(), 'Wait while the entry points are checked.')
  await composer.fill('')
  await app.evaluate(({ Menu }) => Menu.getApplicationMenu().getMenuItemById('turn.rewind').click())
  await dialog.waitFor()
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  await rewound('yellow', turn)
  console.log('PASS: Chat > Undo Last Turn, and every entry point greyed while a turn runs')

  // 4. A turn that ran a command is undone with a warning that the command's effects stay.
  turn = await writeTurn('purple', [() => run('ls')])
  await undo.click()
  await dialog.waitFor()
  await dialog.getByText('Commands that ran may have changed files that won’t be restored.', { exact: true }).waitFor()
  await page.screenshot({ path: join(shots, 'rewind-confirm-warnings.png') })
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.screenshot({ path: join(shots, 'rewind-confirm-warnings-dark.png') })
  await page.emulateMedia({ colorScheme: 'light' })
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  await rewound('purple', turn)
  console.log('PASS: a command in the turn is named as a coverage gap before the rewind')

  assert.deepEqual(uiErrors, []); assert.deepEqual(failures, [])
  console.log(`PASS: undo and rewind against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'rewind-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText()).slice(-5000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
