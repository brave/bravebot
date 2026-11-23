#!/usr/bin/env python3
"""Changes one token at a time in every function that carries a Verus contract, and fails on a
change Verus still verifies that `verify/survivors.txt` does not list.

`verify/mutations.txt` holds the defects a person thought of. This makes the ones nobody listed:
`==` and `!=` swapped, `&&` and `||`, `true` and `false`, a `!` dropped, `<` and `<=`, `>` and
`>=`, a trusted or public label made untrusted or private and back, and an `allow` ruling made an
`ask` and back. Each is made in the body of a function whose contract Verus proves, on a copy of
the crate in a temporary directory, so the tree is never edited.

A change Verus refuses shows the contract states what that token decides. A change that fails to
compile is no program and is not counted. A change Verus verifies survives, and fails the run
unless `verify/survivors.txt` lists it with the reason it is acceptable. Most survivors make a gate
stricter: a contract states that a gate never lets through what it should not, and a change that
lets through less still meets it. A listed change that no longer survives fails the run too, so
the list says what is true of the tree.

A survivor is four lines in `verify/survivors.txt`:

    survivor: <why it is acceptable>
    in: <path from the repository root>::<function>
    was: <the line, without its indentation>
    now: <the line after the change>

Attributes, comments, string literals and `proof!` blocks are not changed: none of them is code
that ships. Standard library only.
"""

import argparse
from collections import Counter
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
import queue

ROOT = Path(__file__).resolve().parent.parent
SURVIVORS = ROOT / "verify" / "survivors.txt"


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "contrib" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = _load("check_verus", "check-verus.py")
LEDGER = _load("check_verus_contracts", "check-verus-contracts.py")

# What one change is: a name, the token it matches, and what the token becomes.
CHANGES = (
    ("== to !=", r"(?<![=!<>])==(?!=)", "!="),
    ("!= to ==", r"!=", "=="),
    ("&& to ||", r"&&", "||"),
    ("|| to &&", r"\|\|", "&&"),
    ("true to false", r"\btrue\b", "false"),
    ("false to true", r"\bfalse\b", "true"),
    ("! dropped", r"(?<![!=\w)\]])!(?=[\w(])", ""),
    # A comparison has a space on each side, which a generic's `<` and `>` do not.
    ("< to <=", r"(?<=\s)<(?=\s)", "<="),
    ("<= to <", r"<=", "<"),
    ("> to >=", r"(?<=\s)>(?=\s)", ">="),
    (">= to >", r">=", ">"),
    ("trusted to untrusted", r"\bIntegrity::Trusted\b", "Integrity::Untrusted"),
    ("untrusted to trusted", r"\bIntegrity::Untrusted\b", "Integrity::Trusted"),
    ("public to private", r"\bConfidentiality::Public\b", "Confidentiality::Private"),
    ("private to public", r"\bConfidentiality::Private\b", "Confidentiality::Public"),
    ("allow to ask", r"\bRuling::Allow\b", "Ruling::Ask"),
    ("ask to allow", r"\bRuling::Ask\b", "Ruling::Allow"),
)
GHOST = re.compile(r"\bproof!\s*\{")
# A change Verus has not answered in this long is a run that went wrong, not a verdict.
SECONDS = 600


def _closing_brace(code, opened):
    """The index of the `}` that closes the `{` at `opened`."""
    depth = 0
    for at in range(opened, len(code)):
        if code[at] == "{":
            depth += 1
        elif code[at] == "}":
            depth -= 1
            if depth == 0:
                return at
    raise ValueError("unclosed brace")


def _attributes(code):
    """Each attribute in `code` as `(start, end, item)`, where `item` is the offset of the item it
    is on: the first character past it and any attributes after it."""
    found = []
    for opened in re.finditer(r"#!?\[", code):
        start = opened.start() if code[opened.start() + 1] == "[" else opened.start() + 1
        end = LEDGER._attribute_end(code, start)
        item = end
        while True:
            while item < len(code) and code[item].isspace():
                item += 1
            if code.startswith("#[", item):
                item = LEDGER._attribute_end(code, item)
                continue
            break
        found.append((opened.start(), end, item))
    return found


def functions(text):
    """Each function in `text` whose body Verus reads against a contract, as `(name, start, end)`
    with the offsets of its body between the braces. A function marked `external_body` has a
    contract Verus assumes, so its body is not one."""
    code = LEDGER.blanked(text)
    attributes = _attributes(code)
    on = {}
    for start, end, item in attributes:
        on.setdefault(item, []).append(code[start:end])
    found = []
    for item, held in sorted(on.items()):
        if not any(LEDGER.OPENING in attribute for attribute in held):
            continue
        if any("external_body" in attribute for attribute in held):
            continue
        signature = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?fn\s+(\w+)", code[item:])
        if signature is None:
            continue
        opened = code.index("{", item + signature.end())
        found.append((signature.group(1), opened + 1, _closing_brace(code, opened)))
    return found


def changes(path, text):
    """Every change to `text`, the source of the file at `path`, as a dict with `at`, `function`,
    `change`, `start`, `end`, `new`, `was` and `now`."""
    code = LEDGER.blanked(text)
    skipped = [(start, end) for start, end, _ in _attributes(code)]
    for ghost in GHOST.finditer(code):
        skipped.append((ghost.start(), _closing_brace(code, ghost.end() - 1) + 1))
    found = []
    for name, body, end in functions(text):
        for change, pattern, new in CHANGES:
            for match in re.finditer(pattern, code[body:end]):
                start, stop = body + match.start(), body + match.end()
                if any(low <= start < high for low, high in skipped):
                    continue
                line_start = text.rfind("\n", 0, start) + 1
                line_end = text.find("\n", start)
                line_end = len(text) if line_end < 0 else line_end
                found.append({
                    "at": f"{path}:{text.count(chr(10), 0, start) + 1}",
                    "function": f"{path}::{name}",
                    "change": change,
                    "start": start,
                    "end": stop,
                    "new": new,
                    "was": text[line_start:line_end].strip(),
                    "now": (text[line_start:start] + new + text[stop:line_end]).strip(),
                })
    return found


def parse(text):
    """The survivors `text` lists, as dicts with `survivor`, `in`, `was`, `now` and `line`."""
    found, current = [], {}
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip() or line.startswith("#"):
            continue
        key, _, value = line.partition(":")
        key, value = key.strip(), value.strip()
        if key == "survivor":
            if current:
                found.append(current)
            current = {"survivor": value, "line": number}
        elif key in ("in", "was", "now") and current:
            current[key] = value
        else:
            raise ValueError(f"line {number}: `{key}` is not a field of a survivor")
    if current:
        found.append(current)
    for survivor in found:
        missing = {"in", "was", "now"} - set(survivor)
        if missing:
            raise ValueError(f"line {survivor['line']}: no {', '.join(sorted(missing))}")
        if not survivor["survivor"]:
            raise ValueError(f"line {survivor['line']}: a survivor says why it is acceptable")
    return found


def key(entry):
    """What one change is the same change by, across edits that move it to another line."""
    return (entry.get("function") or entry["in"], entry["was"], entry["now"])


def problems(survived, listed):
    """One line for each change that survived and is not listed, and each listed one that did not
    survive."""
    unlisted = Counter(map(key, survived)) - Counter(map(key, listed))
    stale = Counter(map(key, listed)) - Counter(map(key, survived))
    for entry in survived:
        if unlisted[key(entry)]:
            unlisted[key(entry)] -= 1
            yield (
                f"survived, and {SURVIVORS.relative_to(ROOT)} does not list it: {entry['at']} "
                f"in {entry['function']} ({entry['change']})\n"
                f"    was: {entry['was']}\n    now: {entry['now']}"
            )
    for entry in listed:
        if stale[key(entry)]:
            stale[key(entry)] -= 1
            yield (
                f"{SURVIVORS.relative_to(ROOT)}:{entry['line']} lists a change that did not "
                f"survive, or is no longer made, in {entry['in']}\n"
                f"    was: {entry['was']}\n    now: {entry['now']}"
            )


def verdict(output):
    """`refused`, `survived`, or `no program` for Verus's output on a changed crate."""
    found = CHECK.RESULTS.search(output)
    if found is None:
        return "no program"
    return "refused" if int(found.group(2)) > 0 else "survived"


def run(verus_bin, scratch, crate, change):
    """The verdict on one change, made in the copy of the crate at `scratch`."""
    target = scratch / change["file"]
    text = target.read_text(encoding="utf-8")
    target.write_text(text[:change["start"]] + change["new"] + text[change["end"]:],
                      encoding="utf-8")
    try:
        result = subprocess.run(  # nosemgrep: dangerous-subprocess-use-audit
            [verus_bin, str(scratch / crate / "src" / "lib.rs"), "--crate-type=lib",
             "--crate-name", CHECK.CRATES[crate], "--edition", "2024"],
            capture_output=True,
            text=True,
            check=False,
            timeout=SECONDS,
        )
        return verdict(result.stdout + result.stderr)
    except subprocess.TimeoutExpired:
        return "undecided"
    finally:
        target.write_text(text, encoding="utf-8")


def selftest():
    failures = []
    source = (
        "impl X {\n"
        "    #[cfg_attr(verus_keep_ghost, verus_spec(r => ensures r == (a == b),))]\n"
        "    pub fn same(a: u8, b: u8) -> bool {\n"
        "        // a == b in a comment\n"
        "        let said = \"a == b\";\n"
        "        #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= n,))]\n"
        "        while i < n { i += 1; }\n"
        "        #[cfg(verus_keep_ghost)]\n"
        "        proof! { assert(a == a); }\n"
        "        let v: Vec<u8> = Vec::new();\n"
        "        a == b && !said.is_empty()\n"
        "    }\n"
        "    #[cfg_attr(verus_keep_ghost, verus_verify(external_body))]\n"
        "    #[cfg_attr(verus_keep_ghost, verus_spec(r => ensures r == a,))]\n"
        "    fn assumed(a: bool) -> bool { a == true }\n"
        "    fn uncontracted(a: bool) -> bool { a == true }\n"
        "}\n"
    )
    got = [(c["function"], c["change"], c["now"]) for c in changes("x.rs", source)]
    want = [
        ("x.rs::same", "< to <=", "while i <= n { i += 1; }"),
        ("x.rs::same", "== to !=", "a != b && !said.is_empty()"),
        ("x.rs::same", "&& to ||", "a == b || !said.is_empty()"),
        ("x.rs::same", "! dropped", "a == b && said.is_empty()"),
    ]
    if sorted(got) != sorted(want):
        failures.append(f"changes made {got!r}, not {want!r}")
    if [c["at"] for c in changes("x.rs", source) if c["change"] == "&& to ||"] != ["x.rs:11"]:
        failures.append("a change was not placed on the line it is on")
    listed = parse(
        "# a comment\n"
        "survivor: asks more\n"
        "in: x.rs::same\n"
        "was: a == b && !said.is_empty()\n"
        "now: a == b || !said.is_empty()\n"
    )
    if [key(entry) for entry in listed] != [
        ("x.rs::same", "a == b && !said.is_empty()", "a == b || !said.is_empty()")
    ]:
        failures.append(f"parse read {listed!r}")
    for broken in ("survivor: x\nin: y\nwas: z\n", "survivor:\nin: y\nwas: z\nnow: w\n",
                   "in: y\n"):
        try:
            parse(broken)
            failures.append(f"parse accepted {broken!r}")
        except ValueError:
            pass
    survived = [c for c in changes("x.rs", source) if c["change"] == "&& to ||"]
    if list(problems(survived, listed)):
        failures.append("a listed survivor was reported")
    if len(list(problems(survived, []))) != 1:
        failures.append("a survivor nobody listed was not reported")
    if len(list(problems([], listed))) != 1:
        failures.append("a listed change that did not survive was not reported")
    if len(list(problems(survived * 2, listed))) != 1:
        failures.append("a second survivor on a listed line was not reported")
    if verdict("verification results:: 3 verified, 1 errors") != "refused":
        failures.append("a reported error was not read as refused")
    if verdict("verification results:: 3 verified, 0 errors") != "survived":
        failures.append("a clean run was not read as a survivor")
    if verdict("error[E0308]: mismatched types") != "no program":
        failures.append("a failure to compile was read as a verdict")
    for failure in failures:
        print(f"check-verus-sweep selftest: {failure}", file=sys.stderr)
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true", help="check the reader and the list")
    parser.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2),
                        help="how many copies of the crate Verus runs on at once")
    args = parser.parse_args()
    if args.selftest:
        return selftest()

    verus_bin = os.environ.get("VERUS", "verus")
    if shutil.which(verus_bin) is None:
        print(f"check-verus-sweep: `{verus_bin}` is not on PATH; see "
              "docs/development/verification.md", file=sys.stderr)
        return 1
    listed = parse(SURVIVORS.read_text(encoding="utf-8"))

    made = []
    for crate in CHECK.CRATES:
        for path in sorted((ROOT / crate / "src").rglob("*.rs")):
            relative = path.relative_to(ROOT).as_posix()
            for change in changes(relative, path.read_text(encoding="utf-8")):
                made.append(dict(change, crate=crate, file=relative))

    scratches = []
    try:
        for _ in range(max(1, min(args.jobs, len(made)))):
            scratch = Path(tempfile.mkdtemp(prefix="check-verus-sweep-"))
            for part in CHECK.COPIED:
                shutil.copytree(ROOT / part, scratch / part)
            scratches.append(scratch)
        # The copy as it is has to verify, or every change would be refused for nothing.
        for crate in CHECK.CRATES:
            output, _ = CHECK.verus(verus_bin, scratches[0] / crate, CHECK.CRATES[crate])
            if verdict(output) != "survived":
                print(output, file=sys.stderr)
                print(f"check-verus-sweep: {crate} does not verify unchanged", file=sys.stderr)
                return 1
        free = queue.Queue()
        for scratch in scratches:
            free.put(scratch)

        def judge(change):
            scratch = free.get()
            try:
                return dict(change, verdict=run(verus_bin, scratch, change["crate"], change))
            finally:
                free.put(scratch)

        with ThreadPoolExecutor(len(scratches)) as pool:
            judged = list(pool.map(judge, made))
    finally:
        for scratch in scratches:
            shutil.rmtree(scratch, ignore_errors=True)

    counts = Counter(change["verdict"] for change in judged)
    print(f"check-verus-sweep: {len(judged)} changes: {counts['refused']} refused, "
          f"{counts['survived']} survived, {counts['no program']} no program, "
          f"{counts['undecided']} undecided", flush=True)
    found = [
        f"undecided after {SECONDS}s: {change['at']} in {change['function']} "
        f"({change['change']})\n    now: {change['now']}"
        for change in judged if change["verdict"] == "undecided"
    ]
    found += problems([change for change in judged if change["verdict"] == "survived"], listed)
    for line in found:
        print(f"check-verus-sweep: {line}", file=sys.stderr)
    if found:
        print(
            "check-verus-sweep: a change Verus verifies is one the contract does not state. "
            f"Strengthen the contract, or list it in {SURVIVORS.relative_to(ROOT)} with the reason "
            "it is acceptable",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
