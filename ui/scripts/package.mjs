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
// meant to run is built by `make app-bundle` at the repository root, which refuses a build with
// no credentials in it rather than producing one.
import { packager } from '@electron/packager'
import { existsSync, readFileSync, realpathSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

// Which Rust build profile the bundle carries, and where the two executables it copies in come
// from. The default is the debug build, because that is what a development checkout has already
// built and what every `npm run` path here produces. A bundle for anyone else carries the release
// build, and nothing in a finished bundle says which one it got, so this is asked for rather than
// guessed at: `--release`, as `make app-bundle` passes it.
export function buildProfile(argv) {
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
  }
}

async function main() {
  if (!['darwin', 'linux'].includes(process.platform)) {
    throw new Error(`Packaging is not supported on ${process.platform}`)
  }

  const { version } = JSON.parse(readFileSync('package.json', 'utf8'))
  const profile = buildProfile(process.argv.slice(2))

  for (const binary of [profile.files, profile.agent]) {
    if (existsSync(binary)) continue
    console.error(`no ${profile.name} binary at ${binary}: run \`${profile.build}\` first`)
    process.exit(1)
  }
  if (!existsSync('out/main/index.js')) {
    console.error('no bundle in out/: run `electron-vite build` first')
    process.exit(1)
  }

  const paths = await packager({
    dir: '.',
    out: 'dist',
    name: 'Brave Bot',
    appBundleId: 'dev.bravebot.ui',
    appVersion: version,
    platform: process.platform,
    arch: process.arch,
    overwrite: true,
    prune: true,
    asar: true,
    extraResource: [profile.agent, profile.files],
    // What not to carry. `target` is the Rust build directory and is gigabytes of object
    // files; `src` and `crates` are sources whose output is already in `out/`. Leaving any of
    // them in would ship the whole workshop with the furniture.
    ignore: [
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
  for (const path of paths) console.log(`packaged: ${path} (${profile.name} executables)`)
}

// Run when this file is the program, and not when a test imports it for the profile above.
// Through the real path both ways: Node resolves a module's own URL through any symlink on the
// way to it and leaves `argv[1]` as it was typed, and a checkout reached through a symlink is a
// case this front end already has to handle. Comparing the two as given would leave packaging
// exiting 0 with no bundle written.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  await main()
}
