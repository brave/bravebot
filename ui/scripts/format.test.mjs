import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('module', 'exports', source)(module, module.exports)
  return module.exports
}

test('a session row says how long ago in the fewest characters that still read', () => {
  const { shortAgo } = load('src/renderer/time.ts')
  const now = Date.UTC(2026, 8, 29, 12, 0, 0)
  const at = (seconds) => Math.floor(now / 1000) - seconds
  assert.equal(shortAgo(at(0), now), 'now')
  assert.equal(shortAgo(at(59), now), 'now')
  assert.equal(shortAgo(at(4 * 60), now), '4m')
  assert.equal(shortAgo(at(3 * 3600 + 59), now), '3h')
  assert.equal(shortAgo(at(5 * 86400), now), '5d')
  assert.equal(shortAgo(Date.UTC(2026, 2, 4, 12) / 1000, now), 'Mar 4')
  assert.equal(shortAgo(Date.UTC(2024, 2, 4, 12) / 1000, now), 'Mar 4, 2024')
  assert.equal(shortAgo(at(-30), now), 'now', 'a clock a little ahead of ours is not the future')
})

test('a long path loses its middle, never its filename', () => {
  const { middleTruncate } = load('src/renderer/truncate.ts')
  assert.equal(middleTruncate('src/store.ts', 40), 'src/store.ts')
  const cut = middleTruncate('src/renderer/components/deeply/nested/Transcript.tsx', 30)
  assert.equal(cut.length, 30)
  assert.ok(cut.endsWith('/Transcript.tsx'))
  assert.ok(cut.startsWith('src/'))
  assert.ok(cut.includes('…'))
  const long = middleTruncate('a-filename-that-is-longer-than-the-whole-budget.ts', 20)
  assert.equal(long.length, 20)
  assert.ok(long.endsWith('budget.ts'))
  const windows = middleTruncate('C:\\long-folder\\another-folder\\important-report.ts', 30)
  assert.equal(windows.length, 30)
  assert.ok(windows.endsWith('\\important-report.ts'))
})
