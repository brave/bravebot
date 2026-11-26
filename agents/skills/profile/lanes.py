#!/usr/bin/env python3
"""What each lane of the profile skill makes of the output of the program it ran.

Nothing here runs a program. Every function takes the text a tool printed, or numbers already
measured, and returns rows: one measurement each, with the threshold it is ranked against. A row
becomes a candidate in `judge` when it is over its threshold, or when it is well over the value the
ledger recorded for the same key at an earlier commit.

Standard library only, like the other checks.
"""

import html
import json
import re
from pathlib import Path

# The thresholds, each a statement that a person can disagree with. A measurement is ranked by how
# many times over its threshold it is, so the order does not depend on the unit.
TEST_SECONDS = 30.0  # one test
BINARY_SECONDS = 120.0  # the tests of one test binary, added up
START_SECONDS = {"version": 0.5, "terminal": 3.0}  # median of a cold start
UNIT_SECONDS = 60.0  # one compile unit
CRITICAL_SECONDS = UNIT_SECONDS / 2  # a compile unit on the critical path
BINARY_BYTES = 100 * 1024 * 1024  # one release binary
SCRIPT_SECONDS = 20.0  # one script the commit hook or CI runs
FUNCTION_SHARE = 0.20  # the samples one function holds itself, of those taken from one command
INCLUSIVE_SHARE = 0.50  # the samples a function is on the stack for, callees included

# A value this many times the one in the ledger is a regression, if it is also at least FLOOR of
# its threshold, so a test that went from 1 ms to 2 ms is not one. Sizes move less than times do,
# and their regression is a tenth.
REGRESSION = 1.25
SIZE_REGRESSION = 1.10
FLOOR = 0.25

SLOWEST_TESTS = 25
SLOWEST_BINARIES = 10
SLOWEST_UNITS = 15
CRATES_SHOWN = 15
FUNCTIONS_PER_COMMAND = 10  # functions kept for each sampled command, by the samples they hold themselves
SAMPLED_COMMANDS = 5  # commands the functions lane samples, the ones ranked furthest over their threshold
MIN_SAMPLES = 20  # fewer samples than this say nothing about how a command's time divides
HOT_SHOWN = 3  # functions named beside the command's own candidate

ATTRIBUTE = re.compile(r'([\w:.-]+)="([^"]*)"')
TESTCASE = re.compile(r"<testcase\b([^>]*?)/?>")
REPORT_LINE = re.compile(r"^test (\S+) \.\.\. (?:ok|FAILED|ignored)(?:, [^<]*)? <([\d.]+)s>\s*$")
RUNNING = re.compile(r"^\s*Running .*\(([^)]*)\)\s*$")
DOC_TESTS = re.compile(r"^\s*Doc-tests (\S+)")
UNIT_DATA = re.compile(r"UNIT_DATA\s*=\s*(\[.*?\]);\s*$", re.DOTALL | re.MULTILINE)
FRAME = re.compile(r"^\s+([0-9a-fA-F]+)(?:\s+(.*?))?\s*$")
SYMBOL_OFFSET = re.compile(r"\+0x[0-9a-fA-F]+$")
OBJECT_FILE = re.compile(r"\s+\((?:/|\[)[^()]*\)$")
SYMBOL_HASH = re.compile(r"::h[0-9a-f]{16}$")
NM_LINE = re.compile(r"^([0-9a-fA-F]+) [TtWw] (.+)$")
DISCRIMINATOR = re.compile(r"\s+\(discriminator \d+\)$")
UNKNOWN = "[unknown]"
ADDRESS_LINE = re.compile(r"^0x[0-9a-fA-F]+$")


def row(lane, key, name, value, unit, threshold, **more):
    """One measurement. `key` names it from one run to the next, `name` is what a person reads."""
    return {"lane": lane, "key": key, "name": name, "value": value, "unit": unit,
            "threshold": threshold, **more}


def fmt(value, unit):
    if unit == "bytes":
        return f"{value / (1024 * 1024):.1f} MiB"
    if unit == "share":
        return f"{value * 100:.1f}% of the samples"
    return f"{value:.2f}s"


def judge(entry, previous):
    """The candidate a row makes, or None. `previous` is the value the ledger holds for its key."""
    value, threshold = entry["value"], entry["threshold"]
    over = value / threshold
    growth = entry.get("growth", REGRESSION)
    regressed = bool(previous) and value >= previous * growth and value >= threshold * FLOOR
    if over < 1 and not regressed:
        return None
    ratio = value / previous if regressed else 0.0
    detail = f"{entry['name']}: {fmt(value, entry['unit'])}, {over:.2f} times its threshold of {fmt(threshold, entry['unit'])}"
    if regressed:
        detail += f", {ratio:.2f} times the {fmt(previous, entry['unit'])} the ledger recorded"
    return dict(entry, rank=max(over, ratio), previous=previous, regressed=regressed, detail=detail)


def attributes(text):
    return {name: html.unescape(value) for name, value in ATTRIBUTE.findall(text)}


def parse_junit(text):
    """`(binary, test, seconds)` for each test case in nextest's JUnit file. The time of the test
    case is read, not the time of the suite around it, which is the sum of its tests."""
    cases = []
    for found in TESTCASE.finditer(text):
        given = attributes(found.group(1))
        try:
            seconds = float(given["time"])
        except (KeyError, ValueError):
            continue
        cases.append((given.get("classname", ""), given.get("name", ""), seconds))
    return cases


def parse_report_time(text):
    """The same from `cargo test -- --report-time`, which prints `test name ... ok <0.123s>` under
    a `Running ... (target/debug/deps/binary-hash)` line for each binary."""
    cases, binary = [], ""
    for line in text.splitlines():
        running = RUNNING.match(line)
        if running:
            binary = re.sub(r"-[0-9a-f]{8,}$", "", running.group(1).rsplit("/", 1)[-1])
            continue
        doc = DOC_TESTS.match(line)
        if doc:
            binary = f"{doc.group(1)} doc-tests"
            continue
        timed = REPORT_LINE.match(line)
        if timed:
            cases.append((binary, timed.group(1), float(timed.group(2))))
    return cases


def test_rows(cases):
    """The slowest tests and the slowest test binaries. A binary's time is its tests added up, so a
    binary of many quick tests is found as well as one slow test."""
    totals = {}
    for binary, _, seconds in cases:
        totals[binary] = totals.get(binary, 0.0) + seconds
    slowest = sorted(cases, key=lambda c: (-c[2], c[0], c[1]))[:SLOWEST_TESTS]
    rows = [row("tests", f"tests:{binary}::{name}", f"test {name} in {binary}", seconds, "seconds",
                TEST_SECONDS, binary=binary, test=name) for binary, name, seconds in slowest]
    ranked = sorted(totals.items(), key=lambda item: (-item[1], item[0]))[:SLOWEST_BINARIES]
    rows += [row("tests", f"tests:{binary}", f"the tests of {binary}, added up", seconds, "seconds",
                 BINARY_SECONDS, binary=binary) for binary, seconds in ranked]
    return rows


def median(samples):
    ordered = sorted(samples)
    middle = len(ordered) // 2
    return ordered[middle] if len(ordered) % 2 else (ordered[middle - 1] + ordered[middle]) / 2


def startup_row(kind, name, samples):
    """The median of the runs as the value, and the worst of them in the detail."""
    return row("startup", f"startup:{kind}", f"{name}, median of {len(samples)} runs", median(samples),
               "seconds", START_SECONDS[kind], worst=max(samples), samples=len(samples))


def parse_units(text):
    """The compile units in the page `cargo build --timings` writes, or [] where it holds none."""
    found = UNIT_DATA.search(text)
    if not found:
        return []
    try:
        units = json.loads(found.group(1))
    except json.JSONDecodeError:
        return []
    return [u for u in units if isinstance(u, dict) and "duration" in u and "start" in u]


def unit_name(unit):
    name = f"{unit.get('name', '?')} {unit.get('version', '')}".strip()
    return f"{name} (build script run)" if unit.get("mode") == "run-custom-build" else name


def critical_path(units):
    """The chain of units that decided the build's length, last first. From the unit that finished
    last, each step goes to the unit that unlocked it and finished latest, since that is the one the
    unit waited for."""
    if not units:
        return []
    ends = {u.get("i"): u["start"] + u["duration"] for u in units}
    current, chain = max(units, key=lambda u: ends[u.get("i")]), []
    while current is not None:
        chain.append(current)
        waited_for = [u for u in units if current.get("i") in (u.get("unlocked_units") or [])]
        current = max(waited_for, key=lambda u: ends[u.get("i")]) if waited_for else None
        if current is not None and current in chain:
            break
    return chain


def unit_rows(units):
    on_path = {u.get("i") for u in critical_path(units)}
    chosen = {u.get("i"): u for u in sorted(units, key=lambda u: -u["duration"])[:SLOWEST_UNITS]}
    chosen.update({u.get("i"): u for u in units if u.get("i") in on_path})
    rows = []
    for index, unit in sorted(chosen.items(), key=lambda item: (-item[1]["duration"], str(item[0]))):
        critical = index in on_path
        rows.append(row("build", f"build:{unit_name(unit)}", unit_name(unit), float(unit["duration"]),
                        "seconds", CRITICAL_SECONDS if critical else UNIT_SECONDS, critical=critical))
    return rows


def parse_bloat(text):
    """`(size of the file, [(crate, bytes)])` from `cargo bloat --crates --message-format json`."""
    try:
        data = json.loads(text)
        crates = [(c["name"], int(c["size"])) for c in data.get("crates", [])]
        return int(data.get("file-size", 0)), crates
    except (json.JSONDecodeError, KeyError, TypeError, ValueError, AttributeError):
        return 0, []


def size_rows(sizes, crates_by_binary=None):
    """One row per release binary. Its crates, where cargo-bloat gave them, are in the detail."""
    rows = []
    for name, size in sorted(sizes.items()):
        crates = (crates_by_binary or {}).get(name, [])[:CRATES_SHOWN]
        rows.append(row("size", f"size:{name}", f"release binary {name}", float(size), "bytes",
                        float(BINARY_BYTES), growth=SIZE_REGRESSION, crates=crates))
    return rows


def prerequisites(makefile, target):
    """The prerequisites of `target` in a Makefile, with its backslash continuations joined."""
    lines, joined = makefile.splitlines(), ""
    for line in lines:
        text = joined + line.strip()
        if line.rstrip().endswith("\\"):
            joined = text[:-1].rstrip() + " "
            continue
        joined = ""
        head, colon, rest = text.partition(":")
        if colon and head.strip() == target and not rest.startswith("="):
            return rest.split()
    return []


def script_rows(timed):
    """`timed` is `(make target, seconds)` for each script the commit hook or CI runs."""
    return [row("scripts", f"scripts:{target}", f"make {target}", seconds, "seconds", SCRIPT_SECONDS)
            for target, seconds in sorted(timed)]


def sampled_commands(candidates):
    """The candidates of the other lanes that the functions lane samples: a single test, and a start.
    The ones furthest over their threshold come first, since `rank` does not depend on the unit. A
    test binary added up and a compile unit are not one command to run, and a size is not a time."""
    found = [c for c in candidates if (c["lane"] == "tests" and c.get("test")) or c["lane"] == "startup"]
    return sorted(found, key=lambda c: (-c["rank"], c["key"]))[:SAMPLED_COMMANDS]


def clean_symbol(text):
    """The function a frame names, from the text after its address in `perf script -F ip,sym`: without
    the offset into the function, the object file and the hash a legacy Rust symbol ends in, so every
    sample inside one function is one name."""
    name = SYMBOL_HASH.sub("", SYMBOL_OFFSET.sub("", OBJECT_FILE.sub("", (text or "").strip())))
    return name or UNKNOWN


def parse_perf_script(text):
    """The samples in `perf script -F ip,sym` output, each a list of function names with the one that was
    running first. A blank line ends a sample, and so does a line that is not a frame, such as the header
    a perf that prints one writes before each stack."""
    samples, stack = [], []
    for line in text.splitlines():
        frame = FRAME.match(line) if line.strip() else None
        if frame:
            stack.append(clean_symbol(frame.group(2)))
        elif stack:
            samples.append(stack)
            stack = []
    if stack:
        samples.append(stack)
    return samples


def rank_functions(samples):
    """`(samples taken, functions)`, most self samples first. A function's self samples are those where it
    was the one running, and its inclusive samples are those where it was anywhere on the stack, counted
    once however often recursion put it there. Frames with no name are in the total and not listed."""
    own, inclusive = {}, {}
    for stack in samples:
        own[stack[0]] = own.get(stack[0], 0) + 1
        for name in set(stack):
            inclusive[name] = inclusive.get(name, 0) + 1
    functions = [{"name": name, "self": own.get(name, 0), "inclusive": count}
                 for name, count in inclusive.items() if name != UNKNOWN]
    functions.sort(key=lambda f: (-f["self"], -f["inclusive"], f["name"]))
    return len(samples), functions


def function_rows(command, samples, locate=None):
    """`(rows, samples taken)` for the functions in the samples of one command, which is a row of another
    lane. A function is a row by the share of the samples it holds itself, and by the share it is on the
    stack for unless it is on every stack, which says nothing of where the time went. `locate` takes the
    names of the functions kept and returns `{name: file:line}`."""
    total, functions = rank_functions(samples)
    if total < MIN_SAMPLES:
        return [], total
    own = [f for f in functions if f["self"]][:FUNCTIONS_PER_COMMAND]
    below = sorted((f for f in functions if f["inclusive"] < total), key=lambda f: (-f["inclusive"], -f["self"], f["name"]))
    chosen = {"self": (own, FUNCTION_SHARE), "inclusive": (below[:FUNCTIONS_PER_COMMAND], INCLUSIVE_SHARE)}
    places = (locate or (lambda names: {}))(sorted({f["name"] for kept, _ in chosen.values() for f in kept}))
    rows = []
    for kind, (kept, threshold) in chosen.items():
        for f in kept:
            rows.append(row(
                "functions", f"functions:{command['key']}:{kind}:{f['name']}", f"{f['name']} ({kind} time) in {command['name']}",
                f[kind] / total, "share", threshold, command=command["key"], function=f["name"], time_kind=kind,
                self_share=f["self"] / total, inclusive_share=f["inclusive"] / total, samples=total,
                location=places.get(f["name"])))
    return rows, total


def test_command(executable, name):
    """The arguments that run one test of a test binary, and no other whose name contains it."""
    return [str(executable), "--exact", name, "--nocapture", "--test-threads", "1"]


def package_name(package_id):
    """The package in a cargo package id, in the `path+file:///dir#name@version` form and the older
    `name version (source)` one."""
    if "#" in package_id:
        place, _, tail = package_id.rpartition("#")
        name = tail.split("@")[0]
        return name if name and not name[0].isdigit() else place.rstrip("/").rsplit("/", 1)[-1]
    return (package_id.split() or [""])[0]


def parse_test_executables(text):
    """`{binary: (executable, directory)}` from `cargo test --no-run --message-format json`. A binary is
    named as nextest names it (`pkg` for a library, `pkg::test` for an integration test, `pkg::bin/name`)
    and as `cargo test` prints it (the file name without its hash). The directory is the package's, where
    cargo runs the binary. An artifact that is not a test build is not a test binary."""
    found = {}
    for line in text.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(message, dict) or message.get("reason") != "compiler-artifact":
            continue
        executable = message.get("executable")
        target = message.get("target") or {}
        if not executable or not (message.get("profile") or {}).get("test"):
            continue
        package, name = package_name(str(message.get("package_id", ""))), target.get("name", "")
        kinds = target.get("kind") or [""]
        if kinds[0] in ("lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"):
            named = package
        elif kinds[0] == "test":
            named = f"{package}::{name}"
        else:
            named = f"{package}::{kinds[0]}/{name}"
        entry = (executable, str(Path(message.get("manifest_path", "")).parent))
        found[named] = entry
        found.setdefault(re.sub(r"-[0-9a-f]{8,}$", "", Path(executable).name), entry)
    return found


def parse_nm(text, wanted=None):
    """`{function: address}` from `nm -C --defined-only`, the lowest address where one name has several
    (a generic function, a clone), for the names in `wanted` or for all of them."""
    found = {}
    for line in text.splitlines():
        symbol = NM_LINE.match(line)
        if not symbol:
            continue
        name, address = SYMBOL_HASH.sub("", symbol.group(2).strip()), int(symbol.group(1), 16)
        if (wanted is None or name in wanted) and address < found.get(name, address + 1):
            found[name] = address
    return found


def parse_addr2line(text):
    """`{address: file:line}` from `addr2line -a`, which prints an address and then its place. A place
    it could not find (`??:0`) is None, and a discriminator is not part of the line."""
    found, address = {}, None
    for line in text.splitlines():
        line = line.strip()
        if ADDRESS_LINE.match(line):
            address = int(line, 16)
        elif address is not None:
            place = DISCRIMINATOR.sub("", line)
            found[address] = None if place.startswith("?") else place
            address = None
    return found


def short_location(place, root):
    """`file:line` as a reader of the checkout would write it: relative to `root` where the file is in
    it, and from `library/` for the standard library, whose path is the builder's."""
    if not place:
        return None
    prefix = str(root).rstrip("/") + "/"
    if place.startswith(prefix):
        return place[len(prefix):]
    if "/library/" in place:
        return "library/" + place.split("/library/", 1)[1]
    return place
