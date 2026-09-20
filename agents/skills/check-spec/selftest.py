#!/usr/bin/env python3
"""Prove each mechanical check fires.

A check that never fires is worse than no check: it reports a clean spec tree forever and
somebody trusts it. Each case here builds a small fixture repository, breaks exactly one
thing, and asserts that exactly that check reports it. The first case breaks nothing, so a
check that fires on anything at all is caught too.

The cases after those cover the generated list of clauses nothing pins: the line a clause gets,
what stays out of it, and which runs may write the file.

    python3 agents/skills/check-spec/selftest.py
"""

import os
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import importlib.util  # noqa: E402

from specs import TestIndex, crate_directories, load_specs  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "check_spec", Path(__file__).resolve().parent / "check-spec.py"
)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

_drafts = importlib.util.spec_from_file_location(
    "draft_issues", Path(__file__).resolve().parent / "draft-issues.py"
)
draft = importlib.util.module_from_spec(_drafts)
_drafts.loader.exec_module(draft)


CLEAN_SPEC = """\
---
id: DEMO
title: A demonstration
status: normative
governs:
  - crates/demo/src/lib.rs
guards:
  - symbol: Gate::open
    sites:
      - crates/demo/src/lib.rs: 1
  - symbol: Gate::new
    sites:
      - crates/demo/src/lib.rs: 1
---

## Clauses

<a id="DEMO-1"></a>
### DEMO-1: the gate opens only once

`verified-by: bravebot_demo::lib::the_gate_opens_only_once`

<a id="DEMO-2"></a>
### DEMO-2: nothing else can open it

`verified-by: by-construction (the field is private)`
"""

CLEAN_README = """\
# Specs

| Spec | Id | Clauses | Topic |
|---|---|---|---|
| [demo.md](demo.md) | `DEMO` | 2 | a demonstration |
"""

CLEAN_SOURCE = """\
pub struct Gate;

impl Gate {
    pub fn open(&self) {}

    pub fn new<S: Into<String>>(_name: S) -> Self {
        Gate
    }
}

fn opens_twice() {}

#[cfg(test)]
mod tests {
    #[test]
    fn the_gate_opens_only_once() {}
}
"""


# The same gate with its type and its `impl` indented inside a module, which is where one type
# among several in a file lands.
NESTED_SOURCE = """\
pub mod gate {
    pub struct Gate;

    impl Gate {
        pub fn open(&self) {}

        pub fn new<S: Into<String>>(_name: S) -> Self {
            Gate
        }
    }
}

fn opens_twice() {}

#[cfg(test)]
mod tests {
    #[test]
    fn the_gate_opens_only_once() {}
}
"""


def build_fixture(root):
    (root / "docs" / "specs").mkdir(parents=True)
    (root / "docs" / "specs" / "demo.md").write_text(CLEAN_SPEC, encoding="utf-8")
    (root / "docs" / "specs" / "README.md").write_text(CLEAN_README, encoding="utf-8")
    # No clause here is verified by nothing, so the list the fixture starts with is empty.
    (root / check.UNVERIFIED_FILE).write_text(check.render_unverified([]), encoding="utf-8")
    (root / "crates" / "demo" / "src").mkdir(parents=True)
    (root / "crates" / "demo" / "Cargo.toml").write_text(
        '[package]\nname = "bravebot-demo"\n', encoding="utf-8"
    )
    (root / "crates" / "demo" / "src" / "lib.rs").write_text(CLEAN_SOURCE, encoding="utf-8")


def run_checks():
    specs = load_specs()
    sources = check.load_sources()
    crates = crate_directories()
    index = TestIndex()
    prefixes = {s.id for s in specs if s.id}
    findings = []
    for one in specs:
        findings.extend(check.check_front_matter(one))
        findings.extend(check.check_clause_numbering(one))
        findings.extend(check.check_coverage(one, index, crates))
        findings.extend(check.check_anchors(one))
        findings.extend(check.check_governs(one))
        findings.extend(check.check_guards(one, sources))
        findings.extend(check.check_isolation(one, prefixes))
        findings.extend(check.check_prose(one))
    findings.extend(check.check_readme(specs))
    findings.extend(check.check_unverified_file(specs, index, crates))
    return findings


def edit_spec(root, old, new):
    path = root / "docs" / "specs" / "demo.md"
    text = path.read_text(encoding="utf-8")
    assert old in text, f"fixture does not contain {old!r}"
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def use_the_gate_elsewhere(root):
    """A file the allowlist has never heard of, which is the shape a new escape hatch takes."""
    (root / "crates" / "demo" / "src" / "extra.rs").write_text(
        "use crate::Gate;\n\nfn reach(gate: &Gate) {\n    gate.open();\n}\n", encoding="utf-8"
    )


def use_the_gate_again(root):
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8") + "\nfn reach(gate: &Gate) {\n    gate.open();\n}\n",
        encoding="utf-8",
    )


def use_the_gate_twice_on_one_line(root):
    """Two uses folded onto one line, with the pin raised to what counting lines would give.
    Only counting occurrences reports this, so the case fails if the unit ever goes back to
    being the line."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\nfn reach(a: &Gate, b: &Gate) {\n    a.open(); b.open();\n}\n",
        encoding="utf-8",
    )
    edit_spec(root, "crates/demo/src/lib.rs: 1", "crates/demo/src/lib.rs: 2")


def build_the_gate(root):
    """The one form an associated function can be written in, which is what has to be counted."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8") + '\nfn build() -> Gate {\n    Gate::new("a gate")\n}\n',
        encoding="utf-8",
    )


def build_a_sibling_of_the_gate(root):
    """A constructor whose name begins with the guarded one's.

    `Gate::new_named` is a different symbol, and counting the guarded name wherever it appears as
    a prefix of another reports a use of `Gate::new`, which is the one call nobody made."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\nimpl Gate {\n    fn new_named(_name: u8) -> Self {\n        Gate\n    }\n}\n"
        + "\nfn build() -> Gate {\n    Gate::new_named(1)\n}\n",
        encoding="utf-8",
    )


def name_a_free_function_the_same(root):
    """A module-level function of the guarded name, below the block that defines it.

    The definition belongs to `Gate` only while the `impl Gate` block is open, and this one is
    outside it. Taking the last `impl` the file mentioned as the enclosing block instead puts
    every later `fn new` into the count, which the next unrelated function of that name pays
    for."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8") + "\nfn new(width: u8) -> u8 {\n    width\n}\n",
        encoding="utf-8",
    )


def renew_in_a_test_module(root):
    """Another type's method of the same name, in a module indented inside the guarded type's own
    file.

    The nearest `impl` above `fn new(&self)` here is `impl Latch`, four spaces in. Reading the
    outermost one instead finds a receiver on what it takes to be `Gate::new`, and every `.new(`
    in the tree joins the count."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\n#[cfg(test)]\nmod inner {\n    struct Latch;\n\n"
        + "    impl Latch {\n        fn new(&self) {}\n    }\n\n"
        + "    fn renew(latch: &Latch) {\n        latch.new();\n    }\n}\n",
        encoding="utf-8",
    )


def build_something_else(root):
    """Another type's constructor, in the file that defines the guarded one.

    `Gate::new` is an associated function, so counting `fn new` wherever the file names `Gate` puts
    every constructor this crate has in the count, and the pin then fails on the next type it
    gains rather than on a new way in."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\nstruct Latch;\n\nimpl Latch {\n    fn new() -> Self {\n        Latch\n    }\n}\n",
        encoding="utf-8",
    )


def renew_something_else(root):
    """Another type's method of the same name, called on a receiver.

    An associated function has no receiver to be called on, so `latch.new()` cannot be a use of
    `Gate::new` no matter which file it is in."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\nstruct Latch;\n\nimpl Latch {\n    fn new(&self) {}\n}\n"
        + "\nfn renew(latch: &Latch) {\n    latch.new();\n}\n",
        encoding="utf-8",
    )


def define_the_gate_inside_a_module(root):
    """The guarded definition indented inside a module, beside another type's method of the same
    name on a receiver.

    The definition is the same definition wherever the block it sits in begins. Recognising a block
    only at the margin misses it, and the guard then reads as a method: `latch.new()` joins the
    count, and so does every other `.new(` in the tree."""
    (root / "crates" / "demo" / "src" / "lib.rs").write_text(NESTED_SOURCE, encoding="utf-8")
    renew_something_else(root)


def mention_the_gate_in_comments(root):
    """Prose about a gate is not a use of it, in all four shapes the tree contains: a comment at
    the margin, a comment after code, a block comment across lines, and a `//` inside a string
    that is not a comment at all. None of them may move the count."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8")
        + "\n// Gate::open is the only way in.\n"
        + "fn document() {\n"
        + '    let _url = "https://example.test/open"; // Gate::open again\n'
        + "    /* Gate::open, and gate.open(),\n"
        + "       described across two lines */\n"
        + "}\n",
        encoding="utf-8",
    )


def list_says(root, lines):
    (root / check.UNVERIFIED_FILE).write_text(check.render_unverified(lines), encoding="utf-8")


def cite_another_spec(root):
    """A citation is only recognisable as one when the prefix belongs to a real spec, so
    the fixture grows a second spec for this case."""
    (root / "docs" / "specs" / "other.md").write_text(
        CLEAN_SPEC.replace("DEMO", "OTHER"), encoding="utf-8"
    )
    edit_spec(root, "the gate opens only once", "the same rule `OTHER-1` states")


CASES = [
    ("a clean spec tree reports nothing", lambda root: None, None),
    (
        "a spec with no id",
        lambda root: edit_spec(root, "id: DEMO\n", ""),
        "front-matter-missing",
    ),
    (
        "a spec governing nothing",
        lambda root: edit_spec(root, "governs:\n  - crates/demo/src/lib.rs\n", ""),
        "front-matter-missing",
    ),
    (
        "a clause numbered out of order",
        lambda root: edit_spec(root, "### DEMO-2:", "### DEMO-4:"),
        "clause-numbering",
    ),
    (
        "the same clause id twice",
        lambda root: edit_spec(root, "### DEMO-2:", "### DEMO-1:"),
        "clause-duplicate",
    ),
    (
        "a clause id from another spec's series",
        lambda root: edit_spec(root, "### DEMO-1:", "### OTHER-1:"),
        "clause-prefix-mismatch",
    ),
    (
        "a clause with no anchor to link to",
        lambda root: edit_spec(root, '<a id="DEMO-2"></a>\n', ""),
        "clause-anchor-missing",
    ),
    (
        "a clause with no verified-by at all",
        lambda root: edit_spec(
            root, "`verified-by: bravebot_demo::lib::the_gate_opens_only_once`\n", ""
        ),
        "clause-unverified",
    ),
    (
        "a clause verified by nothing",
        lambda root: edit_spec(
            root, "bravebot_demo::lib::the_gate_opens_only_once`", "none`"
        ),
        "clause-uncovered",
    ),
    (
        "by-construction without a reason",
        lambda root: edit_spec(root, "by-construction (the field is private)", "by-construction"),
        "by-construction-unexplained",
    ),
    (
        "a verified-by naming a test that does not exist",
        lambda root: edit_spec(root, "the_gate_opens_only_once`", "the_gate_never_opens`"),
        "verified-by-missing",
    ),
    (
        "a verified-by naming a test in the wrong module",
        lambda root: edit_spec(root, "bravebot_demo::lib::", "bravebot_demo::gate::"),
        "verified-by-moved",
    ),
    (
        "a verified-by naming a crate outside the workspace",
        lambda root: edit_spec(root, "bravebot_demo::", "bravebot_ghost::"),
        "verified-by-unknown-crate",
    ),
    (
        "a governs path that was renamed away",
        lambda root: (root / "crates" / "demo" / "src" / "lib.rs").rename(
            root / "crates" / "demo" / "src" / "gate.rs"
        ),
        "governs-missing",
    ),
    (
        "a guarded symbol that was renamed away",
        lambda root: edit_spec(root, "symbol: Gate::open", "symbol: Gate::unlock"),
        "guard-missing",
    ),
    (
        "a guarded symbol whose name is only the prefix of a function that exists",
        lambda root: edit_spec(root, "symbol: Gate::open", "symbol: Gate::opens"),
        "guard-missing",
    ),
    (
        "a guarded symbol used in a file its allowlist does not name",
        use_the_gate_elsewhere,
        "guard-site-unlisted",
    ),
    (
        "a guarded symbol used more times than its allowlist pins",
        use_the_gate_again,
        "guard-site-count",
    ),
    (
        "an allowlist left above the uses that are left",
        lambda root: edit_spec(root, "crates/demo/src/lib.rs: 1", "crates/demo/src/lib.rs: 2"),
        "guard-site-count",
    ),
    (
        "a guarded symbol used twice on one line",
        use_the_gate_twice_on_one_line,
        "guard-site-count",
    ),
    (
        "a guarded associated function used more times than its allowlist pins",
        build_the_gate,
        "guard-site-count",
    ),
    ("another type's constructor of the same name", build_something_else, None),
    ("another type's method of the same name, on a receiver", renew_something_else, None),
    ("a constructor whose name extends the guarded one's", build_a_sibling_of_the_gate, None),
    ("a free function of the same name below the block", name_a_free_function_the_same, None),
    ("another type's method of the same name, in an inner module", renew_in_a_test_module, None),
    ("a guarded definition inside a module", define_the_gate_inside_a_module, None),
    ("a guarded symbol named only in comments", mention_the_gate_in_comments, None),
    (
        # The exact set, because a missing count once reported the file as unlisted too, which
        # sends the author to add a path that is already there.
        "an allowlisted site that is not a path and a count",
        lambda root: edit_spec(root, "crates/demo/src/lib.rs: 1", "crates/demo/src/lib.rs"),
        {"guard-sites-malformed"},
    ),
    (
        "an allowlisted site pinned at no uses at all",
        lambda root: edit_spec(root, "crates/demo/src/lib.rs: 1", "crates/demo/src/lib.rs: 0"),
        "guard-sites-malformed",
    ),
    (
        "the same path pinned twice",
        lambda root: edit_spec(
            root,
            "      - crates/demo/src/lib.rs: 1\n",
            "      - crates/demo/src/lib.rs: 1\n      - crates/demo/src/lib.rs: 1\n",
        ),
        "guard-sites-malformed",
    ),
    (
        "a sites list indented out of the guards entry it belongs to",
        lambda root: edit_spec(root, "    sites:\n", "  sites:\n"),
        "front-matter-unknown-key",
    ),
    (
        # Read by the security-audit skill rather than here. Removing it from the known keys would
        # fault the spec that carries it, so the key is held by this case rather than by memory.
        "a front matter key another skill reads",
        lambda root: edit_spec(
            root, "guards:\n", "reads_a_step_without_keying:\n  - crates/demo/src/lib.rs::plain\nguards:\n"
        ),
        None,
    ),
    (
        "one guarded symbol pinning its sites while another does not",
        lambda root: edit_spec(root, "guards:\n", "guards:\n  - symbol: opens_twice\n"),
        "guard-sites-partial",
    ),
    (
        "a spec citing another spec's clause ids",
        lambda root: cite_another_spec(root),
        "cross-spec-citation",
    ),
    (
        "an em-dash",
        lambda root: edit_spec(root, "the gate opens only once", "the gate opens — once"),
        "em-dash",
    ),
    (
        "a spec missing from the README table",
        lambda root: (root / "docs" / "specs" / "README.md").write_text(
            "# Specs\n", encoding="utf-8"
        ),
        "readme-unlisted",
    ),
    (
        "a README row with the wrong clause count",
        lambda root: (root / "docs" / "specs" / "README.md").write_text(
            CLEAN_README.replace("| 2 |", "| 5 |"), encoding="utf-8"
        ),
        "readme-wrong-count",
    ),
    (
        "a README row with the wrong id",
        lambda root: (root / "docs" / "specs" / "README.md").write_text(
            CLEAN_README.replace("`DEMO`", "`OTHER`"), encoding="utf-8"
        ),
        "readme-wrong-id",
    ),
    (
        "a README row for a spec that is not there",
        lambda root: (root / "docs" / "specs" / "README.md").write_text(
            CLEAN_README + "| [gone.md](gone.md) | `GONE` | 1 | nothing |\n", encoding="utf-8"
        ),
        "readme-phantom",
    ),
    (
        "a clause nothing pins that the committed list does not name",
        lambda root: edit_spec(root, "bravebot_demo::lib::the_gate_opens_only_once`", "none`"),
        "unverified-list-stale",
    ),
    (
        # The direction a hand-kept file drifts in on its own: the clause was given a test and
        # nobody went back to the list.
        "a committed list naming a clause a test pins",
        lambda root: list_says(root, ["docs/specs/demo.md:DEMO-1: the gate opens only once"]),
        "unverified-list-stale",
    ),
]


def withdraw_the_second_clause(root):
    """A withdrawn clause carries no `verified-by` line, which a generator reading the front
    matter for itself would report as a clause verified by nothing."""
    edit_spec(
        root, "### DEMO-2: nothing else can open it", "### DEMO-2: withdrawn, replaced by DEMO-1"
    )
    edit_spec(root, "`verified-by: by-construction (the field is private)`\n", "")


UNVERIFIED_CASES = [
    ("a clean spec tree lists no clause", None, []),
    (
        "a clause verified by nothing is listed with its heading",
        lambda root: edit_spec(root, "bravebot_demo::lib::the_gate_opens_only_once`", "none`"),
        ["docs/specs/demo.md:DEMO-1: the gate opens only once"],
    ),
    ("a withdrawn clause is not listed", withdraw_the_second_clause, []),
]

VIOLATION = {
    "spec": "docs/specs/demo.md",
    "clause": "DEMO-1",
    "severity": "error",
    "kind": "violation",
    "summary": "the gate opens twice",
    "evidence": ["crates/demo/src/lib.rs:4 nothing records that it was opened"],
    "failure": "call open twice and the second call is accepted",
    "fix": "refuse the second call",
    "screen": "open the gate, then open it again, and look at what is drawn",
    "source": "review",
}
UNCOVERED = {
    "spec": "docs/specs/demo.md",
    "clause": "DEMO-1",
    "severity": "warning",
    "kind": "clause-uncovered",
    "summary": "`DEMO-1` is `verified-by: none`, so nothing pins it",
    "evidence": "docs/specs/demo.md:9",
    "source": "mechanical",
}
# A run that did not finish, and a review that could not tell. Neither names anything to go and fix.
UNFINISHED = dict(VIOLATION, kind="review-incomplete", clause="DEMO-2")
UNDECIDED = dict(VIOLATION, kind="unclear", severity="warning", clause="DEMO-2")
# A mechanical error fails `make check-spec`, so it is red on the branch that caused it.
RED_IN_CI = dict(UNCOVERED, kind="clause-numbering", severity="error", clause="DEMO-2")

A_SCREEN = "the gate is open\nand a ``` fence, which must not close the block\n"


def draft_checks():
    """What one body says, against a finding whose every field is known.

    A body nobody can act on is the state this replaces rather than an improvement on it, so each
    claim here is one thing the reader of an issue needs and would have to go and find out without.
    """
    draft._LOADED.clear()  # The fixture is a different spec tree from the last case's.
    out = Path("issues")
    findings = [VIOLATION, UNCOVERED, UNFINISHED, UNDECIDED, RED_IN_CI]
    drafts = draft.draft(draft.draftable(findings, errors_only=False), out)
    by_kind = {entry["kind"]: entry for entry in drafts}
    body = Path(by_kind["violation"]["body_file"]).read_text(encoding="utf-8")

    checks = [
        (
            "only the findings a green CI run leaves unsaid are drafted",
            sorted(by_kind) == ["clause-uncovered", "violation"],
        ),
        (
            "two findings on one clause get a body each",
            by_kind["violation"]["body_file"] != by_kind["clause-uncovered"]["body_file"],
        ),
        ("the title leads with the clause", by_kind["violation"]["title"].startswith("DEMO-1: ")),
        ("a violation is labelled a mismatch", by_kind["violation"]["label"] == "spec-mismatch"),
        (
            "a clause nothing pins is labelled coverage",
            by_kind["clause-uncovered"]["label"] == "spec-coverage",
        ),
        (
            "the body quotes the clause that was broken",
            "> ### DEMO-1: the gate opens only once" in body,
        ),
        ("the body says where the clause is", "`docs/specs/demo.md#DEMO-1`" in body),
        # Every field the review produced, since the fixer starts from these and nothing else.
        ("the body keeps the failure", VIOLATION["failure"] in body),
        ("the body keeps the evidence", "`crates/demo/src/lib.rs:4`" in body),
        ("the body keeps the fix", VIOLATION["fix"] in body),
        ("the body names what the spec governs", "`crates/demo/src/lib.rs`" in body),
        (
            "the body says a check drafted it rather than a person",
            "rather than written by a person" in body,
        ),
        # Shown, not described: with no screen yet, the code at the evidence line is the artifact.
        ("the body shows the code at the evidence line", "pub fn open(&self) {}" in body),
        (
            "a screen is asked for where the reviewer said one could be seen",
            by_kind["violation"]["screen_wanted"] == VIOLATION["screen"]
            and not by_kind["violation"]["has_screen"],
        ),
        (
            "a clause nothing pins asks for no screen",
            not by_kind["clause-uncovered"]["screen_wanted"],
        ),
    ]

    Path(by_kind["violation"]["screen_file"]).write_text(A_SCREEN, encoding="utf-8")
    drafts = draft.draft(draft.draftable(findings, errors_only=False), out)
    by_kind = {entry["kind"]: entry for entry in drafts}
    shown = Path(by_kind["violation"]["body_file"]).read_text(encoding="utf-8")
    checks += [
        ("a captured screen is read back into the body", "the gate is open" in shown),
        (
            "the screen replaces the code it was captured instead of",
            "pub fn open(&self) {}" not in shown,
        ),
        # The screen is whatever the interface drew, backticks included.
        ("a fence inside a screen does not close the block early", "````text" in shown),
        ("the draft stops asking for a screen it has", by_kind["violation"]["has_screen"]),
    ]

    errors = draft.draft(draft.draftable(findings, errors_only=True), out)
    checks.append(
        ("--errors leaves the warnings out", [e["kind"] for e in errors] == ["violation"])
    )
    return checks


# Which runs may write the file. A run given a filter read part of the tree, and the list is
# about all of it.
SELECTIONS = [
    ("a full run may write the list", [], None, False),
    ("a named spec may not write the list", ["demo"], None, True),
    ("a changed-only run may not write the list", [], "main", True),
]


def in_fixture(break_it, ask):
    """`ask` run against a fresh fixture repository, broken as the case says."""
    original = Path.cwd()
    root = Path(tempfile.mkdtemp(prefix="check-spec-selftest-"))
    try:
        build_fixture(root)
        os.chdir(root)
        if break_it is not None:
            break_it(root)
        return ask()
    finally:
        os.chdir(original)
        shutil.rmtree(root, ignore_errors=True)


def generated_list():
    return check.unverified_clauses(load_specs(), TestIndex(), crate_directories())


def main():
    failures = []

    def note(name, ok, detail):
        print(f"{'ok  ' if ok else 'FAIL'}  {name}")
        if not ok:
            failures.append(f"{name}: {detail}")

    for name, break_it, expected in CASES:
        kinds = in_fixture(break_it, lambda: sorted({f["kind"] for f in run_checks()}))
        if expected is None:
            note(name, not kinds, f"reported {kinds}")
        elif isinstance(expected, set):
            wanted = sorted(expected)
            note(name, set(kinds) == expected, f"reported {kinds}, wanted exactly {wanted}")
        else:
            note(name, expected in kinds, f"reported {kinds}, wanted {expected}")

    for name, break_it, expected in UNVERIFIED_CASES:
        lines = in_fixture(break_it, generated_list)
        note(name, lines == expected, f"generated {lines}, wanted {expected}")

    for name, selectors, changed_base, refused in SELECTIONS:
        answer = check.partial_selection(selectors, changed_base)
        note(name, answer == refused, f"answered {answer}, wanted {refused}")

    drafted = in_fixture(None, draft_checks)
    for name, held in drafted:
        note(name, held, "the body does not say it")

    total = len(CASES) + len(UNVERIFIED_CASES) + len(SELECTIONS) + len(drafted)
    print()
    if failures:
        for failure in failures:
            print(f"  {failure}")
        print(f"{len(failures)} of {total} cases failed")
        return 1
    print(f"{total} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
