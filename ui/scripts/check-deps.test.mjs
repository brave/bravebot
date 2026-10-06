import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { checkDeps } from './check-deps.mjs'

function fixture(t, { installed }) {
  const root = mkdtempSync(join(tmpdir(), 'check-deps-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  writeFileSync(join(root, 'package.json'), JSON.stringify({
    dependencies: { react: '^19.0.0' },
    devDependencies: { typescript: '^5.7.2' },
  }))
  writeFileSync(join(root, 'pnpm-lock.yaml'), [
    // The two documents pnpm 12 writes: the first holds pnpm's own packageManagerDependencies,
    // the second the project's importers, each followed by a `packages:` section.
    '---',
    'lockfileVersion: \'9.0\'',
    '',
    'importers:',
    '',
    '  .:',
    '    configDependencies: {}',
    '    packageManagerDependencies:',
    '      pnpm:',
    '        specifier: 12.9.1',
    '        version: 12.9.1',
    '',
    'packages:',
    '',
    "  '@pnpm/exe@12.9.1':",
    '    resolution: {integrity: sha512-AAA}',
    '',
    '---',
    'lockfileVersion: \'9.0\'',
    '',
    'settings:',
    '  autoInstallPeers: true',
    '',
    'importers:',
    '',
    '  .:',
    '    dependencies:',
    '      react:',
    '        specifier: ^19.0.0',
    '        version: 19.1.0',
    '    devDependencies:',
    '      typescript:',
    '        specifier: ^5.7.2',
    '        version: 5.9.2',
    '',
    'packages:',
    '',
    '  react@19.1.0:',
    '    resolution: {integrity: sha512-AAA}',
  ].join('\n'))
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
  const root = fixture(t, { installed: { react: '19.1.0', typescript: '5.9.2' } })
  assert.deepEqual(checkDeps({ root }), { missing: false, stale: [] })
})

test('a direct dependency absent or at another version is reported', (t) => {
  const root = fixture(t, { installed: { react: '19.0.0' } })
  assert.deepEqual(checkDeps({ root }).stale, [
    { name: 'react', wanted: '19.1.0', installed: '19.0.0' },
    { name: 'typescript', wanted: '5.9.2', installed: undefined },
  ])
})
