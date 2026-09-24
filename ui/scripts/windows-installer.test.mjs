// What goes into the Windows installer and what it is called, which a machine that is not Windows
// can check without running it.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { SENTINEL } from './fuses.mjs'
import { executableTarget } from './package.mjs'
import { HELPERS, assetName, buildInstaller, checkBundle, checkHost, installerConfig } from './windows-installer.mjs'

const X64 = 0x8664
const ARM64 = 0xaa64

// Electron 44's wire as a release fuses it, and as it ships.
const FUSED = '000011011'
const SHIPPED = '101100011'

// A PE as far as its machine word, then a fuse wire: all an installer reads of an executable.
function executable(machine, wire) {
  const header = Buffer.alloc(0x86)
  header.write('MZ', 0, 'latin1')
  header.writeUInt32LE(0x80, 0x3c)
  header.write('PE\0\0', 0x80, 'latin1')
  header.writeUInt16LE(machine, 0x84)
  if (wire === undefined) return header
  return Buffer.concat([header, Buffer.from(SENTINEL), Buffer.from([1, wire.length]), Buffer.from(wire, 'latin1')])
}

// A bundle with the three executables `scripts/package.mjs` puts in one, and nothing else.
function bundle(root, name, { machine = X64, wire = FUSED, helper = machine, helpers = HELPERS } = {}) {
  const dir = join(root, name)
  mkdirSync(join(dir, 'resources'), { recursive: true })
  writeFileSync(join(dir, 'Brave Bot.exe'), executable(machine, wire))
  for (const file of helpers) writeFileSync(join(dir, 'resources', file), executable(helper))
  return dir
}

// A string from a PE's version resource: the key in UTF-16 and its terminator, the padding that
// aligns the value, and the value up to its own terminator.
function versionString(bytes, key) {
  const tag = Buffer.from(`${key}\0`, 'utf16le')
  const at = bytes.indexOf(tag)
  if (at === -1) return undefined
  let start = at + tag.length
  while (bytes.readUInt16LE(start) === 0) start += 2
  let end = start
  while (bytes.readUInt16LE(end) !== 0) end += 2
  return bytes.subarray(start, end).toString('utf16le')
}

// The product version in a PE's VS_FIXEDFILEINFO, the one Windows compares, as `a.b.c.d`.
function fixedVersion(bytes) {
  const at = bytes.indexOf(Buffer.from([0xbd, 0x04, 0xef, 0xfe]))
  const ms = bytes.readUInt32LE(at + 16)
  const ls = bytes.readUInt32LE(at + 20)
  return [ms >>> 16, ms & 0xffff, ls >>> 16, ls & 0xffff].join('.')
}

function digests(dir) {
  const found = {}
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (!entry.isFile()) continue
    const path = join(entry.parentPath, entry.name)
    found[path.slice(dir.length)] = createHash('sha256').update(readFileSync(path)).digest('hex')
  }
  return found
}

// The CLI beside it on the releases page is `bravebot-windows-amd64.exe`, and the Makefile has both
// spellings of each architecture to hand, so Electron's name reaching here is a one-word slip that
// would publish an asset under a name no download link points at.
test("an installer is named for its architecture the way the release's assets name it", () => {
  assert.equal(assetName('amd64'), 'bravebot-desktop-windows-amd64-setup.exe')
  assert.equal(assetName('arm64'), 'bravebot-desktop-windows-arm64-setup.exe')
  for (const arch of ['x64', 'constructor']) {
    assert.throws(() => assetName(arch), { message: `--arch is amd64 or arm64, the names the assets use, not ${arch}` })
  }
})

// Each of these is permanent from the first release. An upgrade finds the install it replaces
// through the uninstall key, and the directory and scope say where that install is, so an
// installer that changes any of them installs a second copy beside the first.
test('the install directory, the uninstall key, the per-user scope and the Apps list name are the ones the first release fixes', () => {
  const config = installerConfig({ arch: 'amd64' })
  assert.equal(config.extraMetadata.name, 'bravebot-desktop')
  assert.equal(config.nsis.guid, 'bravebot-desktop')
  assert.equal(config.nsis.uninstallDisplayName, 'Brave Bot Desktop')
  assert.equal(config.nsis.oneClick, true)
  assert.equal(config.nsis.perMachine, false)
  assert.equal(config.productName, 'Brave Bot Desktop')
  assert.equal(config.executableName, 'Brave Bot')
})

// On Linux electron-builder would run the uninstaller's stub under a Wine nothing here pins, and
// the failure without one arrives after the whole bundle has been compressed.
test('a Windows installer is refused on Linux, where writing its uninstaller needs Wine', () => {
  assert.throws(() => checkHost('linux'), {
    message: 'a Windows installer is built on Windows or macOS, and this is linux: electron-builder needs Wine to write the uninstaller here',
  })
  checkHost('darwin')
  checkHost('win32')
})

// The bundle is what the release job signed, so a wrong one is a signed installer for the wrong
// machine, or one whose app ignores the fuses a release sets. The two that are right get through.
test('a bundle missing an executable, built for the other architecture, or not fused is refused', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'windows-installer-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))

  const noHelper = bundle(root, 'no-helper', { helpers: ['bravebot-rpc.exe'] })
  const otherArch = bundle(root, 'other-arch', { machine: ARM64 })
  const otherHelper = bundle(root, 'other-helper', { helper: ARM64 })
  const notFused = bundle(root, 'not-fused', { wire: SHIPPED })
  const noApp = join(root, 'no-app')
  mkdirSync(noApp)

  const refusals = [
    [noApp, `no ${join(noApp, 'Brave Bot.exe')}: run \`make app-bundles-windows\` first`],
    [noHelper, `no ${join(noHelper, 'resources', 'bravebot-ui-files.exe')}: run \`make app-bundles-windows\` first`],
    [otherArch, `${join(otherArch, 'Brave Bot.exe')} is a win32 arm64 executable, and this is the amd64 installer`],
    [otherHelper, `${join(otherHelper, 'resources', 'bravebot-rpc.exe')} is a win32 arm64 executable, and this is the amd64 installer`],
    [notFused, `${join(notFused, 'Brave Bot.exe')} is not fused, so it is not a release bundle: run \`make app-bundles-windows\``],
  ]
  for (const [dir, message] of refusals) {
    assert.throws(() => checkBundle({ bundle: dir, arch: 'amd64' }), { message })
  }
  checkBundle({ bundle: bundle(root, 'x64'), arch: 'amd64' })
  checkBundle({ bundle: bundle(root, 'arm64', { machine: ARM64 }), arch: 'arm64' })
})

// The signatures the release job put on the bundle are the ones Windows checks when the app starts,
// so the bundle has to reach the installer as it was handed over: a file electron-builder added to
// it, or rewrote, would be one nobody signed. Both paths are relative, as the Makefile's are, and
// from a directory that is not ui/, since electron-builder resolves a relative bundle against ui/.
// The build number is a CI's, which electron-builder would otherwise put in the version.
// Needs the NSIS toolset electron-builder downloads to its cache on first use.
test('the installer carries the bundle as it was handed over, under the asset name and nothing else', async (t) => {
  const root = mkdtempSync(join(tmpdir(), 'windows-installer-'))
  const cwd = process.cwd()
  const buildNumber = process.env.BUILD_NUMBER
  t.after(() => {
    process.chdir(cwd)
    if (buildNumber === undefined) delete process.env.BUILD_NUMBER
    else process.env.BUILD_NUMBER = buildNumber
    rmSync(root, { recursive: true, force: true })
  })
  const dir = bundle(root, 'Brave Bot-win32-x64')
  process.chdir(root)
  process.env.BUILD_NUMBER = '412'
  if (process.platform === 'linux') {
    await assert.rejects(buildInstaller({ bundle: 'Brave Bot-win32-x64', arch: 'amd64', out: 'out' }), { message: /needs Wine/ })
    return
  }
  const before = digests(dir)

  const written = await buildInstaller({ bundle: 'Brave Bot-win32-x64', arch: 'amd64', out: 'out' })

  assert.deepEqual(digests(dir), before)
  assert.equal(written, join('out', 'bravebot-desktop-windows-amd64-setup.exe'))
  assert.deepEqual(readdirSync(join(root, 'out')), ['bravebot-desktop-windows-amd64-setup.exe'])
  const installer = readFileSync(written)
  assert.equal(executableTarget(installer)?.platform, 'win32')
  // What the installer's properties dialog shows before anyone runs it.
  const { version } = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
  assert.equal(versionString(installer, 'ProductName'), 'Brave Bot Desktop')
  assert.equal(versionString(installer, 'FileDescription'), 'Brave Bot Desktop')
  assert.equal(versionString(installer, 'CompanyName'), 'Brave Software, Inc.')
  assert.equal(versionString(installer, 'LegalCopyright'), 'Copyright (c) Brave Software, Inc. All rights reserved.')
  assert.equal(versionString(installer, 'ProductVersion'), version)
  assert.equal(versionString(installer, 'FileVersion'), version)
  assert.equal(fixedVersion(installer), `${version}.0`)
})
