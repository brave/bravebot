#!/usr/bin/env python3
"""Fails a change that narrates its writer's own process.

Commit messages, the pull request title and body, and every line a change adds are read by
somebody who never saw the conversation they came from, so a correction of an earlier claim or an
account of what was guessed tells them nothing about the code. What is true belongs there, in the
present tense, as though said for the first time.

This decides the part of that a tool can: a fixed list of first-person phrases. A paraphrase passes
it, and a reviewer is still the one who catches those. The list is kept short enough that a hit is a
sentence worth rewriting rather than a false alarm, and there is no way to allow one.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import os
import re
# Fixed git commands with argument lists, never a shell.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Matched as whole words, case and punctuation aside, across the line breaks a comment or a
# wrapped paragraph puts in the middle of one.
PHRASES = (
    ("i", "was", "wrong"),
    ("i", "was", "mistaken"),
    ("i", "was", "incorrect"),
    ("i", "stand", "corrected"),
    ("my", "mistake"),
    ("correcting", "my"),
    ("as", "i", "said"),
    ("as", "i", "mentioned"),
    ("as", "i", "noted"),
    ("this", "corrects", "what"),
    ("my", "earlier"),
    ("my", "previous"),
    ("i", "initially"),
    ("i", "originally", "thought"),
    ("i", "previously"),
    ("i", "assumed"),
    ("i", "guessed"),
    ("i", "misread"),
    ("i", "checked"),
    ("i", "verified"),
)

# This file lists the phrases above, so it would report itself. Lockfiles are generated.
OWN_FILE = "contrib/check-narration.py"
LOCKFILES = ("Cargo.lock", "package-lock.json")
PATHSPECS = (f":(exclude){OWN_FILE}", *(f":(exclude,glob)**/{name}" for name in LOCKFILES))

WORD = re.compile(r"[a-z0-9']+")
HUNK = re.compile(r"^@@ -\S+ \+(\d+)")


def git(root, *args):
    return subprocess.run(
        ["git", "-c", "core.quotepath=off", *args], cwd=root, check=True, capture_output=True,
        text=True, encoding="utf-8", errors="replace",
    ).stdout


def hits(lines):
    """The phrases in `lines`, a list of (line number, text), with the line each starts on."""
    tokens = []
    for number, text in lines:
        for word in WORD.findall(text.lower().replace("’", "'")):
            tokens.append((word, number))
    found = []
    for index, (word, number) in enumerate(tokens):
        for phrase in PHRASES:
            if word == phrase[0] and tuple(w for w, _ in tokens[index:index + len(phrase)]) == phrase:
                found.append((number, " ".join(phrase)))
    return found


def added_blocks(diff):
    """Each run of added lines in a zero-context diff, as (path, [(line number, text)])."""
    blocks, path, number, current = [], None, 0, []

    def flush():
        if current:
            blocks.append((path, list(current)))
            current.clear()

    for raw in diff.split("\n"):
        if raw.startswith("+++ "):
            flush()
            path = raw[4:].removeprefix("b/")
        elif raw.startswith("--- "):
            flush()
        elif raw.startswith("@@"):
            flush()
            found = HUNK.match(raw)
            number = int(found.group(1)) if found else 0
        elif raw.startswith("+"):
            current.append((number, raw[1:]))
            number += 1
    flush()
    return blocks


def pull_request_range(root):
    """The base, the tip of the pull request, and the merge commit, from its merge checkout."""
    parents = git(root, "rev-list", "--parents", "-n", "1", "HEAD").split()
    if len(parents) != 3:
        raise SystemExit(
            "check-narration: a pull request is checked out as the commit merging it into its "
            "base, and HEAD is not one"
        )
    return parents[1], parents[2]


def branch_base(root, base):
    for ref in ([base] if base else ["upstream/main", "origin/main"]):
        try:
            return git(root, "merge-base", ref, "HEAD").strip()
        except subprocess.CalledProcessError:
            continue
    raise SystemExit("check-narration: no upstream/main or origin/main to measure from; pass --base")


def commit_messages(root, revisions):
    output = git(root, "log", "--no-merges", "--format=%h%x1f%B%x1e", revisions)
    for entry in output.split("\x1e"):
        if entry.strip():
            short, _, message = entry.strip().partition("\x1f")
            yield f"commit {short}", list(enumerate(message.split("\n"), start=1))


def untracked_blocks(root):
    for name in git(root, "ls-files", "--others", "--exclude-standard").split("\n"):
        if not name or name == OWN_FILE or Path(name).name in LOCKFILES:
            continue
        try:
            text = (Path(root) / name).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        yield name, list(enumerate(text.split("\n"), start=1))


def sources(root, base, env):
    """Everything this change writes, as (where, [(line number, text)])."""
    for label, key in (("pull request title", "PR_TITLE"), ("pull request body", "PR_BODY")):
        if env.get(key):
            yield label, list(enumerate(env[key].split("\n"), start=1))
    if env.get("GITHUB_EVENT_NAME") == "pull_request":
        start, tip = pull_request_range(root)
        yield from commit_messages(root, f"{start}..{tip}")
        diff = git(root, "diff", "--no-color", "--no-ext-diff", "--no-renames", "--unified=0",
                   start, "HEAD", "--", ".", *PATHSPECS)
    else:
        fork = branch_base(root, base)
        yield from commit_messages(root, f"{fork}..HEAD")
        diff = git(root, "diff", "--no-color", "--no-ext-diff", "--no-renames", "--unified=0",
                   fork, "--", ".", *PATHSPECS)
        yield from untracked_blocks(root)
    yield from added_blocks(diff)


def check(root, base, env):
    found = []
    for where, lines in sources(root, base, env):
        found.extend(f"{where}:{number}: \"{phrase}\"" for number, phrase in hits(lines))
    return found


def selftest():
    failures = []

    def expect(claim, got, want):
        if got != want:
            failures.append(f"{claim}: expected {want!r}, got {got!r}")

    def lines(*text):
        return list(enumerate(text, start=1))

    expect("plain prose", hits(lines("The store reports nothing.")), [])
    expect("a correction", hits(lines("I was wrong about this")), [(1, "i was wrong")])
    expect("case and punctuation", hits(lines("// As I SAID, it fails")), [(1, "as i said")])
    expect("a curly apostrophe is not a word break", hits(lines("it’s what i said")), [])
    expect(
        "a phrase across a wrapped comment",
        hits(lines("// this corrects", "// what the last commit claimed")),
        [(1, "this corrects what")],
    )
    expect("a longer word is not the phrase", hits(lines("hi was wrong")), [])
    expect("a word inside another is not the phrase", hits(lines("i checkedout")), [])
    expect("a second hit keeps its line", hits(lines("ok", "i assumed it", "i guessed")),
           [(2, "i assumed"), (3, "i guessed")])
    expect("first person only", hits(lines("the caller assumed a path")), [])
    expect("a phrase is not split across unrelated words", hits(lines("i", "was", "here")), [])

    diff = (
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,0 +5,2 @@\n+// one\n+// i assumed\n"
        "@@ -9 +11 @@\n-old\n+// i guessed\n"
        "diff --git a/b.md b/b.md\n--- a/b.md\n+++ b/b.md\n@@ -0,0 +1 @@\n+text\n"
    )
    expect(
        "added lines keep file and number",
        added_blocks(diff),
        [
            ("a.rs", [(5, "// one"), (6, "// i assumed")]),
            ("a.rs", [(11, "// i guessed")]),
            ("b.md", [(1, "text")]),
        ],
    )
    expect("removed lines are not read", added_blocks("--- a/x\n+++ b/x\n@@ -1 +0,0 @@\n-i assumed\n"), [])

    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)

        def run(*args):
            subprocess.run(
                ["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid",
                 "-c", "commit.gpgsign=false", *args],
                cwd=root, check=True, capture_output=True,
            )

        run("init", "-q", "-b", "main")
        (root / "a.txt").write_text("one\n")
        run("add", "a.txt")
        run("commit", "-q", "-m", "start")
        run("checkout", "-q", "-b", "topic")
        expect("a branch with nothing on it", check(root, "main", {}), [])

        (root / "a.txt").write_text("one\ntwo\n")
        run("commit", "-q", "-am", "state the behaviour\n\nI was wrong about the old one.")
        first = git(root, "rev-parse", "--short", "HEAD").strip()
        (root / "b.txt").write_text("fine\n# as I said above\n")
        run("add", "b.txt")
        run("commit", "-q", "-m", "add b")
        (root / "c.txt").write_text("fine\nmy mistake\n")
        got = check(root, "main", {"PR_TITLE": "ok", "PR_BODY": "all good\nI initially thought so"})
        expect(
            "a message, a committed line, an untracked file and a body are all read",
            sorted(got),
            sorted([
                f"commit {first}:3: \"i was wrong\"",
                "b.txt:2: \"as i said\"",
                "c.txt:2: \"my mistake\"",
                "pull request body:2: \"i initially\"",
            ]),
        )
        (root / "a.txt").write_text("one\ntwo\ni verified it\n")
        expect("an uncommitted edit is read", [g for g in check(root, "main", {}) if g.startswith("a.txt")],
               ["a.txt:3: \"i verified\""])

        run("add", "-A")
        run("commit", "-q", "-m", "fold in")
        run("checkout", "-q", "main")
        run("merge", "-q", "--no-ff", "--no-edit", "topic")
        expect(
            "a merge checkout reads the pull request's commits and lines",
            sorted(check(root, "main", {"GITHUB_EVENT_NAME": "pull_request"})),
            sorted([
                f"commit {first}:3: \"i was wrong\"",
                "a.txt:3: \"i verified\"",
                "b.txt:2: \"as i said\"",
                "c.txt:2: \"my mistake\"",
            ]),
        )
        run("checkout", "-q", "topic")
        try:
            check(root, "main", {"GITHUB_EVENT_NAME": "pull_request"})
            failures.append("a pull request checkout that is not a merge was accepted")
        except SystemExit:
            pass

    for failure in failures:
        print(f"selftest: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("selftest: narration checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Fail a change that narrates its writer's own process.")
    ap.add_argument("--base", help="measure from this ref instead of upstream/main or origin/main")
    ap.add_argument("--root", type=Path, default=ROOT, help="the checkout to read")
    ap.add_argument("--selftest", action="store_true", help="check the verdict on known inputs")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    found = check(args.root, args.base, os.environ)
    for line in found:
        print(f"narration: {line}", file=sys.stderr)
    if found:
        print(
            "narration: state what is true about the code in the present tense, as though for the "
            "first time (docs/development/commits.md)",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
