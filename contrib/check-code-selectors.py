#!/usr/bin/env python3
"""Holds each variable in gcloud.rs CODE_SELECTORS to a test that keeps it and one that removes it.

`gcloud` is started with the person's environment, so a variable its launcher reads to choose code
to run is taken off when its value names a path a stage may write. CODE_SELECTORS lists those
variables. A name added to the list with no test is a removal nothing pins, and a test that checks
only one value of it passes for a loop that never removes anything or one that removes everything.

So this fails while a name in the list lacks either test in the same file:

- an assertion that the name is withheld (`assert!(withheld("NAME", ...))`), which a loop that
  keeps every value fails, and
- an assertion that it is kept for a safe value (`assert!(!withheld("NAME", ...))`), which a loop
  that removes every value fails.

It cannot tell that the list is complete. A variable the launcher reads that is not listed is found
by reading the launcher, which is what PATH-008 in docs/best-practices/paths.md asks a reviewer to
do.
"""

import re
import sys
from pathlib import Path

SOURCE = Path(__file__).resolve().parent.parent / "crates/agent/src/gcloud.rs"

LIST = re.compile(r"const\s+CODE_SELECTORS\b[^=]*=\s*\[(.*?)\]\s*;", re.DOTALL)
ENTRY = re.compile(r'\(\s*"([A-Za-z0-9_]+)"\s*,\s*Names::\w+\s*,?\s*\)')
TESTS = re.compile(r"#\[cfg\(\s*all\(\s*test\b[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{")


def selectors(text):
    """The names in CODE_SELECTORS, or None where the list is not found.

    A parser that matches nothing would report every list covered, so an absent list and an empty
    one are both reported by the caller rather than passed.
    """
    found = LIST.search(text)
    if not found:
        return None
    return ENTRY.findall(found.group(1))


def tests_of(text):
    """The text of the test module, or None where there is none."""
    found = TESTS.search(text)
    return text[found.end():] if found else None


def asserts(tests, name, kept):
    """Whether a test asserts `withheld("name", ...)`, or `!withheld("name", ...)` where `kept`."""
    bang = r"!\s*" if kept else ""
    pattern = r"assert!\(\s*" + bang + r'withheld\(\s*"' + re.escape(name) + r'"'
    return re.search(pattern, tests) is not None


def verdict(text):
    """Every way the list and the tests disagree, as complaints to print."""
    names = selectors(text)
    if names is None:
        return ["CODE_SELECTORS is not in gcloud.rs. If it moved, teach check-code-selectors.py."]
    if not names:
        return [
            "CODE_SELECTORS holds no name this script could read. If its shape changed, "
            "teach check-code-selectors.py."
        ]
    tests = tests_of(text)
    if tests is None:
        return ["gcloud.rs has no test module, so no name in CODE_SELECTORS has a test."]
    complaints = []
    for name in names:
        if not asserts(tests, name, kept=False):
            complaints.append(
                f"{name} is in CODE_SELECTORS and no test asserts withheld(\"{name}\", ...) "
                f"for a value a stage may write."
            )
        if not asserts(tests, name, kept=True):
            complaints.append(
                f"{name} is in CODE_SELECTORS and no test asserts !withheld(\"{name}\", ...) "
                f"for a safe value."
            )
    return complaints


COVERED = """\
const CODE_SELECTORS: [(&str, Names); 2] = [
    ("A_PROGRAM", Names::Program),
    ("A_DIRECTORY", Names::Directory),
];
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, reason = "test helpers unwrap")]
mod tests {
    fn t() {
        assert!(withheld("A_PROGRAM", &planted, &session));
        assert!(!withheld("A_PROGRAM", &genuine, &session));
        assert!(withheld(
            "A_DIRECTORY",
            &planted,
            &session
        ));
        assert!(!withheld(
            "A_DIRECTORY",
            &genuine,
            &session
        ));
    }
}
"""


def selftest():
    """Prove the parse and the verdict on inputs whose answer is known.

    A gate that is quietly wrong is worse than none, and this one's claim is that a listed name
    cannot lose its tests unnoticed. Each case is one way the claim could fail to hold.
    """
    checks = []

    def case(claim, held, got):
        checks.append((claim, held, got))

    got = selectors(COVERED)
    case("parse: both names are read", got == ["A_PROGRAM", "A_DIRECTORY"], got)
    case("parse: a file with no list is not an empty list", selectors("fn main() {}") is None, None)
    case("verdict: a covered list passes", verdict(COVERED) == [], verdict(COVERED))

    cases = [
        (
            "a name asserted only as kept fails once",
            COVERED.replace('assert!(withheld("A_PROGRAM"', 'assert!(!withheld("A_PROGRAM"'),
            1,
        ),
        (
            "a name asserted only as withheld fails once",
            COVERED.replace('assert!(!withheld("A_PROGRAM"', 'assert!(withheld("A_PROGRAM"'),
            1,
        ),
        (
            "a name added to the list with no test fails twice",
            COVERED.replace(
                '("A_DIRECTORY", Names::Directory),',
                '("A_DIRECTORY", Names::Directory),\n    ("A_NEW_ONE", Names::Program),',
            ),
            2,
        ),
        (
            "a test mentioning the name outside an assertion does not count",
            COVERED.replace(
                "    fn t() {",
                '    fn t() {\n        let _ = "A_NEW_ONE";',
            ).replace(
                '("A_DIRECTORY", Names::Directory),',
                '("A_DIRECTORY", Names::Directory),\n    ("A_NEW_ONE", Names::Program),',
            ),
            2,
        ),
        (
            "a list with no test module fails",
            COVERED.split("#[cfg(all(test")[0],
            1,
        ),
        (
            "an empty list fails",
            COVERED.replace(
                '    ("A_PROGRAM", Names::Program),\n    ("A_DIRECTORY", Names::Directory),\n', ""
            ),
            1,
        ),
    ]
    for claim, text, expected in cases:
        got = verdict(text)
        case(f"verdict: {claim}", len(got) == expected, got)

    broke = [(claim, got) for claim, held, got in checks if not held]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


def main():
    if "--selftest" in sys.argv[1:]:
        return selftest()
    if not SOURCE.exists():
        print(f"{SOURCE} is not there", file=sys.stderr)
        return 1
    text = SOURCE.read_text(encoding="utf-8")
    complaints = verdict(text)
    for complaint in complaints:
        print(complaint, file=sys.stderr)
    if complaints:
        return 1
    print(f"check-code-selectors: {len(selectors(text))} names, each with a keep and a remove test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
