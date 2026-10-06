#!/usr/bin/env python3
"""Bring a brave/bravebot pull request up to date and green, in a worktree of its own.

    pr-fix.py start <pr>                fetch, check the head out in ../<checkout>-<pr>, rebase
    pr-fix.py continue <pr>             stage the resolved files and carry on
    pr-fix.py ci <pr> [--wait]          the checks GitHub ran on the head, with the log of each failure
    pr-fix.py comments <pr>             the review threads and reviews still waiting on a change
    pr-fix.py check <pr> [--target T]   run the checks the changed files call for, or the make
                                        targets named, printing only a failure
    pr-fix.py push <pr>                 push over the head `start` fetched, and nothing newer
    pr-fix.py review <pr>               ask netzenbot-reviewer to review the pull request again

Every step that needs no judgement is decided here, so what it prints is only what a model has to
act on: the line ranges of each conflict, the log of each failing job, the text of each open review
comment. <pr> is a number or a pull request URL, several of them separated by commas or spaces, or
`all` for every open pull request in brave/bravebot by the user git is configured as. Several run in
parallel, each in a process of its own. It works from the main clone or from any worktree.
"""

import argparse
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
from pathlib import Path
from types import SimpleNamespace

try:
    import fcntl
except ImportError:
    fcntl = None

BASE_REPO = "brave/bravebot"
REVIEWER = "netzenbot-reviewer"
SCRIPT = "python3 agents/skills/pr-fix/pr-fix.py"
GITHUB_URL = re.compile(r"(?:^|[@/])github\.com[:/]+([\w.-]+)/([\w.-]+?)(?:\.git)?/?$")
PULL_URL = re.compile(r"github\.com/([\w.-]+)/([\w.-]+)/pull/(\d+)")
OURS = re.compile(r"^<{7}(?: |$)")
THEIRS = re.compile(r"^>{7}(?: |$)")
MAKE_FAILED = re.compile(r"^make(?:\[\d+\])?: \*\*\*")
SPEC_RECORDS = {"agents/unverified-clauses.txt", "agents/renumbered-clauses.txt"}
LOCK_RESOLVES = ["cargo", "metadata", "--locked", "--format-version", "1"]
ANSI = re.compile(r"\x1b\[[0-9;]*m")
LOG_PREFIX = re.compile(r"^[^\t]*\t[^\t]*\t\d{4}-\d\d-\d\dT[\d:.]+Z ?")
JOB_URL = re.compile(r"/actions/runs/(\d+)/job/(\d+)")
PASSED = {"SUCCESS", "NEUTRAL", "SKIPPED"}
STILL_RUNNING = {"PENDING", "EXPECTED"}
WAIT_SECONDS = 540
MAX_PARALLEL = 8
POLL_SECONDS = 30
LOG_LINES = 40
WIDTH = 300
BODY_WIDTH = 1500
THREADS = """
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        pageInfo { hasNextPage }
        nodes {
          isResolved isOutdated path line originalLine
          comments(first: 20) { nodes { author { login } authorAssociation body } }
        }
      }
      reviews(last: 50) { nodes { author { login } authorAssociation state body } }
    }
  }
}
"""


class SharedRepo:
    """One process at a time changes what every worktree shares: remotes, fetched refs, the worktree list."""

    def __init__(self):
        self.handle = None

    def acquire(self, here):
        if fcntl is None or self.handle:
            return
        path = Path(here, git("rev-parse", "--git-common-dir", cwd=here)) / "pr-fix.lock"
        self.handle = open(path, "w")
        fcntl.flock(self.handle, fcntl.LOCK_EX)

    def release(self):
        if self.handle:
            self.handle.close()
            self.handle = None


shared = SharedRepo()


def die(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def run(*args, cwd=None, check=True, env=None):
    done = subprocess.run(args, cwd=cwd, capture_output=True, text=True, env=env)
    if check and done.returncode:
        die(f"{' '.join(map(str, args))} failed:\n{(done.stderr or done.stdout).strip()}")
    return done


def git(*args, cwd):
    return run("git", *args, cwd=cwd).stdout.strip()


def succeeds(*args, cwd):
    return run("git", *args, cwd=cwd, check=False).returncode == 0


def pr_number(arg):
    url = PULL_URL.search(arg)
    if url:
        if f"{url[1]}/{url[2]}".lower() != BASE_REPO:
            die(f"{arg} is not a {BASE_REPO} pull request")
        return int(url[3])
    bare = re.fullmatch(r"#?(\d+)", arg.strip())
    if bare is None:
        die(f"{arg} is neither a pull request number nor a {BASE_REPO} pull request URL")
    return int(bare[1])


def github_remotes(listing):
    """`git remote -v` as {owner/repo: (remote, url)}, the first remote naming a repo winning."""
    found = {}
    for line in listing.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[2] == "(fetch)":
            match = GITHUB_URL.search(parts[1])
            if match:
                found.setdefault(f"{match[1]}/{match[2]}".lower(), (parts[0], parts[1]))
    return found


def fork_url(repo, like):
    if like.startswith("https://"):
        return f"https://github.com/{repo}.git"
    return f"git@github.com:{repo}.git"


def local_branch(head, base, number):
    # A fork's `main` would otherwise land on, and fast-forward, this clone's own.
    return f"pr-{number}" if head == base else head


def marker_ranges(text):
    ranges, start = [], None
    for number, line in enumerate(text.split("\n"), 1):
        if OURS.match(line):
            start = number
        elif THEIRS.match(line) and start is not None:
            ranges.append((start, number))
            start = None
    return ranges


def worktrees(here):
    """{branch: path} for every worktree on a branch, and the main worktree's path.

    A worktree whose directory was deleted is still listed until pruned; it holds nothing.
    """
    found, path, main = {}, None, None
    for line in git("worktree", "list", "--porcelain", cwd=here).splitlines():
        if line.startswith("worktree "):
            path = Path(line[len("worktree "):])
            main = main or path
        elif line.startswith("branch refs/heads/") and path.exists():
            found[line[len("branch refs/heads/"):]] = path
    return found, main


def pull(number, view, here, remotes, create=False):
    """What every step needs, from the pull request and this clone's remotes."""
    if view["state"] != "OPEN":
        die(f"{view['url']} is {view['state'].lower()}")
    if not view.get("headRepository"):
        die(f"{view['url']} has no head repository any more")
    if BASE_REPO not in remotes:
        die(f"no remote of {here} points at {BASE_REPO}")
    base_remote, base_url = remotes[BASE_REPO]
    owner = view["headRepositoryOwner"]["login"]
    head_repo = f"{owner}/{view['headRepository']['name']}".lower()
    if head_repo in remotes:
        head_remote = remotes[head_repo][0]
    elif create:
        head_remote = owner.lower()
        git("remote", "add", head_remote, fork_url(head_repo, base_url), cwd=here)
        print(f"added remote {head_remote} for {head_repo}")
    else:
        die(f"no remote points at {head_repo}; `{SCRIPT} start {number}` adds one")
    base, head = view["baseRefName"], view["headRefName"]
    branch = local_branch(head, base, number)
    checked_out, main = worktrees(here)
    return SimpleNamespace(
        number=number,
        url=view["url"],
        head_sha=view.get("headRefOid", ""),
        rollup=view.get("statusCheckRollup") or [],
        here=here,
        base=base,
        head=head,
        branch=branch,
        base_remote=base_remote,
        head_remote=head_remote,
        onto=f"{base_remote}/{base}",
        tip=f"{head_remote}/{head}",
        tree=checked_out.get(branch) or main.parent / f"{main.name}-{number}",
        checked_out=branch in checked_out,
    )


def lookup(number, create):
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    fields = "url,state,baseRefName,headRefName,headRefOid,headRepository,headRepositoryOwner,statusCheckRollup"
    view = json.loads(
        run("gh", "pr", "view", str(number), "--repo", BASE_REPO, "--json", fields).stdout
    )
    return pull(number, view, here, github_remotes(git("remote", "-v", cwd=here)), create)


def fetch(here, remote, branch):
    git("fetch", "--quiet", remote, f"+refs/heads/{branch}:refs/remotes/{remote}/{branch}", cwd=here)


def rev(ref, cwd):
    return git("rev-parse", "--verify", f"{ref}^{{commit}}", cwd=cwd)


def rebase_dir(tree):
    for name in ("rebase-merge", "rebase-apply"):
        path = Path(tree, git("rev-parse", "--git-path", name, cwd=tree))
        if path.is_dir():
            return path
    return None


def unmerged(tree):
    listing = git("diff", "--name-only", "--diff-filter=U", "-z", cwd=tree)
    return [one for one in listing.split("\0") if one]


def resolved_log(tree):
    return Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / "rebase-resolved"


def resolved(tree):
    """The files a person resolved during the rebase in `tree`, which is all a check is chosen from."""
    log = resolved_log(tree)
    return [one for one in log.read_text(encoding="utf-8").split("\0") if one] if log.is_file() else []


def remember_resolved(tree, files):
    joined = "\0".join(sorted({*resolved(tree), *files}))
    resolved_log(tree).write_text(joined, encoding="utf-8")


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


def package_of(tree, directory):
    manifest = Path(tree, "crates", directory, "Cargo.toml")
    if not manifest.is_file():
        return None
    return tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {}).get("name")


def plan(tree, files):
    """The commands the changed files call for, each the cheapest that could catch what they break.

    A file no rule names gets no local command: CI runs on the pull request anyway, and it is the
    only place the desktop app, the website and the dependency policy are built.
    """
    crates, tests, targets, commands = set(), [], set(), []
    for name in files:
        path = Path(name)
        parts = path.parts
        if len(parts) > 2 and parts[0] == "crates":
            if path.suffix == ".rs" or path.name == "Cargo.toml":
                crates.add(parts[1])
            if len(parts) == 4 and parts[2] == "tests" and path.suffix == ".rs":
                tests.append((parts[1], path.stem))
            if parts[1:3] == ("i18n", "locales"):
                targets.add("check-locales")
        if name == "contrib/untranslated-messages.txt":
            targets.add("check-locales")
        if path.suffix == ".rs" or parts[:2] == ("docs", "specs") or name in SPEC_RECORDS:
            targets.add("check-spec")
        if parts[:2] == (".github", "workflows") or name == "contrib/required-checks.txt":
            targets.add("check-security")
        if path.name in ("Cargo.toml", "Makefile") or path.name.startswith("Dockerfile"):
            targets.add("check-security")
        if path.name in ("Cargo.toml", "Cargo.lock", "package.json", "package-lock.json"):
            targets.add("check-versions")
        if path.name in ("Cargo.toml", "Cargo.lock") and LOCK_RESOLVES not in commands:
            commands.append(LOCK_RESOLVES)
    packages = sorted(filter(None, (package_of(tree, one) for one in crates)))
    if packages:
        flags = [flag for one in packages for flag in ("-p", one)]
        commands.append(["cargo", "fmt", "--all", "--", "--check"])
        commands.append(["cargo", "clippy", *flags, "--all-targets", "--all-features", "--", "-D", "warnings"])
        for directory, test in sorted(tests):
            package = package_of(tree, directory)
            if package:
                commands.append(["cargo", "test", "-p", package, "--locked", "--test", test])
    if targets:
        commands.append(["make", "-k", *sorted(targets)])
    return commands


def describe(tree, name):
    path = Path(tree, name)
    if not path.is_file():
        return f"{name}: deleted on one side; `git rm` it or restore it"
    ranges = marker_ranges(path.read_text(encoding="utf-8", errors="replace"))
    if not ranges:
        return f"{name}: no markers (binary, mode or modify/delete); see `git -C {tree} status`"
    return f"{name}:" + ",".join(f"{start}-{end}" for start, end in ranges)


def report(pr):
    tree = pr.tree
    print(f"worktree {tree}")
    state = rebase_dir(tree)
    if state is None:
        if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
            die(f"{pr.branch} is not on {pr.onto}; `{SCRIPT} start {pr.number}`")
        tip, now = rev(pr.tip, tree), rev("HEAD", tree)
        if now == tip:
            print(f"{pr.url} is already on {pr.onto}; nothing to push")
            return 0
        count = git("rev-list", "--count", f"{pr.onto}..HEAD", cwd=tree)
        print(f"{count} commit(s) on {pr.onto} {rev(pr.onto, tree)[:8]}: {tip[:8]} -> {now[:8]}")
        print(f"next: {SCRIPT} push {pr.number}")
        return 0

    def read(name):
        path = state / name
        return path.read_text().strip() if path.is_file() else "?"

    step = f"{read('msgnum')}/{read('end')}" if (state / "msgnum").is_file() else f"{read('next')}/{read('last')}"
    stopped = run("git", "log", "-1", "--format=%h %s", "REBASE_HEAD", cwd=tree, check=False)
    print(f"stopped at {step}: {stopped.stdout.strip()}")
    files = unmerged(tree)
    if not files:
        print(f"stopped without a conflict; see `git -C {tree} status`")
        return 1
    print("conflicts:")
    for name in files:
        print(f"  {describe(tree, name)}")
    onto, orig = read("onto"), read("orig-head")
    fork = run("git", "merge-base", orig, onto, cwd=tree, check=False).stdout.strip()
    if fork:
        touching = git("log", "--oneline", "--no-decorate", "-n", "10", f"{fork}..{onto}", "--", *files, cwd=tree)
        if touching:
            print(f"{pr.onto} commits on these files:")
            print("\n".join(f"  {line}" for line in touching.splitlines()))
    print(f"resolve each range, then: {SCRIPT} continue {pr.number}")
    return 1


def start(pr):
    git("worktree", "prune", cwd=pr.here)
    fetch(pr.here, pr.base_remote, pr.base)
    fetch(pr.here, pr.head_remote, pr.head)
    tree = pr.tree
    if not pr.checked_out:
        if tree.exists():
            die(f"{tree} exists and is not a worktree on {pr.branch}")
        if succeeds("rev-parse", "--verify", "--quiet", f"refs/heads/{pr.branch}", cwd=pr.here):
            git("worktree", "add", str(tree), pr.branch, cwd=pr.here)
        else:
            git("worktree", "add", "-b", pr.branch, str(tree), pr.tip, cwd=pr.here)
    shared.release()
    if rebase_dir(tree):
        return report(pr)
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes")

    def subjects(ref):
        return set(git("log", "--format=%s", f"{pr.onto}..{ref}", cwd=tree).splitlines())

    if succeeds("merge-base", "--is-ancestor", "HEAD", pr.tip, cwd=tree):
        git("merge", "--ff-only", "--quiet", pr.tip, cwd=tree)
    elif succeeds("merge-base", "--is-ancestor", pr.tip, "HEAD", cwd=tree):
        print(f"{pr.branch} has commits the pull request does not; they are kept")
    elif not subjects(pr.tip) <= subjects("HEAD"):
        die(f"{pr.branch} in {tree} and {pr.tip} have diverged")
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        resolved_log(tree).unlink(missing_ok=True)
        done = run("git", "rebase", pr.onto, cwd=tree, check=False)
        if done.returncode and rebase_dir(tree) is None:
            die(f"git rebase {pr.onto} failed:\n{(done.stderr or done.stdout).strip()}")
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr)


def resume(pr):
    tree = pr.tree
    if rebase_dir(tree) is None:
        return report(pr)
    files = unmerged(tree)
    left = [
        one
        for one in files
        if Path(tree, one).is_file()
        and marker_ranges(Path(tree, one).read_text(encoding="utf-8", errors="replace"))
    ]
    if left:
        print("markers remain:")
        for name in left:
            print(f"  {describe(tree, name)}")
        return 1
    if files:
        git("add", "-A", "--", *files, cwd=tree)
        remember_resolved(tree, files)
    done = run(
        "git", "rebase", "--continue", cwd=tree, check=False, env={**os.environ, "GIT_EDITOR": "true"}
    )
    if done.returncode and rebase_dir(tree) is None:
        die(f"git rebase --continue failed:\n{(done.stderr or done.stdout).strip()}")
    if rebase_dir(tree) is None:
        mark_base(tree)
    return report(pr)


def check(pr, targets):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    commands = [["make", "-k", *targets]] if targets else plan(tree, changed(tree))
    if not commands:
        print("nothing changed needs a local check; CI runs the rest")
        return 0
    # The worktree's .envrc holds what the build reads, and a subprocess never loads it.
    wrap = ["direnv", "exec", str(tree)] if shutil.which("direnv") and Path(tree, ".envrc").is_file() else []
    log = Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / "pr-fix-check.log"
    failed = False
    with open(log, "wb") as out:
        for command in commands:
            shown = " ".join(command)
            out.write(f"$ {shown}\n".encode())
            out.flush()
            begin = out.tell()
            done = subprocess.run([*wrap, *command], cwd=tree, stdout=out, stderr=subprocess.STDOUT)
            if done.returncode == 0:
                print(f"passed: {shown}")
                continue
            failed = True
            out.flush()
            with open(log, "rb") as back:
                back.seek(begin)
                lines = back.read().decode(errors="replace").splitlines()
            print(f"failed: {shown} (full log {log})")
            print("\n".join([one for one in lines if MAKE_FAILED.match(one)] + ["..."] + lines[-30:]))
    return 1 if failed else 0


def push(pr):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes; commit them first")
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        die(f"{pr.branch} is not on {pr.onto}; `{SCRIPT} start {pr.number}`")
    tip, now = rev(pr.tip, tree), rev("HEAD", tree)
    if now == tip:
        print(f"{pr.url} is already at {now[:8]}; nothing to push")
        return 0
    # The lease is the head `start` fetched, so a push made since is refused rather than lost.
    git(
        "push",
        f"--force-with-lease=refs/heads/{pr.head}:{tip}",
        pr.head_remote,
        f"HEAD:refs/heads/{pr.head}",
        cwd=tree,
    )
    print(f"pushed {tip[:8]} -> {now[:8]} to {pr.tip}\n{pr.url}")
    return 0


def review(pr):
    done = run(
        "gh", "api", "--method", "POST",
        f"repos/{BASE_REPO}/pulls/{pr.number}/requested_reviewers",
        "-f", f"reviewers[]={REVIEWER}",
        check=False,
    )
    if done.returncode:
        print(f"could not request {REVIEWER} on {pr.url}:\n{(done.stderr or done.stdout).strip()}", file=sys.stderr)
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


def ci(pr, wait):
    tree = pr.tree
    if tree.is_dir() and rev("HEAD", tree) != pr.head_sha:
        print(f"{tree} is at {rev('HEAD', tree)[:8]} and GitHub tests {pr.head_sha[:8]}; "
              f"these results are for {pr.head_sha[:8]}")
    rollup, waited = pr.rollup, 0
    while True:
        failing, pending, passing = classify(rollup)
        running = bool(pending) or not rollup
        if failing or not running or not wait or waited >= WAIT_SECONDS:
            break
        time.sleep(POLL_SECONDS)
        waited += POLL_SECONDS
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
        lines.append(f"thread on {where}" + (" (outdated: the code it was on has changed)" if thread["isOutdated"] else ""))
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


def child_command(step, number, flags):
    return [sys.executable, str(Path(__file__).resolve()), step, str(number), *flags]


def fan_out(step, numbers, flags):
    """Run the step for each pull request in a process of its own, printing each one's output in order."""

    def one(number):
        done = subprocess.run(
            child_command(step, number, flags), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )
        return done.returncode, done.stdout

    with concurrent.futures.ThreadPoolExecutor(max_workers=min(len(numbers), MAX_PARALLEL)) as pool:
        running = [pool.submit(one, number) for number in numbers]
        codes = []
        for number, future in zip(numbers, running):
            code, out = future.result()
            print(f"== #{number}")
            print(out.rstrip("\n"))
            codes.append(code)
    return 1 if 1 in codes else max(codes)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("step", choices=["start", "continue", "ci", "comments", "check", "push", "review"])
    parser.add_argument("prs", nargs="+", metavar="pr")
    parser.add_argument("--target", action="append", default=[], help="a make target for check to run")
    parser.add_argument("--wait", action="store_true")
    args = parser.parse_args()
    if args.target and args.step != "check":
        parser.error("only check takes make targets")
    if args.wait and args.step != "ci":
        parser.error("only ci can wait")
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    numbers = pr_numbers(args.prs, here)
    if not numbers:
        sys.exit(0)
    if len(numbers) > 1:
        flags = (["--wait"] if args.wait else []) + [f for t in args.target for f in ("--target", t)]
        sys.exit(fan_out(args.step, numbers, flags))
    steps = {
        "start": start,
        "continue": resume,
        "ci": lambda pr: ci(pr, args.wait),
        "comments": comments,
        "check": lambda pr: check(pr, args.target),
        "push": push,
        "review": review,
    }
    try:
        if args.step == "start":
            shared.acquire(here)
        sys.exit(steps[args.step](lookup(numbers[0], create=args.step == "start")))
    finally:
        shared.release()


if __name__ == "__main__":
    main()
