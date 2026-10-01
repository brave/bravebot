#!/usr/bin/env python3
"""Which checks a change needs, decided from the paths it touches.

A job skipped by its condition reports success, and branch protection counts that as passing, so
a wrong "not needed" here is a check that passed without running. Every doubt therefore resolves
toward running: a path no rule names needs everything, a change to the files that make this
decision needs everything, a run that is not a pull request needs everything, and the workflows
run a job unless this said "false", so a run where this failed to answer runs every job.

The checks that answer in seconds are not selected at all: the specs, the security audit, the
catalogs, the versions and the script selftests run on every change. That is what makes it safe
for a path here to need nothing heavier.

Standard library only, and no TOML parser, because the one thing read from a manifest is which
workspace crates it names.
"""

import argparse
import contextlib
import io
import os
import re
# Fixed git commands with argument lists, never a shell.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

AREAS = ("rust", "ui", "build", "docs", "npm", "deps")
EVERYTHING = frozenset(AREAS)
NOTHING = frozenset()

# What makes this decision: the workflows holding the conditions, the recipes the local run
# drives, and this file.
DECIDES = ("Makefile", "contrib/affected-checks.py")
DECIDES_UNDER = (".github/workflows/",)

# Outside crates/, and read by a test all the same.
READ_BY_RUST = ("docs/specs/layering.md",)

# Outside crates/, run by the installer tests themselves rather than read as data: a change to
# one is a change to what those tests exercise, whatever else reads it.
READ_BY_INSTALLER_TESTS = {
    "install.sh": frozenset({"rust"}),
    "npm/scripts/postinstall.js": frozenset({"npm", "rust"}),
}

# Named by a crate's source without being read from this tree, which the selftest's scan cannot
# tell from a read by itself.
NAMED_NOT_READ = {
    ".gitignore": "the name git reads in any repository, and a fixture's in the git tests",
    "README.md": "a fixture's name in the manifest, policy and reference tests",
    "agents/AGENTS.md": "a marker doctor checks exists, which its tests write for themselves",
    "agents/setup.py": "a marker doctor checks exists, which its tests write for themselves",
    "docs/development/agent-configuration.md": "a marker doctor checks exists, likewise",
}

# Read by nothing heavier than the checks that always run.
QUIET = (
    "README.md", "CHANGELOG.md", "LICENSE", ".gitignore", ".envrc.example",
    ".github/CODEOWNERS", ".github/renovate.json",
)
QUIET_UNDER = ("docs/", "agents/", "contrib/", ".githooks/")

# The crates the desktop app spawns, which it builds, with everything they name, before it runs.
DESKTOP = ("ui-bridge", "ui-files")

# The local gates, the ones taking seconds first. Cross-building has none, since the container
# gates cover what it checks short of linking every target.
ALWAYS = (
    "check-scripts", "check-spec", "check-security", "check-locales", "check-versions",
    "check-narration", "check-reviewdog",
)
HOST = {
    "rust": ("check",),
    "ui": ("check-ui",),
    "docs": ("check-docs",),
    "npm": ("check-npm",),
    "deps": ("check-deps",),
}
CONTAINERS = {"rust": ("check-msrv", "check-windows", "check-linux")}


def classify(path, desktop):
    """The areas a change to `path` could affect, where `desktop` is desktop_crates()."""
    if path in DECIDES or path.startswith(DECIDES_UNDER):
        return EVERYTHING
    if path in ("Cargo.toml", "Cargo.lock"):
        return frozenset({"rust", "ui", "build", "deps"})
    if path in ("deny.toml", "contrib/check-deny-reasons.py"):
        return frozenset({"deps"})
    if path == "Dockerfile.cross":
        return frozenset({"build"})
    parts = path.split("/")
    if parts[0] == "crates" and len(parts) > 2:
        found = {"rust"}
        if parts[1] in desktop:
            found.add("ui")
        if parts[2:] == ["Cargo.toml"]:
            found |= {"build", "deps"}
        elif parts[2:] == ["build.rs"]:
            found.add("build")
        return frozenset(found)
    if path in READ_BY_RUST:
        return frozenset({"rust"})
    if path in READ_BY_INSTALLER_TESTS:
        return READ_BY_INSTALLER_TESTS[path]
    if path.startswith("extension/"):
        # The job for the desktop UI runs the extension's tests, and a Rust test reads its files.
        return frozenset({"rust", "ui"})
    for under, area in (("docs/website/", "docs"), ("ui/", "ui")):
        if path.startswith(under):
            lockfile = path[len(under):] in ("package.json", "package-lock.json")
            return frozenset({area, "npm"} if lockfile else {area})
    if path in ("package.json", "package-lock.json") or path.startswith("npm/"):
        return frozenset({"npm"})
    if path in QUIET or path.startswith(QUIET_UNDER):
        return NOTHING
    return EVERYTHING


PACKAGE_NAME = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.M)


def names(text, package):
    """Whether a manifest depends on `package`, in any of the ways Cargo lets it say so.

    A dependency's key is its package name unless `package =` renames it, so the key, a table
    header ending in it, or the rename finds every form: inline, dotted, workspace-inherited, and
    under dev-, build- or target-specific dependencies. A match anywhere else only adds a crate.
    """
    name = re.escape(package)
    return re.search(
        rf'^\s*"?{name}"?\s*[=.]|^\s*\[[^\n]*\.{name}\]|package\s*=\s*"{name}"', text, re.M
    ) is not None


def desktop_crates(root):
    """The directories under crates/ the desktop app builds, by name."""
    manifests = {}
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        found = PACKAGE_NAME.search(text)
        manifests[manifest.parent.name] = (found.group(1) if found else None, text)
    reached, todo = set(), [crate for crate in DESKTOP if crate in manifests]
    while todo:
        crate = todo.pop()
        if crate in reached:
            continue
        reached.add(crate)
        text = manifests[crate][1]
        todo.extend(
            other for other, (package, _) in manifests.items()
            if package and other not in reached and names(text, package)
        )
    return reached


def git(root, *args, env=None):
    return subprocess.run(
        ["git", *args], cwd=root, check=True, capture_output=True, text=True, env=env
    ).stdout


def listed(output):
    return [path for path in output.split("\0") if path]


def changed(root, base, head=None, env=None):
    """Every path that differs, with a rename counted at both ends so a move out of crates/ is
    still a change to crates/."""
    return listed(git(root, "diff", "--no-renames", "--name-only", "-z", base,
                      *([head] if head else []), env=env))


def pull_request_paths(root, env=None):
    """What the pull request changes, or None where this checkout cannot say.

    A pull request is checked out as the commit merging it into its base, so its first parent is
    the base as it now is and the diff against that is the change as it would land.
    """
    if (env or os.environ).get("GITHUB_EVENT_NAME") != "pull_request":
        return None
    parents = git(root, "rev-list", "--parents", "-n", "1", "HEAD", env=env).split()
    if len(parents) != 3:
        return None
    return changed(root, parents[1], "HEAD", env=env)


def branch_paths(root, base, env=None):
    """What this branch changes against `base`: its commits, its uncommitted edits, and files
    git has not been told about yet."""
    fork = git(root, "merge-base", base, "HEAD", env=env).strip()
    untracked = listed(git(root, "ls-files", "--others", "--exclude-standard", "-z", env=env))
    return sorted(set(changed(root, fork, env=env)) | set(untracked))


def needed(paths, desktop):
    """Each area some path needs, with the first path that needs it."""
    because = {}
    for path in paths:
        for area in classify(path, desktop):
            because.setdefault(area, path)
    return because


def targets(areas, table):
    return [target for area in AREAS if area in areas for target in table.get(area, ())]


def explain(because, out):
    for area in AREAS:
        said = f"needed for {because[area]}" if area in because else "not needed"
        print(f"{area}: {said}", file=out)


def github_output(root, env=None):
    env = env or os.environ
    paths = pull_request_paths(root, env)
    if paths is None:
        because = {area: "any checkout that is not a pull request's merge" for area in AREAS}
    else:
        because = needed(paths, desktop_crates(root))
        print(f"{len(paths)} paths changed")
    explain(because, sys.stdout)
    # Written once, after everything is known, so a failure part way leaves no area false.
    with open(env["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        for area in AREAS:
            out.write(f"{area}={'true' if area in because else 'false'}\n")
    return 0


def default_base(root):
    """upstream/main in a fork's checkout, origin/main otherwise, as check-reviewdog.sh has it."""
    try:
        git(root, "rev-parse", "--verify", "--quiet", "upstream/main")
        return "upstream/main"
    except subprocess.CalledProcessError:
        return "origin/main"


def local(root, base, containers):
    base = base or default_base(root)
    try:
        paths = branch_paths(root, base)
    except subprocess.CalledProcessError as failed:
        print(f"affected-checks: cannot diff against {base}: {failed.stderr.strip()}",
              file=sys.stderr)
        return 1
    because = needed(paths, desktop_crates(root))
    print(f"affected-checks: {len(paths)} paths changed against {base}", file=sys.stderr)
    explain(because, sys.stderr)
    chosen = targets(because, CONTAINERS) if containers else list(ALWAYS) + targets(because, HOST)
    print(" ".join(chosen))
    return 0


FIXTURE_DESKTOP = frozenset({"core", "ui-bridge"})

CASES = (
    ("contrib/affected-checks.py", EVERYTHING),
    ("Makefile", EVERYTHING),
    (".github/workflows/ci.yml", EVERYTHING),
    (".github/actions/setup/action.yml", EVERYTHING),
    ("crates/README.md", EVERYTHING),
    ("somewhere-new/file.txt", EVERYTHING),
    ("Cargo.lock", {"rust", "ui", "build", "deps"}),
    ("Cargo.toml", {"rust", "ui", "build", "deps"}),
    ("deny.toml", {"deps"}),
    ("contrib/check-deny-reasons.py", {"deps"}),
    ("Dockerfile.cross", {"build"}),
    ("crates/tui/src/render.rs", {"rust"}),
    ("crates/core/src/policy.rs", {"rust", "ui"}),
    ("crates/tui/Cargo.toml", {"rust", "build", "deps"}),
    ("crates/config/build.rs", {"rust", "build"}),
    ("docs/specs/layering.md", {"rust"}),
    ("docs/specs/routing.md", NOTHING),
    ("docs/website/docs/intro.md", {"docs"}),
    ("docs/website/package-lock.json", {"docs", "npm"}),
    ("ui/src/App.tsx", {"ui"}),
    ("ui/package.json", {"ui", "npm"}),
    ("extension/tools.js", {"rust", "ui"}),
    ("extension/tests/tools.test.mjs", {"rust", "ui"}),
    ("npm/scripts/postinstall.js", {"npm", "rust"}),
    ("install.sh", {"rust"}),
    ("package-lock.json", {"npm"}),
    ("agents/skills/rebase/SKILL.md", NOTHING),
    ("contrib/check-locales.py", NOTHING),
    ("README.md", NOTHING),
)

# One crate reached each way a manifest can name another, one reached only through another, and
# one nothing names.
FIXTURE_MANIFESTS = {
    "ui-bridge": '[package]\nname = "x-bridge"\n\n[dependencies]\n'
                 'x-inline = { path = "../inline" }\nx-dotted.workspace = true\n'
                 'alias = { path = "../renamed", package = "x-renamed" }\n\n'
                 '[dev-dependencies.x-table]\npath = "../table"\n\n'
                 "[target.'cfg(windows)'.dependencies]\nx-target = { path = \"../target\" }\n",
    "ui-files": '[package]\nname = "x-files"\n',
    "inline": '[package]\nname = "x-inline"\n\n[dependencies]\nx-deep = { path = "../deep" }\n',
    "dotted": '[package]\nname = "x-dotted"\n',
    "renamed": '[package]\nname = "x-renamed"\n',
    "table": '[package]\nname = "x-table"\n',
    "target": '[package]\nname = "x-target"\n',
    "deep": '[package]\nname = "x-deep"\n',
    "apart": '[package]\nname = "x-apart"\n\n[dependencies]\nx-files = { path = "../ui-files" }\n',
}

STRING = re.compile(r'"([^"\\\n]+)"')
CONDITION = re.compile(r"needs\.changes\.outputs\.(\w+)(\s*!=\s*'false')?")


def named_by_rust(root):
    """Each tracked file outside crates/ that a string in a crate's Rust source names."""
    tracked = set(listed(git(root, "ls-files", "-z")))
    found = {}
    for source in sorted(tracked):
        if not (source.startswith("crates/") and source.endswith(".rs")):
            continue
        for literal in STRING.findall((root / source).read_text(encoding="utf-8")):
            path = re.sub(r"^(\.\.?/)+", "", literal)
            if path in tracked and not path.startswith("crates/"):
                found.setdefault(path, source)
    return found


def fixture_git(root):
    env = dict(
        os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1",
        GIT_AUTHOR_NAME="selftest", GIT_AUTHOR_EMAIL="selftest@example.invalid",
        GIT_COMMITTER_NAME="selftest", GIT_COMMITTER_EMAIL="selftest@example.invalid",
    )
    env.pop("GITHUB_EVENT_NAME", None)

    def run(*args):
        return git(root, *args, env=env)

    def write(path, text):
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(text, encoding="utf-8")

    return env, run, write


def selftest():
    """Prove each rule, the direction of every doubt, and the conditions reading the answer."""
    checks = []

    for path, wanted in CASES:
        got = classify(path, FIXTURE_DESKTOP)
        checks.append((f"{path} needs {sorted(wanted)}", got == frozenset(wanted), sorted(got)))

    real = desktop_crates(ROOT)
    for crate in (*DESKTOP, "core"):
        checks.append((f"the desktop app builds {crate}", crate in real, sorted(real)))

    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        for crate, text in FIXTURE_MANIFESTS.items():
            (root / "crates" / crate).mkdir(parents=True)
            (root / "crates" / crate / "Cargo.toml").write_text(text, encoding="utf-8")
        got = desktop_crates(root)
    wanted = set(FIXTURE_MANIFESTS) - {"apart"}
    checks.append(("every way a manifest names a crate reaches it", got == wanted, sorted(got)))

    # A test reading a file outside crates/ makes that file Rust's, and the one way to find such
    # a read without running anything is the name it reads by.
    named = named_by_rust(ROOT)
    for path, source in sorted(named.items()):
        if path not in NAMED_NOT_READ:
            got = classify(path, real)
            checks.append((f"{path}, named in {source}, needs rust", "rust" in got, sorted(got)))
    for path in sorted(set(NAMED_NOT_READ) - set(named)):
        checks.append((f"{path} is still named by a crate, as NAMED_NOT_READ says", False, None))

    with tempfile.TemporaryDirectory() as scratch, tempfile.TemporaryDirectory() as elsewhere:
        root = Path(scratch)
        env, run, write = fixture_git(root)
        run("init", "-q", "-b", "main")
        write("crates/tui/src/moved.rs", "fn moved() {}\n")
        write("docs/specs/routing.md", "old\n")
        run("add", ".")
        run("commit", "-q", "-m", "base")
        run("checkout", "-q", "-b", "prose")
        write("docs/specs/routing.md", "new\n")
        run("commit", "-q", "-am", "prose only")
        run("checkout", "-q", "-b", "move", "main")
        run("mv", "crates/tui/src/moved.rs", "docs/moved.rs")
        run("commit", "-q", "-m", "a move out of crates/")
        run("checkout", "-q", "main")
        write("README.md", "landed on the base after both branched\n")
        run("add", ".")
        run("commit", "-q", "-m", "base moves on")

        pull_request = dict(env, GITHUB_EVENT_NAME="pull_request")
        for branch, wanted in (("prose", NOTHING), ("move", {"rust"})):
            run("checkout", "-q", "--detach", "main")
            run("merge", "-q", "--no-ff", "-m", "merge", branch)
            got = frozenset(needed(pull_request_paths(root, pull_request), NOTHING))
            checks.append((f"a pull request of {branch} needs {sorted(wanted)}",
                           got == frozenset(wanted), sorted(got)))

        def written(event):
            out = Path(elsewhere) / "github-output"
            out.write_text("", encoding="utf-8")
            with contextlib.redirect_stdout(io.StringIO()):
                github_output(root, dict(env, GITHUB_EVENT_NAME=event, GITHUB_OUTPUT=str(out)))
            return out.read_text(encoding="utf-8").splitlines()

        got = written("pull_request")
        wanted = [f"{area}={'true' if area == 'rust' else 'false'}" for area in AREAS]
        checks.append(("the job writes each area once, false where the pull request does not "
                       "need it", got == wanted, got))
        got = written("push")
        wanted = [f"{area}=true" for area in AREAS]
        checks.append(("the job writes every area true for a push", got == wanted, got))

        push = pull_request_paths(root, dict(env, GITHUB_EVENT_NAME="push"))
        checks.append(("a push answers nothing, so needs everything", push is None, push))
        run("checkout", "-q", "prose")
        unmerged = pull_request_paths(root, pull_request)
        checks.append(("a checkout that is not a merge answers nothing", unmerged is None, unmerged))

        write("docs/specs/routing.md", "edited, not committed\n")
        write("ui/new.ts", "untracked\n")
        got = branch_paths(root, "main", env=env)
        wanted = ["docs/specs/routing.md", "ui/new.ts"]
        checks.append(("a branch is its commits, its edits and its new files, not the base's",
                       got == wanted, got))

    for area in set(AREAS) - {"build"}:
        runs = targets({area}, HOST) + targets({area}, CONTAINERS)
        checks.append((f"a local run needing {area} runs a gate for it", bool(runs), runs))
    containers = targets({"rust"}, CONTAINERS)
    checks.append(("a Rust change runs every container gate",
                   containers == ["check-msrv", "check-windows", "check-linux"], containers))
    makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
    recipes = set(re.findall(r"^([\w-]+):", makefile, re.M))
    for target in (*ALWAYS, *targets(EVERYTHING, HOST), *targets(EVERYTHING, CONTAINERS)):
        checks.append((f"the Makefile has {target}", target in recipes, None))

    # The workflows are where a wrong answer becomes a skipped check, so each condition has to
    # run its job on anything but "false", and on a run where the job deciding failed.
    read = set()
    for workflow in sorted((ROOT / ".github/workflows").glob("*.yml")):
        for number, line in enumerate(workflow.read_text(encoding="utf-8").splitlines(), 1):
            found = CONDITION.findall(line)
            if not found:
                continue
            where = f"{workflow.name}:{number}"
            checks.append((f"{where} runs its job when the decision failed",
                           "!cancelled()" in line, line.strip()))
            for area, negative in found:
                read.add(area)
                checks.append((f"{where} names an area there is", area in AREAS, area))
                checks.append((f"{where} skips only on 'false'", bool(negative), line.strip()))
    for area in AREAS:
        checks.append((f"a workflow reads {area}", area in read, sorted(read)))

    broke = [(claim, got) for claim, held, got in checks if not held]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Say which checks the paths a change touches need.")
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--github-output", action="store_true",
                      help="write each area as true or false to $GITHUB_OUTPUT, for ci.yml")
    mode.add_argument("--selftest", action="store_true", help="check the rules and the workflows")
    ap.add_argument("--base", help="the ref a local run is measured from "
                                   "(default: upstream/main, or origin/main without one)")
    ap.add_argument("--containers", action="store_true",
                    help="print the container gates a local run needs, not the host gates")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.github_output:
        return github_output(ROOT)
    return local(ROOT, args.base, args.containers)


if __name__ == "__main__":
    sys.exit(main())
