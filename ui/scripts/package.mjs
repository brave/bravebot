// Build a native macOS app or Linux application directory.
//
// Everything the app needs at run time is already in `out/` — the main process, the preload
// and the renderer, all bundled — so packaging is mostly a matter of saying what to leave
// behind. What has to be *added* is the agent: `Bridge.binaryPath()` looks for
// `bravebot-rpc` in `process.resourcesPath` when `app.isPackaged`, and falls back to the
// `cargo` output only in development. So the binary is copied in as a resource, and the
// packaged app is the only build where that path is ever taken.
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
  const executables = argv.find((arg) => arg.startsWith('--executables='))?.slice('--executables='.length)
  if (executables !== undefined) {
    return {
      name: 'prebuilt',
      agent: `${executables}/bravebot-rpc`,
      files: `${executables}/bravebot-ui-files`,
      build: 'make app-release',
      fused: true,
    }
  }
  const release = argv.includes('--release')
  const dir = release ? '../target/release' : '../target/debug'
  return {
    name: release ? 'release' : 'debug',
    agent: `${dir}/bravebot-rpc`,
    files: `${dir}/bravebot-ui-files`,
    // What to run when one of the two is missing. `npm run bridge` builds the debug pair and
    // only that pair, so naming it for a release profile sends whoever hit this around the
    // same loop again with the same result.
    build: release ? 'make app-bundle' : 'npm run bridge',
    fused: release,
  }
}

// The platform and architecture an executable's header says it was built for, or null for a file
// that is not a 64-bit Mach-O or ELF executable. The names are Electron's, which are the ones the
// bundle is packaged under.
export function executableTarget(header) {
  if (header.length >= 8 && header.readUInt32LE(0) === 0xfeedfacf) {
    const cpu = header.readUInt32LE(4)
    return { platform: 'darwin', arch: cpu === 0x0100000c ? 'arm64' : cpu === 0x01000007 ? 'x64' : `cputype ${cpu.toString(16)}` }
  }
  if (header.length >= 20 && header.readUInt32BE(0) === 0x7f454c46 && header[4] === 2 && header[5] === 1) {
    const machine = header.readUInt16LE(18)
    return { platform: 'linux', arch: machine === 0xb7 ? 'arm64' : machine === 0x3e ? 'x64' : `machine ${machine}` }
  }
  return null
}

function readHeader(path) {
  const fd = openSync(path, 'r')
  try {
    const header = Buffer.alloc(20)
    return header.subarray(0, readSync(fd, header, 0, header.length, 0))
  } finally {
    closeSync(fd)
  }
}

// Setting a fuse rewrites the binary Electron reads them from, which invalidates the ad-hoc
// signature Electron ships it with, and an arm64 Mac kills a process whose pages no longer match
// their signature. The packager has just done the same for the asar digest and restores the
// signature the same way, so the bundle is left as launchable as it found it.
function fuse(bundle) {
  const framework = join(bundle, 'Brave Bot.app', 'Contents', 'Frameworks', 'Electron Framework.framework')
  const binary = process.platform === 'darwin' ? join(framework, 'Electron Framework') : join(bundle, 'Brave Bot')
  const bytes = readFileSync(binary)
  setFuses(bytes, RELEASE_FUSES)
  writeFileSync(binary, bytes)
  if (process.platform !== 'darwin') return
  const signed = spawnSync('codesign', [
    '--sign', '-', '--force', '--deep', '--preserve-metadata=entitlements,requirements,flags,runtime', framework,
  ], { stdio: 'inherit' })
  if (signed.status !== 0) throw new Error(`codesign could not restore the signature on ${framework}`)
}

async function main() {
  if (!['darwin', 'linux'].includes(process.platform)) {
    throw new Error(`Packaging is not supported on ${process.platform}`)
  }

  const { version } = JSON.parse(readFileSync('package.json', 'utf8'))
  const argv = process.argv.slice(2)
  const profile = buildProfile(argv)
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
    if (target?.platform !== process.platform || target.arch !== arch) {
      const is = target ? `a ${target.platform} ${target.arch} executable` : 'not a 64-bit Mach-O or ELF executable'
      console.error(`${binary} is ${is}, and this is a ${process.platform} ${arch} bundle`)
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
    // The packager also looks for an Icon Composer `.icon` beside this and warns that there is
    // none. Without one macOS shows the `.icns`, so the warning is expected.
    icon: 'build/icon.icns',
    platform: process.platform,
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
    if (profile.fused) fuse(path)
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
