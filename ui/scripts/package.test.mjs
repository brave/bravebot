// What a packaged bundle carries, which the bundle itself does not say.
//
// `dist/Brave Bot.app` is named, versioned and laid out identically whichever Rust build went
// into it, and the two builds overwrite each other, so nothing downstream can tell a bundle
// holding a development binary from one meant to be installed. The only place that distinction
// exists is here, where the executables to copy in are chosen.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
// These tests run this repository's own packaging script with a fixed argument list.
import { spawnSync } from 'node:child_process'
import { buildProfile, executableTarget, fusedBinary } from './package.mjs'

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

test('a bundle handed prebuilt executables carries them, from the names they ship under', () => {
  const profile = buildProfile(['--executables=/stage/amd64', '--arch=x64'])
  assert.equal(profile.name, 'prebuilt')
  assert.equal(profile.agent, '/stage/amd64/bravebot-rpc')
  assert.equal(profile.files, '/stage/amd64/bravebot-ui-files')
})

// A fused app ignores `--inspect`, which is how Playwright attaches, so the debug bundle stays
// drivable and everything built for somebody else runs no Node program it is handed.
test('a bundle for somebody else is fused, and the debug bundle the drivers attach to is not', () => {
  assert.equal(buildProfile(['--release']).fused, true)
  assert.equal(buildProfile(['--executables=/stage/arm64']).fused, true)
  assert.equal(buildProfile([]).fused, false)
})

// A PE as far as its machine word: `MZ`, the offset of the real header at 0x3c, and `PE\0\0` with
// the machine behind it there. Nothing fixes that offset, because what lies between the two is a
// DOS stub of whatever length the linker gave it.
function pe(machine, at) {
  const header = Buffer.alloc(at + 6)
  header.write('MZ', 0, 'latin1')
  header.writeUInt32LE(at, 0x3c)
  header.write('PE\0\0', at, 'latin1')
  header.writeUInt16LE(machine, at + 4)
  return header
}

// The first bytes of a 64-bit executable for each platform and architecture, as far as the header
// says which one it is.
const HEADERS = {
  darwin: {
    arm64: Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 0x01, 0, 0, 0, 0]),
    x64: Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0x07, 0, 0, 0x01, 0, 0, 0, 0]),
  },
  linux: {
    arm64: Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0xb7, 0]),
    x64: Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0x3e, 0]),
  },
  win32: {
    arm64: pe(0xaa64, 0x80),
    x64: pe(0x8664, 0x80),
  },
}

// The cross-build names an architecture amd64 and Electron names it x64, so handing one
// architecture's executables to the other's bundle is a one-word slip, and a bundle carrying the
// wrong ones cannot start its agent on an Intel Mac, or on Apple Silicon without Rosetta.
test('an executable header reads as the platform and architecture Electron packages for', () => {
  for (const platform of ['darwin', 'linux']) {
    for (const arch of ['arm64', 'x64']) {
      assert.deepEqual(executableTarget(HEADERS[platform][arch]), { platform, arch })
    }
  }
  assert.equal(executableTarget(Buffer.from('#!/bin/sh\nexec bravebot-rpc\n')), null)
})

// A PE says nothing about itself where the other two do: its first bytes are a DOS stub, and the
// machine word is only reachable through the offset at 0x3c. Two offsets, because the stub the
// two Windows targets' linkers write is not the same length, so a reader that took the machine
// from wherever the first fixture happened to put it would still pass one of them.
test('a Windows executable header reads as the architecture Electron packages for', () => {
  for (const at of [0x80, 0x108]) {
    assert.deepEqual(executableTarget(pe(0x8664, at)), { platform: 'win32', arch: 'x64' })
    assert.deepEqual(executableTarget(pe(0xaa64, at)), { platform: 'win32', arch: 'arm64' })
  }
  // 32-bit x86, which neither the cross-build nor Electron targets, so it is named and not guessed.
  assert.deepEqual(executableTarget(pe(0x14c, 0x80)), { platform: 'win32', arch: 'machine 332' })
})

// Reporting a file as Windows on the strength of its first two bytes would take in every MS-DOS
// program ever written, and reporting one as x64 from a header that was never read would take in
// whatever those bytes happened to be.
test('a file that starts like a Windows executable and is not one reads as no target at all', () => {
  const header = pe(0x8664, 0x80)
  const dosOnly = Buffer.from(header)
  dosOnly.write('NE\0\0', 0x80, 'latin1')
  assert.equal(executableTarget(dosOnly), null)
  assert.equal(executableTarget(header.subarray(0, 0x20)), null)
  assert.equal(executableTarget(header.subarray(0, 0x70)), null)
})

// The fuses go into the file Electron reads them from, which is a different file on each platform.
// A Windows bundle is packaged on a machine that is not Windows, so taking the host's name for it
// would write the fuses into a path that is not there and leave the release unfused.
test('the fuse wire is written to the file its own platform keeps it in', () => {
  const framework = 'Brave Bot.app/Contents/Frameworks/Electron Framework.framework/Electron Framework'
  assert.equal(fusedBinary('/d/Brave Bot-darwin-arm64', 'darwin'), `/d/Brave Bot-darwin-arm64/${framework}`)
  assert.equal(fusedBinary('/d/Brave Bot-win32-x64', 'win32'), '/d/Brave Bot-win32-x64/Brave Bot.exe')
  assert.equal(fusedBinary('/d/Brave Bot-linux-x64', 'linux'), '/d/Brave Bot-linux-x64/Brave Bot')
})

// The fixture above spells the constants the way the code does, so this reads a header neither
// of them wrote: the Electron `npm ci` downloaded for this machine.
test('the Electron installed for this machine reads as this machine', (t) => {
  const dist = fileURLToPath(new URL('../node_modules/electron/dist/', import.meta.url))
  const path = process.platform === 'darwin' ? `${dist}Electron.app/Contents/MacOS/Electron` : `${dist}electron`
  if (!existsSync(path)) return t.skip(`no Electron at ${path}: npm ci runs its install script`)
  assert.deepEqual(executableTarget(readFileSync(path).subarray(0, 20)), { platform: process.platform, arch: process.arch })
})

test('a missing executable is reported with the command that builds that profile', () => {
  assert.equal(buildProfile([]).build, 'npm run bridge')
  assert.equal(buildProfile(['--release']).build, 'make app-bundle')
})

// The cross-build writes the Windows pair with the suffix Windows gives an executable, and the
// bundle carries them under the name they are copied in under, so packaging looking for the
// unsuffixed names would refuse a pair that is sitting right there.
test('a Windows bundle carries the executables under the names Windows gives them', () => {
  const prebuilt = buildProfile(['--executables=/stage/amd64', '--platform=win32', '--arch=x64'])
  assert.equal(prebuilt.agent, '/stage/amd64/bravebot-rpc.exe')
  assert.equal(prebuilt.files, '/stage/amd64/bravebot-ui-files.exe')
  assert.equal(prebuilt.build, 'make app-bundles-windows')
  // The suffix is the bundle's platform and not the host's, so a checkout's own build takes it
  // too: on Windows that is what `cargo build` there writes.
  assert.equal(buildProfile(['--platform=win32']).agent, '../target/debug/bravebot-rpc.exe')
  assert.equal(buildProfile(['--platform=win32', '--release']).files, '../target/release/bravebot-ui-files.exe')
  assert.equal(buildProfile(['--platform=linux', '--executables=/stage/amd64']).agent, '/stage/amd64/bravebot-rpc')
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

// The last row is the pair that does match, which gets past the check and stops at the next thing
// missing from a tree this bare, so the check is shown letting the right executables through as
// well as stopping the wrong ones.
test('packaging refuses executables for another architecture, and an architecture in the cross-build names', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'package-arch-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const front = join(root, 'ui')
  mkdirSync(front)
  writeFileSync(join(front, 'package.json'), '{"version":"0.0.0"}')
  const script = fileURLToPath(new URL('package.mjs', import.meta.url))

  const other = process.arch === 'arm64' ? 'x64' : 'arm64'
  const executables = (name, header) => {
    const dir = join(root, name)
    mkdirSync(dir)
    for (const file of ['bravebot-rpc', 'bravebot-ui-files']) writeFileSync(join(dir, file), header)
    return dir
  }
  const wrong = executables('wrong', HEADERS[process.platform][other])
  const notExecutable = executables('script', Buffer.from('#!/bin/sh\nexec bravebot-rpc\n'))
  const right = executables('right', HEADERS[process.platform][process.arch])

  const here = `${process.platform} ${process.arch}`
  const refusals = [
    [[`--executables=${right}`, '--arch=amd64'], "--arch is arm64 or x64, Electron's names for them, not amd64"],
    [[`--executables=${wrong}`], `${wrong}/bravebot-ui-files is a ${process.platform} ${other} executable, and this is a ${here} bundle`],
    [[`--executables=${notExecutable}`], `${notExecutable}/bravebot-ui-files is not a 64-bit Mach-O, ELF or PE executable, and this is a ${here} bundle`],
    [[`--executables=${right}`, `--arch=${other}`], `${right}/bravebot-ui-files is a ${here} executable, and this is a ${process.platform} ${other} bundle`],
    [[`--executables=${right}`, `--arch=${process.arch}`], 'no bundle in out/: run `electron-vite build` first'],
  ]
  for (const [argv, expected] of refusals) {
    const run = spawnSync(process.execPath, [script, ...argv], { cwd: front, encoding: 'utf8' })
    assert.equal(run.status, 1)
    assert.equal(run.stderr.trim(), expected)
  }
})

// Only Windows is packaged anywhere but on itself, because everything platform-specific about that
// bundle is written by JavaScript. Letting any platform through would produce a macOS bundle whose
// signature was never restored, and letting a Windows one through without prebuilt executables
// would send whoever asked for one to `npm run bridge`, which cannot build for Windows.
test('packaging refuses a platform it does not build, and one this host cannot build', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'package-platform-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const front = join(root, 'ui')
  mkdirSync(front)
  writeFileSync(join(front, 'package.json'), '{"version":"0.0.0"}')
  const script = fileURLToPath(new URL('package.mjs', import.meta.url))

  const here = process.platform
  const elsewhere = here === 'darwin' ? 'linux' : 'darwin'
  const refusals = [
    [['--platform=windows'], "--platform is darwin, linux or win32, Electron's names for them, not windows"],
    [[`--platform=${elsewhere}`], `a ${elsewhere} bundle is built on ${elsewhere}, and this is ${here}`],
    [['--platform=win32'], `a win32 bundle built on ${here} takes --executables=<dir>: this checkout builds for ${here}`],
  ]
  for (const [argv, expected] of refusals) {
    const run = spawnSync(process.execPath, [script, ...argv], { cwd: front, encoding: 'utf8' })
    assert.equal(run.status, 1)
    assert.equal(run.stderr.trim(), expected)
  }
})

// The same check the Mac bundles get, on the platform whose executables are read on a machine that
// cannot run them: nothing else would notice an arm64 pair in the x64 bundle until somebody
// installed it. The last row is the pair that matches, which gets through and stops at the next
// thing a tree this bare is missing.
test('packaging a Windows bundle refuses executables that are not Windows, or not its architecture', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'package-windows-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const front = join(root, 'ui')
  mkdirSync(front)
  writeFileSync(join(front, 'package.json'), '{"version":"0.0.0"}')
  const script = fileURLToPath(new URL('package.mjs', import.meta.url))

  const executables = (name, header) => {
    const dir = join(root, name)
    mkdirSync(dir)
    for (const file of ['bravebot-rpc.exe', 'bravebot-ui-files.exe']) writeFileSync(join(dir, file), header)
    return dir
  }
  const other = executables('other-arch', HEADERS.win32.arm64)
  const notWindows = executables('not-windows', HEADERS[process.platform].x64)
  const right = executables('right', HEADERS.win32.x64)

  const refusals = [
    [other, `${other}/bravebot-ui-files.exe is a win32 arm64 executable, and this is a win32 x64 bundle`],
    [notWindows, `${notWindows}/bravebot-ui-files.exe is a ${process.platform} x64 executable, and this is a win32 x64 bundle`],
    [right, 'no bundle in out/: run `electron-vite build` first'],
  ]
  for (const [dir, expected] of refusals) {
    const argv = [`--executables=${dir}`, '--platform=win32', '--arch=x64']
    const run = spawnSync(process.execPath, [script, ...argv], { cwd: front, encoding: 'utf8' })
    assert.equal(run.status, 1)
    assert.equal(run.stderr.trim(), expected)
  }
})

// ui/docs/testing.md sets up an Apple Silicon checkout with an Intel Rust toolchain, whose own
// build is the other architecture's. Both of its profiles get past the header and stop at the
// next thing a tree this bare is missing.
test("a checkout's own build is packaged whatever architecture its toolchain targets", (t) => {
  const root = mkdtempSync(join(tmpdir(), 'package-own-build-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const front = join(root, 'ui')
  mkdirSync(front)
  writeFileSync(join(front, 'package.json'), '{"version":"0.0.0"}')
  const script = fileURLToPath(new URL('package.mjs', import.meta.url))

  const other = HEADERS[process.platform][process.arch === 'arm64' ? 'x64' : 'arm64']
  for (const profile of ['debug', 'release']) {
    const dir = join(root, 'target', profile)
    mkdirSync(dir, { recursive: true })
    for (const file of ['bravebot-rpc', 'bravebot-ui-files']) writeFileSync(join(dir, file), other)
  }
  for (const argv of [[], ['--release']]) {
    const run = spawnSync(process.execPath, [script, ...argv], { cwd: front, encoding: 'utf8' })
    assert.equal(run.status, 1)
    assert.equal(run.stderr.trim(), 'no bundle in out/: run `electron-vite build` first')
  }
})
