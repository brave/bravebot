import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

const source = buildSync({ entryPoints: ['src/renderer/labels.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('module', 'exports', source)(module, module.exports)
const { parseLabel, labelWords, LABEL_TOOLTIPS } = module.exports

const UP = { trusted: false, public: false }

test('the four short forms and the audit words read as the label they name', () => {
  assert.deepEqual(parseLabel('(T,pub)'), { trusted: true, public: true })
  assert.deepEqual(parseLabel('(T,priv)'), { trusted: true, public: false })
  assert.deepEqual(parseLabel('(U,pub)'), { trusted: false, public: true })
  assert.deepEqual(parseLabel('(U,priv)'), UP)
  assert.deepEqual(parseLabel({ integrity: 'trusted', confidentiality: 'public' }), { trusted: true, public: true })
  assert.deepEqual(parseLabel({ integrity: 'untrusted', confidentiality: 'public' }), { trusted: false, public: true })
  assert.deepEqual(parseLabel({ integrity: 'trusted', confidentiality: 'private' }), { trusted: true, public: false })
})

test('anything else reads as untrusted and private, axis by axis', () => {
  for (const label of [
    undefined, null, '', 'trusted', '(t,pub)', '(T, pub)', ' (T,pub)', '(T,pub) ', '(T,pub)(T,pub)', 'T,pub',
    '(T,public)', 'constructor', '__proto__', 'toString', 42, true, [], ['(T,pub)'], {}, { integrity: 'Trusted', confidentiality: 'Public' },
    { integrity: ['trusted'], confidentiality: 'public ' }, { trusted: true, public: true },
  ]) assert.deepEqual(parseLabel(label), UP, JSON.stringify(label))
  // One unreadable axis does not take the other with it, nor raise it.
  assert.deepEqual(parseLabel({ integrity: 'trusted' }), { trusted: true, public: false })
  assert.deepEqual(parseLabel({ confidentiality: 'public', integrity: 'TRUSTED' }), { trusted: false, public: true })
})

test('the words and tooltips are fixed, one per axis value', () => {
  assert.deepEqual(labelWords(UP), { integrity: 'Untrusted', confidentiality: 'Private' })
  assert.deepEqual(labelWords({ trusted: true, public: true }), { integrity: 'Trusted', confidentiality: 'Public' })
  assert.deepEqual(Object.keys(LABEL_TOOLTIPS).sort(), ['private', 'public', 'trusted', 'untrusted'])
  for (const sentence of Object.values(LABEL_TOOLTIPS)) assert.match(sentence, /^[A-Z][a-z]+: [^.]+\.$/)
})
