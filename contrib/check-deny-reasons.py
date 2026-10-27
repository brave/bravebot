#!/usr/bin/env python3
"""Holds every exception in .config/deny.toml to a written reason.

cargo-deny treats `reason` as optional on an advisory to ignore, a duplicate to skip and a crate to
ban, and accepts a bare string in place of the table. An exception with no reason gives a reviewer
nothing to agree to, and an ignored advisory is one this binary ships with anyway. Whether a reason
is good is judged in review (DEP-001); that one is written is decided here.

cargo-deny takes no `reason` on a licence exception and rejects the key, so the reason there is a
comment on the entry's line or on the line above it.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

EXCEPTIONS = (
    ("advisories", "ignore"),
    ("bans", "skip"),
    ("bans", "skip-tree"),
    ("bans", "deny"),
)


def name_of(entry):
    if isinstance(entry, str):
        return entry
    for key in ("crate", "id", "name"):
        if isinstance(entry.get(key), str):
            return entry[key]
    return "unnamed"


def commented(lines, index):
    """Whether line `index` has a comment of its own, or the line above it is one."""
    code = re.sub(r"\"[^\"]*\"|'[^']*'", '""', lines[index])
    return "#" in code or (index > 0 and lines[index - 1].lstrip().startswith("#"))


def exception_starts(lines, name):
    """The line each licence exception for `name` starts on: its own line where it is an inline
    table, or the `[[licenses.exceptions]]` header above where it is a table of its own. Only the
    licence tables are read, since the same crate can be banned or skipped elsewhere."""
    section, starts = "", []
    for number, line in enumerate(lines):
        if line.strip().startswith("["):
            section = line.split("#")[0].strip()
        if section not in ("[licenses]", "[[licenses.exceptions]]"):
            continue
        if re.search(rf"""\bcrate\s*=\s*["']{re.escape(name)}["']""", line):
            start = number
            while section == "[[licenses.exceptions]]" and not lines[start].lstrip().startswith("[["):
                start -= 1
            starts.append(start)
    return starts


def problems(text):
    """One line for each exception in `text` with no reason."""
    config = tomllib.loads(text)
    for section, key in EXCEPTIONS:
        for index, entry in enumerate(config.get(section, {}).get(key, [])):
            reason = entry.get("reason") if isinstance(entry, dict) else None
            if not isinstance(reason, str) or not reason.strip():
                yield f"[{section}] {key}[{index}] ({name_of(entry)}) has no reason"
    lines = text.splitlines()
    for index, entry in enumerate(config.get("licenses", {}).get("exceptions", [])):
        name = name_of(entry)
        starts = exception_starts(lines, name)
        if not starts or not all(commented(lines, start) for start in starts):
            yield f"[licenses] exceptions[{index}] ({name}) has no comment saying why"


def selftest():
    with_reason = '{ crate = "a:1", reason = "pinned by b" }'
    cases = (
        ("an empty file", "", []),
        ("empty lists", "[advisories]\nignore = []\n[bans]\nskip = []\n", []),
        ("a skip with a reason", f"[bans]\nskip = [{with_reason}]\n", []),
        ("a deny with a reason", '[bans]\ndeny = [{ crate = "x", reason = "runs programs" }]\n', []),
        (
            "an ignore with a reason",
            '[advisories]\nignore = [{ id = "RUSTSEC-2000-0001", reason = "unreachable" }]\n',
            [],
        ),
        (
            "a skip with no reason",
            '[bans]\nskip = [{ crate = "a:1" }]\n',
            ["[bans] skip[0] (a:1) has no reason"],
        ),
        (
            "a bare string in a skip",
            '[bans]\nskip = ["a:1"]\n',
            ["[bans] skip[0] (a:1) has no reason"],
        ),
        (
            "a bare advisory id",
            '[advisories]\nignore = ["RUSTSEC-2000-0001"]\n',
            ["[advisories] ignore[0] (RUSTSEC-2000-0001) has no reason"],
        ),
        (
            "an empty reason",
            '[bans]\nskip = [{ crate = "a:1", reason = "" }]\n',
            ["[bans] skip[0] (a:1) has no reason"],
        ),
        (
            "a reason of spaces",
            '[bans]\ndeny = [{ crate = "x", reason = "   " }]\n',
            ["[bans] deny[0] (x) has no reason"],
        ),
        (
            "a reason that is not a string",
            '[bans]\nskip = [{ crate = "a:1", reason = 3 }]\n',
            ["[bans] skip[0] (a:1) has no reason"],
        ),
        (
            "a skip-tree with no reason",
            '[bans]\nskip-tree = [{ crate = "t" }]\n',
            ["[bans] skip-tree[0] (t) has no reason"],
        ),
        (
            "a licence exception with a comment above it",
            '[licenses]\nexceptions = [\n    # vendored, permissive\n    { crate = "x", allow = ["NCSA"] },\n]\n',
            [],
        ),
        (
            "a licence exception with a comment after it",
            '[licenses]\nexceptions = [\n    { crate = "x", allow = ["NCSA"] },  # vendored\n]\n',
            [],
        ),
        (
            "a licence exception with no comment",
            '[licenses]\nexceptions = [\n    { crate = "x", allow = ["NCSA"] },\n]\n',
            ["[licenses] exceptions[0] (x) has no comment saying why"],
        ),
        (
            "a licence exception whose only # is inside its crate name",
            '[licenses]\nexceptions = [\n    { crate = "x#1", allow = ["NCSA"] },\n]\n',
            ["[licenses] exceptions[0] (x#1) has no comment saying why"],
        ),
        (
            "a comment above one licence exception does not cover the next",
            '[licenses]\nexceptions = [\n    # vendored\n    { crate = "x", allow = ["NCSA"] },\n'
            '    { crate = "y", allow = ["NCSA"] },\n]\n',
            ["[licenses] exceptions[1] (y) has no comment saying why"],
        ),
        (
            "a licence exception in a table of its own, with a comment above its header",
            '# vendored, permissive\n[[licenses.exceptions]]\ncrate = "x"\nallow = ["NCSA"]\n',
            [],
        ),
        (
            "a licence exception in a table of its own, with no comment",
            '[[licenses.exceptions]]\ncrate = "x"\nallow = ["NCSA"]\n',
            ["[licenses] exceptions[0] (x) has no comment saying why"],
        ),
        (
            "a licence exception whose crate is single-quoted, with a comment above it",
            "[licenses]\nexceptions = [\n    # vendored\n    { crate = 'x', allow = ['NCSA'] },\n]\n",
            [],
        ),
        (
            "a licence exception whose only # is inside a single-quoted string",
            "[licenses]\nexceptions = [\n    { crate = \"x\", allow = ['N#'] },\n]\n",
            ["[licenses] exceptions[0] (x) has no comment saying why"],
        ),
        (
            "a crate that is banned with a comment does not explain its licence exception",
            '[licenses]\nexceptions = [\n    { crate = "x", allow = ["NCSA"] },\n]\n'
            '[bans]\ndeny = [\n    # banned\n    { crate = "x", reason = "r" },\n]\n',
            ["[licenses] exceptions[0] (x) has no comment saying why"],
        ),
        (
            "a ban of the same crate does not make a commented licence exception fail",
            '[licenses]\nexceptions = [\n    # vendored\n    { crate = "x", allow = ["NCSA"] },\n]\n'
            '[bans]\ndeny = [\n    { crate = "x", reason = "r" },\n]\n',
            [],
        ),
        (
            "a licence allowance with no exceptions",
            '[licenses]\nallow = ["MIT"]\n',
            [],
        ),
        (
            "only the entry without a reason",
            f'[bans]\nskip = [{with_reason}, {{ crate = "c:2" }}]\n',
            ["[bans] skip[1] (c:2) has no reason"],
        ),
    )
    failed = 0
    for claim, text, expected in cases:
        got = list(problems(text))
        if got != expected:
            failed += 1
            print(f"selftest: {claim}, expected {expected!r}, got {got!r}", file=sys.stderr)
    if failed:
        return 1
    print(f"selftest: {len(cases)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Hold every exception in .config/deny.toml to a reason.")
    ap.add_argument("--file", type=Path, default=ROOT / ".config" / "deny.toml", help="the policy to read")
    ap.add_argument("--selftest", action="store_true", help="check the verdict on known inputs")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    try:
        found = list(problems(args.file.read_text(encoding="utf-8")))
    except (OSError, tomllib.TOMLDecodeError) as error:
        print(f"{args.file}: {error}", file=sys.stderr)
        return 1
    for line in found:
        print(f"{args.file}: {line}", file=sys.stderr)
    if found:
        print(
            ".config/deny.toml: write what ships anyway, or why the duplicate cannot be unified, in `reason`, "
            "and why a licence exception is allowed in a comment on its line or above it",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
