---
sidebar_position: 8
title: Development
description: Building from source, the checks CI runs, and how this project is specified.
---

# Development

The source lives at
[brave-experiments/bravebot](https://github.com/brave-experiments/bravebot).

## Building

Requires a recent stable Rust toolchain.

```sh
cargo build
cargo test
make check     # fmt, clippy -D warnings, and tests: what CI enforces
```

`make check-linux` runs the same checks on Linux under the current stable toolchain. Worth doing
before pushing platform-specific code, since a macOS host never compiles the Linux backend and clippy
gains lints between releases.

Reproducible cross-platform binaries are built in a pinned container, so the same artifact comes out
on any host:

```sh
make all-platforms
```

Targets are macOS, Linux and Windows on both x86_64 and arm64.

## Configuration for a source build

Uses [direnv](https://direnv.net/):

```sh
cp .envrc.example .envrc
direnv allow
```

`.envrc` is gitignored and must never be committed, because it holds a signing key.

The build captures whatever is set at build time, so the resulting binary works in any directory
rather than needing direnv wherever it is started. **A build with nothing set fails**, rather than
producing a binary that only works in the tree it came from. To build one deliberately, set
`BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` and supply the variables at run time.

The environment still wins when set, which is how a released binary is pointed at a local backend
without rebuilding it. Baked values are masked so `strings` on the binary does not print them. That
is obfuscation and not encryption, so a binary built with a live key should be treated as holding one.

The cross-build container does not inherit the host environment, so `make all-platforms` forwards
these variables as a BuildKit secret rather than a build argument, which would record the signing key
in the image metadata.

Run `bravebot doctor` to check configuration and confinement without revealing the signing key. See
[Configuration](customize/configuration.md) for the variables themselves.

## Agent configuration in this repo

Run `make init` in a fresh clone to link the checked-in agent configuration into the places tools
read it from.

```sh
make init
```

`agents/` is the source of truth for what an agent reads here: `AGENTS.md` and the skills under
`agents/skills/`. Nothing discovers it there. Claude Code looks under `.claude/`, and bravebot reads
`AGENTS.md` at the workspace root and skills from `.bravebot/skills`. `make init` creates symlinks and
nothing else:

```
.claude/skills/<name>    ->  agents/skills/<name>
.bravebot/skills/<name>  ->  agents/skills/<name>
.claude/CLAUDE.md        ->  agents/AGENTS.md
AGENTS.md                ->  agents/AGENTS.md
```

The links are gitignored, so a skill is written once rather than copied once per tool. Re-running is
idempotent and silent, a stale link is refreshed, and a real file somebody put in a discovery
directory by hand is left alone. `python3 agents/setup.py list` shows the current state, and `unlink`
removes only the links it owns.

`make init` does **not** grant trust. Expect to be asked about `.bravebot/skills` the first time you
start bravebot in this tree. bravebot loads a workspace skill only from a path a person vouched for,
and a script granting that on your behalf is exactly the inference that rule forbids.

## Which build wrote a session

Every session record carries the build that produced it, and `bravebot --version` prints the same
string:

```
bravebot 0.4.0 (f2a6e1a, modified)
```

Both matter when reading a transcript back: a session that behaved oddly is usually being read against
code that has moved since. Resuming a session recorded by a different build says so.

The stamp watches every crate's sources rather than only its own, so `modified` cannot go stale while
another crate changes underneath it.

## Testing the interface

`cargo test` covers the interface a piece at a time: a key press becomes an action, an action is
handled, a screen is drawn. It cannot reach the wiring between those pieces, which is where the
interface bugs have been. `contrib/drive_tui.py` runs a scripted session against a real terminal so
those paths can be exercised, and `contrib/README.md` says how. It needs a backend and writes real
sessions, so it is a tool to reach for deliberately rather than part of `make check`.

## Spec-enforced development

This project is developed against the mini-specs in
[docs/specs](https://github.com/brave-experiments/bravebot/tree/main/docs/specs), which are the
source of truth for how it behaves. Each clause carries the tests that pin it and is reviewed closely
by a human before it changes. Code under a spec's `governs` list is reviewed against that spec rather
than on its own, and automation checks that every clause still has coverage.

A spec is front matter and then numbered clauses; everything outside a clause is commentary and binds
nobody.

- **`id`** is a short prefix. Clause ids are `PREFIX-N`, allocated in order, **never reused and never
  renumbered**, because a commit message, an issue and a test name all point at one. A withdrawn clause
  stays, marked withdrawn, and says what replaced it.
- **Every clause carries an anchor**, so `labels.md#LABEL-3` is a link that keeps working. The anchor
  GitHub generates from a heading contains the title, so it breaks the moment somebody improves the
  wording.
- **`governs`** lists the paths this spec decides. Anything under no spec's `governs` is ordinary code.
- **`guards`** lists symbols whose every use is review-required.
- **`verified-by:`** lines name the tests that pin a clause. The coverage check reads them and fails
  when a name does not resolve to a test that exists.

Clauses describe behaviour, not implementation. A clause naming a function becomes wrong the next time
somebody renames one, and it pins the code that exists rather than the behaviour the code owes.

## Reviewing for the rule

Everything is predicated on one statement: **untrusted content never enters the driver's context or
the planner's**. The subtle violations look like safety features, which is what makes them hard to
spot. The four things to look for in a diff are in [Security](security/security.md#what-this-defends-against).
