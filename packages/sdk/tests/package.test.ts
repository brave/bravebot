import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

// The manifest of a package that is not published yet (RELEASE-10). Run from dist/tests, so the
// manifest is two directories up. `make check-versions` holds its version to the workspace's.
const manifest = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8')) as Record<string, unknown>

test('the manifest is private until a person publishes the first version', () => {
  assert.equal(manifest.private, true)
})

test('the manifest names what a publish would hold and where it comes from', () => {
  assert.deepEqual(manifest.files, ['dist/src', 'README.md'])
  assert.deepEqual(manifest.publishConfig, { access: 'public' })
  assert.deepEqual(manifest.repository, {
    type: 'git',
    url: 'git+https://github.com/brave/bravebot.git',
    directory: 'packages/sdk',
  })
})

test('installing the package runs no script and fetches no dependency', () => {
  const scripts = Object.keys((manifest.scripts ?? {}) as Record<string, string>)
  assert.deepEqual(scripts.filter((name) => ['preinstall', 'install', 'postinstall', 'prepare'].includes(name)), [])
  for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies']) {
    assert.equal(manifest[field], undefined, field)
  }
})
