// Making the message in the desktop window's box more specific (INPUT-50).
//
// The rewrite is the bridge's (crates/ui-bridge/tests/enhancing.rs). What the window owns is when
// the box may be rewritten, what it holds afterwards and what Escape brings back, so these drive
// the plain functions with the answers the bridge sends. The faults they reject are a rewrite
// written over a line typed since the request went out, a first line lost to a second rewrite, a
// refusal that sends the box to the model anyway, and a rewrite from an untrusted conversation
// reaching the box.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => require(id), module, module.exports)
  return module.exports
}

const e = load("export * from './src/renderer/enhancing'")
const ready = (text, extra = {}) => ({ status: 'ready', text, ...extra })

test('a rewrite replaces the line it was asked for and keeps that line to put back', () => {
  const asked = e.begun({}, 'add retries')
  const { held, write } = e.settled(asked, ready('Add retries to fetch_page.'), 'add retries')
  assert.equal(write, 'Add retries to fetch_page.')
  assert.equal(held.rewritten.original, 'add retries')
  assert.equal(held.asking, null)
  assert.deepEqual(e.restored(held).write, 'add retries')
  assert.equal(e.restored(held).held.rewritten, null)
})

test('a rewrite is not written over a line typed since the request went out', () => {
  const asked = e.begun({}, 'add retries')
  const { held, write } = e.settled(asked, ready('Add retries to fetch_page.'), 'add retries and tests')
  assert.equal(write, null)
  assert.equal(held.rewritten, undefined)
  assert.match(held.note, /changed the message/)
})

test('a second rewrite still puts back the line the person wrote, not the first rewrite', () => {
  const first = e.settled(e.begun({}, 'add retries'), ready('One.'), 'add retries').held
  const second = e.settled(e.begun(first, 'One.'), ready('Two.'), 'One.')
  assert.equal(second.write, 'Two.')
  assert.equal(e.restored(second.held).write, 'add retries')
})

test('an answer that offers nothing leaves the box alone and says why', () => {
  const asked = e.begun({}, 'add retries')
  for (const [data, words] of [
    [{ status: 'untrusted' }, /untrusted/],
    [{ status: 'unusable', limit: 4000 }, /empty or over 4000/],
    [{ status: 'stopped' }, /Stopped/],
    [{ status: 'failed', message: 'offline' }, /offline/],
  ]) {
    const { held, write } = e.settled(asked, data, 'add retries')
    assert.equal(write, null)
    assert.equal(held.rewritten, undefined)
    assert.equal(held.asking, null)
    assert.match(held.note, words)
  }
})

test('a ready answer with no text is not written', () => {
  assert.equal(e.settled(e.begun({}, 'x'), { status: 'ready' }, 'x').write, null)
})

test('characters taken out of a rewrite are counted in the note', () => {
  const one = e.settled(e.begun({}, 'x'), ready('y', { removed: 1 }), 'x').held.note
  const two = e.settled(e.begun({}, 'x'), ready('y', { removed: 2 }), 'x').held.note
  assert.match(one, /1 character that draw as nothing was taken out/)
  assert.match(two, /2 characters that draw as nothing were taken out/)
})

test('Escape has nothing to put back where the box holds no rewrite', () => {
  assert.equal(e.restored({}), null)
  assert.equal(e.restored({ asking: { asked: 'x' } }), null)
})

test('the box is refused for a running turn, nothing written, a staged marker and an overlong line', () => {
  assert.ok(e.refusal('add retries', true, false))
  assert.ok(e.refusal('  \n', false, false))
  assert.ok(e.refusal('see [Image #1]', false, true))
  assert.equal(e.refusal('add retries', false, false), null)
  assert.equal(e.refusal('x'.repeat(e.ENHANCE_LIMIT), false, false), null)
  assert.ok(e.refusal('x'.repeat(e.ENHANCE_LIMIT + 1), false, false))
  const accents = 'é'.repeat(e.ENHANCE_LIMIT)
  assert.equal(e.refusal(accents, false, false), null)
})

test('emptying the box ends a rewrite, and a box with text keeps it', () => {
  const held = { rewritten: { original: 'mine' }, note: 'n' }
  assert.equal(e.forgotten(held, '').rewritten, null)
  assert.equal(e.forgotten(held, 'still here').rewritten.original, 'mine')
})
