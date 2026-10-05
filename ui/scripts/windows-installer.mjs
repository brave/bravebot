// Put a signed Windows bundle in the installer a person runs.
//
// `scripts/package.mjs --platform=win32` writes `dist/Brave Bot-win32-<arch>/`, fused and unsigned,
// and the release job signs every PE in it where it lies. This builds the NSIS installer around that
// directory with electron-builder, handed it as `prepackaged`: electron-builder then packs nothing
// and edits nothing in it, so the fuses, the version resource and the signatures the bundle arrived
// with are the ones that get installed.
//
// The installer is one click and per user, as issue #769 settled: no administrator prompt, the app
// in `%LOCALAPPDATA%\Programs\bravebot-desktop`, a Start menu entry, and an entry in the Apps list
// that uninstalls it. A newer installer finds the install it replaces through the uninstall key and
// runs that install's own uninstaller first, which leaves the app's data where it is.
import { build, Arch, Platform } from 'electron-builder'
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { RELEASE_FUSES, hasFuses } from './fuses.mjs'
import { executableTarget, fusedBinary } from './package.mjs'

// The name wherever Windows needs an identifier: the install directory, the uninstall key, and the
// asset. The first release fixes it. A later rename installs a second copy beside the first rather
// than upgrading it, since the uninstall key is how an upgrade finds the install it replaces.
export const NAME = 'bravebot-desktop'

// The name a person sees: the Start menu entry, the Apps list, and the installer's own properties.
export const DISPLAY_NAME = 'Brave Bot Desktop'

// What `scripts/package.mjs` names the executable, which is what the Start menu entry runs.
export const EXECUTABLE = 'Brave Bot'

// The two executables `scripts/package.mjs` copies into the bundle's resources for the app to spawn.
export const HELPERS = ['bravebot-rpc.exe', 'bravebot-ui-files.exe']

// The asset's name for each architecture, and Electron's name for the same one.
export const ARCHES = { amd64: 'x64', arm64: 'arm64' }

// The project electron-builder is run in: the front end's manifest supplies the version, and the
// icon is the one the bundle's executable carries.
const UI = fileURLToPath(new URL('../', import.meta.url))
const { version } = JSON.parse(readFileSync(join(UI, 'package.json'), 'utf8'))

function electronArch(arch) {
  if (!Object.hasOwn(ARCHES, arch)) {
    throw new Error(`--arch is ${Object.keys(ARCHES).join(' or ')}, the names the assets use, not ${arch}`)
  }
  return ARCHES[arch]
}

// NSIS refuses to write an installer named `setup.exe`, for the compatibility shims Windows applies
// to that name, so the architecture and the product go in front of it.
export function assetName(arch) {
  electronArch(arch)
  return `${NAME}-windows-${arch}-setup.exe`
}

// electron-builder writes the uninstaller by running a build of the installer whose only job is to
// write it. That runs natively on Windows, and on macOS electron-builder reads the uninstaller out of
// it in JavaScript. On Linux it runs it under Wine, which nothing here installs or pins.
export function checkHost(platform) {
  if (platform !== 'win32' && platform !== 'darwin') {
    throw new Error(`a Windows installer is built on Windows or macOS, and this is ${platform}: electron-builder needs Wine to write the uninstaller here`)
  }
}

// The bundle is checked before it is compressed into an installer. The architecture, because the two
// architectures' bundles sit side by side under names one word apart. The fuses, because a debug
// bundle packaged on a Windows checkout has the release's directory name and is not fused.
export function checkBundle({ bundle, arch }) {
  const electron = electronArch(arch)
  const executable = fusedBinary(bundle, 'win32')
  for (const file of [executable, ...HELPERS.map((name) => join(bundle, 'resources', name))]) {
    if (!existsSync(file)) throw new Error(`no ${file}: run \`make app-bundles-windows\` first`)
    const bytes = readFileSync(file)
    const target = executableTarget(bytes)
    if (target?.platform !== 'win32' || target.arch !== electron) {
      const is = target ? `a ${target.platform} ${target.arch} executable` : 'not a 64-bit Mach-O, ELF or PE executable'
      throw new Error(`${file} is ${is}, and this is the ${arch} installer`)
    }
    if (file === executable && !hasFuses(bytes, RELEASE_FUSES)) {
      throw new Error(`${file} is not fused, so it is not a release bundle: run \`make app-bundles-windows\``)
    }
  }
}

export function installerConfig({ arch }) {
  return {
    // The AppUserModelID the Start menu entry carries, which Windows groups the app's windows and
    // notifications under. The same as the macOS bundle id, which is the app's identity there.
    appId: 'com.brave.bravebot',
    productName: DISPLAY_NAME,
    executableName: EXECUTABLE,
    // Without this, electron-builder writes its own line with this year in it.
    copyright: 'Copyright (c) Brave Software, Inc. All rights reserved.',
    // Otherwise a CI's BUILD_NUMBER becomes the installer's fourth version part, and not the app's.
    buildVersion: version,
    buildNumber: '0',
    extraMetadata: {
      name: NAME,
      // The Start menu entry's tooltip, the Apps list's comment, and the installer's description.
      // The manifest's own says macOS and Linux.
      description: DISPLAY_NAME,
      // Read as `author.name`, so a bare string would leave the company out.
      author: { name: 'Brave Software, Inc.' },
    },
    publish: null,
    win: { icon: join(UI, 'build', 'icon.ico') },
    nsis: {
      oneClick: true,
      perMachine: false,
      guid: NAME,
      // electron-builder's default appends the version, which renames the Apps list entry every release.
      uninstallDisplayName: DISPLAY_NAME,
      // Otherwise electron-builder copies its unsigned `elevate.exe` into the signed bundle. A
      // per-user install never elevates.
      packElevateHelper: false,
      // A one-click installer asks nothing, so a desktop shortcut would be one nobody chose.
      createDesktopShortcut: false,
      // Its only use is the blockmap electron-updater downloads updates by, and the app has no updater.
      differentialPackage: false,
      artifactName: assetName(arch),
    },
  }
}

// The 7-Zip electron-builder compresses the bundle with puts each ARM64 executable through a filter
// the 7-Zip the installer extracts it with predates. The installer skips every file it cannot
// decode and still exits 0, so an arm64 install left to that has no `Brave Bot.exe`, no helpers
// and no DLLs.
// A filter named here goes on every file instead, at about a tenth more in size, and BCJ is one
// both have. The x64 executables get BCJ2 unasked, which both have too.
const ARCHIVE_FILTERS = { arm64: 'BCJ' }

// Build the installer for one architecture's bundle into `out`, and return its path. electron-builder
// writes into a scratch directory, since it leaves its logs and intermediate files beside the
// installer, and only the installer is copied out.
export async function buildInstaller({ bundle, arch, out }) {
  checkHost(process.platform)
  checkBundle({ bundle, arch })
  const scratch = mkdtempSync(join(tmpdir(), 'windows-installer-'))
  const filter = process.env.ELECTRON_BUILDER_7Z_FILTER
  if (Object.hasOwn(ARCHIVE_FILTERS, arch)) process.env.ELECTRON_BUILDER_7Z_FILTER = ARCHIVE_FILTERS[arch]
  try {
    await build({
      projectDir: UI,
      // electron-builder resolves a relative path against the project, not the working directory.
      prepackaged: resolve(bundle),
      targets: Platform.WINDOWS.createTarget('nsis', Arch[ARCHES[arch]]),
      config: { ...installerConfig({ arch }), directories: { output: scratch } },
      publish: 'never',
    })
    mkdirSync(out, { recursive: true })
    const written = join(out, assetName(arch))
    copyFileSync(join(scratch, assetName(arch)), written)
    return written
  } finally {
    if (filter === undefined) delete process.env.ELECTRON_BUILDER_7Z_FILTER
    else process.env.ELECTRON_BUILDER_7Z_FILTER = filter
    rmSync(scratch, { recursive: true, force: true })
  }
}

async function main() {
  const argv = process.argv.slice(2)
  const value = (name) => argv.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3)
  for (const name of ['bundle', 'arch', 'out']) {
    if (value(name) === undefined) {
      console.error(`usage: node scripts/windows-installer.mjs --bundle=<dir> --arch=<${Object.keys(ARCHES).join('|')}> --out=<dir>`)
      process.exit(1)
    }
  }
  try {
    console.log(`wrote: ${await buildInstaller({ bundle: value('bundle'), arch: value('arch'), out: value('out') })}`)
  } catch (error) {
    console.error(error.message)
    process.exit(1)
  }
}

// Run when this file is the program, and not when a test imports it. Through the real path both
// ways, for the reason `scripts/package.mjs` says.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  await main()
}
