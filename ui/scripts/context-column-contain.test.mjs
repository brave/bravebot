// The inspector column's scrolling box is sized by the column and not by its panels (#3581).
//
// With the renderer's custom scrollbar (base.css), a column of panels taller than the window that
// sized itself from them ended the renderer process with SIGSEGV when the pointer moved over the
// window. `pnpm run drive:pointer-crash` shows that in a real window; this pins the rule that
// stops it, where a run of the driver is too slow to sit in every test pass. It rejects a stylesheet
// where `.context-content` scrolls without `contain: size`.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const css = readFileSync(new URL('../src/renderer/styles/inspector.css', import.meta.url), 'utf8')

test('the inspector column scrolls in a box that is size-contained', () => {
  const rule = css.match(/^\.context-content\s*\{([^}]*)\}/m)
  assert.ok(rule, 'a `.context-content` rule is declared')
  assert.match(rule[1], /overflow-y:\s*auto/, 'the column scrolls')
  assert.match(rule[1], /contain:\s*[^;]*\bsize\b/, 'its size does not come from its panels')
})
