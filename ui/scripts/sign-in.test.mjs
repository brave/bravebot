// The gate in front of the window's sign-in to the ChatGPT and xAI accounts and an AWS account
// (CHATGPT-15..17, XAI-20..22, BACKEND-70): the page only asks, the main process asks the person in a dialog of its own, and the
// address, URL and code stay with the main process and its sign-in window.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)

function load(entry) {
  const source = buildSync({
    stdin: { contents: `export * from '${entry}'\n`, resolveDir: process.cwd(), loader: 'ts' },
    bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { SignIn, COOLDOWN_MS, dialogOptions, mayOpen, opensExternally, SIGN_IN_ORIGINS, AWS_SIGN_IN_HOSTS } = load('./src/main/sign-in')
const { sanitised } = load('./src/main/sanitise')

const row = (way, canSignOut) => ({ way, id: null, name: way, standing: canSignOut ? 'signedIn' : 'notSignedIn', keyFrom: null, detail: null, command: null, canSignOut, host: null })

/** A gate with the bridge, the dialog, the window and the clock stubbed. */
function gate({ answer = 0, stored = [], attempt = 'a1', origins = SIGN_IN_ORIGINS } = {}) {
  const webContents = {}
  const window = { webContents, focused: true, destroyed: false, isFocused() { return this.focused }, isDestroyed() { return this.destroyed } }
  const sent = []
  const dialogs = []
  const told = []
  const opened = []
  const shown = []
  const updated = []
  const stub = {
    clock: 1_000_000, changed: 0, closed: 0, answer, otherBusy: false,
    rows: { accounts: stored },
    started: { attempt },
  }
  const signIn = new SignIn({
    ask: async (method, params) => {
      sent.push({ method, params })
      if (method === 'accounts.list') return stub.rows
      if (method === 'accounts.signIn') return stub.started
      return {}
    },
    window: () => window,
    showMessageBox: async (_window, options) => { dialogs.push(options); return { response: typeof stub.answer === 'function' ? await stub.answer() : stub.answer } },
    openExternal: async (url) => { opened.push(url) },
    tell: (notice) => told.push(notice),
    showWindow: (state) => shown.push(state),
    updateWindow: (state) => updated.push(state),
    closeWindow: () => { stub.closed += 1 },
    changed: () => { stub.changed += 1 },
    origins: () => origins,
    otherBusy: () => stub.otherBusy,
    now: () => stub.clock,
  })
  return { signIn, window, webContents, sent, dialogs, told, opened, shown, updated, stub }
}

const signIns = (sent) => sent.filter((each) => each.method === 'accounts.signIn')

test('accounts.signIn and accounts.cancelSignIn are not methods the page may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)[1]
  assert.match(allowed, /'accounts\.list'/)
  assert.match(allowed, /'accounts\.signOut'/)
  assert.doesNotMatch(allowed, /accounts\.signIn|accounts\.cancelSignIn/)
})

test('a sign-in is sent only on a yes in the dialog', async () => {
  const no = gate({ answer: 1 })
  assert.equal(await no.signIn.request(no.webContents, 'chatgpt'), null)
  assert.equal(no.dialogs.length, 1)
  assert.deepEqual(signIns(no.sent), [], 'a no sends nothing')

  const yes = gate({ answer: 0 })
  assert.deepEqual(await yes.signIn.request(yes.webContents, 'xai'), { attempt: 'a1' })
  assert.deepEqual(signIns(yes.sent), [{ method: 'accounts.signIn', params: { way: 'xai', replace: false } }])
})

test('Cancel is the default and the answer to Escape, and the words are English', () => {
  for (const way of ['chatgpt', 'xai']) {
    const options = dialogOptions(way, false)
    assert.equal(options.buttons[options.defaultId], 'Cancel')
    assert.equal(options.buttons[options.cancelId], 'Cancel')
    assert.equal(options.buttons[0], 'Sign in')
  }
  assert.equal(dialogOptions('chatgpt', false).message, 'Sign in to ChatGPT?')
  assert.match(dialogOptions('chatgpt', false).detail, /^A browser opens at OpenAI\. The sign-in is kept on this computer\.$/)
  assert.match(dialogOptions('xai', true).detail, /A code is shown to enter at xAI\.[\s\S]*This replaces the xAI sign-in stored on this computer\./)
})

test('an ask is dropped for an unknown way, a foreign sender and an unfocused window', async () => {
  for (const way of ['brave', 'gateway', 'Bedrock', 'ChatGPT', ' xai', '', null, { way: 'xai' }]) {
    const g = gate()
    assert.equal(await g.signIn.request(g.webContents, way), null, `way ${JSON.stringify(way)}`)
    assert.equal(g.dialogs.length, 0)
    assert.deepEqual(g.sent, [])
  }
  const foreign = gate()
  assert.equal(await foreign.signIn.request({}, 'chatgpt'), null)
  assert.equal(foreign.dialogs.length, 0)

  const unfocused = gate()
  unfocused.window.focused = false
  assert.equal(await unfocused.signIn.request(unfocused.webContents, 'chatgpt'), null)
  assert.equal(unfocused.dialogs.length, 0)
})

test('one dialog or attempt at a time, for either way', async () => {
  const g = gate()
  let release
  g.stub.answer = () => new Promise((resolve) => { release = resolve })
  const first = g.signIn.request(g.webContents, 'chatgpt')
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(await g.signIn.request(g.webContents, 'xai'), null, 'a second ask while the dialog is up')
  assert.equal(g.dialogs.length, 1)
  release(0)
  assert.deepEqual(await first, { attempt: 'a1' })
  g.stub.answer = 0
  assert.equal(await g.signIn.request(g.webContents, 'chatgpt'), null, 'and while the attempt runs')
  assert.equal(g.dialogs.length, 1)

  g.signIn.handle({ event: 'accounts.signIn.ended', data: { attempt: 'a1', outcome: 'cancelled' } })
  g.stub.started = { attempt: 'a2' }
  assert.deepEqual(await g.signIn.request(g.webContents, 'xai'), { attempt: 'a2' }, 'an ended attempt frees the gate')
})

test('an ask in the seconds after a no is dropped', async () => {
  const g = gate({ answer: 1 })
  await g.signIn.request(g.webContents, 'chatgpt')
  g.stub.answer = 0
  g.stub.clock += COOLDOWN_MS - 1
  assert.equal(await g.signIn.request(g.webContents, 'chatgpt'), null)
  assert.equal(g.dialogs.length, 1)
  g.stub.clock += 2
  assert.deepEqual(await g.signIn.request(g.webContents, 'chatgpt'), { attempt: 'a1' })
})

test('replace comes from what the bridge lists, not from the page', async () => {
  const g = gate({ stored: [row('chatgpt', true), row('xai', false)] })
  await g.signIn.request(g.webContents, 'chatgpt', { replace: false })
  assert.deepEqual(g.sent[0], { method: 'accounts.list', params: { bedrock: false } }, 'the AWS rows are not checked for a ChatGPT ask')
  assert.deepEqual(signIns(g.sent)[0].params, { way: 'chatgpt', replace: true })
  assert.match(g.dialogs[0].detail, /This replaces the ChatGPT sign-in/)

  const other = gate({ stored: [row('chatgpt', true), row('xai', false)] })
  await other.signIn.request(other.webContents, 'xai')
  assert.deepEqual(signIns(other.sent)[0].params, { way: 'xai', replace: false })
  assert.doesNotMatch(other.dialogs[0].detail, /replaces/)
})

test('an answer that is not a list asks nobody', async () => {
  const g = gate()
  g.stub.rows = {}
  assert.ok('error' in await g.signIn.request(g.webContents, 'chatgpt'))
  assert.equal(g.dialogs.length, 0)
  assert.deepEqual(signIns(g.sent), [])
})

const SECRET = { url: 'https://auth.openai.com/oauth/authorize?state=s3cr3t', code: 'WXYZ-1234', address: 'https://accounts.x.ai/device?user_code=WXYZ-1234' }

test('the page is told of a sign-in without its URL, address or code', async () => {
  for (const way of ['chatgpt', 'xai']) {
    const g = gate()
    await g.signIn.request(g.webContents, way)
    assert.equal(g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way, port: 1455, expiresIn: 600, ...SECRET } }), true)
    g.signIn.handle({ event: 'accounts.signIn.ended', data: { attempt: 'a1', outcome: 'signedIn' } })
    const page = JSON.stringify([...g.told, g.signIn.current(g.webContents)])
    for (const value of Object.values(SECRET)) assert.ok(!page.includes(value), `${way}: ${value} reached the page`)
    assert.deepEqual(g.told[0], { kind: 'started', attempt: 'a1', way, id: null, port: 1455, expiresIn: 600 })
    assert.deepEqual(g.told[1], { kind: 'ended', attempt: 'a1', way, id: null, outcome: 'signedIn', sentence: null })
    assert.equal(g.shown[0].way, way, 'the sign-in window is shown what the bridge announced')
    assert.equal(g.stub.changed, 1, 'a stored sign-in makes the model listing stale')
    assert.equal(g.stub.closed, 1)
  }
})

test('an ending that arrives before the answer, or with no start, ends the ask', async () => {
  const g = gate()
  g.stub.started = new Promise((resolve) => setImmediate(() => {
    g.signIn.handle({ event: 'accounts.signIn.ended', data: { attempt: 'a1', outcome: 'failed', sentence: 'Refused.' } })
    resolve({ attempt: 'a1' })
  }))
  assert.deepEqual(await g.signIn.request(g.webContents, 'xai'), { attempt: 'a1' })
  assert.deepEqual(g.told, [{ kind: 'ended', attempt: 'a1', way: 'xai', id: null, outcome: 'failed', sentence: 'Refused.' }])
  assert.equal(g.signIn.current(g.webContents), null, 'and nothing is left running')
})

test('ChatGPT opens its recorded URL once started; xAI opens nothing unasked', async () => {
  const chatgpt = gate()
  await chatgpt.signIn.request(chatgpt.webContents, 'chatgpt')
  chatgpt.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'chatgpt', url: SECRET.url, port: 1455 } })
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(chatgpt.opened, [SECRET.url])

  const xai = gate()
  await xai.signIn.request(xai.webContents, 'xai')
  xai.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'xai', code: SECRET.code, address: SECRET.address, expiresIn: 900 } })
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(xai.opened, [])
  assert.equal(await xai.signIn.open(), true)
  assert.deepEqual(xai.opened, [SECRET.address])
  assert.equal(xai.signIn.copyable('code'), SECRET.code)
  assert.equal(xai.signIn.copyable('url'), null)
  assert.equal(xai.signIn.copyable('anything'), null)
})

test('the browser is sent only to the recorded address at exactly the way\'s origin', async () => {
  const origins = SIGN_IN_ORIGINS
  assert.equal(mayOpen('chatgpt', SECRET.url, SECRET.url, origins), true)
  assert.equal(mayOpen('xai', SECRET.address, SECRET.address, origins), true)
  assert.equal(mayOpen('chatgpt', SECRET.url, 'https://auth.openai.com/other', origins), false, 'not the recorded URL')
  for (const url of ['https://auth.openai.com.evil.example/a', 'http://auth.openai.com/a', 'https://auth.openai.com:8443/a', 'https://evil@auth.openai.com.example/', 'javascript:alert(1)', 'file:///etc/passwd']) {
    assert.equal(mayOpen('chatgpt', url, url, origins), false, url)
  }
  assert.equal(mayOpen('xai', SECRET.url, SECRET.url, origins), false, 'one way\'s origin is not the other\'s')

  const g = gate()
  await g.signIn.request(g.webContents, 'chatgpt')
  g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'chatgpt', url: 'https://evil.example/authorize' } })
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(g.opened, [], 'a URL at another origin is not opened, even when the bridge announced it')
})

test('a loopback address is never opened, stand-in or not', () => {
  assert.deepEqual(SIGN_IN_ORIGINS, { chatgpt: 'https://auth.openai.com', xai: 'https://accounts.x.ai' })
  const loopback = 'http://127.0.0.1:5000/authorize'
  assert.equal(mayOpen('chatgpt', loopback, loopback, SIGN_IN_ORIGINS), false)
  assert.equal(mayOpen('xai', loopback, loopback, SIGN_IN_ORIGINS), false)
})

test('a link in the main window opens only a web page', () => {
  for (const url of ['https://brave.com/', 'http://localhost:3000/']) assert.equal(opensExternally(url), true, url)
  for (const url of ['file:///etc/passwd', 'javascript:alert(1)', 'smb://host/share', 'x-apple.systempreferences:', 'mailto:a@b', 'not a url']) assert.equal(opensExternally(url), false, url)
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /setWindowOpenHandler\(\(\{ url \}\) => \{\s*if \(opensExternally\(url\)\) void shell\.openExternal\(url\)/)
})

test('cancelling sends cancelSignIn; a bridge that cannot be asked ends the attempt here', async () => {
  const g = gate()
  await g.signIn.request(g.webContents, 'chatgpt')
  await g.signIn.cancelFrom({}, 'a1')
  assert.equal(g.sent.filter((each) => each.method === 'accounts.cancelSignIn').length, 0, 'not from a foreign sender')
  await g.signIn.cancelFrom(g.webContents, 'a1')
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.cancelSignIn', params: { attempt: 'a1' } })

  const broken = gate()
  await broken.signIn.request(broken.webContents, 'xai')
  broken.signIn.deps.ask = async () => { throw new Error('gone') }
  await broken.signIn.cancel('a1')
  assert.deepEqual(broken.told.at(-1), { kind: 'ended', attempt: 'a1', way: 'xai', id: null, outcome: 'cancelled', sentence: null })
})

test('closing the window or quitting cancels without waiting', async () => {
  const g = gate()
  await g.signIn.request(g.webContents, 'chatgpt')
  g.signIn.deps.ask = (method, params) => { g.sent.push({ method, params }); return new Promise(() => {}) }
  assert.equal(g.signIn.abandon(), undefined)
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.cancelSignIn', params: { attempt: 'a1' } })
  assert.equal(g.signIn.current(g.webContents), null)
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /on\('before-quit'[\s\S]{0,400}signIn\.abandon\(\)/)
})

test('the model listing is dropped after a sign-in is stored and after a sign-out', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  assert.match(index, /new SignIn<BrowserWindow>\(\{[\s\S]*?changed: \(\) => \{ modelListing = null \}/)
  assert.match(index, /method === 'accounts\.signOut'[^\n]*\n?[^\n]*modelListing = null/)
})

test('accounts.list and accounts.signOut forward only their own fields', () => {
  assert.deepEqual(sanitised('accounts.list', { session: 's', way: 'chatgpt', replace: true }), { session: 's' })
  assert.deepEqual(sanitised('accounts.signOut', { session: 's', way: 'gateway', id: 'g', replace: true, attempt: 1 }), { session: 's', way: 'gateway', id: 'g' })
})

const awsRow = (id, fields = {}) => ({ way: 'bedrock', id, name: id === null ? 'AWS Bedrock' : `AWS Bedrock (${id})`, standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login bedrock', canSignOut: false, host: null, ...fields })
const AWS = { url: 'https://device.sso.us-west-2.amazonaws.com/', code: 'QRST-5678' }

test('an AWS sign-in names a row the bridge lists as one a sign-in is the remedy for', async () => {
  const g = gate({ stored: [awsRow(null), awsRow('work')] })
  assert.deepEqual(await g.signIn.request(g.webContents, 'bedrock', 'work'), { attempt: 'a1' })
  assert.deepEqual(g.sent[0], { method: 'accounts.list', params: {} }, 'an AWS ask checks the AWS rows')
  assert.deepEqual(signIns(g.sent), [{ method: 'accounts.signIn', params: { way: 'bedrock', id: 'work' } }])
  assert.deepEqual(g.signIn.current(g.webContents), { attempt: 'a1', way: 'bedrock', id: 'work', started: false, port: null, expiresIn: null }, 'a page that reloads is told the row')
  g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'bedrock', id: 'work', url: AWS.url } })
  assert.equal(g.signIn.current(g.webContents).id, 'work')
  assert.equal(g.dialogs[0].message, 'Sign in to AWS (work)?')
  assert.match(g.dialogs[0].detail, /^The AWS command-line tool opens a browser at AWS and shows a code to confirm there\./)
  assert.doesNotMatch(g.dialogs[0].detail, /replaces/)

  const chain = gate({ stored: [awsRow(null)] })
  assert.deepEqual(await chain.signIn.request(chain.webContents, 'bedrock', null), { attempt: 'a1' })
  assert.deepEqual(signIns(chain.sent)[0].params, { way: 'bedrock', id: null })
  assert.equal(chain.dialogs[0].message, 'Sign in to AWS?')

  for (const [stored, id] of [
    [[awsRow(null)], 'not-listed'],
    [[awsRow('work', { command: null, standing: 'unusable' })], 'work'],
    [[awsRow('work', { standing: 'signedIn', command: null })], 'work'],
    [[], null],
  ]) {
    const refused = gate({ stored })
    assert.equal(await refused.signIn.request(refused.webContents, 'bedrock', id), null, JSON.stringify({ stored, id }))
    assert.equal(refused.dialogs.length, 0)
    assert.deepEqual(signIns(refused.sent), [])
  }
  for (const id of ['', 7, { id: 'work' }]) {
    const bad = gate({ stored: [awsRow('work')] })
    assert.equal(await bad.signIn.request(bad.webContents, 'bedrock', id), null)
    assert.deepEqual(bad.sent, [], 'an id that is not a profile asks nobody')
  }
})

test('the sign-in window names the row as the settings page lists it', async () => {
  const g = gate({ stored: [{ ...row('xai', false), name: 'xAI account' }] })
  await g.signIn.request(g.webContents, 'xai')
  g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'xai', ...SECRET } })
  assert.equal(g.shown[0].name, 'xAI account')
  const aws = gate({ stored: [awsRow(null), awsRow('work')] })
  await aws.signIn.request(aws.webContents, 'bedrock', 'work')
  aws.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'bedrock', id: 'work', url: AWS.url } })
  assert.equal(aws.shown[0].name, 'AWS Bedrock (work)', 'an AWS row is the profile asked for, not the default chain')
  const unlisted = gate()
  await unlisted.signIn.request(unlisted.webContents, 'chatgpt')
  unlisted.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'chatgpt', url: SECRET.url } })
  assert.equal(unlisted.shown[0].name, 'ChatGPT', 'a way the list does not name keeps its own word')
})

test('an AWS sign-in shows its URL, then its code, to the window and neither to the page', async () => {
  const g = gate({ stored: [awsRow('work')] })
  await g.signIn.request(g.webContents, 'bedrock', 'work')
  g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'bedrock', id: 'work', url: AWS.url, code: null } })
  assert.equal(g.signIn.handle({ event: 'accounts.signIn.updated', data: { attempt: 'a1', way: 'bedrock', id: 'work', url: 'https://evil.example/', code: AWS.code } }), true)
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(g.shown.map((state) => [state.way, state.id, state.url, state.code]), [['bedrock', 'work', AWS.url, null]])
  assert.deepEqual(g.updated.map((state) => [state.url, state.code]), [[AWS.url, AWS.code]], 'an update adds the code and keeps the URL')
  assert.deepEqual(g.opened, [], 'the AWS command-line tool opens its own browser')
  assert.equal(g.signIn.copyable('code'), AWS.code)
  assert.equal(g.signIn.copyable('url'), AWS.url)
  assert.equal(await g.signIn.open(), true)
  assert.deepEqual(g.opened, [AWS.url])
  g.signIn.handle({ event: 'accounts.signIn.updated', data: { attempt: 'other', way: 'bedrock', code: 'ZZZZ-0000' } })
  assert.equal(g.updated.length, 1, 'another attempt\'s update is not shown')
  g.signIn.handle({ event: 'accounts.signIn.ended', data: { attempt: 'a1', way: 'bedrock', id: 'work', outcome: 'signedIn', sentence: 'Recorded.' } })
  assert.deepEqual(g.told, [
    { kind: 'started', attempt: 'a1', way: 'bedrock', id: 'work', port: null, expiresIn: null },
    { kind: 'ended', attempt: 'a1', way: 'bedrock', id: 'work', outcome: 'signedIn', sentence: 'Recorded.' },
  ])
  const page = JSON.stringify(g.told)
  for (const value of Object.values(AWS)) assert.ok(!page.includes(value), `${value} reached the page`)
  assert.equal(g.stub.changed, 1)
})

test('an AWS page is opened only at the recorded https URL under the AWS sign-in hosts', () => {
  assert.equal(AWS_SIGN_IN_HOSTS.length, 3)
  for (const url of [AWS.url, 'https://oidc.eu-west-1.amazonaws.com/authorize?response_type=code', 'https://d-1234567890.awsapps.com/start/#/device']) {
    assert.equal(mayOpen('bedrock', url, url, SIGN_IN_ORIGINS), true, url)
  }
  for (const url of ['https://s3.amazonaws.com/bucket/page.html', 'https://bucket.s3.us-west-2.amazonaws.com/', 'https://execute-api.us-east-1.amazonaws.com/', 'https://a.b.awsapps.com/', 'https://device.sso.amazonaws.com/',
    'http://device.sso.us-west-2.amazonaws.com/', 'https://amazonaws.com.evil.example/', 'https://evilamazonaws.com/', 'https://user@device.sso.us-west-2.amazonaws.com/', 'https://device.sso.us-west-2.amazonaws.com:8443/', 'https://auth.openai.com/', 'javascript:alert(1)']) {
    assert.equal(mayOpen('bedrock', url, url, SIGN_IN_ORIGINS), false, url)
  }
  assert.equal(mayOpen('bedrock', AWS.url, 'https://other.awsapps.com/', SIGN_IN_ORIGINS), false, 'not the recorded URL')
  assert.equal(mayOpen('chatgpt', AWS.url, AWS.url, SIGN_IN_ORIGINS), false, 'an AWS host is not another way\'s')
})

test('closing the window or quitting cancels an AWS sign-in too', async () => {
  const g = gate({ stored: [awsRow(null)] })
  await g.signIn.request(g.webContents, 'bedrock', null)
  g.signIn.handle({ event: 'accounts.signIn.started', data: { attempt: 'a1', way: 'bedrock', id: null, url: AWS.url } })
  await g.signIn.cancelShown()
  assert.deepEqual(g.sent.at(-1), { method: 'accounts.cancelSignIn', params: { attempt: 'a1' } })
  g.signIn.abandon()
  assert.equal(g.signIn.current(g.webContents), null)
  const window = readFileSync('src/main/sign-in-window.ts', 'utf8')
  assert.match(window, /const title = `Sign in to \$\{state\.name\}`/)
  assert.match(window, /smallWindow\('sign-in'\)/)
  assert.match(readFileSync('src/main/small-window.ts', 'utf8'), /on\('closed', \(\) => \{[\s\S]{0,80}if \(!closing\) cancel\(\)/)
})

test('the session the page names reaches every ask of the bridge, and one that is not an id drops the ask', async () => {
  const g = gate({ stored: [row('chatgpt', false)] })
  assert.deepEqual(await g.signIn.request(g.webContents, 'chatgpt', null, 's1'), { attempt: 'a1' })
  assert.deepEqual(g.sent.map((each) => each.params.session), ['s1', 's1'])
  const aws = gate({ stored: [awsRow('work')] })
  await aws.signIn.request(aws.webContents, 'bedrock', 'work', 's2')
  assert.deepEqual(aws.sent, [
    { method: 'accounts.list', params: { session: 's2' } },
    { method: 'accounts.signIn', params: { way: 'bedrock', id: 'work', session: 's2' } },
  ])
  for (const session of ['', 7, { id: 's' }]) {
    const bad = gate()
    assert.equal(await bad.signIn.request(bad.webContents, 'chatgpt', null, session), null)
    assert.deepEqual(bad.sent, [], JSON.stringify(session))
  }
})

test('no sign-in is asked for while the key window is open or asked for', async () => {
  const g = gate()
  g.stub.otherBusy = true
  assert.equal(await g.signIn.request(g.webContents, 'chatgpt'), null)
  assert.deepEqual(g.sent, [])
  assert.equal(g.signIn.busy(), false)
  g.stub.otherBusy = false
  g.stub.answer = () => new Promise(() => {})
  void g.signIn.request(g.webContents, 'chatgpt')
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(g.signIn.busy(), true, 'a dialog that is up holds the key window off')
})

test('a sign-in or key window that is open already is told what to show when it is brought back', () => {
  const signInWindow = readFileSync('src/main/sign-in-window.ts', 'utf8')
  assert.match(readFileSync('src/main/small-window.ts', 'utf8'), /if \(open\) \{\s*reuse\(open\)/)
  assert.match(signInWindow, /reuse: \(window\) => \{[\s\S]{0,80}window\.webContents\.send\('bravebot:sign-in-window:changed', state\)/)
  assert.match(signInWindow, /Math\.min\(SIGN_IN_WINDOW_HEIGHTS\.most, Math\.max\(SIGN_IN_WINDOW_HEIGHTS\.least/, 'the page cannot size its window past the bounds')
  const keyWindow = readFileSync('src/main/key-window.ts', 'utf8')
  assert.match(keyWindow, /reuse: \(window\) => \{[\s\S]{0,80}window\.webContents\.send\('bravebot:key-window:changed', state\)/)
  assert.match(readFileSync('src/preload/key-window.ts', 'utf8'), /ipcRenderer\.on\('bravebot:key-window:changed'/)
  assert.match(readFileSync('src/renderer/key-window.tsx', 'utf8'), /onChanged\(setState\)/)
})
