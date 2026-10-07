#!/usr/bin/env python3
"""Rebase a brave/bravebot pull request onto its base, in a worktree of its own.

    rebase.py start <pr>                fetch, check the head out in ../<checkout>-<pr>, rebase
    rebase.py continue <pr>             stage the resolved files and carry on
    rebase.py check <pr> [target ...]   run the checks the resolved files call for, or the make
                                        targets named, printing only a failure
    rebase.py push <pr>                 push over the head `start` fetched, and nothing newer

Every step that needs no judgement is decided here, so what it prints is only what a model has to
act on: the line ranges of each conflict. <pr> is a number or a pull request URL.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from types import SimpleNamespace

BASE_REPO = "brave/bravebot"
SCRIPT = "python3 agents/skills/rebase/rebase.py"
GITHUB_URL = re.compile(r"(?:^|[@/])github\.com[:/]+([\w.-]+)/([\w.-]+?)(?:\.git)?/?$")
PULL_URL = re.compile(r"github\.com/([\w.-]+)/([\w.-]+)/pull/(\d+)")
OURS = re.compile(r"^<{7}(?: |$)")
THEIRS = re.compile(r"^>{7}(?: |$)")
PUSH_DENIED = re.compile(r"Permission to \S+ denied to ")
MAKE_FAILED = re.compile(r"^make(?:\[\d+\])?: \*\*\*")
SPEC_RECORDS = {"agents/unverified-clauses.txt", "agents/renumbered-clauses.txt"}
LOCK_RESOLVES = ["cargo", "metadata", "--locked", "--format-version", "1"]


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
    """{branch: path} for every worktree on a branch, and the main worktree's path."""
    found, path, main = {}, None, None
    for line in git("worktree", "list", "--porcelain", cwd=here).splitlines():
        if line.startswith("worktree "):
            path = Path(line[len("worktree "):])
            main = main or path
        elif line.startswith("branch refs/heads/"):
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
    fields = "url,state,baseRefName,headRefName,headRepository,headRepositoryOwner"
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


def package_of(tree, directory):
    manifest = Path(tree, "crates", directory, "Cargo.toml")
    if not manifest.is_file():
        return None
    return tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {}).get("name")


def plan(tree, files):
    """The commands the resolved files call for, each the cheapest that could catch what they break.

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
        if path.name in ("Cargo.toml", "Cargo.lock", "package.json"):
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
    return report(pr)


def check(pr, targets):
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{SCRIPT} continue {pr.number}`")
    commands = [["make", "-k", *targets]] if targets else plan(tree, resolved(tree))
    if not commands:
        print("nothing resolved needs a local check; CI runs the rest")
        return 0
    # The worktree's .envrc holds what the build reads, and a subprocess never loads it.
    wrap = ["direnv", "exec", str(tree)] if shutil.which("direnv") and Path(tree, ".envrc").is_file() else []
    log = Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / "rebase-check.log"
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
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        die(f"{pr.branch} is not on {pr.onto}; `{SCRIPT} start {pr.number}`")
    tip, now = rev(pr.tip, tree), rev("HEAD", tree)
    if now == tip:
        print(f"{pr.url} is already at {now[:8]}; nothing to push")
        return 0
    # The lease is the head `start` fetched, so a push made since is refused rather than lost.
    args = [
        "push",
        f"--force-with-lease=refs/heads/{pr.head}:{tip}",
        pr.head_remote,
        f"HEAD:refs/heads/{pr.head}",
    ]
    done = run("git", *args, cwd=tree, check=False)
    key = None
    if done.returncode and PUSH_DENIED.search(done.stderr):
        done, key = push_as_another_key(args, tree, done)
    if done.returncode:
        die(f"git {' '.join(args)} failed:\n{(done.stderr or done.stdout).strip()}")
    if key:
        print(f"pushed as the ssh-agent key {key}")
    print(f"pushed {tip[:8]} -> {now[:8]} to {pr.tip}\n{pr.url}")
    return 0


def agent_keys():
    listed = run("ssh-add", "-L", check=False)
    if listed.returncode:
        return []
    return [line for line in listed.stdout.splitlines() if line.startswith(("ssh-", "ecdsa-", "sk-"))]


def push_as_another_key(args, tree, denied):
    """GitHub refused the key ssh offered first; try each other key the agent holds, once."""
    with tempfile.TemporaryDirectory() as tmp:
        for number, line in enumerate(agent_keys()):
            file = Path(tmp) / f"key{number}.pub"
            file.write_text(line + "\n")
            env = {**os.environ, "GIT_SSH_COMMAND": f"ssh -i {file} -o IdentitiesOnly=yes"}
            done = run("git", *args, cwd=tree, check=False, env=env)
            if not done.returncode:
                return done, line.split(maxsplit=2)[-1]
            if not PUSH_DENIED.search(done.stderr):
                return done, None
    return denied, None


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("step", choices=["start", "continue", "check", "push"])
    parser.add_argument("pr")
    parser.add_argument("targets", nargs="*")
    args = parser.parse_args()
    if args.targets and args.step != "check":
        parser.error("only check takes make targets")
    pr = lookup(pr_number(args.pr), create=args.step == "start")
    if args.step == "start":
        sys.exit(start(pr))
    if args.step == "continue":
        sys.exit(resume(pr))
    if args.step == "check":
        sys.exit(check(pr, args.targets))
    sys.exit(push(pr))


if __name__ == "__main__":
    main()
