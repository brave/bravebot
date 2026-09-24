# Development and packaging

Start with [setup](setup.md). Run the `npm` commands from `ui/`; this is not an npm
workspace of the package at the repository root, so nothing there reaches these scripts.
Run the `cargo` commands from the root, where the workspace the two front-end crates are
members of lives.

| Command | What it does |
| --- | --- |
| `npm ci` | Install locked npm dependencies and set up Electron |
| `npm run dev` | Set up Electron, build both Rust executables, start hot reload |
| `npm run bridge` | Build `bravebot-rpc` and `bravebot-ui-files`, loading credentials where configured |
| `npm run setup:electron` | Install the Electron runtime if needed and name the development app |
| `npm run name-dev-app` | Restore “Brave Bot” as the development app's menu-bar name |
| `npm run typecheck` | Run `tsc --noEmit` |
| `npm run build` | Build both Rust executables, typecheck, bundle into `out/` |
| `npm start` | Set up Electron and preview the existing bundle; does not rebuild it |
| `npm run package` | Build both Rust executables, bundle, package for macOS or Linux; does not typecheck |
| `make app-bundle` (from the root) | The same bundle, carrying release executables built with credentials required, fused |
| `make app-release` (from the root) | A disk image per Mac architecture, from the cross-built executables in `dist/`; see [releasing](../../docs/development/releasing.md#the-desktop-application) |
| `make app-bundles-windows` (from the root) | A Windows bundle per architecture, from the cross-built executables in `dist/`, on any host; see [releasing](../../docs/development/releasing.md#the-windows-bundle) |
| `make check-ui` (from the root) | Install, build the file helper, and run every `scripts/*.test.mjs` |
| `cargo test -p bravebot-ui-bridge -p bravebot-ui-files` | Test the two front-end crates |
| `cargo test --all` | Test the whole workspace, agent crates included |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint the whole workspace |
| `npm run drive` / `npm run drive:<name>` | Run a named Electron driver; see [testing](testing.md) |
| `npm run demo -- --record` | Record a walkthrough; see [demo costs and setup](demo.md) |

TypeScript uses strict checking, including `noUncheckedIndexedAccess`,
`noUnusedLocals` and `noUnusedParameters`. There is no ESLint or Prettier gate.
See [testing](testing.md#what-ci-runs) for exactly what CI checks and what must run locally.

## Build and preview

```bash
npm run build
npm start
```

With a desktop display available, `npm run drive:smoke` checks that the window and
main UI are visible and the Rust bridge responds, then saves a screenshot under
`/tmp/bravebot-ui/`. To check a Linux package, run
`npm run drive:smoke -- "./dist/Brave Bot-linux-x64/Brave Bot"`.

`scripts/build-bridge.sh` builds the Rust bridge and secure-file helper. TypeScript
then checks the code and electron-vite bundles the main process, preload and React
renderer into `out/`. Development executables are in the workspace `../target/debug/`.

The bridge talks to the pinned agent library over a child-process protocol; it does
not drive a terminal. See the [protocol design](phase-0-rpc-protocol.md) and
[security boundaries](security.md).

## Packaging

```bash
npm run typecheck
npm run package
```

`npm run package` does not typecheck. It builds the Rust executables and Electron
bundles, then `scripts/package.mjs` uses `@electron/packager` to create
`dist/Brave Bot-darwin-<arch>/Brave Bot.app` on macOS or
`dist/Brave Bot-linux-<arch>/` on Linux. Launch the Linux package with
`"./dist/Brave Bot-linux-x64/Brave Bot"` (replace `x64` for other architectures).
The platform follows the Node process, and so does the architecture unless
`node scripts/package.mjs --arch=arm64` or `--arch=x64` names one, in Electron's names rather than
the cross-build's `amd64`. Rust uses its configured toolchain target; for a native bundle, use
matching Node and Rust architectures.

`--platform=win32` packages `dist/Brave Bot-win32-<arch>/` instead, and is the one platform that
does not have to be the host's: the icon and the version resource are all that distinguishes a
Windows bundle, and `@electron/packager` writes both with resedit rather than with a Windows tool.
Because the bundle is somebody else's platform, it takes `--executables=<dir>` with a pair built
for Windows in it; `make app-bundles-windows` is that command with the cross-build's pair staged
for it. Packaging reads each executable's header, as it does for a Mac release, and refuses one
that is not a Windows executable for the architecture asked for.

Which Rust build the bundle carries is the one thing the finished bundle does not record: it is
named, versioned and laid out identically either way, and the two overwrite each other in
`dist/`. `npm run package` carries the debug executables, which is what a checkout has already
built and what makes it the right command for testing the bundle itself. The last line it prints
says which it took:

```
packaged: dist/Brave Bot-linux-x64 (debug executables, not fused)
```

The bundle's version is `package.json`'s, and that is the repository's version rather than one
of the front end's own: the app ships an agent build, so the two are one release.
`make bump-version` at the root rewrites this manifest and its lockfile along with the workspace's,
and `make check-versions` fails a tree where they disagree. Neither is edited by hand.

Both `bravebot-rpc` and `bravebot-ui-files` are copied into the app's Resources.
Packaged builds use those copies; development builds use the workspace `../target/debug/`.

A bundle for anybody else is `make app-bundle`, from the repository root: it builds both
executables in release mode with the backend credentials required rather than optional, and
packages those instead. `make app-release` packages the release's own cross-built pair for each
Mac architecture instead, through `--executables=<dir>`, and packaging reads the header of each
executable in that pair and refuses one built for another platform or architecture than the
bundle's. Neither command signs or notarises the
result, so what comes out is installable and not distributable, and how a release gets the rest
is [releasing](../../docs/development/releasing.md#the-desktop-application). Git commit
signatures are separate from macOS app signing.

Those two are fused and `npm run package` is not. `scripts/fuses.mjs` turns off the Electron
fuses that let the binary run code that is not the app's: `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`
and the `--inspect` arguments. It turns on asar integrity validation and loading only from the
asar. A fused app cannot be driven, because Playwright attaches through `--inspect`, so
`npm run drive:packaged` and `SECURE_FILES_APP` take the unfused bundle `npm run package` writes.
All three write to the same `dist/` path, and `drive:packaged` refuses a fused bundle there rather
than wait out its launch timeout.

Every macOS bundle carries the bundle id `com.brave.bravebot` and the icon `build/icon.icns`. macOS
keys privacy grants and keychain items on the bundle id, so it does not change between releases.
A Windows bundle carries `build/icon.ico` and a version resource naming Brave as the company and
"Brave Bot" as the product, which is what its properties dialog and Task Manager show; without one
they both say Electron, because the resource would be the one Electron's own build left behind.
The icon is the About mascot in Brave orange on a macOS tile, drawn in `build/icon.svg`; after
changing the drawing, remake both icons from it:

```bash
inkscape build/icon.svg -w 1024 -h 1024 -o /tmp/icon-1024.png
mkdir /tmp/icon.iconset
for s in 16 32 128 256 512; do
  sips -z $s $s /tmp/icon-1024.png --out /tmp/icon.iconset/icon_${s}x${s}.png
  sips -z $((s*2)) $((s*2)) /tmp/icon-1024.png --out /tmp/icon.iconset/icon_${s}x${s}@2x.png
done
iconutil -c icns /tmp/icon.iconset -o build/icon.icns
python3 -c "from PIL import Image; Image.open('/tmp/icon-1024.png').save('build/icon.ico', \
  sizes=[(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)])"
```

The `.ico` holds every size Windows asks for, from the file list at 16 to the extra-large view at
256. A file holding only the largest is legal, and Windows scales it down itself, but a mascot with
two eyes in it does not survive that to 16 pixels.

A development checkout can produce a configured or unconfigured binary depending
on the build environment. Follow [credentials](setup.md#credentials) before packaging
for inference; Finder does not load shell configuration. Packaging itself does not
check that backend credentials exist.

To check the bundle, use the [packaged-app drivers](testing.md#current-regression-checks).
AppKit takes the menu-bar name from the bundle's `CFBundleName`; changing
`app.name` would also change Electron's user-data location.
