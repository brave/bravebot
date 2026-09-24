# Checks

## Choosing checks

Before changing behaviour or test assertions, use the repository's
[testing-preflight skill](../../agents/skills/testing-preflight/SKILL.md). Identify which plausible
mistake each changed test must catch, then choose checks from this page and the affected CI jobs.
Passing a large test suite does not show that a test can detect the regression it claims to cover.

Run `make check-spec` before committing changes to spec clauses, their referenced tests, guarded
symbols or call sites, or `agents/unverified-clauses.txt`. This catches stale verification metadata while
the change is still local. The broader before-push requirements below still apply.

## Before a commit

**fmt and clippy.** Both take seconds and have no exemption for a change that only touched a
comment, a document, or a name: they fail on those as readily as on anything else, and `make init`
installs a pre-commit hook that refuses a commit failing either.

Then run the tests the diff is under, scoped as tightly as the diff is: `cargo test -p bravebot-tui
--lib` for the interface, `cargo test -p bravebot-agent --test workspace` for one test binary. A
commit still has to be a state that stands up, and a change nothing covers is a change to be
suspicious of.

## Before pushing

Choose how much to run:

| Command | Coverage |
|---|---|
| `make check-all-local` | Script selftests, host formatting, Clippy and Rust tests, specs, security rules, locales, versions, toolchain age, docs, npm lockfiles, dependency policy, desktop UI and reviewdog. No Docker. |
| `make check-all` | Everything in `check-all-local`, plus Docker checks for minimum Rust, Windows Clippy and Linux. |

Before a PR, run the checks relevant to your change and state which command passed.
If you use `check-all-local`, leave the platform checks to CI and say they were not run
locally. You can also run an individual target, such as `make check-reviewdog`, or check
formatting with `cargo fmt --all -- --check`.

If the checkout has no backend credentials, use
`BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1 make check-all-local` (or `check-all`) to allow
the Rust build.

`make check-all` requires Docker for the platform checks. A missing prerequisite fails
the target; it does not count as a pass. Add `-k`, for example `make -k check-all-local`,
to continue independent checks when one fails.

For the full run on macOS, start Docker Desktop and run `make check-all` from the checkout
you plan to submit. The Linux checks run inside Docker, and Windows Clippy cross-compiles there;
neither needs a separate Linux or Windows machine. Host Rust tests use four threads by
default; override this with `RUST_TEST_THREADS=8 make check-all` if needed.
The containers receive current source, including local edits and untracked files,
but exclude ignored files, build output and nested checkouts. They use Docker's native
architecture so Apple Silicon does not need to emulate an x86 Linux CPU.

`make check-ui` installs the desktop dependencies, typechecks and builds the app,
runs its Node tests, and drives the Electron walkthrough. Those Node tests are the only thing
pinning what the desktop renderer owes the layering spec: that released content is marked by a
container it cannot forge, reaches no raw markup and makes the app fetch nothing, and that a
replayed message is drawn from the record rather than from its own words. Both are properties of a
surface this workspace does not compile, so no Rust test can observe them and `make check` never
did. Run this for a change under `ui/`. On macOS it needs a logged-in
desktop session. On Linux it needs `xvfb-run` and Electron's runtime libraries; CI
installs `xvfb`, `libgtk-3-0`, `libnss3` and `libasound2t64`. CI uses the same build
and walkthrough targets, with a separate timeout for the walkthrough.
The check build leaves out backend credentials, even when your development build has
them, so the walkthrough can test the unconfigured app without using your account.
Run `npm --prefix ui run build` afterwards to restore a configured development build.

`make check-scripts` checks that every required gate runs, that UI and lockfile failures
reach the caller, and that scanner failures cannot pass as empty scans. It runs
`check-all-selftest` and `check-reviewdog-selftest`, which can also run separately.
To also exercise the installed reviewdog binary, set `REVIEWDOG_TEST_BINARY` to its
absolute path when running the target. Without it, that integration test is reported as skipped.

These are local checks, not a promise that every CI environment passes. Windows Clippy
cross-compiles without running Windows tests. Docker Desktop cannot exercise Landlock;
CI's native Linux tests cover that gap. Release cross-builds remain separate.

`make check` runs the whole suite and takes minutes. It is what to run before pushing a branch and
what CI runs, not what to run between two edits to the same file, and never twice to confirm the
same thing. Reaching for it out of caution is not free: it is the difference between a review that
takes a minute and one that takes twenty, and the reviewer is the person waiting.

`make check-spec` checks the mechanical half of the specs: clause numbering, the tests each clause
names, the paths it governs, the call sites a guarded symbol pins, and the table in
[../specs/README.md](../specs/README.md). CI runs it too, so a new use of a guarded symbol fails a
pull request rather than waiting for somebody to notice it. It also holds
[../../agents/unverified-clauses.txt](../../agents/unverified-clauses.txt) to the clauses that are
`verified-by: none`, so giving a clause a test, or setting one to `none`, is a line in a diff
rather than a warning nobody has to read. `make write-unverified` writes the file; commit what it
writes.

`make check-security` decides the things `check-spec` structurally cannot: whether two documents
agree about how many exceptions to the rule are admitted, whether every crate keeping a network
client of its own is one the egress register admits and no comment claims a dependency a manifest
beside it declares, whether the register admitting them leaves as many prompts out of a verdict's
reach as the clause deciding that does, whether a field documented as read in one place is read in
one place, whether a trait reaches into a `Labelled`, whether a spec pins the constructors of one as
well as the releases, whether every gate handing released content to a closure the driver wrote is
counted rather than merely named and so is every function forwarding its caller's closure to one,
whether every workflow step is on a commit rather than a tag its owner can move, whether every
container image this tree runs
names a digest rather than a tag its publisher can move, and whether a job holding `id-token: write`
or a secret installs or runs an npm dependency, which every step in that job could read the
credential from, whether a checkout of this tree names a kind of ref rather than a bare name a
branch and a tag can share, and whether the check contexts a merge is held to are written down,
name jobs that exist, and cover every job whose purpose is running a check. CI runs it too, so a
pull request that moves a workflow step or a build image onto a movable tag, that puts an install
back beside the publish credential, that points a checkout at a name instead of a ref, that gives a
crate a network client the egress register does not admit, that drops the entry pinning a
constructor, or that adds a check nothing holds a merge to, fails rather than holding only for
whoever remembers to run this. It is the deterministic half of the
[security-audit skill](../../agents/skills/security-audit/SKILL.md); the lanes that read code are the
other half, and a person runs those. Run it before a commit that touches a label,
`.github/workflows`, a Dockerfile, the Makefile's containers, a crate's manifest, the required
check contexts, or the trust specs.

Wherever this page says a check fails a pull request, what does the failing is not the job going
red. A job reports. What holds a merge is branch protection's list of required contexts, which is
a repository setting no file in a checkout can read, so
[../../contrib/required-checks.txt](../../contrib/required-checks.txt) is where the tree states
what that list has to hold. `make check-security` decides the three things about it a checkout
can: that every name in the file is the display name of a job in `.github/workflows/`, that every
job running a check target is named in the file, and that a target this page promises will fail a
pull request is run by some job at all. A required context matches a check run's name exactly, so
a job renamed on one side and not the other leaves a context nothing reports under, which is the
shape of the one this repository already has: the required lower-case `security` is the
organisation's scan, supplied by a workflow that is in no checkout here, and it is a different
check run from CI's `Security`. What is left is comparing the file against what the protection
actually requires, which needs the network and a token; the file carries the two `gh` queries
for it.

`make check-locales` holds every message catalog to
[../../contrib/untranslated-messages.txt](../../contrib/untranslated-messages.txt), the record of what each
translation is missing. A translation is allowed to lag the reference, since what it lacks is shown
in English; what is not allowed is lagging it silently, so the check fails on a gap the file does
not list and on a line for a gap that is no longer there. `make write-untranslated` writes the file;
commit what it writes. CI runs this too, so adding a message to `en-US.ftl` and no other catalog
fails a pull request rather than printing a build warning into a job that passed, which is how
eleven messages came to ship untranslated. `make locales` is the same count as a report, which no
gap makes fail, and is the one to read while translating. Run the check before a commit that touches
`crates/i18n/locales`.

`make check-versions` holds every file that states the version to the workspace manifest: the
published wrapper and its lockfile, and the desktop application and its lockfile, each of which
states it in its own header and, for a lockfile, in the entry for the package it locks. `make
check` runs it and so does CI, because the tagging path's own refusal fires on release day, which
is long after the pull request that moved one file and not the others. It carries a `--selftest`,
which `make check-versions` runs first.

`make check-npm` installs from the lockfile and lints it, as CI does. `make check-deps` decides
`deny.toml`: an advisory against anything in the tree, a licence the binary cannot ship, a crate the
build compiles at two versions without a recorded reason, and a dependency from anywhere but
crates.io. CI runs this same target on every pull request, on main, and once a day, since an
advisory arrives without a commit. `make check-reviewdog` is the [security scan](security-scan.md).

`make check-windows` lints the target that ships to Windows, `x86_64-pc-windows-gnu`, over every
target including the tests. Nothing else compiles the `#[cfg(windows)]` arms of this tree for a
check: the cross-build produces the shipped binary and lints nothing, and compiles no test target
at all, so a Windows arm that does not build or that trips a lint reaches main with CI green. It
cross-compiles, which clippy allows because it stops before linking, so no Windows host is
involved and the suite is not run on one. Worth running for anything with a platform branch in it,
and for any test that has one: a `#[cfg(unix)]` left off a test is invisible from every platform
that has `unix`.

`make check-linux` runs fmt, clippy and the tests on Linux under the stable toolchain its container
is pinned to, which is a digest rather than whatever `rust:slim` resolves to today, so moving it on
is an edit somebody makes when `make check-toolchain` says the host has fallen behind.
Worth doing before pushing platform-specific code, since a macOS host never compiles the Linux
backend. Its one gap is the Landlock tests: the kernel in play is Docker's, and Docker Desktop's
implements no Landlock at all, so that target sets the switch that skips them instead of failing and
they say so as they go. On a Linux host `make check` runs them against the host kernel already. `make check-msrv` builds against the declared minimum toolchain, which the pinned
cross-build container ships.

## Clippy here is not the clippy CI runs

CI installs whatever stable is current on the day it runs, and clippy gains lints with every
release, so a host a few releases behind passes a warning CI fails on, and the first report of it
is a red build on code that was checked before it was pushed. `make check-toolchain` measures that
gap from the release date rustc states, and it runs last in `make check`. When it fires, the answer
is `make check-linux`. Another local `cargo clippy` is not: it is the same weaker lint set a second
time.

## Reading a result

Read what a command exits with rather than a filter over it: `make check | grep error` reports
success on a formatting failure, because a fmt diff says nothing matching that pattern and grep
exited happily.

**A test that fails on the parent commit is not yours to fix.** Establish that once, cheaply, and
move on: name the test, say it reproduces without the change, and carry on with the work. Do not
bisect it, do not build a baseline worktree for it, and do not re-run the suite hoping. Some tests
here spawn real processes against a wall clock, so they fail on a loaded machine and pass on the
next run; that is a flake, not a signal, and chasing one costs more than the failure does.

**Which tests those are is measured rather than assumed.** The weekly
[Test determinism](../../.github/workflows/test-determinism.yml) workflow runs the suite a hundred
times against one build, at four threads and at sixteen, and names every test whose outcome changed,
with the rate and the panic it produced. Each one gets an issue titled `Flaky test: <name>`, and a
test that already has one gets nothing further, so the list of open flakes is the search rather than
a job summary somebody has to remember to read. Search the issues before treating a failure as a
known flake, and run `contrib/measure-flakes.py --runs 30` to ask this machine over a shorter sweep.
Neither fails a build over a rate.

**A test missing from that list is not thereby clean.** A hundred runs can only account for a test
that loses more than about three in a hundred, and the job measures a quiet four core runner, while
the mock server races that `make check-linux` caps threads for want a machine with something else on
it. So the workflow reporting nothing means less than the local command reporting nothing on the
machine that actually failed: every report states the rate it saw, and that bound is what to read
before concluding a test is deterministic.

If a check cannot pass for a reason outside the change, say so in the commit message rather than
leaving it to be discovered.
