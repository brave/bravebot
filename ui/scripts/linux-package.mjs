// Lay out a Linux install of the packaged app, and write what a .deb and an .rpm are told
// about it.
//
// `scripts/package.mjs` writes `dist/Brave Bot-linux-<arch>/`, a directory somebody has to
// unpack and run themselves. That is not a way to ship this app. Where unprivileged user
// namespaces are unavailable, which Ubuntu 24.04's AppArmor policy makes the default, Chromium
// falls back to its setuid helper and aborts at start unless `chrome-sandbox` is owned by root
// with mode 4755. Only an installer can set that, so the shipped artefact is a package and not
// a tarball, and `--no-sandbox` is never a supported way to start an agent.
//
// Both formats are built from one staged tree, so the layout, the modes and the launcher are
// decided once here rather than twice in two package descriptions that can drift. The tools
// that pack it are `dpkg-deb` and `rpmbuild` in pinned containers, which the Makefile runs: no
// npm packager, nothing downloaded at build time, and nothing about the payload decided by
// either tool.
import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, statSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

// The name upgrades and conflicts key on, in both formats. Like the macOS bundle id it is fixed
// by the first release: a later rename is a second package that installs beside the first rather
// than over it. It is not the CLI's `bravebot`, which an apt or dnf repository would want to
// ship as a package of its own.
export const PACKAGE = 'brave-bot'

// Where the bundle lands. `/opt/<name>` is where a self-contained application that is not built
// from the distribution's own sources goes, and it is where Chrome and every Electron packager
// put one.
export const INSTALL_DIR = `/opt/${PACKAGE}`

// What `@electron/packager` names the executable inside the bundle, from the application name.
// It has a space in it, which is why the launcher below runs the symlink instead.
export const EXECUTABLE = 'Brave Bot'

// Chromium's setuid sandbox helper, the reason this is a package at all.
export const SANDBOX = 'chrome-sandbox'

// The name in the icon theme, which is what `Icon=` in the desktop entry resolves. The file
// installed under it keeps that name and not the source's.
export const ICON = PACKAGE

// The asset's name for an architecture, and what each package format calls the same one. Three
// spellings of two architectures: a package built for `arm64` and declared `x64`, or an rpm
// declared `arm64`, is one the package manager either refuses or installs on the wrong machine.
export const ARCHES = {
  amd64: { deb: 'amd64', rpm: 'x86_64' },
  arm64: { deb: 'arm64', rpm: 'aarch64' },
}

// What Electron needs from the distribution, in the names Debian and Ubuntu use. Alternatives
// where the two disagree: Ubuntu 24.04's 64-bit time transition renamed a package rather than
// changing it, so `libasound2t64` and `libasound2` are the same library under the only name
// each release has for it, and naming one of them makes the package uninstallable on the other.
// A dependency this list is missing is a package that installs and then fails at start with a
// message about a shared object, which is why they are declared rather than left to whatever a
// desktop happens to have.
export const DEB_DEPENDS = [
  'libasound2t64 | libasound2',
  'libatk-bridge2.0-0t64 | libatk-bridge2.0-0',
  'libatk1.0-0t64 | libatk1.0-0',
  'libcairo2',
  'libcups2t64 | libcups2',
  'libdbus-1-3',
  'libdrm2',
  'libexpat1',
  'libgbm1',
  'libglib2.0-0t64 | libglib2.0-0',
  'libgtk-3-0t64 | libgtk-3-0',
  'libnspr4',
  'libnss3',
  'libpango-1.0-0',
  'libx11-6',
  'libxcb1',
  'libxcomposite1',
  'libxdamage1',
  'libxext6',
  'libxfixes3',
  'libxkbcommon0',
  'libxrandr2',
  // The app opens a link in the person's browser rather than in a window of its own.
  'xdg-utils',
]

// The same libraries in Fedora's names, which are not Debian's for any of them.
export const RPM_REQUIRES = [
  'alsa-lib',
  'at-spi2-atk',
  'atk',
  'cairo',
  'cups-libs',
  'dbus-libs',
  'expat',
  'glib2',
  'gtk3',
  'libX11',
  'libXcomposite',
  'libXdamage',
  'libXext',
  'libXfixes',
  'libXrandr',
  'libdrm',
  'libxkbcommon',
  'mesa-libgbm',
  'nspr',
  'nss',
  'pango',
  'xdg-utils',
]

const SUMMARY = 'A general-purpose agent resistant to prompt injection'

// The address a package manager shows and a bug report goes to. Both formats want one, and
// neither treats it as decoration: `apt` prints it, and an rpm with no `Packager` is one nobody
// can trace.
const MAINTAINER = 'Brave Software, Inc. <support@brave.com>'

const HOMEPAGE = 'https://github.com/brave/bravebot'

// The manifest the app is packaged from. Both values below are read from it rather than passed
// in, so a package cannot be built naming a version no tree states, and the launcher entry
// cannot name a window class no window carries.
function manifest() {
  return JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
}

// `make check-versions` is what holds this manifest's version to the workspace's.
export function appVersion() {
  return manifest().version
}

// What Electron calls the application, which is not "Brave Bot": nothing here calls
// `app.setName` and the manifest states no product name, so the name is the package's. It is
// what Chromium puts in the main window's `WM_CLASS`, measured on the packaged bundle:
//
//     WM_CLASS(STRING) = "bravebot-ui", "bravebot-ui"
export function appName() {
  return manifest().name
}

// What a desktop shows in its launcher, and what it runs from there.
//
// `Exec` is the symlink in `/usr/bin` rather than the executable itself, because the executable's
// name has a space in it. A desktop entry's `Exec` is a command line, so an unquoted path with a
// space in it is a program and an argument, and the launcher reports that `/opt/brave-bot/Brave`
// does not exist. Quoting it is legal and several launchers get it wrong, so the entry names a
// path that needs none.
//
// `StartupWMClass` is what groups the window that opens with the icon that opened it, and the
// value has to be the class the window actually carries rather than anything the launcher says:
// a desktop matches the two as strings, and a mismatch is a second icon in the dock.
export function desktopEntry() {
  return [
    '[Desktop Entry]',
    'Type=Application',
    'Name=Brave Bot',
    `Comment=${SUMMARY}`,
    `Exec=/usr/bin/${PACKAGE}`,
    `Icon=${ICON}`,
    'Terminal=false',
    'Categories=Development;Utility;',
    `StartupWMClass=${appName()}`,
    '',
  ].join('\n')
}

// The .deb's metadata. `Installed-Size` is in kibibytes and is what apt reports as the disk a
// person is about to spend; without it every install of this package claims to need nothing.
export function debianControl({ version, arch, installedSize }) {
  const { deb } = architecture(arch)
  return [
    `Package: ${PACKAGE}`,
    `Version: ${version}`,
    `Architecture: ${deb}`,
    `Maintainer: ${MAINTAINER}`,
    `Installed-Size: ${installedSize}`,
    `Depends: ${DEB_DEPENDS.join(', ')}`,
    'Section: devel',
    'Priority: optional',
    `Homepage: ${HOMEPAGE}`,
    `Description: ${SUMMARY}`,
    ' Brave Bot is a desktop interface to the bravebot agent. Untrusted content can be',
    ' carried and written, and it never decides what happens.',
    '',
  ].join('\n')
}

// The .rpm's. Three things are turned off, each of which would otherwise act on a binary that is
// already finished:
//
//   * `__os_install_post` is the set of scripts that strip executables and rewrite them. The
//     Electron binary carries fuses written into it after packaging and an asar integrity hash
//     that covers what it loads, so a build that rewrites it ships an app that refuses to start.
//   * `debug_package` and `_build_id_links` are the debuginfo machinery, which has nothing to
//     extract from a binary somebody else built and fails rather than saying so.
//   * `AutoReqProv` reads every ELF file and turns its shared objects into dependencies on the
//     build host's packages. The build host is a container, not a supported distribution, so
//     what it produces is a list of sonames that release does not have.
//
// `%install` copies rather than being handed the tree, because rpmbuild empties the build root
// before it runs. `cp -a` is what carries the modes, the setuid bit included.
//
// The architecture is `ExclusiveArch` rather than `BuildArch`, because rpmbuild refuses a
// `BuildArch` the host it runs on cannot execute, and both architectures are packaged on one host.
// The build is given the architecture as its `--target` instead, which the Makefile reads from
// this line, and `ExclusiveArch` refuses any other target, so a build given none or the wrong one
// stops rather than labelling the package with the host's architecture.
export function rpmSpec({ version, arch }) {
  const { rpm } = architecture(arch)
  return [
    '%global __os_install_post %{nil}',
    '%global debug_package %{nil}',
    '%global _build_id_links none',
    '',
    `Name: ${PACKAGE}`,
    `Version: ${version}`,
    'Release: 1',
    `Summary: ${SUMMARY}`,
    'License: MPL-2.0',
    `URL: ${HOMEPAGE}`,
    `Packager: ${MAINTAINER}`,
    `ExclusiveArch: ${rpm}`,
    'AutoReqProv: no',
    ...RPM_REQUIRES.map((name) => `Requires: ${name}`),
    '',
    '%description',
    'Brave Bot is a desktop interface to the bravebot agent. Untrusted content can be',
    'carried and written, and it never decides what happens.',
    '',
    '%install',
    'cp -a %{_sourcedir}/payload/. %{buildroot}/',
    '',
    // `-` for the mode is what carries each file's own, which is where the setuid bit on the
    // sandbox helper is set. Naming it a second time here would be the same file listed twice,
    // which rpmbuild refuses.
    '%files',
    '%defattr(-,root,root,-)',
    ...installedPaths(),
    '',
  ].join('\n')
}

// Every path the package owns, which is the same list both formats are built from: the .rpm
// names them and the .deb takes whatever is in the tree. A path staged and left out of this
// list fails the rpm build as an unpackaged file, which is the right end to find it at.
export function installedPaths() {
  return [
    INSTALL_DIR,
    `/usr/bin/${PACKAGE}`,
    `/usr/share/applications/${PACKAGE}.desktop`,
    `/usr/share/icons/hicolor/scalable/apps/${ICON}.svg`,
  ]
}

function architecture(arch) {
  const named = ARCHES[arch]
  if (named === undefined) {
    throw new Error(`--arch is ${Object.keys(ARCHES).join(' or ')}, the names the assets use, not ${arch}`)
  }
  return named
}

// The mode a package records is the mode the installed file gets, and dpkg applies a recorded
// directory mode to a directory that already exists. So a tree staged under a lax umask ships a
// package that makes `/usr/bin` group-writable on every machine that installs it, and what the
// package does depends on who built it. Directories are 0755 and nothing is writable by anyone
// but its owner; the executable and setuid bits the bundle carries are left where they are.
//
// Symlinks are passed over: `chmod` follows one, and Linux has no way to set a link's own mode.
function normalise(dir) {
  chmodSync(dir, 0o755)
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isSymbolicLink()) continue
    if (entry.isDirectory()) normalise(path)
    else chmodSync(path, statSync(path).mode & 0o7777 & ~0o022)
  }
}

// Kibibytes, rounded up per file the way dpkg reports it: a tree of many small files takes more
// disk than the sum of their sizes.
function installedSize(dir) {
  let blocks = 0
  for (const entry of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (!entry.isFile()) continue
    blocks += Math.ceil(statSync(join(entry.parentPath, entry.name)).size / 1024)
  }
  return blocks
}

// Write the install tree and the two package descriptions into `into`, and return where each
// went. Nothing here runs a packaging tool: the tree is what the containers pack, and it is the
// same tree for both, so the layout cannot differ between the formats a person installs.
export function stage({ bundle, arch, into }) {
  // Before the copy rather than at the control file it is read for, so an architecture nothing
  // names is refused in a moment instead of after a few hundred megabytes.
  architecture(arch)
  const executable = join(bundle, EXECUTABLE)
  if (!existsSync(executable)) {
    throw new Error(`no packaged app at ${executable}: run \`make app-bundles-linux\` first`)
  }

  const payload = join(into, 'payload')
  const opt = join(payload, INSTALL_DIR.slice(1))
  mkdirSync(payload, { recursive: true })
  cpSync(bundle, opt, { recursive: true })

  // Root's, and setuid: Chromium re-executes this helper to enter a namespace the calling user
  // cannot, and refuses to use it when it is neither. The mode is set here rather than in either
  // package description because both formats carry what the tree has, so setting it twice is
  // two chances to set it once.
  chmodSync(join(opt, SANDBOX), 0o4755)

  // Relative, so it is still the same link if the whole tree is examined from somewhere else.
  const bin = join(payload, 'usr', 'bin')
  mkdirSync(bin, { recursive: true })
  symlinkSync(`../..${INSTALL_DIR}/${EXECUTABLE}`, join(bin, PACKAGE))

  const applications = join(payload, 'usr', 'share', 'applications')
  mkdirSync(applications, { recursive: true })
  writeFileSync(join(applications, `${PACKAGE}.desktop`), desktopEntry())

  // The drawing itself, in the theme's scalable directory, rather than a set of sizes rendered
  // from it: every desktop that reads a `.desktop` file also loads an SVG through the same
  // library GTK draws with, and rendering would put a rasteriser in a build that otherwise
  // needs none.
  const icons = join(payload, 'usr', 'share', 'icons', 'hicolor', 'scalable', 'apps')
  mkdirSync(icons, { recursive: true })
  cpSync(new URL('../build/icon.svg', import.meta.url), join(icons, `${ICON}.svg`))

  normalise(payload)

  const version = appVersion()
  const control = join(into, 'control')
  writeFileSync(control, debianControl({ version, arch, installedSize: installedSize(payload) }))
  const spec = join(into, `${PACKAGE}.spec`)
  writeFileSync(spec, rpmSpec({ version, arch }))
  return { payload, control, spec }
}

function main() {
  const argv = process.argv.slice(2)
  const value = (name) => argv.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3)
  for (const name of ['bundle', 'arch', 'stage']) {
    if (value(name) === undefined) {
      console.error(`usage: node scripts/linux-package.mjs --bundle=<dir> --arch=<${Object.keys(ARCHES).join('|')}> --stage=<dir>`)
      process.exit(1)
    }
  }
  try {
    const { payload, control, spec } = stage({ bundle: value('bundle'), arch: value('arch'), into: value('stage') })
    console.log(`staged: ${payload}`)
    console.log(`wrote: ${control}`)
    console.log(`wrote: ${spec}`)
  } catch (error) {
    console.error(error.message)
    process.exit(1)
  }
}

// Run when this file is the program, and not when a test imports it. Through the real path both
// ways, for the reason `scripts/package.mjs` says.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main()
}
