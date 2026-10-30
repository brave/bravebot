import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const components = new URL('../src/renderer/components/', import.meta.url).pathname
const sources = readdirSync(components).filter(name => name.endsWith('.tsx'))
  .map(name => ({ name, text: readFileSync(join(components, name), 'utf8') }))

// Each element carrying `modal-leading`, with what it holds up to its closing tag.
function leading() {
  const found = []
  for (const { name, text } of sources) {
    const opening = /<(Button|div)\b[^>]*className="[^"]*\bmodal-leading\b[^"]*"[^>]*>/g
    for (const match of text.matchAll(opening)) {
      const close = text.indexOf(`</${match[1]}>`, match.index + match[0].length)
      found.push({ name, inner: text.slice(match.index + match[0].length, close) })
    }
  }
  return found
}

const ANSWERS = /\b(Cancel|Don['’]t|Not now|Close|Keep)\b/

test('no Cancel or refusal is put at the far end of a dialog, away from the primary action', () => {
  const found = leading()
  assert.ok(found.some(({ name }) => name === 'Watches.tsx'), 'Stop all watches still stands apart')
  for (const { name, inner } of found) assert.doesNotMatch(inner, ANSWERS, `${name}: an answer to the dialog sits beside its primary action`)
})

test('Undo and the trust question keep their Cancel and refusal in the footer', () => {
  const footer = name => sources.find(source => source.name === name).text
  assert.match(footer('Rewind.tsx'), /data-test="rewind-cancel"/)
  assert.match(footer('TrustPrompt.tsx'), /data-test="trust-decline"/)
})
