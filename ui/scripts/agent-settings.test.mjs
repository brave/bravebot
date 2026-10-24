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

// CHATGPT-8: the agent sends which failure of the ChatGPT account it was as a name from a closed
// set, beside the category, and the window words it from the name. Each name is its own title
// and sentence, and the usage limit is told apart from the account not being usable.
test('a failure of the ChatGPT account is worded from the name the agent sent and from nothing else', () => {
  const { failureSummary } = load('src/renderer/failure.ts')
  const signIn = 'Run bravebot auth login chatgpt in a terminal to sign in'
  const rows = [
    ['not-signed-in', 'unauthorized', 'Not signed in to ChatGPT', `${signIn}, then send the message again.`],
    ['ended', 'unauthorized', 'The ChatGPT sign-in has ended', `${signIn} again.`],
    ['token-refused', 'unauthorized', 'OpenAI refused the ChatGPT sign-in', `${signIn} again.`],
    ['unusable', 'unauthorized', 'The stored ChatGPT sign-in could not be read', 'Run bravebot auth status chatgpt in a terminal to see why.'],
    ['usage-limit', 'refused', 'OpenAI says this ChatGPT account has reached its usage limit', 'Choose a model from another service to keep working.'],
    ['not-permitted', 'refused', 'OpenAI does not allow this ChatGPT account to be used this way', 'Choose a model from another service to keep working.'],
    ['renewal-refused', 'refused', 'The ChatGPT sign-in could not be renewed', 'The request to renew it was refused.'],
    ['renewal-rate-limited', 'rate-limited', 'The ChatGPT sign-in could not be renewed', 'The request to renew it was rate-limited. Wait before trying again.'],
    ['renewal-unavailable', 'unavailable', 'The ChatGPT sign-in could not be renewed', 'The request to renew it was met with an unavailable service. Try again when it recovers.'],
    ['renewal-incomplete', 'incomplete', 'The ChatGPT sign-in could not be renewed', 'The reply to the request to renew it could not be read.'],
    ['renewal-transport', 'transport', 'The ChatGPT sign-in could not be renewed', 'The request to renew it was not completed. Check the connection and try again.'],
  ]
  for (const [name, category, title, description] of rows) {
    assert.deepEqual(failureSummary(category, null, name), { title, description }, name)
    assert.notDeepEqual(failureSummary(category, null, name), failureSummary(category), `${name} is worded as its category is`)
  }
  assert.equal(new Set(rows.map(([name]) => JSON.stringify(failureSummary('refused', null, name)))).size, rows.length, 'two failures are worded alike')

  // The name decides, whatever category it arrives with, and a refusal that names the account does not
  // send the person to sign in again, which would change nothing.
  assert.equal(failureSummary('unauthorized', null, 'usage-limit').title, 'OpenAI says this ChatGPT account has reached its usage limit')
  for (const name of ['usage-limit', 'not-permitted']) assert.doesNotMatch(JSON.stringify(failureSummary('refused', null, name)), /auth login|sign in/, name)

  // Without a name, and for one this build does not know, the card says what the category says. Nothing
  // of what a name spells is drawn: a name is looked up, never printed.
  for (const unknown of [undefined, null, '', 'ended-in-the-future', 'constructor', '__proto__', 'toString', '<b>sk-live-123</b>']) {
    assert.deepEqual(failureSummary('unauthorized', null, unknown), failureSummary('unauthorized'), String(unknown))
    assert.deepEqual(failureSummary('model-unconfigured', null, unknown), failureSummary('model-unconfigured'), String(unknown))
  }
  assert.equal(failureSummary('unauthorized', null, 'not-signed-in').title.includes('Authentication'), false)
})
