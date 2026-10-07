import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { checkDeps, lockedVersions } from './check-deps.mjs'

const LEO = 'https://codeload.github.com/brave/leo/tar.gz/abc'
const LOCK = `lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      '@brave/leo':
        specifier: github:brave/leo#abc
        version: ${LEO}(react@19.1.0)
      react:
        specifier: ^19.0.0
        version: 19.1.0
    devDependencies:
      typescript:
        specifier: ^5.7.2
        version: 5.9.2

packages:

  '@brave/leo@${LEO}':
    resolution: {gitHosted: true, tarball: ${LEO}}
    version: 0.0.1

  react@19.1.0:
    resolution: {integrity: sha512-x}
    version: 19.1.0

snapshots:

  react@19.1.0: {}
`

function fixture(t, { installed }) {
  const root = mkdtempSync(join(tmpdir(), 'check-deps-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  writeFileSync(join(root, 'package.json'), JSON.stringify({
    dependencies: { react: '^19.0.0', '@brave/leo': 'github:brave/leo#abc' },
    devDependencies: { typescript: '^5.7.2' },
  }))
  writeFileSync(join(root, 'pnpm-lock.yaml'), LOCK)
  if (installed) {
    for (const [name, version] of Object.entries(installed)) {
      mkdirSync(join(root, 'node_modules', name), { recursive: true })
      writeFileSync(join(root, 'node_modules', name, 'package.json'), JSON.stringify({ name, version }))
    }
  }
  return root
}

test('a checkout with no node_modules reads as missing', (t) => {
  assert.deepEqual(checkDeps({ root: fixture(t, {}) }), { missing: true, stale: [] })
})

test('an install matching the lockfile passes', (t) => {
  const root = fixture(t, { installed: { react: '19.1.0', typescript: '5.9.2', '@brave/leo': '0.0.1' } })
  assert.deepEqual(checkDeps({ root }), { missing: false, stale: [] })
})

test('a direct dependency absent or at another version is reported', (t) => {
  const root = fixture(t, { installed: { react: '19.0.0' } })
  assert.deepEqual(checkDeps({ root }).stale, [
    { name: 'react', wanted: '19.1.0', installed: '19.0.0' },
    { name: '@brave/leo', wanted: '0.0.1', installed: undefined },
    { name: 'typescript', wanted: '5.9.2', installed: undefined },
  ])
})

test('a git dependency is compared by the version its packages entry records', (t) => {
  const root = fixture(t, { installed: { react: '19.1.0', typescript: '5.9.2', '@brave/leo': '0.0.2' } })
  assert.deepEqual(checkDeps({ root }).stale, [{ name: '@brave/leo', wanted: '0.0.1', installed: '0.0.2' }])
})

test('the committed lockfile yields a version for every direct dependency', () => {
  const root = fileURLToPath(new URL('../', import.meta.url))
  const manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  const locked = lockedVersions(readFileSync(join(root, 'pnpm-lock.yaml'), 'utf8'))
  const names = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
  assert.deepEqual(names.filter((name) => !/^\d+\.\d+\.\d+/.test(locked.get(name) ?? '')), [])
})
