#!/usr/bin/env python3
"""Reports a change that weakens what `make check-verus` holds the code to, for a person to read.

`verify/contracts.txt` records every Verus contract and every assumption Verus is told to accept,
`verify/mutations.txt` the defects Verus must refuse, and `verify/survivors.txt` the one-token
changes Verus may accept. A change can weaken all three and still pass every check: a contract
loosened and its ledger line rewritten to match, a mutation deleted with the contract that refused
it, a survivor listed with a reason that sounds right. Each shows in the diff, and nothing points a
reviewer at it. This does.

It compares the three files at the merge base with the checkout and reports, line by line:

- a contract removed, where a function had one and has none, and a contract changed, shown as it
  was and as it is, since whether the new one says more or less is for a person to read;
- an assumption added, which is something Verus now accepts without proof;
- a mutation removed, which is a defect the contracts no longer have to refuse;
- a survivor added, which is a change Verus may now accept;
- any change to `crates/core/src/spec.rs` or to the scripts that run the checks, which can change
  what every contract means without changing a line of the ledger. This weakens nothing by itself,
  so it is reported apart, as `tooling`, and the rest as `weakened`.

A contract or a mutation added, and an assumption or a survivor removed, is a strengthening and is
not reported. With `--summary` the report is written as Markdown for a job summary, and with
`--github-output` the job is told whether anything was found, which the `Proof weakened` workflow
reads to label the pull request and ask bbondy and darkdh to review it. That workflow runs the
default branch's copy of this script, with `--head` on the pull request's commit, so a pull request
cannot change what is reported about it. It exits 0 either way: whether a weakening is right is a person's call.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
from collections import Counter
import os
from pathlib import Path
import re
# Fixed git commands with argument lists, never a shell.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
CONTRACTS = "verify/contracts.txt"
MUTATIONS = "verify/mutations.txt"
SURVIVORS = "verify/survivors.txt"
# A change here changes what a contract means, or what the checks hold, without a ledger line.
WATCHED = (
    "crates/core/src/spec.rs",
    "contrib/check-verus.py",
    "contrib/check-verus-contracts.py",
    "contrib/check-verus-sweep.py",
    "contrib/check-verus-weakened.py",
    "verify/history.rs",
)
ASSUMES = re.compile(r"^\S+: assumes ")
# The kind a watched file's edit is reported as. It weakens nothing by itself, and changes what the
# checks mean, so it is reported apart from the weakenings for an owner to read.
TOOLING = "changes what the checks mean"


def weakened(found):
    """The items of `found` that weaken a contract, a mutation or a survivor."""
    return [(kind, text) for kind, text in found if kind != TOOLING]


def tooling(found):
    """The watched files `found` names, each of which changes what the checks mean."""
    return [text for kind, text in found if kind == TOOLING]


def git(root, *args):
    return subprocess.run(  # nosemgrep: dangerous-subprocess-use-audit
        ["git", *args], cwd=root, capture_output=True, text=True, check=True
    ).stdout


def at(root, commit, path):
    """The text of `path` at `commit`, or the empty string where it did not exist."""
    run = subprocess.run(  # nosemgrep: dangerous-subprocess-use-audit
        ["git", "show", f"{commit}:{path}"], cwd=root, capture_output=True, text=True,
        check=False,
    )
    return run.stdout if run.returncode == 0 else ""


def ledger(text):
    """The ledger's lines, without its comments."""
    return [line for line in text.splitlines() if line.strip() and not line.startswith("#")]


def entries(text, opening):
    """Each entry of a mutations or survivors file as one string: the lines from one starting
    with `opening` up to the next, joined, without comments or blank lines."""
    found, current = [], []
    for line in text.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        if line.startswith(opening) and current:
            found.append("\n".join(current))
            current = []
        current.append(line.rstrip())
    if current:
        found.append("\n".join(current))
    return found


def weakenings(before, after, watched_changed):
    """What the change from `before` to `after`, each a dict of the three files' texts, weakens,
    as `(kind, text)` pairs in a stable order."""
    found = []
    old_ledger, new_ledger = ledger(before[CONTRACTS]), ledger(after[CONTRACTS])
    # Counted, not collected into sets: the ledger names an assumption by its file and item alone,
    # so two `impl Clone` in one file are the same line twice, and a third is one more of them.
    gone = Counter(old_ledger) - Counter(new_ledger)
    came = Counter(new_ledger) - Counter(old_ledger)
    # A function's contracts the change added, by name, so a line that changed is told apart from
    # a contract that went. Whether a changed one says more or less is a person's to read.
    now = {}
    for line in came.elements():
        if not ASSUMES.match(line):
            now.setdefault(line.split(": ", 1)[0], []).append(line)
    for line in gone.elements():
        if ASSUMES.match(line):
            continue
        replaced = now.get(line.split(": ", 1)[0])
        if replaced:
            found.append(("contract changed", f"was: {line}\nnow: {replaced.pop(0)}"))
        else:
            found.append(("contract removed", line))
    for line in came.elements():
        if ASSUMES.match(line):
            found.append(("assumption added", line))
    removed = Counter(entries(before[MUTATIONS], "mutation:")) - Counter(
        entries(after[MUTATIONS], "mutation:")
    )
    found.extend(("mutation removed", entry) for entry in removed.elements())
    added = Counter(entries(after[SURVIVORS], "survivor:")) - Counter(
        entries(before[SURVIVORS], "survivor:")
    )
    found.extend(("survivor added", entry) for entry in added.elements())
    for path in watched_changed:
        found.append((TOOLING, path))
    return found


def render(found, markdown):
    """The report a person reads: the weakenings, then the watched files the change edits."""
    if not found:
        return "check-verus-weakened: nothing the contracts hold is weakened\n"
    if not markdown:
        return "".join(
            f"check-verus-weakened: {kind}: {text.splitlines()[0]}\n" for kind, text in found
        )
    lines = []
    if weakened(found):
        lines += [
            "## This change weakens what Verus checks",
            "",
            "Each item below is a contract, a mutation or a survivor that holds the code to less"
            " than the base did. The pull request has to say why each is right. See"
            " `docs/best-practices/verification.md`.",
            "",
        ]
        for kind, text in weakened(found):
            lines.append(f"- **{kind}**")
            lines.append("")
            lines.append("  ```")
            lines.extend(f"  {line}" for line in text.splitlines())
            lines.append("  ```")
        lines.append("")
    else:
        lines += ["## This change weakens no contract, mutation or survivor", ""]
    if tooling(found):
        lines += [
            "## Files that change what the checks mean",
            "",
            "These hold the definitions every contract names, or run the checks. An edit to one"
            " weakens nothing by itself, and can change what every contract means, so a person"
            " reads it.",
            "",
        ]
        lines.extend(f"- `{path}`" for path in tooling(found))
    return "\n".join(lines).rstrip("\n") + "\n"


def check(root, base, head=None):
    """What the change from `base` to `head` weakens, or to the checkout where `head` is `None`."""
    files = (CONTRACTS, MUTATIONS, SURVIVORS)
    before = {path: at(root, base, path) for path in files}
    if head is None:
        after = {
            path: (root / path).read_text(encoding="utf-8") if (root / path).exists() else ""
            for path in files
        }
        changed = git(root, "diff", "--name-only", base, "--", *WATCHED)
    else:
        after = {path: at(root, head, path) for path in files}
        changed = git(root, "diff", "--name-only", base, head, "--", *WATCHED)
    changed = set(changed.split())
    return weakenings(before, after, [path for path in WATCHED if path in changed])


class Selftest(unittest.TestCase):
    def files(self, contracts="", mutations="", survivors=""):
        return {CONTRACTS: contracts, MUTATIONS: mutations, SURVIVORS: survivors}

    def test_a_contract_loosened_and_its_ledger_line_rewritten_is_reported_as_it_was_and_is(self):
        before = self.files("# header\na.rs::f: r => ensures r > 0,\n")
        after = self.files("# header\na.rs::f: r => ensures true,\n")
        self.assertEqual(
            weakenings(before, after, []),
            [(
                "contract changed",
                "was: a.rs::f: r => ensures r > 0,\nnow: a.rs::f: r => ensures true,",
            )],
        )

    def test_a_function_that_loses_its_contract_is_reported_as_removed(self):
        before = self.files("a.rs::f: r => ensures r > 0,\na.rs::g: r => ensures r,\n")
        after = self.files("a.rs::g: r => ensures r,\n")
        self.assertEqual(
            weakenings(before, after, []),
            [("contract removed", "a.rs::f: r => ensures r > 0,")],
        )

    def test_a_contract_added_is_not_reported(self):
        before = self.files("a.rs::f: r => ensures r > 0,\n")
        after = self.files("a.rs::f: r => ensures r > 0,\na.rs::g: r => ensures r,\n")
        self.assertEqual(weakenings(before, after, []), [])

    def test_an_assumption_added_is_reported_and_one_removed_is_not(self):
        before = self.files("a.rs::f: assumes external_body\n")
        after = self.files("a.rs::g: assumes requires\n")
        self.assertEqual(
            weakenings(before, after, []),
            [("assumption added", "a.rs::g: assumes requires")],
        )

    def test_a_second_copy_of_an_assumption_line_already_there_is_reported(self):
        line = "a.rs::clone: assumes external\n"
        self.assertEqual(
            weakenings(self.files(line), self.files(line * 2), []),
            [("assumption added", line.strip())],
        )
        self.assertEqual(weakenings(self.files(line * 2), self.files(line), []), [])

    def test_a_second_copy_of_a_mutation_removed_is_reported(self):
        entry = "mutation: f lets x through\nfile: a.rs\nreplace: x => y\n"
        self.assertEqual(
            weakenings(
                self.files(mutations=entry + "\n" + entry), self.files(mutations=entry), []
            ),
            [("mutation removed", entry.strip())],
        )

    def test_a_mutation_removed_is_reported_and_one_added_is_not(self):
        one = "mutation: f lets x through\nfile: a.rs\nreplace: x => y\n"
        two = "mutation: g lets z through\nfile: a.rs\nreplace: z => w\n"
        self.assertEqual(
            weakenings(self.files(mutations=one + "\n" + two), self.files(mutations=two), []),
            [("mutation removed", one.strip())],
        )
        self.assertEqual(
            weakenings(self.files(mutations=two), self.files(mutations=one + "\n" + two), []),
            [],
        )

    def test_a_mutation_whose_replacement_was_rewritten_is_reported(self):
        before = self.files(mutations="mutation: f\nfile: a.rs\nreplace: x => y\n")
        after = self.files(mutations="mutation: f\nfile: a.rs\nreplace: x => x\n")
        self.assertEqual([kind for kind, _ in weakenings(before, after, [])], ["mutation removed"])

    def test_a_survivor_added_is_reported_and_one_removed_is_not(self):
        entry = "survivor: asks more\nin: a.rs::f\nwas: a\nnow: b\n"
        self.assertEqual(
            weakenings(self.files(), self.files(survivors="# c\n\n" + entry), []),
            [("survivor added", entry.strip())],
        )
        self.assertEqual(weakenings(self.files(survivors=entry), self.files(), []), [])

    def test_a_change_to_a_watched_file_is_reported_by_name(self):
        self.assertEqual(
            weakenings(self.files(), self.files(), ["crates/core/src/spec.rs"]),
            [(TOOLING, "crates/core/src/spec.rs")],
        )

    def test_a_watched_file_alone_is_tooling_and_not_a_weakening(self):
        found = weakenings(self.files(), self.files(), ["contrib/check-verus.py"])
        self.assertEqual(weakened(found), [])
        self.assertEqual(tooling(found), ["contrib/check-verus.py"])
        report = render(found, markdown=True)
        self.assertIn("weakens no contract, mutation or survivor", report)
        self.assertIn("`contrib/check-verus.py`", report)
        self.assertNotIn("This change weakens what Verus checks", report)

    def test_a_weakening_beside_a_watched_file_reports_both_apart(self):
        before = self.files("a.rs::f: r => ensures r > 0,\n")
        found = weakenings(before, self.files(), ["crates/core/src/spec.rs"])
        self.assertEqual([kind for kind, _ in weakened(found)], ["contract removed"])
        self.assertEqual(tooling(found), ["crates/core/src/spec.rs"])
        report = render(found, markdown=True)
        self.assertLess(
            report.index("This change weakens what Verus checks"),
            report.index("Files that change what the checks mean"),
        )

    def test_comments_and_blank_lines_do_not_count(self):
        before = self.files("# old header\n\na.rs::f: r => ensures r,\n")
        after = self.files("# new header\na.rs::f: r => ensures r,\n\n")
        self.assertEqual(weakenings(before, after, []), [])

    def test_the_markdown_report_names_every_item_and_a_clean_change_says_so(self):
        found = [("assumption added", "a.rs::g: assumes requires")]
        report = render(found, markdown=True)
        self.assertIn("**assumption added**", report)
        self.assertIn("a.rs::g: assumes requires", report)
        self.assertIn("nothing the contracts hold is weakened", render([], markdown=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true", help="check the classifier")
    parser.add_argument("--base", help="the commit to compare with; BASE_SHA by default")
    parser.add_argument("--head", help="the commit to read; the checkout by default")
    parser.add_argument("--summary", help="append the report as Markdown to this file")
    parser.add_argument("--report", help="write the report as Markdown to this file")
    parser.add_argument(
        "--github-output",
        help="write weakened= and tooling=, each true or false, to this file",
    )
    args = parser.parse_args()
    if args.selftest:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(Selftest)
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=0).run(suite)
        return 0 if result.wasSuccessful() else 1
    base = args.base or os.environ.get("BASE_SHA")
    if not base:
        base = git(ROOT, "merge-base", "HEAD", "origin/main").strip()
    base = git(ROOT, "rev-parse", "--verify", f"{base}^{{commit}}").strip()
    head = args.head and git(ROOT, "rev-parse", "--verify", f"{args.head}^{{commit}}").strip()
    found = check(ROOT, base, head or None)
    sys.stdout.write(render(found, markdown=False))
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as summary:
            summary.write(render(found, markdown=True))
    if args.report:
        Path(args.report).write_text(render(found, markdown=True), encoding="utf-8")
    if args.github_output:
        with open(args.github_output, "a", encoding="utf-8") as output:
            output.write(f"weakened={'true' if weakened(found) else 'false'}\n")
            output.write(f"tooling={'true' if tooling(found) else 'false'}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
