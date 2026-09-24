// What the two Linux packages install, which neither package description states on its own.
//
// A .deb and an .rpm are built from one staged tree by two tools that read different files, so
// the things that can be wrong are the things the two descriptions disagree about, or that the
// tree carries and neither states: an architecture spelled the way a different format spells
// it, a dependency under the name only one release has, a launcher that names a path it cannot
// run, and the mode on the sandbox helper that decides whether the app starts at all.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import {
  ARCHES,
  DEB_DEPENDS,
  EXECUTABLE,
  ICON,
  ICON_SIZES,
  INSTALL_DIR,
  PACKAGE,
  SANDBOX,
  appName,
  appVersion,
  debianControl,
  desktopEntry,
  installedPaths,
  rpmSpec,
  stage,
} from './linux-package.mjs'

// A packaged app as far as anything here reads one: the executable the launcher runs and the
// helper whose mode decides whether it can open a sandbox.
function bundle(t, extra = {}) {
  const root = mkdtempSync(join(tmpdir(), 'linux-package-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const dir = join(root, 'Brave Bot-linux-x64')
  mkdirSync(dir)
  const files = { [EXECUTABLE]: 'ELF', [SANDBOX]: 'ELF', 'resources/app.asar': 'asar', ...extra }
  for (const [name, contents] of Object.entries(files)) {
    mkdirSync(join(dir, name, '..'), { recursive: true })
    writeFileSync(join(dir, name), contents)
  }
  // The agent the app runs, which `make app-bundles-linux` installs executable and the packager
  // copies in as it found it.
  writeFileSync(join(dir, 'resources', 'bravebot-rpc'), 'ELF')
  chmodSync(join(dir, 'resources', 'bravebot-rpc'), 0o755)
  return { root, dir }
}

// What a PNG states about itself in its first twenty four bytes: the signature, then an IHDR
// chunk whose first two fields are the width and the height, big-endian. Read here rather than
// through an image library, which packaging does not have and this should not add.
function bitmap(path) {
  const bytes = readFileSync(path)
  assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], `${path} is not a PNG`)
  assert.equal(bytes.subarray(12, 16).toString('ascii'), 'IHDR', `${path} starts with no header chunk`)
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
}

test('each architecture is named the way the format being built names it', () => {
  assert.deepEqual(ARCHES.amd64, { deb: 'amd64', rpm: 'x86_64' })
  assert.deepEqual(ARCHES.arm64, { deb: 'arm64', rpm: 'aarch64' })
  assert.match(debianControl({ version: '1.2.3', arch: 'arm64', installedSize: 1 }), /^Architecture: arm64$/m)
  assert.match(rpmSpec({ version: '1.2.3', arch: 'arm64' }), /^BuildArch: aarch64$/m)
  assert.match(debianControl({ version: '1.2.3', arch: 'amd64', installedSize: 1 }), /^Architecture: amd64$/m)
  assert.match(rpmSpec({ version: '1.2.3', arch: 'amd64' }), /^BuildArch: x86_64$/m)
})

test('an architecture neither asset name covers is refused rather than packaged as something', () => {
  for (const arch of ['x64', 'aarch64', 'i386']) {
    assert.throws(() => debianControl({ version: '1.2.3', arch, installedSize: 1 }), /--arch is amd64 or arm64/)
    assert.throws(() => rpmSpec({ version: '1.2.3', arch }), /--arch is amd64 or arm64/)
  }
})

// The 64-bit time transition renamed the package rather than changing what is in it, so a
// dependency on one spelling is a package that will not install on the releases that have the
// other. Ubuntu 24.04 has `libasound2t64` and Debian 12 has `libasound2`, and both are
// supported.
test('a renamed dependency is named under every supported release\'s name for it', () => {
  const depends = debianControl({ version: '1.2.3', arch: 'amd64', installedSize: 1 })
    .split('\n').find((line) => line.startsWith('Depends: '))
  for (const alternatives of ['libasound2t64 | libasound2', 'libgtk-3-0t64 | libgtk-3-0', 'libcups2t64 | libcups2']) {
    assert.ok(depends.includes(alternatives), `${alternatives} is not in ${depends}`)
  }
  // The names Fedora has instead, none of which are Debian's.
  assert.equal(DEB_DEPENDS.some((name) => name === 'gtk3' || name === 'alsa-lib'), false)
  assert.match(rpmSpec({ version: '1.2.3', arch: 'amd64' }), /^Requires: gtk3$/m)
  assert.match(rpmSpec({ version: '1.2.3', arch: 'amd64' }), /^Requires: alsa-lib$/m)
})

// A desktop entry's `Exec` is a command line, so the space in the packaged executable's name
// would make it a program and an argument, and the launcher would report that
// `/opt/brave-bot/Brave` does not exist.
test('the launcher runs a path with no space in it, and that path is the packaged executable', (t) => {
  const exec = desktopEntry().split('\n').find((line) => line.startsWith('Exec='))
  const command = exec.slice('Exec='.length)
  assert.equal(command.includes(' '), false, `${exec} is a command and an argument`)

  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })
  const link = join(payload, command.slice(1))
  assert.equal(resolve(join(link, '..'), readlinkSync(link)), join(payload, INSTALL_DIR.slice(1), EXECUTABLE))
  assert.ok(existsSync(join(payload, INSTALL_DIR.slice(1), EXECUTABLE)))
})

// Chromium re-executes this helper to enter a user namespace the calling user cannot open for
// itself, which is what Ubuntu 24.04's AppArmor policy makes the usual case, and refuses to use
// a helper that is not owned by root with the setuid bit set. Without it the app aborts at
// start and the only way to run it is `--no-sandbox`.
test('the sandbox helper is staged setuid, which is what lets the app open a sandbox at all', (t) => {
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })
  assert.equal(statSync(join(payload, INSTALL_DIR.slice(1), SANDBOX)).mode & 0o7777, 0o4755)
  // Every other file keeps what the packager gave it, so the mode above is this line and not a
  // blanket one.
  assert.equal(statSync(join(payload, INSTALL_DIR.slice(1), EXECUTABLE)).mode & 0o4000, 0)
})

// dpkg applies a recorded directory mode to a directory that already exists, so a package whose
// modes came from the builder's umask is one that changes `/usr/bin` on every machine that
// installs it, and two people packaging the same tree produce two different packages.
test('the modes a package records are the same whichever umask staged it', (t) => {
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const umask = process.umask(0o002)
  t.after(() => process.umask(umask))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })

  const modes = []
  const walk = (at) => {
    for (const entry of readdirSync(at, { withFileTypes: true })) {
      const path = join(at, entry.name)
      if (entry.isSymbolicLink()) continue
      modes.push([path, statSync(path).mode & 0o7777])
      if (entry.isDirectory()) walk(path)
    }
  }
  walk(payload)
  assert.ok(modes.length > 0)
  for (const [path, mode] of modes) {
    assert.equal(mode & 0o022, 0, `${path} is ${mode.toString(8)} and writable by more than its owner`)
  }
  for (const [path, mode] of modes.filter(([at]) => statSync(at).isDirectory())) {
    assert.equal(mode, 0o755, `${path} is ${mode.toString(8)}`)
  }
  // Normalising the modes takes nothing off the two files whose own bits matter: the helper the
  // app needs setuid, and the agent it has to be able to run.
  assert.equal(statSync(join(payload, INSTALL_DIR.slice(1), SANDBOX)).mode & 0o7777, 0o4755)
  assert.equal(statSync(join(payload, INSTALL_DIR.slice(1), 'resources', 'bravebot-rpc')).mode & 0o7777, 0o755)
})

// Measured on a packaged bundle under Xvfb: the main window carries
// `WM_CLASS(STRING) = "bravebot-ui", "bravebot-ui"`, because nothing calls `app.setName` and
// the manifest states no product name. A desktop matches `StartupWMClass` against that string,
// so naming the app the way a person reads it leaves the running window ungrouped from the
// launcher icon that started it, which shows up as a second icon in the dock.
test('the launcher entry names the window class the packaged app carries', () => {
  const entry = desktopEntry().split('\n')
  const field = (name) => entry.find((line) => line.startsWith(`${name}=`))?.slice(name.length + 1)
  assert.equal(field('StartupWMClass'), appName())
  assert.notEqual(field('StartupWMClass'), EXECUTABLE)
  assert.notEqual(field('StartupWMClass'), field('Name'))
})

test('the desktop entry names the icon the package installs', (t) => {
  const icon = desktopEntry().split('\n').find((line) => line.startsWith('Icon='))
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })
  const installed = join(payload, 'usr/share/icons/hicolor/scalable/apps', `${icon.slice('Icon='.length)}.svg`)
  assert.ok(existsSync(installed), `${icon} names nothing the package installs`)
  assert.equal(readFileSync(installed, 'utf8'), readFileSync(new URL('../build/icon.svg', import.meta.url), 'utf8'))
})

// gdk-pixbuf identifies an image by sniffing the head of the file rather than by its name, and
// its SVG patterns are the root element and a doctype. A drawing whose first bytes are a
// comment is one it cannot recognise at all, so the scalable icon fails to load even where
// librsvg is installed, and what a desktop reports is an unrecognised file format.
test('the drawing is installed starting with its root element, which is how a loader identifies it', (t) => {
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })
  const installed = readFileSync(join(payload, 'usr/share/icons/hicolor/scalable/apps', `${ICON}.svg`), 'utf8')
  assert.match(installed, /^<(\?xml|svg)[\s>]/, 'the drawing starts with something a loader sniffs past')
})

// The Icon Theme Specification makes PNG the format a desktop has to read and SVG one it may,
// and GTK reads a scalable icon only through librsvg's gdk-pixbuf loader, which neither
// dependency list names. So a package carrying the drawing alone installs a launcher entry that
// shows a generic icon wherever that loader is absent. Each bitmap has to be the size of the
// directory it sits in, since a theme lookup reads the directory and not the file.
test('the icon the entry names is installed as a bitmap at every size the theme is told about', (t) => {
  const name = desktopEntry().split('\n').find((line) => line.startsWith('Icon=')).slice('Icon='.length)
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload } = stage({ bundle: dir, arch: 'amd64', into })

  assert.ok(ICON_SIZES.length > 0)
  for (const size of ICON_SIZES) {
    const installed = join(payload, 'usr/share/icons/hicolor', `${size}x${size}`, 'apps', `${name}.png`)
    assert.ok(existsSync(installed), `${installed} is a size the theme is told about and nothing installs`)
    assert.deepEqual(bitmap(installed), { width: size, height: size }, `${installed} is not ${size} pixels`)
  }
})

// The app ships an agent build rather than versioning separately, so a package naming anything
// else is one whose version no tree states.
test('both packages state the version the front end states', (t) => {
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { control, spec } = stage({ bundle: dir, arch: 'amd64', into })
  const version = appVersion()
  assert.equal(version, JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version)
  assert.match(readFileSync(control, 'utf8'), new RegExp(`^Version: ${version}$`, 'm'))
  assert.match(readFileSync(spec, 'utf8'), new RegExp(`^Version: ${version}$`, 'm'))
})

// rpmbuild fails on a file in the build root that no `%files` entry claims, so a path added to
// the staged tree and not to that list stops the release rather than shipping half a package.
test('the rpm packages every path the staged tree installs', (t) => {
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload, spec } = stage({ bundle: dir, arch: 'amd64', into })
  const files = readFileSync(spec, 'utf8').split('%files\n')[1].split('\n').filter((line) => line.startsWith('/'))
  assert.deepEqual(files, installedPaths())
  for (const path of installedPaths()) {
    assert.ok(existsSync(join(payload, path.slice(1))), `${path} is packaged and not staged`)
  }
  // Every path outside the bundle is claimed by name, so a file added to the staging and left
  // out of the list is found here and not by a release that has already built the bundles. The
  // bundle is the one entry that stands for a directory, since `%files` takes a directory and
  // rpmbuild counts what is under it as packaged.
  const claimed = new Set(installedPaths())
  const walk = (at, path) => {
    for (const entry of readdirSync(at, { withFileTypes: true })) {
      const installed = `${path}/${entry.name}`
      if (installed === INSTALL_DIR) continue
      if (entry.isDirectory()) walk(join(at, entry.name), installed)
      else assert.ok(claimed.has(installed), `${installed} is staged and no %files entry claims it`)
    }
  }
  walk(payload, '')
})

test('a bundle with no packaged app in it is refused, rather than packaged around a broken link', (t) => {
  const { root } = bundle(t)
  const empty = join(root, 'empty')
  mkdirSync(empty)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  assert.throws(() => stage({ bundle: empty, arch: 'amd64', into }), /no packaged app at .*Brave Bot: run `make app-bundles-linux` first/)
})

// The tests above read the staged tree and the files the tools are given. This reads what one of
// the tools makes of them: `dpkg-deb` parses the control file the way `apt` will, and reports
// the archive's contents with the modes it recorded. It is skipped where the tool is absent,
// which is every macOS host; CI's desktop job runs on Linux, where dpkg is part of the system.
test('dpkg builds a package whose contents and metadata are the staged tree', (t) => {
  if (spawnSync('dpkg-deb', ['--version']).status !== 0) return t.skip('no dpkg-deb on this host')
  const { dir } = bundle(t)
  const into = mkdtempSync(join(tmpdir(), 'linux-stage-'))
  t.after(() => rmSync(into, { recursive: true, force: true }))
  const { payload, control } = stage({ bundle: dir, arch: 'arm64', into })

  const built = join(into, 'build')
  mkdirSync(join(built, 'DEBIAN'), { recursive: true })
  spawnSync('cp', ['-a', `${payload}/.`, built])
  spawnSync('cp', [control, join(built, 'DEBIAN', 'control')])
  const deb = join(into, 'out.deb')
  const build = spawnSync('dpkg-deb', ['--build', '--root-owner-group', built, deb], { encoding: 'utf8' })
  assert.equal(build.status, 0, build.stderr)

  const info = spawnSync('dpkg-deb', ['--field', deb], { encoding: 'utf8' }).stdout
  assert.match(info, new RegExp(`^Package: ${PACKAGE}$`, 'm'))
  assert.match(info, /^Architecture: arm64$/m)
  assert.match(info, new RegExp(`^Version: ${appVersion()}$`, 'm'))

  // Owned by root, and setuid: `--root-owner-group` is what makes the first true of a package
  // built by whoever is packaging, and the mode is the staged tree's.
  const contents = spawnSync('dpkg-deb', ['--contents', deb], { encoding: 'utf8' }).stdout
  const sandbox = contents.split('\n').find((line) => line.endsWith(`${INSTALL_DIR}/${SANDBOX}`))
  assert.match(sandbox, /^-rwsr-xr-x root\/root /)
  assert.ok(contents.includes(`/usr/bin/${PACKAGE} -> ../..${INSTALL_DIR}/${EXECUTABLE}`), contents)
  // dpkg applies this to the /usr/bin every machine already has.
  assert.match(contents.split('\n').find((line) => line.endsWith(' ./usr/bin/')), /^drwxr-xr-x root\/root /)
  assert.ok(contents.includes(`/usr/share/applications/${PACKAGE}.desktop`))
  for (const size of ICON_SIZES) {
    assert.ok(contents.includes(`/usr/share/icons/hicolor/${size}x${size}/apps/${ICON}.png`), contents)
  }
  assert.ok(contents.includes(`/usr/share/icons/hicolor/scalable/apps/${ICON}.svg`))
})
