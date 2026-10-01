#!/usr/bin/env python3
"""Holds every exception in deny.toml to a written reason.

cargo-deny treats `reason` as optional on an advisory to ignore, a duplicate to skip and a crate to
ban, and accepts a bare string in place of the table. An exception with no reason gives a reviewer
nothing to agree to, and an ignored advisory is one this binary ships with anyway. Whether a reason
is good is judged in review (DEP-001); that one is written is decided here.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
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


def problems(text):
    """One line for each exception in `text` with no reason."""
    config = tomllib.loads(text)
    for section, key in EXCEPTIONS:
        for index, entry in enumerate(config.get(section, {}).get(key, [])):
            reason = entry.get("reason") if isinstance(entry, dict) else None
            if not isinstance(reason, str) or not reason.strip():
                yield f"[{section}] {key}[{index}] ({name_of(entry)}) has no reason"


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
    ap = argparse.ArgumentParser(description="Hold every exception in deny.toml to a reason.")
    ap.add_argument("--file", type=Path, default=ROOT / "deny.toml", help="the policy to read")
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
            "deny.toml: write what ships anyway, or why the duplicate cannot be unified, in `reason`",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
