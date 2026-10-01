import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { readFileSync } from 'node:fs'

function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('module', 'exports', source)(module, module.exports)
  return module.exports
}
const { COMPACT, SIDES, fit, isCompact } = load('src/renderer/columns.ts')

const open = { left: false, right: false }

test('a phone-width window is compact and a desktop one never is', () => {
  assert.equal(isCompact(412), true)
  assert.equal(isCompact(COMPACT), true)
  assert.equal(isCompact(COMPACT + 1), false)
  // Electron's narrowest window, which is why the drawer never appears on a desktop.
  assert.equal(isCompact(900), false)
})

test('in a compact window the session list takes no width from the conversation', () => {
  // As a column it would be squeezed to its minimum against a 412px window; as a drawer it keeps
  // the width it will be drawn at when it opens over the conversation.
  const widths = fit({ left: 300, right: SIDES.right.initial }, 412, open)
  assert.equal(widths.left, 300)
})

test('above the breakpoint the session list is still a column that gives way to the conversation', () => {
  const widths = fit({ left: 300, right: SIDES.right.initial }, 760, open)
  assert.ok(widths.left < 300, `left stayed ${widths.left}`)
  assert.ok(widths.left >= SIDES.left.min)
})

test('the drawer styles switch on at the same width the layout code does', () => {
  const css = readFileSync('src/renderer/styles/shell.css', 'utf8')
  assert.match(css, new RegExp(`@media \\(max-width: ${COMPACT}px\\)`))
})

test('a dialog becomes a bottom sheet at the same width', () => {
  const modal = readFileSync('src/renderer/components/Modal.tsx', 'utf8')
  assert.match(modal, new RegExp(`@media \\(max-width: ${COMPACT}px\\)`))
})
