#!/usr/bin/env python3
"""Runs Verus on the crates that carry contracts, and shows that each contract can fail.

`make check-verus` runs this. For each crate in `CRATES` it runs Verus once on the tree as it is,
which has to verify, and then once for each mutation in `verify/mutations.txt` that edits a file of
that crate, which has to fail verification. A mutation restores a defect a contract exists to
refuse, such as a gate that releases untrusted content, on a copy of the crate in a temporary
directory, so the tree is never edited. A contract that the mutated code also met would prove
nothing, and a mutation whose text is no longer in the file means the code moved under it; both
fail the run.

A mutation is three lines in `verify/mutations.txt`:

    mutation: <what the defect is>
    file: <path from the repository root>
    replace: <one line of the file, exactly> => <what it becomes>

Standard library only.
"""

import argparse
import os
import re
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MUTATIONS = ROOT / "verify" / "mutations.txt"

# The crates Verus reads, by directory, with the name rustc gives each. `bravebot-core` includes a
# file from `ui-files` by path, so the copy a mutation runs in holds that crate too.
CRATES = {"crates/core": "bravebot_core"}
COPIED = ("crates/core", "crates/ui-files/src")

RESULTS = re.compile(r"^verification results:: (\d+) verified, (\d+) errors", re.M)


def parse(text):
    """The mutations in `text`, as dicts with `mutation`, `file`, `old` and `new`."""
    found, current = [], {}
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip() or line.startswith("#"):
            continue
        key, _, value = line.partition(":")
        key, value = key.strip(), value.strip()
        if key == "mutation":
            if current:
                found.append(current)
            current = {"mutation": value, "line": number}
        elif key == "file":
            current["file"] = value
        elif key == "replace":
            old, arrow, new = value.partition(" => ")
            if not arrow:
                raise ValueError(f"line {number}: `replace:` has no ` => `")
            current["old"], current["new"] = old, new
        else:
            raise ValueError(f"line {number}: `{key}` is not a field of a mutation")
    if current:
        found.append(current)
    for mutation in found:
        missing = {"file", "old", "new"} - set(mutation)
        if missing:
            raise ValueError(f"line {mutation['line']}: no {', '.join(sorted(missing))}")
    return found


def apply(text, old, new):
    """`text` with the one line equal to `old`, after its indentation, replaced by `new` at the same
    indentation, or `None` where no line or more than one is."""
    lines = text.split("\n")
    at = [index for index, line in enumerate(lines) if line.strip() == old]
    if len(at) != 1:
        return None
    line = lines[at[0]]
    lines[at[0]] = line[: len(line) - len(line.lstrip())] + new
    return "\n".join(lines)


def verus(verus_bin, crate_dir, name):
    """Verus's output and exit status for the crate at `crate_dir`."""
    run = subprocess.run(  # nosemgrep: dangerous-subprocess-use-audit
        [verus_bin, str(crate_dir / "src" / "lib.rs"), "--crate-type=lib", "--crate-name", name,
         "--edition", "2024"],
        capture_output=True,
        text=True,
        check=False,
    )
    return run.stdout + run.stderr, run.returncode


def refused(output):
    """Whether Verus ran and reported at least one verification error."""
    found = RESULTS.search(output)
    return found is not None and int(found.group(2)) > 0


def selftest():
    failures = []
    text = (
        "# a comment\n"
        "mutation: meet keeps a result trusted\n"
        "file: crates/core/src/label.rs\n"
        "replace: if a || b { => if a && b {\n"
    )
    got = parse(text)
    want = [{"mutation": "meet keeps a result trusted", "line": 2,
             "file": "crates/core/src/label.rs", "old": "if a || b {", "new": "if a && b {"}]
    if got != want:
        failures.append(f"parse read {got!r}")
    if apply("    if a || b {\n    }", "if a || b {", "if a && b {") != "    if a && b {\n    }":
        failures.append("apply did not keep the indentation")
    if apply("x\nx", "x", "y") is not None:
        failures.append("apply edited a line that is in the file twice")
    if apply("x", "z", "y") is not None:
        failures.append("apply edited a file that does not hold the line")
    if not refused("verification results:: 3 verified, 1 errors"):
        failures.append("a reported error was not read as refused")
    if refused("verification results:: 3 verified, 0 errors"):
        failures.append("a clean run was read as refused")
    if refused("error[E0425]: cannot find function"):
        failures.append("a failure to compile was read as refused")
    for failure in failures:
        print(f"check-verus selftest: {failure}", file=sys.stderr)
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true", help="check the parser and the reader")
    args = parser.parse_args()
    if args.selftest:
        return selftest()

    verus_bin = os.environ.get("VERUS", "verus")
    if shutil.which(verus_bin) is None:
        print(f"check-verus: `{verus_bin}` is not on PATH; see docs/development/verification.md",
              file=sys.stderr)
        return 1

    status = 0
    for crate, name in CRATES.items():
        output, code = verus(verus_bin, ROOT / crate, name)
        found = RESULTS.search(output)
        if code != 0 or found is None or int(found.group(2)) != 0:
            print(output, file=sys.stderr)
            print(f"check-verus: {crate} does not verify", file=sys.stderr)
            return 1
        print(f"check-verus: {crate}: {found.group(1)} verified, 0 errors")

    for mutation in parse(MUTATIONS.read_text(encoding="utf-8")):
        crate = next((c for c in CRATES if mutation["file"].startswith(c + "/")), None)
        if crate is None:
            print(f"check-verus: {mutation['file']} is in no crate Verus reads", file=sys.stderr)
            status = 1
            continue
        with tempfile.TemporaryDirectory() as scratch:
            scratch = Path(scratch)
            for part in COPIED:
                shutil.copytree(ROOT / part, scratch / part)
            target = scratch / mutation["file"]
            edited = apply(target.read_text(encoding="utf-8"), mutation["old"], mutation["new"])
            if edited is None:
                print(
                    f"check-verus: `{mutation['old']}` is not one line of {mutation['file']}, so "
                    f"the mutation `{mutation['mutation']}` tests nothing",
                    file=sys.stderr,
                )
                status = 1
                continue
            target.write_text(edited, encoding="utf-8")
            output, _ = verus(verus_bin, scratch / crate, CRATES[crate])
        if refused(output):
            print(f"check-verus: refused, as it must be: {mutation['mutation']}")
        else:
            print(output, file=sys.stderr)
            print(f"check-verus: not refused: {mutation['mutation']}", file=sys.stderr)
            status = 1
    return status


if __name__ == "__main__":
    sys.exit(main())
