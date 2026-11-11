#!/usr/bin/env python3
"""Fails when fuzz/Cargo.toml's clippy lints drift from the root workspace's.

The root `Cargo.toml` holds the clippy lint policy in `[workspace.lints.clippy]`, and every member
crate takes it with `[lints] workspace = true`. `fuzz/Cargo.toml` declares its own `[workspace]`
table so the root workspace never compiles libFuzzer, and a package in a workspace of its own has
no outer table to inherit from, so it keeps a copy in `[lints.clippy]` instead. A copy drifts
silently from its original the moment one file changes and the other does not; this reads both and
fails on the first difference.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def clippy_lints(text, *path):
    """The table at `path` in `text` (for example `("workspace", "lints", "clippy")`), or `{}` if
    any step of `path` is absent."""
    node = tomllib.loads(text)
    for key in path:
        if not isinstance(node, dict):
            return {}
        node = node.get(key, {})
    return node if isinstance(node, dict) else {}


def problems(root_text, fuzz_text):
    """One line for each lint the two tables disagree on."""
    root = clippy_lints(root_text, "workspace", "lints", "clippy")
    fuzz = clippy_lints(fuzz_text, "lints", "clippy")
    for lint in sorted(set(root) | set(fuzz)):
        if lint not in fuzz:
            yield f"{lint!r} is in the root workspace's lints but not fuzz/Cargo.toml's"
        elif lint not in root:
            yield f"{lint!r} is in fuzz/Cargo.toml's lints but not the root workspace's"
        elif root[lint] != fuzz[lint]:
            yield f"{lint!r} is {root[lint]!r} in the root workspace and {fuzz[lint]!r} in fuzz/Cargo.toml"


def selftest():
    root = '[workspace.lints.clippy]\nunwrap_used = "deny"\ntodo = "deny"\n'
    cases = (
        ("identical tables", root, '[lints.clippy]\nunwrap_used = "deny"\ntodo = "deny"\n', []),
        (
            "fuzz is missing a lint",
            root,
            '[lints.clippy]\nunwrap_used = "deny"\n',
            ["'todo' is in the root workspace's lints but not fuzz/Cargo.toml's"],
        ),
        (
            "fuzz has an extra lint",
            root,
            '[lints.clippy]\nunwrap_used = "deny"\ntodo = "deny"\ndbg_macro = "deny"\n',
            ["'dbg_macro' is in fuzz/Cargo.toml's lints but not the root workspace's"],
        ),
        (
            "a level differs",
            root,
            '[lints.clippy]\nunwrap_used = "warn"\ntodo = "deny"\n',
            ["'unwrap_used' is 'deny' in the root workspace and 'warn' in fuzz/Cargo.toml"],
        ),
        ("both empty", "[workspace]\n", "[package]\n", []),
    )
    failed = 0
    for claim, root_text, fuzz_text, expected in cases:
        got = list(problems(root_text, fuzz_text))
        if got != expected:
            failed += 1
            print(f"selftest: {claim}, expected {expected!r}, got {got!r}", file=sys.stderr)
    if failed:
        return 1
    print(f"selftest: {len(cases)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(
        description="Fail when fuzz/Cargo.toml's clippy lints drift from the root workspace's."
    )
    ap.add_argument("--root", type=Path, default=ROOT / "Cargo.toml", help="the root manifest")
    ap.add_argument("--fuzz", type=Path, default=ROOT / "fuzz" / "Cargo.toml", help="the fuzz crate's manifest")
    ap.add_argument("--selftest", action="store_true", help="check the verdict on known inputs")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    try:
        found = list(
            problems(
                args.root.read_text(encoding="utf-8"),
                args.fuzz.read_text(encoding="utf-8"),
            )
        )
    except (OSError, tomllib.TOMLDecodeError) as error:
        print(error, file=sys.stderr)
        return 1
    for line in found:
        print(f"{args.fuzz}: {line}", file=sys.stderr)
    if found:
        print(
            f"{args.fuzz}: copy the root workspace's [workspace.lints.clippy] table into "
            "[lints.clippy] here so the two cannot drift apart unnoticed",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
