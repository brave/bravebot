// CMD-12 in the window: Keep Computer Awake During Turns. The main process's rules are run against
// a fake `powerSaveBlocker`, the window's steps against a fake preload, and the dialog is rendered
// through `react-dom/server` with `Modal` drawn in place of its portal.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { build, buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { Caffeinate, trackTurns } = load('src/main/caffeinate.ts')
const { NOTHING_OPEN, isChecked, isEnabled, parseWindowState, command } = load('src/shared/commands.ts')

const inPlaceModal = {
  name: 'in-place-modal',
  setup(built) {
    built.onResolve({ filter: /^\.\/Modal$/ }, () => ({ path: 'modal', namespace: 'in-place-modal' }))
    built.onLoad({ filter: /.*/, namespace: 'in-place-modal' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export function Modal({ title, children, actions }) { return createElement('div', { className: 'modal', 'data-title': title }, children, actions) }`,
    }))
  },
}
let bundle
async function dialog() {
  bundle ??= build({
    entryPoints: ['src/renderer/components/CaffeinateConfirm.tsx'],
    bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins: [inPlaceModal],
  }).then((built) => {
    const module = { exports: {} }
    new Function('require', 'module', 'exports', built.outputFiles[0].text)(require, module, module.exports)
    return module.exports
  })
  return bundle
}

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
  const awake = new Caffeinate(fake, true)
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

test('the setting holds nothing until the dialog has been agreed to', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake)
  awake.setBusy(true)
  awake.setEnabled(true)
  assert.equal(fake.started(), 0, 'a window saying the setting is on is not enough')
  assert.equal(awake.isConfirmed, false)
  awake.confirm()
  awake.setEnabled(true)
  assert.equal(fake.started(), 1, 'held once agreed to')
  assert.equal(new Caffeinate(blocker(), true).isConfirmed, true, 'an agreement from an earlier run is kept')
})

test('closing the window lets go, and the setting is off until the next window turns it on', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake, true)
  awake.setEnabled(true)
  awake.setBusy(true)
  awake.windowClosed()
  assert.equal(fake.started(), 0)
  awake.setBusy(true)
  assert.equal(fake.started(), 0, 'a turn in the next window holds nothing on its own')
})

test('quitting lets go of a hold', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake, true)
  awake.setEnabled(true)
  awake.setBusy(true)
  awake.release()
  assert.equal(fake.started(), 0)
  awake.setBusy(true)
  assert.equal(fake.started(), 0, 'and does not take it again')
})

test('the turns in flight follow the bridge’s events, so the hold follows the last turn', () => {
  const fake = blocker()
  const awake = new Caffeinate(fake, true)
  awake.setEnabled(true)
  const running = new Set()
  const feed = (event, session) => { trackTurns(running, { event, session }); awake.setBusy(running.size > 0) }
  feed('turn.started', 'a')
  feed('turn.started', 'b')
  assert.equal(fake.started(), 1)
  feed('turn.done', 'a')
  assert.equal(fake.started(), 1, 'one session still has a turn running')
  feed('text', 'b')
  feed('turn.started', null)
  assert.deepEqual([...running], ['b'], 'other events, and events for no session, change nothing')
  feed('turn.error', 'b')
  assert.equal(fake.started(), 0, 'let go when the last turn fails')
})

test('the main process feeds every event, the window’s setting, the agreement, a close and a quit to it', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const listener = /new Bridge\(\(message\) => \{\s*trackTurns\(runningHandles, message\)\s*caffeinate\?\.setBusy\(runningHandles\.size > 0\)/
  assert.match(index, listener, 'each bridge event updates the turns in flight before anything else')
  assert.match(index, /new Caffeinate\(powerSaveBlocker, readState\(\)\.caffeinateConfirmed\)/)
  assert.match(index, /parseWindowState\(value\)[\s\S]{0,120}caffeinate\?\.setEnabled\(state\.caffeinated\)/)
  assert.match(index, /'bravebot:caffeinate:confirm', \(\) => \{\s*caffeinate\?\.confirm\(\)\s*putCaffeinateConfirmed\(\)/)
  assert.match(index, /window\.on\('closed', \(\) => \{[^}]*caffeinate\?\.windowClosed\(\)/)
  assert.match(index, /app\.on\('before-quit', \(\) => \{\s*caffeinate\?\.release\(\)/)
  assert.equal(index.match(/caffeinate\?\.set(Enabled|Busy)\(/g).length, 2, 'nothing else switches it')
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

test('the window asks first, Cancel leaves it off and asks again, and once agreed it never asks', async () => {
  const { KEPT_OFF, keepAwakeChosen, keepAwakeAgreed } = await dialog()
  // The preload, answered by the main process's own rules.
  const main = new Caffeinate(blocker())
  const preload = { confirmed: async () => main.isConfirmed, confirm: async () => main.confirm() }

  let now = await keepAwakeChosen(KEPT_OFF, preload.confirmed)
  assert.deepEqual(now, { on: false, asking: true }, 'the first time, the dialog and not the setting')
  now = KEPT_OFF // Cancel
  assert.equal(main.isConfirmed, false)
  now = await keepAwakeChosen(now, preload.confirmed)
  assert.deepEqual(now, { on: false, asking: true }, 'asked again after a cancel')

  now = await keepAwakeAgreed(preload.confirm)
  assert.deepEqual(now, { on: true, asking: false })
  assert.equal(main.isConfirmed, true)
  now = await keepAwakeChosen(now, preload.confirmed)
  assert.deepEqual(now, KEPT_OFF, 'chosen again it goes off')
  now = await keepAwakeChosen(now, preload.confirmed)
  assert.deepEqual(now, { on: true, asking: false }, 'and on again with no dialog')

  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /<CaffeinateConfirm onCancel=\{\(\) => setKeepAwake\(KEPT_OFF\)\}/, 'the window’s Cancel is the step above')
  assert.match(app, /caffeinated = keepAwake\.on/, 'and the menu’s tick is the setting')
})

test('the dialog says what staying awake means and offers Cancel and Enable', async () => {
  const { CaffeinateConfirm } = await dialog()
  const markup = renderToStaticMarkup(React.createElement(CaffeinateConfirm, { onCancel() {}, onEnable() {} }))
  assert.ok(markup.includes('data-title="Keep computer awake during turns?"'))
  assert.ok(markup.includes('with your credentials in reach of the turn'))
  assert.ok(markup.includes('It’s off again each time Brave Bot opens.'))
  assert.ok(markup.includes('data-test="caffeinate-cancel"') && markup.includes('data-test="caffeinate-enable"'))
})
