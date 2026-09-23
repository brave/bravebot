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
# later: Actions → Publish npm, with tag v<version>
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

After Jenkins has created that GitHub release, dispatch `publish-npm.yml` with the same tag to
publish `@brave/bravebot`. OIDC trusted publishing; there is no `NPM_TOKEN`. The trusted
publisher on npmjs.com must name that workflow file. Run the workflow from the version tag
(Use workflow from / `--ref`). The npm `postinstall` verifies the binary against its published
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

The third family of artifact, and the one the tag does not produce. It is released for macOS
only. A Linux desktop release needs a package format and a machine to test it on, and has neither.

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
