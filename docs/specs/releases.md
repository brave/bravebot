---
id: RELEASE
title: Releases
status: normative
governs:
  - Makefile
  - .github/workflows/ci.yml
  - .github/workflows/publish-npm.yml
  - npm/bin/bravebot.js
  - npm/scripts/stage-platform-package.js
  - npm/scripts/bootstrap-platform-packages.js
  - npm/tests/stage-platform-package.test.mjs
  - npm/tests/bootstrap-platform-packages.test.mjs
  - npm/tests/launcher.test.mjs
  - npm/tests/package.test.mjs
  - install.sh
  - install.ps1
  - package.json
  - ui/package.json
  - contrib/check-versions.py
  - contrib/release-preflight.py
  - contrib/package-manifests.py
  - contrib/check-pnpm-lockfiles.py
documented-by: docs/website/docs/quickstart.md
---

## Scope

Turning a commit into binaries somebody else installs: what names a version, what starts a
release, what refuses one, and what an installer trusts about what it fetched.

Building for a platform is not this topic. Reproducible cross-builds are ordinary code, described
in [../development/releasing.md](../development/releasing.md). This file governs only the path from a version to a
published asset, and the checks along it.

Four routes carry what is published: the npm packages, which hold the binaries themselves and are
built from the GitHub release when they are published, the shell script and the PowerShell script
served from the trunk of this repository, which fetch the release when they run, and the Homebrew
cask and the WinGet package, whose manifests name the release's assets and their digests. All are
governed here. Learning from a running copy that a newer release exists is [updates.md](updates.md).

GitHub Actions compiles and tests. It does not create a GitHub release. Signed, configured
binaries are built and published by the Jenkins job `bravebot-build` in the devops repository.
The npm package is published afterwards, from a manually dispatched GitHub Actions workflow,
once that release exists.

Most of this is not pinned by a Rust test. Those rules are enforced by a refusal in the tagging
path, the Jenkins publish path, or the npm publish workflow rather than by anything the test
suite can execute, so each of those clauses says in brackets what makes it hold. Some are the
exception. What the package script downloads from, what the wrapper installs and which package the
launcher runs are pinned by Node tests that the npm lockfile job runs and `cargo test` does not
reach, and what the package script and `install.sh` do with a signature is pinned by a Rust test
that runs both of them against a release it made. What `install.ps1` fetches, checks and writes is
pinned by another, which runs it under `pwsh`.

## Clauses

<a id="RELEASE-1"></a>
### RELEASE-1: one version names a release, and every file that states it agrees

The workspace manifest holds the version. Every other file that repeats it, the npm package
manifest and the desktop application's manifest in particular, states the same value, and a
disagreement stops a release rather than being resolved in favour of either. The npm package
manifest repeats it for each of its six platform packages, which it pins by exact version. Those
packages are written from that manifest when they are published, so each states the version in no
file of its own. A disagreement is reported by a check that runs on every pull request, rather
than only by the refusal at the tag.

**Why.** The publish step derives the tag it downloads from the version in the tree, so two
files disagreeing does not produce a mislabelled release, it produces a release whose assets that
step looks for under a name that was never uploaded. The application is packaged from its own
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

**Why.** The publish step derives the tag it downloads from the version in the tree, so
a release whose name is not the version in the tree is a release whose assets that step
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
### RELEASE-9: a downloaded binary is checked against its published checksum before it is written

`install.sh`, `install.ps1` and the script that builds the npm platform packages each fetch the
published checksum, compare it against what they downloaded, and write no executable when the two
differ or the checksum is not well formed. Well formed is the digest and nothing else: sixty-four hex
digits, with no filename beside them. An npm install fetches no checksum: the registry's integrity
hash covers the bytes of the package it serves, and those bytes passed this check when the package
was built.

**Why.** Without this the binary runs, or is published, on the strength of the transport alone,
and a substituted release asset is indistinguishable from a good one. The checksum is what both
can check on every platform. It is published beside the binary, so it shows that the download
arrived intact and not who produced it; [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15)
are what check that.

`verified-by: bravebot_cli::installer_powershell::a_binary_that_does_not_match_its_checksum_installs_nothing`
`verified-by: bravebot_cli::installer_powershell::a_malformed_checksum_installs_nothing`
`verified-by: bravebot_cli::installer_powershell::a_checksum_in_either_case_installs`
`verified-by: by-construction (install.sh and the package script each hash what they downloaded, compare it against the published value, and exit without writing when they differ or the published value is not sixty-four hex digits)`

<a id="RELEASE-10"></a>
### RELEASE-10: the npm packages are published by hand, after that version's GitHub release exists

A branch push, a pull request, and a tag push do not publish to the registry. Someone dispatches
`publish-npm.yml` with the version tag after Jenkins has created the GitHub release of that name.
What is published is the tree that tag names, and a branch of the same name does not supply it.
Seven packages are published: the six platform packages, and the wrapper only after all six are on
the registry. A dispatch is refused before anything is published when its tag is not `v` plus the
version in the tree, when the wrapper runs an install script or depends on anything but the six
platform packages at its own version, when its GitHub release is missing any platform asset, any
checksum, or the signature beside a Linux checksum, or when a Linux signature does not verify
against the key the tree carries. A macOS or Windows binary that is not signed by Brave is refused
by the job for that platform, which can be after other platforms have been published. A package
already on the registry at that version is left alone, so a dispatch after a partial failure
publishes what is missing. A package name the registry has never held is created first, by
[RELEASE-21](#RELEASE-21), and not by this workflow.

**Why.** A platform package is built from the release, so it cannot exist before it. npm skips an
optional dependency it cannot find without a message, so a wrapper published ahead of its packages
installs and then has no binary until they exist, which reads as the tool being broken. Jenkins is
started by hand, days later if need be, so npm publish is the same kind of step: it happens when
somebody chooses, not when the tag lands. Publishing from a branch puts an unreviewed version on
the registry. A version cannot be published twice, and the signature checks need a macOS and a
Windows machine, so a failure on one platform can follow successes on others; leaving a published
package alone lets the next dispatch finish the release rather than fail on the first package it
finds.

`verified-by: by-construction (publish-npm.yml runs only on workflow_dispatch; its lint job refuses a wrapper that runs an install script or depends on anything but the six platform packages at its own version; its release job checks out refs/tags/ the given tag so a branch of that name cannot supply the tree and a tag that does not exist fails the run, refuses unless that tag is v plus the version in both files, and refuses unless each named asset and its checksum, and each Linux checksum's signature, are on the GitHub release and each of those signatures verifies against the key in npm/scripts/stage-platform-package.js; each platform job needs the release job and skips a package the registry already has, and the wrapper's job needs every platform job; make check-security faults a checkout of a bare name, and npm/tests/package.test.mjs holds the matrix of platform jobs to the six platforms, each on a runner of the system its signature check runs on)`

<a id="RELEASE-11"></a>
### RELEASE-11: the registry authenticates the workflow, not a stored token

Each job that publishes proves itself with a short-lived OIDC identity for this workflow in this
repository, and sets no long-lived npm credential. Every package it publishes carries provenance
for that run. No dependency of this repository is installed or run in a job that holds the grant:
the grant is declared on the jobs that publish rather than for the whole workflow, and the
lockfile lint and the checks of the release are jobs of their own that hold only `contents: read`.
Each of those jobs names the GitHub environment `npm-publish`, which has a required reviewer who is
not the person who dispatched, so a dispatch of any ref waits for a second person before a token
exists.

**Why.** A token in GitHub secrets is a credential that publishes if it leaks, and it outlives
the run that needed it. OIDC binds the publish to this file on this repository, so a different
workflow, or the same workflow in a fork, cannot use it. The grant is a permission to request a
token, and every step of the job holding it can exercise that permission, so a job is the smallest
boundary it has: a dependency installed or run in the publishing job could mint the credential and
publish a tarball with this repository's provenance on it, while under `contents: read` the
identical compromise reaches a green check and nothing else. The wrapper depends on nothing but
the platform packages this workflow publishes, so the job that lints holds the lower grant to keep that true of whatever is added. The trusted publisher on npmjs.com keys on the repository and the workflow filename rather
than a job name, so which job publishes is this repository's to choose. `workflow_dispatch` runs the
file at the ref it is dispatched on and the record does not check the ref, so a person with write
access could push a branch holding a copy of this file without its refusals and dispatch that. The
dispatcher writes that copy, so nothing inside the file can stop it; the environment's reviewer is a
setting of the repository, which a copy cannot remove, and npm compares the environment claim once a
job declares one.

`verified-by: by-construction (the platform and wrapper publish jobs grant id-token: write, set no NPM_TOKEN or NODE_AUTH_TOKEN, and call npm publish --access public --provenance; make check-security faults a step that installs or runs an npm dependency in a job holding the grant, and faults a job holding the grant that names no environment:, which pins the key and not the environment's reviewer)`

<a id="RELEASE-12"></a>
### RELEASE-12: the published wrapper depends on its platform packages alone, and the pnpm lockfiles are committed and linted

The wrapper that is published declares no dependency except exactly its six platform packages,
as `optionalDependencies`, so installing it fetches nothing but the wrapper and the package for
the machine. The three JavaScript projects that are not published (`ui`, `docs/website` and
`packages/agent-client`) each commit a `pnpm-lock.yaml`, CI installs from it with
`--frozen-lockfile`, and a lockfile that pulls from anywhere but the npm registry, or from a
GitHub repository other than the one its `package.json` names, fails that job.

**Why.** A dependency that appeared only at install time would be bytes nobody reviewed, and the
first place that would show up is the pipeline that publishes. The platform packages are the
exception because they are the binary: this workflow builds each from the verified release and
publishes it, and the wrapper pins each at its own version, since a range would let an install take
a binary from another release. The lockfile is the whole of what
an install fetches, so an entry edited to point at another host changes what runs in CI without
touching `package.json`.

`verified-by: by-construction (package.json declares no dependency but the six platform packages at its own version, which npm/tests/package.test.mjs holds and publish-npm.yml refuses a tag whose package.json breaks; each project's pnpm-lock.yaml is in the tree, CI installs with pnpm install --frozen-lockfile, and contrib/check-pnpm-lockfiles.py refuses a registry entry that is not an sha512 integrity alone, a git source the project's package.json does not name, and a link, file or URL version)`

<a id="RELEASE-13"></a>
### RELEASE-13: each tool's release origin is composed from nothing

The repository `install.sh`, `install.ps1` and the package script download from is stated in
each. Nothing outside it chooses that repository: not an argument, not a file, and not the
environment.

**Why.** Each fetches an asset and the checksum of that asset from the same release, so an
origin supplied from outside moves both halves of the comparison together. A substituted binary
published beside its own true digest then satisfies the checksum check rather than failing it, and
is written executable onto the path, or is published to the registry under Brave's name with
provenance for a run that did nothing wrong. Nothing else on either path notices: the package
manifest names the real package, and the log prints the version it fetched rather than where it
fetched it from. The line a person is handed to update with is one fixed line for the same reason,
which [updates.md](updates.md) sets out.

The one input either install script accepts that reaches a URL is a version: three decimal numbers
with an optional leading `v`, composed into a tag under that fixed repository, and refused before
any request when it is anything else. It chooses a release, never a host or a path, and every
check in [RELEASE-9](#RELEASE-9), [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) runs
unchanged against it. `crates/cli/tests/installer_signature.rs` pins which URLs each argument
reaches for `install.sh`, and `crates/cli/tests/installer_powershell.rs` does the same for
`install.ps1` and also holds it to the environment variables it is written to read, none of which
names a repository, a host, a tag or a URL. The tag the newest-release lookup answers with is held
to the shape of a published one before it is composed into a URL.

`verified-by: bravebot_cli::installer_powershell::the_script_installs_the_version_it_is_given_and_refuses_what_is_not_one`
`verified-by: bravebot_cli::installer_powershell::nothing_the_environment_names_chooses_where_the_script_downloads_from`
`verified-by: bravebot_cli::installer_powershell::a_newest_release_tag_that_is_not_a_version_installs_nothing`
`verified-by: by-construction (install.sh states the repository as a literal, and the package script composes both URLs from one constant and the version in the tree; npm/tests/stage-platform-package.test.mjs calls that origin with an environment set against it, holds the script to that one URL, and fails on any reach into the environment. make check-npm runs it)`

<a id="RELEASE-14"></a>
### RELEASE-14: on Linux, a checksum whose signature does not verify installs nothing

On Linux, where `gpg` can be run, `install.sh` and the package script also fetch the signature
published beside the checksum and check it over the checksum exactly as it was fetched, against
the release signing key they carry embedded. The signature counts only when it was made by the key
they name by its fingerprint. When the signature cannot be fetched, or does not verify, or was
made by any other key, no executable is written. Where `gpg` cannot be run, `install.sh` says it
skipped the check and installs on the checksum alone, and the package script writes nothing.
Darwin and Windows publish no such signature and are checked as [RELEASE-15](#RELEASE-15) says.

**Why.** The Linux binary carries no signature of its own, and its checksum is uploaded beside it,
so whoever can replace the one can replace the other: the checksum proves the download arrived
intact, not who produced it. A signature from a key held outside the release means a substituted
binary also needs that key. The key is embedded rather than fetched because the bytes that decide
what to trust are then the tool's own, reviewed and published with it, and no second host can
withhold it or fail an install for a reason that has nothing to do with what was downloaded. A
missing signature is refused rather than skipped because skipping it would let whoever can change
the release turn the check off by deleting one file, and that is the person the check exists for. A
missing `gpg` is skipped by `install.sh` rather than refused because it is a property of the
machine, which nobody changing a release controls, and requiring it would fail an install for a
reason that has nothing to do with what was downloaded. The package script runs on a runner the
workflow chose, which has `gpg`, so there the tool is part of the check, and a build that skipped
it for want of the tool would be the one an attacker arranges.

**Note.** The check runs in a keyring made for it and removed afterwards, so nothing is read from
or added to the person's own. The signature covers the checksum and nothing else, so it shows who
produced that checksum and not which release it belongs to: whoever can replace release assets can
serve an older release's binary, checksum and signature together, and they verify.

`verified-by: bravebot_cli::installer_signature::both_installers_embed_the_release_key_they_name`
`verified-by: bravebot_cli::installer_signature::a_linux_checksum_signed_by_the_release_key_installs`
`verified-by: bravebot_cli::installer_signature::a_checksum_replaced_after_it_was_signed_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_checksum_signed_by_any_key_but_the_release_key_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_signature_that_verifies_but_was_made_by_a_key_the_installer_does_not_name_installs_nothing`
`verified-by: bravebot_cli::installer_signature::a_linux_checksum_with_no_signature_installs_nothing`
`verified-by: bravebot_cli::installer_signature::without_gpg_a_linux_install_says_it_skipped_the_signature_and_installs`
`verified-by: bravebot_cli::installer_signature::a_linux_package_is_not_built_without_gpg`
`verified-by: bravebot_cli::installer_signature::a_darwin_binary_signed_by_brave_and_notarized_installs`
`verified-by: bravebot_cli::installer_signature::a_windows_binary_signed_by_brave_installs`

<a id="RELEASE-15"></a>
### RELEASE-15: on macOS and Windows, a binary not signed by Brave installs nothing

Before an executable is written, `install.sh`, `install.ps1` and the package script check the code
signature the binary carries. On macOS the binary has to satisfy a `codesign` requirement that names Brave's
Developer ID team, and has to pass `spctl` as an installer package, which is where notarization is
checked. On Windows, in `install.ps1` and the package script, Authenticode has to report the signature as
valid and the signing certificate's name has to be `Brave Software, Inc.`. When the signature is
missing, is from any other signer, or does not verify, or when the tool that checks it cannot be
run, no executable is written. Linux binaries carry no code signature and are checked as
[RELEASE-14](#RELEASE-14) says. `install.sh` does not install Windows, and `install.ps1` installs
nothing else: a PowerShell with no `Get-AuthenticodeSignature`, such as PowerShell on macOS or
Linux, installs nothing and names `install.sh`. The package script builds a macOS package
on a macOS runner and a Windows package on a Windows runner, because `codesign`, `spctl` and
PowerShell's Authenticode check run nowhere else. An npm install checks no signature of its own.

**Why.** The checksum is published beside the binary, so whoever can replace one can replace the
other, and the checksum cannot say who produced the binary. The macOS and Windows binaries are
already signed and the operating systems can check that, but a binary run from a terminal is not
assessed by Gatekeeper, so a tool that does not ask never finds out. `codesign` and
Authenticode on their own accept a valid signature from any signer whose certificate a trusted
authority issued, which includes anybody with a developer account, so the requirement names
Brave's team and the Windows check names the signer. A missing tool is refused rather than skipped
because, unlike `gpg` on Linux, these tools ship with the operating system, so a machine without
one is not a machine to trust an unchecked binary on. `spctl -t install` is used
because `-t execute` rejects any bare executable that is not an app bundle, however it is signed.

**Note.** The macOS check was run by hand against the published arm64 release and accepted it. The
Windows check has not been run on Windows: the signer name was read from the certificate with
`openssl`, and the PowerShell that reads it is exercised here only through a stand-in, in the
package script's tests and in `install.ps1`'s alike. `install.ps1` has been run under PowerShell 7
on macOS and Linux with `Get-AuthenticodeSignature` replaced by a function, and not under Windows
PowerShell 5.1, which is the edition Windows ships. It compares the certificate's common name and
pins no thumbprint, so a certificate carrying that
name from another authority the machine trusts would pass; a thumbprint would refuse every install
at the next certificate renewal. The tests replace `codesign`, `spctl` and `powershell.exe` with
programs, and `Get-AuthenticodeSignature` with a function, that answer from the file they are given,
so they hold `install.sh`, `install.ps1` and the package script to what they ask and how they treat
the answer, and not to what the real tools say. `spctl` can need
the network to check notarization, so an offline macOS install may be refused. The first run of the
package script on a hosted macOS or Windows runner is a release's publish.

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
`verified-by: bravebot_cli::installer_powershell::a_windows_binary_signed_by_brave_installs`
`verified-by: bravebot_cli::installer_powershell::a_windows_binary_signed_by_someone_else_installs_nothing`
`verified-by: bravebot_cli::installer_powershell::an_unsigned_windows_binary_installs_nothing`
`verified-by: bravebot_cli::installer_powershell::a_windows_binary_whose_signature_is_not_valid_installs_nothing`
`verified-by: bravebot_cli::installer_powershell::an_install_without_the_signature_tool_installs_nothing`

<a id="RELEASE-16"></a>
### RELEASE-16: installing the npm package runs no script and fetches nothing from the release

Installing `@brave/bravebot` runs no script and makes no request outside npm's own. The wrapper
holds the launcher and no binary. Each binary is in the package for its platform, which the
wrapper lists in `optionalDependencies` at its own exact version, and npm installs the one whose
`os` and `cpu` match the machine. A package npm cannot find, such as one for a version that has not
been published, is skipped and the install still succeeds.

**Why.** A script runs on a machine before the person has run anything of ours, and the npm
settings that ask for scripts to be allowed, or skip them, make an install that depends on one
fail or leave no binary: an install that depends on a script is refused under
`--strict-allow-scripts` and leaves nothing under `--ignore-scripts`. A binary in a package is
installed by npm itself, which works under both. A download keyed to the package's own version
fails for a version with no release, such as a checkout at a bumped but unreleased version, because
the assets it asks for do not exist yet. A platform package that does not exist is skipped instead,
and the launcher says what is missing ([RELEASE-17](#RELEASE-17)).

`verified-by: by-construction (package.json declares no preinstall, install, postinstall or prepare script and no dependency but the six platform packages at its own version, which npm/tests/package.test.mjs holds and the lint job of publish-npm.yml refuses at publish; make check-versions holds each pin to the workspace version; npm/tests/stage-platform-package.test.mjs holds each package's os and cpu to the asset it holds)`

<a id="RELEASE-17"></a>
### RELEASE-17: the launcher runs the package for its own platform, and says what is missing

The launcher runs the binary in `@brave/bravebot-<platform>-<arch>`, named from `process.platform`
and `process.arch` of the node running it and from nothing else: not an argument, not a file, and
not the environment. A package for another platform is never run. When that package is not
installed, or holds no binary, the launcher exits 1 with a message that names the package and
platform, the installs that leave it out, and what to do. It tells the binary it was started by
npm ([UPDATE-2](updates.md#UPDATE-2)) and exits with the binary's status.

**Why.** Which package is run decides which executable starts, so no value somebody supplies may
choose it, for the reason [RELEASE-13](#RELEASE-13) gives for the origin. The binary can still be
absent: `--omit=optional` skips the package, a platform with no package has none, and a version
that has not been published has none. The message says what happened, because advice to reinstall
changes nothing in those cases.

**Note.** Node's own lookup also searches the directories `NODE_PATH` names, after the install
tree, so a package placed there is run when the installed one is missing. The launcher reads no
variable itself.

`verified-by: by-construction (npm/tests/launcher.test.mjs lays out a node_modules tree and runs the real launcher in it: it runs the package of its own platform with its arguments and its exit status, never another platform's, finds a package installed beside the wrapper or beneath it, names the missing package when there is none or no binary, reads no environment value except to hand it on, and runs a package built by the real publish step. make check-npm runs it)`

<a id="RELEASE-18"></a>
### RELEASE-18: the Homebrew cask and the WinGet package name the release that was published, and nothing else

`contrib/package-manifests.py` writes both from a tag and the `.sha256` files of that GitHub
release. The tag has to be `v` plus the version in the tree, each asset URL is the fixed
repository's release URL for that tag, and each digest is the published `.sha256` value of the
asset the URL names. It writes nothing and fails when the tag is anything else, when a digest file
is missing, or when one is not 64 hex digits: a package with a digest it did not read would
install whatever was served.

**Why.** The package manager's check is the digest in its manifest, so the manifest decides what
is installed. The origin is the same literal [RELEASE-13](#RELEASE-13) requires, for the same reason:
a URL and a digest both chosen from outside agree with each other whatever the bytes are.

Only the macOS and Windows binaries have a package, and what the package manager checks is weaker
than the installers' checks. Homebrew and WinGet compare the downloaded bytes to the digest in the
manifest and no more, so [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) are not repeated:
no signature is verified at install, and the team and the signer are not compared. That is
accepted because both packages are portable single binaries that the cask's tap and the WinGet
repository publish from digests taken after Jenkins signed the asset, and Gatekeeper and SmartScreen
still assess a signed binary when it runs. Someone who can change the manifest, which is a pull
request reviewed in another repository, can change what installs.

`verified-by: by-construction (the script states the repository as a literal and composes every URL from it and the tag; its selftest, which make check-scripts runs, writes both packages from made-up digests and pins that each digest and URL lands in the manifest, and that a tag that is not the tree's version, a missing digest file for each of the four assets, and a digest that is empty, short, long or not hex each fail having written nothing)`

<a id="RELEASE-19"></a>
### RELEASE-19: the packages are written by hand, after that version's GitHub release exists

`make package-manifests TAG=v<version>` downloads the macOS and Windows `.sha256` files of that
release and runs the script into `dist/packages/`. Nothing in this tree opens the change against
the tap or the WinGet community repository, and no workflow does.

**Why.** Both repositories are somebody else's and take a pull request from an account that is
not this one's workflow, so there is no token here to grant and none to leak. The release must
exist first because the digests are the ones Jenkins published, so a run before it fails at the
download rather than writing a package for a version nobody can fetch.

`verified-by: by-construction (the target requires TAG on the command line and has no step that pushes, tags or opens a pull request; gh release download fails before the script runs when the release or a checksum asset does not exist)`

<a id="RELEASE-20"></a>
### RELEASE-20: `install.ps1` puts its directory on the user's `PATH`, and changes nothing else in it

Once the binary is in place, `install.ps1` adds the directory it installed to the end of the
person's own `PATH`, the one stored for their account, and to the `PATH` of the session it ran in. A
directory the session already finds is left alone, and so is one the stored value already names,
whether the entry differs in case, has a separator after it, or is a `%NAME%` reference to it. Case
is folded, so where a folder holds two directories whose names differ only in case, as a
case-sensitive folder on Windows can, the script takes them for one and does not add the directory
if its twin is already on the `PATH`. An entry that only starts with the directory's name, one
inside it, the one that holds it, and one with the same last name under another parent are different
directories and do not stop it being added. Every other entry is written back as it was: the stored
value is read with its `%NAME%` references unexpanded and written as an expandable string, and a
value that is missing, empty or ends in a separator gains no empty entry. A directory whose name
holds a semicolon or a percent sign, a stored value the account cannot write, and
`BRAVEBOT_NO_MODIFY_PATH` set to anything leave the stored value and the session as they were, say
which directory to add, and do not undo the install. A refused install does not read or change
either `PATH`.

**Why.** The script already knows the directory, so it can add it at no cost to the person. Without
that, the person has to find out how Windows keeps a `PATH`. The stored value is the person's, and it
outlives the install, so the script changes the one entry it owns: reading it expanded and writing
that back replaces every `%NAME%` reference with what it names today and leaves a plain string that
Windows no longer expands, which changes entries this script has no business with. The comparison
folds case because Windows compares `PATH` entries that way and a folder is case-insensitive unless
it was made otherwise, which the script has no cheap way to read for each entry; the cost is the
twin directory above, and it joins nothing else. The directory goes last because a directory put
first decides which program runs under a name the `PATH` already resolved, and `bravebot.exe` is no
reason to move any other. A semicolon divides a `PATH`, so a directory holding one would be written
as two entries, neither of them the directory, and a percent sign starts a reference Windows expands
in a stored value, so a terminal opened later would look in a different directory than the one
installed to. An install that was refused has put nothing where the `PATH` would lead, so there is
nothing to add. The variable is the way out for a person who manages their `PATH` themselves, and it
works through `irm | iex`, which cannot take an argument.

**Note.** The tests replace the calls that read and write the registry with files, so they hold the
script to what it asks and what it writes, and not to what Windows does with it. What Windows does
is in Known costs.

`verified-by: bravebot_cli::installer_powershell::a_directory_the_session_already_finds_is_left_alone`
`verified-by: bravebot_cli::installer_powershell::the_directory_is_appended_to_the_users_path_and_to_the_session`
`verified-by: bravebot_cli::installer_powershell::a_directory_already_on_the_users_path_is_not_added_again`
`verified-by: bravebot_cli::installer_powershell::a_different_directory_on_the_users_path_does_not_stop_the_directory_being_added`
`verified-by: bravebot_cli::installer_powershell::an_empty_or_unterminated_users_path_is_joined_without_an_empty_entry`
`verified-by: bravebot_cli::installer_powershell::a_person_who_opts_out_keeps_their_path_as_it_is`
`verified-by: bravebot_cli::installer_powershell::a_refused_registry_write_leaves_the_install_and_says_what_is_left`
`verified-by: bravebot_cli::installer_powershell::a_directory_a_path_would_misread_is_not_put_on_it`
`verified-by: bravebot_cli::installer_powershell::a_refused_install_leaves_the_users_path_alone`

<a id="RELEASE-21"></a>
### RELEASE-21: a platform package's name exists on the registry before the workflow publishes to it

`make npm-bootstrap` publishes a placeholder for each platform package the registry has no package
of, so that a trusted publisher can be added to it on npmjs.com. The placeholder is version
`0.0.0-bootstrap`, published under the `bootstrap` tag, and holds a manifest and the licence: no
binary, no script and no dependency. The publish is made with the maintainer's own `npm login`, and
nothing in the tree or in a workflow holds a credential for it. The target does nothing when every
name exists, when npm is not logged in, or when the registry answers a lookup with anything but a
package or a 404. `make publish-npm` runs the same script with `--check` before it dispatches, and
stops naming each platform package the registry lacks.

**Why.** A trusted publisher is a record on a package, and a package that has never been published
has none. OIDC then has nothing to match, and the registry answers the workflow's publish with a
404. The first version of a name has to come from somewhere other than the workflow. Making it a
placeholder keeps every version with a binary on the path [RELEASE-11](#RELEASE-11) describes:
published by the workflow, with provenance, after the environment's reviewer approves. The wrapper
pins each platform package at its own exact version, and the placeholder is a prerelease of `0.0.0`
under another tag, so no install of the wrapper resolves to it. The check runs where it fails in a
second at the maintainer's terminal. Without it, the dispatch costs a second person's approval and
six runner jobs before it fails on the 404.

`verified-by: by-construction (npm/tests/bootstrap-platform-packages.test.mjs runs the script against a fake npm: it publishes a placeholder for each name the registry answers 404 for and for none it holds, never one with more than a manifest and the licence, never at the version of a release or under latest, nothing without a login, and nothing when the registry does not answer with a package or a 404; the check names every missing package and publishes nothing; the script reads no environment value and starts npm alone; and the recipe of make publish-npm is held to run the check before gh workflow run. make check-npm runs it)`

## Known costs

- **The signature checks are pinned by a Rust test, and the package script and launcher by Node
  ones.** Every clause but [RELEASE-13](#RELEASE-13) to [RELEASE-17](#RELEASE-17) is
  by-construction, which means a refusal can be removed and only a reader will notice. The tests
  for [RELEASE-14](#RELEASE-14) and [RELEASE-15](#RELEASE-15) run on Linux alone and skip where
  `gpg` or `node` is missing, so on a macOS checkout it passes having checked nothing; the Linux job in CI has
  both. The tests for `install.ps1` run wherever `pwsh` is installed and skip where it is not, which
  the Linux job in CI has, since its image ships PowerShell. The tagging path is shell in a makefile,
  publication is a Jenkins job in another repository, and a test that shelled out to a real tag
  push would have to publish something to prove anything. Three checks hold part of it from outside
  the suite. `make check-security` holds the shape of the publish workflow rather than any of these
  refusals: that no job beside the credential installs a dependency, and that the checkout names a
  kind of ref. `make check-versions` holds [RELEASE-1](#RELEASE-1) itself, over the six files that
  state a version, and carries its own selftest because a version check that is quietly partial
  reports success forever. Both of those are read off files, so a refusal deleted from a `run:`
  block passes them. `make check-npm` is the one that runs something: it calls the package script's
  origin as code and runs the real launcher in a tree laid out the way npm lays one out, so
  [RELEASE-13](#RELEASE-13) fails rather than reports when that origin becomes something a caller
  supplies, and [RELEASE-17](#RELEASE-17) when the launcher and the publish step stop agreeing. It
  reaches the package script alone, and the shell script beside it is read by nothing. It does not
  run `publish-npm.yml`, whose platform jobs run for the first time at a release.

- **`install.ps1` appends, so another `bravebot` can still be the one that runs.** It installs under
  `%LOCALAPPDATA%\Programs\bravebot` unless `INSTALL_DIR` or an earlier run's record names another
  directory, and adds that directory to the end of the `PATH` ([RELEASE-20](#RELEASE-20)). It does not
  say which `bravebot` runs when another, an npm copy's shim for one, comes first on the `PATH`.
  A terminal that is already open keeps the `PATH` it started with: the session the script ran in
  gets the directory, and one opened afterwards reads the stored value.

- **`install.ps1` has not run on Windows.** Its tests run it under PowerShell 7 on macOS and Linux,
  with the network and `Get-AuthenticodeSignature` replaced by functions. What Windows PowerShell
  5.1 does with it, including the TLS setting it adds for that edition, the architecture an emulated
  PowerShell reports, what `Get-AuthenticodeSignature` says about Brave's binary, and what the
  registry does with the `PATH` write and the call after it that tells running programs of the change,
  are first seen by the person who runs it. A command it needs that is missing is a refusal that writes nothing, so
  the failure is an install that does not happen.

- **On Windows the update line may not be able to replace a copy that is running.** The startup
  notice is shown inside a session, so the line it offers is run beside that session, and Windows
  does not let a running executable be overwritten. Where replacing it fails, `install.ps1` says so,
  installs nothing and leaves the old copy in place, and the person closes the session and runs the
  line again. Nothing in the tests runs it on Windows (the entry above).

- **Opening the package changes is manual, and a package lags its release.** Between a GitHub
  release and the tap and WinGet repository taking the change, the startup notice
  ([UPDATE-2](updates.md#UPDATE-2)) can offer `brew upgrade --cask bravebot` or `winget upgrade
  Brave.Bravebot` for a version neither can install yet. The command then says there is nothing to
  upgrade, and a later launch is told again. The tap repository this cask belongs in is not named anywhere in
  this tree, so who creates it, and what `brew install` names, is decided where it is published.

- **Publication lives outside this repository.** `bravebot-build` in devops is what signs and
  attaches assets. A change there can break RELEASE-6 through RELEASE-8 without this tree
  noticing. RELEASE-9 is `install.sh` and the package script, in this repository: a checksum
  Jenkins ships in the wrong form is refused here, not accepted.

- **Only the npm publish refuses a release whose Linux checksums carry no signature that verifies.**
  The publish job in devops checks for each binary but not for a signature. The install script
  fetches the newest GitHub release whether or not npm has been published, so a release Jenkins
  publishes without them is refused by every Linux install that has `gpg` from the moment it
  exists, and the npm check comes too late to prevent that.

- **Changing the signing key breaks the install script that carries the old one.** The npm
  packages carry no key and check nothing when installed, so a change reaches them only through the
  next publish, which reads the key from the tag it publishes. The install script is served from the
  trunk and installs the newest release, so the key it carries has to change at the moment the
  first release signed by the new key is published, and either order leaves a window in which Linux
  installs with `gpg` are refused. The macOS team and the Windows signer name are embedded the same
  way, in `install.sh` and the package script. An embedded key cannot be revoked or updated after it
  is published: a revocation or a new signing subkey reaches only the install scripts served, and
  the publishes made, after it.

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

- **The `npm-publish` environment's reviewer is configured on GitHub, not here.** `make
  check-security` faults a job holding `id-token: write` with no `environment:` key, and that is all
  a checkout can read. Whether the environment exists, has a required reviewer who is not the
  dispatcher, and is named on each of the seven trusted-publisher records is recorded in
  [branch-protection.md](../development/branch-protection.md#the-npm-publish-environment). An
  environment with no reviewer, or a record that omits it, passes every check here and leaves a
  dispatch of a branch's copy of the workflow unheld. A deployment rule limited to `v*` tags would
  narrow that and does not replace the reviewer, since the tag ruleset refuses an update or a
  deletion and not the creation of a tag.

- **The trusted publisher is configured on npmjs.com, not here.** OIDC will refuse until that
  record names this repository and `publish-npm.yml` exactly. A mismatch looks like a 404 from
  the registry. Each of the seven package names has a record of its own, so a platform package
  without one fails its job while the others publish. A record can be added only to a package that
  exists, which is what [RELEASE-21](#RELEASE-21) creates. Enabling 2FA on maintainer npm accounts
  is also outside this tree.

- **A placeholder is published by a person, with no provenance.** `make npm-bootstrap` uses the
  maintainer's own login and one-time password, so a maintainer who can publish under `@brave` can
  create any package name there, and the placeholder carries no provenance. It holds no binary, and
  every version that does is published by the workflow. Which accounts can publish under `@brave`
  is set on npmjs.com, outside this tree.

- **Two third party actions still run in each job holding the grant.** Checking out the tag and
  pointing npm at the registry is what those jobs are, so `actions/checkout` and
  `actions/setup-node` cannot be moved out of them. Both are pinned to a commit, which is the whole
  of what is held against them, so what remains is whoever owns those two commits.

- **A refused platform can come after others are published.** The macOS and Windows signatures are
  checked by the job for that platform, which holds the grant and publishes its own package, so a
  binary the check refuses is found after the packages of the other platforms may be on the
  registry. The wrapper is not published until every platform is, and the next dispatch publishes
  what is missing. A version on the registry cannot be taken back.

- **A second dispatch for a published version fails at the wrapper.** The registry will not take a
  version twice, and the platform jobs before it leave a published package alone. Dispatching
  before Jenkins has created the GitHub release fails the asset check instead. A delayed Jenkins
  run does not start npm publish on its own.

- **Nothing at install time checks a code signature.** npm checks the integrity hash the registry
  serves, which covers the bytes that were published and not who made them. Those bytes were
  checked here, at publish, so the check has moved from the machine that installs to the one that
  publishes. Someone who can publish as this workflow, or make the registry serve other bytes, is
  not stopped by the launcher.

- **Under Rosetta the x64 package is the one installed.** npm matches `cpu` against the node that
  runs it, which reports x64 on an arm64 Mac when it is an Intel build, so that machine gets the
  x64 binary and runs it translated.

- **A version with no platform packages installs without a binary.** npm skips an optional
  dependency it cannot find, so an install from a checkout or a git ref at a version that has not
  been published succeeds, and the launcher says the binary is missing.

- **OIDC trusted publishing does not support Jenkins.** That is why this one publication step
  is in GitHub Actions while the signed binaries stay in Jenkins.
