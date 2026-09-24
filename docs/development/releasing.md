# Releasing

Reproducible cross-platform binaries are built in a pinned container, so the same artifact
comes out on any host:

```sh
make all-platforms
```

## Publishing a version

Two steps to name the version, then two manual publishes: Jenkins for signed binaries, then
GitHub Actions for the npm package. Either can wait days after the tag.

```sh
make bump-version BUMP=bugfix   # or minor, major
# review the commit it made, land it on main
make github-release
# later: Jenkins job bravebot-build with UPLOAD and RELEASE
# later: make publish-npm TAG=v<version>
```

`bump-version` rewrites the version in `Cargo.toml`, `Cargo.lock`, `package.json`,
`package-lock.json`, `ui/package.json`, and `ui/package-lock.json`, commits exactly those six as
`Bump version to <version>`, and stops there: nothing is pushed and nothing is tagged. It refuses
if any of the six is already modified, rather than committing changes it did not write, and it
runs `make check-versions` before committing, so a bump that misses one fails there rather than
at the tag. `github-release` refuses to tag unless the tree is clean, every file that states the
version agrees, and HEAD is `main` at the release remote's `main`, then pushes `v<version>` there.

The desktop application is packaged from `ui/package.json`, so its version is the one the app
bundle carries and the one the About window shows beside the agent build. It is the same release:
the app ships an agent build rather than versioning separately. `make check-versions` is what
holds the six together, and CI runs it, so a manifest left behind fails the pull request that
left it rather than the release weeks later.

**The release remote.** `origin` by default, which is right for a clone of this repository and
wrong for a clone of a fork of it, where `origin` names the fork. A tag pushed to a fork is one
the releases page never sees, and the tag ruleset here refuses an update or a deletion, so the
mistake cannot be corrected in place: the version has to be bumped again. A clone whose `origin`
is a fork names the remote that is not, once:

```sh
git config bravebot.releaseRemote upstream
```

`github-release` fails before tagging if no remote by that name exists.

The tag push does not publish binaries. GitHub Actions still builds and tests on the tag.
Signed, configured assets are built, notarised, and uploaded by the Jenkins job
`bravebot-build` with `UPLOAD` and `RELEASE` enabled. That job builds the tip of the branch it
is given. A RELEASE run refuses unless tag `v<version>` from `Cargo.toml` already exists and
points at that commit, then creates a GitHub release of that name and attaches the signed
binaries plus the `.sha256` files written after signing. `gh release create` fails if that
release already exists.

After Jenkins has created that GitHub release, `make publish-npm TAG=v<version>` dispatches
`publish-npm.yml` from that tag to publish `@brave/bravebot`. Without `TAG` it takes the latest
GitHub release rather than the version in the tree, which may already be bumped past it. The
workflow, not the target, refuses a tag that does not match its tree or a release missing an
asset. OIDC trusted publishing; there is no `NPM_TOKEN`. The trusted publisher on npmjs.com must
name that workflow file. The npm `postinstall` verifies the binary against its published
`.sha256` before writing it.

`BRAVEBOT_ALLOW_UNCONFIGURED_BUILD` is set in GitHub Actions so forks and PRs compile without
secrets. Jenkins does not set it on an upload, so a missing credential fails the release
instead of shipping a binary that cannot reach the backend.

Darwin binaries are codesigned and notarised, Windows binaries are Authenticode-signed.

## The desktop application

```sh
make darwin-arm64 darwin-amd64 strip
make app-release
```

The third family of artifact, and the one the tag does not produce. macOS gets a disk image per
architecture, Linux a `.deb` and an `.rpm` per architecture from
[the Linux packages](#the-linux-packages) below, and Windows an installer per architecture from
[the Windows installers](#the-windows-installers).

`app-release` runs on a Mac of either architecture and needs no Rust toolchain. It packages the
app once per architecture from what the cross-build left in `dist/`, and writes a disk image for
each:

| Reads | Writes |
| --- | --- |
| `dist/bravebot-rpc-darwin-arm64`, `dist/bravebot-ui-files-darwin-arm64` | `dist/bravebot-app-darwin-arm64.dmg` |
| `dist/bravebot-rpc-darwin-amd64`, `dist/bravebot-ui-files-darwin-amd64` | `dist/bravebot-app-darwin-amd64.dmg` |

Those two executables are the agent the app talks to and its secure file helper. A Mac
cross-build writes them beside the CLI and `make strip` strips them with it, so the agent inside
the app is the build the CLI asset on the same release page is. They go into the images and are not
assets of their own, so `make checksums` leaves them out. Each image holds
`Brave Bot.app`, a link to `/Applications`, and the Electron and Chromium licence files.
`ui/package.json` states the app's version, which is the version above, so the app names the
release it ships an agent build of. Its bundle id is `com.brave.bravebot`: macOS keys privacy
grants and keychain items on it, so it stays the same across releases.

The app is fused and not signed. The fuses turn off `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS` and
`--inspect`, each a way to make the signed binary run code that is not the app's, and turn on
asar integrity validation and loading only from the asar. Fusing rewrites the Electron binary,
which a signature covers, so it comes first, and `app-release` is two steps with the signing
between them:

```sh
make app-bundles   # ui/dist/Brave Bot-darwin-arm64/ and ui/dist/Brave Bot-darwin-x64/
# sign and notarise each Brave Bot.app where it lies
make app-dmg       # dist/bravebot-app-darwin-<arch>.dmg, then sign and notarise those
```

The two executables in `Contents/Resources/` have to be signed on their own before the app is:
`codesign --deep` does not look there, and notarisation rejects an unsigned one.

Nothing in this repository runs any of it: no CI job packages the app and the tag does not
either. Like `make strip` and `make checksums`, the targets state the format and the release job
consumes it.

`make app-bundle` is the same app for the machine it runs on, from this checkout's own release
build rather than from `dist/`, left in `ui/dist/` with no image. It refuses a build with no
backend credentials in it, which the front end's own `npm run bridge` allows on purpose: a bundle
built from an unconfigured shell starts, lists sessions, opens them, and fails at the first
inference request, and Finder loads no shell configuration for the person who would then report
that. So the credentials have to be in the environment `make` runs in, as they are for the
cross-builds above.

### The Linux packages

```sh
make linux-amd64 linux-arm64 strip
make app-release-linux
```

On a Linux host, and needing no Rust toolchain. It packages the app once per architecture from
what the cross-build left in `dist/`, and writes two packages for each:

| Reads | Writes |
| --- | --- |
| `dist/bravebot-rpc-linux-arm64`, `dist/bravebot-ui-files-linux-arm64` | `dist/bravebot-app-linux-arm64.deb`, `dist/bravebot-app-linux-arm64.rpm` |
| `dist/bravebot-rpc-linux-amd64`, `dist/bravebot-ui-files-linux-amd64` | `dist/bravebot-app-linux-amd64.deb`, `dist/bravebot-app-linux-amd64.rpm` |

Both architectures are packaged on one host. `@electron/packager` downloads the Electron runtime
for the architecture it is asked for, and `dpkg-deb` and `rpmbuild` archive a finished tree rather
than executing anything in it, so neither step needs the machine the package is for. Each tool
runs in a container pinned by digest, as the cross-build is: `make check-security` fails on one
named by a tag.

**Why a package and not a tarball.** Chromium's sandbox. Where unprivileged user namespaces are
unavailable, which Ubuntu 24.04's AppArmor policy makes the default, Electron falls back to its
setuid helper and aborts at start unless `chrome-sandbox` is owned by root with mode 4755. An
installer can set that and something a person unpacks themselves cannot, which would leave
`--no-sandbox` as the way to start it. That is not a supported way to run an agent.

What the packages install:

| Path | |
| --- | --- |
| `/opt/brave-bot/` | the bundle, with `chrome-sandbox` setuid root |
| `/usr/bin/brave-bot` | a symlink to the executable inside it, whose name has a space |
| `/usr/share/applications/brave-bot.desktop` | the launcher entry, running that symlink |
| `/usr/share/icons/hicolor/scalable/apps/brave-bot.svg` | `build/icon.svg`, the app's icon |

`ui/scripts/linux-package.mjs` writes that tree and both package descriptions, so the layout is
the same whichever format was installed, and `make app-packages-linux` is only the packing. The
runtime dependencies are declared there, in each distribution's names: Ubuntu 24.04's 64-bit time
transition renamed several of them, so those are named as alternatives and a package built for one
release still installs on the other.

`brave-bot` is the package name in both formats. Upgrades and conflicts key on it, so like the
macOS bundle id it is fixed by the first release. It is not the CLI's `bravebot`, which an apt or
dnf repository would want as a package of its own.

The packages are fused and unsigned, as the Mac bundles are. Signing them, and an apt or dnf
repository to serve updates from, are issue #770; the app has no updater on any platform.

**What is not checked here.** Installing one. That needs a virtual machine of each supported
release, because a container shares the host's kernel and AppArmor policy and so cannot exercise
the sandbox the packages exist for. On a machine of each, the package has to install, start from
its launcher with the sandbox on, start its agent, ignore `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`
and `--inspect`, and uninstall cleanly.

### The Windows installers

```sh
make windows-amd64 windows-arm64 strip
make app-release-windows
```

It packages the app once per architecture from what the cross-build left in `dist/`, and writes an
installer for each:

| Reads | Writes |
| --- | --- |
| `dist/bravebot-rpc-windows-arm64.exe`, `dist/bravebot-ui-files-windows-arm64.exe` | `dist/bravebot-desktop-windows-arm64-setup.exe` |
| `dist/bravebot-rpc-windows-amd64.exe`, `dist/bravebot-ui-files-windows-amd64.exe` | `dist/bravebot-desktop-windows-amd64-setup.exe` |

Two steps with the signing between them, as on the Mac, and each runs on different hosts:

```sh
make app-bundles-windows      # ui/dist/Brave Bot-win32-arm64/ and ui/dist/Brave Bot-win32-x64/, on any host
# sign every PE in each bundle where it lies
make app-installers-windows   # the two installers, on Windows or a Mac; then sign those
```

The bundles build on any host, because everything platform-specific about a Windows bundle is the
icon and the version resource in `Brave Bot.exe`, and `@electron/packager` writes both with
resedit, a JavaScript library. The Windows cross-build keeps the same pair of executables beside
the CLI as the Mac one does, `make strip` strips them with the rest, and `make checksums` leaves
them out of the assets. The bundle is fused, in `Brave Bot.exe` rather than in a framework, and
unsigned. The job signs every PE in it where it lies: `Brave Bot.exe`, both helpers in
`resources/`, and Electron's own DLLs.

The installers do not build on Linux. electron-builder writes each one's uninstaller by running a
stub build of the installer, which runs natively on Windows and which it reads in JavaScript on
macOS, and on Linux it needs Wine. So the release builds the installers on the Windows node that
signed the bundles, and signs them there too.

**What the installer does.** It is NSIS, written by electron-builder from the bundle as
`prepackaged`, so electron-builder packs and edits nothing and the files the job signed are the
files installed. It is one click and per user, with no administrator prompt, as
[#769](https://github.com/brave/bravebot/issues/769) settled:

| | |
| --- | --- |
| Install directory | `%LOCALAPPDATA%\Programs\bravebot-desktop` |
| Uninstall key | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\bravebot-desktop` |
| Start menu entry, Apps list | `Brave Bot Desktop` |
| Silent install | `/S`, which Intune and SCCM can run; Group Policy software installation takes only MSI |

A newer installer finds the install it replaces through the uninstall key and runs that install's
uninstaller first. So the key is fixed by the first release, like the macOS bundle id and the Linux
package name, and so are the directory and the per-user scope: an installer that changes any of
them installs a second copy beside the first. Uninstalling leaves the app's data and `~/.bravebot`
where they are. `ui/scripts/windows-installer.mjs` states all of it, and refuses a bundle of the
other architecture, or one that is not fused, which is what a debug bundle packaged on a Windows
checkout looks like under the same directory name.

The Start menu entry carries `com.brave.bravebot` as its AppUserModelID. Windows groups a window
under the entry with the ID the process sets, and the app sets none yet, which is part of
[#767](https://github.com/brave/bravebot/issues/767).

Each installer carries an uninstaller that electron-builder writes during the build, so the job
cannot sign it afterwards the way it signs the installer. electron-builder signs it in the same
build when given a certificate there, and setting that up is
[#770](https://github.com/brave/bravebot/issues/770).

**What is not checked here.** Installing one. On Windows of each architecture, the installer has to
install without an administrator prompt, start the app from its Start menu entry, start its agent,
upgrade over the previous release, and uninstall cleanly. GitHub's hosted `windows-latest` and
`windows-11-arm` runners can do that as a CI job once the app starts on Windows, which is #767.
