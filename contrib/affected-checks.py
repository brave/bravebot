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

AREAS = ("rust", "ui", "mobile", "build", "installers", "docs", "help", "npm", "deps", "fuzz")
EVERYTHING = frozenset(AREAS)
NOTHING = frozenset()

# What makes this decision: the workflows holding the conditions, the recipes the local run
# drives, and this file.
DECIDES = ("Makefile", "contrib/affected-checks.py")
DECIDES_UNDER = (".github/workflows/",)

# Files the guard of the cli crate's tests that reads every build command reads and pins a command
# in: it counts the lines that ask for every feature in each, so a change to one can change what
# that test finds whatever else the change is. They add the Rust checks to what their own area
# needs.
READ_BY_THE_SEAM_GUARD = frozenset({
    "contrib/envrc.example",
    ".githooks/pre-commit",
    "contrib/Dockerfile.cross",
    "agents/pr_worktree.py",
    "agents/pr_worktree_selftest.py",
    "agents/skills/maintenance/maintenance.py",
    ".config/deny.toml",
    "docs/development/fuzzing.md",
    "docs/development/releasing.md",
    "docs/website/docs/development.md",
    "docs/website/package.json",
    "fuzz/Cargo.lock",
    "fuzz/Cargo.toml",
    "mobile/package.json",
    "package.json",
    "packages/agent-client/README.md",
    "packages/agent-client/package.json",
    "ui/README.md",
    "ui/docs/development.md",
    "ui/docs/testing.md",
    "ui/docs/v0.9.0-implementation.md",
    "ui/package.json",
    "ui/pnpm-lock.yaml",
    "ui/scripts/build-bridge.sh",
    "ui/src/renderer/App.tsx",
})

# Files that belong to another area and are also read by a test in the cli crate. They add the
# Rust checks to what their own area needs.
ALSO_READ_BY_RUST = frozenset({
    "docs/website/docs/install.md",
})

# Outside crates/, and read by a test all the same.
READ_BY_RUST = ("CHANGELOG.md", "README.md")

# Read by a test, and by the build of the agent crate, which carries each spec's clause headings.
READ_BY_RUST_UNDER = ("docs/specs/",)

# Part of the desktop UI, and compared by a Rust test with what it generates from the catalogs.
UI_READ_BY_RUST = ("ui/src/renderer/i18n/messages.gen.ts",)

# Outside crates/, run by the installer tests themselves rather than read as data: a change to
# one is a change to what those tests exercise, whatever else reads it.
READ_BY_INSTALLER_TESTS = {
    "install.sh": frozenset({"rust"}),
    "install.ps1": frozenset({"rust"}),
    "npm/scripts/stage-platform-package.js": frozenset({"npm", "rust"}),
}

# The scripts that build, fuse and check the Windows installers. CI runs them only in the jobs
# after the cross-build, so a change to one needs the cross-build too, or the job that installs
# the result is skipped and reports success. Nothing else on a pull request needs the installers:
# a change to the lockfile, a manifest or a build script runs the cross-build, which links every
# target, and is packaged and installed on Windows once it lands, since main and every tag build
# and install them whatever changed.
WINDOWS_INSTALLER_SCRIPTS = (
    "ui/scripts/windows-installer.mjs", "ui/scripts/check-windows-install.mjs",
    "ui/scripts/package.mjs", "ui/scripts/fuses.mjs",
)

# Named by a crate's source without being read from this tree, which the selftest's scan cannot
# tell from a read by itself.
NAMED_NOT_READ = {
    ".gitignore": "the name git reads in any repository, and a fixture's in the git tests",
    "agents/AGENTS.md": "a marker doctor checks exists, which its tests write for themselves",
    "agents/setup.py": "a marker doctor checks exists, which its tests write for themselves",
    "docs/development/agent-configuration.md": "a marker doctor checks exists, likewise",
}

# What a project directory holds for the package manager. The npm names stay so that deleting a
# leftover lockfile still counts as a change to the install.
PACKAGE_FILES = (
    "package.json", "package-lock.json", "pnpm-lock.yaml", "pnpm-workspace.yaml",
)

# Read by nothing heavier than the checks that always run.
QUIET = (
    "LICENSE", ".github/SECURITY.md", ".gitignore",
    ".github/CODEOWNERS", ".github/renovate.json",
)
QUIET_UNDER = ("docs/", "agents/", "contrib/", ".githooks/")

# The crates the desktop app spawns, which it builds, with everything they name, before it runs.
DESKTOP = ("ui-bridge", "ui-files")

# The package that fuzzes the crates its manifest names, and everything those name. It is a
# workspace of its own, so no rule below reaches it through crates/.
FUZZ_MANIFEST = "fuzz/Cargo.toml"

# The local gates, the ones taking seconds first. Cross-building has none, since the container
# gates cover what it checks short of linking every target. The fuzz targets have none either:
# they need a nightly toolchain and cargo-fuzz, so `make check-fuzz` is run by name.
ALWAYS = (
    "check-scripts", "check-spec", "check-security", "check-locales", "check-versions",
    "check-narration", "check-reviewdog",
)
HOST = {
    "rust": ("check",),
    "ui": ("check-ui", "check-agent-client"),
    "mobile": ("check-mobile",),
    "docs": ("check-docs",),
    "help": ("check-help",),
    "npm": ("check-npm",),
    "deps": ("check-deps",),
}
CONTAINERS = {"rust": ("check-msrv", "check-windows", "check-linux")}


def classify(path, desktop, fuzz):
    """The areas a change to `path` could affect, where `desktop` is desktop_crates() and `fuzz`
    is fuzz_crates()."""
    found = classify_by_area(path, desktop, fuzz)
    if path in READ_BY_THE_SEAM_GUARD or path in ALSO_READ_BY_RUST:
        found = found | {"rust"}
    return found


def classify_by_area(path, desktop, fuzz):
    """The areas `path` belongs to by what it is, before a test that reads it is counted."""
    if path in DECIDES or path.startswith(DECIDES_UNDER):
        return EVERYTHING
    if path == "Cargo.toml":
        # Every crate the fuzz package builds takes its version and dependencies from here, so
        # its lockfile goes stale with this one. Cargo.lock below is not read by that build.
        return frozenset({"rust", "ui", "build", "deps", "fuzz"})
    if path == "Cargo.lock":
        return frozenset({"rust", "ui", "build", "deps"})
    if path in (".config/deny.toml", "contrib/check-deny-reasons.py"):
        return frozenset({"deps"})
    if path == "contrib/check-pnpm-lockfiles.py":
        return frozenset({"npm"})
    if path == "contrib/check-lint-increase.py":
        return frozenset({"rust"})
    if path == "contrib/Dockerfile.cross":
        return frozenset({"build"})
    parts = path.split("/")
    if parts[0] == "crates" and len(parts) > 2:
        found = {"rust"}
        if parts[1] in desktop:
            found.add("ui")
        if parts[1] in fuzz:
            found.add("fuzz")
        if parts[2:] == ["Cargo.toml"]:
            found |= {"build", "deps"}
        elif parts[2:] == ["build.rs"]:
            found.add("build")
        return frozenset(found)
    if path.startswith("fuzz/"):
        # Nothing in the root workspace reads it, so the Rust jobs have nothing to check. Its
        # manifest and lockfile are what the dependency policy reads of it.
        if path in ("fuzz/Cargo.toml", "fuzz/Cargo.lock"):
            return frozenset({"fuzz", "deps"})
        return frozenset({"fuzz"})
    if path in READ_BY_RUST or path.startswith(READ_BY_RUST_UNDER):
        return frozenset({"rust"})
    if path in READ_BY_INSTALLER_TESTS:
        return READ_BY_INSTALLER_TESTS[path]
    if path in WINDOWS_INSTALLER_SCRIPTS:
        return frozenset({"ui", "build", "installers"})
    if path.startswith("extension/"):
        # The job for the desktop UI runs the extension's tests, and a Rust test reads its files.
        return frozenset({"rust", "ui"})
    if path.startswith("packages/agent-client/"):
        # The client's tests run in the job for the desktop UI against the built bridge, and the
        # mobile app links the client and builds it first. Only the wire contract file is also
        # read by a Rust test.
        name = path[len("packages/agent-client/"):]
        found = {"ui", "mobile"}
        if name == "test-fixtures/wire-contract.json":
            found.add("rust")
        if name in PACKAGE_FILES:
            found.add("npm")
        return frozenset(found)
    for under, area in (("docs/website/", "docs"), ("ui/", "ui"), ("mobile/", "mobile")):
        if path.startswith(under):
            lockfile = path[len(under):] in PACKAGE_FILES
            found = {area, "npm"} if lockfile else {area}
            if path in UI_READ_BY_RUST:
                found.add("rust")
            # help.rs compiles each page under docs/website/docs into a fixed table, builds an
            # index naming every page's title from its front matter, and loads a page by the
            # name that index gives. An add, an edit that breaks a page's front matter, or a
            # remove can each fail a test that reads one of those, whatever the path's status.
            if area == "docs" and path.startswith("docs/website/docs/"):
                found |= {"help"}
            return frozenset(found)
    if path in PACKAGE_FILES or path.startswith("npm/"):
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


def crate_manifests(root):
    """Each crate's package name and manifest text, by its directory under crates/."""
    manifests = {}
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8")
        found = PACKAGE_NAME.search(text)
        manifests[manifest.parent.name] = (found.group(1) if found else None, text)
    return manifests


def reaching(manifests, start):
    """The crates in `start` and every crate any of them names, by directory."""
    reached, todo = set(), list(start)
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


def desktop_crates(root):
    """The directories under crates/ the desktop app builds, by name."""
    manifests = crate_manifests(root)
    return reaching(manifests, [crate for crate in DESKTOP if crate in manifests])


def fuzz_crates(root):
    """The directories under crates/ the fuzz package builds, by name: the crates its manifest
    names, with everything they name."""
    try:
        text = (root / FUZZ_MANIFEST).read_text(encoding="utf-8")
    except FileNotFoundError:
        return set()
    manifests = crate_manifests(root)
    return reaching(
        manifests,
        [crate for crate, (package, _) in manifests.items() if package and names(text, package)],
    )


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


def needed(paths, desktop, fuzz):
    """Each area some path needs, with the first path that needs it."""
    because = {}
    for path in paths:
        for area in classify(path, desktop, fuzz):
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
        because = needed(paths, desktop_crates(root), fuzz_crates(root))
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
    because = needed(paths, desktop_crates(root), fuzz_crates(root))
    print(f"affected-checks: {len(paths)} paths changed against {base}", file=sys.stderr)
    explain(because, sys.stderr)
    if "fuzz" in because and not containers:
        print("affected-checks: the fuzz job is not run here: make check-fuzz runs it, and needs the "
              "nightly toolchain and cargo-fuzz", file=sys.stderr)
    chosen = targets(because, CONTAINERS) if containers else list(ALWAYS) + targets(because, HOST)
    print(" ".join(chosen))
    return 0


FIXTURE_DESKTOP = frozenset({"core", "ui-bridge"})
FIXTURE_FUZZ = frozenset({"agent", "core"})

CASES = (
    ("contrib/affected-checks.py", EVERYTHING),
    ("Makefile", EVERYTHING),
    (".github/workflows/ci.yml", EVERYTHING),
    (".github/actions/setup/action.yml", EVERYTHING),
    ("crates/README.md", EVERYTHING),
    ("somewhere-new/file.txt", EVERYTHING),
    ("Cargo.lock", {"rust", "ui", "build", "deps"}),
    ("Cargo.toml", {"rust", "ui", "build", "deps", "fuzz"}),
    (".config/deny.toml", {"deps", "rust"}),
    ("contrib/check-deny-reasons.py", {"deps"}),
    ("contrib/Dockerfile.cross", {"build", "rust"}),
    ("crates/tui/src/render.rs", {"rust"}),
    ("crates/core/src/policy.rs", {"rust", "ui", "fuzz"}),
    ("crates/agent/src/regex.rs", {"rust", "fuzz"}),
    ("crates/agent/Cargo.toml", {"rust", "build", "deps", "fuzz"}),
    ("crates/tui/Cargo.toml", {"rust", "build", "deps"}),
    ("fuzz/src/lib.rs", {"fuzz"}),
    ("fuzz/fuzz_targets/regex.rs", {"fuzz"}),
    ("fuzz/seeds/regex/0a1b", {"fuzz"}),
    ("fuzz/dictionaries/regex.dict", {"fuzz"}),
    ("fuzz/Cargo.toml", {"fuzz", "deps", "rust"}),
    ("fuzz/Cargo.lock", {"fuzz", "deps", "rust"}),
    ("docs/development/fuzzing.md", {"rust"}),
    ("crates/config/build.rs", {"rust", "build"}),
    ("docs/specs/layering.md", {"rust"}),
    ("docs/specs/credential-protection.md", {"rust"}),
    ("docs/specs/sandboxing.md", {"rust"}),
    ("CHANGELOG.md", {"rust"}),
    ("docs/specs/routing.md", {"rust"}),
    ("docs/specs/tools/read-file.md", {"rust"}),
    ("docs/website/docs/install.md", {"docs", "help", "rust"}),
    ("docs/website/docs/intro.md", {"docs", "help"}),
    ("docs/website/docs/using/watches.md", {"docs", "help"}),
    ("docs/website/package-lock.json", {"docs", "npm"}),
    ("docs/website/pnpm-lock.yaml", {"docs", "npm"}),
    ("docs/website/pnpm-workspace.yaml", {"docs", "npm"}),
    ("ui/src/App.tsx", {"ui"}),
    ("ui/package.json", {"ui", "npm", "rust"}),
    ("ui/pnpm-lock.yaml", {"ui", "npm", "rust"}),
    ("ui/pnpm-workspace.yaml", {"ui", "npm"}),
    ("ui/scripts/windows-installer.mjs", {"ui", "build", "installers"}),
    ("ui/scripts/check-windows-install.mjs", {"ui", "build", "installers"}),
    ("ui/scripts/package.mjs", {"ui", "build", "installers"}),
    ("ui/scripts/fuses.mjs", {"ui", "build", "installers"}),
    ("ui/scripts/windows-installer.test.mjs", {"ui"}),
    ("ui/src/renderer/i18n/messages.gen.ts", {"rust", "ui"}),
    ("packages/agent-client/src/common/client.ts", {"ui", "mobile"}),
    ("packages/agent-client/test-fixtures/wire-contract.json", {"rust", "ui", "mobile"}),
    ("packages/agent-client/package-lock.json", {"ui", "mobile", "npm"}),
    ("packages/agent-client/pnpm-lock.yaml", {"ui", "mobile", "npm"}),
    ("packages/agent-client/pnpm-workspace.yaml", {"ui", "mobile", "npm"}),
    ("mobile/src/App.tsx", {"mobile"}),
    ("mobile/ios/Bravebot.xcodeproj/project.pbxproj", {"mobile"}),
    ("mobile/package.json", {"mobile", "npm", "rust"}),
    ("mobile/pnpm-lock.yaml", {"mobile", "npm"}),
    ("mobile/pnpm-workspace.yaml", {"mobile", "npm"}),
    ("contrib/check-pnpm-lockfiles.py", {"npm"}),
    ("contrib/check-lint-increase.py", {"rust"}),
    ("extension/tools.js", {"rust", "ui"}),
    ("extension/tests/tools.test.mjs", {"rust", "ui"}),
    ("npm/scripts/stage-platform-package.js", {"npm", "rust"}),
    ("install.sh", {"rust"}),
    ("install.ps1", {"rust"}),
    ("package-lock.json", {"npm"}),
    ("pnpm-lock.yaml", {"npm"}),
    ("pnpm-workspace.yaml", {"npm"}),
    ("agents/skills/rebase/SKILL.md", NOTHING),
    ("agents/skills/security-audit/security-audit.py", NOTHING),
    ("crates/agent/src/answers.rs", {"rust", "fuzz"}),
    ("crates/agent/src/kept_answers.rs", {"rust", "fuzz"}),
    ("docs/specs/prompting.md", {"rust"}),
    ("contrib/check-locales.py", NOTHING),
    ("README.md", {"rust"}),
    (".github/SECURITY.md", NOTHING),
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

# The fuzz package names two crates, one of which names a third, and has a target called after a
# crate it does not depend on.
FIXTURE_FUZZ_MANIFEST = (
    '[package]\nname = "x-fuzz"\n\n[dependencies]\n'
    'x-inline = { path = "../crates/inline" }\nx-dotted.workspace = true\n\n'
    '[[bin]]\nname = "x-apart"\npath = "fuzz_targets/apart.rs"\n'
)

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
        got = classify(path, FIXTURE_DESKTOP, FIXTURE_FUZZ)
        checks.append((f"{path} needs {sorted(wanted)}", got == frozenset(wanted), sorted(got)))

    real = desktop_crates(ROOT)
    real_fuzz = fuzz_crates(ROOT)
    for crate in (*DESKTOP, "core"):
        checks.append((f"the desktop app builds {crate}", crate in real, sorted(real)))
    for crate in ("agent", "aichat", "bedrock", "core"):
        checks.append((f"the fuzz package builds {crate}", crate in real_fuzz, sorted(real_fuzz)))

    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        for crate, text in FIXTURE_MANIFESTS.items():
            (root / "crates" / crate).mkdir(parents=True)
            (root / "crates" / crate / "Cargo.toml").write_text(text, encoding="utf-8")
        got = desktop_crates(root)
        without = fuzz_crates(root)
        (root / "fuzz").mkdir()
        (root / "fuzz" / "Cargo.toml").write_text(FIXTURE_FUZZ_MANIFEST, encoding="utf-8")
        fuzzed = fuzz_crates(root)
    wanted = set(FIXTURE_MANIFESTS) - {"apart"}
    checks.append(("every way a manifest names a crate reaches it", got == wanted, sorted(got)))
    checks.append(("no fuzz package reaches no crate", without == set(), sorted(without)))
    checks.append(("the fuzz package reaches what its manifest names and what those name, and "
                   "not a crate only a target is called after",
                   fuzzed == {"inline", "deep", "dotted"}, sorted(fuzzed)))

    # A test reading a file outside crates/ makes that file Rust's, and the one way to find such
    # a read without running anything is the name it reads by.
    named = named_by_rust(ROOT)
    for path, source in sorted(named.items()):
        if path not in NAMED_NOT_READ:
            got = classify(path, real, real_fuzz)
            checks.append((f"{path}, named in {source}, needs rust", "rust" in got, sorted(got)))
    for path in sorted(set(NAMED_NOT_READ) - set(named)):
        checks.append((f"{path} is still named by a crate, as NAMED_NOT_READ says", False, None))

    with tempfile.TemporaryDirectory() as scratch, tempfile.TemporaryDirectory() as elsewhere:
        root = Path(scratch)
        env, run, write = fixture_git(root)
        run("init", "-q", "-b", "main")
        write("crates/tui/src/moved.rs", "fn moved() {}\n")
        write("docs/development/checks.md", "old\n")
        write("docs/website/docs/using/existing.md", "old\n")
        run("add", ".")
        run("commit", "-q", "-m", "base")
        run("checkout", "-q", "-b", "prose")
        write("docs/development/checks.md", "new\n")
        run("commit", "-q", "-am", "prose only")
        run("checkout", "-q", "-b", "docs-add", "main")
        write("docs/website/docs/using/new.md", "new\n")
        run("add", ".")
        run("commit", "-q", "-m", "a new page")
        run("checkout", "-q", "-b", "docs-edit", "main")
        write("docs/website/docs/using/existing.md", "edited\n")
        run("commit", "-q", "-am", "edited a page")
        run("checkout", "-q", "-b", "docs-remove", "main")
        run("rm", "-q", "docs/website/docs/using/existing.md")
        run("commit", "-q", "-m", "removed a page")
        run("checkout", "-q", "-b", "move", "main")
        run("mv", "crates/tui/src/moved.rs", "docs/moved.rs")
        run("commit", "-q", "-m", "a move out of crates/")
        run("checkout", "-q", "main")
        write("README.md", "landed on the base after both branched\n")
        run("add", ".")
        run("commit", "-q", "-m", "base moves on")

        pull_request = dict(env, GITHUB_EVENT_NAME="pull_request")
        # A page added, edited or removed under docs/website/docs between docs-add/-edit/-remove
        # here needs help whatever its status, with "move" kept last so the assertions below it
        # still read the merge they name.
        for branch, wanted in (
            ("prose", NOTHING),
            ("docs-add", {"docs", "help"}),
            ("docs-edit", {"docs", "help"}),
            ("docs-remove", {"docs", "help"}),
            ("move", {"rust"}),
        ):
            run("checkout", "-q", "--detach", "main")
            run("merge", "-q", "--no-ff", "-m", "merge", branch)
            got = frozenset(needed(pull_request_paths(root, pull_request), NOTHING, NOTHING))
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

        write("docs/development/checks.md", "edited, not committed\n")
        write("ui/new.ts", "untracked\n")
        got = branch_paths(root, "main", env=env)
        wanted = ["docs/development/checks.md", "ui/new.ts"]
        checks.append(("a branch is its commits, its edits and its new files, not the base's",
                       got == wanted, got))

    # `check-security` is selected by no area: it is in ALWAYS, which a local run prints whatever
    # the paths need, including the paths the security audit's Stop hook watches and a change that
    # needs nothing else (the audit script itself is under agents/).
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        env, run, write = fixture_git(root)
        run("init", "-q", "-b", "main")
        write("README.md", "base\n")
        run("add", ".")
        run("commit", "-q", "-m", "base")
        for path in (
            "docs/specs/prompting.md",
            "crates/agent/src/answers.rs",
            "crates/agent/src/kept_answers.rs",
            "agents/skills/security-audit/security-audit.py",
            "agents/skills/rebase/SKILL.md",
        ):
            run("checkout", "-q", "-B", "branch", "main")
            write(path, "changed\n")
            run("add", ".")
            run("commit", "-q", "-m", "change")
            printed = io.StringIO()
            with contextlib.redirect_stdout(printed), contextlib.redirect_stderr(io.StringIO()):
                status = local(root, "main", False)
            gates = printed.getvalue().split()
            checks.append((f"a local run for a change to {path} runs check-security",
                           status == 0 and "check-security" in gates, gates))

    for area in set(AREAS) - {"build", "installers", "fuzz"}:
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
