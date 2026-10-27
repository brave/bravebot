// The update notice: what About says, when the sidebar banner shows, and that a dismissal is kept
// on disk and lasts until another version runs (UPDATE-12).
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)
function load(path, electron = {}) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => id === 'electron' ? electron : require(id), module, module.exports)
  return module.exports
}

const update = load('src/shared/update.ts')

test('an update.status answer is read only in its own shape', () => {
  assert.deepEqual(update.parseUpdateStatus({ check: 'on', running: '1.2.0', newer: true }), { check: 'on', running: '1.2.0', newer: true })
  assert.deepEqual(update.parseUpdateStatus({ check: 'on', running: '1.2.0', newer: null }), { check: 'on', running: '1.2.0', newer: null })
  assert.equal(update.parseUpdateStatus({ check: 'off', running: '1.2.0', newer: true }).newer, null, 'a check that is off says nothing about a release')
  for (const bad of [null, 'on', {}, { check: 'maybe', running: '1', newer: true }, { check: 'on', running: '../x', newer: true },
    { check: 'on', running: '1', newer: 'yes' }, { check: 'on', running: 'x'.repeat(65), newer: false }]) {
    assert.equal(update.parseUpdateStatus(bad), null, JSON.stringify(bad))
  }
})

test('About says one of four things, and names no release', () => {
  const at = (check, newer) => ({ check, running: '1.2.0', newer })
  assert.equal(update.updateLine(at('off', null)), 'Update checks are off')
  assert.equal(update.updateLine(at('on', null)), 'Not checked yet')
  assert.equal(update.updateLine(null), 'Not checked yet', 'an answer that could not be read is the same as none')
  assert.equal(update.updateLine(at('on', false)), 'Up to date as of the last check')
  assert.equal(update.updateLine(at('on', true)), 'A newer version is available')
  assert.equal(update.updateState(at('on', true)), 'newer')
})

test('the banner shows for a newer release until this version is dismissed, and comes back for another', () => {
  const newer = { check: 'on', running: '1.2.0', newer: true }
  assert.equal(update.bannerShown(newer, null), true)
  assert.equal(update.bannerShown(newer, '1.2.0'), false, 'dismissed while this version runs')
  assert.equal(update.bannerShown({ ...newer, running: '1.3.0' }, '1.2.0'), true, 'another running version shows it again')
  assert.equal(update.bannerShown({ ...newer, newer: false }, null), false)
  assert.equal(update.bannerShown({ ...newer, newer: null }, null), false)
  assert.equal(update.bannerShown({ ...newer, check: 'off' }, null), false)
  assert.equal(update.bannerShown(null, null), false)
})

test('a dismissal is kept on disk, private, and only in a version\'s shape', (t) => {
  const userData = mkdtempSync(join(tmpdir(), 'bravebot-update-banner-'))
  t.after(() => rmSync(userData, { recursive: true, force: true }))
  const main = load('src/main/experience.ts', { app: { getPath: (name) => { assert.equal(name, 'userData'); return userData } } })
  assert.equal(main.readExperience().dismissedUpdate, null, 'nothing is dismissed at first')
  main.writeExperience('recentModels', ['sample/fast'])
  assert.equal(main.writeExperience('dismissedUpdate', '1.2.0').dismissedUpdate, '1.2.0')
  const file = join(userData, 'experience.json')
  assert.equal(JSON.parse(readFileSync(file, 'utf8')).dismissedUpdate, '1.2.0')
  if (process.platform !== 'win32') assert.equal(statSync(file).mode & 0o777, 0o600)
  // A fresh load reads what the last run left.
  const again = load('src/main/experience.ts', { app: { getPath: () => userData } })
  assert.equal(again.readExperience().dismissedUpdate, '1.2.0')
  assert.deepEqual(again.readExperience().recentModels, ['sample/fast'], 'and keeps what else was there')
  for (const bad of ['../../etc', '', 7, { v: 1 }, 'x'.repeat(65)]) {
    assert.equal(again.writeExperience('dismissedUpdate', bad).dismissedUpdate, null, JSON.stringify(bad))
  }
  assert.equal(again.writeExperience('dismissedUpdate', null).dismissedUpdate, null, 'and can be undone')
})

test('the window dismisses the version it is running, through the experience file', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /setExperience\('dismissedUpdate', [^)]*running/)
  assert.match(app, /bannerShown\(/)
  const banner = readFileSync('src/renderer/components/UpdateBanner.tsx', 'utf8')
  assert.match(banner, /data-test="update-dismiss"/)
  assert.match(banner, /data-test="update-download"/)
})
