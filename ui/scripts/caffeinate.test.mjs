import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { Caffeinate } = load('src/main/caffeinate.ts')
const { NOTHING_OPEN, isChecked, isEnabled, parseWindowState, command } = load('src/shared/commands.ts')

function blocker() {
  const calls = []
  let next = 0
  return {
    calls,
    started: () => calls.filter(([kind]) => kind === 'start').length - calls.filter(([kind]) => kind === 'stop').length,
    start(type) { calls.push(['start', type]); return ++next },
    stop(id) { calls.push(['stop', id]) },
  }
}

test('the computer is kept awake only while the setting is on and a turn runs', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake)
  awake.setBusy(true)
  assert.equal(fake.started(), 0, 'off at launch, so a turn alone holds nothing')
  awake.setEnabled(true)
  assert.deepEqual(fake.calls, [['start', 'prevent-app-suspension']])
  awake.setBusy(true)
  awake.setEnabled(true)
  assert.equal(fake.calls.length, 1, 'held once, however often it is told')
  awake.setBusy(false)
  assert.deepEqual(fake.calls.at(-1), ['stop', 1], 'let go when the turns end')
  awake.setBusy(true)
  assert.equal(fake.started(), 1)
  awake.setEnabled(false)
  assert.equal(fake.started(), 0, 'let go when the setting goes off mid-turn')
  assert.equal(awake.holding, false)
})

test('quitting lets go of a hold', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake)
  awake.setEnabled(true)
  awake.setBusy(true)
  awake.release()
  assert.equal(fake.started(), 0)
  awake.setBusy(true)
  assert.equal(fake.started(), 0, 'and does not take it again')
})

test('the menu item is a checkbox ticked by the window state and offered with nothing open', () => {
  const item = command('app.caffeinate')
  assert.equal(item.label, 'Keep Computer Awake During Turns')
  assert.equal(item.checkbox, true)
  assert.equal(isEnabled(item.requires, NOTHING_OPEN), true)
  assert.equal(isChecked('app.caffeinate', NOTHING_OPEN), false)
  assert.equal(isChecked('app.caffeinate', { ...NOTHING_OPEN, caffeinated: true }), true)
  assert.equal(parseWindowState({ ...NOTHING_OPEN, caffeinated: 'yes' }), null)
  assert.equal(parseWindowState({ ...NOTHING_OPEN, caffeinated: true }).caffeinated, true)
})
