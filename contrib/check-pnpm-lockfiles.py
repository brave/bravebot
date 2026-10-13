#!/usr/bin/env python3
"""Lints the pnpm lockfiles of the three JavaScript projects that are not the Rust workspace.

A lockfile is the whole of what an install fetches, so a hand-edited or tool-edited entry that
points a package at a different host, a local directory or a git repository changes what runs on
a developer's machine and in CI without touching package.json. lockfile-lint did this for the
npm lockfiles and cannot read pnpm's, so this holds the same rules for pnpm-lock.yaml:

- every registry package resolves by an sha512 integrity hash and nothing else, which leaves the
  registry as the only source (pnpm records no URL for it);
- the only other source is a GitHub tarball of a repository in ALLOWED_GIT_REPOS at the commit
  the project's own package.json names, so a bump of that dependency is a change to package.json
  and the lockfile together and nothing else needs editing;
- no entry resolves to a directory or a link, and no version is a file, link, workspace or URL
  reference;
- the file has packages, so a lockfile that lost its entries fails instead of passing empty.

The file is machine-written and regular, so it is read line by line. Standard library only, like
the other checks CI runs without a toolchain.
"""

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LOCKFILES = (
    "ui/pnpm-lock.yaml",
    "docs/website/pnpm-lock.yaml",
    "packages/agent-client/pnpm-lock.yaml",
)

ALLOWED_GIT_REPOS = ("brave/leo",)

INTEGRITY = re.compile(r"sha512-[A-Za-z0-9+/]{86}==")
REGISTRY_RESOLUTION = re.compile(r"\{integrity: (" + INTEGRITY.pattern + r")\}")
GIT_RESOLUTION = re.compile(
    r"\{gitHosted: true, integrity: (" + INTEGRITY.pattern + r"), tarball: (\S+)\}"
)
SEMVER = r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.+-]+)?"
REGISTRY_KEY = re.compile(r"^(?:@[^/@]+/)?[^/@]+@" + SEMVER + r"$")
GIT_KEY = re.compile(r"^((?:@[^/@]+/)?[^/@]+)@(https://codeload\.github\.com/([^/]+/[^/]+)/tar\.gz/([0-9a-f]{40}))$")
GIT_SPECIFIER = re.compile(r"^github:([^#/]+/[^#]+)#([0-9a-f]{40})$")
FOREIGN_VERSION = re.compile(r"^(?:link|file|workspace|portal|http|https|git|git\+[a-z]+|github):")


def unquote(text):
    return re.sub(r"""^(['"])(.*)\1$""", r"\2", text)


def sections(text):
    """The top-level sections of the file as {name: [lines]}."""
    found = {}
    name = None
    for line in text.splitlines():
        if line and not line.startswith((" ", "#")):
            name = line.split(":", 1)[0]
            found[name] = []
        elif name is not None:
            found[name].append(line)
    return found


def entries(lines):
    """The two-space-indented keys of a section, each with the lines under it."""
    found = {}
    key = None
    for line in lines:
        match = re.match(r"^  (\S.*):(?: .*)?$", line)
        if match and not line.startswith("   "):
            key = unquote(match.group(1))
            found[key] = []
        elif key is not None:
            found[key].append(line)
    return found


def allowed_git_sources(manifest):
    """The GitHub tarball URLs the manifest's own git dependencies resolve to."""
    urls = set()
    names = {**manifest.get("dependencies", {}), **manifest.get("devDependencies", {})}
    for specifier in names.values():
        match = GIT_SPECIFIER.match(specifier)
        if match and match.group(1) in ALLOWED_GIT_REPOS:
            urls.add(f"https://codeload.github.com/{match.group(1)}/tar.gz/{match.group(2)}")
    return urls


def lint(text, manifest):
    """Every rule the lockfile text breaks, as a list of messages."""
    problems = []
    top = sections(text)
    if not re.search(r"^lockfileVersion: '9\.0'$", text, re.MULTILINE):
        problems.append("lockfileVersion is not '9.0'; this lint reads that format only")
    packages = entries(top.get("packages", []))
    if not packages:
        problems.append("no entries under packages:")
    git_sources = allowed_git_sources(manifest)
    for key, body in packages.items():
        resolutions = [line.strip() for line in body if line.startswith("    resolution:")]
        if len(resolutions) != 1:
            problems.append(f"{key}: expected one resolution line, found {len(resolutions)}")
            continue
        resolution = resolutions[0][len("resolution:"):].strip()
        git_key = GIT_KEY.match(key)
        if git_key:
            url = git_key.group(2)
            found = GIT_RESOLUTION.fullmatch(resolution)
            if url not in git_sources:
                problems.append(f"{key}: {url} is not the git source of a dependency in package.json")
            elif not found or found.group(2) != url:
                problems.append(f"{key}: resolution is not the gitHosted tarball {url} with an integrity")
        elif REGISTRY_KEY.match(key):
            if not REGISTRY_RESOLUTION.fullmatch(resolution):
                problems.append(f"{key}: resolution is not an sha512 integrity alone: {resolution}")
        else:
            problems.append(f"{key}: not a registry version or an allowed git source")
    for line in top.get("importers", []):
        match = re.match(r"^\s+version: (\S+)$", line)
        if match and FOREIGN_VERSION.match(match.group(1)):
            if not any(match.group(1).startswith(url) for url in git_sources):
                problems.append(f"importers: version {match.group(1)} is not a registry version")
    for line in top.get("snapshots", []):
        match = re.match(r"^      \S.*: (.+)$", line)
        if match and FOREIGN_VERSION.match(unquote(match.group(1))):
            if not any(unquote(match.group(1)).startswith(url) for url in git_sources):
                problems.append(f"snapshots: dependency on {match.group(1)} is not a registry version")
    return problems


def lint_file(path):
    manifest = json.loads((path.parent / "package.json").read_text())
    return lint(path.read_text(), manifest)


def main(paths):
    failed = False
    for relative in paths:
        path = ROOT / relative
        if not path.is_file():
            print(f"{relative}: missing")
            failed = True
            continue
        problems = lint_file(path)
        for problem in problems:
            print(f"{relative}: {problem}")
        failed = failed or bool(problems)
        if not problems:
            print(f"{relative}: ok")
    return 1 if failed else 0


SHA = "a" * 40
INTEGRITY_A = "sha512-" + "A" * 86 + "=="
LEO_URL = f"https://codeload.github.com/brave/leo/tar.gz/{SHA}"
MANIFEST = {"dependencies": {"react": "^19.0.0", "@brave/leo": f"github:brave/leo#{SHA}"}}

CLEAN = f"""lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      '@brave/leo':
        specifier: github:brave/leo#{SHA}
        version: {LEO_URL}(react@19.1.0)
      react:
        specifier: ^19.0.0
        version: 19.1.0

packages:

  '@brave/leo@{LEO_URL}':
    resolution: {{gitHosted: true, integrity: {INTEGRITY_A}, tarball: {LEO_URL}}}
    version: 0.0.1

  react@19.1.0:
    resolution: {{integrity: {INTEGRITY_A}}}

snapshots:

  '@brave/leo@{LEO_URL}(react@19.1.0)':
    dependencies:
      react: 19.1.0

  react@19.1.0: {{}}
"""


def selftest():
    failures = []

    def expect(name, text, manifest, needle):
        problems = lint(text, manifest)
        joined = "\n".join(problems)
        if needle is None and problems:
            failures.append(f"{name}: expected a clean lockfile, got {joined}")
        elif needle is not None and needle not in joined:
            failures.append(f"{name}: expected a problem naming {needle!r}, got {joined or 'none'}")

    expect("clean lockfile", CLEAN, MANIFEST, None)
    expect(
        "registry package from another host",
        CLEAN.replace(
            f"resolution: {{integrity: {INTEGRITY_A}}}",
            f"resolution: {{integrity: {INTEGRITY_A}, tarball: https://evil.example/react.tgz}}",
        ),
        MANIFEST,
        "react@19.1.0",
    )
    expect(
        "registry package without integrity",
        CLEAN.replace(f"resolution: {{integrity: {INTEGRITY_A}}}", "resolution: {}"),
        MANIFEST,
        "react@19.1.0",
    )
    expect(
        "registry package with a weak hash",
        CLEAN.replace(f"resolution: {{integrity: {INTEGRITY_A}}}", "resolution: {integrity: sha1-AAAA}"),
        MANIFEST,
        "react@19.1.0",
    )
    expect(
        "directory resolution",
        CLEAN.replace(
            f"resolution: {{integrity: {INTEGRITY_A}}}", "resolution: {directory: ../react, type: directory}"
        ),
        MANIFEST,
        "react@19.1.0",
    )
    expect(
        "git source the manifest does not name",
        CLEAN,
        {"dependencies": {"react": "^19.0.0", "@brave/leo": "github:brave/leo#" + "b" * 40}},
        "is not the git source",
    )
    expect(
        "git source from a repository outside the allow-list",
        CLEAN.replace("brave/leo", "someone/leo"),
        {"dependencies": {"@brave/leo": f"github:someone/leo#{SHA}"}},
        "is not the git source",
    )
    expect(
        "git source whose tarball differs from its key",
        CLEAN.replace(f"tarball: {LEO_URL}", "tarball: https://evil.example/leo.tgz"),
        MANIFEST,
        "gitHosted tarball",
    )
    expect(
        "package keyed by a URL",
        CLEAN.replace("  react@19.1.0:\n    resolution", "  react@https://evil.example/react.tgz:\n    resolution"),
        MANIFEST,
        "not a registry version",
    )
    expect(
        "link version in an importer",
        CLEAN.replace("version: 19.1.0\n\npackages", "version: link:../react\n\npackages"),
        MANIFEST,
        "importers: version link:../react",
    )
    expect(
        "file dependency in a snapshot",
        CLEAN.replace("      react: 19.1.0\n", "      react: file:../react\n"),
        MANIFEST,
        "snapshots: dependency on file:../react",
    )
    expect(
        "other lockfile format",
        CLEAN.replace("lockfileVersion: '9.0'", "lockfileVersion: '10.0'"),
        MANIFEST,
        "lockfileVersion",
    )
    expect(
        "lockfile with no packages",
        "lockfileVersion: '9.0'\n\nimporters:\n\n  .: {}\n",
        {},
        "no entries",
    )

    with tempfile.TemporaryDirectory() as tmp:
        project = Path(tmp)
        (project / "package.json").write_text(json.dumps(MANIFEST))
        (project / "pnpm-lock.yaml").write_text(CLEAN)
        if lint_file(project / "pnpm-lock.yaml"):
            failures.append("lint_file: a clean project reported problems")
        (project / "package.json").write_text(json.dumps({"dependencies": {"react": "^19.0.0"}}))
        if not lint_file(project / "pnpm-lock.yaml"):
            failures.append("lint_file: a lockfile with a git source the manifest dropped passed")

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("check-pnpm-lockfiles selftest: ok")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--selftest", action="store_true", help="run the built-in cases and exit")
    parser.add_argument("paths", nargs="*", help="lockfiles to lint, relative to the repository root")
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main(args.paths or LOCKFILES))
