#!/usr/bin/env python3
"""Holds every file that states this repository's version to the one in the workspace manifest.

RELEASE-1 says one version names a release and every file that repeats it agrees, and until now
the only thing holding that was a refusal in the tagging path comparing two files. The front end
under ui/ arrived from another repository at 0.1.0 while this workspace was at 0.9.0, and nothing
anywhere said so: the packaged application takes its bundle version from ui/package.json, so the
disagreement is not cosmetic, it is an app whose About window and installer metadata name a
version that was never released.

A refusal at tagging time reports that on release day, which is the shape of deferred discovery
folding the front end in was meant to end. This reports it in the pull request that caused it,
which is why CI runs it and why `make bump-version` runs it before committing what it wrote: a
bump that misses one of these files fails instead of being found later.

What is in scope is the version of the thing people install: the workspace, the published npm
wrapper, and the desktop application. docs/website is a documentation site whose package version
names nothing anybody installs, so it is deliberately not here.

Standard library only, like the other checks CI runs without a toolchain.
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A manifest states the version once. A lockfile states it twice, in its own header and in the
# entry for the package it locks, and npm rewrites both, so a hand edit that moves one is exactly
# the mistake this has to catch.
MANIFESTS = ("package.json", "ui/package.json")
LOCKFILES = ("package-lock.json", "ui/package-lock.json")

VERSION_LINE = re.compile(r'version\s*=\s*"([^"]+)"')


def workspace_version(text):
    """The version in [workspace.package], which every other file repeats.

    Read by section rather than by the first match: a crate manifest inlined into this file, or
    the rust-version above it, would otherwise decide what a release is called.
    """
    section = None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            section = stripped
        elif section == "[workspace.package]":
            found = VERSION_LINE.match(stripped)
            if found:
                return found.group(1)
    return None


def stated(root):
    """Every place a version is written down, as (where, version), with None for unreadable.

    None is carried rather than raised so one missing file does not hide the rest: the answer
    wanted is every disagreement at once, not the first.
    """
    found = []
    for name in MANIFESTS:
        data = read_json(root / name)
        found.append((name, pick(data, "version")))
    for name in LOCKFILES:
        data = read_json(root / name)
        found.append((name, pick(data, "version")))
        locked = pick(data, "packages")
        found.append((f'{name} packages[""]', pick(pick(locked, ""), "version")))
    return found


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def pick(data, key):
    return data.get(key) if isinstance(data, dict) else None


def serialise(data):
    """The form npm writes a manifest or a lockfile in: two spaces and a trailing newline."""
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def write(root, version):
    """Set the version in every JSON file that states it, and change nothing else.

    The cargo half of a bump is cargo's: the workspace manifest is rewritten before this is
    called and `cargo update` follows it through to the lockfile. What is here is the four JSON
    files, in one place with the check that reads them, because they were rewritten by a node
    program embedded in a make recipe that could not run: a recipe's line continuations are
    literal backslashes inside the single quotes holding the program, so node was handed a source
    beginning with one and refused it.

    A file is left alone where re-serialising it as read would not reproduce it. What is being
    written is a version, and a lockfile silently reformatted is a diff nobody asked for.
    """
    ready = []
    for name in MANIFESTS + LOCKFILES:
        path = root / name
        try:
            text = path.read_text(encoding="utf-8")
            data = json.loads(text)
        except (OSError, ValueError) as why:
            print(f"{name}: {why}", file=sys.stderr)
            return 1
        if serialise(data) != text:
            print(
                f"{name}: writing it back would change more than the version, so nothing was "
                "written. Set the version there by hand and say why here.",
                file=sys.stderr,
            )
            return 1
        data["version"] = version
        locked = pick(data, "packages")
        if isinstance(pick(locked, ""), dict):
            locked[""]["version"] = version
        ready.append((path, serialise(data)))

    # Every file is read and checked before any is written, so a refusal leaves the tree as it
    # was rather than half bumped, which is the state the guard before the commit reports and
    # nobody can tell from a genuine disagreement.
    for path, text in ready:
        path.write_text(text, encoding="utf-8")
    print(f"check-versions: wrote {version} into {len(ready)} files")
    return 0


def verdict(version, found):
    """What is wrong, one line each, empty when every file agrees."""
    complaints = []
    for where, stated_version in found:
        if stated_version is None:
            complaints.append(f"{where}: no version could be read from it")
        elif stated_version != version:
            complaints.append(f"{where}: states {stated_version}, not {version}")
    return complaints


def check(root):
    manifest = root / "Cargo.toml"
    try:
        version = workspace_version(manifest.read_text(encoding="utf-8"))
    except OSError:
        version = None
    if version is None:
        print(f"{manifest}: no [workspace.package] version to compare against", file=sys.stderr)
        return 1

    found = stated(root)
    complaints = verdict(version, found)
    for complaint in complaints:
        print(complaint, file=sys.stderr)
    if complaints:
        print(
            "Every file that states the version states the same one. Run "
            "`make bump-version BUMP=...` rather than editing one of them by hand.",
            file=sys.stderr,
        )
        return 1

    print(f"check-versions: {len(found) + 1} statements of {version}, all agreeing")
    return 0


# One fixture per file this has to read, each broken in one way, because a check that reads four
# of the six files passes on this tree today and would have passed on the tree that shipped the
# front end at 0.1.0.
CASES = [
    ("an agreeing tree passes", {}, None),
    ("the published wrapper lagging", {"package.json": "0.8.0"}, "package.json"),
    (
        "the wrapper's lockfile header lagging",
        {"package-lock.json": "0.8.0"},
        "package-lock.json",
    ),
    (
        "the wrapper's locked entry lagging",
        {"package-lock.json packages": "0.8.0"},
        'package-lock.json packages[""]',
    ),
    ("the application lagging", {"ui/package.json": "0.1.0"}, "ui/package.json"),
    (
        "the application's lockfile header lagging",
        {"ui/package-lock.json": "0.1.0"},
        "ui/package-lock.json",
    ),
    (
        "the application's locked entry lagging",
        {"ui/package-lock.json packages": "0.1.0"},
        'ui/package-lock.json packages[""]',
    ),
    ("a file that is not there", {"ui/package.json": None}, "ui/package.json"),
]


def fixture(root, version, broken):
    """A tree stating `version` everywhere, except where `broken` says otherwise."""
    (root / "ui").mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        '[workspace]\nrust-version = "1.88"\n\n[workspace.package]\n'
        f'version = "{version}"\nedition = "2024"\n',
        encoding="utf-8",
    )
    for name in MANIFESTS:
        if broken.get(name, version) is None:
            continue
        (root / name).write_text(
            serialise({"name": name, "version": broken.get(name, version)}), encoding="utf-8"
        )
    for name in LOCKFILES:
        if broken.get(name, version) is None:
            continue
        (root / name).write_text(
            serialise(
                {
                    "name": name,
                    "version": broken.get(name, version),
                    "packages": {
                        "": {"version": broken.get(f"{name} packages", version)},
                        "node_modules/react": {"version": "19.0.0"},
                    },
                }
            ),
            encoding="utf-8",
        )


def quietly(run):
    """Run it, returning what it exited with and everything it printed."""
    captured = []
    out, err = sys.stdout, sys.stderr
    sys.stdout = sys.stderr = Captured(captured)
    try:
        code = run()
    finally:
        sys.stdout, sys.stderr = out, err
    return code, "".join(captured)


def selftest():
    """Prove the check fails on each file separately, and for the reason it names.

    A version check that is quietly partial reports success forever, and the thing it was meant
    to stop is a release nobody can install. So each case breaks one statement of the version and
    asks both questions: whether the check failed, and whether it said which file.

    The write is checked against the same fixtures, because a bump that misses a file and a check
    that cannot see one produce the same green tree.
    """
    import tempfile

    checks = []
    for name, broken, blamed in CASES:
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            fixture(root, "0.9.0", broken)
            code, said = quietly(lambda: check(root))
        wanted = 0 if blamed is None else 1
        checks.append((f"{name}: exits {wanted}", code == wanted, code))
        if blamed is not None:
            checks.append((f"{name}: names {blamed}", blamed in said, said))

    # Every file behind at once, which is the state a bump starts from, and then the check as the
    # question of whether the write reached all of them.
    behind = {name: "0.8.0" for name in MANIFESTS + LOCKFILES}
    behind.update({f"{name} packages": "0.8.0" for name in LOCKFILES})
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        fixture(root, "0.9.0", behind)
        wrote, _ = quietly(lambda: write(root, "0.9.0"))
        code, said = quietly(lambda: check(root))
    checks.append(("writing a version: succeeds", wrote == 0, wrote))
    checks.append(("writing a version: leaves every file agreeing", code == 0, said))

    # A file this cannot write back as it found it, which is a lockfile it would silently
    # reformat around the one value it was asked to change.
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        fixture(root, "0.9.0", {})
        hand_edited = root / "ui/package-lock.json"
        hand_edited.write_text(
            hand_edited.read_text(encoding="utf-8").replace("\n  ", "\n    "), encoding="utf-8"
        )
        before = {name: (root / name).read_text(encoding="utf-8") for name in MANIFESTS + LOCKFILES}
        wrote, said = quietly(lambda: write(root, "1.0.0"))
        after = {name: (root / name).read_text(encoding="utf-8") for name in MANIFESTS + LOCKFILES}
    checks.append(("a file it cannot write back: refuses", wrote == 1, wrote))
    checks.append(("a file it cannot write back: names it", "ui/package-lock.json" in said, said))
    checks.append(("a file it cannot write back: writes nothing at all", after == before, wrote))

    broke = [(claim, got) for claim, held, got in checks if not held]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


class Captured:
    """Collects what the check writes, so the selftest can read what it blamed and print nothing
    of its own beyond its verdict."""

    def __init__(self, into):
        self.into = into

    def write(self, text):
        self.into.append(text)
        return len(text)

    def flush(self):
        pass


def main():
    ap = argparse.ArgumentParser(
        description="Check that every file stating a version states the workspace's.",
    )
    ap.add_argument("--root", type=Path, default=ROOT, help="the tree to read (default: this one)")
    ap.add_argument(
        "--set",
        dest="version",
        help="write this version into every JSON file that states one, for `make bump-version`",
    )
    ap.add_argument(
        "--selftest", action="store_true", help="check the verdict on known trees, reading no tree"
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.version:
        return write(args.root, args.version)
    return check(args.root)


if __name__ == "__main__":
    sys.exit(main())
