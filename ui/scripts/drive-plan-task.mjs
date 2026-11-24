// Chat > Plan a Task… through the real menu, Electron IPC and the Rust bridge (MODE-12). Only the
// model server and the folder picker are substituted. It checks that the item sits under Cycle
// Permission Mode and is offered mid-turn, that its subtitle says it waits while a turn runs, that a
// mode the bridge did not set is an error entry and sends nothing, that a /plan line typed mid-turn
// waits in the Queued tray with the chip on Ask, that confirming with no task only switches the
// mode to Plan, that confirming with a task starts a plan-mode turn sent the task alone, and that a
// plan asked for mid-turn waits in the queue until that turn ends.
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { inheritedEnv, launchWorld, listening } from './drive-world.mjs'
import { COMPOSER_NAME, approveTrust } from './drive-support.mjs'

const shots = process.env.PLAN_SHOTS ?? join(tmpdir(), 'bravebot-plan-task')
mkdirSync(shots, { recursive: true })
const root = mkdtempSync(join(tmpdir(), 'bravebot-plan-task-'))
const home = join(root, 'home'), project = join(root, 'project'), profile = join(root, 'profile')
for (const path of [join(home, '.bravebot'), project, profile]) mkdirSync(path, { recursive: true })
writeFileSync(join(project, 'README.md'), '# Plan\n')
const token = 'plan-task-only-fake-token'
const failures = [], steps = [], asked = []
let app, page
const content = text => ({ content: text })
const server = createServer(async (req, res) => {
  try {
    let raw = ''; for await (const chunk of req) raw += chunk
    assert.equal(req.headers.authorization, `Bearer ${token}`)
    asked.push(JSON.parse(raw))
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
const lastUser = (request) => {
  const said = request.messages.filter(m => m.role === 'user').at(-1)?.content
  return typeof said === 'string' ? said : said.map(part => part.text ?? '').join('')
}

try {
  const world = await launchWorld({ profile, env, linuxArgs: ['--disable-gpu'], timeout: 15000, viewport: { width: 1350, height: 900 } })
  app = world.app; page = world.page
  await app.evaluate(({ dialog }, folder) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] }) }, project)
  await page.evaluate(() => {
    window.planEvents = []
    window.bravebot.onEvent(event => window.planEvents.push(event))
  })
  const events = (name) => page.evaluate((name) => window.planEvents.filter(e => e.event === name), name)
  const ended = async () => (await events('turn.done')).length + (await events('turn.error')).length
  const menuItem = id => app.evaluate(({ Menu }, id) => {
    const item = Menu.getApplicationMenu().getMenuItemById(id)
    return item && { label: item.label, enabled: item.enabled }
  }, id)
  const chatItems = () => app.evaluate(({ Menu }) =>
    Menu.getApplicationMenu().items.find(m => m.label === 'Chat').submenu.items.map(i => i.id ?? i.type))
  const choose = id => app.evaluate(({ Menu }, id) => { Menu.getApplicationMenu().getMenuItemById(id).click() }, id)
  const theme = async (name) => {
    await page.evaluate((name) => window.bravebot.writeTheme(name), name)
    await page.waitForFunction((name) => document.documentElement.getAttribute('data-theme') === name, name)
  }
  const mode = () => page.locator('[data-test="mode-trigger"]').getAttribute('data-mode')
  const composer = page.getByRole('textbox', { name: COMPOSER_NAME })
  const dialog = page.getByRole('dialog', { name: 'Plan a task', exact: true })
  const planWith = async (task) => {
    await choose('session.plan')
    await dialog.waitFor()
    if (task) await dialog.locator('textarea').fill(task)
    await dialog.getByRole('button', { name: 'Plan', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
  }

  await page.getByRole('button', { name: 'Open project', exact: true }).click()
  const trust = page.getByRole('dialog', { name: 'Project trust', exact: true })
  await trust.waitFor()
  await approveTrust(trust)
  await trust.waitFor({ state: 'hidden' })

  // 1. The item, under Cycle Permission Mode in Chat, and the dialog in both themes.
  assert.equal((await menuItem('session.plan')).label, 'Plan a Task…')
  assert.equal((await menuItem('session.plan')).enabled, true)
  const items = await chatItems()
  assert.equal(items[items.indexOf('mode.cycle') + 1], 'session.plan')
  const idle = 'Switches this chat to Plan mode. With a task, starts a turn that plans it without changing anything.'
  const waits = 'Waits for the running turn, then switches this chat to Plan mode. With a task, starts a turn that plans it without changing anything.'
  // Opens the dialog, checks its subtitle, shoots it empty (when `empty` names the shot) and with a
  // task typed, then cancels.
  const shootDialog = async (subtitle, empty, typed) => {
    await choose('session.plan')
    await dialog.waitFor()
    await dialog.getByText('Task (optional)').first().waitFor()
    await dialog.getByText(subtitle, { exact: true }).waitFor()
    await page.waitForTimeout(200)
    if (empty) await page.screenshot({ path: join(shots, empty) })
    await dialog.locator('textarea').fill('list the TODOs in the parser')
    await page.waitForTimeout(200)
    await page.screenshot({ path: join(shots, typed) })
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
    await dialog.waitFor({ state: 'detached' })
  }
  for (const name of ['light', 'dark']) {
    await theme(name)
    await shootDialog(idle, `plan-dialog-empty-${name}.png`, `plan-dialog-${name}.png`)
  }
  await theme('light')
  assert.equal(await mode(), 'ask', 'cancelling changes nothing')
  console.log('PASS: Plan a Task… sits under Cycle Permission Mode and its dialog asks for an optional task')

  // 2. A mode the bridge did not set: an error entry in the transcript, and nothing is sent.
  await app.evaluate(({ ipcMain }) => {
    const bridge = ipcMain._invokeHandlers.get('bravebot:request')
    let failed = false
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', (event, method, params) => {
      if (method === 'session.mode' && !failed) { failed = true; return { error: { code: 'internal', message: 'The bridge did not answer.' } } }
      return bridge(event, method, params)
    })
  })
  await composer.fill('/plan list the TODOs')
  await composer.press('Enter')
  const card = page.locator('[data-test="error-card"]').last()
  await card.getByText('Could not switch to Plan mode', { exact: true }).waitFor()
  await card.getByText('Nothing was sent, so try again or choose Plan from the mode menu.', { exact: true }).waitFor()
  assert.equal(await card.locator('[data-test="error-retry"], [data-test="error-model"]').count(), 0, 'no turn or model to recover')
  await card.locator('[data-test="error-details"]').click()
  await card.getByText('The bridge did not answer.', { exact: true }).waitFor()
  await page.mouse.move(5, 5)
  // A fixed wait, since TS-005 allows one for an absence: it gives a wrong mode change or send that
  // followed the error card time to show up in the chip or in `asked`. The card is the last thing
  // the failed /plan does, and no event marks that no send will come after it.
  await page.waitForTimeout(300)
  assert.equal(await mode(), 'ask', 'the mode is unchanged')
  assert.equal(asked.length, 0, 'no turn started')
  for (const name of ['light', 'dark']) {
    await theme(name)
    await page.waitForTimeout(200)
    await page.screenshot({ path: join(shots, `plan-mode-failed-${name}.png`) })
  }
  await theme('light')
  console.log('PASS: a mode the bridge did not set is shown in the transcript and starts no turn')

  // 3. Typed mid-turn in an Ask chat: the dialog says it waits, and /plan x waits in the Queued tray
  //    with the chip still on Ask, then switches the chip to Plan once the turn ends.
  {
    let release
    steps.push(() => new Promise(resolve => { release = () => resolve(content('Noted.')) }))
    const before = await ended()
    await composer.fill('first, a question')
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await until(async () => release !== undefined, 'the turn reaches the model')
    for (const name of ['light', 'dark']) {
      await theme(name)
      await shootDialog(waits, null, `plan-dialog-waiting-${name}.png`)
    }
    await theme('light')
    const sent = asked.length
    await composer.fill('/plan x')
    await composer.press('Enter')
    await page.getByText('/plan x', { exact: true }).first().waitFor()
    // A fixed wait, since TS-005 allows one for an absence: it gives a mode change or a send that
    // skipped the queue time to reach the chip or `asked`. Nothing marks that the queued line will
    // not be sent early. The release() below, after which `asked` gains the task, shows the
    // counter does count a send in this window.
    await page.waitForTimeout(300)
    assert.equal(await mode(), 'ask', 'the chip stays on Ask while the line waits')
    await page.screenshot({ path: join(shots, 'plan-typed-queued-light.png') })
    assert.equal(asked.length, sent, 'the typed line waits for the running turn')
    steps.push(() => content('Planned x.'))
    release()
    await until(async () => await ended() > before + 1, 'the queued plan ends')
    assert.equal(lastUser(asked.at(-1)), 'x')
    assert.equal(await mode(), 'plan')
    await page.getByText('Planned x.').first().waitFor()
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(shots, 'plan-typed-after-light.png') })
    console.log('PASS: /plan x typed mid-turn waits with the chip on Ask, then switches to Plan')
  }

  // Back to Ask through the mode menu, so the next step shows the dialog changing the mode.
  const trigger = page.locator('[data-test="mode-trigger"]')
  await trigger.click()
  await page.locator('[data-test="mode-option-ask"]').click()
  await until(async () => await mode() === 'ask', 'the mode reads Ask')
  if ((await trigger.getAttribute('aria-expanded')) === 'true') await trigger.click()

  // 4. No task: only the mode changes, and nothing reaches the model.
  const asking = asked.length
  await planWith('')
  await until(async () => await mode() === 'plan', 'the mode reads Plan')
  // A fixed wait, since TS-005 allows one for an absence: carryOutPlan sets the chip from the
  // session.mode reply before it would call send, so a wrongly started turn would reach `asked`
  // after the chip reads Plan, and no event marks that no send is coming.
  await page.waitForTimeout(500)
  assert.equal(asked.length, asking, 'no turn started')
  console.log('PASS: confirming with no task only switches the mode to Plan')

  // 5. A task: a plan-mode turn sent the task alone.
  steps.push(() => content('Here is the plan.'))
  let before = await ended()
  await planWith('list the TODOs')
  await until(async () => await ended() > before, 'the planned turn ends')
  assert.equal(lastUser(asked.at(-1)), 'list the TODOs', 'the model is sent the task, not the /plan line')
  assert.equal(await mode(), 'plan')
  await page.getByText('Here is the plan.').first().waitFor()
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(shots, 'plan-turn-light.png') })
  console.log('PASS: confirming with a task starts a plan-mode turn sent only the task')

  // 6. Mid-turn: offered, and the plan waits in the queue until the turn ends.
  let release
  steps.push(() => new Promise(resolve => { release = () => resolve(content('Noted.')) }))
  before = await ended()
  await composer.fill('first, a question')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await until(async () => release !== undefined, 'the turn reaches the model')
  assert.equal((await menuItem('session.plan')).enabled, true, 'offered mid-turn')
  const sent = asked.length
  await planWith('check the tests')
  await page.getByText('/plan check the tests').first().waitFor()
  // A fixed wait, since TS-005 allows one for an absence: it gives a send that skipped the queue
  // time to reach `asked`, and nothing marks that the queued plan will not be sent early. The
  // release() below, after which `asked` gains the task, shows the counter does count a send in
  // this window.
  await page.waitForTimeout(300)
  await page.screenshot({ path: join(shots, 'plan-queued-light.png') })
  assert.equal(asked.length, sent, 'the plan waits for the running turn')
  steps.push(() => content('Planned the tests.'))
  release()
  await until(async () => asked.length > sent, 'the queued plan reaches the model')
  assert.equal(lastUser(asked.at(-1)), 'check the tests')
  await until(async () => await ended() > before + 1, 'the queued plan ends')
  console.log('PASS: a plan asked for mid-turn waits in the queue, then is sent the task alone')

  assert.deepEqual(world.errors, []); assert.deepEqual(failures, [])
  console.log(`PASS: Plan a Task… against the real bridge; screenshots in ${shots}`)
} catch (error) {
  console.error('Fixture failures:', failures)
  if (page) { await page.screenshot({ path: join(shots, 'plan-failure.png') }).catch(() => {}); console.error((await page.locator('body').innerText().catch(() => '')).slice(-3000)) }
  throw error
} finally {
  if (app) await app.close()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  rmSync(root, { recursive: true, force: true })
}
