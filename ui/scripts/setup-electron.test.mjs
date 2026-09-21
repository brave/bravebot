import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setupElectron } from './setup-electron.mjs'

function fixture(t, installer) {
  const root = mkdtempSync(join(tmpdir(), 'electron-setup-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const dep = join(root, 'node_modules/electron')
  mkdirSync(dep, { recursive: true })
  writeFileSync(join(root, 'package.json'), '{}')
  writeFileSync(join(dep, 'package.json'), '{"name":"electron"}')
  writeFileSync(join(dep, 'install.js'), installer)
  return { root, dep }
}

test('runs the dependency installer with Node and inherits download configuration', (t) => {
  const { root, dep } = fixture(t, `
    require('fs').writeFileSync(require('path').join(__dirname, 'path.txt'), process.env.ELECTRON_MIRROR)
  `)
  setupElectron({ root, env: { ...process.env, ELECTRON_SKIP_BINARY_DOWNLOAD: '', ELECTRON_MIRROR: 'test-mirror' } })
  assert.equal(readFileSync(join(dep, 'path.txt'), 'utf8'), 'test-mirror')
})

test('CI skip avoids invoking the installer', (t) => {
  const { root, dep } = fixture(t, `require('fs').writeFileSync(require('path').join(__dirname, 'ran'), '')`)
  setupElectron({ root, env: { ELECTRON_SKIP_BINARY_DOWNLOAD: '1' } })
  assert.equal(existsSync(join(dep, 'ran')), false)
})

test('an installer failure fails setup with a recovery command', (t) => {
  const { root } = fixture(t, 'process.exit(1)')
  assert.throws(() => setupElectron({ root, env: {} }), /retry npm run setup:electron/)
})
