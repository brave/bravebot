#!/usr/bin/env python3
"""Fails a change that adds hits of a clippy lint the tree has not been made to satisfy.

`make check` runs clippy with the lints that hold at zero. The lints here report hundreds of sites
in the existing code, so enabling them would need every site fixed or marked in one change. This
holds the number where it is: clippy runs over the libraries and binaries at the merge base and at
the checkout, and a lint that reports more sites in a crate now than it did then fails the check.
Sites a change removes are not credited to one it adds in another crate, and nothing is recorded in
the tree, so there is no list to keep in step with the code.

Both runs use the same toolchain on the same host, so a release of clippy that finds more or fewer
sites, and code that only one platform compiles, change both counts alike. A site that is right is
marked where it is with `#[expect(clippy::lint, reason = "...")]`, which clippy does not report.

Standard library only, like the other checks CI runs without a toolchain, though this one runs
clippy and so needs one.
"""

import argparse
import json
import os
# Fixed git and cargo commands with argument lists, never a shell.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# The lints the tree has sites of, and which it is held to no more of. A lint that reports none
# belongs in `[workspace.lints.clippy]` instead, which refuses the first one. These are the
# restriction and pedantic lints whose sites are a judgement at each place rather than a fault,
# and none is in the `nursery` group, which reports differently from one release to the next.
# `allow_attributes` has few sites and is here anyway: what is left is an `#[allow]` that must stay
# one because `unsafe_code` and `dead_code` hold on one platform and not another, where an
# `#[expect]` the other platform never fulfils fails its build.
LINTS = (
    "let_underscore_must_use",
    "expect_used",
    "panic",
    "string_slice",
    "cast_possible_truncation",
    "cast_sign_loss",
    "too_many_lines",
    "needless_pass_by_value",
    "wildcard_imports",
    "items_after_statements",
    "manual_let_else",
    "implicit_clone",
    "unused_self",
    "struct_excessive_bools",
    "fn_params_excessive_bools",
    "undocumented_unsafe_blocks",
    "multiple_unsafe_ops_per_block",
    "allow_attributes",
)

# Only code under these is counted: what ships, from the crates, and not a build script's output
# or a dependency's source.
COUNTED = "crates/"
# A change that touches none of these cannot change what clippy reports.
RUST_PATHS = ("crates", "Cargo.toml", "Cargo.lock", "clippy.toml")


def git(root, *args):
    return subprocess.run(  # nosemgrep: gitlab.bandit.B603
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=True
    ).stdout


def base_commit(root, base, env):
    """The commit to compare with: the one named, the pull request's base, or where the branch
    left `upstream/main` or `origin/main`."""
    if base or env.get("BASE_SHA"):
        return git(root, "rev-parse", "--verify", f"{base or env['BASE_SHA']}^{{commit}}").strip()
    for ref in ("upstream/main", "origin/main"):
        try:
            return git(root, "merge-base", ref, "HEAD").strip()
        except subprocess.CalledProcessError:
            continue
    raise SystemExit("check-lint-increase: no upstream/main or origin/main to measure from; pass --base")


def clippy_command():
    return [
        "cargo", "clippy", "--workspace", "--lib", "--bins", "--all-features", "--quiet",
        "--message-format=json", "--", *(f"-Wclippy::{lint}" for lint in LINTS),
    ]


def run_clippy(directory, target_dir):
    """clippy's JSON lines for the checkout at `directory`, built into `target_dir` so a second
    checkout reuses the dependencies the first compiled."""
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir))
    done = subprocess.run(  # nosemgrep: gitlab.bandit.B603
        clippy_command(), cwd=directory, env=env, capture_output=True, text=True, check=False
    )
    if done.returncode != 0 and '"reason":"compiler-message"' not in done.stdout:
        raise SystemExit(f"check-lint-increase: clippy failed in {directory}:\n{done.stderr[-2000:]}")
    if done.returncode != 0 and '"level":"error"' in done.stdout:
        raise SystemExit(f"check-lint-increase: {directory} does not compile:\n{done.stderr[-2000:]}")
    return done.stdout


def sites(output):
    """(lint, path, line, column) for each site of a lint in LINTS, once each, from clippy's JSON."""
    found = set()
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message.get("message") or {}
        lint = ((diagnostic.get("code") or {}).get("code") or "").removeprefix("clippy::")
        spans = [s for s in diagnostic.get("spans", []) if s.get("is_primary")]
        if lint not in LINTS or not spans:
            continue
        path = spans[0]["file_name"].removeprefix("./")
        if path.startswith(COUNTED):
            found.add((lint, path, spans[0]["line_start"], spans[0]["column_start"]))
    return found


def crate_of(path):
    return path.split("/")[1]


def counts(found):
    """How many sites each lint has in each crate, and in each file."""
    per_crate = Counter((lint, crate_of(path)) for lint, path, _, _ in found)
    per_file = Counter((lint, path) for lint, path, _, _ in found)
    return per_crate, per_file


def increases(before, after):
    """(lint, crate, sites before, sites after, where) for each lint with more sites in a crate
    than there were. `where` is the sites in the files where its count rose, since a site in a
    file whose count did not is one that was already there."""
    crate_before, file_before = counts(before)
    crate_after, file_after = counts(after)
    grown = []
    for (lint, crate), now in sorted(crate_after.items()):
        was = crate_before.get((lint, crate), 0)
        if now <= was:
            continue
        where = sorted(
            f"{path}:{line}"
            for found_lint, path, line, _ in after
            if found_lint == lint
            and crate_of(path) == crate
            and file_after[(lint, path)] > file_before.get((lint, path), 0)
        )
        grown.append((lint, crate, was, now, where))
    return grown


def render(grown):
    lines = []
    for lint, crate, was, now, where in grown:
        lines.append(f"clippy::{lint} reports {now} sites in {crate}, and {was} at the base:")
        lines.extend(f"  {site}" for site in where[:12])
        if len(where) > 12:
            lines.append(f"  and {len(where) - 12} more")
    lines.append("")
    lines.append(
        "Fix each site, or where the code is right mark it with "
        '`#[expect(clippy::<lint>, reason = "...")]`, which clippy does not report.'
    )
    if any(lint == "allow_attributes" for lint, *_ in grown):
        lines.append(
            'Write `#[expect(<lint>, reason = "...")]` in place of an `#[allow]`; where a platform '
            "the lint does not fire on builds the code, keep the `#[allow(<lint>, reason = \"...\")]` "
            'and mark it `#[expect(clippy::allow_attributes, reason = "...")]`.'
        )
    return "\n".join(lines)


def rust_changed(root, base):
    """Whether anything that clippy reads differs between `base` and the checkout, uncommitted
    edits included."""
    return bool(git(root, "diff", "--name-only", base, "--", *RUST_PATHS).strip())


def check(root, base, run=run_clippy):
    """The increases of a lint's sites over `base`, as a list. `run(directory, target_dir)` is
    clippy's JSON for a checkout of the tree."""
    target = Path(os.environ.get("CARGO_TARGET_DIR") or root / "target")
    if not rust_changed(root, base):
        return []
    after = sites(run(root, target))
    with tempfile.TemporaryDirectory(prefix="lint-increase-") as temp:
        directory = Path(temp) / "base"
        git(root, "worktree", "add", "--detach", str(directory), base)
        try:
            before = sites(run(directory, target))
        finally:
            git(root, "worktree", "remove", "--force", str(directory))
    return increases(before, after)


class Verdicts(unittest.TestCase):
    def hit(self, lint, path, line=1, column=1):
        return (lint, path, line, column)

    def test_a_lint_reporting_more_sites_in_a_crate_than_before_is_an_increase(self):
        before = {self.hit("panic", "crates/a/src/lib.rs", 3)}
        after = before | {self.hit("panic", "crates/a/src/lib.rs", 9)}
        (grown,) = increases(before, after)
        self.assertEqual(grown[:4], ("panic", "a", 1, 2))
        self.assertEqual(grown[4], ["crates/a/src/lib.rs:3", "crates/a/src/lib.rs:9"])

    def test_the_same_number_of_sites_moved_within_a_crate_is_not(self):
        before = {self.hit("string_slice", "crates/a/src/old.rs", 3)}
        after = {self.hit("string_slice", "crates/a/src/new.rs", 40)}
        self.assertEqual(increases(before, after), [])

    def test_fewer_sites_is_not_an_increase_and_does_not_pay_for_another_crates(self):
        before = {self.hit("panic", "crates/a/src/lib.rs", n) for n in (1, 2, 3)}
        after = {self.hit("panic", "crates/b/src/lib.rs", 1)}
        (grown,) = increases(before, after)
        self.assertEqual(grown[:4], ("panic", "b", 0, 1))

    def test_a_lint_counts_apart_from_another_in_the_same_crate(self):
        before = {self.hit("panic", "crates/a/src/lib.rs", 1)}
        after = {self.hit("expect_used", "crates/a/src/lib.rs", 1)}
        (grown,) = increases(before, after)
        self.assertEqual(grown[0], "expect_used")

    def test_sites_in_files_whose_count_did_not_rise_are_not_listed(self):
        before = {self.hit("panic", "crates/a/src/lib.rs", 3)}
        after = before | {self.hit("panic", "crates/a/src/other.rs", 5)}
        (grown,) = increases(before, after)
        self.assertEqual(grown[4], ["crates/a/src/other.rs:5"])


class Reading(unittest.TestCase):
    def line(self, lint, path, line=1, primary=True, reason="compiler-message"):
        return json.dumps(
            {
                "reason": reason,
                "message": {
                    "code": {"code": f"clippy::{lint}"},
                    "spans": [{"file_name": path, "line_start": line, "column_start": 1, "is_primary": primary}],
                },
            }
        )

    def test_only_the_lints_held_to_a_count_in_the_crates_are_read_and_each_site_once(self):
        output = "\n".join(
            [
                self.line("panic", "crates/a/src/lib.rs", 4),
                self.line("panic", "crates/a/src/lib.rs", 4),
                self.line("needless_return", "crates/a/src/lib.rs", 5),
                self.line("panic", "target/debug/build/x/out.rs", 1),
                self.line("panic", "crates/a/src/lib.rs", 6, primary=False),
                self.line("panic", "crates/a/src/lib.rs", 7, reason="compiler-artifact"),
                "not json",
            ]
        )
        self.assertEqual(sites(output), {("panic", "crates/a/src/lib.rs", 4, 1)})

    def test_the_report_names_the_sites_and_how_to_mark_one_that_is_right(self):
        text = render([("panic", "a", 1, 2, ["crates/a/src/lib.rs:9"])])
        self.assertIn("clippy::panic reports 2 sites in a, and 1 at the base:", text)
        self.assertIn("  crates/a/src/lib.rs:9", text)
        self.assertIn("#[expect(clippy::<lint>", text)

    def test_an_allow_attribute_is_counted_and_the_report_says_to_write_expect(self):
        output = self.line("allow_attributes", "crates/a/src/lib.rs", 8)
        self.assertEqual(sites(output), {("allow_attributes", "crates/a/src/lib.rs", 8, 1)})
        text = render([("allow_attributes", "a", 0, 1, ["crates/a/src/lib.rs:8"])])
        self.assertIn("#[expect(<lint>", text)
        self.assertNotIn("#[expect(<lint>", render([("panic", "a", 0, 1, ["crates/a/src/lib.rs:8"])]))

    def test_the_command_reads_the_code_that_ships_with_every_lint_it_counts(self):
        command = clippy_command()
        self.assertIn("--lib", command)
        self.assertNotIn("--all-targets", command)
        for lint in LINTS:
            self.assertIn(f"-Wclippy::{lint}", command)


class Comparing(unittest.TestCase):
    """The checkout and its base, with clippy replaced by what each would report."""

    def repository(self, temp):
        root = Path(temp) / "repo"
        (root / "crates/a/src").mkdir(parents=True)
        (root / "crates/a/src/lib.rs").write_text("pub fn a() {}\n")
        for args in (["init", "-q"], ["add", "-A"], ["-c", "user.name=t", "-c", "user.email=t@example.invalid",
                                                       "commit", "-q", "-m", "base"]):
            git(root, *args)
        return root, git(root, "rev-parse", "HEAD").strip()

    def reporting(self, base_sites, head_sites):
        def run(directory, target_dir):
            lines = base_sites if Path(directory).name == "base" else head_sites
            return "\n".join(
                json.dumps(
                    {
                        "reason": "compiler-message",
                        "message": {
                            "code": {"code": f"clippy::{lint}"},
                            "spans": [{"file_name": path, "line_start": line, "column_start": 1, "is_primary": True}],
                        },
                    }
                )
                for lint, path, line in lines
            )

        return run

    def test_a_change_that_adds_a_site_fails_and_one_that_adds_none_passes(self):
        site = ("panic", "crates/a/src/lib.rs", 1)
        with tempfile.TemporaryDirectory() as temp:
            root, base = self.repository(temp)
            (root / "crates/a/src/lib.rs").write_text("pub fn a() {}\npub fn b() {}\n")
            grown = check(root, base, self.reporting([site], [site, ("panic", "crates/a/src/lib.rs", 2)]))
            same = check(root, base, self.reporting([site], [site]))
        self.assertEqual([g[:4] for g in grown], [("panic", "a", 1, 2)])
        self.assertEqual(same, [])

    def test_a_change_that_touches_no_rust_runs_nothing(self):
        def run(directory, target_dir):
            raise AssertionError("clippy ran")

        with tempfile.TemporaryDirectory() as temp:
            root, base = self.repository(temp)
            (root / "README.md").write_text("text\n")
            self.assertEqual(check(root, base, run), [])

    def test_the_base_checkout_is_removed_afterwards(self):
        with tempfile.TemporaryDirectory() as temp:
            root, base = self.repository(temp)
            (root / "crates/a/src/lib.rs").write_text("pub fn a() {}\n// changed\n")
            check(root, base, self.reporting([], []))
            listed = git(root, "worktree", "list", "--porcelain")
        self.assertEqual(listed.count("worktree "), 1)

    def test_the_base_is_the_one_named_then_the_pull_requests_then_the_merge_base(self):
        with tempfile.TemporaryDirectory() as temp:
            root, base = self.repository(temp)
            self.assertEqual(base_commit(root, base, {}), base)
            self.assertEqual(base_commit(root, None, {"BASE_SHA": base}), base)
            with self.assertRaises(SystemExit):
                base_commit(root, None, {})


def main():
    parser = argparse.ArgumentParser(description="Fail a change that adds sites of a held clippy lint.")
    parser.add_argument("--base", help="compare with this ref; default $BASE_SHA, then the merge base with upstream/main")
    parser.add_argument("--root", type=Path, default=ROOT, help="the checkout to read")
    parser.add_argument("--selftest", action="store_true", help="check the verdict on known inputs")
    args = parser.parse_args()
    if args.selftest:
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=0).run(
            unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
        )
        return 0 if result.wasSuccessful() else 1
    base = base_commit(args.root, args.base, os.environ)
    grown = check(args.root, base)
    if grown:
        print(render(grown), file=sys.stderr)
        return 1
    print(f"check-lint-increase: no lint reports more sites than at {base[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
