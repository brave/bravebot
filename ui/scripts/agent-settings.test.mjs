import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdtempSync, writeFileSync, readFileSync, rmSync, symlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(id => id === 'electron' ? { app: { getAppPath: () => process.cwd(), isPackaged: false } } : require(id), module, module.exports)
  return module.exports
}
const { composeHooks, unusableTimeout } = load('src/shared/agent-settings.ts')
const { saveHooks } = load('src/main/agent-settings.ts')
const t = load('src/renderer/transcript.ts')

test('an edit writes the words as typed and carries nothing the agent does not read', () => {
  const text = composeHooks([
    { on: 'tool-finished', tool: 'write_file', run: ['formatter', 'two words', '$(touch nope)', ''], timeout: 120, firesForNothing: false },
    { on: 'turn-started', tool: null, run: ['begin'], timeout: null, firesForNothing: false },
  ])
  // The agent's own answer about an entry is not written back into the file it was read from.
  assert.deepEqual(JSON.parse(text), {
    hooks: [
      { on: 'tool-finished', tool: 'write_file', run: ['formatter', 'two words', '$(touch nope)', ''], timeout: 120 },
      { on: 'turn-started', run: ['begin'] },
    ],
  })
})

test('a timeout the agent would drop the entry for is found before anything is written', () => {
  const range = { min: 1, max: 600 }
  const hook = timeout => ({ on: 'turn-finished', tool: null, run: ['echo'], timeout, firesForNothing: false })
  for (const bad of [0, 601, 1.5, -1, Number.NaN]) assert.equal(unusableTimeout([hook(null), hook(bad)], range), 1, `timeout ${bad}`)
  for (const good of [null, undefined, 1, 30, 600]) assert.equal(unusableTimeout([hook(good)], range), null, `timeout ${good}`)
  // The bounds are the ones the agent reported and not a copy held here.
  assert.equal(unusableTimeout([hook(700)], { min: 1, max: 900 }), null)
  assert.equal(unusableTimeout([hook(5)], { min: 10, max: 900 }), 0)
})

test('hook saves are explicit, reject conflicts and refuse symlinks', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-hooks-test-'))
  try {
    const path = join(directory, 'hooks.json')
    const text = composeHooks([{ on: 'turn-finished', tool: null, run: ['echo', 'literal;argument'], timeout: null, firesForNothing: false }])
    saveHooks(directory, text, null)
    assert.equal(readFileSync(path, 'utf8'), text)
    // `expected` is the text the agent last reported, and a file that has moved since is refused.
    assert.throws(() => saveHooks(directory, composeHooks([]), null), /changed since this editor opened/)
    assert.equal(readFileSync(path, 'utf8'), text)
    rmSync(path)
    const outside = join(directory, 'other.json')
    writeFileSync(outside, text)
    symlinkSync(outside, path)
    assert.throws(() => saveHooks(directory, composeHooks([]), text))
    assert.equal(readFileSync(outside, 'utf8'), text)
  } finally { rmSync(directory, { recursive: true, force: true }) }
})

test('vetted approval requires the same kind and cancellation makes it unanswerable', () => {
  const request = { request: 4, origin: 'notes', expects: 'notes', content: '<script>not executed</script>', lines: 1, vetting: { verdict: 'safe' } }
  const entries = [t.askedVet(request)]
  assert.equal(t.outstanding(entries).kind, 'vet')
  assert.equal(t.decide(entries, 'output', 4, 'approve')[0].decision, null)
  assert.equal(t.decide(entries, 'vet', 4, 'approve')[0].decision, 'approve')
  const cancelled = t.interruptPending(entries)
  assert.equal(t.outstanding(cancelled), null)
  assert.equal(t.decide(cancelled, 'vet', 4, 'approve')[0].decision, null)
  assert.deepEqual(t.conversation(entries), [], 'untrusted evidence is not exported as a conversation message')
})

test('watch turns are visibly automatic, including after reopening', () => {
  const entries = t.fromSaid([{ kind: 'watch', number: 3, path: 'src/a.ts' }])
  assert.equal(entries[0].kind, 'watch')
  assert.equal(t.beginTurn(entries, 2)[1].kind, 'turn-start')
  assert.deepEqual(t.conversation(entries), [])
  // Because the record says a watch fired, and not because of how the sentence reads. Typed into
  // the composer, the same words are a prompt and are drawn as one.
  const prompt = 'Watch 3 fired: src/a.ts looks written to since the last look.\n\nNothing has been read. Read the file if you need it.'
  assert.equal(t.fromSaid([{ kind: 'user', text: prompt }])[0].kind, 'user')
})

test('stable failure categories do not depend on service wording', () => {
  const { failureSummary } = load('src/renderer/failure.ts')
  assert.match(failureSummary('transport').description, /proxy and certificate/)
  assert.equal(failureSummary('cancelled').title, 'Task stopped')
  assert.equal(failureSummary('unauthorized').title, 'Authentication failed')
  assert.equal(failureSummary('model-unconfigured').title, 'Choose a model from your configured service')
  assert.match(failureSummary('unknown-future-category').title, /could not finish/)
})

// CHATGPT-8: the bridge sends the sentence for a failure of the ChatGPT account beside the category,
// and the window draws it as the description under one title. The window keeps no table of the
// sentences, so a category's card is replaced by the sentence and by nothing else.
test('a failure of the ChatGPT account is worded from the sentence the bridge sent', () => {
  const { failureSummary } = load('src/renderer/failure.ts')
  const sentence = 'The ChatGPT sign-in has ended, so run bravebot auth login chatgpt in a terminal to sign in again.'
  for (const category of ['unauthorized', 'refused', 'rate-limited', 'unavailable']) {
    const card = failureSummary(category, null, sentence)
    assert.equal(card.description, sentence, category)
    assert.notEqual(card.title, failureSummary(category).title, `${category} kept its own title`)
  }
  // Two sentences are two cards, and the category does not decide between them.
  assert.notDeepEqual(failureSummary('refused', null, 'one'), failureSummary('refused', null, 'two'))

  // Without a sentence the card says what the category says.
  for (const none of [undefined, null, '']) {
    assert.deepEqual(failureSummary('unauthorized', null, none), failureSummary('unauthorized'), String(none))
    assert.deepEqual(failureSummary('model-unconfigured', null, none), failureSummary('model-unconfigured'), String(none))
  }
})

// The Accounts block (CHATGPT-15, XAI-20, BACKEND-70): what each row says, and what can be done from it.
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')
const { loadComponents } = await import('./render-support.mjs')
const { AccountRowView, SignOutConfirm } = await loadComponents('src/renderer/components/Accounts.tsx')
const { ModelWarnings } = await loadComponents('src/renderer/components/ModelWarnings.tsx')
const { parseAccounts, signInWayOf } = load('src/shared/accounts.ts')

const account = (fields) => ({ way: 'gateway', id: null, name: 'Row', standing: 'notSignedIn', keyFrom: null, detail: null, command: null, canSignOut: false, host: null, ...fields })
const noop = () => {}
const rowProps = (row, more = {}) => ({ row, waiting: null, busy: false, signingOut: false, onSignIn: noop, onSignOut: noop, onShow: noop, onCancel: noop, ...more })
const drawRow = (row, more) => renderToStaticMarkup(React.createElement('ul', null, React.createElement(AccountRowView, rowProps(row, more))))
const standing = (html) => /data-test="account-standing">([^<]*)</.exec(html)?.[1]

/** The elements a component returned, depth first, without rendering anything that has hooks. */
function* elements(node) {
  if (Array.isArray(node)) { for (const each of node) yield* elements(each); return }
  if (!node || typeof node !== 'object' || !('props' in node)) return
  yield node
  yield* elements(node.props.children)
  for (const value of Object.values(node.props)) if (value && typeof value === 'object' && 'props' in value) yield* elements(value)
}
const byTest = (tree, name) => [...elements(tree)].find((each) => each.props['data-test'] === name)

test('each row says its standing, and a key says where it comes from', () => {
  assert.equal(standing(drawRow(account({ way: 'chatgpt', name: 'ChatGPT', standing: 'signedIn' }))), 'Signed in')
  assert.equal(standing(drawRow(account({ way: 'xai', standing: 'signedIn', keyFrom: 'stored' }))), 'Signed in')
  assert.equal(standing(drawRow(account({ way: 'bedrock', standing: 'signedIn' }))), 'Signed in')
  assert.equal(standing(drawRow(account({ way: 'brave', standing: 'signedIn', keyFrom: 'stored' }))), 'Key stored')
  assert.equal(standing(drawRow(account({ standing: 'signedIn', keyFrom: 'stored' }))), 'Key stored')
  assert.equal(standing(drawRow(account({ standing: 'signedIn', keyFrom: 'environment' }))), 'Key from the environment')
  assert.equal(standing(drawRow(account({ standing: 'signedIn', keyFrom: 'none' }))), 'No key needed')
  assert.equal(standing(drawRow(account({ standing: 'notSignedIn' }))), 'Not signed in')
  const unusable = drawRow(account({ way: 'xai', standing: 'unusable', detail: 'The stored xAI sign-in cannot be read.' }))
  assert.equal(standing(unusable), 'Can&#x27;t be used')
  assert.match(unusable, /<leo-alert data-test="account-detail">The stored xAI sign-in cannot be read\.<\/leo-alert>/)
  const misconfigured = drawRow(account({ standing: 'misconfigured', detail: 'settings.json is not JSON.' }))
  assert.equal(standing(misconfigured), 'The configuration can&#x27;t be read')
  assert.match(misconfigured, /<leo-alert data-test="account-detail">settings\.json is not JSON\.<\/leo-alert>/)
  assert.match(drawRow(account({ standing: 'notSignedIn', detail: 'There is no home directory.' })), /<span class="settings-caption" data-test="account-detail">There is no home directory\.<\/span>/)
})

test('ChatGPT, xAI and AWS sign in from the window; every other row shows its command', () => {
  for (const way of ['chatgpt', 'xai', 'bedrock']) {
    for (const standing of ['notSignedIn', 'unusable']) {
      const html = drawRow(account({ way, standing, command: `bravebot auth login ${way}` }))
      assert.match(html, /data-test="account-sign-in"/, `${way} ${standing}`)
      assert.doesNotMatch(html, /account-command/)
    }
    assert.doesNotMatch(drawRow(account({ way, standing: 'signedIn', command: `bravebot auth login ${way}` })), /account-sign-in|account-command/)
  }
  // The bridge gives an AWS row no command where signing in is not the remedy: no AWS CLI, or a
  // profile it does not have.
  assert.doesNotMatch(drawRow(account({ way: 'bedrock', id: 'work', standing: 'unusable', command: null, detail: 'The AWS CLI is not installed.' })), /account-sign-in/)
  const asked = []
  const tree = AccountRowView(rowProps(account({ way: 'bedrock', id: 'work', command: 'bravebot auth login bedrock' }), { onSignIn: (...args) => asked.push(args) }))
  byTest(tree, 'account-sign-in').props.onClick()
  assert.deepEqual(asked, [['bedrock', 'work']], 'the AWS row asks for its own profile')
  const gateway = drawRow(account({ id: 'openrouter', command: 'bravebot auth login gateway openrouter' }))
  assert.doesNotMatch(gateway, /account-sign-in/)
  assert.match(gateway, /With the bravebot command-line tool:<\/span> <span class="account-command-text"><code>bravebot auth login gateway openrouter<\/code>/)
  assert.doesNotMatch(drawRow(account({ way: 'brave', standing: 'signedIn', keyFrom: 'stored', command: 'bravebot auth login brave' })), /account-command/)
})

test('every row carries its mark, hidden from assistive technology, and its standing as a label', () => {
  const row = (fields) => AccountRowView(rowProps(account(fields)))
  const logo = (fields) => byTest(row(fields), 'account-logo')?.props
  const marks = [[{ way: 'chatgpt' }, 'openai'], [{ way: 'xai' }, 'xai'], [{ way: 'brave' }, 'social-brave-release-favicon-fullheight-color'], [{ way: 'bedrock' }, 'amazon-color'],
    [{ way: 'gateway', id: 'openrouter' }, 'openrouter'], [{ way: 'gateway', id: 'work' }, 'key'], [{ way: 'gateways' }, 'key']]
  for (const [fields, name] of marks) {
    assert.equal(logo(fields)?.name, name, JSON.stringify(fields))
    assert.equal(logo(fields)?.['aria-hidden'], 'true')
  }
  const colors = { signedIn: 'green', notSignedIn: 'neutral', unusable: 'yellow', misconfigured: 'red' }
  for (const [standing, color] of Object.entries(colors)) assert.equal(byTest(row({ standing }), 'account-standing').props.color, color, standing)
  assert.match(drawRow(account({ way: 'xai', name: 'xAI' })), /<span class="account-name">xAI<\/span><leo-label data-test="account-standing">Not signed in<\/leo-label>/)
})

// The key window (PREM-16, CRED-24): a row only asks for one, and the key never comes to the page.
const keyEntry = (more = {}) => ({ open: false, busy: false, onEnter: noop, onShow: noop, ...more })

test('a row that can take a key asks for its key window, and one with a key stored asks to replace it', () => {
  const asked = []
  const enter = (row) => byTest(AccountRowView(rowProps(row, { keyEntry: keyEntry({ onEnter: (...args) => asked.push(args) }) })), 'account-enter-key')
  const openrouter = enter(account({ id: 'openrouter', name: 'OpenRouter', canEnterKey: true }))
  assert.equal(openrouter.props.kind, 'filled')
  openrouter.props.onClick()
  const brave = enter(account({ way: 'brave', standing: 'signedIn', keyFrom: 'stored', canSignOut: true, canEnterKey: true }))
  assert.equal(brave.props.kind, 'outline')
  brave.props.onClick()
  assert.deepEqual(asked, [['gateway', 'openrouter'], ['brave', null]], 'the row says only which way, never a key')
  assert.match(drawRow(account({ id: 'openrouter', canEnterKey: true }), { keyEntry: keyEntry() }), /data-test="account-enter-key"[^>]*>Enter key</)
  assert.match(drawRow(account({ way: 'brave', canSignOut: true, canEnterKey: true }), { keyEntry: keyEntry() }), /data-test="account-enter-key"[^>]*>Replace key</)
  for (const row of [account({ canEnterKey: false }), account({ way: 'chatgpt', canEnterKey: true }), account({ way: 'bedrock', canEnterKey: true })]) {
    assert.doesNotMatch(drawRow(row, { keyEntry: keyEntry() }), /account-enter-key/, JSON.stringify(row))
  }
  assert.doesNotMatch(drawRow(account({ canEnterKey: true })), /account-enter-key/, 'nothing is offered without the gate')
  const command = 'bravebot auth login gateway openrouter'
  assert.doesNotMatch(drawRow(account({ id: 'openrouter', canEnterKey: true, command }), { keyEntry: keyEntry() }), /account-command/, 'Enter key stands in for the command')
  assert.match(drawRow(account({ id: 'openrouter', canEnterKey: true, command })), /account-command/, 'without the gate the command is still shown')
  assert.equal(enter(account({ canEnterKey: true, id: 'gw' })).props.isDisabled, false)
  assert.equal(byTest(AccountRowView(rowProps(account({ canEnterKey: true, id: 'gw' }), { keyEntry: keyEntry({ busy: true }) })), 'account-enter-key').props.isDisabled, true)
})

test('while its key window is open a row says so, brings it back, and is not signed out of', () => {
  let shown = 0
  const row = account({ way: 'brave', standing: 'signedIn', keyFrom: 'stored', canSignOut: true, canEnterKey: true })
  const tree = AccountRowView(rowProps(row, { keyEntry: keyEntry({ open: true, busy: true, onShow: () => { shown += 1 } }) }))
  assert.equal(byTest(tree, 'account-enter-key'), undefined)
  assert.equal(byTest(tree, 'account-key-open').props.children, 'Enter the key in its window.')
  byTest(tree, 'account-key-show').props.onClick()
  assert.equal(shown, 1)
  assert.equal(byTest(tree, 'account-sign-out').props.isDisabled, true)
})

test('what a key window came to is said with the row\'s name, and a key stored but not sent is a warning', async () => {
  const { KeyResultAlert } = await loadComponents('src/renderer/components/Accounts.tsx')
  const alert = (result) => renderToStaticMarkup(React.createElement(KeyResultAlert, { result: { way: 'gateway', id: 'openrouter', ...result }, name: 'OpenRouter' }))
  assert.match(alert({ outcome: 'stored', sentence: null }), /<leo-alert[^>]*>Key stored for OpenRouter\.<\/leo-alert>/)
  assert.match(alert({ outcome: 'stored', sentence: 'The key is stored, but…' }), /Key stored for OpenRouter\. The key is stored, but…/)
  assert.match(alert({ outcome: 'failed', sentence: 'An incognito session keeps nothing on disk.' }), /Couldn&#x27;t store the key for OpenRouter\. An incognito session/)
  const type = (result) => KeyResultAlert({ result: { way: 'brave', id: null, ...result }, name: 'Brave' }).props.type
  assert.deepEqual([type({ outcome: 'stored', sentence: null }), type({ outcome: 'stored', sentence: 'x' }), type({ outcome: 'failed', sentence: 'x' })], ['success', 'warning', 'error'])
})

test('an AWS row waits on its own sign-in, and coming back to the window reads no AWS row again', async () => {
  const source = readFileSync('src/renderer/components/Accounts.tsx', 'utf8')
  assert.match(source, /if \(rowsNow\.current\?\.some\(\(row\) => row\.way === 'bedrock'\)\) return/)
  assert.match(source, /signIn\.waiting\?\.way === current\.way && \(current\.way !== 'bedrock' \|\| signIn\.waiting\.id === current\.id\)/)
  const { signInSentence } = await loadComponents('src/renderer/components/SignInButton.tsx')
  assert.equal(signInSentence({ way: 'bedrock', id: null, outcome: 'signedIn', sentence: null }, 'AWS Bedrock'), 'Signed in to AWS Bedrock.')
  assert.equal(signInSentence({ way: 'bedrock', id: null, outcome: 'signedIn', sentence: 'Your settings set BRAVEBOT_USE_BEDROCK=1 now.' }, 'AWS Bedrock'),
    'Signed in to AWS Bedrock. Your settings set BRAVEBOT_USE_BEDROCK=1 now.')
})

test('Copy command copies exactly the command', () => {
  const command = 'bravebot auth login gateway openrouter'
  const copy = byTest(AccountRowView(rowProps(account({ command }))), 'account-copy')
  assert.equal(copy.props.text(), command)
  assert.equal(copy.props.label, 'Copy command')
})

test('Sign out is offered only where something is stored, and only asks', () => {
  for (const fields of [{ way: 'brave', standing: 'signedIn', keyFrom: 'stored' }, { standing: 'notSignedIn' }, { way: 'xai', standing: 'unusable' }]) {
    assert.match(drawRow(account({ ...fields, canSignOut: true })), /data-test="account-sign-out"/)
  }
  for (const fields of [{ standing: 'signedIn', keyFrom: 'environment' }, { standing: 'signedIn', keyFrom: 'none' }, { way: 'bedrock', standing: 'signedIn' }]) {
    assert.doesNotMatch(drawRow(account({ ...fields, canSignOut: false })), /account-sign-out/)
  }
  let asked = 0
  const tree = AccountRowView(rowProps(account({ standing: 'signedIn', keyFrom: 'stored', canSignOut: true }), { onSignOut: () => { asked += 1 } }))
  byTest(tree, 'account-sign-out').props.onClick()
  assert.equal(asked, 1, 'the button opens the question, and AccountRowView sends nothing itself')
})

test('the sign-out question forgets nothing unless it is answered Sign out', async () => {
  const row = account({ id: 'openrouter', name: 'OpenRouter', standing: 'signedIn', keyFrom: 'stored', canSignOut: true, host: 'openrouter.ai' })
  const html = renderToStaticMarkup(React.createElement(SignOutConfirm, { row, busy: false, onConfirm: noop, onCancel: noop }))
  assert.match(html, /data-title="Sign out of OpenRouter\?"/)
  assert.match(html, /bravebot forgets this sign-in on this computer\./)
  assert.match(html, /The key still works at openrouter\.ai until you revoke it there\./)
  assert.doesNotMatch(renderToStaticMarkup(React.createElement(SignOutConfirm, { row: { ...row, way: 'chatgpt', host: null }, busy: false, onConfirm: noop, onCancel: noop })), /sign-out-host/)

  const said = []
  const { Modal } = await loadComponents({ contents: "export { Modal } from './src/renderer/components/Modal'" }, [])
  const tree = SignOutConfirm({ row, busy: false, onConfirm: () => said.push('confirm'), onCancel: () => said.push('cancel') })
  assert.equal(tree.type.name, Modal.name)
  tree.props.onClose()
  byTest(tree, 'sign-out-cancel').props.onClick()
  assert.deepEqual(said, ['cancel', 'cancel'], 'Escape and Cancel only close it')
  byTest(tree, 'sign-out-confirm').props.onClick()
  assert.deepEqual(said, ['cancel', 'cancel', 'confirm'])
})

test('the list is read only where the answer is one', () => {
  const good = { accounts: [account({ way: 'chatgpt', name: 'ChatGPT', command: 'bravebot auth login chatgpt' })] }
  assert.equal(parseAccounts(good).length, 1)
  // A bridge from before the key window says nothing of it, which is no.
  assert.equal(parseAccounts(good)[0].canEnterKey, false)
  assert.equal(parseAccounts({ accounts: [{ ...good.accounts[0], canEnterKey: true }] })[0].canEnterKey, true)
  assert.deepEqual(parseAccounts({ accounts: [] }), [])
  for (const bad of [{}, null, undefined, { accounts: {} }, { accounts: [null] },
    { accounts: [{ ...good.accounts[0], way: 'chatgpt-2' }] }, { accounts: [{ ...good.accounts[0], standing: 'expired' }] },
    { accounts: [{ ...good.accounts[0], keyFrom: 'keychain' }] }, { accounts: [{ ...good.accounts[0], canSignOut: 'yes' }] },
    { accounts: [{ ...good.accounts[0], name: undefined }] }, { accounts: [{ ...good.accounts[0], detail: 4 }] }]) {
    assert.equal(parseAccounts(bad), null, JSON.stringify(bad))
  }
})

test('Sign in is offered beside a command the window can run itself, and no other', () => {
  assert.equal(signInWayOf('bravebot auth login chatgpt'), 'chatgpt')
  assert.equal(signInWayOf('bravebot auth login xai'), 'xai')
  for (const command of [null, undefined, '', 'bravebot auth login brave', 'bravebot auth login bedrock', 'bravebot auth login gateway xai', 'bravebot auth login xai --device', 'run bravebot auth login chatgpt']) {
    assert.equal(signInWayOf(command), null, String(command))
  }
  const html = renderToStaticMarkup(React.createElement(ModelWarnings, {
    warnings: ['Sign in to xAI: bravebot auth login xai', 'The gateway needs a key: bravebot auth login gateway g'],
    commands: ['bravebot auth login xai', 'bravebot auth login gateway g'],
  }))
  assert.equal(html.match(/data-test="sign-in-xai"/g)?.length, 1)
  assert.doesNotMatch(html, /sign-in-gateway/)
})

test('setting up a model service starts with the sign-ins and keys under Accounts, which is above it', async () => {
  const { SetupRoutes } = await loadComponents('src/renderer/components/ProvidersSettings.tsx')
  const html = renderToStaticMarkup(React.createElement(SetupRoutes))
  const headings = [...html.matchAll(/<h4>([^<]*)<\/h4>/g)].map((m) => m[1])
  assert.deepEqual(headings, ['Sign in to a service', 'A local model or another service', 'AWS Bedrock'])
  assert.match(html, /data-test="settings-setup-accounts">Sign in to a ChatGPT, xAI or AWS account, or enter a key for Brave Premium, OpenRouter or another gateway, under Accounts, above\.</)
  assert.match(html, /data-test="settings-setup-gateway-key">For a service that needs a key, enter it on its row under Accounts, or name/)
  const page = readFileSync('src/renderer/components/ProvidersSettings.tsx', 'utf8')
  assert.ok(page.indexOf('<Accounts ') < page.indexOf('<SetupRoutes />'), 'Accounts is drawn before the routes that point to it')
})

test('AWS Bedrock is set up by naming the account and signing in to it', async () => {
  const { SetupRoutes } = await loadComponents('src/renderer/components/ProvidersSettings.tsx')
  const html = renderToStaticMarkup(React.createElement(SetupRoutes))
  assert.doesNotMatch(html, /<pre>BRAVEBOT_USE_BEDROCK/, 'signing in records the switch, so it is not the first line to copy')
  assert.match(html, /data-test="settings-setup-bedrock">Then sign in to the profile under Accounts, which turns Bedrock on\. Where the profile needs no sign-in, set <code>BRAVEBOT_USE_BEDROCK=1<\/code> as well\./)
})

test('a sign-in or a key stored under Accounts reads the connection again as well as the models', () => {
  const page = readFileSync('src/renderer/components/ProvidersSettings.tsx', 'utf8')
  assert.match(page, /<Accounts session=\{session\} refresh=\{0\} onChanged=\{\(\) => \{ onChanged\(\); void load\(\) \}\} pick=\{pick\} \/>/)
})

test('a sign-in from a picker warning lists the models again, in the session the picker lists for', async () => {
  const signedIn = () => {}
  const tree = ModelWarnings({ warnings: ['Sign in to xAI'], commands: ['bravebot auth login xai'], session: 's1', onSignedIn: signedIn })
  const button = [...elements(tree)].find((each) => each.type?.name === 'SignInButton')
  assert.equal(button.props.onSignedIn, signedIn)
  assert.equal(button.props.session, 's1')
  const picker = readFileSync('src/renderer/components/ModelPicker.tsx', 'utf8')
  assert.match(picker, /<ModelWarnings [^>]*session=\{session\} onSignedIn=\{\(\) => setRevision\(\(n\) => n \+ 1\)\} \/>/)
})

test('a waiting sign-in announces its words and not its buttons', async () => {
  const { SignInWaiting } = await loadComponents('src/renderer/components/SignInButton.tsx')
  const html = renderToStaticMarkup(React.createElement(SignInWaiting, { waiting: { attempt: 'a1', way: 'chatgpt', id: null, started: true, port: null, until: null }, onShow: noop, onCancel: noop }))
  assert.equal(html.match(/role="status"/g)?.length, 1)
  assert.match(html, /<span role="status">[^<]*<\/span>/)
})

test('an ending for a row no longer listed is named for its account, not its way', () => {
  const source = readFileSync('src/renderer/components/Accounts.tsx', 'utf8')
  assert.match(source, /\?\? \(keyed\.way === 'brave' \? 'Brave Premium' : keyed\.id \?\? keyed\.way\)/)
  assert.match(source, /\?\? SIGN_IN_NAMES\[ended\.way\]/)
})
