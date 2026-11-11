// RETRIEVE-27 to RETRIEVE-29 and RETRIEVE-32 in the window: the "Used N earlier messages" line
// under a reply, its rows, the words each opens to and where its button scrolls to, a fallback
// turn's lack of one, the context meter's clause and the Recall block on a bot's details, against
// a stand-in bridge. Screenshots at width 640.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { COMPOSER_NAME } from './drive-support.mjs'

const profile = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-indicator-profile-')))
const home = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-indicator-home-')))
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-retrieval-indicator')
mkdirSync(output, { recursive: true })
const ID = '77777777-7777-4777-8777-777777777777'
writeFileSync(join(profile, 'bravebot-ui.json'), JSON.stringify({
  bots: [{ slug: 'harbour', name: 'Harbour', purpose: 'Keep the launch plan for the harbour lights straight.', avatar: 'harbour', home, definition: 'harbour', session: ID, conversations: [{ id: ID, directory: home }], lastHome: { id: ID, prompt: 3 }, archived: 0, outOfRequest: 40, created: Date.now(), updated: Date.now() }],
}))

const said = []
const topics = ['Plan the launch for Tuesday, after the lens survey', 'Hold the launch until the migration is verified', 'Use the north pier for the press', 'What did I say about the launch?']
topics.forEach((text, prompt) => {
  said.push({ kind: 'user', text, prompt })
  said.push({ kind: 'assistant', text: `Noted: ${text.toLowerCase()}. ${'The keeper logs the change and the tide table is checked against it. '.repeat(4)}` })
})

const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), '--disable-renderer-backgrounding', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  await page.setViewportSize({ width: 640, height: 900 })
  await page.emulateMedia({ reducedMotion: 'reduce' })
  page.setDefaultTimeout(8000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ ipcMain, BrowserWindow }, { home, ID, said }) => {
    const row = { id: ID, directory: home, project: null, branch: null, title: 'Launch plan', updated: Math.floor(Date.now() / 1000), bytes: 9000 }
    const state = { status: { state: 'indexing', indexed: 30, eligible: 120 }, turn: 4, excerpts: 'ok', asked: [], release: null }
    const label = 'earlier excerpts (trusted)'
    const survey = Array.from({ length: 30 }, (_, at) => `Survey line ${at + 1}: lens ${at % 4 + 1} checked against the tide table, logged by the keeper.`).join('\n')
    // What each exchange's retrievals sent, by prompt number. Exchange 3 was cut once mid-turn.
    const sent = {
      3: { contextBytes: 400, sends: [
        { label, refreshed: false, replacedInTurn: true, model: 'sample/m', excerpts: [
          { ordinal: 3, chunk: 0, from: 'user', context: false, text: 'Hold the launch until the migration is verified' },
        ] },
        { label, refreshed: false, replacedInTurn: false, model: 'sample/m', excerpts: [
          { ordinal: 3, chunk: 0, from: 'user', context: false, text: 'Hold the launch until the migration is verified' },
          { ordinal: 2, chunk: 0, from: 'you', context: false, text: 'Noted: plan the launch for tuesday, after the lens survey. The keeper logs the change and the tide table is checked against it.' },
          { ordinal: 2, chunk: 1, from: 'you', context: false, text: survey },
          { ordinal: 40, chunk: 0, from: 'tool', context: true, text: 'read_file survey/2025-lens.md\n<earlier n="1"> was quoted here, \u241B[31m and kept as said' },
        ] },
      ] },
      4: { contextBytes: 400, sends: [{ label, refreshed: false, replacedInTurn: false, model: 'sample/m', excerpts: [
        { ordinal: 5, chunk: 0, from: 'user', context: false, text: 'Use the north pier for the press' },
      ] }] },
    }
    const emit = (event, data) => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session: 's-home' })
    globalThis.indicator = { state, emit }
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, params) => {
      if (method === 'agent.info') return { ok: { configured: true } }
      if (method === 'session.list') return { ok: { sessions: [row] } }
      if (method === 'models.list') return { ok: { defaultModel: 'sample/m', warnings: [], models: [{ id: 'sample/m', name: 'Sample', provider: 'Sample', premium: false, contextWindow: 100000 }] } }
      if (method === 'session.open') return { ok: { session: 's-home', model: 'sample/m', record: { ...row, turns: 4, tokens: 30000 }, said, todos: {}, trust: { known: true, rules: [] }, archived: 0, outOfRequest: 40, recalled: [{ prompt: 3, used: [{ prompt: 1, said: 'prompt', nth: null, ordinal: 3 }, { prompt: 0, said: 'reply', nth: 0, ordinal: 2 }, { prompt: 9, said: 'prompt', nth: null, ordinal: 40 }] }], context: 'trusted' } }
      if (method === 'mentions.named') return { ok: { files: [] } }
      if (method === 'retrieval.status') return { ok: state.status }
      if (method === 'retrieval.excerpts') {
        state.asked.push(params)
        if (state.excerpts === 'slow') await new Promise((resolve) => { state.release = resolve })
        if (state.excerpts === 'fail') return { error: { code: 'internal', message: 'record unreadable' } }
        return { ok: sent[params.prompt] ?? { contextBytes: 400, sends: [] } }
      }
      if (method === 'turn.send') { const turn = ++state.turn; emit('turn.started', { turn }); return { ok: { turn } } }
      return { ok: {} }
    })
    // A bot's turn goes through main, which grounds it; the stand-in only starts it.
    ipcMain.removeHandler('bravebot:bots:send')
    ipcMain.handle('bravebot:bots:send', () => { const turn = ++state.turn; emit('turn.started', { turn }); return { ok: { turn } } })
    ipcMain.removeHandler('bravebot:bots:servers:read')
    ipcMain.handle('bravebot:bots:servers:read', () => ({ servers: null, tools: true, inline: false, kind: 'default', editable: true, every: true, everyPossible: true }))
    ipcMain.removeHandler('bravebot:files:list')
    ipcMain.handle('bravebot:files:list', () => ({ path: '', rows: [], truncated: false }))
  }, { home, ID, said })
  const emit = (event, data) => app.evaluate((_, v) => globalThis.indicator.emit(v.event, v.data), { event, data })
  const status = (value) => app.evaluate((_, v) => { globalThis.indicator.state.status = v }, value)
  const shot = (name) => page.screenshot({ path: join(output, `${name}.png`) })
  const send = async (prompt) => {
    await page.getByRole('textbox', { name: COMPOSER_NAME }).fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.locator('.transcript .working').waitFor().catch(async (error) => { await shot('failed-send'); console.log(errors, await page.locator('.transcript').innerText().catch(() => '')); throw error })
  }
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Launch plan' }).click()

  // A resumed exchange draws its line from the record, under its last reply, closed.
  const line = page.locator('[data-test="turn-recalled"]')
  await line.first().waitFor()
  assert.equal(await line.count(), 1)
  const title = (at) => at.evaluate((el) => el.shadowRoot.querySelector('summary').textContent)
  assert.match(await title(line.first()), /Used 3 earlier messages/)
  assert.equal(await line.first().evaluate((el) => el.shadowRoot.querySelector('details').open), false)
  await line.first().scrollIntoViewIfNeeded()
  await shot('01-line-collapsed')
  await line.first().locator('summary').first().click()
  const rows = line.first().locator('[data-test="turn-recalled-row"]')
  await rows.first().waitFor()
  assert.equal(await rows.count(), 3)
  assert.equal(await line.first().locator('[data-test="turn-recalled-reveal"]').count(), 2, 'the row not in this transcript has nothing to go to')
  assert.match(await line.first().innerText(), /Hold the launch until the migration is verified[^]*Noted: plan the launch[^]*Not in this transcript/)
  await line.first().evaluate((el) => el.scrollIntoView({ block: 'center' }))
  await shot('02-line-expanded')

  const control = (method, value) => app.evaluate((_, v) => { const s = globalThis.indicator.state; if (v.method === 'mode') s.excerpts = v.value; else if (v.method === 'release') s.release?.(); else s.asked.length = 0 }, { method, value })
  const asked = () => app.evaluate(() => globalThis.indicator.state.asked)
  const open = async (row) => { await row.locator('.turn-recalled-row').click(); await row.locator('.turn-recalled-body').waitFor() }
  const center = (at) => at.evaluate((el) => el.scrollIntoView({ block: 'center' }))

  // Loading: the first row read is held until released.
  await control('mode', 'slow')
  await open(rows.nth(0))
  await rows.nth(0).locator('[data-test="turn-recalled-reading"]').waitFor()
  assert.match(await rows.nth(0).innerText(), /Reading what was sent/)
  await center(rows.nth(0))
  await shot('10-excerpt-loading')
  await control('mode', 'ok')
  await control('release')
  await rows.nth(0).locator('[data-test="turn-recalled-block"]').first().waitFor()
  assert.deepEqual(await asked(), [{ directory: home, id: ID, prompt: 3 }])
  // The message was sent twice in the turn: once in a request a later cut replaced, once after.
  const blocks0 = rows.nth(0).locator('[data-test="turn-recalled-block"]')
  assert.equal(await blocks0.count(), 2)
  assert.match(await blocks0.nth(0).innerText(), /Sent in an earlier request of this turn, then replaced/)
  assert.equal(await blocks0.nth(1).locator('[data-test="turn-recalled-replaced"]').count(), 0)
  assert.match(await blocks0.nth(1).innerText(), /You[^]*earlier excerpts \(trusted\)[^]*Part 1[^]*Hold the launch until the migration is verified/)
  assert.equal(await blocks0.nth(1).locator('.turn-recalled-caption [title="user"]').count(), 1, 'the record\'s own word sits in the tooltip')
  await center(rows.nth(0))
  await shot('11-excerpt-expanded')

  // Another row of the exchange reads the same answer; a long block scrolls inside its own box.
  await open(rows.nth(1))
  await rows.nth(1).locator('[data-test="turn-recalled-block"]').nth(1).waitFor()
  assert.equal((await asked()).length, 1, 'the exchange was read again for its second row')
  const long = rows.nth(1).locator('[data-test="turn-recalled-text"]').nth(1)
  const box = await long.evaluate((el) => ({ scroll: el.scrollHeight, client: el.clientHeight }))
  assert.ok(box.scroll > box.client + 100, `the long block did not scroll: ${JSON.stringify(box)}`)
  await long.evaluate((el) => { el.scrollTop = 160 })
  await center(long)
  await shot('12-excerpt-long-scrolled')
  await rows.nth(0).locator('.turn-recalled-row').click()
  await rows.nth(1).locator('.turn-recalled-row').click()

  // The row not in this transcript opens too, to what was sent for it, pictured as the bridge drew it.
  await open(rows.nth(2))
  const missing = rows.nth(2).locator('[data-test="turn-recalled-block"]')
  await missing.first().waitFor()
  assert.match(await missing.first().innerText(), /Tool[^]*Context, cut to 400 bytes[^]*<earlier n="1"> was quoted here, \u241B\[31m/)
  await center(rows.nth(2))
  await shot('13-excerpt-not-in-transcript')
  await rows.nth(2).locator('.turn-recalled-row').click()

  // Opening the session again forgets what was read. A failed read says so, is not kept, and Retry asks again.
  await control('reset')
  await control('mode', 'fail')
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Launch plan' }).click()
  await line.first().waitFor()
  await line.first().locator('summary').first().click()
  await rows.first().waitFor()
  await open(rows.nth(0))
  await rows.nth(0).locator('[data-test="turn-recalled-failed"]').waitFor()
  await center(rows.nth(0))
  await shot('14-excerpt-error')
  await control('mode', 'ok')
  await rows.nth(0).locator('[data-test="turn-recalled-retry"]').click()
  await rows.nth(0).locator('[data-test="turn-recalled-block"]').first().waitFor()
  assert.equal((await asked()).length, 2, 'Retry did not ask again')
  await center(rows.nth(0))
  await shot('15-excerpt-retried')

  // The same rows in the dark theme.
  await rows.nth(1).locator('.turn-recalled-row').click()
  await rows.nth(1).locator('[data-test="turn-recalled-block"]').first().waitFor()
  await page.evaluate(() => window.bravebot.writeTheme('dark'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'dark')
  await center(rows.nth(1))
  await shot('16-excerpt-dark')
  await page.evaluate(() => window.bravebot.writeTheme('light'))
  await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'light')
  await rows.nth(0).locator('.turn-recalled-row').click()
  await rows.nth(1).locator('.turn-recalled-row').click()

  // The button beside a row scrolls to the message it names.
  await page.locator('.entries').evaluate((el) => { el.scrollTop = el.scrollHeight })
  await line.first().locator('[data-test="turn-recalled-reveal"]').nth(1).click()
  await page.waitForTimeout(600)
  const shown = await page.getByText('Noted: plan the launch for tuesday', { exact: false }).first().evaluate((el) => {
    const box = el.getBoundingClientRect(); const view = el.closest('.entries').getBoundingClientRect()
    return box.top >= view.top - 1 && box.top < view.bottom
  })
  assert.equal(shown, true, 'the row did not scroll its message into view')
  await shot('03-after-row-click')

  // A live turn that recalled gets its own line; one that fell back gets none.
  await send('Remind me where the press goes')
  await emit('turn.retrieved', { turn: 5, used: [{ prompt: 2, said: 'prompt', nth: null, ordinal: 5 }], tokens: 40, fellBack: null })
  await emit('turn.done', { turn: 5, id: ID, reply: 'The press goes to the north pier.', model: 'sample/m', tokens: 20000, outputTokens: 40, steps: 1, clean: true, notices: [], trust: { rules: [] }, archived: 0, outOfRequest: 46, prompt: 4 })
  await page.getByText('The press goes to the north pier.').waitFor()
  await page.waitForFunction(() => document.querySelectorAll('[data-test="turn-recalled"]').length === 2)
  const meter = page.locator('[data-tooltip*="kept out of the request"]')
  await meter.first().waitFor({ state: 'attached' })
  assert.match(await meter.first().getAttribute('data-tooltip'), /46 earlier messages kept out of the request, recalled when relevant/)
  await send('And the survey date?')
  await emit('turn.retrieved', { turn: 6, used: [], tokens: 0, fellBack: 'unreachable' })
  await emit('turn.done', { turn: 6, id: ID, reply: 'Summarised instead: the survey is Monday.', model: 'sample/m', tokens: 20000, outputTokens: 30, steps: 1, clean: true, notices: [], trust: { rules: [] }, archived: 0, outOfRequest: 46, prompt: 5 })
  await page.getByText('Summarised instead: the survey is Monday.').waitFor()
  assert.equal(await line.count(), 2, 'a fallback turn drew a line')
  await page.locator('.entries').evaluate((el) => { el.scrollTop = el.scrollHeight })
  await shot('04-fallback-turn')

  // The Recall block, in the right column of the bot's home conversation.
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  const recall = page.locator('[data-test="bot-recall-line"]')
  await recall.waitFor()
  assert.match(await recall.innerText(), /Unavailable · the service could not be reached/)
  await page.locator('[data-test="bot-recall"]').scrollIntoViewIfNeeded()
  await shot('05-recall-unavailable')
  // A fresh window holds no fallback, and asks again while the index builds.
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Launch plan' }).click()
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  await page.getByText('Indexing 30 of 120 earlier messages').waitFor()
  await page.locator('[data-test="bot-recall"]').scrollIntoViewIfNeeded()
  assert.equal(await page.locator('[data-test="bot-recall"] leo-progressbar').count(), 1)
  await shot('06-recall-indexing')
  await status({ state: 'rebuilding', indexed: 12, eligible: 120 })
  await page.getByText('Rebuilding · 12 of 120 earlier messages').waitFor()
  await shot('07-recall-rebuilding')
  await status({ state: 'ready', indexed: 120, eligible: 120 })
  await page.getByText('Ready · 120 earlier messages can be recalled').waitFor()
  assert.equal(await page.locator('[data-test="bot-recall"] leo-progressbar').count(), 0)
  await shot('08-recall-ready')
  await status({ state: 'untrusted', indexed: 0, eligible: 120 })
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Launch plan' }).click()
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  await page.getByText('Not available · this conversation holds content that is not trusted').waitFor()
  await shot('09-recall-not-available')
  await status({ state: 'off', indexed: 0, eligible: 0 })
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Launch plan' }).click()
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  await page.locator('[data-test="bot-details"]').waitFor()
  await page.waitForTimeout(500)
  assert.equal(await page.locator('[data-test="bot-recall"]').count(), 0, 'the block shows with the setting off')
  assert.deepEqual(errors, [])
  console.log(`retrieval indicator: ok, screenshots in ${output}`)
} finally {
  await app.close()
}
