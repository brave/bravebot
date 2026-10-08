#!/usr/bin/env python3
"""The checks and the call that every poster of an issue on brave/bravebot shares.

    python3 agents/issue_helper.py post --title TITLE --body-file FILE --label NAME [--label NAME ...]
        [--assignee LOGIN] [--repo OWNER/NAME]

The `issue-poster` definition (agents/agents/issue-poster.md) runs this to file an issue, and the
scripted posters under agents/skills/ import it, so a label or a login the repository would refuse
is turned away the same way whichever of them posts. `gh issue create` fails on both, and finding
that out on the fourth of six issues leaves half a report filed.

Creating a label changes what everybody sees, so a missing label is a refusal that prints the
`gh label create` line for a person to run, never something this does.
"""

import argparse
import json
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys

REPO = "brave/bravebot"


def gh(args, repo=None):
    """A `gh` call, as the text it printed.

    Every argument is its own list element and no shell is involved, so a title or a search term is
    an argument whatever it holds. A title may be written by a lane reading code that an attacker
    chose the wording of.

    `repo` is left out for the calls that name the repository in the path themselves, since `gh api`
    takes no `--repo`.
    """
    result = subprocess.run(
        ["gh", *args, *(["--repo", repo] if repo else [])],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError((result.stderr or result.stdout).strip())
    return result.stdout


def existing_labels(repo):
    listed = json.loads(gh(["label", "list", "--limit", "200", "--json", "name"], repo))
    return {one["name"] for one in listed}


def assignable(repo, login):
    """Whether GitHub would accept this login as an assignee on this repository."""
    try:
        gh(["api", f"repos/{repo}/assignees/{login}"])
    except RuntimeError:
        return False
    return True


def refusal(repo, labels, assignee=None, *, have=None, can_assign=None):
    """Why nothing should be posted, as the lines to print, or None when it is clear to post.

    `have` and `can_assign` default to the real lookups. A caller that was handed its own, such as a
    poster under test, passes them.
    """
    have = have or existing_labels
    can_assign = can_assign or assignable
    try:
        known = have(repo)
    except RuntimeError as problem:
        return [f"could not read the labels of {repo}: {problem}"]
    missing = [label for label in sorted(set(labels)) if label not in known]
    if missing:
        lines = [
            f"{repo} has no label {', '.join(missing)}",
            "",
            "Nothing was posted. Creating a label changes what everybody sees, so it is a",
            "person's call:",
        ]
        lines += [f"  gh label create {label} --repo {repo} --description ... --color ..." for label in missing]
        return lines
    if assignee and not can_assign(repo, assignee):
        return [
            f"{repo} would not take {assignee} as an assignee",
            "",
            "Nothing was posted. A login the repository refuses fails the create it is passed to.",
        ]
    return None


def create(repo, title, body_file, labels, assignee=None):
    """File the issue and return its URL. One `--label` for each label."""
    args = ["issue", "create", "--title", title, "--body-file", str(body_file)]
    for label in labels:
        args += ["--label", label]
    if assignee:
        args += ["--assignee", assignee]
    return gh(args, repo).strip().splitlines()[-1]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    post = commands.add_parser("post", help="check the labels and the assignee, then file the issue")
    post.add_argument("--repo", default=REPO)
    post.add_argument("--title", required=True)
    post.add_argument("--body-file", required=True)
    post.add_argument("--label", action="append", default=[], dest="labels")
    post.add_argument("--assignee", default=None)
    args = parser.parse_args(argv)

    why = refusal(args.repo, args.labels, args.assignee)
    if why:
        print("\n".join(why), file=sys.stderr)
        return 2
    try:
        print(create(args.repo, args.title, args.body_file, args.labels, args.assignee))
    except RuntimeError as problem:
        print(f"could not file {args.title!r}: {problem}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
