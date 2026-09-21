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
const { parseHooks } = load('src/shared/agent-settings.ts')
const { readHooks, saveHooks } = load('src/main/agent-settings.ts')
const t = load('src/renderer/transcript.ts')

test('hook arguments stay literal and unsupported documents cannot be silently rewritten', () => {
  const text = JSON.stringify({ hooks: [{ on: 'tool-finished', tool: 'write_file', run: ['formatter', 'two words', '$(touch nope)', ''] }] })
  assert.deepEqual(parseHooks(text)[0].run, ['formatter', 'two words', '$(touch nope)', ''])
  for (const document of [[], { hooks: [{ on: 'other', run: ['x'] }] }, { hooks: [{ on: 'turn-started', run: 'x y' }] }, { hooks: [{ on: 'turn-started', tool: 'write_file', run: ['x'] }] }, { hooks: [], extra: true }]) {
    assert.throws(() => parseHooks(JSON.stringify(document)))
  }
})

test('hook saves are explicit, reject conflicts and refuse symlinks', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bravebot-hooks-test-'))
  try {
    const before = readHooks(directory)
    const text = JSON.stringify({ hooks: [{ on: 'turn-finished', run: ['echo', 'literal;argument'] }] })
    const after = saveHooks(directory, text, before.text)
    assert.equal(after.text, text)
    assert.throws(() => saveHooks(directory, '{"hooks":[]}', before.text), /changed on disk/)
    assert.equal(readFileSync(after.path, 'utf8'), text)
    rmSync(after.path)
    const outside = join(directory, 'other.json')
    writeFileSync(outside, text)
    symlinkSync(outside, after.path)
    assert.throws(() => readHooks(directory))
    assert.throws(() => saveHooks(directory, '{"hooks":[]}', text))
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
  const prompt = 'Watch 3 fired: src/a.ts looks written to since the last look.\n\nNothing has been read. Read the file if you need it.'
  const entries = t.fromSaid([{ kind: 'user', text: prompt }])
  assert.equal(entries[0].kind, 'watch')
  assert.equal(t.beginTurn(entries, 2)[1].kind, 'turn-start')
  assert.deepEqual(t.conversation(entries), [])
})

test('stable failure categories do not depend on service wording', () => {
  const { failureSummary } = load('src/renderer/failure.ts')
  assert.match(failureSummary('transport').description, /proxy and certificate/)
  assert.equal(failureSummary('cancelled').title, 'Task stopped')
  assert.equal(failureSummary('unauthorized').title, 'Authentication failed')
  assert.equal(failureSummary('model-unconfigured').title, 'Choose a model from your configured service')
  assert.match(failureSummary('unknown-future-category').title, /could not finish/)
})
