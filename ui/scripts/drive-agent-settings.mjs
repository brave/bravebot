// 0.9 desktop acceptance, isolated profile and deterministic agent replies.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const profile = mkdtempSync(join(tmpdir(), 'bravebot-agent-settings-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-agent-settings-project-'))
const output = join(tmpdir(), 'bravebot-agent-settings')
mkdirSync(output, { recursive: true })
const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
let page
try {
  page = await app.firstWindow()
  await page.setViewportSize({ width: 1350, height: 900 })
  page.setDefaultTimeout(10000)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const errors = []
  page.on('pageerror', e => errors.push(e.message))
  await app.evaluate(({ ipcMain, BrowserWindow }, directory) => {
    const report = { build: 'bravebot 0.9.0 fixture', configured: true, problem: null, model: 'local/test', brave: false, bedrock: false, providers: [{ name: 'Local', credential: 'not required' }], selected: null, layers: [], overrides: [], managed: { path: '/etc/bravebot/managed.json', keys: ['provider'] }, network: { roots: ['/test/roots.pem'], problem: null, trustsNothing: false, proxy: 'proxy.example:8080', authenticated: true, unusableProxy: null, noProxy: 'localhost' } }
    const state = { report, hooks: { path: '/test/.bravebot/hooks.json', text: '{"hooks":[]}', hooks: [] }, watches: [], replies: [], turns: 0, conflict: false }
    const emit = (event, data, session = 's-a') => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session })
    globalThis.agentSettingsFixture = { state, emit }
    const replace = (name, fn) => { ipcMain.removeHandler(name); ipcMain.handle(name, fn) }
    const rows = ['a', 'b'].map(id => ({ id, title: `Agent settings ${id}`, directory, project: 'Fixture', branch: null, updated: 1, bytes: 1 }))
    replace('bravebot:request', (_, method, params = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: report.build } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: 'local/test', record: { ...rows.find(r => r.id === params.id), turns: 0, tokens: 0 }, said: [], todos: {}, trust: { known: true, rules: [] }, archived: 0, contextTokens: 0 } }
      if (method === 'models.list') return { ok: { models: [{ id: 'local/test', name: 'Test model', provider: 'Local', premium: false, contextWindow: 32000 }], defaultModel: 'local/test', warnings: [] } }
      if (method === 'settings.inspect') return { ok: report }
      if (method === 'watches.add') { state.watches.push({ number: 1, path: params.path, state: 'watching', remainingSeconds: 604800, armedBy: 0 }) }
      if (method === 'watches.stop') state.watches = params.all ? [] : state.watches.filter(w => w.number !== params.number)
      if (method.startsWith('watches.')) return { ok: { watches: state.watches, busy: false } }
      if (method === 'turn.send') { emit('turn.started', { turn: ++state.turns }, params.session); return { ok: { turn: state.turns } } }
      if (method.endsWith('.reply')) state.replies.push({ method, params })
      return { ok: {} }
    })
    replace('bravebot:settings:select', (_, clear) => { report.selected = clear ? null : '/test/override.json'; report.layers = clear ? [] : [report.selected]; return report })
    replace('bravebot:hooks:read', () => state.hooks)
    replace('bravebot:hooks:save', (_, text, expected) => {
      if (state.conflict || expected !== state.hooks.text) throw new Error('Hooks changed on disk. Reload before saving.')
      state.hooks = { ...state.hooks, text, hooks: JSON.parse(text).hooks }; return state.hooks
    })
    replace('bravebot:files:list', () => ({ path: '', rows: [], truncated: false }))
  }, directory)
  const emit = (event, data, session = 's-a') => app.evaluate((_, args) => globalThis.agentSettingsFixture.emit(args.event, args.data, args.session), { event, data, session })
  const snap = name => page.screenshot({ path: join(output, name + '.png'), scale: 'css' })
  await page.reload()
  await page.getByRole('button', { name: 'Agent settings', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Agent settings', exact: true })
  await dialog.getByText('Model service configured', { exact: true }).waitFor()
  await dialog.getByText('Locked by your administrator: provider', { exact: true }).waitFor()
  await dialog.getByText('proxy.example:8080 · authenticated', { exact: true }).waitFor()
  await snap('01-connection')
  await dialog.getByRole('tab', { name: 'Run settings', exact: true }).click()
  await dialog.getByRole('button', { name: 'Choose settings file…' }).click()
  await dialog.getByRole('button', { name: 'Clear override' }).waitFor({ state: 'visible' })
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent === 'Clear override' && !b.disabled))
  await dialog.getByRole('button', { name: 'Clear override' }).click()
  await dialog.getByText('No override selected', { exact: true }).waitFor()
  await dialog.getByRole('tab', { name: 'Hooks', exact: true }).click()
  await dialog.getByText('No hooks configured.', { exact: true }).waitFor()
  await dialog.getByRole('button', { name: 'Add hook', exact: true }).click()
  await dialog.getByLabel('Program', { exact: true }).fill('/usr/bin/notify-send')
  await dialog.getByRole('button', { name: 'Add argument', exact: true }).click()
  await dialog.getByLabel('Argument 1', { exact: true }).fill('Done; $(never execute)')
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  assert.deepEqual(await app.evaluate(() => globalThis.agentSettingsFixture.state.hooks.hooks[0].run), ['/usr/bin/notify-send', 'Done; $(never execute)'])
  await snap('02-hooks')
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.conflict = true })
  await dialog.getByLabel('Program', { exact: true }).fill('/usr/bin/echo')
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'changed on disk' }).waitFor()
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.conflict = false })
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  await page.keyboard.press('Escape')
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(await page.getByRole('button', { name: 'Agent settings', exact: true }).evaluate(el => el === document.activeElement), true)
  await page.locator('.session').filter({ hasText: 'Agent settings a' }).click()
  await page.getByText('Context not yet measured', { exact: true }).waitFor()
  await page.getByRole('button', { name: 'Watches', exact: true }).click()
  const watches = page.getByRole('dialog', { name: 'File watches', exact: true })
  await watches.getByLabel('Project file', { exact: true }).fill('src/example.ts')
  await watches.getByRole('button', { name: 'Watch file', exact: true }).click()
  await watches.getByRole('button', { name: 'Stop watching src/example.ts', exact: true }).waitFor()
  await snap('03-watches')
  await watches.getByRole('button', { name: 'Stop all watches', exact: true }).click()
  await watches.getByText('No files watched.', { exact: false }).waitFor()
  await page.keyboard.press('Escape')
  await emit('watch.fired', { number: 1, path: 'src/example.ts' })
  await emit('turn.started', { turn: 1 })
  await page.getByText('File watch 1: src/example.ts', { exact: true }).waitFor()
  await emit('confirm.request', { request: 1, path: 'file.ts', intent: 'edit', untrusted: true, existing: true, added: 1, removed: 1, exact: true, changes: [{ kind: 'removed', text: 'old' }, { kind: 'added', text: 'new' }], remark: { preview: ['Fixed a typo'], lines: 5, label: 'untrusted' } })
  await page.getByText('Processor’s remark · untrusted', { exact: true }).waitFor()
  await page.getByText('4 more lines not shown', { exact: false }).waitFor()
  await page.getByRole('button', { name: 'Don’t write', exact: true }).click()
  await emit('output.request', { request: 2, command: 'cat notes.md', reference: '1', output: 'Quarantined output', lines: 1, summary: 'Read output', vetting: { verdict: 'unsafe', reason: 'Treat this as advice.' } })
  await page.locator('.confirm.output').getByText('Possible instructions detected', { exact: true }).waitFor()
  await page.locator('.confirm.output').getByRole('button', { name: 'Keep it out', exact: true }).click()
  await emit('vouch.request', { request: 3, path: 'source.md', preview: 'Partial file', truncated: true, vetting: { verdict: 'inconclusive', detail: 'Checker unavailable' } })
  await page.locator('.confirm.vouch').getByText('Check inconclusive', { exact: true }).waitFor()
  await page.locator('.confirm.vouch').getByText('Content may already have reached the backend.', { exact: false }).waitFor()
  await page.locator('.confirm.vouch').getByRole('button', { name: 'Leave it confined', exact: true }).click()
  await emit('vet.request', { request: 4, origin: 'notes.md', expects: 'project notes', content: '<script>untrusted words, rendered literally</script>\nSecond line', lines: 2, vetting: { verdict: 'unsafe', reason: 'Possible attempt to redirect the task.' } })
  await page.locator('.vetted-read').getByText('Possible instructions detected', { exact: true }).waitFor()
  await page.getByText('<script>untrusted words, rendered literally</script>\nSecond line', { exact: true }).waitFor()
  await snap('04-approval')
  await page.getByRole('button', { name: 'Let the planner read once', exact: true }).click()
  const replies = await app.evaluate(() => globalThis.agentSettingsFixture.state.replies)
  assert.equal(replies.at(-1).method, 'vet.reply')
  assert.equal(replies.at(-1).params.decision, 'approve')
  await emit('turn.done', { turn: 1, id: 'a', reply: 'Done', model: 'local/test', steps: 1, clean: true, tokens: 200, outputTokens: 20, contextTokens: 180, notices: ['Hook turn-finished: command failed'], trust: { rules: [] }, archived: 1 })
  await page.getByText('180 context tokens at last request', { exact: false }).waitFor()
  await page.getByText('Earlier context summarised', { exact: false }).waitFor()
  await emit('turn.started', { turn: 2 })
  await emit('turn.error', { turn: 2, kind: 'chat', category: 'transport', message: 'transport', attempts: 3, status: null, id: 'a' })
  await page.getByText('The model service could not be reached', { exact: true }).waitFor()
  await snap('05-failure')
  // Background approvals and cancellation stay bound to the originating session.
  await page.locator('.session').filter({ hasText: 'Agent settings b' }).click()
  await emit('turn.started', { turn: 3 })
  await emit('vet.request', { request: 3, origin: 'background.md', expects: 'notes', content: 'content', lines: 1, vetting: { verdict: 'safe' } })
  assert.equal(await page.getByText('background.md', { exact: true }).count(), 0)
  await page.locator('.session').filter({ hasText: 'Agent settings a' }).click()
  await page.getByText('background.md', { exact: true }).waitFor()
  await emit('turn.error', { turn: 3, kind: 'cancelled', message: 'cancelled', id: 'a' })
  assert.equal(await page.getByRole('button', { name: 'Let the planner read once', exact: true }).count(), 0)
  await page.setViewportSize({ width: 560, height: 780 })
  assert(await page.locator('.conversation-toolbar').evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'toolbar fits narrow layout')
  // The settings dialog is also accessible from the persistent sidebar on small screens.
  if (await page.locator('.app.left-folded').count()) await page.getByRole('button', { name: 'Session list', exact: true }).click()
  await page.getByRole('button', { name: 'Agent settings', exact: true }).click()
  await dialog.getByText('Model service configured', { exact: true }).waitFor()
  assert(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'dialog fits narrow layout')
  await snap('06-narrow-settings')
  assert.deepEqual(errors, [])
  console.log('PASS: connection and managed diagnostics, settings override, hook editing/conflicts, watches, automatic turns, approval evidence, background cancellation, context, stable failures, narrow layout and focus restoration')
} catch (error) { if (page) await page.screenshot({ path: join(output, 'failure.png') }); throw error }
finally { await app.close() }
