// Build a macOS app bundle, or a Linux or Windows application directory.
//
// Everything the app needs at run time is already in `out/` — the main process, the preload
// and the renderer, all bundled — so packaging is mostly a matter of saying what to leave
// behind. What has to be *added* is the agent: `Bridge.binaryPath()` looks for
// `bravebot-rpc` in `process.resourcesPath` when `app.isPackaged`, and falls back to the
// `cargo` output only in development. So the binary is copied in as a resource, and the
// packaged app is the only build where that path is ever taken. It is copied in under the name
// it has, which on Windows ends in `.exe`; teaching that lookup to find it is issue #767.
//
// The bundle is named here, which is also the only way the menu bar gets the right word in
// a release: AppKit reads it from `CFBundleName` before any of our code runs.
// `scripts/name-dev-app.mjs` does the same job for `npm run dev` by renaming Electron's own
// bundle; this does it properly, by building one of our own.
//
// Credentials: a bundle built from a development checkout carries a `bravebot-rpc` that reads
// its credentials from the environment, which a double-clicked `.app` does not have. It will
// start, list sessions and open them, and fail at the first inference request. That is the
// documented degraded mode, not a packaging fault, and it is why the bundle somebody else is
// meant to run is built at the repository root: by `make app-bundle`, which refuses a build with
// no credentials in it rather than producing one, or by `make app-release`, from the executables
// the release's own cross-build made.
import { packager } from '@electron/packager'
import { spawnSync } from 'node:child_process'
import { closeSync, existsSync, openSync, readFileSync, readSync, realpathSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { RELEASE_FUSES, setFuses } from './fuses.mjs'

// Which Rust build profile the bundle carries, and where the two executables it copies in come
// from. The default is the debug build, because that is what a development checkout has already
// built and what every `npm run` path here produces. A bundle for anyone else carries the release
// build, and nothing in a finished bundle says which one it got, so this is asked for rather than
// guessed at: `--release`, as `make app-bundle` passes it, or `--executables=<dir>`, a directory
// holding the pair under the names they ship as, which is how `make app-release` hands over the
// cross-built ones for an architecture this machine is not.
//
// Both of those are fused, and the debug bundle is not: a fused app ignores `--inspect`, which
// is how Playwright attaches, so fusing it would leave `drive-packaged.mjs` and the packaged
// secure-files run nothing to drive.
export function buildProfile(argv) {
  // Windows names an executable with a suffix and the other two do not, so the pair the cross
  // build left in `dist/` is `bravebot-rpc.exe` there and `bravebot-rpc` everywhere else. The
  // bundle's platform decides it rather than the host's, because a Windows bundle is the one
  // that gets built somewhere else.
  const platform = bundlePlatform(argv)
  const exe = platform === 'win32' ? '.exe' : ''
  const executables = argv.find((arg) => arg.startsWith('--executables='))?.slice('--executables='.length)
  if (executables !== undefined) {
    return {
      name: 'prebuilt',
      agent: `${executables}/bravebot-rpc${exe}`,
      files: `${executables}/bravebot-ui-files${exe}`,
      // The target that hands the pair over, which is a different one per platform. Naming
      // another platform's sends whoever hit this to a target that will not produce the
      // executables they are missing.
      build: PREBUILT_TARGET[platform],
      fused: true,
    }
  }
  const release = argv.includes('--release')
  const dir = release ? '../target/release' : '../target/debug'
  return {
    name: release ? 'release' : 'debug',
    agent: `${dir}/bravebot-rpc${exe}`,
    files: `${dir}/bravebot-ui-files${exe}`,
    // What to run when one of the two is missing. `npm run bridge` builds the debug pair and
    // only that pair, so naming it for a release profile sends whoever hit this around the
    // same loop again with the same result.
    build: release ? 'make app-bundle' : 'npm run bridge',
    fused: release,
  }
}

// What builds the prebuilt pair for each platform the app is packaged for.
const PREBUILT_TARGET = {
  darwin: 'make app-release',
  linux: 'make app-bundles-linux',
  win32: 'make app-bundles-windows',
}

// Which platform the bundle is for, in Electron's names: the host's unless `--platform=` says
// otherwise. Only a Windows bundle is ever built anywhere else, and `main` is where that is
// enforced; this is the one place that reads the argument, because the executables' names depend
// on it too.
function bundlePlatform(argv) {
  return argv.find((arg) => arg.startsWith('--platform='))?.slice('--platform='.length) ?? process.platform
}

// The platform and architecture an executable's header says it was built for, or null for a file
// that is not a 64-bit Mach-O, ELF or PE executable. The names are Electron's, which are the ones
// the bundle is packaged under.
export function executableTarget(header) {
  if (header.length >= 8 && header.readUInt32LE(0) === 0xfeedfacf) {
    const cpu = header.readUInt32LE(4)
    return { platform: 'darwin', arch: cpu === 0x0100000c ? 'arm64' : cpu === 0x01000007 ? 'x64' : `cputype ${cpu.toString(16)}` }
  }
  if (header.length >= 20 && header.readUInt32BE(0) === 0x7f454c46 && header[4] === 2 && header[5] === 1) {
    const machine = header.readUInt16LE(18)
    return { platform: 'linux', arch: machine === 0xb7 ? 'arm64' : machine === 0x3e ? 'x64' : `machine ${machine}` }
  }
  // A PE says nothing at its own start: the first two bytes are still the `MZ` of the DOS stub it
  // has carried since 1985, and the only fixed field is the offset at 0x3c of the real header.
  // So the machine word is reached through that offset, and a file too short to hold either is
  // not a PE rather than a PE whose architecture is unknown.
  if (header.length >= 0x40 && header.readUInt16LE(0) === 0x5a4d) {
    const at = header.readUInt32LE(0x3c)
    if (at + 6 > header.length || header.readUInt32LE(at) !== 0x00004550) return null
    const machine = header.readUInt16LE(at + 4)
    return { platform: 'win32', arch: machine === 0x8664 ? 'x64' : machine === 0xaa64 ? 'arm64' : `machine ${machine}` }
  }
  return null
}

// Enough of the file to reach a PE header through the offset at 0x3c, which the linkers in play
// put a few hundred bytes in. Mach-O and ELF both say which architecture they are in their first
// twenty bytes, so this was that long before Windows.
function readHeader(path) {
  const fd = openSync(path, 'r')
  try {
    const header = Buffer.alloc(4096)
    return header.subarray(0, readSync(fd, header, 0, header.length, 0))
  } finally {
    closeSync(fd)
  }
}

// Which file in a packaged bundle carries the fuse wire, by the bundle's platform rather than the
// host's, since a Windows bundle is packaged elsewhere. macOS keeps it in the framework the app
// links against, and the other two in the executable itself, under the name that platform gives it.
export function fusedBinary(bundle, platform) {
  if (platform === 'darwin') {
    return join(bundle, 'Brave Bot.app', 'Contents', 'Frameworks', 'Electron Framework.framework', 'Electron Framework')
  }
  return join(bundle, platform === 'win32' ? 'Brave Bot.exe' : 'Brave Bot')
}

// Setting a fuse rewrites the binary Electron reads them from, which invalidates the ad-hoc
// signature Electron ships it with, and an arm64 Mac kills a process whose pages no longer match
// their signature. The packager has just done the same for the asar digest and restores the
// signature the same way, so the bundle is left as launchable as it found it.
function fuse(bundle, platform) {
  const framework = join(bundle, 'Brave Bot.app', 'Contents', 'Frameworks', 'Electron Framework.framework')
  const binary = fusedBinary(bundle, platform)
  const bytes = readFileSync(binary)
  setFuses(bytes, RELEASE_FUSES)
  writeFileSync(binary, bytes)
  if (platform !== 'darwin') return
  const signed = spawnSync('codesign', [
    '--sign', '-', '--force', '--deep', '--preserve-metadata=entitlements,requirements,flags,runtime', framework,
  ], { stdio: 'inherit' })
  if (signed.status !== 0) throw new Error(`codesign could not restore the signature on ${framework}`)
}

async function main() {
  const { version } = JSON.parse(readFileSync('package.json', 'utf8'))
  const argv = process.argv.slice(2)
  const platform = bundlePlatform(argv)
  if (!['darwin', 'linux', 'win32'].includes(platform)) {
    console.error(`--platform is darwin, linux or win32, Electron's names for them, not ${platform}`)
    process.exit(1)
  }
  // A Windows bundle is the one that does not need its own platform to build: everything
  // platform-specific about it is the icon and the version resource written into `Brave Bot.exe`,
  // and `@electron/packager` writes both with resedit, which is JavaScript. So the release job
  // builds it on the host it already has rather than on the Windows node it signs on, and Wine is
  // never involved. The other two do need their own: the fuse step restores a macOS signature
  // with `codesign`, and nothing here has ever built a Linux bundle anywhere else.
  if (platform !== process.platform && platform !== 'win32') {
    console.error(`a ${platform} bundle is built on ${platform}, and this is ${process.platform}`)
    process.exit(1)
  }
  const profile = buildProfile(argv)
  // A cross-packaged bundle carries executables built for it somewhere else, and this checkout's
  // own `target/` holds this host's. Without this the refusal below would be about a path under
  // `../target/` that nothing on this machine could ever put a Windows executable at, and would
  // name `npm run bridge` as the way to fix it.
  if (platform !== process.platform && profile.name !== 'prebuilt') {
    console.error(`a ${platform} bundle built on ${process.platform} takes --executables=<dir>: this checkout builds for ${process.platform}`)
    process.exit(1)
  }
  const arch = argv.find((arg) => arg.startsWith('--arch='))?.slice('--arch='.length) ?? process.arch
  if (!['arm64', 'x64'].includes(arch)) {
    console.error(`--arch is arm64 or x64, Electron's names for them, not ${arch}`)
    process.exit(1)
  }

  for (const binary of [profile.files, profile.agent]) {
    if (!existsSync(binary)) {
      console.error(`no ${profile.name} binary at ${binary}: run \`${profile.build}\` first`)
      process.exit(1)
    }
    // Only the pair a release hands over is checked. The cross-build says amd64 where Electron
    // says x64, and one slip puts one architecture's pair in the other's bundle, whose agent then
    // needs Rosetta on Apple Silicon and cannot run on an Intel Mac. The other two profiles carry
    // this checkout's own build, and ui/docs/testing.md describes an Apple Silicon checkout with
    // an Intel Rust toolchain.
    if (profile.name !== 'prebuilt') continue
    const target = executableTarget(readHeader(binary))
    if (target?.platform !== platform || target.arch !== arch) {
      const is = target ? `a ${target.platform} ${target.arch} executable` : 'not a 64-bit Mach-O, ELF or PE executable'
      console.error(`${binary} is ${is}, and this is a ${platform} ${arch} bundle`)
      process.exit(1)
    }
  }
  if (!existsSync('out/main/index.js')) {
    console.error('no bundle in out/: run `electron-vite build` first')
    process.exit(1)
  }

  const paths = await packager({
    dir: '.',
    out: 'dist',
    name: 'Brave Bot',
    // macOS keys a person's privacy grants and keychain items on this and the signing team, so
    // changing it after a release throws away every grant made to the one before.
    appBundleId: 'com.brave.bravebot',
    appVersion: version,
    // Without this, both the Windows version resource and the Mac bundle's `Info.plist` carry
    // Electron's own line, which names GitHub as the owner of a signed Brave product. No year,
    // because nothing here would move one on and a stale one is a worse claim than none.
    appCopyright: 'Copyright (c) Brave Software, Inc. All rights reserved.',
    // The packager also looks for an Icon Composer `.icon` beside the `.icns` and warns that there
    // is none. Without one macOS shows the `.icns`, so the warning is expected. Linux takes
    // neither: the packager writes no icon there, and a desktop entry is the installer's business.
    icon: platform === 'win32' ? 'build/icon.ico' : 'build/icon.icns',
    // What the properties dialog, Task Manager's description column and an installer reading the
    // file back all show. Without it every one of them says "Electron", because the resource is
    // the one Electron's own build left in the executable.
    win32metadata: {
      CompanyName: 'Brave Software, Inc.',
      FileDescription: 'Brave Bot',
      ProductName: 'Brave Bot',
      InternalName: 'Brave Bot',
      OriginalFilename: 'Brave Bot.exe',
    },
    platform,
    arch,
    overwrite: true,
    prune: true,
    asar: true,
    extraResource: [profile.agent, profile.files],
    // What not to carry. `target` is the Rust build directory and is gigabytes of object
    // files; `src` and `crates` are sources whose output is already in `out/`. Leaving any of
    // them in would ship the whole workshop with the furniture.
    ignore: [
      /^\/build($|\/)/,
      /^\/src($|\/)/,
      /^\/crates($|\/)/,
      /^\/vendor($|\/)/,
      /^\/target($|\/)/,
      /^\/docs($|\/)/,
      /^\/dist($|\/)/,
      /^\/scripts($|\/)/,
      /^\/\.git($|\/)/,
      /^\/\.cargo($|\/)/,
      /^\/Cargo\.(toml|lock)$/,
      /^\/tsconfig\.json$/,
      /^\/electron\.vite\.config\.ts$/,
    ],
  })

  // Which profile, because the bundle is named and versioned the same either way: a debug
  // bundle and a release one overwrite each other in `dist/` and look identical afterwards.
  for (const path of paths) {
    if (profile.fused) fuse(path, platform)
    console.log(`packaged: ${path} (${profile.name} executables, ${profile.fused ? 'fused' : 'not fused'})`)
  }
}

// Run when this file is the program, and not when a test imports it for the profile above.
// Through the real path both ways: Node resolves a module's own URL through any symlink on the
// way to it and leaves `argv[1]` as it was typed, and a checkout reached through a symlink is a
// case this front end already has to handle. Comparing the two as given would leave packaging
// exiting 0 with no bundle written.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  await main()
}
