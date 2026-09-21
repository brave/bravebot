// Isolated Electron acceptance: deterministic events, no provider calls or user records.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const profile = mkdtempSync(join(tmpdir(), 'bravebot-turn-details-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-turn-details-project-'))
const output = join(tmpdir(), 'bravebot-turn-details')
mkdirSync(output, { recursive: true })
// XWayland keeps animation frames flowing during Playwright's stability checks.
const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), '--disable-renderer-backgrounding', '--disable-background-timer-throttling', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  // Tiling compositors may ignore BrowserWindow.setSize; fix the renderer viewport too.
  await page.setViewportSize({ width: 1350, height: 900 })
  await page.emulateMedia({ reducedMotion: 'reduce' })
  page.setDefaultTimeout(8000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ ipcMain, BrowserWindow }, directory) => {
    const rows = ['a', 'b'].map(id => ({ id, directory, project: 'Turn details fixture', branch: 'main', title: `Conversation ${id}`, updated: Math.floor(Date.now() / 1000), bytes: 100 }))
    const counts = { a: 1, b: 1 }
    const emit = (event, data, session = 's-a') => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session })
    globalThis.turnDetailsFixture = { emit }
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', (_, method, params) => {
      if (method === 'agent.info') return { ok: { configured: true } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'models.list') return { ok: { defaultModel: 'sample/selected', warnings: [], models: [{ id: 'sample/selected', name: 'Selected model', provider: 'Sample', premium: false, contextWindow: 100000 }] } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: 'sample/selected', record: { ...rows.find(row => row.id === params.id), turns: 1, tokens: 9999 }, said: [{ kind: 'user', text: 'An earlier question' }, { kind: 'assistant', text: 'An earlier answer' }], todos: {}, trust: { known: true, rules: [] }, archived: 0 } }
      if (method === 'turn.send') {
        const turn = ++counts[params.session.slice(2)]
        emit('turn.started', { turn }, params.session)
        return { ok: { turn } }
      }
      return { ok: {} }
    })
    ipcMain.removeHandler('bravebot:files:list')
    ipcMain.handle('bravebot:files:list', () => ({ path: '', rows: [], truncated: false }))
    BrowserWindow.getAllWindows()[0].setSize(1350, 900)
  }, directory)
  const emit = (event, data, session = 's-a') => app.evaluate((_, value) => globalThis.turnDetailsFixture.emit(value.event, value.data, value.session), { event, data, session })
  const done = (turn, changes = {}, session = 's-a') => emit('turn.done', { turn, id: session.slice(2), reply: `Finished turn ${turn}.`, model: 'sample/actual', tokens: 12840, outputTokens: 1620, steps: 4, clean: false, notices: ['Loaded instructions from AGENTS.md.', 'Could not load skill “interface-review”: file not found.'], trust: { rules: [] }, archived: 0, ...changes }, session)
  const selectConversation = (id) => page.locator('.session').filter({ hasText: `Conversation ${id}` }).click()
  const send = async (prompt) => {
    await page.getByRole('textbox', { name: 'Message the agent' }).fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.locator('.transcript .working').waitFor()
  }
  await page.reload()
  await selectConversation('a')
  await page.getByRole('button', { name: 'Audit unavailable', exact: false }).click()
  await page.getByText('Audit details aren’t available for this saved reply.').waitFor()
  assert.equal(await page.locator('.turn-statistics').count(), 0, 'session total is not a turn statistic')
  await page.getByRole('button', { name: 'Close audit inspector' }).click()

  await send('Improve the empty state')
  await page.locator('.transcript .working').getByRole('button', { name: 'Audit', exact: true }).click()
  await page.getByText('Live · This turn', { exact: true }).waitFor()
  await emit('audit', { turn: 2, event: { kind: 'action_field', tool: 'write_file', field: 'path', role: 'routing', label: { integrity: 'untrusted', confidentiality: 'public' }, allowed: false } })
  await page.getByText('Untrusted data used as a destination', { exact: true }).first().waitFor()
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  await selectConversation('b')
  await emit('audit', { turn: 2, event: { kind: 'future_event', detail: 'Preserve unknown evidence' } })
  await done(2)
  await selectConversation('a')
  await page.getByText('Finished turn 2.', { exact: true }).waitFor()
  const notices = page.locator('.turn-notices').first()
  assert.equal(await notices.getAttribute('open'), '')
  assert.equal(await notices.locator('li').count(), 2)
  const footer = page.locator('.turn-footer').last()
  await footer.locator('summary').click()
  assert.match(await footer.innerText(), /12,840/)
  assert.match(await footer.innerText(), /1,620/)
  assert.match(await footer.innerText(), /Tool-calling rounds/)
  assert.match(await footer.innerText(), /sample\/actual/)
  assert.doesNotMatch(await footer.innerText(), /sample\/selected/)
  await selectConversation('b'); await selectConversation('a')
  assert.equal(await page.locator('.turn-statistics').last().getAttribute('open'), '')
  await footer.getByRole('button', { name: 'Policy blocked an action', exact: false }).click()
  const inspector = page.locator('.audit-inspector')
  await inspector.getByText('Recorded evidence', { exact: true }).first().click()
  assert.match(await inspector.locator('pre').first().innerText(), /"allowed": false/)
  await inspector.locator('.audit-all > summary').click()
  await inspector.getByText('future_event', { exact: true }).waitFor()
  await page.screenshot({ path: join(output, 'desktop.png') })
  await page.keyboard.press('Escape')
  assert.equal(await footer.getByRole('button', { name: 'Policy blocked an action', exact: false }).evaluate(el => el === document.activeElement), true)

  await send('Repeat the same instructions')
  await page.locator('.transcript .working').getByRole('button', { name: 'Audit', exact: true }).click()
  await done(3, { clean: true, model: 'sample/second', tokens: 0, outputTokens: 0, steps: 0 })
  await page.getByText('Finished turn 3.', { exact: true }).waitFor()
  assert.equal(await page.locator('.turn-notices').last().getAttribute('open'), null)
  assert.match(await page.locator('.turn-statistics summary').last().innerText(), /0 tokens/)
  await page.getByText('No policy refusals recorded for this turn.').waitFor()
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  assert.equal(await page.locator('.turn-footer').last().getByRole('button', { name: 'Audit', exact: false }).evaluate(el => el === document.activeElement), true, 'completed footer replaces the live audit trigger')

  await send('Cancel this turn')
  await emit('turn.error', { turn: 4, kind: 'cancelled', message: 'Cancelled', id: 'a' })
  await page.getByText('Final usage unavailable', { exact: true }).waitFor()
  await page.locator('.turn-footer').last().getByRole('button', { name: 'Audit', exact: false }).click()
  await page.getByText('Capture incomplete', { exact: true }).waitFor()
  await page.getByRole('button', { name: 'Close audit inspector' }).click()

  // Late-arriving notices must not move the passage the user is reading.
  await send('Check reading position')
  for (let index = 0; index < 30; index++) await emit('narration', { text: `Reading anchor ${index}. A longer progress message that gives the transcript enough height to read earlier activity while this turn is running.` })
  await page.locator('.narration').last().waitFor()
  const anchor = await page.locator('.entries').evaluate(el => {
    el.scrollTop = el.scrollHeight / 2
    el.dispatchEvent(new Event('scroll'))
    const row = [...el.querySelectorAll('.narration')].find(row => row.getBoundingClientRect().top >= el.getBoundingClientRect().top + 20)
    return { text: row.textContent, top: row.getBoundingClientRect().top }
  })
  await done(5, { notices: ['New instructions loaded.', 'A changed notice group.'] })
  await page.getByText('Finished turn 5.', { exact: true }).waitFor({ state: 'attached' })
  const after = await page.getByText(anchor.text, { exact: true }).evaluate(el => el.getBoundingClientRect().top)
  assert.ok(Math.abs(after - anchor.top) < 2, `notice insertion moved reading position by ${after - anchor.top}px`)
  assert.equal(await page.locator('.turn-notices').last().getAttribute('open'), '')

  // An open file tree keeps its tab and component state while audit temporarily replaces it.
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  await page.getByRole('tab', { name: 'Files', exact: true }).click()
  await page.locator('.turn-footer').last().getByRole('button', { name: 'Policy blocked an action', exact: false }).click()
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  assert.equal(await page.getByRole('tab', { name: 'Files', exact: true }).getAttribute('aria-selected'), 'true')

  await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setSize(950, 780))
  await page.setViewportSize({ width: 950, height: 780 })
  await page.locator('.app.right-folded').waitFor()
  await page.locator('.turn-footer').filter({ hasText: 'sample/actual' }).first().getByRole('button', { name: 'Policy blocked an action', exact: false }).click()
  await page.locator('.app:not(.right-folded)').waitFor()
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' })
  await page.screenshot({ path: join(output, 'narrow-dark.png') })
  assert.equal(await inspector.evaluate(el => el.scrollWidth <= el.clientWidth), true, 'audit evidence fits the drawer')
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  await page.locator('.app.right-folded').waitFor()
  assert.deepEqual(errors, [])
  console.log('PASS: historical unknowns, live audit, background delivery, notices, per-turn usage, disclosure persistence, evidence, unknown tags, cancellation, focus restoration, and narrow drawer.')
  console.log(`Screenshots: ${output}`)
} finally {
  await app.close()
}
