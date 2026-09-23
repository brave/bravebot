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
import { buildProfile, executableTarget } from './package.mjs'

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
    [[`--executables=${notExecutable}`], `${notExecutable}/bravebot-ui-files is not a 64-bit Mach-O or ELF executable, and this is a ${here} bundle`],
    [[`--executables=${right}`, `--arch=${other}`], `${right}/bravebot-ui-files is a ${here} executable, and this is a ${process.platform} ${other} bundle`],
    [[`--executables=${right}`, `--arch=${process.arch}`], 'no bundle in out/: run `electron-vite build` first'],
  ]
  for (const [argv, expected] of refusals) {
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
