#!/usr/bin/env python3
"""The `manifest.json` every skill script keeps in its work directory.

A skill's preparing script writes the manifest, and its collect, verify and draft scripts read it
back. The scripts load this module by path, the way they load `issue_helper.py`.
"""

import json
import sys
from pathlib import Path

MANIFEST = "manifest.json"


def manifest_path(work_dir):
    """Where the manifest of a work directory is."""
    return Path(work_dir) / MANIFEST


def read_manifest(work_dir):
    """The parsed `manifest.json` of a work directory, or None after saying there is none."""
    path = manifest_path(work_dir)
    if not path.exists():
        print(f"no manifest at {path}", file=sys.stderr)
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def write_manifest(work_dir, manifest):
    """Write `manifest` as the `manifest.json` of a work directory and return its path."""
    path = manifest_path(work_dir)
    path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return path
