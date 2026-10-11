---
id: RELEASE
title: Releases
status: normative
governs:
  - Makefile
  - .github/workflows/ci.yml
  - .github/workflows/publish-npm.yml
  - npm/scripts/postinstall.js
  - npm/tests/postinstall.test.mjs
  - install.sh
  - package.json
  - ui/package.json
  - contrib/check-versions.py
  - contrib/release-preflight.py
  - contrib/check-pnpm-lockfiles.py
documented-by: docs/website/docs/quickstart.md
---

## Scope

Turning a commit into binaries somebody else installs: what names a version, what starts a
release, what refuses one, and what an installer trusts about what it fetched.

Building for a platform is not this topic. Reproducible cross-builds are ordinary code, described
in [../development/releasing.md](../development/releasing.md). This file governs only the path from a version to a
published asset, and the checks along it.

Two installers fetch what is published: the npm package's install step, and the shell script
served from the trunk of this repository. Both are governed here. Learning from a running copy
that a newer release exists is [updates.md](updates.md).

GitHub Actions compiles and tests. It does not create a GitHub release. Signed, configured
binaries are built and published by the Jenkins job `bravebot-build` in the devops repository.
The npm package is published afterwards, from a manually dispatched GitHub Actions workflow,
once that release exists.

Most of this is not pinned by a Rust test. Those rules are enforced by a refusal in the tagging
path, the Jenkins publish path, or the npm publish workflow rather than by anything the test
suite can execute, so each of those clauses says in brackets what makes it hold. Two of them are
the exception. What the npm installer downloads from is pinned by a Node test that the npm
lockfile job runs and `cargo test` does not reach, and what the installers do with a Linux
signature is pinned by a Rust test that runs both of them against a release it made.

## Clauses

<a id="RELEASE-1"></a>
### RELEASE-1: one version names a release, and every file that states it agrees

The workspace manifest holds the version. Every other file that repeats it, the npm package
manifest and the desktop application's manifest in particular, states the same value, and a
disagreement stops a release rather than being resolved in favour of either. A disagreement is
reported by a check that runs on every pull request, rather than only by the refusal at the tag.

**Why.** The installer derives the tag it downloads from the version it was published with, so two
files disagreeing does not produce a mislabelled release, it produces a release whose assets the
installer looks for under a name that was never uploaded. The application is packaged from its own
manifest and ships an agent build, so a version left behind there is an app naming a release that
was never made. Reporting it at the tag is release day; reporting it in CI is the pull request
that caused it, and the front end sat two minor versions behind from the day it was folded in with
nothing anywhere saying so.

`verified-by: by-construction (bumping rewrites every file that states the version in one step and checks its own work before committing, make check-versions faults a disagreement on every pull request, and both tagging and Jenkins refuse a mismatch)`

<a id="RELEASE-2"></a>
### RELEASE-2: setting the next version commits every file that states it, and publishes nothing

Choosing a version rewrites every file that states it, commits exactly those files under a
message naming the version, and stops. It pushes nothing and tags nothing. Where any of those
files is already modified, it refuses rather than committing work it did not write. Before it
changes anything it warns, and asks whether to go on, about open `release-blocking` issues,
untranslated messages, and a peer advisory check that has not run in the last seven days. The
date of the last run is the newest one recorded in a comment on issue #1901 by a person with
write access, and the last commit to the vetted-advisories ledger where there is no such comment
or it cannot be read. The warning says which of the two it used.

**Why.** The files that state a version are only correct together, so a bump left uncommitted is
one a lockfile can be dropped from, which is the disagreement [RELEASE-1](#RELEASE-1) exists to
stop. Committing them is mechanical and has one right answer, so leaving it to be done by hand
adds a way to get it wrong without adding a decision. The review still happens: the commit is
read before it lands, and [RELEASE-4](#RELEASE-4) refuses to tag anything that is not on the
trunk at the remote.

`verified-by: by-construction (the bump target commits an explicit list of paths and contains no push or tag, and refuses when one of those paths is already modified; it runs contrib/release-preflight.py before changing any file, and make check-scripts runs that script's selftest, which pins each warning, that the advisory date comes from the issue comment when there is one and from the ledger otherwise, that a comment from someone without write access is ignored, that a declined prompt or no terminal without YES=1 stops the bump, and that YES=1 goes on)`

<a id="RELEASE-3"></a>
### RELEASE-3: GitHub Actions does not publish a GitHub release

No branch push, no pull request, no tag push, and no manually dispatched GitHub Actions run
creates a GitHub release or attaches binaries people install. Publication of those assets
happens in Jenkins, when `bravebot-build` is run with `UPLOAD` and `RELEASE`. The npm package
is a later step, specified below, and is not a GitHub release.

**Why.** GitHub Actions cannot codesign Darwin or Authenticode-sign Windows. If it published,
the public assets would be unsigned until a later overwrite, and a tag re-push would put those
unsigned bytes back.

`verified-by: by-construction (ci.yml has no release job and contents: write is not granted)`

<a id="RELEASE-4"></a>
### RELEASE-4: nothing is tagged from a tree that was not reviewed

A tag is created only from a clean working tree, on the trunk, at the same commit the remote
trunk is at, and only when that tag does not already exist.

**Why.** The tag is the record of what was released, so it has to name a commit others can see.
Tagging a dirty tree names a commit that does not contain what was built; tagging a branch names
work that was never reviewed; tagging ahead of the remote names a commit nobody else has.

`verified-by: by-construction (each condition is a separate refusal in the tagging path, checked before the tag is created)`

<a id="RELEASE-5"></a>
### RELEASE-5: the published name is the version in the tree that was built

The makefile names the tag `v` plus the version in `Cargo.toml`, after refusing any
disagreement among the files that state it. Jenkins names the GitHub release the same way from the `Cargo.toml`
of the commit it checked out, after the same refusal. Neither path publishes under a name that
disagrees with the tree it built.

**Why.** The installer derives the tag it downloads from the version it was published with, so
a release whose name is not the version in the tree is a release whose assets the installer
looks for under a name that was never uploaded.

`verified-by: by-construction (the tagging path sets the tag from Cargo.toml after make check-versions, a Jenkins RELEASE refuses unless that tag already names the commit it checked out and package.json states the same version, then names the GitHub release from Cargo.toml, and the npm publish refuses unless the tag is v plus the same version in both files)`

<a id="RELEASE-6"></a>
### RELEASE-6: a published binary carries the configuration it needs to run

A build that is going to be published fails when the configuration baked into it is missing.
Builds that nobody installs, a pull request or a fork in particular, are allowed to lack it and
compile anyway.

**Why.** Configuration is captured at build time, so a binary built without it cannot reach the
backend at all. Shipping one moves the failure from a build log, where it is one person's problem
and obvious, to every machine that installed it, where it reads as the program being broken.

**Note.** The permission to build unconfigured is granted by an exact value, so a setting that is
present but means nothing does not grant it.

`verified-by: by-construction (the build fails on missing configuration unless permitted, and a Jenkins upload does not grant that permission)`

<a id="RELEASE-7"></a>
### RELEASE-7: a release has every supported platform in it, or is not published

Every platform the installer can ask for is present before anything is uploaded. A missing one
fails the release instead of publishing the rest.

**Why.** The installer maps a platform to exactly one asset name, so a partial release is not a
smaller release. It is a broken install for whoever is on the platform that went missing, and it
reads to them as the release existing but the tool not working.

`verified-by: by-construction (the publish job in devops downloads each of the six named assets before it creates the GitHub release; a missing object fails that download. This tree does not contain that job)`

<a id="RELEASE-8"></a>
### RELEASE-8: every asset is published with a checksum of the bytes that were uploaded

Each asset has a checksum published beside it, covering the asset in its final form, after every
step that alters its bytes.

**Why.** A checksum taken before a later step rewrites the file is worse than none: it will not
match, and the mismatch looks exactly like tampering, so the one signal that is supposed to
distinguish a bad download from a good one now fires on every good one.

`verified-by: by-construction (Jenkins attaches the .sha256 written after signing, covering the bytes that were uploaded)`

<a id="RELEASE-9"></a>
### RELEASE-9: a downloaded binary is checked against its published checksum before it is installed

Each installer fetches the published checksum, compares it against what it downloaded, and writes
no executable when the two differ or the checksum is not well formed. Well formed is the digest
and nothing else: sixty-four hex digits, with no filename beside them.

**Why.** Without this the binary runs on the strength of the transport alone, and a substituted
release asset is indistinguishable from a good one. The checksum is what an installer can check
on every platform. It is published beside the binary, so it shows that the download arrived
intact and not who produced it; [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) are what
check that.

`verified-by: by-construction (each installer hashes what it downloaded, compares it against the published value, and exits without writing when they differ or the published value is not sixty-four hex digits)`

<a id="RELEASE-10"></a>
### RELEASE-10: the npm package is published by hand, after that version's GitHub release exists

A branch push, a pull request, and a tag push do not publish to the registry. Someone dispatches
`publish-npm.yml` with the version tag after Jenkins has created the GitHub release of that name.
What is published is the tree that tag names, and a branch of the same name does not supply it. A
dispatch whose tag is not `v` plus the version in the tree, or whose GitHub release is missing any
platform asset, any checksum, or the signature beside a Linux checksum, or whose Linux signature
does not verify against the key the tree carries, is refused rather than publishing a wrapper whose
installer has nothing to fetch or refuses what it fetches.

**Why.** The installer derives the download from the version it was published with. Publishing
the wrapper first makes every install fail until the assets exist, which reads as the tool being
broken. Jenkins is started by hand, days later if need be, so npm publish is the same kind of
step: it happens when somebody chooses, not when the tag lands. Publishing from a branch puts
an unreviewed version on the registry.

`verified-by: by-construction (publish-npm.yml runs only on workflow_dispatch, checks out refs/tags/ the given tag so a branch of that name cannot supply the tree and a tag that does not exist fails the run, refuses unless that tag is v plus the version in both files, and refuses unless each named asset and its checksum, and each Linux checksum's signature, are on the GitHub release and each of those signatures verifies against the key in npm/scripts/postinstall.js; make check-security faults a checkout of a bare name)`

<a id="RELEASE-11"></a>
### RELEASE-11: the registry authenticates the workflow, not a stored token

The publish job proves itself with a short-lived OIDC identity for this workflow in this
repository, and sets no long-lived npm credential. The package it publishes carries provenance
for that run. No dependency of this repository is installed or run in the job that holds the grant:
the grant is declared on that job rather than for the whole workflow, and the lockfile lint is a
job of its own that holds only `contents: read`.

**Why.** A token in GitHub secrets is a credential that publishes if it leaks, and it outlives
the run that needed it. OIDC binds the publish to this file on this repository, so a different
workflow, or the same workflow in a fork, cannot use it. The grant is a permission to request a
token, and every step of the job holding it can exercise that permission, so a job is the smallest
boundary it has: a dependency installed or run in the publishing job could mint the credential and
publish a tarball with this repository's provenance on it, while under `contents: read` the
identical compromise reaches a green check and nothing else. The wrapper declares no dependency
today, so the job that lints holds the lower grant to keep that true of whatever is added. The trusted publisher on npmjs.com keys on the repository and the workflow filename rather
than a job name, so which job publishes is this repository's to choose.

`verified-by: by-construction (the publish job grants id-token: write, sets no NPM_TOKEN or NODE_AUTH_TOKEN, and calls npm publish --access public --provenance; make check-security faults a step that installs or runs an npm dependency in a job holding the grant)`

<a id="RELEASE-12"></a>
### RELEASE-12: the published package has no dependency, and the pnpm lockfiles are committed and linted

The wrapper that is published declares no dependency, so installing it fetches nothing but the
wrapper. The three JavaScript projects that are not published (`ui`, `docs/website` and
`packages/agent-client`) each commit a `pnpm-lock.yaml`, CI installs from it with
`--frozen-lockfile`, and a lockfile that pulls from anywhere but the npm registry, or from a
GitHub repository other than the one its `package.json` names, fails that job.

**Why.** A dependency that appeared only at install time would be bytes nobody reviewed, and the
first place that would show up is the pipeline that publishes. The lockfile is the whole of what
an install fetches, so an entry edited to point at another host changes what runs in CI without
touching `package.json`.

`verified-by: by-construction (package.json declares no dependency and publish-npm.yml refuses a tag whose package.json declares one; each project's pnpm-lock.yaml is in the tree, CI installs with pnpm install --frozen-lockfile, and contrib/check-pnpm-lockfiles.py refuses a registry entry that is not an sha512 integrity alone, a git source the project's package.json does not name, and a link, file or URL version)`

<a id="RELEASE-13"></a>
### RELEASE-13: each installer's release origin is composed from nothing

The repository an installer downloads from is stated in the installer. Nothing outside it chooses
that repository: not an argument, not a file, and not the environment.

**Why.** An installer fetches an asset and the checksum of that asset from the same release, so an
origin supplied from outside moves both halves of the comparison together. A substituted binary
published beside its own true digest then satisfies the checksum check rather than failing it, and
is written executable onto the path. Nothing else on the install path notices: the package
manifest names the real package, the lockfile carries the registry's integrity hash for it, and
the install prints the version it fetched rather than where it fetched it from. The line a person
is handed to update with is one fixed line for the same reason, which [updates.md](updates.md)
sets out.

The one input install.sh accepts is a version: three decimal numbers with an optional leading `v`,
composed into a tag under that fixed repository, and refused before any request when it is
anything else. It chooses a release, never a host or a path, and every check in
[RELEASE-9](#RELEASE-9), [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) runs unchanged
against it. `crates/cli/tests/installer_signature.rs` pins which URLs each argument reaches.

`verified-by: by-construction (install.sh states the repository as a literal, and the npm installer composes both URLs from one constant; npm/tests/postinstall.test.mjs calls that origin with an environment set against it, holds the installer to that one URL, and fails on any reach into the environment beyond the two values that choose whether to download and for which architecture. make check-npm runs it)`

<a id="RELEASE-14"></a>
### RELEASE-14: on Linux, a checksum whose signature does not verify installs nothing

On Linux, where `gpg` can be run, each installer also fetches the signature published beside the
checksum and checks it over the checksum exactly as it was fetched, against the release signing key
it carries embedded. The signature counts only when it was made by the key the installer names by
its fingerprint. When the signature cannot be fetched, or does not verify, or was made by any other
key, no executable is written. Where `gpg` cannot be run, the installer says it skipped the check
and installs on the checksum alone. Darwin and Windows publish no such signature and are checked
as [RELEASE-15](#RELEASE-15) says.

**Why.** The Linux binary carries no signature of its own, and its checksum is uploaded beside it,
so whoever can replace the one can replace the other: the checksum proves the download arrived
intact, not who produced it. A signature from a key held outside the release means a substituted
binary also needs that key. The key is embedded rather than fetched because the bytes that decide
what to trust are then the installer's own, reviewed and published with it, and no second host can
withhold it or fail an install for a reason that has nothing to do with what was downloaded. A
missing signature is refused rather than skipped because skipping it would let whoever can change
the release turn the check off by deleting one file, and that is the person the check exists for. A
missing `gpg` is skipped rather than refused because it is a property of the machine, which nobody
changing a release controls, and requiring it would fail an install for a reason that has nothing
to do with what was downloaded.

**Note.** The check runs in a keyring made for it and removed afterwards, so nothing is read from
or added to the person's own. The signature covers the checksum and nothing else, so it shows who
produced that checksum and not which release it belongs to: whoever can replace release assets can
serve an older release's binary, checksum and signature together, and they verify.

`verified-by: bravebot_cli::installer_signature::both_installers_embed_the_release_key_they_name`
`verified-by: bravebot_cli::installer_signature::a_linux_checksum_signed_by_the_release_key_installs`
`verified-by: bravebot_cli::installer_signature::a_checksum_replaced_after_it_was_signed_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_checksum_signed_by_any_key_but_the_release_key_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_linux_checksum_with_no_signature_installs_nothing`
`verified-by: bravebot_cli::installer_signature::without_gpg_a_linux_install_says_it_skipped_the_signature_and_installs`
`verified-by: bravebot_cli::installer_signature::a_darwin_binary_signed_by_brave_and_notarized_installs`
`verified-by: bravebot_cli::installer_signature::a_windows_binary_signed_by_brave_installs`

<a id="RELEASE-15"></a>
### RELEASE-15: on macOS and Windows, a binary not signed by Brave installs nothing

Before an executable is written, each installer checks the code signature the binary carries.
On macOS the binary has to satisfy a `codesign` requirement that names Brave's Developer ID team,
and has to pass `spctl` as an installer package, which is where notarization is checked. On
Windows, in the npm installer, Authenticode has to report the signature as valid and the signing
certificate's name has to be `Brave Software, Inc.`. When the signature is missing, is from any
other signer, or does not verify, or when the tool that checks it cannot be run, no executable is
written. Linux binaries carry no code signature and are checked as [RELEASE-14](#RELEASE-14)
says. The install script does not install Windows.

**Why.** The checksum is published beside the binary, so whoever can replace one can replace the
other, and the checksum cannot say who produced the binary. The macOS and Windows binaries are
already signed and the operating systems can check that, but a binary run from a terminal is not
assessed by Gatekeeper, so an installer that does not ask never finds out. `codesign` and
Authenticode on their own accept a valid signature from any signer whose certificate a trusted
authority issued, which includes anybody with a developer account, so the requirement names
Brave's team and the Windows check names the signer. A missing tool is refused rather than skipped
because, unlike `gpg` on Linux, these tools ship with the operating system, so a machine without
one is not a machine the installer should trust an unchecked binary on. `spctl -t install` is used
because `-t execute` rejects any bare executable that is not an app bundle, however it is signed.

**Note.** The macOS check was run by hand against the published arm64 release and accepted it. The
Windows check has not been run on Windows: the signer name was read from the certificate with
`openssl`, and the PowerShell that reads it is exercised here only through a stand-in. It
compares the certificate's common name and pins no thumbprint, so a certificate carrying that
name from another authority the machine trusts would pass; a thumbprint would refuse every install
at the next certificate renewal. The tests replace `codesign`, `spctl` and `powershell.exe` with
programs that answer from a marker in the binary, so they hold the installers to what they ask and
how they treat the answer, and not to what the real tools say. `spctl` can need the network to check notarization, so an offline macOS
install may be refused.

`verified-by: bravebot_cli::installer_signature::a_darwin_binary_signed_by_brave_and_notarized_installs`
`verified-by: bravebot_cli::installer_signature::a_darwin_binary_signed_by_another_team_installs_nothing`
`verified-by: bravebot_cli::installer_signature::an_unsigned_darwin_binary_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_darwin_binary_that_is_not_notarized_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_darwin_install_without_codesign_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_windows_binary_signed_by_brave_installs`
`verified-by: bravebot_cli::installer_signature::a_windows_binary_signed_by_someone_else_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_windows_binary_whose_signature_is_not_valid_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_windows_install_without_powershell_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_linux_install_runs_neither_codesign_nor_powershell`

## Known costs

- **The signature checks are pinned by a Rust test, and the npm origin by a Node one.** Every
  clause but [RELEASE-13](#RELEASE-13), [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) is
  by-construction, which means a refusal can be removed and only a reader will notice. The tests
  for [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) run on Linux alone and skip where
  `gpg` or `node` is missing, so on a macOS checkout it passes having checked nothing; the Linux job in CI has
  both. The tagging path is shell in a makefile,
  publication is a Jenkins job in another repository, and a test that shelled out to a real tag
  push would have to publish something to prove anything. Three checks hold part of it from outside
  the suite. `make check-security` holds the shape of the publish workflow rather than any of these
  refusals: that no job beside the credential installs a dependency, and that the checkout names a
  kind of ref. `make check-versions` holds [RELEASE-1](#RELEASE-1) itself, over the six files that
  state a version, and carries its own selftest because a version check that is quietly partial
  reports success forever. Both of those are read off files, so a refusal deleted from a `run:`
  block passes them. `make check-npm` is the one that runs something: it calls the npm installer's
  origin as code, so [RELEASE-13](#RELEASE-13) fails rather than reports when that origin becomes
  something a caller supplies. It reaches the npm installer alone, and the shell script beside it
  is read by nothing.

- **Publication lives outside this repository.** `bravebot-build` in devops is what signs and
  attaches assets. A change there can break RELEASE-6 through RELEASE-8 without this tree
  noticing. RELEASE-9 is the installer in this repository: a checksum Jenkins ships in the
  wrong form is refused here, not accepted.

- **Only the npm publish refuses a release whose Linux checksums carry no signature that verifies.**
  The publish job in devops checks for each binary but not for a signature. The install script
  fetches the newest GitHub release whether or not npm has been published, so a release Jenkins
  publishes without them is refused by every Linux install that has `gpg` from the moment it
  exists, and the npm check comes too late to prevent that.

- **Changing the signing key breaks every installer that carries the old one.** Each npm version
  carries the key it was published with and installs its own version's release, so it keeps
  working while that release is served. The install script is served from the trunk and installs
  the newest release, so the key it carries has to change at the moment the first release signed
  by the new key is published, and either order leaves a window in which Linux installs with `gpg`
  are refused. The macOS team and the Windows signer name are embedded the same way, so a change
  of either reaches only the installers shipped after it. An embedded key cannot be revoked or updated after it is published: a revocation or
  a new signing subkey reaches only the installers shipped after it.

- **The refusals before a tag guard the tag, not the branch.** Tagging checks the branch, the
  remote, and the agreement among the files that state a version. The publish job builds the tip of a
  branch it is handed, refuses a `package.json` mismatch, and refuses unless tag `v` plus the
  version in `Cargo.toml` already names that commit. A commit that is not `origin/main` can
  still be published if that is what the tag names and BRANCH points there.

- **Who published a release is not recoverable from here.** A publish is a parameterised build in
  another system, so who asked for it lives in that system's history. The job requires the tag
  to name the commit it built, so a GitHub release created by that job is built from that tag. A
  tag with no GitHub release is not a publish.

- **No build in this repository is configured.** Every job in the workflow carries the permission
  to compile without credentials, so the first configured build of a commit happens in the publish
  job. A change that only fails with configuration baked in gets no signal before then.

- **A second Jenkins run of RELEASE for the same version fails.** `gh release create` does not
  replace an existing release, so a retry after a successful publish, or after a hand-made
  release of the same tag, stops before upload.

- **The trusted publisher is configured on npmjs.com, not here.** OIDC will refuse until that
  record names this repository and `publish-npm.yml` exactly. A mismatch looks like a 404 from
  the registry. Enabling 2FA on maintainer npm accounts is also outside this tree.

- **Two third party actions still run in the job holding the grant.** Checking out the tag and
  pointing npm at the registry is what that job is, so `actions/checkout` and `actions/setup-node`
  cannot be moved out of it. Both are pinned to a commit, which is the whole of what is held
  against them, so what remains is whoever owns those two commits.

- **A second npm publish of the same version fails at the registry.** Dispatching before Jenkins
  has created the GitHub release fails the asset check instead. A delayed Jenkins run does not
  start npm publish on its own.

- **OIDC trusted publishing does not support Jenkins.** That is why this one publication step
  is in GitHub Actions while the signed binaries stay in Jenkins.
