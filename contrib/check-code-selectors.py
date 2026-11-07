#!/usr/bin/env python3
"""Holds each variable in a CODE_SELECTORS list to a test that keeps it and one that removes it.

`gcloud`, `aws`, `ssh-keygen`, `ssh` and `git` are started with the person's environment, so a
variable their launchers read to choose code to run is taken off when its value names a path a
stage may write. Each program has a
CODE_SELECTORS list of those variables. A name added to a list with no test is a removal nothing
pins, and a test that checks only one value of it passes for a loop that never removes anything or
one that removes everything.

So this fails while a name in a list lacks either test in the same file:

- an assertion that the name is withheld (`assert!(withheld("NAME", ...))`), which a loop that
  keeps every value fails, and
- an assertion that it is kept for a safe value (`assert!(!withheld("NAME", ...))`), which a loop
  that removes every value fails.

It cannot tell that a list is complete. A variable the launcher reads that is not listed is found
by reading the launcher, which is what PATH-008 in docs/best-practices/paths.md asks a reviewer to
do. A program added to the sources below has the same check.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The files that hold a CODE_SELECTORS list, each with the tests for it in the same file.
SOURCES = [
    "crates/agent/src/gcloud.rs",
    "crates/bedrock/src/credentials.rs",
    "crates/cli/src/push_identity.rs",
]

LIST = re.compile(r"const\s+CODE_SELECTORS\b[^=]*=\s*\[(.*?)\]\s*;", re.DOTALL)
ENTRY = re.compile(
    r'\(\s*"([A-Za-z0-9_]+)"\s*,\s*Names::\w+\s*,?\s*\)'
    r"|\bselectors::([A-Z0-9_]+)"
)
# A name shared between lists, defined once in the sandbox crate: `pub const NAME: (&str, Names) =
# ("NAME", ...)`. A list names it as `selectors::NAME`.
SHARED_FILE = "crates/sandbox/src/programs.rs"
SHARED = re.compile(r'pub const ([A-Z0-9_]+): \(&str, Names\) = \(\s*"([A-Za-z0-9_]+)"')
TESTS = re.compile(
    r"#\[cfg\(\s*(?:test|all\(\s*test\b[^\]]*)\)\s*\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{"
)


def selectors(text):
    """The names in CODE_SELECTORS, or None where the list is not found.

    A parser that matches nothing would report every list covered, so an absent list and an empty
    one are both reported by the caller rather than passed.
    """
    found = LIST.search(text)
    if not found:
        return None
    return [a or b for a, b in ENTRY.findall(found.group(1))]


def options_in(text):
    """The names in CODE_SELECTORS that take options and not a path.

    An option string names nothing outside a stage-writable directory, so it is removed whatever it
    holds and has no value that is kept. Such a name needs the removal assertion only.
    """
    found = LIST.search(text)
    return re.findall(r'"([A-Za-z0-9_]+)"\s*,\s*Names::Options\b', found.group(1)) if found else []


def shared_mismatches(text):
    """The shared constants whose Rust name is not the variable they hold.

    A list names a shared entry by its constant, and this script takes that constant's name as the
    variable, so a constant holding another variable would be tested under the wrong name.
    """
    return [const for const, name in SHARED.findall(text) if const != name]


def tests_of(text):
    """The text of the test module, or None where there is none."""
    found = TESTS.search(text)
    return text[found.end():] if found else None


def asserts(tests, name, kept):
    """Whether a test asserts `withheld("name", ...)`, or `!withheld("name", ...)` where `kept`."""
    bang = r"!\s*" if kept else ""
    pattern = r"assert!\(\s*" + bang + r'withheld\(\s*"' + re.escape(name) + r'"'
    return re.search(pattern, tests) is not None


def verdict(text, source="the source file"):
    """Every way the list and the tests disagree, as complaints to print."""
    names = selectors(text)
    if names is None:
        return [f"CODE_SELECTORS is not in {source}. If it moved, teach check-code-selectors.py."]
    if not names:
        return [
            f"CODE_SELECTORS in {source} holds no name this script could read. If its shape "
            "changed, teach check-code-selectors.py."
        ]
    tests = tests_of(text)
    if tests is None:
        return [f"{source} has no test module, so no name in CODE_SELECTORS has a test."]
    complaints = []
    options = options_in(text)
    for name in names:
        if not asserts(tests, name, kept=False):
            complaints.append(
                f"{name} is in CODE_SELECTORS in {source} and no test asserts "
                f"withheld(\"{name}\", ...) for a value a stage may write."
            )
        if name not in options and not asserts(tests, name, kept=True):
            complaints.append(
                f"{name} is in CODE_SELECTORS in {source} and no test asserts "
                f"!withheld(\"{name}\", ...) for a safe value."
            )
    return complaints


COVERED = """\
const CODE_SELECTORS: [(&str, Names); 3] = [
    ("A_PROGRAM", Names::Program),
    ("A_DIRECTORY", Names::Directory),
    ("A_FILE", Names::File),
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
        assert!(withheld("A_FILE", &planted, &session));
        assert!(!withheld("A_FILE", &genuine, &session));
    }
}
"""

# The same list under the plain `#[cfg(test)]` a crate without a unix-only test module has.
COVERED_PLAIN = COVERED.replace("#[cfg(all(test, unix))]", "#[cfg(test)]")

# A list that takes one of its entries from the shared module.
SHARED_LIST = """\
const CODE_SELECTORS: [(&str, Names); 3] = [
    ("A_PROGRAM", Names::Program),
    selectors::A_SHARED,
    (
        "A_FILE",
        Names::File,
    ),
];
"""

# A list whose one name takes options, which has no value to keep.
OPTIONS_LIST = """\
const CODE_SELECTORS: [(&str, Names); 1] = [
    ("AN_OPTION", Names::Options),
];
#[cfg(test)]
mod tests {
    fn t() {
        assert!(withheld("AN_OPTION", &genuine, &session));
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
    case("parse: all names are read", got == ["A_PROGRAM", "A_DIRECTORY", "A_FILE"], got)
    got = selectors(SHARED_LIST)
    case(
        "parse: a shared entry is read as its constant's name",
        got == ["A_PROGRAM", "A_SHARED", "A_FILE"],
        got,
    )
    case(
        "shared: a constant named for its variable passes",
        shared_mismatches('pub const A_SHARED: (&str, Names) = ("A_SHARED", Names::File);') == [],
        None,
    )
    case(
        "shared: a constant holding another variable is found",
        shared_mismatches('pub const A_SHARED: (&str, Names) = ("OTHER", Names::File);')
        == ["A_SHARED"],
        None,
    )
    case(
        "verdict: an options name needs only the removal assertion",
        verdict(OPTIONS_LIST) == [],
        verdict(OPTIONS_LIST),
    )
    case(
        "verdict: an options name with no removal assertion fails once",
        len(verdict(OPTIONS_LIST.replace('assert!(withheld("AN_OPTION"', 'let _ = ("AN_OPTION"'))) == 1,
        verdict(OPTIONS_LIST.replace('assert!(withheld("AN_OPTION"', 'let _ = ("AN_OPTION"')),
    )
    case(
        "verdict: a path name is not excused for lacking a keep assertion",
        len(verdict(OPTIONS_LIST.replace("Names::Options", "Names::File"))) == 1,
        verdict(OPTIONS_LIST.replace("Names::Options", "Names::File")),
    )
    case("parse: a file with no list is not an empty list", selectors("fn main() {}") is None, None)
    case("verdict: a covered list passes", verdict(COVERED) == [], verdict(COVERED))
    case(
        "verdict: a covered list under a plain cfg(test) passes",
        verdict(COVERED_PLAIN) == [],
        verdict(COVERED_PLAIN),
    )

    new_one = '("A_DIRECTORY", Names::Directory),\n    ("A_NEW_ONE", Names::Program),'
    cases = [
        (
            "a name asserted only as kept fails once",
            COVERED.replace('assert!(withheld("A_PROGRAM"', 'assert!(!withheld("A_PROGRAM"'),
            1,
        ),
        (
            "a name asserted only as withheld fails once",
            COVERED.replace('assert!(!withheld("A_FILE"', 'assert!(withheld("A_FILE"'),
            1,
        ),
        (
            "a name added to the list with no test fails twice",
            COVERED.replace('("A_DIRECTORY", Names::Directory),', new_one),
            2,
        ),
        (
            "a test mentioning the name outside an assertion does not count",
            COVERED.replace(
                "    fn t() {",
                '    fn t() {\n        let _ = "A_NEW_ONE";',
            ).replace('("A_DIRECTORY", Names::Directory),', new_one),
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
                '    ("A_PROGRAM", Names::Program),\n    ("A_DIRECTORY", Names::Directory),\n'
                '    ("A_FILE", Names::File),\n',
                "",
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
    complaints = []
    total = 0
    shared = ROOT / SHARED_FILE
    if shared.exists():
        for const in shared_mismatches(shared.read_text(encoding="utf-8")):
            complaints.append(
                f"{const} in {SHARED_FILE} holds another variable's name, so a list naming "
                f"selectors::{const} would be tested under the wrong name."
            )
    else:
        complaints.append(f"{shared} is not there")
    for relative in SOURCES:
        source = ROOT / relative
        if not source.exists():
            complaints.append(f"{source} is not there")
            continue
        text = source.read_text(encoding="utf-8")
        complaints += verdict(text, relative)
        total += len(selectors(text) or [])
    for complaint in complaints:
        print(complaint, file=sys.stderr)
    if complaints:
        return 1
    print(
        f"check-code-selectors: {total} names in {len(SOURCES)} files, "
        "each with a keep and a remove test"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
