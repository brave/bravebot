// The server picker on a bot's details: every server, only these, a refused write, and back.
//
// Isolated profile from the acceptance fixture. `bot.servers.*` are called by the main process
// rather than the window, so their IPC handlers are replaced here and answer from a stored line;
// `connectors.list` is answered beside the fixture's bridge. The main process's own handlers, and
// the definition name they send, are covered by bot-servers.test.mjs.
//
//   pnpm drive:bot-servers
import assert from 'node:assert/strict'
import { mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { installBridge, makeProfile } from './acceptance-fixture.mjs'

const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-bot-servers')
mkdirSync(output, { recursive: true })
const { profile, directory } = makeProfile()
const app = await electron.launch({ args: ['.', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(7000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await installBridge(app, { directory })
  await app.evaluate(({ ipcMain }) => {
    const fixture = ipcMain._invokeHandlers?.get('bravebot:request')
    if (!fixture) throw new Error('the fixture bridge is not installed')
    globalThis.servers = {
      line: { servers: null, tools: false, inline: false, kind: 'worker', editable: true, every: true, everyPossible: true },
      writes: [], refuse: null, unlisted: false, thrown: [],
    }
    // Electron's own handler puts a modal box up, which stops the main process and this driver with it.
    process.on('uncaughtException', (error) => globalThis.servers.thrown.push(String(error?.stack ?? error)))
    const replace = (name, fn) => { ipcMain.removeHandler(name); ipcMain.handle(name, fn) }
    replace('bravebot:request', (event, method, params) => method !== 'connectors.list'
      ? fixture(event, method, params)
      : globalThis.servers.unlisted ? { error: { code: 'internal', message: 'mcp.json could not be read' } }
      : { ok: { home: null, state: null, writable: true, unavailable: null, connectors: [
        { alias: 'github', connected: true, problem: null, refused: null, changed: false },
        { alias: 'linear', connected: false, problem: null, refused: null, changed: false },
      ] } })
    replace('bravebot:bots:servers:read', () => globalThis.servers.line)
    replace('bravebot:bots:servers:write', async (_event, _slug, servers) => {
      const state = globalThis.servers
      state.writes.push(servers)
      if (state.refuse) throw new Error(state.refuse)
      state.line = { ...state.line, servers, every: servers === null && state.line.everyPossible }
      return state.line
    })
  })
  const writes = () => app.evaluate(() => globalThis.servers.writes)
  const wrote = async (count) => {
    for (const started = Date.now(); Date.now() - started < 5000; await page.waitForTimeout(50)) {
      const all = await writes()
      if (all.length >= count) return all.at(-1)
    }
    throw new Error(`write ${count} was never sent`)
  }
  const snap = async (name) => { await page.waitForTimeout(300); await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' }); console.log('VISUAL', name) }
  const picker = page.locator('[data-test="bot-servers"]')
  const row = (alias) => picker.locator(`[data-test="bot-server"][data-alias="${alias}"] leo-checkbox`)
  const ticked = async (alias, value) => {
    const box = (alias) => document.querySelector(`[data-test="bot-server"][data-alias="${alias}"] leo-checkbox`)
    try {
      await page.waitForFunction(([alias, value, box]) => new Function('alias', `return (${box})(alias)`)(alias)?.checked === value,
        [alias, value, box.toString()], { polling: 100, timeout: 5000 })
    } catch {
      const shown = await page.evaluate(([alias, box]) => new Function('alias', `return (${box})(alias)`)(alias)?.checked, [alias, box.toString()])
      throw new Error(`${alias} should be ${value ? 'ticked' : 'clear'}, and is ${shown ? 'ticked' : 'clear'}`)
    }
  }

  const mode = picker.locator('[data-test="bot-servers-mode"]')
  const choose = async (name) => {
    await mode.click()
    await page.getByRole('option', { name, exact: true }).click()
  }

  await page.reload()
  await page.getByRole('button', { name: 'Bots', exact: true }).click()
  await page.locator('[data-test="bot-details"]').waitFor()
  await page.waitForFunction(() => document.querySelector('[data-test="bot-servers-mode"]')?.value === 'every', null, { polling: 100, timeout: 5000 })
  assert.equal(await picker.locator('[data-test="bot-server"]').count(), 0, 'every server lists none to tick')
  await picker.getByText('Applies from this bot’s next turn, in open conversations too.').waitFor()
  await snap('00-every-server')

  await choose('Only these')
  assert.deepEqual(await wrote(1), ['github', 'linear'], 'choosing only these keeps every declared server at first')
  await row('linear').waitFor()
  await ticked('github', true)
  await picker.locator('[data-test="bot-server"][data-alias="github"]').getByText('Connected', { exact: true }).waitFor()
  await picker.locator('[data-test="bot-server"][data-alias="linear"]').getByText('Not connected', { exact: true }).waitFor()

  await row('linear').click()
  assert.deepEqual(await wrote(2), ['github'])
  await ticked('linear', false)
  await snap('01-only-github')

  await app.evaluate(() => { globalThis.servers.refuse = 'This definition declares its servers inline.' })
  await row('github').click()
  await wrote(3)
  await picker.getByRole('alert').getByText('This definition declares its servers inline.').waitFor()
  await ticked('github', true)
  await snap('02-refused')

  await app.evaluate(() => { globalThis.servers.refuse = null })
  await choose('Every connected server')
  assert.equal(await wrote(4), null, 'every server removes the line')
  await page.waitForFunction(() => !document.querySelector('[data-test="bot-server"]'), null, { polling: 100, timeout: 5000 })
  await snap('03-every-again')

  // A list that cannot be read offers no switch, since "Only these" would save a line selecting none.
  await app.evaluate(() => { globalThis.servers.unlisted = true })
  const reopen = async () => {
    await page.reload()
    await page.getByRole('button', { name: 'Bots', exact: true }).click()
    await page.getByText('Review Bot', { exact: true }).first().click()
    await page.locator('[data-test="bot-details"]').waitFor()
  }
  await reopen()
  await picker.locator('[data-test="bot-servers-unlisted"]').waitFor()
  assert.equal(await picker.locator('[data-test="bot-servers-mode"]').count(), 0, 'no switch without the list')
  await snap('04-unlisted')

  // Under a tools line, removing the line would leave none, so the switch is not offered.
  await app.evaluate(() => {
    globalThis.servers.unlisted = false
    globalThis.servers.line = { ...globalThis.servers.line, servers: null, tools: true, every: false, everyPossible: false }
  })
  await reopen()
  await picker.locator('[data-test="bot-servers-tools"]').waitFor()
  assert.equal(await picker.locator('[data-test="bot-servers-mode"]').count(), 0, 'no switch under a tools line')
  await ticked('github', false)
  await row('github').click()
  assert.deepEqual(await wrote(5), ['github'])
  await ticked('github', true)
  await snap('05-tools-line')

  // With the details panel dragged to its narrowest, the choice still fits inside it.
  await app.evaluate(() => {
    globalThis.servers.line = { ...globalThis.servers.line, tools: false, every: true, everyPossible: true, servers: null }
  })
  await reopen()
  await mode.waitFor()
  const seam = await page.locator('.gutter.right').boundingBox()
  await page.mouse.move(seam.x + seam.width / 2, seam.y + seam.height / 2)
  await page.mouse.down()
  await page.mouse.move(seam.x + 400, seam.y + seam.height / 2, { steps: 10 })
  await page.mouse.up()
  const [panel, choice] = await Promise.all([
    page.locator('[data-test="bot-details"]').boundingBox(),
    mode.boundingBox(),
  ])
  assert.ok(panel.width < 260, `the panel was dragged narrow, and is ${panel.width}px`)
  assert.ok(choice.x + choice.width <= panel.x + panel.width, 'the choice stays inside the details panel')
  await snap('06-narrow')

  assert.deepEqual(errors, [])
  // A checkout without `pnpm run bridge` throws from the watch poll; that is not the picker's.
  const thrown = await app.evaluate(() => globalThis.servers.thrown)
  assert.deepEqual(thrown.filter((each) => !each.includes('bravebot-rpc is not built')), [])
  console.log('drive-bot-servers: ok')
} finally {
  await app.close()
}
