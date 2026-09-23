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
const { composeHooks } = load('src/shared/agent-settings.ts')
const { saveHooks } = load('src/main/agent-settings.ts')
const t = load('src/renderer/transcript.ts')

test('an edit writes the words as typed and carries nothing the agent does not read', () => {
  const text = composeHooks([
    { on: 'tool-finished', tool: 'write_file', run: ['formatter', 'two words', '$(touch nope)', ''], firesForNothing: false },
    { on: 'turn-started', tool: null, run: ['begin'], firesForNothing: false },
  ])
  // The agent's own answer about an entry is not written back into the file it was read from.
  assert.deepEqual(JSON.parse(text), {
    hooks: [
      { on: 'tool-finished', tool: 'write_file', run: ['formatter', 'two words', '$(touch nope)', ''] },
      { on: 'turn-started', run: ['begin'] },
    ],
  })
})

test('hook saves are explicit, reject conflicts and refuse symlinks', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-hooks-test-'))
  try {
    const path = join(directory, 'hooks.json')
    const text = composeHooks([{ on: 'turn-finished', tool: null, run: ['echo', 'literal;argument'], firesForNothing: false }])
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
