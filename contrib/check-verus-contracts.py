#!/usr/bin/env python3
"""Fails when a Verus contract or assumption in `crates/` is not one `verify/contracts.txt` records.

A function with a contract carries `#[cfg_attr(verus_keep_ghost, verus_spec(...))]`, which a build
without Verus never reads, so deleting or weakening one compiles and passes every test. This reads
each contract out of the source and compares it with the ledger, so a contract that is removed,
added or changed is a line in a diff for a reviewer to read. `make check-verus` is what checks that
the code meets the contracts; this checks that the contracts are still the ones somebody agreed to,
and needs no toolchain.

It reads the assumptions the same way. `external_body` and `assume_specification` make Verus accept
a contract without reading the body, `assume` and `admit` make it accept a fact without proof, and
`external` makes it skip an item. Verus verifies a crate that has any of them, so each one is also a
line in the ledger. So is a `requires` in the contract of a function outside `verus!`: Verus checks
it only at a caller it verifies, and a caller it does not read, in the crate or outside it, can
break it while the crate verifies.

The ledger holds one line per contract, `path::function: clauses`, where `clauses` is the contract
with its whitespace collapsed, and one line per assumption, `path::item: assumes <what>`. `--write`
rewrites it from the tree.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "verify" / "contracts.txt"
HEADER = """\
# The Verus contracts on the code in crates/, one per line, as `path::function: clauses`, then what
# Verus is told to accept without proof, one per line, as `path::item: assumes <what>`.
#
# contrib/check-verus-contracts.py fails when a contract or an assumption in the tree differs from
# this file, so one that is removed, added or weakened shows in a diff. After changing one, run
# `python3 contrib/check-verus-contracts.py --write` and say in the pull request why it changed.
# docs/development/verification.md says how a contract is written and run.
"""
ASSUMPTIONS_HEADER = "# What Verus accepts without proof."

OPENING = "verus_spec("
FN_NAME = re.compile(r"\bfn\s+(\w+)")
LOOP = re.compile(r"^\s*(?:'\w+:\s*)?(?:while|loop|for)\b")

# A file holds Verus code when it names one of these.
VERUS_MARKERS = ("verus_keep_ghost", "verus!", "vstd::")
# The attribute arguments under which Verus reads a signature and not the body, takes a
# specification or a termination on trust, or skips an item.
ASSUMING = (
    "external_body",
    "external",
    "external_fn_specification",
    "external_type_specification",
    "external_trait_specification",
    "assume_termination",
    "assume_compatibility_with_trait_spec",
)
ATTRIBUTE_WORD = re.compile(r"\b(?:verifier::(\w+)|verus_verify\s*\(([^()]*)\))")
ITEM_NAME = re.compile(r"\b(?:fn|struct|enum|trait|union|type|mod)\s+(\w+)")
STATEMENT = re.compile(r"(?<![\w.])(assume|admit)\s*\(")
SPECIFICATION = re.compile(r"\bassume_specification\b\s*(?:<[^\[]*>)?\s*\[\s*([^\]]+?)\s*\]")
RAW_OPENING = re.compile(r'r(#*)"')
CHAR_LITERAL = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]*\}|.)|[^\\'\n])'")


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


def blanked(text):
    """`text` with every comment and the inside of every string and character literal replaced by
    spaces, keeping each newline, so an offset into it is the same offset into `text`."""
    out = list(text)
    at, end = 0, len(text)

    def blank(start, stop):
        for index in range(start, min(stop, end)):
            if out[index] != "\n":
                out[index] = " "

    while at < end:
        char = text[at]
        if text.startswith("//", at):
            stop = text.find("\n", at)
            stop = end if stop < 0 else stop
            blank(at, stop)
            at = stop
        elif text.startswith("/*", at):
            depth, scan = 1, at + 2
            while scan < end and depth:
                if text.startswith("/*", scan):
                    depth, scan = depth + 1, scan + 2
                elif text.startswith("*/", scan):
                    depth, scan = depth - 1, scan + 2
                else:
                    scan += 1
            blank(at, scan)
            at = scan
        elif char == "r" and RAW_OPENING.match(text, at) and (
            at == 0 or not (text[at - 1].isalnum() or text[at - 1] == "_") or text[at - 1] == "b"
        ):
            hashes = len(RAW_OPENING.match(text, at).group(1))
            opened = at + 2 + hashes
            closing = text.find('"' + "#" * hashes, opened)
            stop = end if closing < 0 else closing + 1 + hashes
            blank(opened, stop - 1 - hashes)
            at = stop
        elif char == '"':
            scan = at + 1
            while scan < end and text[scan] != '"':
                scan += 2 if text[scan] == "\\" else 1
            blank(at + 1, scan)
            at = scan + 1
        elif char == "'":
            # A character literal, `'x'` or `'\n'`; anything else is a lifetime or a label.
            literal = CHAR_LITERAL.match(text, at)
            if literal:
                blank(at + 1, at + len(literal.group(0)) - 1)
                at += len(literal.group(0))
            else:
                at += 1
        else:
            at += 1
    return "".join(out)


def _attribute_end(text, start):
    """The index just past the `]` that closes the attribute whose `#[` is at `start`."""
    depth, at = 0, start + 1
    while at < len(text):
        if text[at] in "([{":
            depth += 1
        elif text[at] in ")]}":
            depth -= 1
            if depth == 0:
                return at + 1
        at += 1
    return len(text)


def _item_after(text, at):
    """The name of the item that starts at or after `at`, past any further attributes."""
    while True:
        while at < len(text) and text[at].isspace():
            at += 1
        if text.startswith("#[", at) or text.startswith("#![", at):
            at = _attribute_end(text, text.index("[", at) - 1)
            continue
        break
    line = text[at:at + 400].split("{", 1)[0].split(";", 1)[0]
    named = ITEM_NAME.search(line)
    if named:
        return named.group(1)
    return " ".join(line.split()) or "?"


def assumptions(text):
    """Each assumption in `text` as `(item, what)`, in the order they appear. An `assume` or an
    `admit` is named after the function it is in."""
    code = blanked(text)
    found = []
    functions = [(match.start(), match.group(1)) for match in FN_NAME.finditer(code)]

    def function_before(at):
        before = [name for start, name in functions if start < at]
        return before[-1] if before else "?"
    for opened in re.finditer(r"#!?\[", code):
        start = opened.start() if code[opened.start() + 1] == "[" else opened.start() + 1
        end = _attribute_end(code, start)
        words = []
        for word in ATTRIBUTE_WORD.finditer(code, start, end):
            if word.group(1):
                words.append(word.group(1))
            else:
                words.extend(part.strip() for part in word.group(2).split(","))
        for what in words:
            if what in ASSUMING:
                item = "crate" if code[opened.start() + 1] == "!" else _item_after(code, end)
                found.append((opened.start(), item, what))
    for statement in STATEMENT.finditer(code):
        if re.search(r"\bfn\s+$", code[max(0, statement.start() - 16):statement.start()]):
            continue
        item = function_before(statement.start())
        found.append((statement.start(), item, statement.group(1)))
    for specification in SPECIFICATION.finditer(code):
        item = " ".join(specification.group(1).split())
        found.append((specification.start(), item, "assume_specification"))
    return [(item, what) for _, item, what in sorted(found)]


REQUIRES = re.compile(r"^(?:\w+\s*=>\s*)?requires\b")


def unchecked(found):
    """The functions of `found` whose contract holds a `requires`, which only a verified caller
    is checked against."""
    return [function for function, clauses in found if REQUIRES.match(clauses)]


def tree(root):
    """Every contract under `crates/`, then every assumption, as ledger lines."""
    lines, assumed = [], []
    for path in sorted((root / "crates").rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(root).as_posix()
        found = contracts(relative, text) if OPENING in text else []
        for function, clauses in found:
            lines.append(f"{relative}::{function}: {clauses}")
        if any(marker in text for marker in VERUS_MARKERS):
            for item, what in assumptions(text):
                assumed.append(f"{relative}::{item}: assumes {what}")
            for function in unchecked(found):
                assumed.append(f"{relative}::{function}: assumes requires")
    return lines, assumed


def recorded(text):
    """The ledger's lines, without its comments."""
    return [line for line in text.splitlines() if line.strip() and not line.startswith("#")]


def problems(in_tree, in_ledger):
    """One line for each contract or assumption the tree and the ledger disagree on, counting a
    line that is there twice as two."""
    extra = Counter(in_tree) - Counter(in_ledger)
    missing = Counter(in_ledger) - Counter(in_tree)
    for line in in_tree:
        if extra[line]:
            extra[line] -= 1
            yield f"not in {LEDGER.relative_to(ROOT)}: {line}"
    for line in in_ledger:
        if missing[line]:
            missing[line] -= 1
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
    assuming = (
        "// verifier::external_body in a comment, and assume(x) in one.\n"
        "/* #[verifier::external_body] in a block comment */\n"
        "#[derive(Debug)]\n"
        "#[cfg_attr(verus_keep_ghost, verus_verify, verifier::external_body)]\n"
        "pub struct Opaque { reason: &'static str }\n"
        "impl Opaque {\n"
        "    #[cfg_attr(verus_keep_ghost, verus_verify(external_body))]\n"
        "    /// A doc comment saying assume(x).\n"
        "    fn made(reason: &'static str) -> Self { Self { reason } }\n"
        "}\n"
        "#[cfg_attr(\n"
        "    verus_keep_ghost,\n"
        "    verus_verify,\n"
        "    verifier::external_body,\n"
        ")]\n"
        "#[cfg_attr(verus_keep_ghost, verus_verify(external))]\n"
        "pub struct Gate<'a, S: Sink> { sink: &'a mut S }\n"
        "fn admit(x: u8) {}\n"
        "fn not_an_assumption() { admit_named(2); let q = '\"'; let s = \"assume(\"; }\n"
        "verus! {\n"
        "#[verifier::external_type_specification]\n"
        "#[verifier::external_body]\n"
        "pub struct ExDenial(Denial);\n"
        "proof fn taken(x: int) ensures x > 0 { assume(x > 0); admit(); }\n"
        "pub assume_specification<T>[ std::mem::swap::<T> ](a: &mut T, b: &mut T);\n"
        "}\n"
    )
    got = assumptions(assuming)
    want = [
        ("Opaque", "external_body"),
        ("made", "external_body"),
        ("Gate", "external_body"),
        ("Gate", "external"),
        ("ExDenial", "external_type_specification"),
        ("ExDenial", "external_body"),
        ("taken", "assume"),
        ("taken", "admit"),
        ("std::mem::swap::<T>", "assume_specification"),
    ]
    if got != want:
        failures.append(f"assumptions read {got!r}, not {want!r}")
    required = [
        ("owed", "r => requires a > 0, ensures r,"),
        ("bare", "requires a > 0,"),
        ("same", "r => ensures r == (a == b),"),
        ("same/loop", "invariant i <= n,"),
        ("named", "r => ensures r.requires_nothing,"),
    ]
    if unchecked(required) != ["owed", "bare"]:
        failures.append(f"read {unchecked(required)!r} as the contracts holding a requires")
    once = ["x.rs::taken: assumes assume"]
    if len(list(problems(once * 2, once))) != 1:
        failures.append("a second assumption in the same function was not reported")
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
    in_tree, assumed = tree(ROOT)
    if args.write:
        LEDGER.write_text(
            HEADER + "\n" + "\n".join(in_tree) + "\n\n" + ASSUMPTIONS_HEADER + "\n"
            + "\n".join(assumed) + "\n",
            encoding="utf-8",
        )
        return 0
    found = list(problems(in_tree + assumed, recorded(LEDGER.read_text(encoding="utf-8"))))
    for line in found:
        print(f"check-verus-contracts: {line}", file=sys.stderr)
    if found:
        print(
            "check-verus-contracts: a Verus contract or assumption changed. If that is intended, "
            "run `python3 contrib/check-verus-contracts.py --write` and say why in the pull "
            "request",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
