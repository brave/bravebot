#!/usr/bin/env python3
"""Fails a pull request whose body does not say which cases it leaves undone, with an issue for each.

A body that says a case is not handled, in prose, leaves nothing in the issue tracker for anyone to
pick up, and the case is found again later as a new defect. The body therefore has a line:

    Not covered: none

or the line followed by one bullet per case, each naming the issue that tracks it, as `#N` or
`brave/bravebot#N`:

    Not covered:
    - `aws` keeps bravebot's own PATH (#2756)
    - hooks named by a relative path (brave/bravebot#2753)

This decides what a tool can: the line exists, and every bullet under it names an issue. It cannot
read whether the list is complete or whether the issue is about that case, which a reviewer checks
against the diff (docs/best-practices/gaps.md), and it does not look an issue number up. Text inside a
fenced code block or an HTML comment is not read, since a person reading the body does not see it as
the line, so a body can quote it.

Only pull requests opened by netzenbot, the account the agent's work goes through, are held to the
line. Other authors may write it and a reviewer still reads their description for the cases it
leaves (docs/best-practices/gaps.md). A run that cannot name the author holds the body to the line.

The body reaches this as the PR_BODY environment variable and the author's login as PR_AUTHOR,
never interpolated into a command. Standard library only, like the other checks CI runs without a
toolchain.
"""

import argparse
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LABEL = "Not covered:"

# `#N` on its own, or `brave/bravebot#N`. A reference to another repository, a URL and a word
# ending in `#N` are not either of those, and `#0` is no issue.
ISSUE = re.compile(
    r"(?<![\w/#])#[1-9][0-9]*(?!\w)|(?<![\w/.-])brave/bravebot#[1-9][0-9]*(?!\w)", re.IGNORECASE
)
BULLET = re.compile(r"(?:[-*+]|[0-9]+[.)])(?:\s|$)")
COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
FENCE = re.compile(r"\s{0,3}(`{3,}|~{3,})(.*)$")

# The accounts whose pull requests have to carry the line. A login is the one GitHub reports for the
# opener, which a person cannot choose for a branch or a title, and it ignores case.
HELD = ("netzenbot",)

SHAPE = (
    f"`{LABEL} none`, or `{LABEL}` followed by one bullet per case, each naming the issue that "
    "tracks it as `#N` or `brave/bravebot#N`"
)


def fence_of(raw):
    """The (character, length, text after it) of a fence line, else None. A backtick fence has no
    backtick after it."""
    found = FENCE.match(raw)
    if not found:
        return None
    marks, rest = found.groups()
    if marks[0] == "`" and "`" in rest:
        return None
    return marks[0], len(marks), rest.strip()


def unfenced(body):
    """The lines of `body` outside fenced code blocks, without their line endings.

    A block closes on a fence of its own character, at least as long, with nothing after it, so a
    shorter fence or one with an info string inside it is part of what it quotes.
    """
    opened = None
    for raw in body.splitlines():
        fence = fence_of(raw)
        if opened is None:
            if fence:
                opened = fence[:2]
                continue
            yield raw.rstrip()
        elif fence and fence[0] == opened[0] and fence[1] >= opened[1] and not fence[2]:
            opened = None


def cases_under(lines, at):
    """The bullets that follow the label on `lines[at]`, each as the list of lines it spans.

    A blank line does not end the list. A line that is neither a bullet nor indented under one does.
    """
    items = []
    for line in lines[at + 1 :]:
        if not line.strip():
            continue
        if BULLET.match(line):
            items.append([line])
        elif line[0].isspace() and items:
            items[-1].append(line)
        else:
            break
    return items


def label_problem(lines, at):
    value = lines[at][len(LABEL) :].strip()
    items = cases_under(lines, at)
    if value.lower() in ("none", "none."):
        if items:
            return f"`{LABEL} none` is followed by a list of cases; write {SHAPE}"
        return None
    if value:
        return (
            f"the text after `{LABEL}` is not `none`; put each case on a bullet of its own, naming "
            "the issue that tracks it as `#N` or `brave/bravebot#N`"
        )
    if not items:
        return f"`{LABEL}` lists no case; write `{LABEL} none` where there is none"
    for item in items:
        text = " ".join(part.strip() for part in item)
        if not ISSUE.search(text):
            shown = text if len(text) <= 60 else text[:57] + "..."
            return f"the case `{shown}` under `{LABEL}` names no issue (`#N` or `brave/bravebot#N`)"
    return None


def not_covered_problem(body):
    """Why `body` does not say what it leaves undone the way the repository asks, else None."""
    lines = list(unfenced(COMMENT.sub("", body)))
    labels = [at for at, line in enumerate(lines) if line.startswith(LABEL)]
    if not labels:
        return f"the body needs a `{LABEL}` line: {SHAPE}"
    for at in labels:
        problem = label_problem(lines, at)
        if problem:
            return problem
    return None


def has_body(env):
    """Whether `env` holds a pull request body to read: a pull request run always does, and an
    empty body is one with nothing in it, so it is read and fails."""
    return "PR_BODY" in env or env.get("GITHUB_EVENT_NAME") == "pull_request"


def author_is_held(author):
    """Whether a pull request opened by `author` has to carry the line. An author that is not given
    is held, so a draft checked locally and a workflow that lost PR_AUTHOR both fail rather than pass."""
    return not author or author.lower() in HELD


def body_problem(env):
    if not author_is_held(env.get("PR_AUTHOR")):
        return None
    return not_covered_problem(env.get("PR_BODY", ""))


def step_text(workflow, command):
    """The step of `workflow` that runs `command`, from its `- ` line to the next step's."""
    steps = re.split(r"(?m)^(?=      - )", workflow)
    return next((step for step in steps if re.search(rf"(?m)^\s+run: {re.escape(command)}$", step)), "")


def selftest():
    failures = []

    def expect(claim, got, want):
        if got != want:
            failures.append(f"{claim}: expected {want!r}, got {got!r}")

    def body(*lines):
        return "\n".join(lines)

    def passes(claim, text):
        expect(claim, not_covered_problem(text), None)

    def fails(claim, text, mentioning=""):
        got = not_covered_problem(text)
        if not got or mentioning not in got:
            failures.append(f"{claim}: expected a problem mentioning {mentioning!r}, got {got!r}")

    heading = ["", "## The problem", "It is broken.", "", "## Test plan", "- [x] make check"]
    closes = "Closes brave/bravebot#2758"

    passes("none passes", body(closes, "User impact: x", "Not covered: none", *heading))
    passes("none in any case passes", body("Not covered: None", *heading))
    fails("a body with no line fails", body(closes, "User impact: x", *heading), "needs a `Not covered:` line")
    fails("an empty body fails", "", "needs a `Not covered:` line")
    fails("a different label fails", body("Not Covered: none", "Covered: all"))
    fails("a comment holding the line is not the line", body("<!-- Not covered: none -->", *heading))
    fails("a line that is not at the start is not the line", body("  Not covered: none"))
    fails("a multi-line comment holding the line is not the line", body("<!--", "Not covered: none", "-->", *heading))
    fails(
        "an issue inside a comment does not cite a case",
        body("Not covered:", "- `aws` keeps the PATH <!-- (#2756) -->", *heading),
        "names no issue",
    )
    passes("a comment after the line leaves it standing", body("Not covered: none <!-- nothing to add -->", *heading))
    passes("none may end a sentence", body("Not covered: None.", *heading))

    passes(
        "every case citing an issue passes",
        body("Not covered:", "- `aws` keeps the PATH (#2756)", "- hooks (brave/bravebot#2753)", *heading),
    )
    fails(
        "a case with no issue fails",
        body("Not covered:", "- `aws` keeps the PATH", *heading),
        "names no issue",
    )
    fails(
        "one cited case does not cover the others",
        body("Not covered:", "- `aws` keeps the PATH (#2756)", "- hooks keep it too", *heading),
        "hooks keep it too",
    )
    fails(
        "an issue elsewhere in the body does not cite a case",
        body(closes, "Not covered:", "- `aws` keeps the PATH", *heading),
        "names no issue",
    )
    fails(
        "an issue cited by a different case does not count for the one after it",
        body("Not covered:", "- first (#1)", "- second", "- third (#3)"),
        "second",
    )

    for text, held in (
        ("#12", True),
        ("(#12)", True),
        ("#12.", True),
        ("brave/bravebot#12", True),
        ("Brave/Bravebot#12", True),
        ("see #12 and #13", True),
        ("other/repo#12", False),
        ("xbrave/bravebot#12", False),
        ("fork/brave/bravebot#12", False),
        ("word#12", False),
        ("#12abc", False),
        ("#0", False),
        ("#", False),
        ("https://github.com/brave/bravebot/issues/12", False),
        ("a bare number 12", False),
    ):
        got = not_covered_problem(body("Not covered:", f"- case {text}"))
        expect(f"`{text}` {'names' if held else 'does not name'} an issue", got is None, held)

    passes("a case wrapped onto the next line passes", body("Not covered:", "- `aws` keeps", "  the PATH (#2756)"))
    passes("a star bullet passes", body("Not covered:", "* `aws` (#2756)"))
    passes("a numbered case passes", body("Not covered:", "1. `aws` (#2756)", "2. hooks (#2753)"))
    passes("a blank line before the list passes", body("Not covered:", "", "- `aws` (#2756)", *heading))
    passes(
        "bullets after a heading are not cases",
        body("Not covered:", "- `aws` (#2756)", "", "## Test plan", "- [x] make check", "- [ ] CI passes cleanly"),
    )
    fails(
        "a blank line inside the list does not hide an uncited case",
        body("Not covered:", "- `aws` (#2756)", "", "- hooks"),
        "hooks",
    )

    fails(
        "a second line is held to the rule as well",
        body("Not covered: none", "", "Not covered:", "- `aws` keeps the PATH"),
        "names no issue",
    )
    fails("a case on the line itself fails", body("Not covered: `aws` keeps the PATH (#2756)"), "bullet of its own")
    fails("several cases on the line fail", body("Not covered: aws (#2756), hooks"), "bullet of its own")
    fails("no cases and no none fails", body("Not covered:", *heading), "lists no case")
    fails(
        "none beside a list of cases fails",
        body("Not covered: none", "- `aws` keeps the PATH", *heading),
        "followed by a list",
    )

    fails("a quoted line in a fence is not the line", body("```", "Not covered: none", "```", *heading))
    fails(
        "a quoted line does not stand in for a real one that fails",
        body("```", "Not covered: none", "```", "Not covered:", "- `aws` keeps the PATH"),
        "names no issue",
    )
    passes(
        "a quoted line beside a real one passes",
        body("~~~text", "Not covered:", "- aws", "~~~", "Not covered: none", *heading),
    )
    fails(
        "a fence with an info string does not close the block it is in",
        body("```", "code", "```text", "Not covered: none", "```"),
        "needs a `Not covered:` line",
    )
    fails(
        "a shorter fence does not close the block it is in",
        body("````", "```", "Not covered: none", "```", "````"),
        "needs a `Not covered:` line",
    )
    passes(
        "a longer fence closes the block",
        body("```", "quoted", "````", "Not covered: none"),
    )
    passes("code quoted on one line is not a fence", body("```code``` is quoted", "Not covered: none"))
    passes(
        "a fence is closed by its own marker",
        body("```", "~~~", "Not covered:", "- aws", "```", "Not covered: none"),
    )

    passes("a body written in the web interface passes", "Not covered: none\r\n\r\n## The problem\r\nx\r\n")
    fails(
        "a web interface body with an uncited case fails",
        "Not covered:\r\n- `aws` keeps the PATH\r\n\r\n## The problem\r\n",
        "names no issue",
    )
    passes(
        "a web interface body with cited cases passes",
        "Not covered:\r\n- `aws` keeps the PATH (#2756)\r\n\r\n## The problem\r\n",
    )

    pull = {"GITHUB_EVENT_NAME": "pull_request", "PR_BODY": "Not covered: none", "PR_AUTHOR": "netzenbot"}
    expect("a pull request body that passes", body_problem(pull), None)
    expect("a pull request with no line", body_problem(dict(pull, PR_BODY="text")) is not None, True)
    expect("a pull request with an empty body is read", has_body(dict(pull, PR_BODY="")), True)
    expect("a pull request with an empty body fails", body_problem(dict(pull, PR_BODY="")) is not None, True)
    expect("a pull request run with no body variable is read", has_body({"GITHUB_EVENT_NAME": "pull_request"}), True)
    expect(
        "a pull request run with no body variable fails",
        body_problem({"GITHUB_EVENT_NAME": "pull_request"}) is not None,
        True,
    )
    expect("a local run with no body has nothing to read", has_body({}), False)
    expect("a local run given a body reads it", has_body({"PR_BODY": "text"}), True)
    expect("a push has nothing to read", has_body({"GITHUB_EVENT_NAME": "push"}), False)
    bot = dict(pull, PR_BODY="Update the crate")
    for author in ("netzenbot", "Netzenbot", "NETZENBOT", ""):
        expect(f"{author!r} is held to the line", body_problem(dict(bot, PR_AUTHOR=author)) is not None, True)
    expect("a missing author is held to the line", body_problem({"PR_BODY": "text"}) is not None, True)
    expect("the bot's body with the line passes", body_problem(dict(bot, PR_BODY="Not covered: none")), None)
    for author in ("bbondy", "aekeus", "renovate[bot]", "netzenbot-reviewer", "evil-netzenbot", "netzenbot[bot]"):
        expect(f"{author!r} is not held to the line", body_problem(dict(bot, PR_AUTHOR=author)), None)
    expect("no author is held and another account is not", [author_is_held(None), author_is_held("bbondy")], [True, False])

    workflow = (ROOT / ".github/workflows/narration.yml").read_text(encoding="utf-8")
    expect(
        "an edit to the body runs the workflow again",
        bool(re.search(r"(?m)^\s+types:\s*\[[^\]]*\bedited\b", workflow)),
        True,
    )
    step = step_text(workflow, "make check-pr-body")
    expect("the workflow runs make check-pr-body", bool(step), True)
    for variable, source in (
        ("PR_BODY", "github.event.pull_request.body"),
        ("PR_AUTHOR", "github.event.pull_request.user.login"),
    ):
        expect(
            f"the step reads {variable} from the event",
            bool(re.search(rf"(?m)^\s+{variable}: \${{{{ {re.escape(source)} }}}}$", step)),
            True,
        )

    for failure in failures:
        print(f"selftest: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("selftest: pull request body checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Fail a pull request body that has no Not covered line.")
    ap.add_argument("--selftest", action="store_true", help="check the verdict on known inputs")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if not has_body(os.environ):
        print("check-pr-body: PR_BODY is not set, so there is no pull request body to read")
        return 0
    problem = body_problem(os.environ)
    if problem:
        print(f"check-pr-body: {problem} (docs/development/commits.md)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
