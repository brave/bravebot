"""What rebase.py and pr-fix.py define alike: reading a pull request, its worktree, and the rebase in it.

Each script loads this module by path and binds the names it uses, so a fix made here reaches both.
"""

import json
import os
import re
import shlex
import shutil
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
import tomllib
from pathlib import Path
from types import SimpleNamespace

BASE_REPO = "brave/bravebot"
# `github.com`, or an ssh config alias for it such as `github-work` (`git@github-work:owner/repo.git`).
# An alias is a single word after `github-`, so `github-evil.example.com` is a different host.
GITHUB_URL = re.compile(r"(?:^|[@/])github(?:\.com|-[\w-]+)[:/]+([\w.-]+)/([\w.-]+?)(?:\.git)?/?$")
# A directory to hold the pull request worktrees. Relative to the main clone unless absolute.
WORKTREES_ENV = "BRAVEBOT_PR_WORKTREES"
PULL_URL = re.compile(r"github\.com/([\w.-]+)/([\w.-]+)/pull/(\d+)")
ISSUE_URL = re.compile(r"github\.com/([\w.-]+)/([\w.-]+)/issues/(\d+)")
OURS = re.compile(r"^<{7}(?: |$)")
THEIRS = re.compile(r"^>{7}(?: |$)")
MAKE_FAILED = re.compile(r"^make(?:\[\d+\])?: \*\*\*")
SPEC_RECORDS = {"agents/unverified-clauses.txt", "agents/renumbered-clauses.txt"}
LOCK_RESOLVES = ["cargo", "metadata", "--locked", "--format-version", "1"]
PUSH_DENIED = re.compile(r"Permission to \S+ denied to ")
REASON_WIDTH = 120
# How much of what git said when a rebase stopped is shown: this many lines, each cut to this width.
REASON_LINES = 3
WIDTH = 300
REBASE_STATES = ("rebase-merge", "rebase-apply")


def die(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def printed(done):
    """A finished command's stderr, then its stdout with a JSON error object cut to `message` and `errors`."""
    # gh api states the reason in that JSON and only a one-line status on stderr; a partial GraphQL response also carries `data`.
    out = done.stdout.strip()
    try:
        body = json.loads(out)
    except (ValueError, RecursionError):
        body = None
    if isinstance(body, dict):
        reason = {key: body[key] for key in ("message", "errors") if body.get(key)}
        if reason:
            out = json.dumps(reason)
    return "\n".join(part for part in (done.stderr.strip(), out) if part)


def run(*args, cwd=None, check=True, env=None):
    done = subprocess.run(args, cwd=cwd, capture_output=True, text=True, env=env)
    if check and done.returncode:
        die(f"{' '.join(map(str, args))} failed:\n{printed(done)}")
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


def issue_number(arg):
    """The number of a brave/bravebot issue named by a link, `#12` or `12`."""
    url = ISSUE_URL.search(arg)
    if url:
        if f"{url[1]}/{url[2]}".lower() != BASE_REPO:
            die(f"{arg} is not a {BASE_REPO} issue")
        return int(url[3])
    bare = re.fullmatch(r"#?(\d+)", arg.strip())
    if bare is None:
        die(f"{arg} is neither an issue number nor a {BASE_REPO} issue URL")
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
    # Only the scheme is taken from the base remote. An ssh alias there names the account that owns
    # the base remote's key, which is not necessarily the account that owns the fork.
    if like.startswith("https://"):
        return f"https://github.com/{repo}.git"
    return f"git@github.com:{repo}.git"


def worktree_path(main, number, issue=False):
    """Where a pull request's worktree goes: `.worktrees/pr-<n>` in the main clone, which git ignores.

    That is inside the workspace, so the file tools reach it. `BRAVEBOT_PR_WORKTREES` names another
    directory, where the name also carries the clone's so that clones can share one. An issue's
    worktree is `issue-<n>` in either place, so it never shares a name with a pull request of the
    same number.
    """
    chosen = os.environ.get(WORKTREES_ENV)
    if chosen:
        return main / Path(chosen).expanduser() / f"{main.name}-{'issue-' if issue else ''}{number}"
    return main / ".worktrees" / f"{'issue' if issue else 'pr'}-{number}"


def standard_path(pr):
    """Where the worktree of `pr` would be made, which an issue's plan states itself."""
    return getattr(pr, "standard", None) or worktree_path(pr.main, pr.number)


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


def rebasing(tree):
    """The branch a rebase in `tree` is rewriting, which `git worktree list` does not name while it runs."""
    done = run("git", "rev-parse", "--absolute-git-dir", cwd=tree, check=False)
    if done.returncode:
        return None
    for name in REBASE_STATES:
        head = Path(done.stdout.strip(), name, "head-name")
        if head.is_file():
            ref = head.read_text(encoding="utf-8", errors="replace").strip()
            return ref[len("refs/heads/"):] if ref.startswith("refs/heads/") else None
    return None


def worktrees(here):
    """({branch: checkout}, main) for every worktree on a branch, and the main worktree's path.

    A checkout has the worktree's `path`, whether it is `locked`, and the `reason` given for the lock.
    A worktree in the middle of a rebase is listed as detached and still holds the branch it is rewriting.
    A worktree whose directory was deleted is still listed until pruned; it holds nothing.
    """
    found, main = {}, None
    for record in git("worktree", "list", "--porcelain", cwd=here).split("\n\n"):
        path = branch = None
        detached = locked = False
        reason = ""
        for line in record.splitlines():
            if line.startswith("worktree "):
                path = Path(line[len("worktree "):])
            elif line.startswith("branch refs/heads/"):
                branch = line[len("branch refs/heads/"):]
            elif line == "detached":
                detached = True
            elif line == "locked" or line.startswith("locked "):
                locked, reason = True, line[len("locked "):]
        main = main or path
        if path is None or not path.exists():
            continue
        branch = branch or (rebasing(path) if detached else None)
        if branch:
            found[branch] = SimpleNamespace(path=path, locked=locked, reason=reason)
    return found, main


def lock_reason(held):
    """The reason a worktree was locked with, on one line and no longer than a short sentence."""
    shown = "".join(one if one.isprintable() else "?" for one in held.reason)
    if not shown:
        return "no reason given"
    return shown if len(shown) <= REASON_WIDTH else shown[:REASON_WIDTH] + "..."


def same_place(one, other):
    return os.path.realpath(one) == os.path.realpath(other)


def reuse_note(pr):
    """One line for a branch already checked out where a new worktree would not go, else an empty string."""
    standard = standard_path(pr)
    if not pr.checked_out or same_place(pr.tree, standard):
        return ""
    tree, main, here = (Path(os.path.realpath(one)) for one in (pr.tree, pr.main, pr.here))
    facts = []
    if tree == main:
        facts.append("the main clone itself")
    elif tree == here:
        facts.append("the checkout this was run from")
    elif not any(tree.is_relative_to(root) for root in (main, here)):
        facts.append(f"outside the main clone {pr.main}, so the file tools refuse it until it is opened")
    if pr.held.locked:
        facts.append(f"locked: {lock_reason(pr.held)}")
    where = f" ({'; '.join(facts)})" if facts else ""
    note = f"reused: {pr.branch} is already checked out in {pr.tree}{where}, so it is used rather than a new worktree"
    if tree not in (main, here):
        quoted = shlex.quote(str(pr.tree))
        unlock = f"git worktree unlock {quoted}; " if pr.held.locked else ""
        note += f". For a new one at {standard}, the person removes it: {unlock}git worktree remove {quoted}"
    return note


def announce(pr):
    """Say which worktree a step works in, and where it is when the script did not choose the place."""
    print(f"worktree {pr.tree}", flush=True)
    note = reuse_note(pr)
    if note:
        print(note, flush=True)


def refuse_locked(pr, script, words, allowed):
    """Stop a step that would work in a locked worktree, which something else may be using, unless the person agreed.

    `words` are the step's arguments as given, so the command offered for running it again is the same step.
    A session started in the worktree chose it, so the lock does not stand between it and the step.
    """
    if not pr.checked_out or not pr.held.locked or allowed or same_place(pr.tree, pr.here):
        return
    quoted = shlex.quote(str(pr.tree))
    die(
        f"{pr.tree} is locked ({lock_reason(pr.held)}), so something else may be using it, and no worktree "
        f"was touched. Ask the person, unless they have already agreed for this pull request: to work in it "
        f"anyway, run `{script} {shlex.join([*words, '--use-locked'])}`; to use "
        f"{standard_path(pr)} instead, they unlock and remove it themselves: "
        f"git worktree unlock {quoted}; git worktree remove {quoted}"
    )


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
    held = checked_out.get(branch)
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
        main=main,
        held=held,
        tree=held.path if held else worktree_path(main, number),
        checked_out=held is not None,
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
    for name in REBASE_STATES:
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


def add_worktree(pr):
    """Prune, fetch the base and the head, and give the branch a worktree if it has none."""
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


def catch_up_with_pull(pr, tree):
    """Fast-forward the worktree to the pull request's head, or say what stops it."""
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


def rebase_onto(pr, tree):
    """Rebase onto the base unless already on it. Returns what git printed to stderr."""
    if succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        return ""
    resolved_log(tree).unlink(missing_ok=True)
    done = run("git", "rebase", pr.onto, cwd=tree, check=False)
    if done.returncode and rebase_dir(tree) is None:
        die(f"git rebase {pr.onto} failed:\n{(done.stderr or done.stdout).strip()}")
    return done.stderr


def resolve_or_stop(tree):
    """Stage the resolved files and continue the rebase.

    Returns None after printing the files that still hold conflict markers, and git's stderr otherwise.
    """
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
        return None
    if files:
        git("add", "-A", "--", *files, cwd=tree)
        remember_resolved(tree, files)
    done = run(
        "git", "rebase", "--continue", cwd=tree, check=False, env={**os.environ, "GIT_EDITOR": "true"}
    )
    if done.returncode and rebase_dir(tree) is None:
        die(f"git rebase --continue failed:\n{(done.stderr or done.stdout).strip()}")
    return done.stderr


def stop_reason(text):
    """What git said when a rebase stopped, from its first error on, without the hints that follow."""
    lines = [one.strip() for one in re.split(r"[\r\n]+", text)]
    lines = [one for one in lines if one and not one.startswith(("hint:", "Rebasing ("))]
    first = next((n for n, one in enumerate(lines) if one.startswith(("error:", "fatal:"))), len(lines) - 1)
    return [one[:WIDTH] for one in lines[max(first, 0) :][:REASON_LINES]]


def conflict_listing(pr, tree, files, script):
    """The conflicted files with their marker ranges, and the base's recent commits on them."""
    state = rebase_dir(tree)

    def read(name):
        path = state / name
        return path.read_text().strip() if path.is_file() else ""

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
    print(f"resolve each range, then: {script} continue {pr.number}")


def report(pr, script, said=""):
    """Say where the rebase stands. `said` is what git printed when it last stopped."""
    tree = pr.tree
    state = rebase_dir(tree)
    if state is None:
        if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
            die(f"{pr.branch} is not on {pr.onto}; `{script} start {pr.number}`")
        tip, now = rev(pr.tip, tree), rev("HEAD", tree)
        if now == tip:
            print(f"{pr.url} is already on {pr.onto}; nothing to push")
            return 0
        count = git("rev-list", "--count", f"{pr.onto}..HEAD", cwd=tree)
        print(f"{count} commit(s) on {pr.onto} {rev(pr.onto, tree)[:8]}: {tip[:8]} -> {now[:8]}")
        print(f"next: {script} push {pr.number}")
        return 0

    def read(name):
        path = state / name
        return path.read_text().strip() if path.is_file() else ""

    number, total = (read("msgnum"), read("end")) if (state / "msgnum").is_file() else (read("next"), read("last"))
    step = f" at {number}/{total}" if number and total else ""
    stopped = run("git", "log", "-1", "--format=%h %s", "REBASE_HEAD", cwd=tree, check=False)
    print(f"stopped{step}: {stopped.stdout.strip()}")
    files = unmerged(tree)
    if not files:
        reason = stop_reason(said)
        print("stopped without a conflict" + ("; git said:" if reason else ""))
        for line in reason:
            print(f"  {line}")
        print(f"see `git -C {tree} status`")
        return 1
    conflict_listing(pr, tree, files, script)
    return 1


def push(pr, script):
    """Push the rebased head over the one `start` fetched, refusing to unsign or to drop uncommitted work."""
    tree = pr.tree
    if rebase_dir(tree):
        die(f"finish the rebase first: `{script} continue {pr.number}`")
    if git("status", "--porcelain", "--untracked-files=no", cwd=tree):
        die(f"{tree} has uncommitted changes; commit them first")
    if not succeeds("merge-base", "--is-ancestor", pr.onto, "HEAD", cwd=tree):
        die(f"{pr.branch} is not on {pr.onto}; `{script} start {pr.number}`")
    tip, now = rev(pr.tip, tree), rev("HEAD", tree)
    if now == tip:
        print(f"{pr.url} is already at {now[:8]}; nothing to push")
        return 0
    refuse_unsigning(pr, tip, now, tree)
    push_head(pr, tree, tip)
    print(f"pushed {tip[:8]} -> {now[:8]} to {pr.tip}\n{pr.url}")
    return 0
