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

# The thresholds, each a statement that a person can disagree with. A measurement is ranked by how
# many times over its threshold it is, so the order does not depend on the unit.
TEST_SECONDS = 30.0  # one test
BINARY_SECONDS = 120.0  # the tests of one test binary, added up
START_SECONDS = {"version": 0.5, "terminal": 3.0}  # median of a cold start
UNIT_SECONDS = 60.0  # one compile unit
CRITICAL_SECONDS = UNIT_SECONDS / 2  # a compile unit on the critical path
BINARY_BYTES = 100 * 1024 * 1024  # one release binary
SCRIPT_SECONDS = 20.0  # one script the commit hook or CI runs

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

ATTRIBUTE = re.compile(r'([\w:.-]+)="([^"]*)"')
TESTCASE = re.compile(r"<testcase\b([^>]*?)/?>")
REPORT_LINE = re.compile(r"^test (\S+) \.\.\. (?:ok|FAILED|ignored)(?:, [^<]*)? <([\d.]+)s>\s*$")
RUNNING = re.compile(r"^\s*Running .*\(([^)]*)\)\s*$")
DOC_TESTS = re.compile(r"^\s*Doc-tests (\S+)")
UNIT_DATA = re.compile(r"UNIT_DATA\s*=\s*(\[.*?\]);\s*$", re.DOTALL | re.MULTILINE)


def row(lane, key, name, value, unit, threshold, **more):
    """One measurement. `key` names it from one run to the next, `name` is what a person reads."""
    return {"lane": lane, "key": key, "name": name, "value": value, "unit": unit,
            "threshold": threshold, **more}


def fmt(value, unit):
    if unit == "bytes":
        return f"{value / (1024 * 1024):.1f} MiB"
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
                TEST_SECONDS, binary=binary) for binary, name, seconds in slowest]
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
