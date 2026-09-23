#!/usr/bin/env python3
"""Stream this checkout's current source to a check container."""

import os
from pathlib import Path
# Git receives fixed arguments, never shell input.
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tarfile


def main():
    # The executable and every argument are fixed; no shell or file contents are evaluated.
    files = subprocess.check_output([  # nosemgrep: gitlab.bandit.B603
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
    ])
    with tarfile.open(fileobj=sys.stdout.buffer, mode="w|") as archive:
        for name in files.split(b"\0"):
            if not name:
                continue
            path = Path(os.fsdecode(name))
            # Deleted files and nested repositories are not part of this snapshot.
            if not os.path.lexists(path) or (path.is_dir() and not path.is_symlink()):
                continue
            archive.add(path, arcname=str(path), recursive=False)


if __name__ == "__main__":
    main()
