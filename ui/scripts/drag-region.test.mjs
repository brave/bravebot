// A control drawn in the conversation title sits over the window's drag strip. A driver click
// reaches it anyway, so a real pointer moves the window unless the control opts out.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { transformSync } from 'esbuild'

test('the back control on earlier conversations takes a click in the titlebar', () => {
  const { code } = transformSync(readFileSync('src/renderer/styles/transcript.css', 'utf8'), { loader: 'css' })
  const rules = [...code.matchAll(/^([^\s@}][^{]*?) \{\n((?:  [^\n]*\n)*)\}$/gm)].map(([, selectors, body]) => ({
    selectors: selectors.split(',\n'),
    declarations: Object.fromEntries([...body.matchAll(/^  ([a-z-]+): (.*);$/gm)].map(([, property, value]) => [property, value.trim()])),
  }))
  const back = rules.find((rule) => rule.selectors.includes('.head-titles .earlier-back'))
  assert.ok(back, 'the back control has a rule of its own')
  assert.equal(back.declarations['-webkit-app-region'], 'no-drag')
  const source = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(source, /className="earlier-back"/)
})
