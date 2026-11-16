#!/usr/bin/env python3
"""Fails when a Verus contract in `crates/` is not the one `verify/contracts.txt` records.

A function with a contract carries `#[cfg_attr(verus_keep_ghost, verus_spec(...))]`, which a build
without Verus never reads, so deleting or weakening one compiles and passes every test. This reads
each contract out of the source and compares it with the ledger, so a contract that is removed,
added or changed is a line in a diff for a reviewer to read. `make check-verus` is what checks that
the code meets the contracts; this checks that the contracts are still the ones somebody agreed to,
and needs no toolchain.

The ledger holds one line per contract, `path::function: clauses`, where `clauses` is the contract
with its whitespace collapsed. `--write` rewrites it from the tree.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "verify" / "contracts.txt"
HEADER = """\
# The Verus contracts on the code in crates/, one per line, as `path::function: clauses`.
#
# contrib/check-verus-contracts.py fails when a contract in the tree differs from this file, so a
# contract that is removed, added or weakened shows in a diff. After changing one, run
# `python3 contrib/check-verus-contracts.py --write` and say in the pull request why the contract
# changed. docs/development/verification.md says how a contract is written and run.
"""

OPENING = "verus_spec("
FN_NAME = re.compile(r"\bfn\s+(\w+)")
LOOP = re.compile(r"^\s*(?:'\w+:\s*)?(?:while|loop|for)\b")


def _closing(text, start):
    """The index just past the parenthesis that closes the one opened before `start`."""
    depth = 1
    at = start
    while at < len(text):
        char = text[at]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return at + 1
        at += 1
    raise ValueError("unclosed verus_spec")


def contracts(path, text):
    """Each contract in `text` as `(function, clauses)`, in the order they appear. A contract on a
    loop has no function of its own and is named after the function it is in, with `/loop`."""
    found = []
    at = 0
    while True:
        start = text.find(OPENING, at)
        if start < 0:
            return found
        end = _closing(text, start + len(OPENING))
        clauses = " ".join(text[start + len(OPENING):end - 1].split())
        # The item the attribute is on is the first line after it that is not an attribute, a
        # comment or the rest of the attribute's own line.
        rest = text[end:].split("\n", 1)[1] if "\n" in text[end:] else ""
        target = next(
            (
                line
                for line in rest.splitlines()
                if line.strip() and not line.lstrip().startswith(("#", "//"))
            ),
            "",
        )
        if LOOP.match(target):
            before = list(FN_NAME.finditer(text, 0, start))
            function = (before[-1].group(1) if before else "?") + "/loop"
        else:
            named = FN_NAME.search(target)
            function = named.group(1) if named else "?"
        found.append((function, clauses))
        at = end


def tree(root):
    """Every contract under `crates/`, as ledger lines."""
    lines = []
    for path in sorted((root / "crates").rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if OPENING not in text:
            continue
        relative = path.relative_to(root).as_posix()
        for function, clauses in contracts(relative, text):
            lines.append(f"{relative}::{function}: {clauses}")
    return lines


def recorded(text):
    """The ledger's lines, without its comments."""
    return [line for line in text.splitlines() if line.strip() and not line.startswith("#")]


def problems(in_tree, in_ledger):
    """One line for each contract the tree and the ledger disagree on."""
    tree_set, ledger_set = set(in_tree), set(in_ledger)
    for line in in_tree:
        if line not in ledger_set:
            yield f"not in {LEDGER.relative_to(ROOT)}: {line}"
    for line in in_ledger:
        if line not in tree_set:
            yield f"in {LEDGER.relative_to(ROOT)} and not in the tree: {line}"


def selftest():
    source = (
        "impl X {\n"
        "    #[cfg_attr(verus_keep_ghost, verus_spec(r =>\n"
        "        ensures\n"
        "            r == (a == b),\n"
        "    ))]\n"
        "    pub fn same(a: u8, b: u8) -> bool {\n"
        "        #[cfg_attr(verus_keep_ghost, verus_spec(\n"
        "            invariant i <= n,\n"
        "        ))]\n"
        "        while i < n { i += 1; }\n"
        "        a == b\n"
        "    }\n"
        "    /// A doc comment between a contract and its function.\n"
        "    #[cfg_attr(verus_keep_ghost, verus_spec(r => ensures r,))]\n"
        "    /// Another.\n"
        "    fn after_a_comment() -> bool { true }\n"
        "}\n"
    )
    got = contracts("x.rs", source)
    want = [
        ("same", "r => ensures r == (a == b),"),
        ("same/loop", "invariant i <= n,"),
        ("after_a_comment", "r => ensures r,"),
    ]
    failures = []
    if got != want:
        failures.append(f"contracts read {got!r}, not {want!r}")
    ledger = ["x.rs::same: r => ensures r == (a == b),"]
    weakened = ["x.rs::same: r => ensures true,"]
    if list(problems(ledger, ledger)):
        failures.append("a tree that matches its ledger was reported")
    if len(list(problems(weakened, ledger))) != 2:
        failures.append("a weakened contract was not reported as one removed and one added")
    if len(list(problems([], ledger))) != 1:
        failures.append("a removed contract was not reported")
    for failure in failures:
        print(f"check-verus-contracts selftest: {failure}", file=sys.stderr)
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true", help="check the reader")
    parser.add_argument("--write", action="store_true", help="rewrite the ledger from the tree")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    in_tree = tree(ROOT)
    if args.write:
        LEDGER.write_text(HEADER + "\n" + "\n".join(in_tree) + "\n", encoding="utf-8")
        return 0
    found = list(problems(in_tree, recorded(LEDGER.read_text(encoding="utf-8"))))
    for line in found:
        print(f"check-verus-contracts: {line}", file=sys.stderr)
    if found:
        print(
            "check-verus-contracts: a Verus contract changed. If that is intended, run "
            "`python3 contrib/check-verus-contracts.py --write` and say why in the pull request",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
