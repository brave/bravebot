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
import sys
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
# How long a program gets to end after the signal that stops it.
STOP_SECONDS = 30.0
# Samples a second. A call graph from DWARF copies part of the stack at every sample, so a test that
# runs for a minute is sampled less often than a start that lasts a second.
TEST_SAMPLE_RATE = 99
START_SAMPLE_RATE = 4999
SAMPLE_SECONDS = 1800
PERF_SCRIPT_SECONDS = 600
TOOL_SECONDS = 300
# The release build of the terminal client that the functions lane samples keeps the line tables the
# release profile leaves out. `--config` changes the one build and leaves `[profile.release]` as it is,
# and the build goes into a target directory of its own, so the binary the size lane measures is the
# release binary.
PROFILING_CONFIG = 'profile.release.debug="line-tables-only"'
# What each startup candidate runs, after the binary.
START_ARGUMENTS = {"startup:version": ["--version"], "startup:terminal": []}
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
        self.profiling = None
        self.lanes = ()
        # The candidates of the other lanes in this run, which the functions lane samples, and the
        # functions it found in each of them by the command's key.
        self.candidates = []
        self.hot = {}

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


def wait_for(child, seconds):
    """Wait up to `seconds` for `child` to end, then kill it, so a program that ignores the signal sent
    to it does not hold the measurement."""
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        done, _ = os.waitpid(child, os.WNOHANG)
        if done:
            return
        time.sleep(0.05)
    os.kill(child, signal.SIGKILL)
    os.waitpid(child, 0)


def terminal_start(argv, env, cwd, stop=signal.SIGKILL):
    """Seconds from starting `argv` under a terminal until its output has been still for
    STILL_SECONDS, or None where it printed nothing. This is the time to a drawn first screen. The
    program is ended with `stop`, which is `SIGINT` for a profiler, so that it writes its file."""
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
    for step in (lambda: os.kill(child, stop), lambda: wait_for(child, STOP_SECONDS), lambda: os.close(terminal)):
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


def tail(text, words=30):
    return " ".join(text.split()[-words:])


def perf_problem(ctx):
    """Why `perf` cannot record a call graph on this machine, or None where it can. A kernel setting
    (`perf_event_paranoid`), a container or a virtual machine without counters stops it, and it is
    found by recording a program that does nothing, before anything is built to be sampled."""
    data = ctx.scratch / "perf-check.data"
    data.unlink(missing_ok=True)
    code, _, err, _ = run(
        [shutil.which("perf"), "record", "--all-user", "-N", "--call-graph", "dwarf", "-o", data, "--", sys.executable, "-c", "pass"],
        ctx.scratch, ctx.env(), TOOL_SECONDS)
    data.unlink(missing_ok=True)
    return None if code == 0 else f"perf cannot record here: {tail(err)}"


def test_executables(ctx):
    """`{binary: (executable, directory)}` for the test binaries of the tests lane, which `cargo test
    --no-run` finds in the target directory the lane built them into."""
    code, out, _, _ = run(
        ["cargo", "test", "--workspace", "--locked", "--no-run", "--message-format", "json"],
        ctx.root, ctx.env(CARGO_TARGET_DIR=str(ctx.root / "target" / "profile-tests")), BUILD_SECONDS)
    return lanes.parse_test_executables(out) if code == 0 else {}


def profiling_build(ctx):
    """`(binary, note)` for the terminal client built with line tables, in a target directory the
    release build for the size lane does not use. The binary is None where the build failed."""
    if ctx.profiling is not None:
        return ctx.profiling
    target = ctx.scratch / "profiling-target"
    code, _, err, _ = run(
        ["cargo", "build", "--release", "--locked", "-p", "bravebot-cli", "--config", PROFILING_CONFIG],
        ctx.root, ctx.env(CARGO_TARGET_DIR=str(target)), BUILD_SECONDS)
    binary = target / "release" / "bravebot"
    ctx.profiling = (binary, None) if code == 0 and binary.exists() else (None, f"the build with line tables failed: {tail(err)}")
    return ctx.profiling


def find_places(ctx, executable, names):
    """`{function: file:line}` for the functions in `names` that `nm` finds in `executable` and
    `addr2line` finds a line for. A function either cannot find is left out, which is the case for a
    name the two tools spell differently, such as a closure."""
    nm, addr2line = shutil.which("nm"), shutil.which("addr2line")
    if not nm or not addr2line:
        return {}
    keys = set(names)
    code, out, _, _ = run([nm, "-C", "--defined-only", executable], ctx.root, ctx.env(), TOOL_SECONDS)
    addresses = lanes.parse_nm(out, keys) if code == 0 else {}
    if not addresses:
        return {}
    code, out, _, _ = run([addr2line, "-e", executable, "-a", *[hex(a) for a in sorted(set(addresses.values()))]],
                          ctx.root, ctx.env(), TOOL_SECONDS)
    places = lanes.parse_addr2line(out) if code == 0 else {}
    found = {}
    for name in names:
        place = places.get(addresses.get(name))
        if place:
            found[name] = lanes.short_location(place, ctx.root)
    return found


def sample(ctx, argv, cwd, rate, terminal=False, **more):
    """`(samples, problem)`: the stacks `perf` took of `argv` while it ran, each with the running
    function first. Under a terminal the program is ended with SIGINT once its output is still."""
    data = ctx.scratch / "perf.data"
    data.unlink(missing_ok=True)
    env = ctx.env(**more)
    record = [shutil.which("perf"), "record", "--all-user", "-N", "-F", str(rate), "--call-graph", "dwarf", "-o", data, "--", *argv]
    if terminal:
        terminal_start([str(a) for a in record], env, cwd, stop=signal.SIGINT)
    else:
        code, _, err, _ = run(record, cwd, env, SAMPLE_SECONDS)
        if code is None:
            return [], f"perf did not start or finish: {tail(err)}"
    if not data.exists() or data.stat().st_size == 0:
        return [], "perf wrote no samples"
    code, out, err, _ = run([shutil.which("perf"), "script", "-i", data, "-F", "ip,sym"], cwd, env, PERF_SCRIPT_SECONDS)
    data.unlink(missing_ok=True)
    if code != 0:
        return [], f"perf script failed: {tail(err)}"
    return lanes.parse_perf_script(out), None


def command_for(ctx, command, executables):
    """`(argv, directory, rate, terminal, executable, problem)` for a candidate of another lane. A test
    runs in its package's directory, where cargo runs it."""
    key = command["key"]
    if command["lane"] == "tests":
        found = executables().get(command.get("binary", ""))
        if not found:
            return None, None, None, False, None, "no test binary of that name was built"
        executable, directory = found
        return lanes.test_command(executable, command["test"]), directory, TEST_SAMPLE_RATE, False, executable, None
    binary, note = profiling_build(ctx)
    if binary is None:
        return None, None, None, False, None, note
    work = ctx.scratch / "start-work"
    work.mkdir(exist_ok=True)
    return [binary, *START_ARGUMENTS[key]], work, START_SAMPLE_RATE, key == "startup:terminal", binary, None


def run_functions(ctx):
    """Sample the commands the other lanes found over their thresholds and rank the functions in them."""
    commands = lanes.sampled_commands(ctx.candidates)
    if not commands:
        if "tests" in ctx.lanes or "startup" in ctx.lanes:
            return [], "no test or start was over its threshold, so there was nothing to sample"
        return [], "the functions lane samples the commands the tests and startup lanes find, and neither ran"
    if shutil.which("perf") is None:
        return [], "perf is not installed, so the functions lane sampled nothing (macOS sample and samply output are not read)"
    problem = perf_problem(ctx)
    if problem:
        return [], problem
    cache = []

    def executables():
        if not cache:
            cache.append(test_executables(ctx))
        return cache[0]

    rows, notes = [], []
    if not (shutil.which("nm") and shutil.which("addr2line")):
        notes.append("nm or addr2line is not installed, so the functions have no source location")
    for command in commands:
        argv, directory, rate, terminal, executable, why = command_for(ctx, command, executables)
        if argv is None:
            notes.append(f"{command['name']} was not sampled: {why}")
            continue
        extra = {"CARGO_MANIFEST_DIR": str(directory)} if command["lane"] == "tests" else {}
        samples, why = sample(ctx, argv, directory, rate, terminal, **extra)
        if why:
            notes.append(f"{command['name']} was not sampled: {why}")
            continue
        found, total = lanes.function_rows(command, samples, lambda names, exe=executable: find_places(ctx, exe, names))
        if not found:
            notes.append(f"{command['name']} gave {total} samples, too few to say where its time goes")
            continue
        rows.extend(found)
        ctx.hot[command["key"]] = [
            {"name": r["function"], "self": r["self_share"], "inclusive": r["inclusive_share"], "location": r["location"]}
            for r in found if r["time_kind"] == "self"][: lanes.HOT_SHOWN]
    return rows, "; ".join(notes) or None


RUNNERS = {"tests": run_tests, "startup": run_startup, "build": run_build, "size": run_size, "scripts": run_scripts,
           "functions": run_functions}
