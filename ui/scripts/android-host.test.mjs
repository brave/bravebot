import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

// The Android host stands where the desktop main process stands: between a page that is not
// trusted to decide what the agent may be asked and the agent itself. These pin the two to each
// other, so a method allowed on one platform is allowed on both and a field one strips the other
// strips too.
const main = readFileSync('src/main/index.ts', 'utf8')
const host = readFileSync('../android/app/src/main/java/com/brave/bravebot/Host.kt', 'utf8')

function block(source, start, end) {
  const from = source.indexOf(start)
  assert.ok(from >= 0, `no ${start}`)
  return source.slice(from, source.indexOf(end, from + start.length))
}

const quoted = (text, quote) => new Set([...text.matchAll(new RegExp(`${quote}([a-z.]+)${quote}`, 'g'))].map(m => m[1]))

test('the Android host allows exactly the methods the desktop main process allows', () => {
  const desktop = quoted(block(main, 'const ALLOWED = new Set([', '])'), "'")
  const android = quoted(block(host, 'val ALLOWED = setOf(', ')\n'), '"')
  assert.ok(desktop.size > 10)
  assert.deepEqual([...android].sort(), [...desktop].sort())
})

test('the Android host strips the same fields from turn.send', () => {
  const destructured = block(main, 'const { files: _files', '} = held')
  const desktop = new Set([...destructured.matchAll(/(\w+)(?::\s*_\w+)?,/g)].map(m => m[1]))
  const android = new Set([...block(host, '"turn.send" ->', '}').matchAll(/remove\("(\w+)"\)/g)].map(m => m[1]))
  assert.deepEqual([...android].sort(), [...desktop].sort())
  // And replaces the file list with an empty one rather than leaving whatever the page sent.
  assert.match(block(host, '"turn.send" ->', '}'), /put\("files", JSONArray\(\)\)/)
})

test('the Android host forwards only the fields manifest.run reads', () => {
  const desktop = new Set([...block(main, "if (method === 'manifest.run') {", '}\n').matchAll(/(\w+): held\.\w+/g)].map(m => m[1]))
  assert.ok(desktop.size > 0)
  const android = new Set([...block(host, '"manifest.run" ->', '"turn.send"').matchAll(/put\("(\w+)"/g)].map(m => m[1]))
  assert.deepEqual([...android].sort(), [...desktop].sort())
})
