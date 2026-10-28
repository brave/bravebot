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

const checked = '2026-10-20T12:00:00Z'

test('an update.status answer is translated, and what the bridge would not vouch for is null', () => {
  assert.deepEqual(update.parseUpdateStatus({ check: 'on', running: '1.2.0', newer: true, checked }), { check: 'on', running: '1.2.0', newer: true, checked })
  assert.deepEqual(update.parseUpdateStatus({ check: 'on', running: '1.2.0', newer: null, checked: null }), { check: 'on', running: '1.2.0', newer: null, checked: null })
  assert.deepEqual(update.parseUpdateStatus({ check: 'on', running: null, newer: true }), { check: 'on', running: null, newer: true, checked: null },
    'the bridge sends null for a running version it would not vouch for, and the window takes it as sent')
  const off = update.parseUpdateStatus({ check: 'off', running: '1.2.0', newer: true, checked })
  assert.equal(off.newer, null, 'a check that is off says nothing about a release')
  assert.equal(off.checked, null, 'or about when it last heard')
  for (const bad of [null, 'on', {}, { check: 'maybe', running: '1', newer: true }, { check: 'on', running: '1', newer: 'yes' }]) {
    assert.equal(update.parseUpdateStatus(bad), null, JSON.stringify(bad))
  }
  // The version's shape is the bridge's rule (SHARE-001), so the window has no copy of it.
  assert.equal(update.isVersion, undefined)
})

test('About says one of five things, dated where a check was, and names no release', () => {
  const at = (check, newer, when = checked) => ({ check, running: '1.2.0', newer, checked: when })
  const date = (iso) => `<${iso}>`
  assert.equal(update.updateLine(at('off', null, null), date), 'Update checks are off')
  assert.equal(update.updateLine(at('on', null, null), date), 'Not checked yet')
  assert.equal(update.updateLine(null, date), 'Not checked yet', 'an answer that could not be read is the same as none')
  assert.equal(update.updateLine(at('on', false), date), `Up to date as of <${checked}>`)
  assert.equal(update.updateLine(at('on', null), date), `Couldn't check recently (last checked <${checked}>)`,
    'a record the bridge found stale says when it last heard and not what it heard')
  assert.equal(update.updateLine(at('on', true), date), 'A newer version is available')
  assert.equal(update.updateState(at('on', true)), 'newer')
  assert.equal(update.updateState(at('on', null)), 'stale')
  assert.equal(update.updateState(at('on', null, null)), 'unchecked')
})

test('About writes the day of the last check as the window writes every other date', () => {
  const time = load('src/renderer/time.ts')
  const now = Date.UTC(2026, 9, 28, 12)
  assert.equal(time.dayOf(checked, now), time.shortDate(new Date(checked), now))
  assert.equal(time.dayOf(checked, now), 'Oct 20')
  assert.equal(time.dayOf('2025-12-03T12:00:00Z', now), 'Dec 3, 2025', 'with the year where it is not this one')
  const about = readFileSync('src/renderer/components/About.tsx', 'utf8')
  assert.match(about, /updateLine\(update, [^\n]*\bdayOf\(/)
})

test('the banner shows for a fresh newer release until this version is dismissed, and comes back for another', () => {
  const newer = { check: 'on', running: '1.2.0', newer: true, checked }
  assert.equal(update.bannerShown(newer, null), true)
  assert.equal(update.bannerShown(newer, '1.2.0'), false, 'dismissed while this version runs')
  assert.equal(update.bannerShown({ ...newer, running: '1.3.0' }, '1.2.0'), true, 'another running version shows it again')
  assert.equal(update.bannerShown({ ...newer, newer: false }, null), false)
  assert.equal(update.bannerShown({ ...newer, newer: null }, null), false, 'a stale record shows nothing in the sidebar')
  assert.equal(update.bannerShown({ ...newer, check: 'off' }, null), false)
  assert.equal(update.bannerShown({ ...newer, running: null }, null), false, 'a banner with no version to name or dismiss')
  assert.equal(update.bannerShown(null, null), false)
})

test('a dismissal is kept on disk, private, and only as a short text', (t) => {
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
  for (const bad of ['', 7, { v: 1 }, 'x'.repeat(1000)]) {
    assert.equal(again.writeExperience('dismissedUpdate', bad).dismissedUpdate, null, JSON.stringify(bad))
  }
  assert.equal(again.writeExperience('dismissedUpdate', null).dismissedUpdate, null, 'and can be undone')
})

test('the Help item says it shows a status, since it opens About and asks nobody', () => {
  const { command } = load('src/shared/commands.ts')
  assert.equal(command('help.update').label, 'Update Status…')
  const renderer = readFileSync('src/renderer/commands.ts', 'utf8')
  assert.match(renderer, /case 'help\.update':\s*return act\.about\(\)/)
})

test('the window dismisses the version it is running, through the experience file', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  assert.match(app, /setExperience\('dismissedUpdate', [^)]*running/)
  assert.match(app, /bannerShown\(/)
  const banner = readFileSync('src/renderer/components/UpdateBanner.tsx', 'utf8')
  assert.match(banner, /data-test="update-dismiss"/)
  assert.match(banner, /data-test="update-download"/)
})
