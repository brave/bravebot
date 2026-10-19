#!/usr/bin/env python3
"""Writes the Homebrew cask and the WinGet manifests for one published release.

    package-manifests.py v<version> <directory of .sha256 files> <output directory>

RELEASE-18 says what the packages hold: the release URL of the fixed repository, the tag `v` plus
the version in the tree, and the digests Jenkins published beside each binary. This is the step
that writes them, after the GitHub release exists and before anybody opens the change against the
tap or the WinGet community repository. It reads files and writes files: nothing is fetched here,
so the digests are the ones somebody downloaded with `gh release download --pattern '*.sha256'`,
and nothing is pushed.

It refuses, writing nothing, when the tag is not the tree's version, when a digest file is missing
or is not 64 hex digits, or when a platform the packages name has none. A package with an empty or
guessed digest would install whatever was served, which is the failure the digest is for.

`--selftest` runs the refusals on made-up files. Standard library only, like the other checks.
"""

import argparse
import importlib.util
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Composed from nothing, as RELEASE-13 requires of every tool that fetches from a release: not an
# argument, not a file, and not the environment. install.sh and the npm package script state the
# same repository.
REPO = "brave/bravebot"
RELEASE_URL = f"https://github.com/{REPO}/releases/download"

# The release asset each package platform installs. A macOS or Windows binary only: Linux is the
# install script's and the npm package's.
CASK_ASSETS = {"arm": "bravebot-darwin-arm64", "intel": "bravebot-darwin-amd64"}
WINGET_ASSETS = {"arm64": "bravebot-windows-arm64.exe", "x64": "bravebot-windows-amd64.exe"}

WINGET_IDENTIFIER = "Brave.Bravebot"
MANIFEST_VERSION = "1.6.0"
DESCRIPTION = "An agent that cannot be steered by the content it reads"

HEX_DIGEST = re.compile(r"[0-9a-fA-F]{64}")
TAG = re.compile(r"v(\d+\.\d+\.\d+)")


class Refused(Exception):
    pass


def tree_version():
    """The version the tree states, read by check-versions so the two never disagree about it."""
    spec = importlib.util.spec_from_file_location("check_versions", ROOT / "contrib" / "check-versions.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.workspace_version((ROOT / "Cargo.toml").read_text())


def named_version(tag, tree):
    found = TAG.fullmatch(tag)
    if not found:
        raise Refused(f"{tag!r} is not v followed by three numbers")
    if found.group(1) != tree:
        raise Refused(f"{tag} is not v{tree}, the version in this tree")
    return found.group(1)


def digest(directory, asset):
    """The published digest of one asset, lowercase, or a refusal saying which one is wrong."""
    path = directory / f"{asset}.sha256"
    try:
        text = path.read_text()
    except OSError:
        raise Refused(f"{asset}.sha256 is missing from {directory}") from None
    fields = text.split()
    if not fields or not HEX_DIGEST.fullmatch(fields[0]):
        raise Refused(f"{asset}.sha256 does not start with a 64 digit hex checksum")
    return fields[0].lower()


def cask(version, digests):
    return f'''cask "bravebot" do
  arch arm: "arm64", intel: "amd64"

  version "{version}"
  sha256 arm:   "{digests["arm"]}",
         intel: "{digests["intel"]}"

  url "{RELEASE_URL}/v#{{version}}/bravebot-darwin-#{{arch}}"
  name "bravebot"
  desc "{DESCRIPTION}"
  homepage "https://github.com/{REPO}"

  livecheck do
    url :url
    strategy :github_latest
  end

  binary "bravebot-darwin-#{{arch}}", target: "bravebot"
end
'''


def winget(version, digests):
    installers = "".join(
        f"""- Architecture: {arch}
  InstallerUrl: {RELEASE_URL}/v{version}/{asset}
  InstallerSha256: {digests[arch].upper()}
"""
        for arch, asset in WINGET_ASSETS.items()
    )
    header = f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.%s.{MANIFEST_VERSION}.schema.json\n"
    return {
        f"{WINGET_IDENTIFIER}.yaml": header % "version" + f"""PackageIdentifier: {WINGET_IDENTIFIER}
PackageVersion: {version}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: {MANIFEST_VERSION}
""",
        f"{WINGET_IDENTIFIER}.installer.yaml": header % "installer" + f"""PackageIdentifier: {WINGET_IDENTIFIER}
PackageVersion: {version}
InstallerType: portable
Commands:
- bravebot
Installers:
{installers}ManifestType: installer
ManifestVersion: {MANIFEST_VERSION}
""",
        f"{WINGET_IDENTIFIER}.locale.en-US.yaml": header % "defaultLocale" + f"""PackageIdentifier: {WINGET_IDENTIFIER}
PackageVersion: {version}
PackageLocale: en-US
Publisher: Brave Software
PublisherUrl: https://brave.com
PackageName: bravebot
PackageUrl: https://github.com/{REPO}
License: MPL-2.0
LicenseUrl: https://github.com/{REPO}/blob/v{version}/LICENSE
ShortDescription: {DESCRIPTION}
ManifestType: defaultLocale
ManifestVersion: {MANIFEST_VERSION}
""",
    }


def write(tag, checksums, out, tree):
    """Write both packages, or raise Refused having written nothing."""
    version = named_version(tag, tree)
    digests = {platform: digest(checksums, asset) for platform, asset in {**CASK_ASSETS, **WINGET_ASSETS}.items()}
    files = {Path("Casks") / "bravebot.rb": cask(version, digests)}
    # The WinGet community repository's layout: first letter of the identifier, then each part.
    directory = Path("manifests") / "b" / "Brave" / "Bravebot" / version
    files.update({directory / name: text for name, text in winget(version, digests).items()})
    for relative, text in files.items():
        target = out / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    return sorted(files)


def selftest():
    checks = []

    def attempt(tag, files, tree="0.9.0", skip=None):
        with tempfile.TemporaryDirectory() as tmp:
            checksums, out = Path(tmp, "in"), Path(tmp, "out")
            checksums.mkdir()
            for asset, text in files.items():
                if asset != skip:
                    (checksums / f"{asset}.sha256").write_text(text)
            try:
                written = write(tag, checksums, out, tree)
            except Refused as refusal:
                return None, str(refusal), list(out.rglob("*")) if out.exists() else []
            return {p: (out / p).read_text() for p in written}, "", []

    assets = [*CASK_ASSETS.values(), *WINGET_ASSETS.values()]
    good = {asset: f"{index:x}" * 64 + "\n" for index, asset in enumerate(assets, 1)}
    done, why, _ = attempt("v0.9.0", good)
    cask_text = done[Path("Casks/bravebot.rb")]
    installer = next(text for path, text in done.items() if path.name.endswith(".installer.yaml"))
    checks.append(("a release writes the cask and three manifests", len(done) == 4, sorted(done)))
    checks.append(("the cask holds each macOS digest", all(good[a].strip() in cask_text for a in CASK_ASSETS.values()), cask_text))
    checks.append(("the manifest holds each Windows digest, in upper case",
                   all(good[a].strip().upper() in installer for a in WINGET_ASSETS.values()), installer))
    checks.append(("the cask names the fixed repository", f"{RELEASE_URL}/v#{{version}}/" in cask_text, cask_text))

    for asset in assets:
        none, why, left = attempt("v0.9.0", good, skip=asset)
        checks.append((f"a missing digest for {asset} writes nothing", none is None and asset in why and not left, why))
    for bad in ("", "abc\n", "g" * 64, "a" * 63, "a" * 65):
        none, why, left = attempt("v0.9.0", {**good, assets[0]: bad})
        checks.append((f"the digest {bad[:8]!r} ({len(bad)} characters) is refused", none is None and not left, why))
    for tag in ("v0.9.1", "0.9.0", "v0.9", "v0.9.0-rc.1", "v0.9.0/../x", ""):
        none, why, left = attempt(tag, good)
        checks.append((f"the tag {tag!r} is refused", none is None and not left, why))

    broke = [(claim, got) for claim, ok, got in checks if not ok]
    for claim, got in broke:
        print(f"selftest: {claim}, got {got!r}", file=sys.stderr)
    if broke:
        print(f"{len(broke)} of {len(checks)} checks failed", file=sys.stderr)
        return 1
    print(f"selftest: {len(checks)} checks passed")
    return 0


def main():
    ap = argparse.ArgumentParser(description="Write the Homebrew cask and WinGet manifests for a release.")
    ap.add_argument("tag", nargs="?")
    ap.add_argument("checksums", nargs="?", type=Path)
    ap.add_argument("output", nargs="?", type=Path)
    ap.add_argument("--selftest", action="store_true", help="check the refusals on made-up files")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if not (args.tag and args.checksums and args.output):
        ap.error("a tag, a directory of checksums and an output directory are required")
    try:
        written = write(args.tag, args.checksums, args.output, tree_version())
    except Refused as refusal:
        print(f"error: {refusal}", file=sys.stderr)
        return 1
    for path in written:
        print(args.output / path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
