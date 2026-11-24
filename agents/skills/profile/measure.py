#!/usr/bin/env python3
"""Run the real programs the profile skill measures, and hand their output to `lanes.py`.

Every program starts with `HOME` pointed at a scratch directory and `BRAVEBOT_HOME` removed, so a
measurement never reads or writes the person's `~/.bravebot`. Cargo and rustup keep working because
their directories are named explicitly before `HOME` moves.

Each runner returns `(rows, note)`. A lane whose tool is missing returns the note and no rows.
"""

import os
import shutil
import signal
import subprocess  # nosemgrep: gitlab.bandit.B404
import time
from pathlib import Path

import lanes

TESTS_SECONDS = 3600
BUILD_SECONDS = 3600
SCRIPT_TIMEOUT = 900
VERSION_RUNS = 5
TERMINAL_RUNS = 3
# The terminal client has started once its output has been still this long.
STILL_SECONDS = 0.5
TERMINAL_LIMIT = 20.0
SCRIPT_TARGETS_EXTRA = ("check-spec", "check-security", "check-locales", "check-versions", "check-code-selectors")


def run(args, cwd, env, timeout, merge_stderr=False):
    """`(exit code, stdout, stderr, seconds)`. The exit code is None where the program could not be
    started or did not finish in `timeout`, and stderr then says why. With `merge_stderr` both streams
    share one pipe, in the order the program wrote them, and come back as stdout with stderr empty."""
    began = time.monotonic()
    try:
        # The program and arguments are written in this file or name a binary this run built.
        done = subprocess.run(  # nosemgrep: gitlab.bandit.B603
            [str(a) for a in args], cwd=str(cwd), env=env, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT if merge_stderr else subprocess.PIPE, text=True,
            timeout=timeout, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as problem:
        return None, "", str(problem), time.monotonic() - began
    return done.returncode, done.stdout, done.stderr or "", time.monotonic() - began


class Context:
    """What one measurement run shares: the checkout it measures, a scratch directory it may fill,
    and the environment every program in it starts with."""

    def __init__(self, root, scratch):
        self.root = Path(root)
        self.scratch = Path(scratch)
        self.home = self.scratch / "home"
        self.home.mkdir(parents=True, exist_ok=True)
        self.built = None

    def env(self, **more):
        env = dict(os.environ)
        for name in ("BRAVEBOT_HOME", "CARGO_TARGET_DIR"):
            env.pop(name, None)
        env["CARGO_HOME"] = os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
        env["RUSTUP_HOME"] = os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup"))
        env["HOME"] = str(self.home)
        env["BRAVEBOT_ALLOW_UNCONFIGURED_BUILD"] = "1"
        env.update(more)
        return env


def release_build(ctx):
    """One cold release build for the build, size and startup lanes: `(directory, note)`, where the
    directory is None when the build did not finish. The target directory is new, so every crate
    compiles and `--timings` has a unit for each."""
    if ctx.built is not None:
        return ctx.built
    if shutil.which("cargo") is None:
        ctx.built = (None, "cargo is not installed, so nothing was built")
        return ctx.built
    target = ctx.scratch / "release-target"
    code, _, err, _ = run(
        ["cargo", "build", "--release", "--locked", "--workspace", "--timings"],
        ctx.root, ctx.env(CARGO_TARGET_DIR=str(target)), BUILD_SECONDS,
    )
    if code != 0:
        tail = " ".join(err.split()[-30:])
        ctx.built = (None, f"the release build failed or did not finish: {tail}")
    else:
        ctx.built = (target, None)
    return ctx.built


def run_tests(ctx):
    if shutil.which("cargo") is None:
        return [], "cargo is not installed, so the tests lane measured nothing"
    target = ctx.root / "target" / "profile-tests"
    env = ctx.env(CARGO_TARGET_DIR=str(target))
    if shutil.which("cargo-nextest"):
        env["NEXTEST_PROFILE"] = "profile"
        report = target / "nextest" / "profile" / "junit.xml"
        # The target directory outlasts a run, so a file found afterwards must be this run's.
        report.unlink(missing_ok=True)
        code, _, why, _ = run(
            ["cargo", "nextest", "run", "--workspace", "--locked", "--no-fail-fast", "--test-threads", "4"],
            ctx.root, env, TESTS_SECONDS)
        if code is None:
            return [], f"cargo-nextest did not start or finish, so the tests lane measured nothing: {why}"
        if not report.exists():
            return [], "cargo-nextest wrote no JUnit file, so the tests lane measured nothing"
        return lanes.test_rows(lanes.parse_junit(report.read_text(encoding="utf-8"))), None
    # libtest's `--report-time` is unstable, and `RUSTC_BOOTSTRAP` is what lets a stable toolchain's test binary take it.
    env["RUSTC_BOOTSTRAP"] = "1"
    # Cargo writes its `Running` and `Doc-tests` lines to stderr, so both streams share one pipe.
    _, out, _, _ = run(
        ["cargo", "test", "--workspace", "--locked", "--no-fail-fast", "--", "-Z", "unstable-options", "--report-time"],
        ctx.root, env, TESTS_SECONDS, merge_stderr=True,
    )
    cases = lanes.parse_report_time(out)
    note = "cargo-nextest is not installed, so the times are from cargo test --report-time, one binary after another"
    return (lanes.test_rows(cases), note) if cases else ([], note + ", and it printed no test times")


def terminal_start(argv, env, cwd):
    """Seconds from starting `argv` under a terminal until its output has been still for
    STILL_SECONDS, or None where it printed nothing. This is the time to a drawn first screen."""
    import fcntl
    import pty
    import select
    import struct
    import termios

    began = time.monotonic()
    child, terminal = pty.fork()
    if child == 0:
        try:
            os.chdir(cwd)
            os.execve(argv[0], argv, dict(env, TERM="xterm-256color"))  # nosemgrep: gitlab.bandit.B606
        finally:
            os._exit(127)
    fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    last, got = began, False
    while time.monotonic() - began < TERMINAL_LIMIT:
        ready, _, _ = select.select([terminal], [], [], 0.05)
        if ready:
            try:
                chunk = os.read(terminal, 65536)
            except OSError:
                break
            if not chunk:
                break
            got, last = True, time.monotonic()
        elif got and time.monotonic() - last >= STILL_SECONDS:
            break
    for step in (lambda: os.kill(child, signal.SIGKILL), lambda: os.waitpid(child, 0), lambda: os.close(terminal)):
        try:
            step()
        except OSError:
            pass
    return last - began if got else None


def run_startup(ctx):
    target, note = release_build(ctx)
    if target is None:
        return [], note
    binary = target / "release" / "bravebot"
    if not binary.exists():
        return [], "the release build has no bravebot binary"
    env, work = ctx.env(), ctx.scratch / "start-work"
    work.mkdir(exist_ok=True)
    found, rows = [], []
    times = [run([binary, "--version"], work, env, 60)[3] for _ in range(VERSION_RUNS)]
    rows.append(lanes.startup_row("version", "bravebot --version", times))
    try:
        starts = [terminal_start([str(binary)], env, work) for _ in range(TERMINAL_RUNS)]
    except ImportError:
        return rows, "the terminal client's start needs a pty, which this platform lacks"
    starts = [s for s in starts if s is not None]
    if starts:
        rows.append(lanes.startup_row("terminal", "the terminal client's first screen", starts))
    else:
        found.append("the terminal client printed nothing under a pty")
    return rows, "; ".join(found) or None


def run_build(ctx):
    target, note = release_build(ctx)
    if target is None:
        return [], note
    page = target / "cargo-timings" / "cargo-timing.html"
    if not page.exists():
        return [], "cargo wrote no timing page"
    units = lanes.parse_units(page.read_text(encoding="utf-8", errors="replace"))
    return (lanes.unit_rows(units), None) if units else ([], "cargo's timing page holds no unit data")


def executables(directory):
    """The release binaries in a cargo release directory, by name."""
    found = {}
    for path in sorted(directory.iterdir()):
        if path.is_file() and not path.name.startswith(".") and path.suffix == "" and os.access(path, os.X_OK):
            found[path.name] = path
    return found


def run_size(ctx):
    target, note = release_build(ctx)
    if target is None:
        return [], note
    binaries = executables(target / "release")
    sizes = {name: path.stat().st_size for name, path in binaries.items()}
    if not sizes:
        return [], "the release build produced no binary"
    crates, found = {}, None
    if shutil.which("cargo-bloat"):
        for name in sizes:
            _, out, _, _ = run(["cargo", "bloat", "--release", "--crates", "--message-format", "json", "-n", "15", "--bin", name],
                               ctx.root, ctx.env(CARGO_TARGET_DIR=str(target)), 900)
            crates[name] = lanes.parse_bloat(out)[1]
    else:
        found = "cargo-bloat is not installed, so the crates behind each size are not listed"
    return lanes.size_rows(sizes, crates), found


def run_scripts(ctx):
    make = shutil.which("make")
    if make is None:
        return [], "make is not installed, so the scripts lane measured nothing"
    makefile = ctx.root / "Makefile"
    if not makefile.exists():
        return [], "there is no Makefile to read the scripts from"
    targets = lanes.prerequisites(makefile.read_text(encoding="utf-8"), "check-scripts")
    targets += [t for t in SCRIPT_TARGETS_EXTRA if t not in targets]
    timed, failed = [], []
    for target in targets:
        code, _, _, seconds = run([make, "--no-print-directory", target], ctx.root, ctx.env(), SCRIPT_TIMEOUT)
        (timed if code == 0 else failed).append((target, seconds) if code == 0 else target)
    return lanes.script_rows(timed), (f"left out because they failed: {', '.join(failed)}" if failed else None)


RUNNERS = {"tests": run_tests, "startup": run_startup, "build": run_build, "size": run_size, "scripts": run_scripts}
