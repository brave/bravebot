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
    const report = { build: 'bravebot 0.9.0 fixture', configured: true, problem: null, model: 'local/test', brave: false, bedrock: false, providers: [{ name: 'Local', key: 'gateway:local', credential: 'not required' }], selected: null, layers: [], overrides: [], managed: { path: '/etc/bravebot/managed.json', keys: ['provider'] }, network: { roots: ['/test/roots.pem'], problem: null, trustsNothing: false, proxy: 'proxy.example:8080', authenticated: true, unusableProxy: null, noProxy: 'localhost' } }
    // Figures that are not the agent's real ones, so a placeholder that held its own would not match them.
    const defaultFor = { 'turn-started': 41, 'tool-finished': 42, 'turn-finished': 43, 'prompt-shown': 44, 'checkout-made': 46, 'compacted': 47, 'session-started': 45, 'session-ended': 7 }
    const accounts = [
      { way: 'brave', id: null, name: 'Brave Premium', standing: 'signedIn', keyFrom: 'stored', detail: null, command: 'bravebot auth login brave', canSignOut: true, host: null, canEnterKey: true },
      { way: 'bedrock', id: 'dev', name: 'AWS Bedrock (dev)', standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login bedrock --profile dev', canSignOut: false, host: null, canEnterKey: false },
      { way: 'gateway', id: 'openrouter', name: 'OpenRouter', standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login gateway openrouter', canSignOut: false, host: 'openrouter.ai', canEnterKey: true },
      // A gateway the machine-level layer keeps from the key window, so its row shows the command.
      { way: 'gateway', id: 'corp', name: 'Corp gateway', standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login gateway corp', canSignOut: false, host: 'llm.corp.example', canEnterKey: false },
      { way: 'gateway', id: 'local', name: 'Local', standing: 'signedIn', keyFrom: 'none', detail: null, command: null, canSignOut: false, host: null, canEnterKey: false },
      { way: 'chatgpt', id: null, name: 'ChatGPT', standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login chatgpt', canSignOut: false, host: null, canEnterKey: false },
      { way: 'xai', id: null, name: 'xAI', standing: 'unusable', keyFrom: null, detail: 'The stored xAI sign-in cannot be read. Sign in again.', command: 'bravebot auth login xai', canSignOut: true, host: null, canEnterKey: false },
    ]
    const state = { report, defaultFor, accounts, signOuts: [], signIns: [], cancels: [], keys: [], hooks: { path: '/test/.bravebot/hooks.json', text: '{"hooks":[]}', entire: true, hooks: [] }, watches: [], replies: [], turns: 0, conflict: false, unreadable: false }
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
      if (method === 'accounts.list') return { ok: { accounts: state.accounts } }
      if (method === 'accounts.signOut') {
        state.signOuts.push(params)
        state.accounts = state.accounts.map(row => row.way === params.way && row.id === params.id ? { ...row, standing: 'notSignedIn', keyFrom: null, detail: null, canSignOut: false } : row)
        return { ok: { accounts: state.accounts } }
      }
      // Stands in for the agent's reader: the panel is told what a hook is rather than deciding.
      if (method === 'hooks.inspect') return state.unreadable ? { error: { code: 'internal', message: 'The hooks file could not be read back.' } } : { ok: { timeoutSeconds: { min: 1, max: 600, defaultFor: state.defaultFor }, ...state.hooks } }
      if (method === 'watches.add') { state.watches.push({ number: 1, path: params.path, state: 'watching', remainingSeconds: 604800, armedBy: 0 }) }
      if (method === 'watches.stop') state.watches = params.all ? [] : state.watches.filter(w => w.number !== params.number)
      if (method.startsWith('watches.')) return { ok: { watches: state.watches, busy: false } }
      if (method === 'turn.send') { emit('turn.started', { turn: ++state.turns }, params.session); return { ok: { turn: state.turns } } }
      if (method.endsWith('.reply')) state.replies.push({ method, params })
      return { ok: {} }
    })
    replace('bravebot:settings:select', (_, clear) => { report.selected = clear ? null : '/test/override.json'; report.layers = clear ? [] : [report.selected]; return report })
    replace('bravebot:hooks:save', (_, text, expected) => {
      if (state.conflict || expected !== state.hooks.text) throw new Error('File changed since this editor opened. Reopen it to review the latest version.')
      state.hooks = { ...state.hooks, text, hooks: JSON.parse(text).hooks.map(hook => ({ tool: null, ...hook, firesForNothing: Boolean(hook.tool) && hook.on !== 'tool-finished' })) }
    })
    // The gate is the main process's and has unit tests of its own; here the page's side is driven.
    const notice = n => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:signIn:event', n)
    globalThis.agentSettingsFixture.notice = notice
    replace('bravebot:signIn', (_, way, id) => { state.signIns.push(id ? `${way}:${id}` : way); return { attempt: `t${state.signIns.length}` } })
    replace('bravebot:signIn:current', () => null)
    replace('bravebot:signIn:show', () => true)
    replace('bravebot:signIn:cancel', (_, attempt) => { state.cancels.push(attempt); notice({ kind: 'ended', attempt, way: null, id: null, outcome: 'cancelled', sentence: null }) })
    // The key window is the main process's; the page is only told it opened and how it ended.
    const keyNotice = n => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:enterKey:event', n)
    globalThis.agentSettingsFixture.keyNotice = keyNotice
    replace('bravebot:enterKey', (_, way, id) => { state.keys.push(id ? `${way}:${id}` : way); keyNotice({ kind: 'opened', way, id }); return { opened: true } })
    replace('bravebot:enterKey:current', () => null)
    replace('bravebot:enterKey:show', () => true)
    replace('bravebot:files:list', () => ({ path: '', rows: [], truncated: false }))
  }, directory)
  const emit = (event, data, session = 's-a') => app.evaluate((_, args) => globalThis.agentSettingsFixture.emit(args.event, args.data, args.session), { event, data, session })
  const snap = async name => {
    // Leo's buttons fade between disabled and enabled; let that finish so a shot isn't caught halfway.
    await page.waitForTimeout(500)
    await page.screenshot({ path: join(output, name + '.png'), scale: 'css' })
  }
  await page.reload()
  // Settings is a page that takes the window, not a dialog. It opens on General, and the agent's
  // configuration is the page beside it, with Connection, Hooks and Run settings stacked.
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  const dialog = page.locator('[data-test="settings-view"]')
  await dialog.waitFor()
  await dialog.locator('[data-test="settings-page-agent"]').click()
  for (const name of ['Connection', 'Hooks', 'Run settings']) await dialog.getByRole('heading', { name, exact: true, level: 2 }).waitFor()
  await dialog.getByText('Locked by your administrator: provider', { exact: true }).waitFor()
  await dialog.getByText('proxy.example:8080 · authenticated', { exact: true }).waitFor()
  await snap('01-connection')
  // The model service and the accounts are the Providers page, the second in the list.
  assert.deepEqual(await dialog.locator('[data-test^="settings-page-"]').evaluateAll((items) => items.map((item) => item.dataset.test)),
    ['settings-page-general', 'settings-page-providers', 'settings-page-connectors', 'settings-page-agent'])
  assert.equal(await dialog.getByText('Model service configured', { exact: true }).count(), 0, 'the model service is not on Agent settings')
  await dialog.locator('[data-test="settings-page-providers"]').click()
  await dialog.getByRole('heading', { name: 'Model service configured', exact: true, level: 2 }).waitFor()
  await dialog.getByText('Credentials not required', { exact: true }).waitFor()

  // Accounts: one row per sign-in, what can be done about each, and nothing stored shown.
  const fixture = () => app.evaluate(() => globalThis.agentSettingsFixture.state)
  const accounts = dialog.locator('[data-test="accounts"]')
  // Only the account chosen in the list is drawn beside it, so a row is chosen before it is read.
  const row = name => accounts.locator(`[data-test="account-${name}"]`)
  const pick = async name => {
    await accounts.locator(`[data-test="provider-${name}"]`).click()
    await row(name).waitFor()
  }
  await dialog.getByRole('heading', { name: 'Accounts', exact: true, level: 2 }).waitFor()
  await pick('brave:')
  assert.equal(await row('brave:').locator('[data-test="account-standing"]').textContent(), 'Key stored')
  await pick('gateway:local')
  assert.equal(await row('gateway:local').locator('[data-test="account-standing"]').textContent(), 'No key needed')
  await pick('xai:')
  assert.equal(await row('xai:').locator('[data-test="account-standing"]').textContent(), "Can't be used")
  await row('xai:').getByText('The stored xAI sign-in cannot be read. Sign in again.').waitFor()
  await pick('gateway:corp')
  assert.equal(await row('gateway:corp').locator('code').textContent(), 'bravebot auth login gateway corp')
  await pick('gateway:openrouter')
  assert.equal(await row('gateway:openrouter').locator('[data-test="account-command"]').count(), 0, 'a row that takes a key in the window shows no command')
  await row('gateway:openrouter').locator('[data-test="account-enter-key"]').waitFor()
  await pick('chatgpt:')
  assert.equal(await row('chatgpt:').locator('[data-test="account-command"]').count(), 0, 'ChatGPT signs in from the window, so it shows no command')
  await pick('bedrock:dev')
  assert.equal(await row('bedrock:dev').locator('[data-test="account-command"]').count(), 0, 'an AWS account signs in from the window, so it shows no command')
  await pick('gateway:local')
  assert.equal(await row('gateway:local').locator('[data-test="account-sign-out"]').count(), 0, 'nothing stored, nothing to sign out of')
  await pick('gateway:corp')
  await row('gateway:corp').locator('[data-test="account-copy"]').scrollIntoViewIfNeeded()
  await snap('01b-accounts')
  await app.evaluate(({ clipboard }) => clipboard.writeText('before'))
  await row('gateway:corp').locator('[data-test="account-copy"]').click()
  for (let tries = 0; tries < 50 && (await app.evaluate(({ clipboard }) => clipboard.readText())) === 'before'; tries++) await page.waitForTimeout(100)
  assert.equal(await app.evaluate(({ clipboard }) => clipboard.readText()), 'bravebot auth login gateway corp', 'Copy writes the command and nothing else')

  // Enter key only asks the main process, which opens a key window of its own; the key never reaches the page.
  const keyNotice = n => app.evaluate((_, n) => globalThis.agentSettingsFixture.keyNotice(n), n)
  await pick('gateway:openrouter')
  await row('gateway:openrouter').locator('[data-test="account-enter-key"]').click()
  await row('gateway:openrouter').locator('[data-test="account-key-open"]').waitFor()
  assert.equal(await row('gateway:openrouter').locator('[data-test="account-key-open"]').textContent(), 'Enter the key in its window.')
  assert.deepEqual((await fixture()).keys, ['gateway:openrouter'])
  await snap('01b2-key-window-open')
  await keyNotice({ kind: 'ended', way: 'gateway', id: 'openrouter', outcome: 'stored', sentence: null })
  await accounts.getByText('Key stored for OpenRouter.', { exact: true }).waitFor()
  await row('gateway:openrouter').locator('[data-test="account-enter-key"]').waitFor()

  // Sign out asks first; Escape and Cancel forget nothing.
  await pick('brave:')
  await row('brave:').locator('[data-test="account-sign-out"]').click()
  const confirm = page.locator('.confirm-sign-out')
  await confirm.getByText('bravebot forgets this sign-in on this computer.', { exact: false }).waitFor()
  await snap('01c-sign-out')
  await page.keyboard.press('Escape')
  await confirm.waitFor({ state: 'detached' })
  await row('brave:').locator('[data-test="account-sign-out"]').click()
  await confirm.locator('[data-test="sign-out-cancel"]').click()
  await confirm.waitFor({ state: 'detached' })
  assert.deepEqual((await fixture()).signOuts, [], 'nothing is forgotten without a yes')
  await row('brave:').locator('[data-test="account-sign-out"]').click()
  await confirm.locator('[data-test="sign-out-confirm"]').click()
  await accounts.getByText('Signed out of Brave Premium.', { exact: true }).waitFor()
  assert.deepEqual((await fixture()).signOuts.map(({ way, id }) => ({ way, id })), [{ way: 'brave', id: null }])
  assert.equal(await row('brave:').locator('[data-test="account-standing"]').textContent(), 'Not signed in')

  // Sign in only asks the main process. The page is told the port and how long it waits, never an address.
  const notice = n => app.evaluate((_, n) => globalThis.agentSettingsFixture.notice(n), n)
  await pick('chatgpt:')
  await row('chatgpt:').locator('[data-test="account-sign-in"]').click()
  await row('chatgpt:').locator('[data-test="sign-in-waiting-chatgpt"]').waitFor()
  assert.deepEqual((await fixture()).signIns, ['chatgpt'])
  await notice({ kind: 'started', attempt: 't1', way: 'chatgpt', id: null, port: 1455, expiresIn: 600 })
  await row('chatgpt:').getByText('Port 1455', { exact: true }).waitFor()
  await row('chatgpt:').locator('[data-test="sign-in-show"]').waitFor()
  await snap('01d-waiting')
  await row('chatgpt:').locator('[data-test="sign-in-cancel"]').click()
  await accounts.getByText('Sign-in cancelled.', { exact: true }).waitFor()
  assert.deepEqual((await fixture()).cancels, ['t1'])
  await snap('01e-cancelled')

  await pick('xai:')
  await row('xai:').locator('[data-test="account-sign-in"]').click()
  await row('xai:').getByText('Requesting a code…', { exact: true }).waitFor()
  await snap('01f-requesting-code')
  await notice({ kind: 'ended', attempt: 't2', way: 'xai', id: null, outcome: 'failed', sentence: 'xAI refused the sign-in. Try again later.' })
  await accounts.getByText('xAI refused the sign-in. Try again later.', { exact: true }).waitFor()
  await row('xai:').locator('[data-test="account-sign-in"]').waitFor()
  await snap('01g-failed')

  // An AWS account is signed in to by its profile, and waits on its own row.
  await pick('bedrock:dev')
  await row('bedrock:dev').locator('[data-test="account-sign-in"]').click()
  await row('bedrock:dev').locator('[data-test="sign-in-waiting-bedrock"]').waitFor()
  assert.deepEqual((await fixture()).signIns, ['chatgpt', 'xai', 'bedrock:dev'])
  await notice({ kind: 'started', attempt: 't3', way: 'bedrock', id: 'dev', port: null, expiresIn: 600 })
  await row('bedrock:dev').locator('[data-test="sign-in-show"]').waitFor()
  await snap('01h-aws-waiting')
  await notice({ kind: 'ended', attempt: 't3', way: 'bedrock', id: 'dev', outcome: 'signedIn', sentence: null })
  await accounts.getByText('Signed in to AWS Bedrock (dev).', { exact: true }).waitFor()
  await dialog.locator('[data-test="settings-page-agent"]').click()
  await dialog.getByRole('button', { name: 'Choose settings file…' }).click()
  await dialog.getByRole('button', { name: 'Clear override' }).waitFor({ state: 'visible' })
  // Leo draws the button inside a shadow root, so wait on the role query, which pierces it.
  for (let tries = 0; tries < 100 && !(await dialog.getByRole('button', { name: 'Clear override' }).isEnabled()); tries++) await page.waitForTimeout(100)
  await dialog.getByRole('button', { name: 'Clear override' }).click()
  await dialog.getByText('No override selected', { exact: true }).waitFor()
  await dialog.getByText('No hooks configured.', { exact: true }).waitFor()
  await dialog.getByRole('button', { name: 'Add hook', exact: true }).click()
  // The timeout field shows the bound the agent reported for the entry's moment, not a figure of the panel's own.
  const placeholder = () => dialog.locator('[data-test="hook-timeout-0"]').evaluate(field => field.placeholder)
  const placeholderBecomes = async expected => {
    for (let tries = 0; tries < 50 && (await placeholder()) !== expected; tries++) await page.waitForTimeout(100)
    assert.equal(await placeholder(), expected)
  }
  await dialog.locator('[data-test="hook-when-0"]').click()
  await page.getByRole('option', { name: 'Question waiting', exact: true }).click()
  await placeholderBecomes('44')
  // The desktop app fires both session moments, so a hook on one carries no 'does not fire this moment yet' note.
  await dialog.locator('[data-test="hook-when-0"]').click()
  await page.getByRole('option', { name: 'Session ends', exact: true }).click()
  await placeholderBecomes('7')
  assert.equal(await dialog.getByText('does not fire this moment yet').count(), 0)
  await dialog.locator('[data-test="hook-when-0"]').click()
  await page.getByRole('option', { name: 'Turn ends', exact: true }).click()
  await placeholderBecomes('43')
  // An entry with no program is one the agent reads no hook out of, and a file this panel wrote and
  // the agent cannot read whole is a file this panel then refuses to edit.
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'Enter a program for every hook' }).waitFor()
  await dialog.getByRole('textbox', { name: 'Program', exact: true }).fill('/usr/bin/notify-send')
  await dialog.getByRole('button', { name: 'Add argument', exact: true }).click()
  await dialog.getByRole('textbox', { name: 'Argument 1', exact: true }).fill('Done; $(never execute)')
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  assert.deepEqual(await app.evaluate(() => globalThis.agentSettingsFixture.state.hooks.hooks[0].run), ['/usr/bin/notify-send', 'Done; $(never execute)'])
  await snap('02-hooks')
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.conflict = true })
  await dialog.getByRole('textbox', { name: 'Program', exact: true }).fill('/usr/bin/echo')
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'changed since this editor opened' }).waitFor()
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.conflict = false })
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  // A write that landed and a reading that did not is not a stale editor: what the next save
  // expects to find is the text this one wrote.
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.unreadable = true })
  await dialog.getByRole('textbox', { name: 'Program', exact: true }).fill('/usr/bin/true')
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'could not be read back' }).waitFor()
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.unreadable = false })
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  // An entry naming a tool on a turn moment is inert, and the agent is what says so.
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.hooks = { path: '/test/.bravebot/hooks.json', text: '{"hooks":[{"on":"turn-started","tool":"write_file","run":["fmt"]}]}', entire: true, hooks: [{ on: 'turn-started', tool: 'write_file', run: ['fmt'], firesForNothing: true }] } })
  await dialog.getByRole('button', { name: 'Reload hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'This hook fires for nothing' }).waitFor()
  // Taking the advice keeps the filter the notice was about. The notice is the agent's answer about
  // the file on disk, so an edit clears it until the file has been read again.
  await dialog.locator('[data-test="hook-when-0"]').click()
  await page.getByRole('option', { name: 'Tool finishes', exact: true }).click()
  assert.equal(await dialog.getByRole('textbox', { name: 'Tool filter (optional)', exact: true }).inputValue(), 'write_file', 'choosing the moment the filter works at keeps the filter')
  await dialog.getByRole('alert').filter({ hasText: 'This hook fires for nothing' }).waitFor({ state: 'detached' })
  await dialog.getByRole('button', { name: 'Save hooks', exact: true }).click()
  await dialog.getByText('Hooks saved. They apply when the next turn starts.').waitFor()
  assert.deepEqual(await app.evaluate(() => globalThis.agentSettingsFixture.state.hooks.hooks[0]), { on: 'tool-finished', tool: 'write_file', run: ['fmt'], firesForNothing: false })
  // A file the agent only partly read is the person's to edit: a form composing it back from the
  // entries it did read would drop the rest.
  await app.evaluate(() => { globalThis.agentSettingsFixture.state.hooks = { path: '/test/.bravebot/hooks.json', text: '{"hooks":[{"on":"turn-started","run":["x"]}],"later":true}', entire: false, hooks: [{ on: 'turn-started', tool: null, run: ['x'], firesForNothing: false }] } })
  await dialog.getByRole('button', { name: 'Reload hooks', exact: true }).click()
  await dialog.getByRole('alert').filter({ hasText: 'did not read all of this file' }).waitFor()
  assert.equal(await dialog.getByRole('textbox', { name: 'Program', exact: true }).isDisabled(), true, 'the entries of a file the agent did not wholly read cannot be edited')
  for (const name of ['Add hook', 'Save hooks']) {
    assert.equal(await dialog.getByRole('button', { name, exact: true }).isDisabled(), true, `${name} is refused for a file the agent did not wholly read`)
  }
  await snap('02b-hooks-unread')
  // Escape leaves the settings, and focus goes back to the row that opened them.
  await page.keyboard.press('Escape')
  await dialog.waitFor({ state: 'detached' })
  // Focus is on the button Leo draws inside the host, which the page reports as the host itself.
  assert.equal(await page.locator('.agent-settings-open').evaluate(el => el === document.activeElement || el.getRootNode().host === document.activeElement), true)
  await page.locator('.session').filter({ hasText: 'Agent settings a' }).click()
  await page.getByText('Context not yet measured', { exact: true }).waitFor()
  await page.getByRole('button', { name: 'More', exact: true }).click(); await page.getByRole('menuitem', { name: 'File watches…', exact: true }).click()
  const watches = page.getByRole('dialog', { name: 'File watches', exact: true })
  await watches.getByRole('textbox', { name: 'Project file', exact: true }).fill('src/example.ts')
  await watches.getByRole('button', { name: 'Watch file', exact: true }).click()
  await watches.getByRole('button', { name: 'Stop watching src/example.ts', exact: true }).waitFor()
  await snap('03-watches')
  await watches.getByRole('button', { name: 'Stop all watches', exact: true }).click()
  await watches.getByText('No files watched.', { exact: false }).waitFor()
  await page.keyboard.press('Escape')
  await emit('watch.fired', { number: 1, path: 'src/example.ts' })
  await emit('turn.started', { turn: 1 })
  await page.getByText('File watch 1: src/example.ts', { exact: true }).waitFor()
  await emit('confirm.request', { request: 1, path: 'file.ts', shownPath: 'file.ts', intent: 'edit', untrusted: true, existing: true, added: 1, removed: 1, exact: true, changes: [{ kind: 'removed', text: 'old' }, { kind: 'added', text: 'new' }], remark: { preview: ['Fixed a typo'], lines: 5, label: 'untrusted' } })
  await page.getByText('Processor’s remark · untrusted', { exact: true }).waitFor()
  await page.getByText('4 more lines not shown', { exact: false }).waitFor()
  await page.getByRole('button', { name: 'Don’t write', exact: true }).click()
  await emit('output.request', { request: 2, command: 'cat notes.md', reference: '1', output: 'Quarantined output', lines: 1, summary: 'Read output', vetting: { verdict: 'unsafe', reason: 'Treat this as advice.' } })
  await page.locator('.confirm.output').getByText('Possible instructions detected', { exact: true }).waitFor()
  await page.locator('.confirm.output').getByRole('button', { name: 'Keep it out', exact: true }).click()
  await emit('vouch.request', { request: 3, path: 'source.md', shownPath: 'source.md', rule: '/work/source.md', shownRule: '/work/source.md', preview: 'Partial file', truncated: true, vetting: { verdict: 'inconclusive', detail: 'Checker unavailable' } })
  await page.locator('.confirm.vouch').getByText('Check inconclusive', { exact: true }).waitFor()
  await page.locator('.confirm.vouch .card-details summary').click()
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
  // Polled rather than read once: the cancellation reaches the renderer over the event channel, so
  // counting on the next line is a race this script used to win by accident.
  await page.waitForFunction(() => ![...document.querySelectorAll('button')].some(b => b.textContent === 'Let the planner read once'))
  await page.setViewportSize({ width: 560, height: 780 })
  assert(await page.locator('.conversation-toolbar').evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'toolbar fits narrow layout')
  // The settings are also reachable from the persistent sidebar on small screens, and open on the
  // page used last.
  if (await page.locator('.app.left-folded').count()) await page.getByRole('button', { name: 'Chat list', exact: true }).click()
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  await dialog.getByText('Model service configured', { exact: true }).waitFor()
  assert(await dialog.locator('.settings-main').evaluate(el => el.scrollWidth <= el.clientWidth + 1), 'settings page fits narrow layout')
  await snap('06-narrow-settings')
  assert.deepEqual(errors, [])
  console.log('PASS: connection and managed diagnostics, settings override, hook editing/conflicts, watches, automatic turns, approval evidence, background cancellation, context, stable failures, narrow layout and focus restoration')
} catch (error) { if (page) await page.screenshot({ path: join(output, 'failure.png') }); throw error }
finally { await app.close() }
