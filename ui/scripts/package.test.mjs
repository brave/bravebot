// What a packaged bundle carries, which the bundle itself does not say.
//
// `dist/Brave Bot.app` is named, versioned and laid out identically whichever Rust build went
// into it, and the two builds overwrite each other, so nothing downstream can tell a bundle
// holding a development binary from one meant to be installed. The only place that distinction
// exists is here, where the executables to copy in are chosen.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
// These tests run this repository's own packaging script with a fixed argument list.
import { spawnSync } from 'node:child_process'
import { buildProfile } from './package.mjs'

test('a bundle asked for a release build carries the release executables', () => {
  const profile = buildProfile(['--release'])
  assert.equal(profile.name, 'release')
  assert.equal(profile.agent, '../target/release/bravebot-rpc')
  assert.equal(profile.files, '../target/release/bravebot-ui-files')
})

test('a bundle asked for nothing in particular carries the debug build a checkout already has', () => {
  const profile = buildProfile([])
  assert.equal(profile.name, 'debug')
  assert.equal(profile.agent, '../target/debug/bravebot-rpc')
  assert.equal(profile.files, '../target/debug/bravebot-ui-files')
})

test('an argument that is not the release flag leaves the debug default alone', () => {
  assert.equal(buildProfile(['--linux', '--arch=x64']).name, 'debug')
})

test('a missing executable is reported with the command that builds that profile', () => {
  assert.equal(buildProfile([]).build, 'npm run bridge')
  assert.equal(buildProfile(['--release']).build, 'make app-bundle')
})

// The profile reaches the person as a refusal, so the refusal is what is checked here rather
// than the field alone: packaging a release the release build has not been made for has to say
// so and stop, because the alternative it used to have was packaging whatever debug binary was
// lying in the tree.
test('packaging refuses the profile that has not been built, naming the command that builds it', (t) => {
  // Two levels, because the paths under test are relative to the working directory: the script
  // looks for `../target/`, so a run from the temporary directory itself would be asking about
  // whatever is beside it in the system temporary directory rather than about a tree this test
  // controls.
  const root = mkdtempSync(join(tmpdir(), 'package-refusal-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const front = join(root, 'ui')
  mkdirSync(front)
  writeFileSync(join(front, 'package.json'), '{"version":"0.0.0"}')
  const script = fileURLToPath(new URL('package.mjs', import.meta.url))

  const refusals = [
    [[], 'no debug binary at ../target/debug/bravebot-ui-files: run `npm run bridge` first'],
    [['--release'], 'no release binary at ../target/release/bravebot-ui-files: run `make app-bundle` first'],
  ]
  for (const [argv, expected] of refusals) {
    const run = spawnSync(process.execPath, [script, ...argv], { cwd: front, encoding: 'utf8' })
    assert.equal(run.status, 1)
    assert.equal(run.stderr.trim(), expected)
  }
})
