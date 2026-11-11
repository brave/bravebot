#!/usr/bin/env python3
"""Holds every `programs::start` call and every CODE_SELECTORS name to what SANDBOX-30 requires.

A program bravebot starts reads variables that name code to run (`LD_PRELOAD`, `PYTHONPATH`,
`CLOUDSDK_PYTHON`, `NODE_OPTIONS`, and so on). `start` and `start_against` take a fourth argument,
`CodeSelectors::Kept` or `CodeSelectors::Checked(&list)`, so a call site says whether those
variables are taken off when their value names a path a stage may write. `gcloud`, `aws`,
`ssh-keygen`, `ssh`, `git`, `systemctl`, `systemd-inhibit` and `wl-copy` each pass a CODE_SELECTORS
list of their own, and the generic list
every checked start removes is in crates/sandbox/src/programs.rs. This script fails for:

1. A `programs::start` or `programs::start_against` call outside crates/sandbox/src/programs.rs
   whose fourth argument is not written as `CodeSelectors::Kept` or
   `CodeSelectors::Checked(&[...])` at the call. A value held in a variable or chosen by a branch
   hides the classification from a reader.
2. A `CodeSelectors::Kept` call in code that is not a test with no comment in its argument list
   saying why the program may run what the environment names.
3. A `CodeSelectors::Checked(&NAME)` call in a file that does not define `const CODE_SELECTORS`.
4. A file that must hold a CODE_SELECTORS list (the generic list and the three above) and does
   not, or whose list this script reads as empty.
5. A name in a `const CODE_SELECTORS` list, or in a `const <PROGRAM>_CODE_SELECTORS` list a file
   keeps for one program, in any file, that lacks a test in the same file:
   - an assertion that the name is removed (`assert!(withheld("NAME", ...))` or
     `assert!(checked("NAME", ...))`), which a loop that keeps every value fails, and
   - for a name whose value is a path, program or list, an assertion that a safe value is kept
     (`assert!(!withheld("NAME", ...))` or `assert!(!checked(...))`), which a loop that removes
     every value fails. A name whose value is a set of options is removed whatever it holds, so it
     has no keep test.
6. A constant in `selectors` whose Rust name is not the variable it holds, since a list names it
   as `selectors::NAME` and this script takes the constant's name as the variable.
7. A `CodeSelectors::Checked(&[])` call outside a test that has no comment in its argument list, or
   whose file has no row in the table of docs/specs/sandboxing.md that follows the line
   `<!-- empty-list-starts -->`. Such a start removes the generic names and no name of its own, so
   the table says what the program reads and why none of it selects code, or which issue tracks
   it. The row is what a reviewer reads; this script checks that it exists, not that it is right.
8. `LOADER_PREFIXES` in crates/sandbox/src/programs.rs, which the stored-login `gh` start of the
   usability suite removes, held to what docs/development/checks.md describes.
9. `HOST_VARIABLES` in crates/agent/src/usability.rs, held to the same description.
10. A difference between the browser opener's list in crates/tui/src/opener.rs and the two lists
   of the same names in the desktop application: `OPENER_SELECTORS` in
   ui/src/main/external-open.ts and `NAMES` in ui/scripts/external-open-electron.test.mjs. The
   application deletes those names from its own environment before it hands an address, a file
   or a folder to the system, so a name added to the Rust list and not there is passed on by the
   application. Three differences are written down: `HOME`, `XDG_RUNTIME_DIR` and
   `DBUS_SESSION_BUS_ADDRESS` are in the Rust list and are not in the others. The application and
   the agent it starts read them, and Electron reaches the session bus through the last two, so the
   application leaves them set. Without `HOME`, `gio` also reads the account's passwd home (the
   comment at the top of external-open.ts and SANDBOX-30 give the reasons). The ten Windows names
   of the rundll32 start are written down the same way. `APPLICATION_EXCEPTIONS` holds them.

It cannot tell that a list is complete, or that a `Kept` call is right. A variable the program
reads that is not listed is found by reading the program, which PATH-008 in
docs/best-practices/paths.md asks a reviewer to do.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"

# The browser opener's list and the two lists in the desktop application that repeat its names.
OPENER = "crates/tui/src/opener.rs"
APPLICATION = "ui/src/main/external-open.ts"
APPLICATION_TEST = "ui/scripts/external-open-electron.test.mjs"

# Names in the opener's list that the desktop application's lists leave out, and why. The
# application and the agent it starts read `HOME`, so deleting it from the application's
# environment would change both, and without `HOME` `gio` reads the account's passwd home, so
# deleting it protects nothing when the real home is the place a stage may write (SANDBOX-30).
# The opener removes it from its own start instead. Electron finds the session bus from
# `DBUS_SESSION_BUS_ADDRESS` and `XDG_RUNTIME_DIR` for its own file manager, portal and
# notification calls, so deleting them would break those. The ten Windows
# names (brave/bravebot#3860) are also left out of the application's list and removed by the
# opener from its own start. Electron's own Windows start is not covered by that removal: it is
# given those names as the application's process holds them.
WINDOWS_REASON = (
    "the opener removes this Windows variable from its own start; the desktop application's list "
    "does not hold it"
)
APPLICATION_EXCEPTIONS = {
    "HOME": (
        "without HOME, gio reads the account's passwd home, and the desktop application and the "
        "agent it starts read HOME, so it is left set there (SANDBOX-30)"
    ),
    "XDG_RUNTIME_DIR": "Electron and the agent it starts find the session bus and their runtime files here",
    "DBUS_SESSION_BUS_ADDRESS": "Electron connects to the session bus it names, so it is not deleted there",
    "PATHEXT": WINDOWS_REASON,
    "COMSPEC": WINDOWS_REASON,
    "SystemRoot": WINDOWS_REASON,
    "windir": WINDOWS_REASON,
    "ProgramFiles": WINDOWS_REASON,
    "ProgramW6432": WINDOWS_REASON,
    "ProgramFiles(x86)": WINDOWS_REASON,
    "LOCALAPPDATA": WINDOWS_REASON,
    "APPDATA": WINDOWS_REASON,
    "USERPROFILE": WINDOWS_REASON,
}

# The file that holds the generic list and the shared names.
PROGRAMS = "crates/sandbox/src/programs.rs"

# The usability suite, which holds `HOST_VARIABLES` and starts the stored-login `gh` with
# `LOADER_PREFIXES`.
USABILITY = "crates/agent/src/usability.rs"

# The files that hold a program's own CODE_SELECTORS list, each with the tests for it in the same
# file.
SOURCES = [
    "crates/agent/src/gcloud.rs",
    "crates/agent/src/usability.rs",
    "crates/bedrock/src/credentials.rs",
    "crates/cli/src/push_identity.rs",
    "crates/schedule/src/schedule.rs",
    "crates/tui/src/opener.rs",
    "crates/tui/src/powershell.rs",
]

# A program's list is `const CODE_SELECTORS`, and a file that gives a second program a list of its own
# names it `const <PROGRAM>_CODE_SELECTORS`. Every such list in a file is read.
LIST = re.compile(r"const\s+(?:\w+_)?CODE_SELECTORS\b[^=]*=\s*&?\[(.*?)\]\s*;", re.DOTALL)
# A variable name is letters, digits and underscores, and may hold parentheses: Windows sets
# `ProgramFiles(x86)`.
NAME = r"[A-Za-z0-9_()]+"
ENTRY = re.compile(
    r'\(\s*"(' + NAME + r')"\s*,\s*Names::\w+\s*,?\s*\)'
    r"|\bselectors::([A-Z0-9_]+)"
)
# A name shared between lists, defined once in the sandbox crate: `pub const NAME: (&str, Names) =
# ("NAME", ...)`. A list names it as `selectors::NAME`.
SHARED = re.compile(r'pub const ([A-Z0-9_]+): \(&str, Names\) = \(\s*"(' + NAME + r')"')
TESTS = re.compile(
    r"#\[cfg\(\s*(?:test|all\(\s*test\b[^\]]*)\)\s*\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{"
)
CALL = re.compile(r"programs::start(?:_against)?\(")
BARE_CALL = re.compile(r"(?<![\w.:])start(?:_against)?\(")
BARE_IMPORT = re.compile(
    r"^\s*use\s+[\w:]*programs::(?:start(?:_against)?\b|\{[^}]*\bstart(?:_against)?\b)",
    re.MULTILINE,
)
FOURTH = re.compile(r"^(?:[\w:]*::)?CodeSelectors::(Kept|Checked\(\s*&(.*?)\s*\))$", re.DOTALL)
FOLLOWS_TEST_ATTRIBUTE = re.compile(r"#\[cfg\(\s*(?:all\(\s*)?test\b")

# The spec file and the line above the table that holds a row for each file with a production start
# that passes no list of its own.
SPEC = "docs/specs/sandboxing.md"
EMPTY_LIST_MARKER = "<!-- empty-list-starts -->"


def listed_in_spec(spec, label):
    """Whether the table after the marker in `spec` has a row that names the file `label`.

    The table is the lines that start with `|` straight after the marker line and its blank line.
    A file named anywhere else in the spec does not count, since the row is where the start's
    reason is written.
    """
    lines = [line.strip() for line in spec.splitlines()]
    if EMPTY_LIST_MARKER not in lines:
        return False
    rows = []
    for line in lines[lines.index(EMPTY_LIST_MARKER) + 1 :]:
        if line.startswith("|"):
            rows.append(line)
        elif rows:
            break
    return any(f"`{label}`" in row for row in rows)


def selectors(text):
    """The names in every CODE_SELECTORS list of a file, or None where there is none.

    A parser that matches nothing would report every list covered, so an absent list and an empty
    one are both reported by the caller rather than passed.
    """
    found = list(LIST.finditer(text))
    if not found:
        return None
    return [a or b for match in found for a, b in ENTRY.findall(match.group(1))]


APPLICATION_LIST = re.compile(
    r"export\s+const\s+OPENER_SELECTORS\b[^=]*=\s*\[(.*?)\]", re.DOTALL
)
APPLICATION_TEST_LIST = re.compile(r"\bconst\s+NAMES\s*=\s*\[(.*?)\]", re.DOTALL)
QUOTED = re.compile(r"""['"](""" + NAME + r""")['"]""")


def quoted_names(pattern, text):
    """The quoted names in the list `pattern` finds in `text`, or None where it finds no list."""
    found = pattern.search(text)
    return QUOTED.findall(found.group(1)) if found else None


def application_complaints(opener_text, application_text, application_test_text):
    """Every way the application's lists of opener names and the opener's own list differ.

    The names must be the same set, apart from the ones in APPLICATION_EXCEPTIONS, which must be
    in the opener's list and in neither of the application's. A list this script cannot find or
    reads as empty is reported, since an absent list would otherwise agree with nothing.
    """
    rust = selectors(opener_text)
    if not rust:
        return [f"CODE_SELECTORS is not readable in {OPENER}, so it cannot be compared with {APPLICATION}."]
    expected = set(rust) - set(APPLICATION_EXCEPTIONS)
    complaints = []
    for name in APPLICATION_EXCEPTIONS:
        if name not in rust:
            complaints.append(
                f"{name} is an exception in APPLICATION_EXCEPTIONS and is not in CODE_SELECTORS "
                f"in {OPENER}. Remove the exception from check-code-selectors.py."
            )
    for label, pattern, text, what in [
        (APPLICATION, APPLICATION_LIST, application_text, "OPENER_SELECTORS"),
        (APPLICATION_TEST, APPLICATION_TEST_LIST, application_test_text, "NAMES"),
    ]:
        found = quoted_names(pattern, text)
        if not found:
            complaints.append(
                f"{what} is not readable in {label}. If its shape changed, teach "
                "check-code-selectors.py."
            )
            continue
        if len(found) != len(set(found)):
            complaints.append(f"{what} in {label} names a variable twice: {found}.")
        missing = sorted(expected - set(found))
        extra = sorted(set(found) - expected)
        if missing:
            complaints.append(
                f"{what} in {label} lacks {', '.join(missing)}, which CODE_SELECTORS in {OPENER} "
                "lists, so the application passes it to the opener."
            )
        if extra:
            complaints.append(
                f"{what} in {label} names {', '.join(extra)}, which CODE_SELECTORS in {OPENER} "
                "does not list. Add it there, or remove it here."
            )
    return complaints


def options_in(text):
    """The names in the CODE_SELECTORS lists that take options and not a path.

    An option string names nothing outside a stage-writable directory, so it is removed whatever it
    holds and has no value that is kept. Such a name needs the removal assertion only.
    """
    return [
        name
        for match in LIST.finditer(text)
        for name in re.findall(r'"(' + NAME + r')"\s*,\s*Names::Options\b', match.group(1))
    ]


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


def code_of(text):
    """The text before the test module, or all of it where there is none."""
    found = TESTS.search(text)
    return text[: found.start()] if found else text


def prefixes(text, name):
    """The strings in `pub const NAME: &[&str] = &[...];`, or None where it is not found."""
    found = re.search(
        r"pub const " + re.escape(name) + r"\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\]\s*;", text, re.DOTALL
    )
    return re.findall(r'"(' + NAME + r')"', found.group(1)) if found else None


HOSTS = re.compile(r"const\s+HOST_VARIABLES\b[^=]*=\s*&?\[(.*?)\]\s*;", re.DOTALL)
# A `for name in [..]` loop, closed by the first brace that starts a line at the indentation of the
# `for` line, whatever that indentation is. Groups: the indentation, the names, the body.
NAME_LOOP = re.compile(
    r"^([ \t]*)for\s+name\s+in\s*\[(.*?)\]\s*\{(.*?)\n\1\}", re.DOTALL | re.MULTILINE
)


def loops_in(tests, start, check):
    """The names in the `for name in ["..."]` loops of `tests` whose body starts with `start` and
    asserts `check` is removed, `start` being `login_with` or `stage_with`."""
    found = []
    for _, names, body in NAME_LOOP.findall(tests):
        if re.search(re.escape(start) + r"\(", body) and re.search(
            r"assert!\(\s*" + re.escape(check) + r"\(", body
        ):
            found += re.findall(r'"(' + NAME + r')"', names)
    return found


def loader_complaints(files):
    """Every way `LOADER_PREFIXES` and the starts and tests that use it disagree.

    The prefixes are applied to the one start that passes them, the stored-login `gh`, and not to
    every checked start, since that would remove `LD_DEBUG` from gcloud and the opener (SANDBOX-30).
    So this asks for that start and its tests, and for the list to stay out of the prefixes every
    checked start applies.
    """
    text = files.get(PROGRAMS)
    if text is None:
        return []
    loader = prefixes(text, "LOADER_PREFIXES")
    if not loader:
        return [
            f"LOADER_PREFIXES is not in {PROGRAMS} or holds no prefix this script could read. If "
            "it moved, teach check-code-selectors.py."
        ]
    complaints = []
    for generic in prefixes(text, "CODE_SELECTOR_PREFIXES") or []:
        for prefix in loader:
            if generic.startswith(prefix) or prefix.startswith(generic):
                complaints.append(
                    f"{prefix} in LOADER_PREFIXES overlaps {generic} in CODE_SELECTOR_PREFIXES, "
                    "which every checked start applies and so would remove LD_DEBUG from the "
                    "starts SANDBOX-30 keeps it for."
                )
    if "LOADER_PREFIXES" not in (tests_of(text) or ""):
        complaints.append(f"No test in {PROGRAMS} uses LOADER_PREFIXES.")
    suite = files.get(USABILITY)
    if suite is None:
        return complaints
    if not re.search(
        r"withhold_prefixed\(\s*&mut\s+\w+\s*,\s*(?:[\w:]*::)?LOADER_PREFIXES\b", code_of(suite)
    ):
        complaints.append(
            f"{USABILITY} does not pass LOADER_PREFIXES to withhold_prefixed, so the stored-login "
            "gh is started with the loader's names (SANDBOX-21)."
        )
    removed = loops_in(tests_of(suite) or "", "login_with", "removed")
    for prefix in loader:
        if not any(name.startswith(prefix) for name in removed):
            complaints.append(
                f"{prefix} is in LOADER_PREFIXES and no test in {USABILITY} starts the "
                f"stored-login gh with a {prefix} name (login_with) and asserts it is removed."
            )
    return complaints


def host_complaints(text, source=USABILITY):
    """Every way `HOST_VARIABLES` and the tests and start that use it disagree."""
    found = HOSTS.search(text)
    names = re.findall(r'"(' + NAME + r')"', found.group(1)) if found else None
    if not names:
        return [
            f"HOST_VARIABLES is not in {source} or holds no name this script could read. If it "
            "moved, teach check-code-selectors.py."
        ]
    complaints = []
    if not re.search(r"for\s+\w+\s+in\s+HOST_VARIABLES\s*\{[^}]*env_remove\(", code_of(text)):
        complaints.append(
            f"{source} has no loop that env_removes each name in HOST_VARIABLES from a stage."
        )
    tests = tests_of(text) or ""
    looped = loops_in(tests, "stage_with", "removed")
    for name in names:
        asserted = re.search(r"assert!\(\s*withheld\(\s*\"" + re.escape(name) + r'"', tests)
        if not asserted and name not in looped:
            complaints.append(
                f"{name} is in HOST_VARIABLES in {source} and no test starts a stage with it and "
                "asserts it is removed."
            )
    return complaints


def asserts(tests, name, kept):
    """Whether a test asserts `withheld("name", ...)`, `withheld_value("name", ...)` or
    `checked("name", ...)`, negated if `kept`."""
    bang = r"!\s*" if kept else ""
    pattern = r"assert!\(\s*" + bang + r'(?:withheld_value|withheld|checked)\(\s*"' + re.escape(name) + r'"'
    if re.search(pattern, tests):
        return True
    # A loop over the whole list covers every name in it, including one added later.
    loop = r"for\s*\(\s*name\s*,\s*\w+\s*\)\s*in\s*(?:&?)CODE_SELECTORS\s*\{(.*?)\n        \}"
    inside = r"assert!\(\s*" + bang + r"(?:withheld_value|withheld|checked)\("
    return any(re.search(inside, body) for body in re.findall(loop, tests, re.DOTALL))


def list_complaints(text, source="the source file"):
    """Every way a file's CODE_SELECTORS list and its tests disagree, as complaints to print."""
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
                f'withheld("{name}", ...) for a value a stage may write.'
            )
        if name not in options and not asserts(tests, name, kept=True):
            complaints.append(
                f"{name} is in CODE_SELECTORS in {source} and no test asserts "
                f'!withheld("{name}", ...) for a safe value.'
            )
    return complaints


def arguments(text, open_paren):
    """The top-level arguments of the call whose `(` is at `open_paren`, as raw text, or None."""
    depth = 0
    start = open_paren + 1
    found = []
    index = open_paren
    while index < len(text):
        char = text[index]
        if text.startswith("//", index):
            index = text.find("\n", index)
            if index == -1:
                return None
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                last = text[start:index]
                if without_comments(last):
                    found.append(last)
                return found
        elif char == "," and depth == 1:
            found.append(text[start:index])
            start = index + 1
        index += 1
    return None


def without_comments(raw):
    return re.sub(r"//[^\n]*", "", raw).strip()


def in_test_code(label, text, position):
    """Whether the call at `position` is in a test file or after a test module attribute."""
    if "/tests/" in label:
        return True
    return FOLLOWS_TEST_ATTRIBUTE.search(text, 0, position) is not None


def call_complaints(label, text, spec=""):
    """Every `programs::start` or `start_against` call in `text` that does not state its class."""
    positions = [found.end() - 1 for found in CALL.finditer(text)]
    if BARE_IMPORT.search(text):
        positions += [
            found.end() - 1
            for found in BARE_CALL.finditer(text)
            if not text[: found.start()].rstrip().endswith("fn")
        ]
    complaints = []
    for position in sorted(set(positions)):
        line = text.count("\n", 0, position) + 1
        if text[:position].rsplit("\n", 1)[-1].lstrip().startswith("//"):
            continue
        given = arguments(text, position)
        where = f"{label}:{line}"
        against = text[:position].endswith("_against")
        wanted = 6 if against else 4
        if given is None or len(given) != wanted:
            complaints.append(
                f"{where}: start is not given {wanted} arguments this script can read."
            )
            continue
        fourth = FOURTH.match(without_comments(given[3]))
        if not fourth:
            complaints.append(
                f"{where}: the fourth argument of start is not written as CodeSelectors::Kept "
                "or CodeSelectors::Checked(&[...]) at the call."
            )
            continue
        if fourth.group(1) == "Kept":
            if "//" not in given[3] and not in_test_code(label, text, position):
                complaints.append(
                    f"{where}: CodeSelectors::Kept has no comment saying why the program may "
                    "run what the environment names (SANDBOX-30)."
                )
        elif fourth.group(2).strip() != "[]":
            if selectors(text) is None:
                complaints.append(
                    f"{where}: Checked(&{fourth.group(2)}) is used in a file with no "
                    "const CODE_SELECTORS list."
                )
        elif not in_test_code(label, text, position):
            if "//" not in given[3]:
                complaints.append(
                    f"{where}: CodeSelectors::Checked(&[]) has no comment saying why the program "
                    "reads no variable of its own that selects code (SANDBOX-30)."
                )
            if not listed_in_spec(spec, label):
                complaints.append(
                    f"{where}: CodeSelectors::Checked(&[]) is in a file with no row in the table "
                    f"after {EMPTY_LIST_MARKER} in {SPEC} (SANDBOX-30)."
                )
    return complaints


def verdict(files, required=None, spec=""):
    """Every complaint over `files`, a mapping of label to text.

    `required` is the labels that must be among the files and hold a list. `spec` is the text of
    docs/specs/sandboxing.md, which holds the table a start with no list of its own is named in.
    """
    required = [PROGRAMS, *SOURCES] if required is None else required
    complaints = []
    for label in required:
        if label not in files:
            complaints.append(
                f"{label} is not among the files read. If it moved, teach check-code-selectors.py."
            )
    for label, text in files.items():
        if label in required or selectors(text) is not None:
            complaints += list_complaints(text, label)
        if label == PROGRAMS:
            for const in shared_mismatches(text):
                complaints.append(
                    f"{const} in {label} holds another variable's name, so a list naming "
                    f"selectors::{const} would be tested under the wrong name."
                )
        else:
            complaints += call_complaints(label, text, spec)
    if USABILITY in required:
        complaints += loader_complaints(files)
        if USABILITY in files:
            complaints += host_complaints(files[USABILITY])
    return complaints


COVERED = """\
const CODE_SELECTORS: &[(&str, Names)] = &[
    ("A_PROGRAM", Names::Program),
    ("A_DIRECTORY", Names::Directory),
    ("A_FILE", Names::File),
    ("A_OPTIONS", Names::Options),
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
        assert!(checked("A_OPTIONS", &planted, &session));
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

# A list whose tests loop over the whole table, as the generic list's do.
LOOPED = """\
const CODE_SELECTORS: &[(&str, Names)] = &[
    ("A_PROGRAM", Names::Program),
    ("AN_OPTION", Names::Options),
];
#[cfg(test)]
mod tests {
    fn t() {
        for (name, _) in CODE_SELECTORS {
            assert!(checked(name, &planted, &session));
        }
        for (name, names) in CODE_SELECTORS {
            if *names == Names::Options {
                continue;
            }
            assert!(!checked(name, &genuine, &session));
        }
    }
}
"""

CALLER = """\
fn command() -> Command {
    bravebot_sandbox::programs::start(
        &program,
        Inherits::All,
        SearchPath::Inherited,
        // The person's own choice (SANDBOX-30).
        bravebot_sandbox::programs::CodeSelectors::Kept,
    )
}
"""

# A file with a list for one program beside the list for the rest.
TWO_LISTS = """\
const CODE_SELECTORS: [(&str, Names); 2] = [
    ("A_PROGRAM", Names::Program),
    ("A_FILE", Names::File),
];
const OSASCRIPT_CODE_SELECTORS: [(&str, Names); 2] = [
    ("AN_OSASCRIPT_DIRECTORY", Names::Directory),
    ("AN_OSASCRIPT_OPTION", Names::Options),
];
#[cfg(test)]
mod tests {
    fn t() {
        assert!(withheld("A_PROGRAM", &planted, &session));
        assert!(!withheld("A_PROGRAM", &genuine, &session));
        assert!(withheld("A_FILE", &planted, &session));
        assert!(!withheld("A_FILE", &genuine, &session));
        assert!(withheld("AN_OSASCRIPT_DIRECTORY", &planted, &session));
        assert!(!withheld("AN_OSASCRIPT_DIRECTORY", &genuine, &session));
        assert!(withheld("AN_OSASCRIPT_OPTION", &planted, &session));
    }
}
"""

CALLER_AGAINST = """\
fn command() -> Command {
    bravebot_sandbox::programs::start_against(
        &program,
        Inherits::All,
        SearchPath::Inherited,
        // This program reads no variable of its own that selects code (SANDBOX-30).
        bravebot_sandbox::programs::CodeSelectors::Checked(&[]),
        held,
        &judge,
    )
}
"""


START_AGAINST = (
    CALLER.replace("programs::start(", "programs::start_against(")
    .replace("    )\n}", "        held,\n        &outside,\n    )\n}")
)


# The table a start with no list of its own is named in, as docs/specs/sandboxing.md writes it.
EMPTY_TABLE = """\
Some text.

<!-- empty-list-starts -->

| Start | File | Reads |
|---|---|---|
| A program | `crates/agent/src/x.rs` | nothing that selects code |

More text.
"""

# The sandbox crate's two prefix lists and a test that uses the loader's.
LOADER_PROGRAMS = """\
pub const CODE_SELECTOR_PREFIXES: &[&str] = &["GIT_CONFIG_"];

pub const LOADER_PREFIXES: &[&str] = &["DYLD_", "LD_"];
#[cfg(test)]
mod tests {
    fn t() {
        withhold_prefixed(&mut command, LOADER_PREFIXES, held);
    }
}
"""

# The usability suite: a host list a stage removes, the stored-login start, and the tests.
LOADER_SUITE = """\
const HOST_VARIABLES: &[&str] = &[
    "GIT_ASKPASS",
    "GIT_DIR",
    "GOROOT",
];
fn stage_command() {
    for name in HOST_VARIABLES {
        command.env_remove(name);
    }
}
fn login_command() {
    bravebot_sandbox::programs::withhold_prefixed(
        &mut command,
        bravebot_sandbox::programs::LOADER_PREFIXES,
        names,
    );
}
#[cfg(test)]
mod tests {
    fn t() {
        assert!(withheld("GIT_ASKPASS", &genuine, &session));
        assert!(withheld("GIT_DIR", &genuine, &session));
        for name in [
            "GOROOT",
        ] {
            assert!(
                removed(&stage_with(OsStr::new(name), parent, &session), OsStr::new(name)),
                "{name}"
            );
        }
        for name in HOST_VARIABLES {
            assert!(removed(&stage_with(OsStr::new(name), g, &session), OsStr::new(name)));
        }
        for name in [
            "DYLD_INSERT_LIBRARIES",
            "LD_PRELOAD",
        ] {
            let command = login_with(OsStr::new(name), genuine.as_os_str(), &session);
            assert!(removed(&command, OsStr::new(name)), "{name}");
        }
        for name in ["LDAP_URI"] {
            let command = login_with(OsStr::new(name), genuine.as_os_str(), &session);
            assert!(!removed(&command, OsStr::new(name)), "{name}");
        }
    }
}
"""

# The opener's list as opener.rs writes it, with a name taken from the shared module, and the
# application's two lists of the names the application deletes.
OPENER_LIST = """\
const CODE_SELECTORS: [(&str, Names); 16] = [
    selectors::BROWSER,
    ("HOME", Names::Directory),
    ("XDG_RUNTIME_DIR", Names::Directory),
    ("DBUS_SESSION_BUS_ADDRESS", Names::BusAddress),
    ("GIO_LAUNCH_DESKTOP", Names::File),
    ("XDG_DATA_HOME", Names::Directory),
    ("PATHEXT", Names::Options),
    ("COMSPEC", Names::File),
    ("SystemRoot", Names::Directory),
    ("windir", Names::Directory),
    ("ProgramFiles", Names::Directory),
    ("ProgramW6432", Names::Directory),
    ("ProgramFiles(x86)", Names::Directory),
    ("LOCALAPPDATA", Names::Directory),
    ("APPDATA", Names::Directory),
    ("USERPROFILE", Names::Directory),
];
"""
TS_LIST = """\
export const OPENER_SELECTORS = [
  'BROWSER',
  'GIO_LAUNCH_DESKTOP',
  'XDG_DATA_HOME',
] as const
"""
MJS_LIST = "const NAMES = ['BROWSER', 'GIO_LAUNCH_DESKTOP', 'XDG_DATA_HOME']\n"


def selftest():
    """Prove the parse and the verdict on inputs whose answer is known.

    A gate that is quietly wrong is worse than none, and this one's claim is that a classification
    cannot be left out or a listed name lose its tests unnoticed. Each case is one way the claim
    could fail to hold.
    """
    checks = []

    def case(claim, held, got):
        checks.append((claim, held, got))

    got = selectors(COVERED)
    case(
        "parse: all names are read",
        got == ["A_PROGRAM", "A_DIRECTORY", "A_FILE", "A_OPTIONS"],
        got,
    )
    got = selectors(SHARED_LIST)
    case(
        "parse: a shared entry is read as its constant's name",
        got == ["A_PROGRAM", "A_SHARED", "A_FILE"],
        got,
    )
    got = selectors('const CODE_SELECTORS: [(&str, Names); 1] = [\n    ("ProgramFiles(x86)", Names::Directory),\n];\n')
    case("parse: a name with parentheses is read", got == ["ProgramFiles(x86)"], got)
    got = options_in('const CODE_SELECTORS: [(&str, Names); 1] = [\n    ("A_NAME(x86)", Names::Options),\n];\n')
    case("parse: an options name with parentheses is read", got == ["A_NAME(x86)"], got)
    got = quoted_names(APPLICATION_LIST, "export const OPENER_SELECTORS = [\n  'ProgramFiles(x86)',\n] as const\n")
    case("parse: a quoted name with parentheses is read", got == ["ProgramFiles(x86)"], got)
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
        list_complaints(OPTIONS_LIST) == [],
        list_complaints(OPTIONS_LIST),
    )
    stripped = OPTIONS_LIST.replace('assert!(withheld("AN_OPTION"', 'let _ = ("AN_OPTION"')
    case(
        "verdict: an options name with no removal assertion fails once",
        len(list_complaints(stripped)) == 1,
        list_complaints(stripped),
    )
    as_file = OPTIONS_LIST.replace("Names::Options", "Names::File")
    case(
        "verdict: a path name is not excused for lacking a keep assertion",
        len(list_complaints(as_file)) == 1,
        list_complaints(as_file),
    )
    case("parse: a file with no list is not an empty list", selectors("fn main() {}") is None, None)
    case(
        "verdict: a file with no list is reported",
        len(list_complaints("fn main() {}")) == 1,
        list_complaints("fn main() {}"),
    )
    case("verdict: a covered list passes", list_complaints(COVERED) == [], list_complaints(COVERED))
    case(
        "verdict: a covered list under a plain cfg(test) passes",
        list_complaints(COVERED_PLAIN) == [],
        list_complaints(COVERED_PLAIN),
    )
    case(
        "verdict: loops over the whole table cover a name, an options name needs no keep",
        list_complaints(LOOPED) == [],
        list_complaints(LOOPED),
    )
    unlooped = LOOPED.replace("assert!(!checked(name", "let _ = (name")
    case(
        "verdict: a loop with no keep assertion fails once, for the program name",
        len(list_complaints(unlooped)) == 1,
        list_complaints(unlooped),
    )

    # A second list in one file, kept for one program (`OSASCRIPT_CODE_SELECTORS`), is read too.
    got = selectors(TWO_LISTS)
    case(
        "parse: a second list named for its program is read with the first",
        got == ["A_PROGRAM", "A_FILE", "AN_OSASCRIPT_DIRECTORY", "AN_OSASCRIPT_OPTION"],
        got,
    )
    got = options_in(TWO_LISTS)
    case(
        "parse: an options name in the second list is read as options",
        got == ["AN_OSASCRIPT_OPTION"],
        got,
    )
    got = list_complaints(TWO_LISTS)
    case("verdict: two covered lists pass", got == [], got)
    got = list_complaints(
        TWO_LISTS.replace(
            'assert!(!withheld("AN_OSASCRIPT_DIRECTORY"', 'assert!(withheld("AN_OSASCRIPT_DIRECTORY"'
        )
    )
    case("verdict: the second list's name with no keep assertion fails once", len(got) == 1, got)
    got = list_complaints(
        TWO_LISTS.replace('assert!(withheld("AN_OSASCRIPT_OPTION"', 'let _ = ("AN_OSASCRIPT_OPTION"')
    )
    case("verdict: the second list's options name with no removal fails once", len(got) == 1, got)

    new_one = '("A_DIRECTORY", Names::Directory),\n    ("A_NEW_ONE", Names::Program),'
    new_options = '("A_OPTIONS", Names::Options),\n    ("A_NEW", Names::Options),'
    list_cases = [
        (
            "a name asserted only as removed fails once",
            COVERED.replace('assert!(!withheld("A_PROGRAM"', 'assert!(withheld("A_PROGRAM"'),
            1,
        ),
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
            "an options name added with no test fails once, for its removal",
            COVERED.replace('("A_OPTIONS", Names::Options),', new_options),
            1,
        ),
        (
            "a test mentioning the name outside an assertion does not count",
            COVERED.replace(
                "    fn t() {",
                '    fn t() {\n        let _ = "A_NEW_ONE";',
            ).replace('("A_DIRECTORY", Names::Directory),', new_one),
            2,
        ),
        ("a list with no test module fails", COVERED.split("#[cfg(all(test")[0], 1),
        (
            "an empty list fails",
            COVERED.replace(
                '    ("A_PROGRAM", Names::Program),\n    ("A_DIRECTORY", Names::Directory),\n'
                '    ("A_FILE", Names::File),\n    ("A_OPTIONS", Names::Options),\n',
                "",
            ),
            1,
        ),
    ]
    programs = {PROGRAMS: COVERED}

    def complaints(text, caller=CALLER, label="crates/agent/src/x.rs", spec=EMPTY_TABLE):
        return verdict({PROGRAMS: text, label: caller}, required=[PROGRAMS], spec=spec)

    case(
        "verdict: a covered list and a classified call pass",
        complaints(COVERED) == [],
        complaints(COVERED),
    )
    for claim, text, expected in list_cases:
        got = complaints(text)
        case(f"list: {claim}", len(got) == expected, got)

    other_list = verdict(
        {**programs, "crates/agent/src/gcloud.rs": COVERED.replace("A_PROGRAM", "B_PROGRAM")},
        required=[PROGRAMS, "crates/agent/src/gcloud.rs"],
    )
    case("list: a required second file's list is read too", other_list == [], other_list)
    untested = verdict(
        {
            **programs,
            "crates/agent/src/gcloud.rs": COVERED.replace("A_PROGRAM", "B_PROGRAM").replace(
                'assert!(!withheld("B_PROGRAM"', 'assert!(withheld("B_PROGRAM"'
            ),
        },
        required=[PROGRAMS, "crates/agent/src/gcloud.rs"],
    )
    case("list: a second file's missing keep test is found", len(untested) == 1, untested)
    unrequired = verdict(
        {**programs, "crates/agent/src/other.rs": COVERED.replace("A_PROGRAM", "B_PROGRAM")},
        required=[PROGRAMS],
    )
    case("list: a list in any file is read", unrequired == [], unrequired)
    no_list = verdict({PROGRAMS: COVERED, "crates/agent/src/gcloud.rs": "fn main() {}"},
                      required=[PROGRAMS, "crates/agent/src/gcloud.rs"])
    case("list: a required file with no list fails", len(no_list) == 1, no_list)
    mismatch = verdict(
        {PROGRAMS: COVERED + 'pub const A_SHARED: (&str, Names) = ("OTHER", Names::File);\n'},
        required=[PROGRAMS],
    )
    case("shared: a mismatched constant fails the verdict", len(mismatch) == 1, mismatch)

    call_cases = [
        (
            "a fourth argument held in a variable fails",
            CALLER.replace("bravebot_sandbox::programs::CodeSelectors::Kept", "selectors"),
            1,
        ),
        (
            "a Kept call with no comment fails",
            CALLER.replace("        // The person's own choice (SANDBOX-30).\n", ""),
            1,
        ),
        (
            "a Kept call in a test module needs no comment",
            "#[cfg(test)]\nmod tests {\n"
            + CALLER.replace("        // The person's own choice (SANDBOX-30).\n", "")
            + "}\n",
            0,
        ),
        (
            "a Checked call with an empty own list passes",
            CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&[])"),
            0,
        ),
        (
            "a Checked call with an empty own list and no comment fails once",
            CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&[])").replace(
                "        // The person's own choice (SANDBOX-30).\n", ""
            ),
            1,
        ),
        (
            "a Checked call with an empty own list in a test module needs neither",
            "#[cfg(test)]\nmod tests {\n"
            + CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&[])").replace(
                "        // The person's own choice (SANDBOX-30).\n", ""
            )
            + "}\n",
            0,
        ),
        (
            "a Checked call naming a list the file lacks fails",
            CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&CODE_SELECTORS)"),
            1,
        ),
        (
            "a start_against call with a variable as its fourth argument fails",
            START_AGAINST.replace("bravebot_sandbox::programs::CodeSelectors::Kept", "selectors"),
            1,
        ),
        (
            "a start_against call with Checked and its own list passes",
            START_AGAINST.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&[])"),
            0,
        ),
        (
            "a start_against call with Kept and no comment fails",
            START_AGAINST.replace("        // The person's own choice (SANDBOX-30).\n", ""),
            1,
        ),
        (
            "a start_against call with five arguments fails",
            START_AGAINST.replace("        &outside,\n", ""),
            1,
        ),
        ("a call with three arguments fails", CALLER.replace("        SearchPath::Inherited,\n", ""), 1),
        ("a start_against call with six arguments passes", CALLER_AGAINST, 0),
        (
            "a start_against call with its classification in a variable fails",
            CALLER_AGAINST.replace("bravebot_sandbox::programs::CodeSelectors::Checked(&[])", "s"),
            1,
        ),
        (
            "a start_against call with four arguments fails",
            CALLER_AGAINST.replace("        held,\n        &judge,\n", ""),
            1,
        ),
        (
            "a call in a doc comment is not a call",
            "/// bravebot_sandbox::programs::start(&p, a, b);\nfn x() {}\n",
            0,
        ),
        (
            "a bare start in a file that imports it is read",
            "use bravebot_sandbox::programs::{Inherits, start};\n"
            "fn c() { start(&p, Inherits::All, SearchPath::Inherited, CodeSelectors::Kept) }\n",
            1,
        ),
        (
            "a bare start_against in a file that imports it is read",
            "use bravebot_sandbox::programs::start_against;\n"
            "fn c() { start_against(&p, a, b, s, h, j) }\n",
            1,
        ),
        (
            "a bare start in a file that does not import it is not read",
            "fn c() { server.start(); start(1); }\n",
            0,
        ),
    ]
    for claim, text, expected in call_cases:
        got = complaints(COVERED, caller=text)
        case(f"call: {claim}", len(got) == expected, got)

    named = complaints(
        COVERED,
        caller=CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&CODE_SELECTORS)")
        + "\n"
        + COVERED,
    )
    case("call: a Checked call naming the file's own list passes", named == [], named)
    empty = CALLER.replace("CodeSelectors::Kept", "CodeSelectors::Checked(&[])")
    for claim, spec, expected in [
        ("no spec text", "", 1),
        ("a spec with no marker", EMPTY_TABLE.replace(EMPTY_LIST_MARKER, ""), 1),
        (
            "a row for another file",
            EMPTY_TABLE.replace("crates/agent/src/x.rs", "crates/agent/src/y.rs"),
            1,
        ),
        (
            "the file named in prose after the table, not in a row",
            EMPTY_TABLE.replace("`crates/agent/src/x.rs`", "-") + "`crates/agent/src/x.rs`\n",
            1,
        ),
        ("a row for the file", EMPTY_TABLE, 0),
    ]:
        got = verdict({PROGRAMS: COVERED, "crates/agent/src/x.rs": empty}, [PROGRAMS], spec)
        case(f"call: a Checked(&[]) start with {claim} gives {expected} complaint(s)", len(got) == expected, got)
    missing = verdict({"crates/agent/src/x.rs": CALLER}, required=[PROGRAMS])
    case("verdict: a tree without programs.rs fails", len(missing) == 1, missing)
    required = verdict({**programs, "crates/tui/src/opener.rs": CALLER}, [PROGRAMS, "crates/tui/src/opener.rs"])
    case("verdict: a required file with no list fails", any("CODE_SELECTORS is not in" in c for c in required), required)
    required = verdict(programs, [PROGRAMS, "crates/tui/src/opener.rs"])
    case("verdict: a required file that is not read fails", len(required) == 1, required)
    required = verdict({**programs, "crates/tui/src/opener.rs": COVERED}, [PROGRAMS, "crates/tui/src/opener.rs"])
    case("verdict: a required file with a covered list passes", required == [], required)
    opener_removed = verdict(
        {**programs, "crates/tui/src/opener.rs": COVERED.replace("A_PROGRAM", "B_PROGRAM")},
        [PROGRAMS, "crates/tui/src/opener.rs"],
    )
    case("verdict: the opener's list is read as the other required lists are", opener_removed == [], opener_removed)
    default_sources = verdict({PROGRAMS: COVERED})
    case(
        "verdict: by default the opener and the usability suite's lists are required",
        any("crates/tui/src/opener.rs is not among" in c for c in default_sources)
        and any("crates/agent/src/usability.rs is not among" in c for c in default_sources),
        default_sources,
    )

    # LOADER_PREFIXES and HOST_VARIABLES: the loader's names go to the one start that passes the
    # list, and not to every checked start (SANDBOX-30 keeps LD_DEBUG for gcloud and the opener).
    files = {PROGRAMS: LOADER_PROGRAMS, USABILITY: LOADER_SUITE}
    got = loader_complaints(files)
    case("loader: the prefixes, their start and their tests pass", got == [], got)
    shared_prefixes = LOADER_PROGRAMS.replace('&["GIT_CONFIG_"]', '&["GIT_CONFIG_", "LD_"]')
    got = loader_complaints({**files, PROGRAMS: shared_prefixes})
    case(
        "loader: a loader prefix added to the prefixes every checked start applies fails once",
        len(got) == 1 and "overlaps" in got[0],
        got,
    )
    got = loader_complaints({**files, USABILITY: LOADER_SUITE.replace("LOADER_PREFIXES,", "&[],")})
    case(
        "loader: a stored-login start that does not pass the prefixes fails",
        any("does not pass LOADER_PREFIXES" in c for c in got),
        got,
    )
    got = loader_complaints({**files, USABILITY: LOADER_SUITE.replace('"LD_PRELOAD",', "")})
    case(
        "loader: a prefix with no test that removes a name under it fails once",
        len(got) == 1 and "is in LOADER_PREFIXES" in got[0],
        got,
    )
    got = loader_complaints(
        {**files, USABILITY: LOADER_SUITE.replace("removed(&command", "kept(&command")}
    )
    case("loader: a loader test that asserts nothing is removed fails for both prefixes", len(got) == 2, got)
    got = loader_complaints({**files, PROGRAMS: LOADER_PROGRAMS.replace('&["DYLD_", "LD_"]', "&[]")})
    case("loader: an empty list is reported", len(got) == 1 and "holds no prefix" in got[0], got)
    got = loader_complaints({**files, PROGRAMS: "fn main() {}"})
    case("loader: a missing list is reported", len(got) == 1, got)
    got = loader_complaints(
        {**files, PROGRAMS: LOADER_PROGRAMS.replace("LOADER_PREFIXES, held", "&[], held")}
    )
    case("loader: a sandbox test that never uses the list fails", len(got) == 1, got)
    got = host_complaints(LOADER_SUITE)
    case("host: every name removed by a test passes", got == [], got)
    got = host_complaints(
        LOADER_SUITE.replace('    "GIT_DIR",\n', '    "GIT_DIR",\n    "A_NEW_ONE",\n', 1)
    )
    case(
        "host: a name added to the list with no test fails once",
        len(got) == 1 and "A_NEW_ONE" in got[0],
        got,
    )
    got = host_complaints(
        LOADER_SUITE.replace(
            "    for name in HOST_VARIABLES {\n        command.env_remove(name);\n    }\n", ""
        )
    )
    case("host: a stage that does not remove the list fails once", len(got) == 1, got)
    got = host_complaints(LOADER_SUITE.replace('assert!(withheld("GIT_DIR"', 'let _ = ("GIT_DIR"'))
    case(
        "host: a name mentioned outside an assertion does not count",
        len(got) == 1 and "GIT_DIR" in got[0],
        got,
    )
    # A test module nested one level deeper closes its loops at another column.
    deeper = "".join("    " + line if line.strip() else line for line in LOADER_SUITE.splitlines(True))
    got = host_complaints(deeper)
    case("host: loops indented four more columns are read as before", got == [], got)
    got = loader_complaints({**files, USABILITY: deeper})
    case("loader: loops indented four more columns are read as before", got == [], got)
    got = host_complaints("fn main() {}")
    case("host: a file with no list is reported", len(got) == 1, got)
    got = verdict(files, required=[PROGRAMS])
    case(
        "verdict: the loader and host checks run only where the suite is required",
        not any("LOADER_PREFIXES" in c or "HOST_VARIABLES" in c for c in got),
        got,
    )
    got = verdict(files, required=[PROGRAMS, USABILITY])
    case(
        "verdict: the suite being required runs the loader and host checks",
        not any("LOADER_PREFIXES" in c or "HOST_VARIABLES" in c for c in got)
        and any("CODE_SELECTORS is not in" in c for c in got),
        got,
    )

    application_cases = [
        ("the lists agree, apart from the names the application keeps", OPENER_LIST, TS_LIST, MJS_LIST, 0),
        (
            "a name only the opener lists fails in both application lists",
            OPENER_LIST.replace('("HOME", Names::Directory),', '("HOME", Names::Directory),\n    ("NEW", Names::File),'),
            TS_LIST,
            MJS_LIST,
            2,
        ),
        (
            "a name missing from the application's list fails",
            OPENER_LIST,
            TS_LIST.replace("  'GIO_LAUNCH_DESKTOP',\n", ""),
            MJS_LIST,
            1,
        ),
        (
            "a name missing from the test's list fails",
            OPENER_LIST,
            TS_LIST,
            MJS_LIST.replace("'GIO_LAUNCH_DESKTOP', ", ""),
            1,
        ),
        (
            "a name only the application lists fails",
            OPENER_LIST,
            TS_LIST.replace("  'BROWSER',\n", "  'BROWSER',\n  'EXTRA',\n"),
            MJS_LIST,
            1,
        ),
        (
            "HOME in the application's list fails",
            OPENER_LIST,
            TS_LIST.replace("  'BROWSER',\n", "  'BROWSER',\n  'HOME',\n"),
            MJS_LIST,
            1,
        ),
        (
            "XDG_RUNTIME_DIR in the application's list fails",
            OPENER_LIST,
            TS_LIST.replace("  'BROWSER',\n", "  'BROWSER',\n  'XDG_RUNTIME_DIR',\n"),
            MJS_LIST,
            1,
        ),
        (
            "DBUS_SESSION_BUS_ADDRESS missing from the opener's list fails the exception",
            OPENER_LIST.replace('    ("DBUS_SESSION_BUS_ADDRESS", Names::BusAddress),\n', ""),
            TS_LIST,
            MJS_LIST,
            1,
        ),
        (
            "HOME missing from the opener's list fails the exception",
            OPENER_LIST.replace('    ("HOME", Names::Directory),\n', ""),
            TS_LIST,
            MJS_LIST,
            1,
        ),
        ("an application list that is not found fails", OPENER_LIST, "export const X = 1\n", MJS_LIST, 1),
        ("a test list that is not found fails", OPENER_LIST, TS_LIST, "const OTHER = []\n", 1),
        ("an empty application list fails", OPENER_LIST, "export const OPENER_SELECTORS = [] as const\n", MJS_LIST, 1),
        ("an opener with no list fails", "fn main() {}\n", TS_LIST, MJS_LIST, 1),
        (
            "a name listed twice fails",
            OPENER_LIST,
            TS_LIST.replace("  'BROWSER',\n", "  'BROWSER',\n  'BROWSER',\n"),
            MJS_LIST,
            1,
        ),
    ]
    for claim, opener_text, ts_text, mjs_text, expected in application_cases:
        got = application_complaints(opener_text, ts_text, mjs_text)
        case(f"application: {claim}", len(got) == expected, got)

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
    if not (ROOT / PROGRAMS).exists():
        print(f"{ROOT / PROGRAMS} is not there", file=sys.stderr)
        return 1
    files = {
        str(path.relative_to(ROOT)): path.read_text(encoding="utf-8")
        for path in sorted(CRATES.rglob("*.rs"))
    }
    spec = (ROOT / SPEC).read_text(encoding="utf-8")
    complaints = verdict(files, spec=spec)
    application_texts = []
    for label in (APPLICATION, APPLICATION_TEST):
        if (ROOT / label).exists():
            application_texts.append((ROOT / label).read_text(encoding="utf-8"))
        else:
            complaints.append(f"{label} is not there. If it moved, teach check-code-selectors.py.")
            application_texts.append("")
    complaints += application_complaints(files.get(OPENER, ""), *application_texts)
    for complaint in complaints:
        print(complaint, file=sys.stderr)
    if complaints:
        return 1
    lists = [(label, selectors(text)) for label, text in files.items() if selectors(text)]
    names = sum(len(found) for _, found in lists)
    calls = sum(len(CALL.findall(text)) for label, text in files.items() if label != PROGRAMS)
    print(
        f"check-code-selectors: {calls} start calls classified, {names} names in {len(lists)} "
        "lists, each with the tests it needs; the application's lists match the opener's"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
