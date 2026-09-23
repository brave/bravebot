BINARY = bravebot
VERSION = $(shell sed -nE 's/^version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' Cargo.toml | head -n 1)
TAG = v$(VERSION)
# Which remote a release is tagged against. `origin` is right for a clone of this repository
# and wrong for a clone of a fork of it, where it names the fork: a tag pushed there is a tag
# the releases page never sees, and it cannot be moved afterwards because the tag ruleset
# refuses an update. Set it per clone rather than per release, with
# `git config bravebot.releaseRemote upstream`.
RELEASE_REMOTE = $(or $(shell git config bravebot.releaseRemote),origin)
# Each of the three images below names a digest as well as a tag, because a tag is whatever its
# publisher points at today and every one of them runs with the whole tree inside it.
# `make check-security` fails on one that names a tag alone. Moving to a newer image is a
# deliberate edit: read the digest with
#   docker buildx imagetools inspect rust:slim --format '{{.Manifest.Digest}}'
# and rewrite the tag beside it in the same line, so the line still says what it runs.
#
# The minimum toolchain CI builds against, declared once in Cargo.toml, and the image that ships
# it. Docker goes by the digest and ignores the tag beside it, so check-msrv refuses to run while
# the two disagree rather than building against a minimum nobody declared.
MSRV = $(shell sed -nE 's/^rust-version[[:space:]]*=[[:space:]]*"([0-9.]+)".*/\1/p' Cargo.toml | head -n 1)
MSRV_IMAGE = rust:1.88-slim@sha256:38bc5a86d998772d4aec2348656ed21438d20fcdce2795b56ca434cf21430d89
# The stable the check targets run, and the image the cross-build is built on, which `make strip`
# runs a second time over the finished assets.
STABLE_IMAGE = rust:1.98-slim@sha256:f47a8de237dcbb0b0ce1099901e60a89728e3d51f24e664b40e947171538ade7
ZIGBUILD_IMAGE = ghcr.io/rust-cross/cargo-zigbuild:0.23.0@sha256:b8364c2c60cdcc9b95c402d17654bff517410926a35678bd89dd924b8158d6ae
# Every file that states the version, which is what a bump rewrites and commits. The two under
# ui/ are the desktop application's: it is packaged from its own manifest, so a version left
# behind there is an app bundle naming a release that does not exist.
VERSION_FILES = Cargo.toml Cargo.lock package.json package-lock.json \
                ui/package.json ui/package-lock.json

# Forwarded into the cross-build container, which does not inherit the host environment.
BUILD_ENV = SERVICES_KEY_AICHAT BRAVE_SERVICES_KEY_ID BRAVE_AI_CHAT_ENDPOINT \
            BRAVE_AI_CHAT_PREMIUM_ENDPOINT BRAVE_AI_CHAT_DEFAULT_MODEL \
            BRAVEBOT_ALLOW_UNCONFIGURED_BUILD

.PHONY: help
help:
	@echo "bravebot $(VERSION)"
	@echo
	@echo "Development:"
	@echo "  make init                  Configure bravebot, Claude Code, Codex and Cursor; install hooks"
	@echo "  make hooks                 Point git at the checked-in git hooks"
	@echo "  make build                 Debug build"
	@echo "  make test                  Run all tests"
	@echo "  make check                 Format check, clippy, tests, and toolchain age"
	@echo "  make check-all-local       All checks except Docker platform checks"
	@echo "  make check-spec            Check docs/specs against the implementation"
	@echo "  make check-security        The security audit's deterministic half"
	@echo "  make check-locales         Hold the catalogs to contrib/untranslated-messages.txt"
	@echo "  make check-versions        Whether every file stating the version states the same one"
	@echo "  make check-docs            Build the documentation website under docs/website"
	@echo "  make docs-changes          What has landed in the specs since the site was updated"
	@echo "  make docs-updated-to-sha   The commit the documentation site is current as of"
	@echo "  make write-unverified      Write agents/unverified-clauses.txt, which check-spec holds it to"
	@echo "  make write-untranslated    Write contrib/untranslated-messages.txt, which check-locales holds it to"
	@echo "  make check-reviewdog       The PR security scan, on this branch's changes"
	@echo "  make check-reviewdog-full  The same scan, over the whole tree"
	@echo "  make check-npm             Install from the lockfile and lint it, as CI does"
	@echo "  make check-deps            Advisories, licences, duplicate versions, and sources"
	@echo "  make check-msrv            Build against the declared minimum toolchain ($(MSRV))"
	@echo "  make check-windows         Lint the Windows target that ships, cross-compiled"
	@echo "  make check-ui              Build the desktop UI, run the tests pinning what it marks, drive it"
	@echo "  make check-all             All local checks, including Linux, UI and the security scan"
	@echo "  make locales               What each translation has, and what it is missing"
	@echo "  make check-linux           The same checks on Linux, current stable toolchain"
	@echo "  make fmt                   Apply formatting"
	@echo "  make aws-logout            End every cached AWS SSO session, to test signing in again"
	@echo
	@echo "Reproducible cross-builds (requires Docker):"
	@echo "  make all-platforms  Every target below"
	@echo "  make darwin-arm64   macOS Apple silicon"
	@echo "  make darwin-amd64   macOS Intel"
	@echo "  make linux-amd64    Linux x86_64"
	@echo "  make linux-arm64    Linux aarch64"
	@echo "  make windows-amd64  Windows x86_64"
	@echo "  make windows-arm64  Windows aarch64"
	@echo
	@echo "Releasing:"
	@echo "  make bump-version BUMP=bugfix|minor|major   Set the next version"
	@echo "  make github-release                         Tag it; Jenkins and npm publish are later"
	@echo "  make app-bundle                             Package the desktop application, release build"
	@echo
	@echo "  make clean          Remove build output"

# agents/ is the checked-in source of truth for skills and AGENTS.md, and no tool reads
# it: each tool looks under its own discovery paths. This creates the symlinks that
# bridge them. The links are gitignored, so a fresh clone needs it once, and it is
# idempotent, so re-running costs nothing.
.PHONY: init
init: hooks
	python3 agents/setup.py link

# .git/hooks is not versioned, so a fresh clone commits with nothing checking it
# until this runs. Idempotent, and part of `init` so nobody has to know it exists.
.PHONY: hooks
hooks:
	git config core.hooksPath .githooks

.PHONY: build
build:
	cargo build

.PHONY: test
test:
	cargo test --all

.PHONY: fmt
fmt:
	cargo fmt --all

# The counterpart to the sign-in a Bedrock turn does for itself, for testing that path deliberately
# rather than waiting for a token to expire.
#
# Every profile, because that is the only thing the CLI offers: `aws sso logout` removes every cached
# token and takes no option to narrow it, so `--profile` would scope nothing while reading as though
# it had. One token serves every profile sharing an sso_session, and other tools reading the same
# cache, Claude Code among them, need a fresh `aws sso login` after this.
.PHONY: aws-logout
aws-logout:
	@echo "ending every cached AWS SSO session; sign in again with: aws sso login --profile <name>"
	aws sso logout

# Host formatting, Clippy, tests and toolchain age. Platform and UI checks are separate.
# Report an old toolchain after the test results, since CI may use newer lints.
.PHONY: check
RUST_TEST_THREADS ?= 4
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	RUST_TEST_THREADS="$(RUST_TEST_THREADS)" cargo test --all --locked
	@python3 contrib/check-versions.py
	@python3 contrib/check-toolchain.py

# Whether every file that states the version states the same one: the workspace manifest, the
# published wrapper and its lockfile, and the desktop application and its lockfile. The tagging
# path refuses a disagreement too, but that is release day, and the front end sat at 0.1.0
# against a 0.9.0 workspace from the day it was folded in with nothing to say so. Run by `check`
# and by CI, so the pull request that causes one is where it is reported.
.PHONY: check-versions
check-versions:
	@python3 contrib/check-versions.py --selftest
	@python3 contrib/check-versions.py

# Whether clippy here knows the lints CI will fail on. Run by `check`; on its own it costs
# nothing and answers immediately.
.PHONY: check-toolchain
check-toolchain:
	@python3 contrib/check-toolchain.py

# The mechanical half of the spec check: clause numbering, the tests each clause names,
# the paths it governs, the symbols it guards, and the table in the specs README. No model
# is involved, so this is deterministic and belongs in CI. Whether the code actually does
# what a clause says needs a reading of the governed source: run the check-spec skill for
# that half.
#
# The screenshot renderer rides along because it is the other half's tool: the skill pastes what
# it prints into issue bodies, it is standard library Python like everything else here, and a
# renderer that is quietly wrong sends a plausible and untrue screen to whoever has to fix the bug.
.PHONY: check-spec
check-spec:
	python3 agents/skills/check-spec/selftest.py
	python3 agents/skills/check-spec/check-spec.py --mechanical-only
	@python3 contrib/terminal-screenshot.py --selftest

# The deterministic half of the security audit. It answers the questions check-spec cannot: whether
# two documents agree about how many exceptions to the rule are admitted, whether the register
# admitting them leaves as many prompts out of a verdict's reach as the clause deciding that does,
# whether a field documented as read in one place is read in one place, whether anything reaches
# into a Labelled, whether a spec pins the constructors as well as the releases, whether every
# workflow step is on a commit rather than a tag somebody else can move, whether every container
# image this tree runs names a digest rather than a tag its publisher can move, and whether a job
# holding `id-token: write` or a secret installs or runs an npm dependency, which every step in that
# job could read the credential from, and whether a checkout of this tree names a kind of ref
# rather than a bare name a branch and a tag can share. No model takes part, so it belongs in CI.
# The lanes that read code are the skill, and a person runs those.
#
# It passes on this tree now that `Labelled::trusted` has a `guards` entry beside `Labelled::new`,
# so ci.yml runs it and check-all has it below. It was held out of both while it failed, because a
# red check-all is the normal state nobody reads.
.PHONY: check-security
check-security:
	python3 agents/skills/security-audit/selftest.py
	python3 agents/skills/security-audit/security-audit.py --mechanical-only

# agents/unverified-clauses.txt, written from the specs. It is the list of clauses nothing pins, and
# check-spec fails while it and the specs disagree, so this is what to run after giving a clause
# a test, or setting one to none.
.PHONY: write-unverified
write-unverified:
	python3 agents/skills/check-spec/check-spec.py --write-unverified

# contrib/untranslated-messages.txt, written from the catalogs. It is the list of messages each translation
# is missing, and check-locales fails while it and the catalogs disagree, so this is what to run
# after translating a message, or after adding one to the reference that no catalog has yet.
.PHONY: write-untranslated
write-untranslated:
	python3 contrib/check-locales.py --write

# The security scan that comments on our pull requests, before pushing rather than
# after. Nothing here configures it: it arrives as an organization-level workflow
# calling brave/security-action, so contrib/check-reviewdog.sh clones that repository
# and drives its reviewdog runners against this checkout, pinning the tool versions
# its action.yml pins. First run downloads opengrep, reviewdog and the rule set into
# ~/.cache; later runs re-use them and take about half a minute.
#
# The two targets are the action's own two modes. On a pull request it scans what the
# branch changed; on workflow_dispatch it scans everything. A finding here is one the
# bot would post, so the full scan reports plenty that predates any given branch --
# check-reviewdog is the one to run before pushing.
#
# No model is involved, so both are deterministic.
.PHONY: check-reviewdog
check-reviewdog: check-reviewdog-selftest
	@contrib/check-reviewdog.sh

.PHONY: check-reviewdog-full
check-reviewdog-full: check-reviewdog-selftest
	@contrib/check-reviewdog.sh --full

# The npm-lockfile job. The published package is a thin wrapper that downloads the
# release binary, so the lockfile is the whole supply chain surface it has. The front end
# under ui/ has a lockfile of its own, holding Electron's tree, and it gets the same lint:
# it is not an npm workspace of this package deliberately, so nothing else reaches it.
.PHONY: check-npm
check-npm:
	npm ci --ignore-scripts
	npm run lint:lockfile
	npm run lint:lockfile:website
	npm run lint:lockfile:ui

# The dependency policy in deny.toml. CI runs this target rather than cargo-deny's action,
# so the version below is the only one anywhere and a pass here means what it means there.
#
# Installed under the cache rather than into ~/.cargo/bin, so running this never changes what
# `cargo deny` means anywhere else.
#
# unmatched-skip is a warning by default: raised here because a skip entry that no longer
# matches is a recorded reason for a duplicate that is no longer there. --locked for the same
# reason every other cargo command here takes it: the answer is about the versions Cargo.lock
# pins, not the ones a resolve on the spot would pick.
CARGO_DENY_VERSION = 0.20.2
CARGO_DENY_ROOT = $(HOME)/.cache/bravebot-deny
.PHONY: check-deps
check-deps:
	@"$(CARGO_DENY_ROOT)/bin/cargo-deny" --version 2>/dev/null | grep -qx "cargo-deny $(CARGO_DENY_VERSION)" || { \
		echo "building cargo-deny $(CARGO_DENY_VERSION) into $(CARGO_DENY_ROOT), which takes a few minutes"; \
		cargo install --quiet --locked cargo-deny@$(CARGO_DENY_VERSION) --root "$(CARGO_DENY_ROOT)"; \
	}
	"$(CARGO_DENY_ROOT)/bin/cargo-deny" --locked check --deny unmatched-skip

# The minimum-toolchain job. Built in a container pinned to the declared MSRV, because
# rustup is not a given here and a Homebrew or distro Rust cannot switch toolchains.
# Catches a feature that only compiles on a newer toolchain than the release build has.
.PHONY: check-msrv
# The source archive contains working-tree edits but excludes ignored local state.
# pipefail keeps a failed archive from becoming a successful container check.
check-msrv check-windows check-linux: SHELL := /bin/bash -o pipefail
check-msrv:
	@case "$(MSRV_IMAGE)" in rust:$(MSRV)-slim@sha256:*) ;; \
		*) echo "MSRV is $(MSRV) and MSRV_IMAGE is $(MSRV_IMAGE): bump both"; exit 1;; esac
	python3 contrib/check-source.py | docker run --rm -i -e BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1 \
		-w /work $(MSRV_IMAGE) sh -c '\
		tar -xf - && \
		cargo build --all --locked'

# The windows job. Nothing else compiles the `#[cfg(windows)]` arms of this tree for a check:
# the cross-build produces the shipped binary and lints nothing, and never compiles a test target
# at all. Cross-compiled rather than run on a Windows host, which clippy allows because it stops
# before linking, and in a container for the reason check-msrv is in one: rustup is not a given
# here, and the C compiler ring wants for the target is a package rather than a toolchain
# component.
.PHONY: check-windows
check-windows:
	python3 contrib/check-source.py | docker run --rm -i -e BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1 \
		-w /work $(STABLE_IMAGE) sh -c '\
		tar -xf - && \
		apt-get update >/dev/null && \
		apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 >/dev/null && \
		rustup component add clippy >/dev/null && \
		rustup target add x86_64-pc-windows-gnu >/dev/null && \
		cargo clippy --target x86_64-pc-windows-gnu --all-targets --all-features --locked \
			-- -D warnings'

# docusaurus.config.js throws on a broken link or anchor, so building the site is the whole
# of its correctness check. --ignore-scripts because the site's dependency tree is a
# thousand packages deep and none of them needs an install hook to build.
.PHONY: check-docs
check-docs:
	npm --prefix docs/website ci --ignore-scripts
	npm --prefix docs/website run build

# How far the site has fallen behind the specs it describes. docs/docs-updated-to-sha records how
# far reading got; these two report against it. Neither is a gate and no CI job runs them:
# the site going stale is not something a diff can decide, so what they are for is answering
# the question without spending a model run. The update-docs skill calls the same script to
# move the baseline or defer a commit, which is why neither of those is a target here.
.PHONY: docs-changes
docs-changes:
	@python3 agents/skills/update-docs/docs-ref.py changes

.PHONY: docs-updated-to-sha
docs-updated-to-sha:
	@python3 agents/skills/update-docs/docs-ref.py show

# All local checks before pushing, including the UI and Linux code paths.
# Requires Docker and the desktop runtime dependencies described in checks.md.
.PHONY: check-all-local check-all
check-all-local: check-scripts check check-spec check-security check-locales check-versions check-docs check-npm check-deps check-ui check-reviewdog
check-all: check-all-local check-msrv check-windows check-linux

.PHONY: check-scripts check-all-selftest check-reviewdog-selftest
check-scripts: check-all-selftest check-reviewdog-selftest

check-all-selftest:
	python3 contrib/check-all-selftest.py

check-reviewdog-selftest:
	python3 contrib/check-reviewdog-selftest.py

# CI and local runs share the same desktop checks. Linux uses a virtual display;
# macOS uses the logged-in desktop session.
#
# The Node tests this runs are the only thing pinning what the desktop renderer owes the layering
# spec: that released content is marked by a container it cannot forge, reaches no raw markup and
# makes the app fetch nothing, and that a replayed message is drawn from the record rather than
# from its own words. Both are properties of a surface this workspace does not compile, so no Rust
# test can cite them and `make check` never ran them. `ls` precedes the run because `node --test`
# given a pattern matching nothing exits 0 having run nothing, which would make a renamed-away pin
# a passing gate. The helper six of them spawn for real is built by the bridge script that
# `npm run build` calls, so it needs no step of its own here.
.PHONY: check-ui check-ui-build check-ui-walkthrough
check-ui: check-ui-build
	$(MAKE) check-ui-walkthrough

check-ui-build:
	npm --prefix ui ci
	npm --prefix ui run typecheck
	BRAVEBOT_BUILD_UNCONFIGURED=1 npm --prefix ui run build
	cd ui && ls scripts/*.test.mjs >/dev/null && node --test scripts/*.test.mjs

check-ui-walkthrough:
	cd ui && if [ "$$(uname -s)" = Linux ]; then \
		xvfb-run -a node scripts/drive-manual-walkthrough.mjs; \
	else \
		node scripts/drive-manual-walkthrough.mjs; \
	fi

# What each catalog has of the reference, and what it is missing. The build says so too, in a
# warning, but a warning is only printed when the build script actually runs, so a translator
# working through a file learns nothing from a cached build. This always answers, and no gap it
# finds makes it fail: it is the report to read while translating, and check-locales is the gate.
.PHONY: locales
locales:
	@python3 contrib/check-locales.py --report

# Whether every catalog matches contrib/untranslated-messages.txt, which records the messages each
# translation is knowingly missing. A gap is allowed and silence about one is not: falling back to
# English is deliberate, so what this gates on is a gap nobody wrote down, and a recorded gap that
# is no longer there. No toolchain and no build, so CI answers in seconds.
.PHONY: check-locales
check-locales:
	python3 contrib/check-locales.py --selftest
	python3 contrib/check-locales.py

# Runs the same checks on Linux with the stable toolchain its image is pinned to. Worth doing
# before pushing platform-specific code: a macOS host never compiles the Linux backend, and
# clippy gains lints between releases, so both can fail in CI while passing locally. The image
# names a digest, so what it runs stays where it is put while stable moves on: when
# `make check-toolchain` reports the host is behind, the tag and digest below are what to bump.
# The environment the container needs: the build script refuses an unconfigured build
# without the first, which ci.yml sets for CI, and the shell-mode test reads `$$USER` the way
# a terminal would, which a CI runner image provides and a bare container does not. The third
# is set here and must not be set in CI: Docker Desktop's kernel implements no Landlock at
# all, so the sandbox tests would fail rather than skip, while on a CI runner that same
# failure is the report that the Linux half of confinement went unexercised.
#
# Keep test parallelism bounded in Docker, as it is for the host checks.
# Python runs the redirected-process fixture in the turn tests.
.PHONY: check-linux
check-linux:
	python3 contrib/check-source.py | docker run --rm -i -e BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1 -e USER=root \
		-e BRAVEBOT_ALLOW_MISSING_LANDLOCK=1 \
		-w /work $(STABLE_IMAGE) sh -c '\
		tar -xf - && \
		apt-get update >/dev/null && \
		apt-get install -y --no-install-recommends python3 >/dev/null && \
		rustup component add clippy rustfmt >/dev/null 2>&1 && \
		cargo fmt --all -- --check && \
		cargo clippy --all-targets --all-features -- -D warnings && \
		cargo test --all -- --test-threads=4'

.PHONY: darwin-arm64
darwin-arm64:
	$(call cross-build,$@,aarch64-apple-darwin)

.PHONY: darwin-amd64
darwin-amd64:
	$(call cross-build,$@,x86_64-apple-darwin)

.PHONY: linux-amd64
linux-amd64:
	$(call cross-build,$@,x86_64-unknown-linux-gnu)

.PHONY: linux-arm64
linux-arm64:
	$(call cross-build,$@,aarch64-unknown-linux-gnu)

.PHONY: windows-amd64
windows-amd64:
	$(call cross-build,$@,x86_64-pc-windows-gnu)

.PHONY: windows-arm64
windows-arm64:
	$(call cross-build,$@,aarch64-pc-windows-gnullvm)

.PHONY: all-platforms
all-platforms: darwin-arm64 darwin-amd64 linux-amd64 linux-arm64 windows-amd64 windows-arm64
	@echo
	@echo "built:"
	@ls -1 dist/

# The desktop application as something somebody else can run: the third family of artifact a
# release produces, beside the signed binaries above and the npm package.
#
# The Rust half is built here rather than through ui/scripts/build-bridge.sh, for the two things
# that script does which a release must not. It builds the debug profile, so a bundle packaged
# after it carries an unoptimised agent; and it defaults BRAVEBOT_ALLOW_UNCONFIGURED_BUILD to 1,
# so a shell with no credentials in it yields a bundle that starts, lists sessions and then fails
# at the first inference request, which is the binary crates/config/build.rs exists to refuse.
# Setting it to 0 fails that build here instead, while whoever is packaging is still watching. It
# is set to 0 rather than left unset because the permission is granted by that exact value: a
# recipe that only declined to set it would still grant it in a shell that exports it, which the
# front-end script and every CI job here do.
# Credentials come from the environment, as they do for every other release build: direnv at the
# root of this repository, or a release job's own secrets.
#
# `npm ci`, never `install`: the lockfile is what CI lints and a resolve on the spot is a
# dependency change. Its install hooks run, and have to: the packager copies the Electron runtime
# into the bundle, and that runtime is what the hook fetches. electron-vite is then run directly
# rather than through `npm run build`, which would build the debug pair again on the way past.
#
# Nothing here signs or notarises the result, so the bundle is installable and not distributable;
# the job that signs the binaries is where that belongs, and issue #394 is where it is decided.
.PHONY: app-bundle
app-bundle:
	BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=0 cargo build --release --locked \
		-p bravebot-ui-bridge -p bravebot-ui-files
	cd ui && npm ci && npm run typecheck && npm exec -- electron-vite build && \
		node scripts/package.mjs --release

# Symbols are kept during the build because Rust's own strip can corrupt some targets
# under zigbuild, so they are removed here instead.
#
# rust-objcopy is LLVM-based and handles Mach-O, ELF, and PE alike, so one tool covers
# every target; the per-target GNU strip binaries are not all present in the image.
#
# The container is asked where its own toolchain is rather than being told: the image is
# published for two architectures and each variant names that directory after its own triple,
# so a written-out path resolves on one release host and not the other, and states the Rust
# version besides. That directory is also LD_LIBRARY_PATH, because rust-objcopy loads the
# libLLVM beside it.
#
# Run by the publish job after `all-platforms`, and by nothing in this repository, so the
# release carries stripped binaries while a local cross-build keeps its symbols.
.PHONY: strip
strip:
	@for f in dist/$(BINARY)-*; do \
		case "$$f" in *.sha256|*SHA256SUMS) continue;; esac; \
		docker run --rm -v "$(PWD)/dist:/dist" -e ASSET="/dist/$$(basename $$f)" \
			$(ZIGBUILD_IMAGE) sh -c '\
			lib=$$(rustc --print sysroot)/lib && \
			host=$$(rustc -vV | sed -n "s/^host: //p") && \
			LD_LIBRARY_PATH=$$lib \
			"$$lib/rustlib/$$host/bin/rust-objcopy" --strip-all "$$ASSET"'; \
	done
	@echo "stripped:"
	@ls -lh dist/ | awk 'NR>1 {print "  " $$9, $$5}'

# Nothing in this repository runs this, and the publish job hashes the signed binaries itself.
# It is the statement of the format those files have to be in: the digest alone, with no filename
# beside it, which is the only thing the npm postinstall accepts. SHA256SUMS is the conventional
# form of the same hashes, for verifying a download by hand.
.PHONY: checksums
checksums:
	@cd dist && rm -f ./*.sha256 SHA256SUMS && \
	for f in $(BINARY)-*; do \
		shasum -a 256 "$$f" | awk '{print $$1}' > "$$f.sha256"; \
		shasum -a 256 "$$f" >> SHA256SUMS; \
	done
	@echo "wrote dist/SHA256SUMS"

# Edits the six files that state the version and commits exactly those, so no lockfile is
# left behind still naming the old one. It stops there: nothing is pushed and nothing is
# tagged, and the commit is still reviewed before `github-release` will tag it.
#
# The four JSON files are written by contrib/check-versions.py, which is also what reads them, so
# the bump and the check cannot disagree about where a version is stated. It replaced a node
# program embedded in this recipe that could not run at all: a recipe's line continuations are
# literal backslashes inside the single quotes holding the program, so node was handed a source
# beginning with one and refused it. The npm install after it re-derives the wrapper's lockfile
# from the manifest, as it did before. Nothing runs npm under ui/: an install there re-resolves
# Electron's tree against the registry, which is a dependency change and not a version stamp.
#
# check-versions then runs as a check, so a bump that missed a file fails here rather than at the
# tag.
.PHONY: bump-version
bump-version:
	@if [ "$(BUMP)" != "bugfix" ] && [ "$(BUMP)" != "minor" ] && [ "$(BUMP)" != "major" ]; then \
		echo "error: BUMP must be one of: bugfix, minor, major"; \
		exit 1; \
	fi
	@set -eu; \
	current="$(VERSION)"; \
	if [ -z "$$current" ]; then \
		echo "error: unable to read version from Cargo.toml"; \
		exit 1; \
	fi; \
	if ! git diff --quiet -- $(VERSION_FILES) || ! git diff --cached --quiet -- $(VERSION_FILES); then \
		echo "error: $(VERSION_FILES) already modified; commit or stash that first"; \
		exit 1; \
	fi; \
	major="$${current%%.*}"; rest="$${current#*.}"; \
	minor="$${rest%%.*}"; patch="$${rest#*.}"; \
	case "$(BUMP)" in \
		bugfix) patch="$$((patch + 1))" ;; \
		minor) minor="$$((minor + 1))"; patch=0 ;; \
		major) major="$$((major + 1))"; minor=0; patch=0 ;; \
	esac; \
	next="$$major.$$minor.$$patch"; \
	awk -v v="$$next" ' \
		BEGIN { in_pkg = 0; done = 0 } \
		/^\[/ { in_pkg = ($$0 == "[workspace.package]") } \
		in_pkg && !done && /^version[[:space:]]*=/ { \
			print "version = \"" v "\""; done = 1; next \
		} \
		{ print }' Cargo.toml > Cargo.toml.tmp; \
	mv Cargo.toml.tmp Cargo.toml; \
	if [ "$$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' Cargo.toml | head -n 1)" != "$$next" ]; then \
		echo "error: Cargo.toml version was not rewritten"; \
		exit 1; \
	fi; \
	cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null; \
	python3 contrib/check-versions.py --set "$$next"; \
	BRAVEBOT_INSTALL_SKIP_DOWNLOAD=1 npm install --package-lock-only --ignore-scripts >/dev/null; \
	python3 contrib/check-versions.py; \
	git commit -q -m "Bump version to $$next" -- $(VERSION_FILES); \
	echo "committed: bumped $$current -> $$next ($(VERSION_FILES))"; \
	echo "review it, land it on main, then run: make github-release"

# Tags the current version and pushes it. GitHub Actions runs CI on the tag.
# Signed assets and the npm package are published later, each by hand: Jenkins
# (bravebot-build with UPLOAD and RELEASE), then Actions → Publish npm.
#
# A refused push takes the tag with it. A tag left behind locally is one the
# remote never got, and the next run reports the version as already tagged
# while the releases page shows nothing.
.PHONY: github-release
github-release:
	@set -eu; \
	if [ -z "$(VERSION)" ]; then \
		echo "error: unable to read version from Cargo.toml"; \
		exit 1; \
	fi; \
	if ! python3 contrib/check-versions.py; then \
		echo "error: the files that state the version disagree; run make bump-version"; \
		exit 1; \
	fi; \
	if ! git diff --quiet || ! git diff --cached --quiet; then \
		echo "error: working tree must be clean before tagging"; \
		exit 1; \
	fi; \
	branch="$$(git rev-parse --abbrev-ref HEAD)"; \
	if [ "$$branch" != "main" ]; then \
		echo "error: releases are tagged from main, not $$branch"; \
		exit 1; \
	fi; \
	if ! git remote get-url "$(RELEASE_REMOTE)" >/dev/null 2>&1; then \
		echo "error: no remote named $(RELEASE_REMOTE); set bravebot.releaseRemote to one this clone has"; \
		exit 1; \
	fi; \
	git fetch --quiet "$(RELEASE_REMOTE)" main; \
	if [ "$$(git rev-parse HEAD)" != "$$(git rev-parse $(RELEASE_REMOTE)/main)" ]; then \
		echo "error: HEAD differs from $(RELEASE_REMOTE)/main; push or pull first"; \
		exit 1; \
	fi; \
	if git rev-parse -q --verify "refs/tags/$(TAG)" >/dev/null; then \
		echo "error: tag $(TAG) already exists"; \
		exit 1; \
	fi; \
	git tag -a -m "bravebot $(TAG)" "$(TAG)"; \
	if ! git push "$(RELEASE_REMOTE)" "$(TAG)"; then \
		git tag -d "$(TAG)"; \
		echo "error: push failed; removed the local $(TAG) so this can be run again"; \
		exit 1; \
	fi; \
	echo "pushed $(TAG) to $(RELEASE_REMOTE); GitHub Actions will run CI on the tag"; \
	echo "publish signed assets later with Jenkins job bravebot-build (UPLOAD and RELEASE)"; \
	echo "then publish npm: gh workflow run publish-npm.yml --ref $(TAG) -f tag=$(TAG)"; \
	echo "watch CI with: gh run watch --repo brave/bravebot"

.PHONY: clean
clean:
	cargo clean
	rm -rf dist
	rm -rf docs/website/build docs/website/.docusaurus

# Configuration reaches the build as a BuildKit secret rather than a build argument,
# which would record the signing key in the image metadata. The temporary file is
# mode 600 and removed even if the build fails.
define cross-build
	set -e; \
	env_file="$$(mktemp)"; trap 'rm -f "$$env_file"' EXIT INT TERM; \
	for name in $(BUILD_ENV); do \
		eval "value=\$$$$name"; \
		if [ -n "$$value" ]; then printf 'export %s=%s\n' "$$name" "$$value" >> "$$env_file"; fi; \
	done; \
	DOCKER_BUILDKIT=1 docker build -f Dockerfile.cross -t $(BINARY)-$(1) \
		--build-arg TARGET=$(2) \
		--secret id=bravebot_env,src="$$env_file" .
	$(call extract,$(BINARY)-$(1),$(1))
endef

# `docker create` on a scratch image needs a command argument even though it never
# runs; the container exists only so the binary can be copied out.
define extract
	mkdir -p dist
	docker rm -f tmp-$(BINARY)-$(2) 2>/dev/null || true
	docker create --name tmp-$(BINARY)-$(2) $(1) /dev/null
	docker cp tmp-$(BINARY)-$(2):/$(BINARY) dist/$(call artifact,$(2))
	docker rm tmp-$(BINARY)-$(2)
endef

define artifact
$(BINARY)-$(1)$(if $(findstring windows,$(1)),.exe,)
endef
