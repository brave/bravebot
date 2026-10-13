"""What rebase.py and pr-fix.py define alike: reading a pull request, its worktree, and the rebase in it.

Each script loads this module by path and binds the names it uses, so a fix made here reaches both.
"""

import json
import os
import re
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import tomllib
from pathlib import Path
from types import SimpleNamespace

BASE_REPO = "brave/bravebot"
GITHUB_URL = re.compile(r"(?:^|[@/])github\.com[:/]+([\w.-]+)/([\w.-]+?)(?:\.git)?/?$")
PULL_URL = re.compile(r"github\.com/([\w.-]+)/([\w.-]+)/pull/(\d+)")
OURS = re.compile(r"^<{7}(?: |$)")
THEIRS = re.compile(r"^>{7}(?: |$)")
MAKE_FAILED = re.compile(r"^make(?:\[\d+\])?: \*\*\*")
SPEC_RECORDS = {"agents/unverified-clauses.txt", "agents/renumbered-clauses.txt"}
LOCK_RESOLVES = ["cargo", "metadata", "--locked", "--format-version", "1"]
PUSH_DENIED = re.compile(r"Permission to \S+ denied to ")


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


def pull(number, view, here, remotes, create, script):
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
        die(f"no remote points at {head_repo}; `{script} start {number}` adds one")
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


def lookup(number, create, script, fields):
    here = Path(git("rev-parse", "--show-toplevel", cwd=None))
    view = json.loads(
        run("gh", "pr", "view", str(number), "--repo", BASE_REPO, "--json", fields).stdout
    )
    return pull(number, view, here, github_remotes(git("remote", "-v", cwd=here)), create, script)


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
    """The commands the given files call for, each the cheapest that could catch what they break.

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


def run_commands(tree, commands, log_name):
    """Run each command in `tree` into a log, printing a line for each and the tail of a failure."""
    # The worktree's .envrc holds what the build reads, and a subprocess never loads it.
    wrap = ["direnv", "exec", str(tree)] if shutil.which("direnv") and Path(tree, ".envrc").is_file() else []
    log = Path(git("rev-parse", "--absolute-git-dir", cwd=tree)) / log_name
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


def signed(sha, cwd):
    """Whether the commit carries a signature, which needs no key to tell."""
    headers = git("cat-file", "commit", sha, cwd=cwd).split("\n\n", 1)[0]
    return any(line.startswith("gpgsig") for line in headers.splitlines())


def signatures(onto, head, cwd):
    """(signed, unsigned) counts for the commits on `head` that `onto` lacks."""
    flags = [signed(sha, cwd) for sha in git("rev-list", f"{onto}..{head}", cwd=cwd).splitlines()]
    return flags.count(True), flags.count(False)


def refuse_unsigning(pr, tip, now, tree):
    """Stop a push from `tip` to `now` that leaves more of a signed pull request's commits unsigned."""
    had_signed, had_unsigned = signatures(pr.onto, tip, tree)
    unsigned = signatures(pr.onto, now, tree)[1]
    if had_signed and unsigned > had_unsigned:
        die(
            f"{pr.tip} has signed commits and the push would leave {unsigned} commit(s) unsigned, so it "
            "would replace the author's signed commits with unsigned ones or add unsigned commits to them. "
            "Report the signing error to the person. Do not rebase the pull request on GitHub with "
            "`update-branch`, which also leaves its commits unsigned."
        )


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


def push_head(pr, tree, tip):
    """Push HEAD to the pull request's head branch, over `tip` and nothing newer."""
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
