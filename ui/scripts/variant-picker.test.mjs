// Picking a model's named option variant from the desktop window (BACKEND-66).
//
// The bridge holds the pick behind `session.variant`. The window reaches it only if the main
// process forwards the method, so the allow-list has to name it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

function allowedMethods(index) {
  const body = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)
  assert.ok(body, 'the allow-list is a literal set the test can read')
  return [...body[1].matchAll(/'([^']+)'/g)].map(each => each[1])
}

test('the main process forwards session.variant from the window', () => {
  const allowed = allowedMethods(readFileSync('src/main/index.ts', 'utf8'))
  assert.ok(allowed.length > 40, 'the list was read whole')
  assert.ok(allowed.includes('session.variant'), 'session.variant is in ALLOWED')
})
