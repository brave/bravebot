# contrib

Tools that are useful for working on bravebot but are not part of it. Nothing here is built or
shipped, and nothing in `crates/` depends on any of it. One is run by CI: `measure-flakes.py`, on a
weekly schedule, since what it measures takes many runs of an unchanged tree.

## check-toolchain.py

Answers whether clippy on this machine knows the lints CI will fail on.

CI installs whatever stable is on the day it runs, and clippy gains lints with every release, so a
host a few releases behind passes a lint CI fails. Nothing in a diff shows this, and the first
report of it is a red build on code that was checked before it was pushed.

Rust ships every six weeks and rustc states its own release date, so the gap is measurable without
asking the network what stable is today. Anything under a release old passes silently.

```sh
make check-toolchain          # or contrib/check-toolchain.py
```

`make check` runs it last and fails on it, since that target's claim is that passing it means CI
passes. It compiles nothing. `--warn` prints the same thing and exits zero, and
`BRAVEBOT_ALLOW_STALE_TOOLCHAIN=1` skips it, for a host that cannot have the newer toolchain.

`make check-linux` is the way to run CI's fmt, clippy and tests on current stable regardless of
what the host has.

## check-locales.py

Holds every message catalog to the record of what it is missing.

A translation with fewer messages than `en-US.ftl` builds, and what it lacks is shown in English.
That is deliberate, and [the spec](../docs/specs/localization.md) gives the reason: a catalog nobody
could use until it was finished would never be started. The cost is that a message can ship
untranslated with nothing saying so, and eleven did. The build script counts the gap, but it prints
a warning, and a warning is silent on a cached build and does not fail a job that passed.

So the gap is not what this gates on. An unrecorded gap is.
[untranslated-messages.txt](untranslated-messages.txt) lists what each catalog is knowingly
missing, and the check fails while the file and the catalogs disagree in either direction: a gap
nobody wrote down, and a line for a gap that is no longer there. The second matters as much as the
first, or the file becomes a graveyard that permits the next one. The same shape as
`agents/unverified-clauses.txt` and `make check-spec`.

```sh
make check-locales        # or contrib/check-locales.py
make locales              # the same count as a report, which no gap it finds makes fail
make write-untranslated   # rewrite the record after translating something
```

`make check-all` runs it and CI has a job for it. It compiles nothing and needs no toolchain.

A line at column zero that is neither blank, a comment, nor a message is an error rather than
something skipped: the parser is hand written against the subset of Fluent in
[locales/README.md](../crates/i18n/locales/README.md), and one that quietly matches nothing would
report every catalog complete forever. `--selftest` checks the parse and the verdict against inputs
whose answer is known, and reads no catalog; `make check-locales` runs it first, for the same reason.

## check-versions.py

Holds every file that states this repository's version to the one in the workspace manifest.

[RELEASE-1](../docs/specs/releases.md) says one version names a release and every file that
repeats it agrees, and what held that was a refusal in the tagging path comparing two files. The
front end under `ui/` arrived from another repository at 0.1.0 while this workspace was at 0.9.0,
and nothing anywhere said so: the application is packaged from its own manifest, so the version
left behind is an app bundle naming a release that was never made.

A refusal at the tag reports that on release day. This reports it in the pull request that caused
it, which is why CI has a job for it and why `make bump-version` runs it over its own work before
committing. Six files state a version: the workspace manifest, the published wrapper and its
lockfile, and the application and its lockfile, where a lockfile states it twice, in its own
header and in the entry for the package it locks.

```sh
make check-versions       # or contrib/check-versions.py
```

`make check` runs it too. It compiles nothing and needs no toolchain. `docs/website` is not in
scope: it is a documentation site whose package version names nothing anybody installs.

`--set <version>` is the writing half, and `make bump-version` is the only thing that calls it:
it puts a version into the four JSON files, or into none of them where one of them could not be
written back as it was found. Reading and writing are the same file so that a bump and a check
cannot disagree about where a version is stated. Cargo's two are cargo's, rewritten by the bump
target itself.

`--selftest` checks both halves against trees whose answer is known and reads no tree;
`make check-versions` runs it first, because a version check that reads four of the six files
passes on this tree today and would have passed on the tree that shipped the front end at 0.1.0.
`--root` points it at another tree, which is what the selftest uses.

## measure-flakes.py

Names every test that does not agree with itself, and how often.

[checks.md](../docs/development/checks.md) says a failure that does not reproduce is a flake and to
carry on rather than chase it, which is sound only if somebody knows which tests actually do that.
Nothing measured it, so the answer was a shrug. Some tests here stand up a real server on an
ephemeral port, and some spawn a program they have just written, and those are the ones that lose a
run.

One build, that build run N times, and a table of the tests whose outcome changed, worst rate first,
each with the panic it produced so that one cause across five tests reads as one thing.

```sh
contrib/measure-flakes.py --runs 30
contrib/measure-flakes.py --runs 30 --test-threads 4   # what make check-linux caps the suite to
```

Nothing fails over a rate. A job that goes red for a known two percent flake is a job people learn
to ignore, which is the problem this exists for, so a rate is reported and never gated on.

`--file-issues OWNER/REPO` opens one issue per flaky test, titled `Flaky test: <name>`, saying the
rate, the parallelism it was measured at and what the failure said. A test that already has an issue
under that title gets nothing, whoever opened it, so a weekly run adds only what is new instead of
filing the same test every Monday. `--assign LOGIN` puts that login on every issue the run opens:
nothing here is triaged, so an issue naming nobody waits until a person notices it and assigns it by
hand. The workflow passes `netzenbot`; a run by hand assigns nobody unless it says otherwise.
`--dry-run` prints what it would send and sends nothing; `--issue-limit` bounds how many one run may
open, since the first measurement of a suite nobody has measured could otherwise arrive as twenty
issues at once. The exit code is still not a verdict on
the tests: it is nonzero when the measurement did not happen, or when an issue that was meant to be
filed was not, because a filer that has quietly stopped working looks exactly like a suite that
stopped being flaky.

`--summary FILE` appends the same report to a file, which is how the weekly
[Test determinism](../.github/workflows/test-determinism.yml) workflow puts it in a job summary.
That workflow names both thread counts rather than leaving either to cargo, since a hosted runner
has four cores and cargo's default there is already the cap: four threads and sixteen. Sixteen on a
four core runner is those cores oversubscribed rather than a stand-in for a sixteen core machine, so
what the legs differ by is the cost of oversubscription. Only the wider leg files issues, since both
legs measure the same tests and both filing would race to open two issues for one flake.

Every report says how many CPUs the machine had and which failure rate that many runs could actually
have seen, because neither is recoverable afterwards and both decide what the numbers mean. Thirty
runs cannot account for a test that loses one run in twenty, and a quiet runner will not reproduce a
race that needs a machine with something else on it, so a report naming nothing bounds the flakiness
rather than ruling it out.

`--selftest` checks the parsing against output whose answer is known and runs no tests. It is a job
of its own in that workflow, which is also what a pull request touching this file runs, and the
thing to run after touching a regular expression here: a parser that matches nothing reports a clean
suite forever.

## drive_tui.py

Drives the terminal interface from a script, so the parts of it that a unit test cannot reach can
be exercised anyway.

Most of the interface is wiring: a key press becomes an `Action`, the action calls into the agent,
and what comes back is drawn. The tests under `crates/tui` cover each of those pieces on its own,
and every interface bug found so far has been in the joins between them rather than in the pieces.
`/compact` shipped once with no capability to reach the model and was refused every time it was
used; the context gauge read nothing at all for a while; a summary came back and the planner
disbelieved it. None of those is visible from `cargo test`, and all three are obvious within one
scripted session.

### Running one

A script is one step per line: how long to wait, then the keys to send.

```
# Answer the trust prompt, hold four short exchanges, then summarise them.
10   y
90   say one\r
90   say two\r
90   say three\r
90   say four\r
90   /compact\r
90   what did I ask you first\r
10   /exit\r
```

```sh
cargo build
cd /tmp/scratch
contrib/drive_tui.py session.txt --squash -- target/debug/bravebot
```

The command to run goes after `--`. `-` in place of the script file reads the script from stdin.

A step's number is a timeout rather than a sleep. While a turn runs the spinner animates several
times a second, so the stream is never quiet, and the step ends as soon as it goes still. Give
each step longer than the slowest reply you expect and the script will still run at the speed of
the model.

### Asserting against the output

Pass `--squash`, and search for text with the spaces taken out:

```sh
contrib/drive_tui.py session.txt --squash -- target/debug/bravebot | grep -o 'summarised[0-9]*earlier'
```

This is not a terminal emulator. Escape sequences are stripped rather than interpreted, so the
characters come out in the right order and the wrong shape: a sentence the interface wrapped
across two lines arrives with a box border through the middle of it. `--squash` removes the
whitespace and the box drawing so a substring check can be trusted. `--raw FILE` keeps the
untouched capture if you want to look at what really happened.

### What it needs

A backend, like any other session. Either the configured one, or a local
[aichat](https://github.com/brave/aichat) with `BRAVE_AI_CHAT_ENDPOINT` pointed at it:

```sh
BRAVE_AI_CHAT_ENDPOINT=http://127.0.0.1:8000 contrib/drive_tui.py session.txt -- target/debug/bravebot
```

`BRAVEBOT_CONTEXT_BUDGET` is worth setting low when the thing being tested is compaction, since the
default is a hundred thousand tokens and a scripted session will not reach it.

Sessions it runs are real: they are written to `~/.bravebot/sessions` like any other, and a script that
approves a write will write the file. Run it somewhere disposable.

## terminal-screenshot.py

Turns a raw capture back into the screen a person would have seen, as text.

`--squash` is for a substring check and is unreadable as a picture: the interface draws by moving
the cursor and overwriting cells, so the bytes it wrote are not the frame it painted. Replaying
those bytes against a grid of the same size gives the frame itself, which is what a bug report
needs. A reader can act on a screen; nobody can act on a paragraph describing one.

```sh
contrib/drive_tui.py session.txt --raw capture.txt -- target/debug/bravebot
contrib/terminal-screenshot.py capture.txt
```

`--cols` and `--rows` default to the 120x40 `drive_tui.py` runs at and must match the capture
otherwise, since a frame laid out for one width replayed at another is wrong in a way that still
looks like a screen. Colour is dropped, so the output survives being quoted and searched.

Anything it does not model is named on stderr rather than dropped, and `--strict` exits non-zero on
one, which is the flag to use before pasting a screen into an issue. `--selftest` checks the replay
against captures whose screen is known and renders nothing; `make check-spec` runs it, because the
[check-spec skill](../agents/skills/check-spec/SKILL.md) pastes what this prints into issue bodies.
