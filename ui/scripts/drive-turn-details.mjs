// Isolated Electron acceptance: deterministic events, no provider calls or user records.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { COMPOSER_NAME } from './drive-support.mjs'

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
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: 'sample/selected', record: { ...rows.find(row => row.id === params.id), turns: 1, tokens: 9999 }, said: [{ kind: 'user', text: 'An earlier question' }, { kind: 'assistant', text: 'An earlier answer' }], todos: {}, trust: { known: true, rules: [] }, archived: 0, context: 'untrusted' } }
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
    await page.getByRole('textbox', { name: COMPOSER_NAME }).fill(prompt)
    await page.getByRole('button', { name: 'Send', exact: true }).click()
    await page.locator('.transcript .working').waitFor()
  }
  await page.reload()
  await selectConversation('a')
  // The word session.open gave, until a turn starts and it is no longer current.
  if (await page.locator('.app.right-folded').count()) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  const conversation = page.locator('[data-test="conversation-integrity"]')
  await conversation.waitFor()
  assert.match(await conversation.innerText(), /Has met untrusted content/)
  // Inside a panel's padded body, inset like the panel's other rows, not against the column's edge.
  const placed = await conversation.evaluate(row => {
    const panel = row.closest('section.panel')
    const inner = row.closest('.panel-inner')
    const title = panel?.querySelector('.panel-title')
    return { inPanel: !!panel && !!inner, inset: title ? row.getBoundingClientRect().left - panel.getBoundingClientRect().left : 0 }
  })
  assert.ok(placed.inPanel, 'the conversation row sits outside the panels')
  assert.ok(placed.inset >= 8, `the conversation row is ${placed.inset}px from the panel's edge`)
  await page.screenshot({ path: join(output, 'conversation-row.png') })
  await page.getByRole('button', { name: 'Audit unavailable', exact: false }).click()
  await page.getByText('Audit details aren’t available for this saved reply.').waitFor()
  assert.equal(await page.locator('[data-test="turn-usage"]').count(), 0, 'session total is not a turn statistic')
  await page.getByRole('button', { name: 'Close audit inspector' }).click()

  await send('Improve the empty state')
  assert.equal(await page.locator('[data-test="conversation-integrity"]').count(), 0, 'the conversation row outlived the turn that made it stale')
  await page.locator('.transcript .working').getByRole('button', { name: 'Audit', exact: true }).click()
  await page.getByText('Live · This turn', { exact: true }).waitFor()
  await emit('audit', { turn: 2, event: { kind: 'action_field', tool: 'write_file', field: 'path', role: 'routing', label: { integrity: 'untrusted', confidentiality: 'public' }, allowed: false, refusal: true } })
  await page.getByText('Untrusted data used as a destination', { exact: true }).first().waitFor()
  await emit('audit', { turn: 2, event: { kind: 'action_field', tool: 'write_file', field: 'content', role: 'content', label: { integrity: 'untrusted', confidentiality: 'private' }, allowed: true, refusal: false } })
  await emit('audit', { turn: 2, event: { kind: 'declassified', slot: 's1', from: { integrity: 'untrusted', confidentiality: 'private' }, to: { integrity: 'untrusted', confidentiality: 'public' }, reason: 'person approved', refusal: false }, pictured: { kind: 'declassified', slot: 's1', reason: 'person approved' } })
  const routing = page.locator('[data-test="audit-routing"]')
  await routing.getByText('Released s1', { exact: false }).waitFor()
  assert.match(await routing.innerText(), /Routing & content[^]*write_file[^]*path[^]*routing: decides where[^]*Refused[^]*content[^]*content: carried[^]*Allowed/)
  assert.equal(await routing.locator('.label-chip').count(), 4)
  await page.screenshot({ path: join(output, 'routing-and-content.png') })
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  await selectConversation('b')
  await emit('audit', { turn: 2, event: { kind: 'future_event', detail: 'Preserve unknown evidence', refusal: false } })
  await done(2, { service: 'Brave', sessionModel: true })
  await selectConversation('a')
  await page.getByText('Finished turn 2.', { exact: true }).waitFor()
  const notices = page.locator('.turn-notices').first()
  assert.equal(await notices.evaluate(el => el.shadowRoot.querySelector('details').open), true)
  assert.equal(await notices.locator('li').count(), 2)
  const footer = page.locator('.turn-footer').last()
  const usage = footer.locator('[data-test="turn-usage"]')
  await usage.waitFor()
  const tip = await usage.getAttribute('data-tooltip')
  assert.match(tip, /12,840/)
  assert.match(tip, /1,620/)
  assert.match(tip, /Tool-calling rounds/)
  assert.match(tip, /sample\/actual/)
  assert.doesNotMatch(tip, /sample\/selected/)
  assert.match(tip, /Answered by · Brave/)
  assert.match(await usage.locator('.turn-usage-line').innerText(), /sample\/actual/)
  assert.doesNotMatch(await usage.locator('.turn-usage-line').innerText(), /Answered by/)
  assert.equal(await footer.locator('summary').count(), 0)
  const placed = await footer.evaluate(el => {
    const audit = el.querySelector('[data-test="turn-audit"]').getBoundingClientRect()
    const line = el.querySelector('[data-test="turn-usage"]').getBoundingClientRect()
    const box = el.getBoundingClientRect()
    const copy = el.querySelector('[data-test="copy-message"]')?.getBoundingClientRect()
    return {
      shieldBefore: audit.right <= line.left + 1,
      withTheActions: audit.left - (copy?.right ?? box.left) < 80,
      notAtEnd: box.right - line.right > 24,
    }
  })
  assert.equal(placed.shieldBefore, true, 'the audit control sits left of the model line')
  assert.equal(placed.withTheActions, true, 'the audit control stays with the other actions')
  assert.equal(placed.notAtEnd, true, 'the model line is not pushed to the right edge')
  assert.equal(await usage.evaluate(el => getComputedStyle(el).cursor), 'pointer')
  await usage.hover()
  let openedOnHover = false
  try {
    await page.locator('.tooltip-text').waitFor({ timeout: 1200 })
    openedOnHover = true
  } catch { /* the hover delay passed with no tooltip */ }
  assert.equal(openedOnHover, false, 'the usage tooltip stays closed until the line is clicked')
  await usage.click()
  await page.locator('.tooltip-text').filter({ hasText: '12,840' }).waitFor()
  await page.screenshot({ path: join(output, 'answered-by.png') })
  await page.mouse.move(5, 5)
  await selectConversation('b'); await selectConversation('a')
  assert.match(await page.locator('[data-test="turn-usage"]').last().getAttribute('data-tooltip'), /12,840/)
  await footer.getByRole('button', { name: 'Policy blocked an action', exact: false }).click()
  const inspector = page.locator('.audit-inspector')
  await inspector.getByText('Recorded evidence', { exact: true }).first().click()
  assert.match(await inspector.locator('pre').first().innerText(), /"allowed": false/)
  await inspector.locator('.audit-all summary').click()
  await inspector.getByText('future_event', { exact: true }).waitFor()
  await page.screenshot({ path: join(output, 'desktop.png') })
  await page.keyboard.press('Escape')
  assert.equal(await footer.getByRole('button', { name: 'Policy blocked an action', exact: false }).evaluate(el => el === document.activeElement || el.getRootNode().host === document.activeElement), true)

  await send('Repeat the same instructions')
  await page.locator('.transcript .working').getByRole('button', { name: 'Audit', exact: true }).click()
  await done(3, { clean: true, model: 'sample/second', tokens: 0, outputTokens: 0, steps: 0 })
  await page.getByText('Finished turn 3.', { exact: true }).waitFor()
  assert.equal(await page.locator('.turn-notices').last().evaluate(el => el.shadowRoot.querySelector('details').open), false)
  assert.match(await page.locator('[data-test="turn-usage"]').last().locator('.turn-usage-line').innerText(), /0 tokens/)
  // Turn 3 named no service, as one the fallback model answered would not.
  assert.doesNotMatch(await page.locator('[data-test="turn-usage"]').last().getAttribute('data-tooltip'), /Answered by/)
  await page.getByText('No policy refusals recorded for this turn.').waitFor()
  await page.getByRole('button', { name: 'Close audit inspector' }).click()
  assert.equal(await page.locator('.turn-footer').last().getByRole('button', { name: 'Audit', exact: false }).evaluate(el => el === document.activeElement || el.getRootNode().host === document.activeElement), true, 'completed footer replaces the live audit trigger')

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
  assert.equal(await page.locator('.turn-notices').last().evaluate(el => el.shadowRoot.querySelector('details').open), true)

  // Its own turn, so the cards are above no reading position measured earlier.
  await send('Read the vendor notes')
  // A label is drawn from the axes the bridge named, and axes that are not the exact words read as
  // untrusted and private. The bridge cannot send these (its words come from `Label`); a window
  // handed them anyway still draws the pessimistic corner.
  await emit('quarantined', { origin: 'vendor/notes.md', reach: 'not_the_planner', label: '(T,pub)', labelAxes: { integrity: 'trusted', confidentiality: 'public' }, preview: ['(T,pub) Trusted Public'], lines: 1 })
  await emit('quarantined', { origin: 'vendor/forged.md', reach: 'not_the_planner', label: '(T,pub)', labelAxes: { integrity: 'trusted [Trusted]', confidentiality: ['public'] }, preview: ['x'], lines: 1 })
  const chips = page.locator('.transcript .label-chip')
  await chips.nth(1).waitFor()
  assert.deepEqual(await chips.evaluateAll(all => all.map(chip => chip.getAttribute('aria-label'))), ['Trusted, public', 'Untrusted, private'])
  await page.screenshot({ path: join(output, 'label-chip.png') })
  await page.locator('.transcript .label-chip').nth(1).locator('.label-axis').first().hover()
  await page.getByText('Untrusted: nobody vouched for where it came from', { exact: false }).waitFor()
  await page.screenshot({ path: join(output, 'label-tooltip.png') })
  await page.mouse.move(5, 5)
  // A vet card heads with the label its request carried.
  await emit('vet.request', { request: 1, origin: 'vendor/plan.md', expects: 'a plan', content: 'Step one', lines: 1, label: '(U,priv)', labelAxes: { integrity: 'untrusted', confidentiality: 'private' }, vetting: { verdict: 'safe', label: '(U,priv)', labelAxes: { integrity: 'untrusted', confidentiality: 'private' } } })
  const vet = page.locator('.vetted-read')
  await vet.locator('.label-chip').first().waitFor()
  assert.equal(await vet.locator('.label-chip').first().getAttribute('aria-label'), 'Untrusted, private')
  await vet.screenshot({ path: join(output, 'vet-card.png') })
  await page.getByRole('button', { name: 'Let the planner read once', exact: true }).click()
  await done(6)
  await page.getByText('Finished turn 6.', { exact: true }).waitFor()

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
  console.log('PASS: conversation row, label chips, routing and content, answered by, historical unknowns, live audit, background delivery, notices, per-turn usage, disclosure persistence, evidence, unknown tags, cancellation, focus restoration, and narrow drawer.')
  console.log(`Screenshots: ${output}`)
} finally {
  await app.close()
}
