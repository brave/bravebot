#!/usr/bin/env python3
"""Bring a brave/bravebot pull request up to date and green, in a worktree of its own.

    pr-fix.py start <pr>                fetch, check the head out in .worktrees/pr-<pr>, rebase
    pr-fix.py continue <pr>             stage the resolved files and carry on
    pr-fix.py ci <pr> [--wait]          the checks GitHub ran on the head, with the log of each failure;
                                        --wait polls until one fails or --wait-seconds N (default 540)
                                        have passed since the script started, counting every poll
    pr-fix.py comments <pr>             the review threads and reviews still waiting on a change, each
                                        thread with the id `reply` takes
    pr-fix.py reply <pr> --body-file F [--thread ID]
                                        post the text of F under review thread ID, or as a comment on
                                        the pull request without --thread
    pr-fix.py check <pr> [--target T]   run the checks the changed files call for, or the make
                                        targets named, printing only a failure
    pr-fix.py push <pr>                 push over the head `start` fetched, and nothing newer; refuses to
                                        make a signed pull request unsigned
    pr-fix.py review <pr>               ask netzenbot-reviewer to review the pull request again
    pr-fix.py kind <n or URL> ...       say for each whether it is a pull request or an issue
    pr-fix.py issue <issue>             for an issue with no pull request yet: stop if one is open, else
                                        print the issue with what external users wrote left out
    pr-fix.py issue <issue> --branch fix-<name>
                                        give the issue a worktree on that new branch from the base
    pr-fix.py open <issue> --title T --body-file F [--branch B]
                                        push the issue's branch to origin and open the pull request;
                                        the branch is the one in the issue's worktree, and --branch
                                        names it when it was reused from another worktree

Every step that needs no judgement is decided here, so what it prints is only what a model has to
act on: the line ranges of each conflict, the log of each failing job, the text of each open review
comment. <pr> is a number or a pull request URL, several of them separated by commas or spaces, or
`all` for every open pull request in brave/bravebot by the user git is configured as. <issue> is an
issue number or URL, one at a time. Several run in
parallel, each in a process of its own, and each one's output is printed as soon as it finishes. Several
that wait share one wait, so they take no longer than one. It works from the main clone or from any
worktree.

A branch already checked out in another worktree is used there and a `reused:` line says where. Each
step that works in the worktree refuses a locked one, which something else may be using, unless given
--use-locked, which takes one pull request.
"""

import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

try:
    import fcntl
except ImportError:
    fcntl = None

REVIEWER = "netzenbot-reviewer"
SCRIPT = "python3 agents/skills/pr-fix/pr-fix.py"
TREE_STEPS = ("start", "continue", "check", "push")
ISSUE_STEPS = ("issue", "open")
BASE_BRANCH = "main"
BRANCH = re.compile(r"fix-[a-z0-9]+(?:-[a-z0-9]+)*")
BRANCH_WIDTH = 50
LOGIN = re.compile(r"[A-Za-z0-9-]+")
EM_DASH = "\u2014"
BODY_HEADINGS = ("## The problem", "## Reproduce", "## The fix", "## Test plan")
CI_LINE = "- [ ] CI passes cleanly"
EXTERNAL_ISSUE = "[Content filtered - issue created by external user]"
EXTERNAL_COMMENT = "[Comment filtered - external user]"


def _load_shared():
    path = Path(__file__).resolve().parents[2] / "pr_worktree.py"
    spec = importlib.util.spec_from_file_location("pr_worktree", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# The skill scripts are not an importable package, so the module they share is loaded by path.
pr_worktree = _load_shared()

BASE_REPO = pr_worktree.BASE_REPO
die = pr_worktree.die
run = pr_worktree.run
git = pr_worktree.git
pr_number = pr_worktree.pr_number
fetch = pr_worktree.fetch
rev = pr_worktree.rev
rebase_dir = pr_worktree.rebase_dir
resolved = pr_worktree.resolved
plan = pr_worktree.plan
signed = pr_worktree.signed
succeeds = pr_worktree.succeeds
worktrees = pr_worktree.worktrees
github_remotes = pr_worktree.github_remotes
issue_number = pr_worktree.issue_number
PULL_URL = pr_worktree.PULL_URL
ISSUE_URL = pr_worktree.ISSUE_URL


# What `gh pr view` is asked for differs by script.
FIELDS = "url,state,baseRefName,headRefName,headRefOid,headRepository,headRepositoryOwner,statusCheckRollup"


def pull(number, view, here, remotes, create=False):
    return pr_worktree.pull(number, view, here, remotes, create, SCRIPT)


def lookup(number, create):
    return pr_worktree.lookup(number, create, SCRIPT, FIELDS)
ANSI = re.compile(r"\x1b\[[0-9;]*m")
LOG_PREFIX = re.compile(r"^[^\t]*\t[^\t]*\t\d{4}-\d\d-\d\dT[\d:.]+Z ?")
JOB_URL = re.compile(r"/actions/runs/(\d+)/job/(\d+)")
PASSED = {"SUCCESS", "NEUTRAL", "SKIPPED"}
STILL_RUNNING = {"PENDING", "EXPECTED"}
WAIT_SECONDS = 540
MAX_PARALLEL = 8
POLL_SECONDS = 30
LOG_LINES = 40
WIDTH = pr_worktree.WIDTH
BODY_WIDTH = 1500
THREADS = """
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        pageInfo { hasNextPage }
        nodes {
          isResolved isOutdated path line originalLine
          comments(first: 20) { nodes { databaseId author { login } authorAssociation body } }
        }
      }
      reviews(last: 50) { nodes { author { login } authorAssociation state body } }
    }
  }
}
"""


def lock_path(here):
    """The lock file for the repository `here` belongs to, named from its common git directory.

    It is in the temp directory because on macOS the sandbox refuses a write under .git from any
    program but git. It only has to serialise these scripts, and every worktree of one clone has the
    same common git directory.
    """
    common = Path(here, git("rev-parse", "--git-common-dir", cwd=here)).resolve()
    digest = hashlib.sha256(str(common).encode()).hexdigest()[:16]
    return Path(tempfile.gettempdir()) / f"pr-fix-{digest}.lock"


class SharedRepo:
    """One process at a time changes what every worktree shares: remotes, fetched refs, the worktree list."""

    def __init__(self):
        self.handle = None

    def acquire(self, here):
        if fcntl is None or self.handle:
            return
        self.handle = open(lock_path(here), "w")
        fcntl.flock(self.handle, fcntl.LOCK_EX)

    def release(self):
        if self.handle:
            self.handle.close()
            self.handle = None


shared = SharedRepo()


def base_marker(tree):
    return Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / "pr-fix-base"


def mark_base(tree):
    """Record where the branch stands once it is on its base, so later work can be told apart."""
    base_marker(tree).write_text(git("rev-parse", "HEAD", cwd=tree), encoding="utf-8")


def changed(tree):
    """What a check is chosen from: the files resolved in the rebase and those changed since."""
    names = set(resolved(tree))
    marker = base_marker(tree)
    if marker.is_file():
        since = marker.read_text(encoding="utf-8").strip()
        names |= {one for one in git("diff", "--name-only", "-z", since, cwd=tree).split("\0") if one}
    names |= {one for one in git("ls-files", "--others", "--exclude-standard", "-z", cwd=tree).split("\0") if one}
    return sorted(names)


def report(pr, said=""):
    return pr_worktree.report(pr, SCRIPT, said)


def start(pr):
    pr_worktree.announce(pr)
    pr_worktree.add_worktree(pr)
    shared.release()
    tree = pr.tree
    if rebase_dir(tree):
        return report(pr)
    pr_worktree.catch_up_with_pull(pr, tree)
    said = pr_worktree.rebase_onto(pr, tree)
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, said)


def resume(pr):
    pr_worktree.announce(pr)
    tree = pr.tree
    if rebase_dir(tree) is None:
        return report(pr)
    said = pr_worktree.resolve_or_stop(tree)
    if said is None:
        return 1
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr, said)


def check(pr, targets):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    commands = [["make", "-k", *targets]] if targets else plan(tree, changed(tree))
    if not commands:
        print("nothing changed needs a local check; CI runs the rest")
        return 0
    return pr_worktree.run_commands(tree, commands, "pr-fix-check.log")


def push(pr):
    return pr_worktree.push(pr, SCRIPT)


def review(pr):
    done = run(
        "gh", "api", "--method", "POST",
        f"repos/{BASE_REPO}/pulls/{pr.number}/requested_reviewers",
        "-f", f"reviewers[]={REVIEWER}",
        check=False,
    )
    if done.returncode:
        print(f"could not request {REVIEWER} on {pr.url}:\n{pr_worktree.printed(done)}", file=sys.stderr)
        return 1
    print(f"requested {REVIEWER} on {pr.url}")
    return 0


def classify(rollup):
    """Split a pull request's checks into (failing, pending, passing), each as (name, link, verdict)."""
    failing, pending, passing = [], [], []
    for one in rollup:
        if one.get("__typename") == "StatusContext":
            name, link, verdict = one["context"], one.get("targetUrl") or "", one["state"]
            bucket = passing if verdict == "SUCCESS" else pending if verdict in STILL_RUNNING else failing
        else:
            workflow = one.get("workflowName")
            name = f"{workflow} / {one['name']}" if workflow else one["name"]
            link = one.get("detailsUrl") or ""
            if one.get("status") != "COMPLETED":
                verdict, bucket = one.get("status", "PENDING"), pending
            else:
                verdict = one.get("conclusion") or "UNKNOWN"
                bucket = passing if verdict in PASSED else failing
        bucket.append((name, link, verdict))
    return failing, pending, passing


def tail(text, count=LOG_LINES):
    cleaned = (LOG_PREFIX.sub("", ANSI.sub("", one)) for one in text.splitlines())
    lines = [one[:WIDTH] for one in cleaned if one.strip()]
    return "\n".join(lines[-count:])


def failure_log(link):
    job = JOB_URL.search(link)
    if job is None:
        return ""
    done = run(
        "gh", "run", "view", job[1], "--repo", BASE_REPO, "--job", job[2], "--log-failed", check=False
    )
    return tail(done.stdout or done.stderr)


def ci(pr, until):
    """Report the checks on the head. With `until`, a `time.monotonic()` value, poll for them to settle until then."""
    tree = pr.tree
    if tree.is_dir() and rev("HEAD", tree) != pr.head_sha:
        print(f"{tree} is at {rev('HEAD', tree)[:8]} and GitHub tests {pr.head_sha[:8]}; "
              f"these results are for {pr.head_sha[:8]}")
    rollup = pr.rollup
    while True:
        failing, pending, passing = classify(rollup)
        running = bool(pending) or not rollup
        left = None if until is None else until - time.monotonic()
        if failing or not running or left is None or left <= 0:
            break
        time.sleep(min(POLL_SECONDS, left))
        rollup = json.loads(
            run("gh", "pr", "view", str(pr.number), "--repo", BASE_REPO, "--json", "statusCheckRollup").stdout
        ).get("statusCheckRollup") or []
    print(f"{len(passing)} passed, {len(failing)} failed, {len(pending)} pending")
    for name, link, verdict in failing:
        print(f"failed ({verdict.lower()}): {name} {link}")
        log = failure_log(link)
        if log:
            print("\n".join(f"    {line}" for line in log.splitlines()))
    for name, link, verdict in pending:
        print(f"pending: {name} {link}")
    if not rollup:
        print("GitHub has reported no checks for this head yet")
    return 1 if failing else 2 if pending or not rollup else 0


def quoted(node):
    author = node["author"]
    who = f"{author['login']}, {node['authorAssociation'].lower()}" if author else "a deleted account"
    text = node["body"].strip()
    text = text if len(text) <= BODY_WIDTH else text[:BODY_WIDTH] + " [cut]"
    return f"    {who}:\n" + "\n".join(f"      {line}" for line in text.splitlines())


def open_comments(data):
    """The unresolved threads and the latest change-asking review of each reviewer, as lines."""
    request = data["data"]["repository"]["pullRequest"]
    lines = []
    threads = request["reviewThreads"]
    if threads["pageInfo"]["hasNextPage"]:
        lines.append("more than 100 review threads; only the first 100 are shown")
    for thread in threads["nodes"]:
        if thread["isResolved"]:
            continue
        where = thread["path"] + (f":{thread['line'] or thread['originalLine']}" if thread["line"] or thread["originalLine"] else "")
        started = thread["comments"]["nodes"][0]["databaseId"]
        outdated = " (outdated: the code it was on has changed)" if thread["isOutdated"] else ""
        lines.append(f"thread {started} on {where}{outdated}")
        lines.extend(quoted(c) for c in thread["comments"]["nodes"])
    latest = {}
    for review in request["reviews"]["nodes"]:
        latest[review["author"]["login"] if review["author"] else ""] = review
    for review in latest.values():
        if review["state"] in ("CHANGES_REQUESTED", "COMMENTED") and review["body"].strip():
            lines.append(f"review ({review['state'].lower().replace('_', ' ')})")
            lines.append(quoted(review))
    return lines


def comments(pr):
    owner, name = BASE_REPO.split("/")
    data = json.loads(
        run(
            "gh", "api", "graphql", "-f", f"query={THREADS}", "-F", f"owner={owner}", "-F", f"name={name}",
            "-F", f"number={pr.number}",
        ).stdout
    )
    lines = open_comments(data)
    if not lines:
        print("no review comment is waiting on a change")
        return 0
    print("\n".join(lines))
    return 1


def reply(pr, thread, body_file):
    """Post the text of `body_file` under review thread `thread`, or on the pull request without one."""
    try:
        body = Path(body_file).read_text(encoding="utf-8").strip()
    except OSError as error:
        die(f"cannot read {body_file}: {error}")
    if not body:
        die(f"{body_file} is empty, so there is nothing to post")
    if EM_DASH in body:
        die(f"{body_file} has an em-dash")
    path = (
        f"repos/{BASE_REPO}/pulls/{pr.number}/comments/{thread}/replies"
        if thread is not None
        else f"repos/{BASE_REPO}/issues/{pr.number}/comments"
    )
    done = run("gh", "api", "--method", "POST", path, "-f", f"body={body}", "--jq", ".html_url", check=False)
    if done.returncode:
        print(f"could not post on {pr.url}:\n{pr_worktree.printed(done)}", file=sys.stderr)
        return 1
    print(done.stdout.strip())
    return 0


def my_pulls(here):
    login = git("config", "user.name", cwd=here)
    if not login:
        die("git has no user.name, so there is no author to list pull requests for")
    listing = run(
        "gh", "pr", "list", "--repo", BASE_REPO, "--author", login, "--state", "open", "--limit", "100",
        "--json", "number",
    ).stdout
    numbers = sorted(one["number"] for one in json.loads(listing))
    if not numbers:
        print(f"no open {BASE_REPO} pull request by {login}")
    return numbers


def pr_numbers(args, here):
    """The pull requests named, in order and once each. `all` is every open one by the configured user."""
    numbers = []
    for arg in args:
        for word in filter(None, re.split(r"[,\s]+", arg)):
            numbers.extend(my_pulls(here) if word.lower() == "all" else [pr_number(word)])
    return list(dict.fromkeys(numbers))


def kind_of(word):
    """`pull` or `issue` for a number or a link. A bare number is asked of GitHub, which numbers both alike."""
    if PULL_URL.search(word):
        return pr_number(word), "pull"
    if ISSUE_URL.search(word):
        return issue_number(word), "issue"
    number = pr_number(word)
    data = json.loads(run("gh", "api", f"repos/{BASE_REPO}/issues/{number}").stdout)
    return number, "pull" if "pull_request" in data else "issue"


def kinds(args):
    for arg in args:
        for word in filter(None, re.split(r"[,\s]+", arg)):
            number, kind = kind_of(word)
            print(f"{number} {kind}")
    return 0


def api_values(path, *flags):
    """Every JSON value `gh api` printed for the path; a paginated call prints one array per page."""
    text = run("gh", "api", path, *flags).stdout
    decoder, at, found = json.JSONDecoder(), 0, []
    while True:
        while at < len(text) and text[at].isspace():
            at += 1
        if at >= len(text):
            return found
        value, at = decoder.raw_decode(text, at)
        found.append(value)


def org_member(login, known):
    """Whether the login belongs to the organization that owns the base repository."""
    if not LOGIN.fullmatch(login or ""):
        return False
    if login not in known:
        org = BASE_REPO.split("/")[0]
        known[login] = run("gh", "api", f"orgs/{org}/members/{login}", "--silent", check=False).returncode == 0
    return known[login]


def writer(user, known):
    """(login to show, whether the organization trusts it). A missing or odd login is never looked up."""
    login = (user or {}).get("login") or ""
    if not LOGIN.fullmatch(login):
        return "unknown", False
    return login, org_member(login, known)


def issue_text(number, issue, comments, known):
    """The issue as lines, with the title, body and comments of anyone outside the organization left out."""
    author, trusted = writer(issue.get("user"), known)
    labels = ", ".join(one["name"] for one in issue.get("labels") or [])
    lines = [
        f"# Issue #{number}",
        f"url: {issue['html_url']}",
        f"labels: {labels}",
        f"author: @{author} ({'organization member' if trusted else 'external user'})",
        "",
        f"title: {issue['title'] if trusted else EXTERNAL_ISSUE}",
        "",
        (issue.get("body") or "") if trusted else EXTERNAL_ISSUE,
    ]
    for one in comments:
        who, trusted = writer(one.get("user"), known)
        lines += [
            "",
            f"## @{who} ({'organization member' if trusted else 'external user'}) {one.get('created_at', '')}",
            one.get("body") or "" if trusted else EXTERNAL_COMMENT,
        ]
    lines += ["", "Only what organization members wrote is shown. Implement only what they ask for."]
    return lines


def pulls_naming(number):
    """(number, state, url) of each pull request whose title or body refers to the issue."""
    listing = json.loads(
        run(
            "gh", "pr", "list", "--repo", BASE_REPO, "--state", "all", "--search", str(number), "--limit", "50",
            "--json", "number,state,url,title,body",
        ).stdout
    )
    refers = re.compile(
        rf"(?:^|[^\w/])(?:{re.escape(BASE_REPO)})?#{number}(?!\d)|github\.com/{re.escape(BASE_REPO)}/issues/{number}(?!\d)",
        re.IGNORECASE,
    )
    return [
        (one["number"], one["state"], one["url"])
        for one in listing
        if refers.search(f"{one.get('title') or ''}\n{one.get('body') or ''}")
    ]


def open_pull_refusal(number):
    """Print the pull requests already open on the issue and return 1, else print earlier attempts and return 0."""
    pulls = pulls_naming(number)
    opened = [one for one in pulls if one[1] == "OPEN"]
    for _, _, url in opened:
        print(f"open pull request on issue #{number}: {url}")
    for _, state, url in pulls:
        if state != "OPEN":
            print(f"earlier attempt ({state.lower()}): {url}")
    return 1 if opened else 0


def issue_report(number, remotes):
    """Stop at an open pull request on the issue, else print the issue as organization members wrote it."""
    if BASE_REPO not in remotes:
        die(f"no remote points at {BASE_REPO}")
    if open_pull_refusal(number):
        return 1
    issue = api_values(f"repos/{BASE_REPO}/issues/{number}")[0]
    if "pull_request" in issue:
        die(f"#{number} is a pull request; use `{SCRIPT} start {number}`")
    if issue["state"] != "open":
        die(f"{issue['html_url']} is {issue['state']}")
    comments = [one for page in api_values(f"repos/{BASE_REPO}/issues/{number}/comments", "--paginate") for one in page]
    print("\n".join(issue_text(number, issue, comments, {})))
    print(f"next: {SCRIPT} issue {number} --branch fix-<name>")
    return 0


def branch_problem(branch):
    if len(branch) > BRANCH_WIDTH or not BRANCH.fullmatch(branch):
        return (
            f"{branch} is not a branch name to use: fix- then lowercase letters, digits and hyphens, "
            f"at most {BRANCH_WIDTH} characters"
        )
    return None


def issue_worktree(number, branch, here, remotes):
    """Give the issue a worktree on a new branch from the base, or use the one the branch is in."""
    problem = branch_problem(branch)
    if problem:
        die(problem)
    if BASE_REPO not in remotes:
        die(f"no remote points at {BASE_REPO}")
    if open_pull_refusal(number):
        return 1
    base_remote = remotes[BASE_REPO][0]
    git("worktree", "prune", cwd=here)
    fetch(here, base_remote, BASE_BRANCH)
    checked_out, main = worktrees(here)
    held = checked_out.get(branch)
    if held:
        if held.locked:
            die(f"{held.path} is locked, so something else may be using it; ask the person before working in it")
        print(f"worktree {held.path}")
        print(f"reused: {branch} is already checked out there")
        return 0
    tree = pr_worktree.worktree_path(main, number, issue=True)
    if tree.exists():
        die(f"{tree} exists and is not a worktree on {branch}")
    if succeeds("rev-parse", "--verify", "--quiet", f"refs/heads/{branch}", cwd=here):
        git("worktree", "add", str(tree), branch, cwd=here)
    else:
        git("worktree", "add", "-b", branch, str(tree), f"{base_remote}/{BASE_BRANCH}", cwd=here)
    print(f"worktree {tree}")
    print(f"branch {branch} from {base_remote}/{BASE_BRANCH}")
    return 0


def body_problem(body, number):
    """Why a pull request body does not have the shape the repository asks for, else None."""
    lines = [one.rstrip() for one in body.strip().splitlines()]
    if not lines or not re.fullmatch(rf"(?:Closes|Part of) {re.escape(BASE_REPO)}#{number}", lines[0]):
        return f"the first line must be `Closes {BASE_REPO}#{number}`, or `Part of` where the change is partial"
    if not any(one.startswith("User impact: ") and one.strip() != "User impact:" for one in lines):
        return "the body needs a `User impact: ` line"
    at = -1
    for heading in BODY_HEADINGS:
        if heading not in lines[at + 1 :]:
            return f"the body needs the heading `{heading}` after the ones before it"
        at = lines.index(heading, at + 1)
    if lines[-1] != CI_LINE:
        return f"the last line must be `{CI_LINE}`"
    if EM_DASH in body or "co-authored-by" in body.lower():
        return "the body has an em-dash or a co-author line"
    return None


def origin_repo(remotes):
    for repo, (remote, _) in remotes.items():
        if remote == "origin":
            return repo
    return None


def issue_branch(checked_out, main, number):
    """The branch checked out in the issue's own worktree, else None."""
    tree = pr_worktree.worktree_path(main, number, issue=True)
    for branch, held in checked_out.items():
        if pr_worktree.same_place(held.path, tree):
            return branch
    return None


def open_pull(number, branch, title, body_file, here, remotes):
    """Push the issue's branch to origin and open a pull request on the base repository for it.

    Without a branch it is the one in the issue's worktree, which `issue --branch` made.
    """
    if BASE_REPO not in remotes:
        die(f"no remote points at {BASE_REPO}")
    repo = origin_repo(remotes)
    if repo is None:
        die("no remote is named origin, which is where the branch is pushed")
    checked_out, main = worktrees(here)
    if branch is None:
        branch = issue_branch(checked_out, main, number)
        if branch is None:
            die(
                f"no worktree for issue #{number}; `{SCRIPT} issue {number} --branch fix-<name>` makes one, "
                f"or pass --branch for a branch checked out in another worktree"
            )
    problem = branch_problem(branch)
    if problem:
        die(problem)
    held = checked_out.get(branch)
    if held is None:
        die(f"{branch} is checked out in no worktree; `{SCRIPT} issue {number} --branch {branch}`")
    tree = held.path
    onto = f"{remotes[BASE_REPO][0]}/{BASE_BRANCH}"
    if rebase_dir(tree):
        die("finish the rebase first")
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes; commit them first")
    if not succeeds("merge-base", "--is-ancestor", onto, "HEAD", cwd=tree):
        die(f"{branch} is not on {onto}; rebase it onto {onto} first")
    if git("rev-list", "--count", f"{onto}..HEAD", cwd=tree) == "0":
        die(f"{branch} has no commit on {onto}")
    if not title.strip() or EM_DASH in title:
        die("the title is empty or has an em-dash")
    problem = body_problem(Path(body_file).read_text(encoding="utf-8"), number)
    if problem:
        die(problem)
    args = ["push", "origin", f"HEAD:refs/heads/{branch}"]
    done = run("git", *args, cwd=tree, check=False)
    if done.returncode and pr_worktree.PUSH_DENIED.search(done.stderr):
        done, _ = pr_worktree.push_as_another_key(args, tree, done)
    if done.returncode:
        die(f"git {' '.join(args)} failed:\n{(done.stderr or done.stdout).strip()}")
    head = branch if repo == BASE_REPO else f"{repo.split('/')[0]}:{branch}"
    made = run(
        "gh", "pr", "create", "--repo", BASE_REPO, "--head", head, "--base", BASE_BRANCH,
        "--title", title, "--body-file", str(body_file), "--assignee", "@me",
    )
    print(made.stdout.strip())
    return 0


def issue_steps(args, here):
    number = issue_number(args.prs[0])
    remotes = github_remotes(git("remote", "-v", cwd=here))
    if args.step == "open":
        return open_pull(number, args.branch, args.title, args.body_file, here, remotes)
    if not args.branch:
        return issue_report(number, remotes)
    shared.acquire(here)
    try:
        return issue_worktree(number, args.branch, here, remotes)
    finally:
        shared.release()


def child_command(step, number, flags):
    return [sys.executable, str(Path(__file__).resolve()), step, str(number), *flags]


def fan_out(step, numbers, flags, queued=True):
    """Run the step for each pull request in a process of its own, printing each one's output as it finishes."""

    def one(number):
        done = subprocess.run(
            child_command(step, number, flags), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )
        return number, done.returncode, done.stdout

    # A wait is never queued: one behind a full pool would start after the others had spent the shared time.
    workers = min(len(numbers), MAX_PARALLEL) if queued else len(numbers)
    codes = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        for future in concurrent.futures.as_completed([pool.submit(one, number) for number in numbers]):
            number, code, out = future.result()
            # Flushed because stdout is a pipe, so a run stopped before the last one settles still returns these.
            print(f"== #{number}\n" + out.rstrip("\n"), flush=True)
            codes.append(code)
    return 1 if 1 in codes else max(codes)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument(
        "step",
        choices=["start", "continue", "ci", "comments", "check", "push", "review", "reply", "kind", *ISSUE_STEPS],
    )
    parser.add_argument("prs", nargs="+", metavar="pr")
    parser.add_argument("--branch", help="the branch an issue is worked on: fix-<name>; open reads it from the issue's worktree without it")
    parser.add_argument("--title", help="the title of the pull request open creates")
    parser.add_argument("--body-file", help="the body of the pull request open creates, or the text reply posts")
    parser.add_argument("--thread", type=int, metavar="ID", help="the review thread reply answers, as comments prints it")
    parser.add_argument("--target", action="append", default=[], help="a make target for check to run")
    parser.add_argument("--wait", action="store_true")
    parser.add_argument(
        "--wait-seconds", type=int, metavar="N",
        help=f"how long --wait may take, counted from when the script started (default {WAIT_SECONDS})",
    )
    parser.add_argument(
        "--use-locked", action="store_true", help="work in a locked worktree the branch is already checked out in"
    )
    args = parser.parse_args()
    if args.target and args.step != "check":
        parser.error("only check takes make targets")
    if args.wait and args.step != "ci":
        parser.error("only ci can wait")
    if args.wait_seconds is not None and not args.wait:
        parser.error("--wait-seconds needs --wait")
    if args.use_locked and args.step not in TREE_STEPS:
        parser.error(f"only {', '.join(TREE_STEPS[:-1])} and {TREE_STEPS[-1]} work in a worktree")
    seconds = WAIT_SECONDS if args.wait_seconds is None else args.wait_seconds
    if args.branch and args.step not in ISSUE_STEPS:
        parser.error("only issue and open take --branch")
    if args.title and args.step != "open":
        parser.error("only open takes --title")
    if args.body_file and args.step not in ("open", "reply"):
        parser.error("only open and reply take --body-file")
    if args.thread is not None and args.step != "reply":
        parser.error("only reply takes --thread")
    if args.step == "open" and not (args.title and args.body_file):
        parser.error("open needs --title and --body-file")
    if args.step == "reply" and not args.body_file:
        parser.error("reply needs --body-file")
    if args.step in ISSUE_STEPS and (len(args.prs) != 1 or re.search(r"[,\s]", args.prs[0].strip())):
        parser.error(f"{args.step} takes one issue")
    if args.step == "kind":
        sys.exit(kinds(args.prs))
    until = time.monotonic() + seconds if args.wait else None
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    if args.step in ISSUE_STEPS:
        sys.exit(issue_steps(args, here))
    numbers = pr_numbers(args.prs, here)
    if not numbers:
        sys.exit(0)
    if args.step == "reply" and len(numbers) > 1:
        parser.error("reply takes one pull request")
    if args.use_locked and len(numbers) > 1:
        parser.error("--use-locked takes the one pull request whose locked worktree the person agreed to")
    if len(numbers) > 1:
        flags = [f for t in args.target for f in ("--target", t)]
        if until is not None:
            # Each process counts from its own start, so what is left of the one shared wait is handed over now.
            flags += ["--wait", "--wait-seconds", str(max(int(until - time.monotonic()), 0))]
        sys.exit(fan_out(args.step, numbers, flags, queued=until is None))
    steps = {
        "start": start,
        "continue": resume,
        "ci": lambda pr: ci(pr, until),
        "comments": comments,
        "check": lambda pr: check(pr, args.target),
        "push": push,
        "review": review,
        "reply": lambda pr: reply(pr, args.thread, args.body_file),
    }
    try:
        if args.step == "start":
            shared.acquire(here)
        pr = lookup(numbers[0], create=args.step == "start")
        if args.step in TREE_STEPS:
            words = [args.step, str(pr.number)] + [f for t in args.target for f in ("--target", t)]
            pr_worktree.refuse_locked(pr, SCRIPT, words, args.use_locked)
        sys.exit(steps[args.step](pr))
    finally:
        shared.release()


if __name__ == "__main__":
    main()
