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

import contextlib
import io
import json
import os
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import importlib.util  # noqa: E402

import reviewed  # noqa: E402
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


DOCUMENTED_BY = "documented-by: docs/website/docs/demo.md"

CLEAN_SPEC = f"""\
---
id: DEMO
title: A demonstration
status: normative
governs:
  - crates/demo/src/lib.rs
guards:
  - symbol: Gate::open
    every-spelling: yes
    sites:
      - crates/demo/src/lib.rs: 1
  - symbol: Gate::new
    sites:
      - crates/demo/src/lib.rs: 1
{DOCUMENTED_BY}
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
    (root / check.UNVERIFIED_FILE).parent.mkdir(parents=True, exist_ok=True)
    (root / check.UNVERIFIED_FILE).write_text(check.render_unverified([]), encoding="utf-8")
    website = root / "docs" / "website" / "docs"
    website.mkdir(parents=True)
    (website / "demo.md").write_text("# Demo\n", encoding="utf-8")
    (root / "crates" / "demo" / "src").mkdir(parents=True)
    (root / "crates" / "demo" / "Cargo.toml").write_text(
        '[package]\nname = "bravebot-demo"\n', encoding="utf-8"
    )
    (root / "crates" / "demo" / "src" / "lib.rs").write_text(CLEAN_SOURCE, encoding="utf-8")
    # A tracked tree, because a body may quote only what the tree publishes and the drafter asks
    # git which files those are.
    git(root, "init", "-q")
    git(root, "add", "-A")
    # And a commit, because what a clause id named is read from history.
    git(root, "commit", "-q", "-m", "the fixture before the case breaks it")


def git(root, *arguments):
    """One git command in the fixture, printing only what it prints on stdout.

    Identity and signing are given on the command rather than taken from the machine, so a case
    says the same thing wherever it runs and a throwaway fixture never asks anybody for a key."""
    return subprocess.run(
        [
            "git",
            "-C",
            str(root),
            "-c",
            "user.name=check-spec selftest",
            "-c",
            "user.email=selftest@example.invalid",
            "-c",
            "commit.gpgsign=false",
            *arguments,
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def run_checks():
    specs = load_specs()
    sources = check.load_sources()
    crates = crate_directories()
    index = TestIndex()
    prefixes = {s.id for s in specs if s.id}
    declared, findings = check.declared_renumbers()
    for one in specs:
        findings.extend(check.check_front_matter(one))
        findings.extend(check.check_clause_numbering(one))
        findings.extend(check.check_clause_history(one, declared))
        findings.extend(check.check_coverage(one, index, crates))
        findings.extend(check.check_anchors(one))
        findings.extend(check.check_governs(one))
        findings.extend(check.check_guards(one, sources))
        findings.extend(check.check_isolation(one, prefixes))
        findings.extend(check.check_prose(one))
        findings.extend(check.check_documentation(one))
    findings.extend(check.check_reviewed_file())
    findings.extend(check.check_readme(specs))
    findings.extend(check.check_unverified_file(specs, index, crates))
    findings.extend(check.check_renumbered_file(specs, declared))
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


def open_the_gate_by_another_spelling(spelling):
    """One use written in a form the count does not read, with the pin raised to what a count that
    did read it would give. The count is then right about every spelling it knows, so the one thing
    that can report the use is a check of the spelling itself."""

    def write(root):
        path = root / "crates" / "demo" / "src" / "lib.rs"
        path.write_text(
            path.read_text(encoding="utf-8") + f"\n{spelling}\n", encoding="utf-8"
        )

    return write


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


def rename_the_gate_past_its_guard(root):
    """The guarded method renamed, beside another type's method of the name it used to have.

    Nothing called `Gate` has an `open` any more; `Latch` is what spells that name now. Counting
    the bare name on any receiver takes the latch's definition and its one call as the gate's, so
    the guard reads as present, and the pin here is the count that answer produces. The check then
    passes over a guard on a symbol no type defines, which is the one answer it must never give.
    """
    path = root / "crates" / "demo" / "src" / "lib.rs"
    renamed = path.read_text(encoding="utf-8").replace(
        "pub fn open(&self) {}", "pub fn opened(&self) {}"
    )
    path.write_text(
        renamed
        + "\nstruct Latch;\n\nimpl Latch {\n    fn open(&self) {}\n}\n"
        + "\nfn reach(latch: &Latch) {\n    latch.open();\n}\n",
        encoding="utf-8",
    )
    edit_spec(root, "crates/demo/src/lib.rs: 1", "crates/demo/src/lib.rs: 2")


def guard_a_function_in_its_module_file(root):
    """A module-qualified guard, which names no type for an `impl` to carry.

    `home::write_file` is the shape, and what answers for it is a free function at the margin of
    the file the module is. Requiring the qualifier to own the definition has to leave this
    resolving, including where nothing in the file spells the module's own name."""
    (root / "crates" / "demo" / "src" / "latch.rs").write_text(
        "pub fn shut() {}\n", encoding="utf-8"
    )
    edit_spec(
        root,
        "guards:\n",
        "guards:\n  - symbol: latch::shut\n    sites:\n      - crates/demo/src/latch.rs: 1\n",
    )


def guard_a_function_in_a_mod_block(root):
    """The same free function, in a `mod` block rather than a file of its own, which is the other
    place a module-qualified guard's definition sits."""
    path = root / "crates" / "demo" / "src" / "lib.rs"
    path.write_text(
        path.read_text(encoding="utf-8") + "\npub mod latch {\n    pub fn shut() {}\n}\n",
        encoding="utf-8",
    )
    edit_spec(
        root,
        "guards:\n",
        "guards:\n  - symbol: latch::shut\n    sites:\n      - crates/demo/src/lib.rs: 1\n",
    )


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


def readme_lists(root, clauses):
    """The README table's count for the fixture spec, which several cases below change."""
    (root / "docs" / "specs" / "README.md").write_text(
        CLEAN_README.replace("| 2 |", f"| {clauses} |"), encoding="utf-8"
    )


A_THIRD_CLAUSE = "### DEMO-3: the gate records who opened it"
ITS_COVERAGE = "`verified-by: by-construction (the caller is on the entry)`"


def insert_a_clause_and_renumber(root):
    """A clause inserted in the middle, with every clause after it renumbered to close the gap.

    The shape a per-file counter cannot see: the ids still read 1..N, with no gap, no duplicate and
    no wrong prefix, so nothing about the file alone is amiss. What is amiss is that DEMO-2 is a
    different clause from the DEMO-2 an issue cited, and the link in that issue still resolves."""
    edit_spec(
        root,
        '<a id="DEMO-2"></a>\n### DEMO-2: nothing else can open it',
        f'<a id="DEMO-2"></a>\n{A_THIRD_CLAUSE.replace("DEMO-3", "DEMO-2")}\n\n{ITS_COVERAGE}\n\n'
        '<a id="DEMO-3"></a>\n### DEMO-3: nothing else can open it',
    )
    readme_lists(root, 3)


def insert_a_clause_under_the_next_id(root):
    """A clause inserted mid-spec under the next free id, so the file reads DEMO-1, DEMO-3,
    DEMO-2. Nothing moved and no id is missing."""
    edit_spec(
        root,
        '<a id="DEMO-2"></a>\n',
        f'<a id="DEMO-3"></a>\n{A_THIRD_CLAUSE}\n\n{ITS_COVERAGE}\n\n<a id="DEMO-2"></a>\n',
    )
    readme_lists(root, 3)


def append_a_clause(root):
    """A clause at the end under the next free id. Nothing moved, so nothing is wrong, and a
    check firing on any new id would stop anybody writing one."""
    path = root / "docs" / "specs" / "demo.md"
    path.write_text(
        f'{path.read_text(encoding="utf-8")}\n<a id="DEMO-3"></a>\n{A_THIRD_CLAUSE}\n\n'
        f"{ITS_COVERAGE}\n",
        encoding="utf-8",
    )
    readme_lists(root, 3)


def append_a_reworded_heading(root):
    """A new clause whose heading is a rewording of one already in the file.

    Nothing moved: the old heading is still under its own id, and the new id is new. A comparison
    that asked only whether a heading is in the file twice over would report this."""
    path = root / "docs" / "specs" / "demo.md"
    path.write_text(
        f'{path.read_text(encoding="utf-8")}\n<a id="DEMO-3"></a>\n'
        "### DEMO-3: nothing else may open it\n\n"
        "`verified-by: by-construction (the field is private)`\n",
        encoding="utf-8",
    )
    readme_lists(root, 3)


def withdraw_in_place(root):
    """A clause withdrawn where it stands, keeping its old wording behind the word.

    Its heading changed and no id took the old one, which is the commonest changed heading in this
    tree's history: 73 commits reworded one, against 4 clauses that moved. Faulting a changed
    heading rather than a moved clause fails every one of them."""
    edit_spec(
        root,
        "### DEMO-2: nothing else can open it",
        "### DEMO-2: withdrawn, nothing else can open it",
    )
    edit_spec(root, "`verified-by: by-construction (the field is private)`", "Replaced by DEMO-1.")
    readme_lists(root, 1)


def add_a_spec_this_branch_wrote(root):
    """A spec file with no committed form at all, which is every spec on its first commit.

    Its headings are the other spec's word for word, so a comparison pooling the tree rather than
    reading each file against its own committed form would report every clause in it as one whose
    id was taken from something."""
    (root / "docs" / "specs" / "fresh.md").write_text(
        CLEAN_SPEC.replace("DEMO", "FRESH"), encoding="utf-8"
    )
    (root / "docs" / "specs" / "README.md").write_text(
        f"{CLEAN_README}| [fresh.md](fresh.md) | `FRESH` | 2 | another demonstration |\n",
        encoding="utf-8",
    )


def unpack_the_tree_outside_git(root):
    """A spec tree with no checkout around it, which is what a release tarball is and what a fresh
    `git init` with nothing committed amounts to. There is no record of what an id named, so this
    one check says nothing and every other still runs."""
    shutil.rmtree(root / ".git")


def declare_the_renumber(root):
    """The renumber a person meant: a spec withdrawn and rewritten wholesale.

    The line names the id the clause carries now and the heading it carries now, so it stays a fact
    about the tree, and it is a line in a diff rather than a variable somebody exported."""
    insert_a_clause_and_renumber(root)
    (root / check.RENUMBERED_FILE).write_text(
        "# demo.md was withdrawn and rewritten.\n"
        "docs/specs/demo.md:DEMO-3: nothing else can open it\n",
        encoding="utf-8",
    )


def renumber_again_past_the_declaration(root):
    """A second move of an id the declarations file already names.

    The entry pins the heading, so it answers for the renumber it was written for and no other. A
    declaration naming the id alone would exempt that id from then on, which is how one line comes
    to stand for every renumber anybody makes afterwards."""
    insert_a_clause_and_renumber(root)
    (root / check.RENUMBERED_FILE).write_text(
        f'docs/specs/demo.md:DEMO-3: {A_THIRD_CLAUSE.split(": ", 1)[1]}\n', encoding="utf-8"
    )


def commit_a_renumber_on_a_branch(root):
    """The renumber committed on a branch cut from origin/main, which is how it reaches CI."""
    git(root, "update-ref", "refs/remotes/origin/main", "HEAD")
    insert_a_clause_and_renumber(root)
    git(root, "commit", "-qam", "renumber")


def branch_and_main_move_on(root):
    """A renumber committed on a branch, then an empty commit on main, returning both heads."""
    before = git(root, "rev-parse", "HEAD")
    insert_a_clause_and_renumber(root)
    git(root, "commit", "-qam", "renumber")
    branch = git(root, "rev-parse", "HEAD")
    git(root, "checkout", "-q", "--detach", before)
    git(root, "commit", "-q", "--allow-empty", "-m", "main moves on")
    return branch, git(root, "rev-parse", "HEAD")


def merge_a_renumber_as_a_pull_request(root):
    """The checkout CI makes of a pull request: main merging the branch, main the first parent."""
    branch, _ = branch_and_main_move_on(root)
    git(root, "merge", "-q", "--no-ff", "-m", "merge the pull request", branch)


def merge_main_into_a_renumbered_branch(root):
    """Main merged into the branch locally, so the first parent is the branch and not main."""
    branch, main = branch_and_main_move_on(root)
    git(root, "update-ref", "refs/remotes/origin/main", main)
    git(root, "checkout", "-q", "--detach", branch)
    git(root, "merge", "-q", "--no-ff", "-m", "merge main", main)


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
        "a spec saying nothing about which pages describe it",
        lambda root: edit_spec(root, f"{DOCUMENTED_BY}\n", ""),
        "spec-undocumented",
    ),
    (
        "a spec naming a page that is not there",
        lambda root: edit_spec(root, DOCUMENTED_BY, "documented-by: docs/website/docs/gone.md"),
        "documented-by-missing",
    ),
    (
        "a spec naming the page from the site's own directory instead of the repository root",
        lambda root: edit_spec(root, DOCUMENTED_BY, "documented-by: demo.md"),
        "documented-by-missing",
    ),
    (
        "a spec saying none without saying why",
        lambda root: edit_spec(root, DOCUMENTED_BY, "documented-by: none"),
        "none-unexplained",
    ),
    (
        "a spec saying none with a reason is answered, not undocumented",
        lambda root: edit_spec(
            root, DOCUMENTED_BY, "documented-by: none (internal: nobody reads it)"
        ),
        None,
    ),
    (
        "a gap in a spec's ids",
        lambda root: edit_spec(root, "### DEMO-2:", "### DEMO-4:"),
        "clause-numbering",
    ),
    ("a clause inserted mid-spec under the next free id", insert_a_clause_under_the_next_id, None),
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
        # The one the counter above cannot see, and the reason this check reads history at all.
        # Exact, because a renumber is the only thing wrong with that file.
        "a clause inserted mid-spec with every clause after it renumbered",
        insert_a_clause_and_renumber,
        {"clause-renumbered"},
    ),
    ("a clause appended at the end", append_a_clause, None),
    ("a new clause whose heading rewords one already there", append_a_reworded_heading, None),
    (
        "a heading reworded under the id it belongs to",
        lambda root: edit_spec(
            root, "### DEMO-1: the gate opens only once", "### DEMO-1: the gate opens exactly once"
        ),
        None,
    ),
    ("a clause withdrawn where it stands", withdraw_in_place, None),
    ("a spec file this branch wrote, with no committed form", add_a_spec_this_branch_wrote, None),
    ("a spec tree with no checkout around it", unpack_the_tree_outside_git, None),
    ("a renumber a person declared", declare_the_renumber, None),
    (
        "an id moved again past the declaration that named it",
        renumber_again_past_the_declaration,
        {"clause-renumbered", "renumber-declaration-stale"},
    ),
    (
        "a declaration naming a clause the tree has not got",
        lambda root: (root / check.RENUMBERED_FILE).write_text(
            "docs/specs/demo.md:DEMO-9: a clause that was never there\n", encoding="utf-8"
        ),
        {"renumber-declaration-stale"},
    ),
    (
        "a declaration that is not a path, an id and a heading",
        lambda root: (root / check.RENUMBERED_FILE).write_text(
            "docs/specs/demo.md DEMO-3 nothing else can open it\n", encoding="utf-8"
        ),
        {"renumber-declaration-malformed"},
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
        "a guarded symbol renamed away while another type keeps the name",
        rename_the_gate_past_its_guard,
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
    (
        "an every-spelling that is not yes",
        lambda root: edit_spec(root, "every-spelling: yes", "every-spelling: true"),
        "guard-every-spelling-malformed",
    ),
    (
        "a guarded method called through a generic path",
        open_the_gate_by_another_spelling("fn reach(g: &Gate) {\n    <Gate>::open(g);\n}"),
        "guard-spelling-unseen",
    ),
    (
        "a guarded method called through an alias",
        open_the_gate_by_another_spelling("use Gate as G;\n\nfn reach(g: &Gate) {\n    G::open(g);\n}"),
        "guard-spelling-unseen",
    ),
    (
        "a guarded method called inside a macro body",
        open_the_gate_by_another_spelling(
            "macro_rules! reach {\n    ($g:expr) => {\n        $g.open()\n    };\n}"
        ),
        "guard-spelling-unseen",
    ),
    (
        "a guarded method named as a function value through an alias",
        open_the_gate_by_another_spelling(
            "use Gate as G;\n\nfn reach(gates: &[Gate]) {\n    gates.iter().for_each(G::open);\n}"
        ),
        "guard-spelling-unseen",
    ),
    (
        "a guarded method called with a turbofish",
        open_the_gate_by_another_spelling("fn reach(g: &Gate) {\n    g.open::<u8>();\n}"),
        "guard-spelling-unseen",
    ),
    (
        "a macro body holding a brace in a string, then a later call",
        open_the_gate_by_another_spelling(
            'macro_rules! shout {\n    () => {\n        println!("{")\n    };\n}\n\n'
            "fn reach(g: &Gate) {\n    g.open();\n}"
        ),
        {"guard-site-count"},
    ),
    (
        "a guarded method called through the qualified name",
        open_the_gate_by_another_spelling("fn reach(g: &Gate) {\n    Gate::open(g);\n}"),
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
        "a guarded free function in the file its module is",
        guard_a_function_in_its_module_file,
        None,
    ),
    ("a guarded free function in a mod block", guard_a_function_in_a_mod_block, None),
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
        "the register's front matter key, which the security-audit skill reads",
        lambda root: edit_spec(root, "guards:\n", "kept_in:\n  - Confirmer::ask: nothing\nguards:\n"),
        None,
    ),
    (
        "the key steps' front matter key, which the security-audit skill reads",
        lambda root: edit_spec(
            root,
            "guards:\n",
            "key_step_allowed:\n  - crates/demo/src/lib.rs::key trim: the reason\nguards:\n",
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


# The contrast the committed comparison exists for, held as a case of its own so that folding it
# back into the counter fails here rather than going quiet. Both halves: the counter cannot see a
# renumber, and it is still the only thing that sees a gap.
COUNTER_ALONE = [
    ("the counter alone passes a renumber", insert_a_clause_and_renumber, []),
    (
        "the counter alone still catches a gap",
        lambda root: edit_spec(root, "### DEMO-2:", "### DEMO-4:"),
        ["clause-numbering"],
    ),
]


def numbering_alone():
    """What the per-file numbering check reports with nothing else running."""
    return sorted({f["kind"] for one in load_specs() for f in check.check_clause_numbering(one)})


# A renumber that is already committed, which is the only form CI sees. Each case names the
# GITHUB_EVENT_NAME it runs under, because that decides whether a merge's first parent is main.
COMMITTED = [
    ("a renumber committed on a branch", commit_a_renumber_on_a_branch, None),
    ("a renumber in a pull request's merge", merge_a_renumber_as_a_pull_request, "pull_request"),
    ("a renumber on a branch that merged main", merge_main_into_a_renumbered_branch, None),
]


def under_event(event):
    """What `run_checks` reports with GITHUB_EVENT_NAME set to `event`, or unset for `None`."""
    saved = os.environ.pop("GITHUB_EVENT_NAME", None)
    if event is not None:
        os.environ["GITHUB_EVENT_NAME"] = event
    try:
        return sorted({f["kind"] for f in run_checks()})
    finally:
        os.environ.pop("GITHUB_EVENT_NAME", None)
        if saved is not None:
            os.environ["GITHUB_EVENT_NAME"] = saved


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
# A clause whose behaviour no code attempts. Nothing is broken by it, so it is not a bug.
NEVER_BUILT = dict(VIOLATION, absent=True, clause="DEMO-2")
# A run that did not finish, and a review that could not tell. Neither names anything to go and fix.
UNFINISHED = dict(VIOLATION, kind="review-incomplete", clause="DEMO-2")
UNDECIDED = dict(VIOLATION, kind="unclear", severity="warning", clause="DEMO-2")
# A mechanical error fails `make check-spec`, so it is red on the branch that caused it.
RED_IN_CI = dict(UNCOVERED, kind="clause-numbering", severity="error", clause="DEMO-2")

# What a clause kept from the record is, in the report: drafted by the run that decided it.
CARRIED_VIOLATION = dict(VIOLATION, clause="DEMO-2", carried=True, recorded="0123456789ab")
CARRIED_GAP = dict(UNCOVERED, kind="untested", clause="DEMO-1", carried=True, recorded="0123456789ab")

THE_TOKEN = "github.com:\n  oauth_token: gho_SELFTEST\n"
A_SCREEN = "the gate is open\nand a ``` fence, which must not close the block\n"

# What a cut takes off a summary, which is where a reviewer puts the cost of the defect.
THE_COST = "so every caller past the first gets in"


def summary_of(length):
    """A summary of exactly `length` characters, ending on the cost a cut would take off.

    Whether a title survives is a question about lengths either side of a boundary, so the fixture
    states the length and the prose is filler. What is not filler is the end: a summary that lost
    it reads as complete, which is why a cut here went unnoticed until two issues were filed.
    """
    filler = "the gate opens twice, " + "and again, " * 30
    return filler[: length - len(THE_COST)] + THE_COST


def manifest_checks():
    """What every step of the pipeline reads: the manifest, or a message and None."""
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "manifest.json").write_text('{"strict": true}', encoding="utf-8")
        found = draft.collect.read_manifest(tmp)
        empty = Path(tmp) / "empty"
        empty.mkdir()
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            missing = draft.collect.read_manifest(empty)
    return [
        ("a manifest in the work directory is read", found == {"strict": True}),
        ("a work directory without one reads as none and says where it looked",
         missing is None and err.getvalue() == f"no manifest at {empty / 'manifest.json'}\n"),
    ]


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
            "a finding the record carried is not drafted again, whatever its severity",
            draft.draftable([VIOLATION, CARRIED_VIOLATION, CARRIED_GAP], errors_only=False) == [VIOLATION],
        ),
        (
            "two findings on one clause get a body each",
            by_kind["violation"]["body_file"] != by_kind["clause-uncovered"]["body_file"],
        ),
        ("the title leads with the clause", by_kind["violation"]["title"].startswith("DEMO-1: ")),
        (
            "a violation is a mismatch, and a bug in what ships today",
            by_kind["violation"]["labels"] == ["spec-mismatch", "bug"],
        ),
        (
            "a clause whose behaviour was never built is missing rather than broken",
            draft.labels_for(NEVER_BUILT) == ["spec-mismatch", "enhancement"],
        ),
        (
            "a clause nothing pins is labelled coverage and nothing else",
            by_kind["clause-uncovered"]["labels"] == ["spec-coverage"],
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

    # Either side of GitHub's cap, counting the `DEMO-N: ` the title leads with.
    fits, over = draft.draft(
        [
            dict(VIOLATION, clause="DEMO-1", summary=summary_of(200)),
            dict(VIOLATION, clause="DEMO-2", summary=summary_of(300)),
        ],
        out,
    )
    checks += [
        (
            "a title longer than a tracker list shows is still filed whole",
            fits["title"] == f"DEMO-1: {summary_of(200)}" and not fits["title_too_long"],
        ),
        (
            "a title over GitHub's cap keeps the cost the end of it carries",
            over["title"] == f"DEMO-2: {summary_of(300)}" and over["title"].endswith(THE_COST),
        ),
        (
            "a title over the cap is marked for a person to shorten",
            over["title_too_long"]
            and "needs a shorter title" in draft.report([over], out)
            and "needs a shorter title" not in draft.report([fits], out),
        ),
    ]
    return checks


def site_checks():
    """Which place a body will open and show, against one a lane could have been steered to write.

    A place arrives as prose from a model reading a tree somebody else wrote, and whatever the
    drafter opens is inlined into a body that gets filed on a tracker. So the only file a body may
    quote is one the tree already publishes: a path out of the checkout reaches the machine the run
    happened on, and one the checkout ignores reaches the credentials it keeps there.
    """
    outside = Path(tempfile.mkdtemp(prefix="check-spec-selftest-outside-")) / "hosts.yml"
    outside.write_text(THE_TOKEN, encoding="utf-8")
    climbing = os.path.relpath(outside.resolve(), Path.cwd())
    ignored = Path.cwd() / "secrets.env"
    ignored.write_text(THE_TOKEN, encoding="utf-8")
    linked = Path.cwd() / "crates" / "demo" / "src" / "linked.rs"
    linked.symlink_to(outside)
    # Tracked, so what rejects it is where it points rather than that nothing published it.
    subprocess.run(["git", "add", "crates/demo/src/linked.rs"], check=True)
    draft._TRACKED.clear()
    gate = (Path("crates/demo/src/lib.rs"), 4)
    try:
        return [
            (
                "a place the tree publishes is the code a body shows",
                draft.first_site(["crates/demo/src/lib.rs:4 the gate"]) == gate,
            ),
            (
                "an absolute place outside the checkout is not opened",
                draft.first_site([f"{outside}:2 the token"]) == (None, None),
            ),
            (
                "a place that climbs out of the checkout is not opened",
                draft.first_site([f"{climbing}:2 the token"]) == (None, None),
            ),
            (
                "a file the checkout keeps but does not track is not opened",
                draft.first_site(["secrets.env:2 the token"]) == (None, None),
            ),
            (
                "a tracked name that is a symlink out of the tree is not opened",
                draft.first_site(["crates/demo/src/linked.rs:2 the token"]) == (None, None),
            ),
            (
                "a place a body may not quote is skipped rather than ending the search",
                draft.first_site([f"{outside}:2 the token", "crates/demo/src/lib.rs:4 the gate"])
                == gate,
            ),
            (
                "a place that names a range or a list of lines is opened at its first line",
                draft.first_site(["crates/demo/src/lib.rs:4-9 the gate"]) == gate
                and draft.first_site(["crates/demo/src/lib.rs:4,12-15 the gate"]) == gate,
            ),
        ]
    finally:
        shutil.rmtree(outside.parent, ignore_errors=True)


def bullet_checks():
    """How an evidence item is written into a body: the place in a code span, then what is there.

    A lane writes a place as one line, a range or several, and a body that closes the span after the
    first number prints the rest as if it were what is there.
    """
    return [
        (
            "a place with one line is a code span and what is there follows it",
            draft.as_bullet("crates/demo/src/lib.rs:4 the gate")
            == "- `crates/demo/src/lib.rs:4` the gate",
        ),
        (
            "a range of lines stays inside the code span",
            draft.as_bullet("crates/demo/src/lib.rs:4-9 the gate")
            == "- `crates/demo/src/lib.rs:4-9` the gate",
        ),
        (
            "a list of lines and ranges stays inside the code span",
            draft.as_bullet("crates/demo/src/lib.rs:4,12-15 the gate")
            == "- `crates/demo/src/lib.rs:4,12-15` the gate",
        ),
        (
            "a number that opens what is there is not taken for a line",
            draft.as_bullet("crates/demo/src/lib.rs:4 12 callers")
            == "- `crates/demo/src/lib.rs:4` 12 callers",
        ),
        (
            "an item that names no place is kept as written",
            draft.as_bullet("the clause says so") == "- the clause says so",
        ),
    ]


def nest_a_spec(root):
    """A spec in a subdirectory of `docs/specs`, which is where most of this tree's specs live
    and the only place a selector can carry a directory at all."""
    nested = root / "docs" / "specs" / "tools"
    nested.mkdir(parents=True)
    (nested / "demo-tool.md").write_text(
        CLEAN_SPEC.replace("DEMO-", "TOOLDEMO-").replace("id: DEMO\n", "id: TOOLDEMO\n"),
        encoding="utf-8",
    )


def selection_checks():
    """Which selectors name a spec.

    A scoped run that resolves nothing reports one mechanical error and writes no reviewer
    prompt, so every form the tree itself prints has to be among them: `grep -rl` prints a path,
    a link in a document carries one, and a person reading either types what they read.
    """
    specs = load_specs()
    nested = "docs/specs/tools/demo-tool.md"

    def picked(*selectors):
        chosen, unknown = check.select(specs, list(selectors), None)
        return [one.rel for one in chosen], unknown

    return [
        ("a path under docs/specs names its spec", picked("tools/demo-tool") == ([nested], [])),
        ("that path takes its suffix or leaves it", picked("tools/demo-tool.md") == ([nested], [])),
        (
            "a path from the repository root names its spec",
            picked("docs/specs/tools/demo-tool.md") == ([nested], []),
        ),
        (
            "that path takes its suffix or leaves it too",
            picked("docs/specs/tools/demo-tool") == ([nested], []),
        ),
        ("a file name carrying .md names its spec", picked("demo-tool.md") == ([nested], [])),
        ("a bare stem names its spec", picked("demo-tool") == ([nested], [])),
        ("an id names its spec, in any case", picked("TOOLdemo") == ([nested], [])),
        (
            "a path naming no spec is reported rather than passed over",
            picked("tools/absent") == ([], ["tools/absent"]),
        ),
        (
            "a directory typed in part names no spec",
            picked("ols/demo-tool") == ([], ["ols/demo-tool"]),
        ),
        (
            "an unknown name beside a known one leaves the known one chosen",
            picked("tools/demo-tool", "tools/absent") == ([nested], ["tools/absent"]),
        ),
    ]


# What a repeat run reviews. The fixture gets a second spec over files of its own, so that a change
# to one governed file can be told apart from a change to the other's, and the scripts under test
# copied in, so that they run as they do in a checkout: from a file whose third parent is the root.

SKILL = Path(__file__).resolve().parent
OTHER_SPEC = f"""\
---
id: OTHER
title: Another demonstration
status: normative
governs:
  - crates/demo/src/other/*.rs
{DOCUMENTED_BY}
---

## Clauses

<a id="OTHER-1"></a>
### OTHER-1: a second spec reads files of its own

`verified-by: bravebot_demo::other_test::a_second_spec_reads_only_its_own_files`
"""
THE_TEST = "#[test]\nfn a_second_spec_reads_only_its_own_files() {}\n"
ALL_THREE = ["DEMO-1", "DEMO-2", "OTHER-1"]
THE_VIOLATION = "the gate stays open"
THE_GAP = "no test pins that the second spec reads only its own files"


def build_incremental(root):
    """The case's `break_it`: the fixture, then a second spec listed in the README, and the scripts."""
    (root / "docs" / "specs" / "other.md").write_text(OTHER_SPEC, encoding="utf-8")
    (root / "docs" / "specs" / "README.md").write_text(
        CLEAN_README + "| [other.md](other.md) | `OTHER` | 1 | another demonstration |\n", encoding="utf-8"
    )
    other = root / "crates" / "demo" / "src" / "other"
    (other / "deep").mkdir(parents=True)
    (other / "a.rs").write_text("pub fn a() {}\n" * 20, encoding="utf-8")
    (other / "deep" / "c.rs").write_text("pub fn c() {}\n", encoding="utf-8")
    (root / "crates" / "demo" / "tests").mkdir()
    (root / "crates" / "demo" / "tests" / "other_test.rs").write_text(THE_TEST, encoding="utf-8")
    for source in sorted(SKILL.glob("*.py")):
        if source.name != "selftest.py":
            copy_into(root, source, f"agents/skills/check-spec/{source.name}")
    copy_into(root, SKILL.parents[1] / "work_dir.py", "agents/work_dir.py")
    copy_into(
        root,
        SKILL.parent / "peer-advisories" / "peer-advisories.py",
        "agents/skills/peer-advisories/peer-advisories.py",
    )
    git(root, "add", "-A")
    git(root, "commit", "-q", "-m", "a second spec, and the scripts that read it")
    # What a person's checkout of main has, which is the one tree a verdict may be recorded for.
    git(root, "update-ref", "refs/remotes/upstream/main", "HEAD")


def copy_into(root, source, target):
    (root / target).parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(source, root / target)


def recorded_at(commit=None, decided=None):
    """What a first run would have written: every clause of the fixture decided at `commit`, as
    conforming unless `decided` says otherwise."""
    commit = commit or git(Path.cwd(), "rev-parse", "HEAD")[:12]
    hashes = digests()
    record = {}
    for one in load_specs():
        for clause in one.clauses:
            verdict, severity, summary = (decided or {}).get(clause.id, ("conforms", "-", ""))
            record[clause.id] = {
                "verdict": verdict,
                "severity": severity,
                "commit": commit,
                "hash": hashes[clause.id],
                "date": "2026-10-20",
                "summary": summary,
            }
    return record


class Plan:
    """What `prepare_review` made of the fixture, with the prompts read before their directory went."""

    def __init__(self, entries, reasons, texts):
        self.entries = entries
        self.reasons = reasons
        self.texts = texts

    @property
    def prompts(self):
        return [p for e in self.entries for p in e["reviewer_prompts"]]

    def reviewing(self, mode):
        return sorted(c for p in self.prompts if p["mode"] == mode for c in p["clauses"])

    @property
    def kept(self):
        return sorted(k["clause"] for e in self.entries for k in e["kept"])


def plan_of(record, threshold=check.DIFF_BELOW):
    with tempfile.TemporaryDirectory(prefix="check-spec-selftest-work-") as work:
        entries, reasons = check.prepare_review(
            load_specs(),
            Path(work),
            check.Repo(Path.cwd()),
            record,
            check.load_sources(),
            crate_directories(),
            TestIndex(),
            6,
            threshold,
        )
        texts = {}
        for entry in entries:
            for prompt in entry["reviewer_prompts"]:
                texts[prompt["prompt_file"]] = Path(prompt["prompt_file"]).read_text(encoding="utf-8")
                if prompt.get("diff_file"):
                    texts[prompt["diff_file"]] = Path(prompt["diff_file"]).read_text(encoding="utf-8")
    return Plan(entries, reasons, texts)


def digests():
    """Each clause's hash, as `prepare_review` takes it: with the guarded uses and the instructions."""
    sources, index, crates = check.load_sources(), TestIndex(), crate_directories()
    repo = check.Repo(Path.cwd())
    found = {}
    for one in load_specs():
        patterns = check.inputs(one, index, crates)
        found.update(check.clause_hashes(one, one.clauses, sources, repo, patterns)[1])
    return found


def spec_file(name):
    return Path.cwd() / "docs" / "specs" / name


LABELS_SPEC = """\
---
id: LABEL
title: The rule every reviewer is told about
status: normative
governs:
  - crates/demo/src/lib.rs
---

## Known costs

- The gate may open twice in a test.
"""


def hash_checks():
    """What reviews a clause again: its own words, and the words of its spec that a reviewer reads
    beside it. What does not: another clause's, and anything a verdict was not read from."""
    labels = spec_file("labels.md")
    labels.write_text(LABELS_SPEC, encoding="utf-8")
    before = digests()
    demo = spec_file("demo.md")
    text = demo.read_text(encoding="utf-8")
    checks = []

    def after(change):
        demo.write_text(text, encoding="utf-8")
        edit_spec(Path.cwd(), *change)
        found = digests()
        demo.write_text(text, encoding="utf-8")
        return found

    reworded = after(("### DEMO-1: the gate opens only once", "### DEMO-1: the gate opens once"))
    checks += [
        ("rewording a clause changes its hash", reworded["DEMO-1"] != before["DEMO-1"]),
        ("rewording a clause leaves its neighbour's hash alone", reworded["DEMO-2"] == before["DEMO-2"]),
        ("a spec's hashes are not another spec's", before["DEMO-1"] != before["OTHER-1"]),
    ]
    renamed_test = after(("the_gate_opens_only_once`", "the_gate_opens_twice`"))
    checks.append(("the tests a clause names are part of what it says", renamed_test["DEMO-1"] != before["DEMO-1"]))

    inserted = after(
        ('<a id="DEMO-2"></a>\n', f'<a id="DEMO-3"></a>\n{A_THIRD_CLAUSE}\n\n{ITS_COVERAGE}\n\n<a id="DEMO-2"></a>\n')
    )
    checks.append(
        (
            "a clause inserted after another does not reword it, though the anchor ending its body changes",
            inserted["DEMO-1"] == before["DEMO-1"] and inserted["DEMO-2"] == before["DEMO-2"],
        )
    )
    spaced = after(("`verified-by: by-construction (the field is private)`", "`verified-by: by-construction (the field is private)`   \n\n\n"))
    checks.append(("trailing space and blank lines are not words", spaced["DEMO-2"] == before["DEMO-2"]))

    context = after(("## Clauses\n", "## Clauses\n\nA sentence the reviewer reads beside every clause.\n"))
    checks.append(
        (
            "text a spec carries before its clauses reviews every clause of it",
            context["DEMO-1"] != before["DEMO-1"] and context["DEMO-2"] != before["DEMO-2"],
        )
    )
    costs = after(("by-construction (the field is private)`\n", "by-construction (the field is private)`\n\n## Known costs\n\nThe gate may open twice in a test.\n"))
    checks.append(
        (
            "an exception a spec admits after its clauses reviews every clause of it",
            costs["DEMO-1"] != before["DEMO-1"] and costs["DEMO-2"] != before["DEMO-2"],
        )
    )
    moved = after(("  - crates/demo/src/lib.rs\nguards:", "  - crates/demo/src/lib.rs\n  - crates/demo/src/extra.rs\nguards:"))
    checks.append(
        (
            "a path added to what a spec governs reviews its clauses, which were never read against it",
            moved["DEMO-1"] != before["DEMO-1"] and moved["OTHER-1"] == before["OTHER-1"],
        )
    )
    pages = after((DOCUMENTED_BY, "documented-by: docs/website/docs/other.md"))
    checks.append(("the pages describing a spec are not what a verdict reads", pages == before))

    appended = after(("  - crates/demo/src/lib.rs\nguards:", "  - crates/demo/src/lib.rs\n  - crates/demo/src/extra.rs\nguards:"))
    reversed_ = after(("governs:\n  - crates/demo/src/lib.rs\n", "governs:\n  - crates/demo/src/extra.rs\n  - crates/demo/src/lib.rs\n"))
    checks.append(
        (
            "the order a spec lists what it governs in is not a change to it",
            appended["DEMO-1"] == reversed_["DEMO-1"] and appended["DEMO-1"] != before["DEMO-1"],
        )
    )
    flipped = after(("    every-spelling: yes\n", "    every-spelling: no\n"))
    checks.append(
        (
            "a change to how a guarded symbol is pinned reviews the clauses of the spec",
            flipped["DEMO-1"] != before["DEMO-1"] and flipped["OTHER-1"] == before["OTHER-1"],
        )
    )

    # A use of a guarded symbol in a file nobody watches, which no diff of the watched paths shows.
    use_the_gate_elsewhere(Path.cwd())
    used = digests()
    (Path.cwd() / "crates" / "demo" / "src" / "extra.rs").unlink()
    checks.append(
        (
            "a new use of a guarded symbol in an unwatched file reviews the clauses of the spec",
            used["DEMO-1"] != before["DEMO-1"] and used["OTHER-1"] == before["OTHER-1"],
        )
    )
    # A use in a file the spec watches is in the diff when it appears, so the hash leaves it out.
    use_the_gate_again(Path.cwd())
    watched_use = digests()
    git(Path.cwd(), "checkout", "--", "crates/demo/src/lib.rs")
    checks.append(
        (
            "a new use of a guarded symbol in a watched file is left to the diff",
            watched_use["DEMO-1"] == before["DEMO-1"],
        )
    )
    labels.write_text(LABELS_SPEC.replace("twice in a test", "twice anywhere"), encoding="utf-8")
    excepted = digests()
    labels.write_text(LABELS_SPEC, encoding="utf-8")
    checks.append(
        (
            "an exception added to the rule every reviewer is told about reviews every clause of every spec",
            all(excepted[c] != before[c] for c in before),
        )
    )
    told = check.GROUND_RULES
    check.GROUND_RULES = told + "\nAnd one more thing a reviewer is told.\n"
    try:
        retold = digests()
    finally:
        check.GROUND_RULES = told
    checks.append(
        (
            "a change to what a reviewer is told reviews every clause",
            all(retold[c] != before[c] for c in before),
        )
    )
    return checks


def change_of(added, deleted, size, untracked=()):
    return check.Change({"p.rs": (added, deleted)}, {"p.rs": size}, list(untracked))


def asked_nothing():
    """Whether `decide` leaves git alone for a clause whose hash differs, which no diff can change."""
    asked = []
    check.decide({"hash": "h"}, "other", lambda: asked.append(1), 0.5)
    return asked == []


def decide_checks():
    """The order of the questions and the edges of the threshold, with no repository in it."""
    held = {"hash": "h"}
    small = change_of(1, 0, 100)

    def mode(record, hashed, change, threshold=0.5):
        return check.decide(record, hashed, lambda: change, threshold)[0]

    return [
        ("no record is a full review", check.decide(None, "h", lambda: None, 0.5) == (check.FULL, check.NO_RECORD)),
        (
            "other words are a full review, whatever the code did",
            check.decide(held, "other", lambda: check.Change({}, {}, []), 0.5) == (check.FULL, check.CHANGED),
        ),
        (
            "git is not asked about a clause whose words changed",
            asked_nothing(),
        ),
        ("a commit with nothing to compare is a full review", check.decide(held, "h", lambda: None, 0.5) == (check.FULL, check.NOT_COMPARABLE)),
        ("an untracked file is a full review, since no diff shows it", check.decide(held, "h", lambda: change_of(0, 0, 100, ["new.rs"]), 0.5) == (check.FULL, check.UNTRACKED)),
        ("no change is no review", mode(held, "h", check.Change({}, {"p.rs": 100}, [])) == check.KEEP),
        (
            "a change smaller than a percent is not drawn as none",
            check.share(0.001) == "under 1%" and check.share(0) == "0%" and check.share(0.153) == "15%",
        ),
        ("a change under the threshold is a review of the diff", mode(held, "h", small) == check.DIFF),
        (
            "a change at the threshold is a full review",
            check.decide(held, "h", lambda: change_of(25, 0, 100), 0.25) == (check.FULL, check.OVER),
        ),
        ("a change one line under it is a review of the diff", mode(held, "h", change_of(24, 0, 100), 0.25) == check.DIFF),
        (
            "a file counts at most once over its length",
            # Sixty lines added and sixty deleted in a hundred-line file beside three hundred
            # unchanged ones is a quarter of the lines. Uncapped, it would count three tenths.
            check.Change({"p.rs": (60, 60)}, {"p.rs": 100, "q.rs": 300}, []).fraction == 0.25
            and mode(held, "h", check.Change({"p.rs": (60, 60)}, {"p.rs": 100, "q.rs": 300}, []), 0.28) == check.DIFF,
        ),
    ]


def incremental_checks():
    """What a run reviews, in the fixture, against a record of an earlier run at its first commit."""
    root = Path.cwd()
    lib = root / "crates" / "demo" / "src" / "lib.rs"
    original = lib.read_text(encoding="utf-8")
    start = git(root, "rev-parse", "HEAD")[:12]
    decided = {"DEMO-2": ("violation", "error", THE_VIOLATION), "OTHER-1": ("untested", "warning", THE_GAP)}
    record = recorded_at(decided=decided)
    checks = []

    def case(name, held):
        checks.append((name, bool(held)))

    unchanged = plan_of(record)
    case("a second run on an unchanged tree starts no reviewers", unchanged.prompts == [] and unchanged.kept == ALL_THREE)
    case(
        "what it keeps is what the record held",
        {k["clause"]: k["verdict"] for e in unchanged.entries for k in e["kept"]}
        == {"DEMO-1": "conforms", "DEMO-2": "violation", "OTHER-1": "untested"},
    )

    unrecorded = plan_of({k: v for k, v in record.items() if k != "OTHER-1"})
    case(
        "a clause with no record is reviewed in full, and the others are kept",
        unrecorded.reviewing("full") == ["OTHER-1"]
        and unrecorded.kept == ["DEMO-1", "DEMO-2"]
        and unrecorded.reasons == {check.NO_RECORD: 1},
    )

    demo = spec_file("demo.md")
    spec_text = demo.read_text(encoding="utf-8")
    edit_spec(root, "### DEMO-1: the gate opens only once", "### DEMO-1: the gate opens once")
    reworded = plan_of(record)
    demo.write_text(spec_text, encoding="utf-8")
    case(
        "a clause whose words changed is reviewed in full, and its neighbours are kept",
        reworded.reviewing("full") == ["DEMO-1"]
        and reworded.kept == ["DEMO-2", "OTHER-1"]
        and reworded.reasons == {check.CHANGED: 1},
    )
    edit_spec(
        root,
        '<a id="DEMO-2"></a>\n',
        f'<a id="DEMO-3"></a>\n{A_THIRD_CLAUSE}\n\n{ITS_COVERAGE}\n\n<a id="DEMO-2"></a>\n',
    )
    added = plan_of(record)
    demo.write_text(spec_text, encoding="utf-8")
    case(
        "a clause added between two others is the only one reviewed",
        added.reviewing("full") == ["DEMO-3"] and added.kept == ALL_THREE and added.reasons == {check.NO_RECORD: 1},
    )

    lib.write_text(original + "\nfn added() {}\n", encoding="utf-8")
    changed = plan_of(record)
    case(
        "a change confined to one governed file starts reviewers for the spec that governs it only",
        changed.reviewing("diff") == ["DEMO-1", "DEMO-2"]
        and changed.reviewing("full") == []
        and changed.kept == ["OTHER-1"],
    )
    # One reviewer for the two clauses. Read as a plain lookup so that a plan with none fails a case
    # instead of ending the run.
    prompt = changed.prompts[0] if len(changed.prompts) == 1 else {}
    diff_file = prompt.get("diff_file", "")
    case(
        "each of them is given the diff since the commit it was reviewed at",
        diff_file.endswith(f"since-{start}.diff") and "+fn added() {}" in changed.texts.get(diff_file, ""),
    )
    text = changed.texts.get(prompt.get("prompt_file"), "")
    case(
        "and the verdict it is asked about, with the summary that went with it",
        THE_VIOLATION in text
        and "`DEMO-1`: conforms" in text
        and f"last reviewed at commit `{start}`" in text
        and prompt.get("previous", {}).get("DEMO-2", {}).get("verdict") == "violation",
    )
    case(
        "and is told to keep the earlier verdict by saying so",
        '"holds": true' in text and "Mark a clause `\"holds\": true` only when you have read the diff" in text,
    )
    full_prompt = unrecorded.prompts[0] if len(unrecorded.prompts) == 1 else {}
    full_text = unrecorded.texts.get(full_prompt.get("prompt_file"), "")
    fields = check.CLAUSE_FIELDS.splitlines()
    case(
        "the full prompt and the diff prompt each show the reviewer every field of a clause's entry",
        len(fields) == 8
        and full_prompt.get("mode") == "full"
        and prompt.get("mode") == "diff"
        and all(line in full_text.splitlines() for line in fields)
        and all(line in text.splitlines() for line in fields),
    )
    git(root, "config", "color.ui", "always")
    git(root, "config", "diff.external", "false")
    coloured = plan_of(record)
    git(root, "config", "--unset", "color.ui")
    git(root, "config", "--unset", "diff.external")
    case(
        "the diff is plain whatever colour or external diff program the person has configured",
        all(
            "\x1b[" not in coloured.texts[p["diff_file"]] and "+fn added() {}" in coloured.texts[p["diff_file"]]
            for p in coloured.prompts
            if p["mode"] == "diff"
        )
        and coloured.reviewing("diff") == ["DEMO-1", "DEMO-2"],
    )
    git(root, "add", "-A")
    git(root, "commit", "-q", "-m", "a change to one governed file")
    committed = plan_of(record)
    case(
        "a committed change is reviewed as an uncommitted one is",
        committed.reviewing("diff") == ["DEMO-1", "DEMO-2"]
        and all("+fn added() {}" in committed.texts[p["diff_file"]] for p in committed.prompts if p["mode"] == "diff"),
    )

    # A clause reviewed later is compared from the commit it was reviewed at, which is after the first.
    later = git(root, "rev-parse", "HEAD")[:12]
    mixed = dict(record, **{"DEMO-2": dict(record["DEMO-2"], commit=later)})
    lib.write_text(lib.read_text(encoding="utf-8") + "\nfn added_again() {}\n", encoding="utf-8")
    split = plan_of(mixed)
    since = {
        p["previous"][p["clauses"][0]]["commit"]: split.texts[p["diff_file"]]
        for p in split.prompts
        if p["mode"] == "diff"
    }
    case(
        "clauses recorded at different commits are given a diff each",
        sorted(since) == sorted([start, later])
        and "+fn added() {}" in since[start]
        and "+fn added_again() {}" in since[start]
        and "+fn added() {}" not in since[later]
        and "+fn added_again() {}" in since[later],
    )

    # Most of the file replaced, with the impl that holds the guarded definitions left as it was, so
    # that the hash is the same and only the size of the change decides.
    kept_impl = "".join(original.splitlines(keepends=True)[:9])
    lib.write_text(kept_impl + "fn rewritten() {}\n" * 40, encoding="utf-8")
    rewritten = plan_of(record)
    case(
        "a diff over the threshold is a full review, and the other spec is still kept",
        rewritten.reviewing("full") == ["DEMO-1", "DEMO-2"]
        and rewritten.kept == ["OTHER-1"]
        and rewritten.reasons == {check.OVER: 2},
    )
    lib.write_text(original + "\nfn added() {}\n", encoding="utf-8")
    case(
        "a threshold of nothing reviews every changed clause in full",
        plan_of(record, threshold=0).reviewing("full") == ["DEMO-1", "DEMO-2"],
    )
    lib.write_text(original, encoding="utf-8")
    git(root, "reset", "-q", "--hard", start)
    case(
        "a tree back where the record was kept nothing",
        plan_of(record).prompts == [],
    )

    named = root / "crates" / "demo" / "tests" / "other_test.rs"
    named.write_text(THE_TEST + "// the test, emptied\n", encoding="utf-8")
    emptied = plan_of(record)
    case(
        "a change to a test a clause names, in a file nobody governs, is read as a diff",
        emptied.reviewing("diff") == ["OTHER-1"] and emptied.kept == ["DEMO-1", "DEMO-2"],
    )
    named.write_text(THE_TEST, encoding="utf-8")

    lib.write_text("\n".join(original.splitlines()[:7]) + "\n", encoding="utf-8")
    shrunk = check.Repo(root).since(start, ["crates/demo/src/lib.rs"])
    case(
        "a file cut short is measured against the length it had",
        shrunk is not None and shrunk.sizes["crates/demo/src/lib.rs"] == len(original.splitlines()),
    )
    lib.write_text(original, encoding="utf-8")
    lib.chmod(0o755)
    case("a change of mode alone is no change to what a clause is about", plan_of(record).prompts == [])
    lib.chmod(0o644)

    deep = root / "crates" / "demo" / "src" / "other" / "deep" / "c.rs"
    deep.write_text("pub fn c() { /* changed */ }\n", encoding="utf-8")
    case(
        "a change in a directory a glob does not reach is not a change to what the spec governs",
        plan_of(record).prompts == [],
    )
    deep.write_text("pub fn c() {}\n", encoding="utf-8")
    fresh = root / "crates" / "demo" / "src" / "other" / "b.rs"
    fresh.write_text("pub fn b() {}\n", encoding="utf-8")
    untracked = plan_of(record)
    case(
        "a file git does not know in what a spec governs is a full review, since no diff shows it",
        untracked.reviewing("full") == ["OTHER-1"]
        and untracked.reasons == {check.UNTRACKED: 1}
        and untracked.kept == ["DEMO-1", "DEMO-2"],
    )
    case(
        "a tree with a file git does not know in what a spec governs is not a commit",
        check.Repo(root).dirty(["crates/demo/src/other/*.rs"]) == ["crates/demo/src/other/b.rs"],
    )
    fresh.unlink()
    case("a tree that is a commit is clean", check.Repo(root).dirty(["crates/demo/src/other/*.rs"]) == [])
    scratch = root / "crates" / "demo" / "src" / "scratch.rs"
    scratch.write_text("fn scratch() {}\n", encoding="utf-8")
    paths = check.review_paths(load_specs(), TestIndex(), crate_directories())
    case(
        "a source git does not know is a difference, since the guarded uses are counted in every source",
        check.Repo(root).dirty(paths) == ["crates/demo/src/scratch.rs"],
    )
    scratch.unlink()
    both = load_specs()
    twice = check.review_paths(both + both, TestIndex(), crate_directories())
    case("a path two specs watch is listed once", len(twice) == len(set(twice)))
    lib.write_text(original + "\nfn added() {}\n", encoding="utf-8")
    case(
        "a tracked file that differs is not a commit either",
        check.Repo(root).dirty([]) == ["crates/demo/src/lib.rs"],
    )
    lib.write_text(original, encoding="utf-8")
    reviewed.write_ledger(root / reviewed.LEDGER, record)
    git(root, "add", "-A")
    git(root, "commit", "-q", "-m", "a record")
    reviewed.write_ledger(root / reviewed.LEDGER, {})
    case("the record is not a difference, since a run writes it", check.Repo(root).dirty([]) == [])
    git(root, "reset", "-q", "--hard", "HEAD")

    elsewhere = git(root, "commit-tree", "HEAD^{tree}", "-m", "a commit on no branch")[:12]
    for name, commit in (("one this history never held", "0123456789ab"), ("one this tree does not descend from", elsewhere)):
        stranded = plan_of(recorded_at(commit=commit))
        case(
            f"a record at {name} is reviewed in full",
            stranded.reviewing("full") == ALL_THREE and stranded.reasons == {check.NOT_COMPARABLE: 3},
        )
    return checks


def run_script(name, *arguments):
    return subprocess.run(
        [sys.executable, "-B", str(Path.cwd() / "agents" / "skills" / "check-spec" / name), *arguments],
        capture_output=True,
        text=True,
        check=False,
    )


def prepared(work, *arguments):
    """The manifest `check-spec.py` writes for a run of the fixture, started as it is in a checkout."""
    done = run_script("check-spec.py", "--work-dir", str(work), *arguments)
    assert done.returncode == 0, done.stderr
    return json.loads((Path(work) / "manifest.json").read_text(encoding="utf-8"))


def answer(manifest, **answers):
    """Write what reviewers would have written, per clause, for each prompt of `manifest` that has one."""
    for entry in manifest["specs"]:
        for prompt in entry["reviewer_prompts"]:
            clauses = [dict(answers[c], clause=c) for c in prompt["clauses"] if c in answers]
            if clauses:
                Path(prompt["results_file"]).write_text(json.dumps({"clauses": clauses}), encoding="utf-8")


def modes(manifest):
    return sorted((c, p["mode"]) for e in manifest["specs"] for p in e["reviewer_prompts"] for c in p["clauses"])


def ledger_of():
    """What the record holds, or nothing where it cannot be read, so that a script that wrote a line
    the record refuses fails the case that read it."""
    try:
        return reviewed.read_ledger(Path.cwd() / reviewed.LEDGER)
    except reviewed.Problem:
        return {}


def record_checks():
    """The runs a person makes in turn, through the scripts: review, record, review again."""
    root = Path.cwd()
    lib = root / "crates" / "demo" / "src" / "lib.rs"
    ledger = root / reviewed.LEDGER
    checks = []

    def case(name, held):
        checks.append((name, bool(held)))

    with tempfile.TemporaryDirectory(prefix="check-spec-selftest-runs-") as runs:
        runs = Path(runs)
        # A line for a clause no spec has any more, left by a spec that was edited since.
        gone = recorded_at(decided={"DEMO-1": ("conforms", "-", "")})["DEMO-1"]
        reviewed.write_ledger(ledger, {"GONE-1": gone})
        stale = ledger.read_bytes()
        first = prepared(runs / "first")
        case(
            "a first run reviews every clause in full",
            modes(first) == [(c, "full") for c in ALL_THREE] and first["commit"] == git(root, "rev-parse", "HEAD"),
        )
        answer(
            first,
            **{
                "DEMO-1": {"verdict": "conforms"},
                "DEMO-2": {"verdict": "violation", "severity": "error", "summary": THE_VIOLATION, "failure": "open it twice"},
                # A severity the record has no place for, from a reviewer that made one up.
                "OTHER-1": {"verdict": "untested", "summary": THE_GAP, "severity": "high"},
            },
        )
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "first"), "--dry-run")
        case("a dry run records nothing", done.returncode == 0 and ledger.read_bytes() == stale)
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "first"))
        written = ledger_of() if done.returncode == 0 else {}
        head = git(root, "rev-parse", "HEAD")[:12]
        case(
            "recording writes what the reviewers decided, at the main commit they read, and drops a clause no spec has",
            {c: (e["verdict"], e["severity"], e["commit"]) for c, e in written.items()}
            == {
                "DEMO-1": ("conforms", "-", head),
                "DEMO-2": ("violation", "error", head),
                "OTHER-1": ("untested", "warning", head),
            }
            and written["DEMO-2"]["summary"] == THE_VIOLATION
            and written["DEMO-1"]["hash"] == digests()["DEMO-1"],
        )

        second = prepared(runs / "second")
        case("a second run on the same tree starts no reviewers", second["progress_lines"][0].startswith("check-spec: 2 specs, 0 reviewers") and modes(second) == [])
        report = run_script("collect-findings.py", "--work-dir", str(runs / "second"))
        case(
            "and still reports the violation it kept, with the commit it was found at",
            report.returncode == 1 and THE_VIOLATION in report.stdout and f"recorded at {head}" in report.stdout
            and "3 clauses, 0 reviewed, 3 carried from the record" in report.stdout,
        )
        drafted = run_script("draft-issues.py", "--work-dir", str(runs / "second"), "--json")
        case("but drafts no issue for it again", json.loads(drafted.stdout)["drafts"] == [])

        case("a full run reviews every clause the record holds", modes(prepared(runs / "all", "--full")) == [(c, "full") for c in ALL_THREE])

        lib.write_text(lib.read_text(encoding="utf-8") + "\nfn added() {}\n", encoding="utf-8")
        reading = prepared(runs / "reading")
        case(
            "a change to a governed file is read as a diff by the spec that governs it",
            modes(reading) == [("DEMO-1", "diff"), ("DEMO-2", "diff")],
        )
        case(
            "a threshold given on the command line decides it",
            modes(prepared(runs / "strict", "--threshold", "0")) == [("DEMO-1", "full"), ("DEMO-2", "full")],
        )
        answer(reading, **{"DEMO-1": {"holds": True}, "DEMO-2": {"holds": False, "verdict": "conforms"}})
        before = ledger.read_bytes()
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "reading"))
        case(
            "a tree that is not main is not recorded",
            done.returncode == 2 and "nothing recorded" in done.stderr and ledger.read_bytes() == before,
        )

        # The run was prepared on a tree with an uncommitted change, which is gone by the time it is
        # recorded: the tree is main again, and what the reviewers read was not.
        changed = lib.read_bytes()
        git(root, "checkout", "--", "crates/demo/src/lib.rs")
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "reading"))
        case(
            "a run prepared on a tree that was not a commit is not recorded, whatever the tree is now",
            done.returncode == 2 and "was not a commit" in done.stderr and ledger.read_bytes() == before,
        )
        lib.write_bytes(changed)

        git(root, "add", "-A")
        git(root, "commit", "-q", "-m", "a change to a governed file")
        # A clean tree that is a commit, and not main: a branch a commit ahead of it.
        ahead = prepared(runs / "ahead")
        answer(ahead, **{"DEMO-1": {"holds": True}, "DEMO-2": {"holds": False, "verdict": "conforms"}})
        before = ledger.read_bytes()
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "ahead"))
        case(
            "a tree that is a commit but not main is not recorded either",
            ahead["dirty"] == [] and done.returncode == 2 and "differs from" in done.stderr
            and ledger.read_bytes() == before,
        )
        git(root, "update-ref", "refs/remotes/upstream/main", "HEAD")
        moved = git(root, "rev-parse", "HEAD")[:12]
        reading = prepared(runs / "moved")
        answer(reading, **{"DEMO-1": {"holds": True}, "DEMO-2": {"holds": False, "verdict": "conforms"}})
        report = run_script("collect-findings.py", "--work-dir", str(runs / "moved"))
        case(
            "a violation the diff fixed is gone from the report",
            report.returncode == 0 and THE_VIOLATION not in report.stdout and "carried from the record" in report.stdout,
        )
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "moved"))
        now = ledger_of()
        case(
            "a verdict that held moves to the commit it was reviewed at again, and one that changed is replaced",
            done.returncode == 0
            and (now["DEMO-1"]["verdict"], now["DEMO-1"]["commit"]) == ("conforms", moved)
            and (now["DEMO-2"]["verdict"], now["DEMO-2"]["severity"], now["DEMO-2"]["commit"])
            == ("conforms", "-", moved)
            and now["DEMO-2"]["summary"] == "",
        )
        case(
            "a clause nothing was reviewed for keeps the line it had",
            (now.get("OTHER-1", {}).get("verdict"), now.get("OTHER-1", {}).get("commit")) == ("untested", head),
        )

        lib.write_text(lib.read_text(encoding="utf-8") + "\nfn added_again() {}\n", encoding="utf-8")
        git(root, "add", "-A")
        git(root, "commit", "-q", "-m", "another change")
        git(root, "update-ref", "refs/remotes/upstream/main", "HEAD")
        unanswered = prepared(runs / "unanswered")
        before = ledger.read_bytes()
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "unanswered"))
        case(
            "a reviewer that returned nothing leaves the lines of its clauses as they were",
            done.returncode == 0 and ledger.read_bytes() == before and modes(unanswered) == [("DEMO-1", "diff"), ("DEMO-2", "diff")],
        )
        answer(unanswered, **{"DEMO-1": {"holds": False, "verdict": "unclear", "summary": "could not tell"}, "DEMO-2": {"holds": True}})
        done = run_script("record-reviewed.py", "--work-dir", str(runs / "unanswered"))
        now = ledger_of()
        case(
            "a clause the reviewer could not decide loses its line, so it is reviewed again from the start",
            done.returncode == 0 and "DEMO-1" not in now and "DEMO-2" in now
            and modes(prepared(runs / "undecided")) == [("DEMO-1", "full")],
        )
        ledger.write_text("DEMO-2 conforms\n", encoding="utf-8")
        done = run_script("check-spec.py", "--mechanical-only", "--json")
        found = json.loads(done.stdout)["findings"]
        case(
            "a record that cannot be read is an error, and until it is fixed every clause is reviewed in full",
            done.returncode == 1
            and [(f["kind"], f["severity"]) for f in found] == [("reviewed-malformed", "error")]
            and modes(prepared(runs / "unreadable")) == [(c, "full") for c in ALL_THREE],
        )
    return checks


def collect_checks():
    """Which answers a reviewer of a change may give, and what each one keeps."""
    earlier = {
        "DEMO-1": {"verdict": "conforms", "severity": "-", "summary": "", "commit": "0123456789ab"},
        "DEMO-2": {"verdict": "violation", "severity": "error", "summary": THE_VIOLATION, "commit": "0123456789ab"},
    }
    with tempfile.TemporaryDirectory(prefix="check-spec-selftest-collect-") as tmp:
        results = Path(tmp) / "results.json"

        def read(*clauses, kept=()):
            results.write_text(json.dumps({"clauses": list(clauses)}), encoding="utf-8")
            prompt = {
                "results_file": str(results),
                "clauses": ["DEMO-1", "DEMO-2"],
                "mode": "diff",
                "previous": earlier,
            }
            entry = {"path": "docs/specs/demo.md", "clauses": 2, "kept": list(kept), "reviewer_prompts": [prompt]}
            return draft.collect.load_reviews({"specs": [entry]})

        both_hold = read({"clause": "DEMO-1", "holds": True}, {"clause": "DEMO-2", "holds": True})
        liar = read(
            {"clause": "DEMO-1", "holds": True, "verdict": "violation", "severity": "error", "summary": "the gate opens twice"},
            {"clause": "DEMO-2", "holds": True, "verdict": "conforms"},
        )
        silent = read({"clause": "DEMO-1"}, {"clause": "DEMO-2", "holds": False})
        fixed = read({"clause": "DEMO-1", "holds": True}, {"clause": "DEMO-2", "holds": False, "verdict": "conforms"})
        kept = read(
            kept=[
                dict(earlier["DEMO-2"], clause="DEMO-2"),
                dict(earlier["DEMO-1"], clause="DEMO-1"),
            ]
        )
    return [
        (
            "a verdict that holds is carried into the report with the commit it was reached at",
            [(f["clause"], f["kind"], f.get("carried"), f.get("recorded")) for f in both_hold[0]]
            == [("DEMO-2", "violation", True, "0123456789ab")]
            and both_hold[1] == 2,
        ),
        ("and is recorded as held", both_hold[2]["DEMO-2"]["held"] and both_hold[2]["DEMO-1"]["held"]),
        (
            "holding does not stand for another verdict: a reviewer that names one has found it",
            [(f["clause"], f["kind"], f.get("carried")) for f in liar[0]] == [("DEMO-1", "violation", None)]
            and not liar[2]["DEMO-1"]["held"]
            and liar[2]["DEMO-2"]["verdict"] == "conforms"
            and not liar[2]["DEMO-2"]["held"],
        ),
        (
            "an answer that neither says the verdict holds nor gives another decides nothing",
            sorted((f["clause"], f["kind"]) for f in silent[0]) == [("DEMO-1", "unclear"), ("DEMO-2", "unclear")]
            and silent[2]["DEMO-1"]["verdict"] == "unclear",
        ),
        (
            "a violation the change fixed leaves the report",
            fixed[0] == [] and fixed[2]["DEMO-2"]["verdict"] == "conforms" and fixed[2]["DEMO-1"]["held"],
        ),
        (
            "a clause kept without a review is in the report when it is a finding, and not when it conforms",
            [(f["clause"], f["carried"]) for f in kept[0] if f["kind"] != "review-incomplete"] == [("DEMO-2", True)],
        ),
    ]


def ledger_checks():
    """The file itself: what it holds, what it refuses, and the order it is written in."""
    entry = {"verdict": "conforms", "severity": "-", "commit": "0123456789ab", "hash": "0123456789abcdef", "date": "2026-10-20", "summary": ""}
    line = "DEMO-1 conforms - 0123456789ab 0123456789abcdef 2026-10-20"
    with tempfile.TemporaryDirectory(prefix="check-spec-selftest-ledger-") as tmp:
        path = Path(tmp) / "ledger"

        def refused(text):
            path.write_text(text, encoding="utf-8")
            try:
                reviewed.read_ledger(path)
            except reviewed.Problem as problem:
                return str(problem)
            return None

        reviewed.write_ledger(
            path,
            {
                "DEMO-10": dict(entry, verdict="violation", severity="error", summary="a cost — and a long one " + "x " * 200),
                "DEMO-2": entry,
                "AAA-3": entry,
            },
        )
        written = path.read_text(encoding="utf-8").splitlines()
        read_back = reviewed.read_ledger(path)
        listing = sorted(p.name for p in Path(tmp).iterdir())

        # A write that dies partway, as a full disk does: the file that was there is still there.
        earlier = path.read_bytes()
        replace = reviewed.os.replace

        def no_space(*_):
            raise OSError("no space left on device")

        reviewed.os.replace = no_space
        try:
            reviewed.write_ledger(path, {"DEMO-1": entry})
            died = False
        except OSError:
            died = True
        finally:
            reviewed.os.replace = replace
        survived = died and path.read_bytes() == earlier
        (Path(tmp) / "ledger.new").unlink(missing_ok=True)

        def refused_bytes(where):
            where.write_bytes(b"DEMO-1 conforms - \xff\xfe\n")
            try:
                reviewed.read_ledger(where)
            except reviewed.Problem as problem:
                return "cannot be read" in str(problem)
            except Exception:  # the run ends where the record should have been refused
                return False
            return False
        return [
            ("clauses are written in order of their numbers, not of their digits", [l.split()[0] for l in written if not l.startswith("#")] == ["AAA-3", "DEMO-2", "DEMO-10"]),
            ("a summary is one short line with no em-dash", "—" not in written[-1] and len(read_back["DEMO-10"]["summary"]) <= reviewed.SUMMARY_LIMIT and read_back["DEMO-10"]["summary"].startswith("a cost, and a long one")),
            ("a record is read back as it was written", read_back["DEMO-2"] == entry),
            ("the file is replaced whole, leaving nothing beside it", listing == ["ledger"]),
            ("a write that dies partway leaves the file that was there", survived),
            ("a missing file is no record", reviewed.read_ledger(Path(tmp) / "absent") == {}),
            ("a file that is not text is refused rather than crashing the run", refused_bytes(path)),
            ("a well formed line is read", refused(line) is None),
            ("a line missing its fields is refused with its number", "ledger:1" in (refused("DEMO-1 conforms\n") or "")),
            ("a verdict the record does not keep is refused", refused(line.replace("conforms", "unclear")) is not None),
            ("a conforming clause with a severity is refused", refused(line.replace(" - ", " error ")) is not None),
            ("a violation with no severity is refused", refused(line.replace("conforms", "violation")) is not None),
            ("a clause recorded twice is refused", refused(f"{line}\n{line}\n") is not None),
            ("a hash that is not one is refused", refused(line.replace("0123456789abcdef", "xyz")) is not None),
            ("a date that is not one is refused", refused(line.replace("2026-10-20", "yesterday")) is not None),
            ("a commit that is not one is refused", refused(line.replace("0123456789ab ", "main ")) is not None),
        ]



THE_RECORD = "DEMO-1 conforms - 0123456789ab 0123456789abcdef 2026-10-20\n"


def write_the_record(text):
    def write(root):
        (root / reviewed.LEDGER).write_text(text, encoding="utf-8")

    return write


CASES += [
    ("a record every line of which is well formed", write_the_record(THE_RECORD), None),
    ("a record line with fields missing", write_the_record("DEMO-1 conforms\n"), "reviewed-malformed"),
    (
        "a record line with a verdict the record does not keep",
        write_the_record(THE_RECORD.replace("conforms", "unclear")),
        "reviewed-malformed",
    ),
]


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
    check._TOPLEVEL.clear()  # A different checkout from the last case's.
    check._BASE.clear()
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

    for name, break_it, expected in COUNTER_ALONE:
        kinds = in_fixture(break_it, numbering_alone)
        note(name, kinds == expected, f"reported {kinds}, wanted {expected}")

    for name, break_it, event in COMMITTED:
        kinds = in_fixture(break_it, lambda: under_event(event))
        note(name, kinds == ["clause-renumbered"], f"reported {kinds}")

    for name, break_it, expected in UNVERIFIED_CASES:
        lines = in_fixture(break_it, generated_list)
        note(name, lines == expected, f"generated {lines}, wanted {expected}")

    for name, selectors, changed_base, refused in SELECTIONS:
        answer = check.partial_selection(selectors, changed_base)
        note(name, answer == refused, f"answered {answer}, wanted {refused}")

    drafted = in_fixture(None, draft_checks)
    for name, held in drafted:
        note(name, held, "the body does not say it")

    manifests = manifest_checks()
    for name, held in manifests:
        note(name, held, "the manifest was not read as the pipeline expects")

    sited = in_fixture(None, site_checks)
    for name, held in sited:
        note(name, held, "the drafter chose a different place")

    for name, held in bullet_checks():
        note(name, held, "the bullet was written differently")

    selected = in_fixture(nest_a_spec, selection_checks)
    for name, held in selected:
        note(name, held, "the selector resolved to something else")

    # What a repeat run reviews. The first three need no repository; the rest run in the fixture.
    repeated = (
        [(c, "the threshold decided otherwise") for c in decide_checks()]
        + [(c, "the file read or wrote something else") for c in ledger_checks()]
        + [(c, "the collector read the answer differently") for c in collect_checks()]
        + [(c, "a hash moved, or did not") for c in in_fixture(build_incremental, hash_checks)]
        + [(c, "the plan was not the one the record calls for") for c in in_fixture(build_incremental, incremental_checks)]
        + [(c, "the scripts did not do what a run needs") for c in in_fixture(build_incremental, record_checks)]
    )
    for (name, held), why in repeated:
        note(name, held, why)

    total = (
        len(CASES)
        + len(COUNTER_ALONE)
        + len(COMMITTED)
        + len(UNVERIFIED_CASES)
        + len(SELECTIONS)
        + len(drafted)
        + len(manifests)
        + len(sited)
        + len(selected)
        + len(repeated)
    )
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
