#!/usr/bin/env python3
"""Record what a check-spec run decided, so the next run reviews only what changed.

    python3 agents/skills/check-spec/record-reviewed.py --work-dir "$WORK_DIR" [--dry-run]

Reads the work directory check-spec.py prepared and the reviewers' results, and writes one line per
clause a reviewer decided to docs/check-spec-reviewed: the verdict, the main commit it was reached
at, and what the clause said then. A clause the reviewer could not decide loses its line, so it is
reviewed from the start next time. A clause with no result keeps the line it had, which still
describes the commit it names. No model takes part in anything this script does.

It writes only for a tree that is main, apart from the ledger, because the commit it records is
what a later run takes a diff from. It does not commit the file.
"""

import argparse
import datetime
import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import reviewed  # noqa: E402
from specs import load_specs  # noqa: E402


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


collect = load("collect_findings", HERE / "collect-findings.py")
# The advisory skill owns the rule for which commit a verdict may be recorded against.
pa = load("peer_advisories", HERE.parent / "peer-advisories" / "peer-advisories.py")


def recorded(manifest, decided, ledger, commit, today):
    """`ledger` with what this run decided put in, and how many lines each kind of change was."""
    counts = {"written": 0, "held": 0, "dropped": 0, "pruned": 0}
    for entry in manifest["specs"]:
        for prompt in entry["reviewer_prompts"]:
            for clause in prompt["clauses"]:
                answer = decided.get(clause)
                if answer is None:
                    continue
                if answer["verdict"] not in reviewed.VERDICTS:
                    counts["dropped"] += ledger.pop(clause, None) is not None
                    continue
                ledger[clause] = {
                    "verdict": answer["verdict"],
                    "severity": reviewed.severity_of(answer["verdict"], answer["severity"]),
                    "commit": commit,
                    "hash": prompt["hashes"][clause],
                    "date": today,
                    "summary": "" if answer["verdict"] == "conforms" else answer["summary"],
                }
                counts["written"] += 1
                counts["held"] += bool(answer.get("held"))
    return counts


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--dry-run", action="store_true", help="say what would be written, and write nothing")
    args = parser.parse_args(argv)

    manifest = collect.read_manifest(args.work_dir)
    if manifest is None:
        return 2
    root = Path(manifest["root"])
    try:
        _, tip = pa.checked_commit(
            root,
            str(reviewed.LEDGER),
            verb="record",
            worktree="check-spec",
            why="a verdict has to be about main",
        )
        if pa.git(root, "rev-parse", "HEAD") != manifest.get("commit"):
            raise pa.Problem("HEAD is not the commit this run was prepared at, so what was reviewed is unknown")
        if manifest.get("dirty") != []:
            raise pa.Problem(
                "this run was prepared on a tree that was not a commit, "
                f"{(manifest.get('dirty') or ['unknown'])[0]} first, so what was reviewed was not main"
            )
        path = root / reviewed.LEDGER
        ledger = reviewed.read_ledger(path)
    except (pa.Problem, reviewed.Problem) as problem:
        print(f"nothing recorded: {problem}", file=sys.stderr)
        return 2

    _, _, decided = collect.load_reviews(manifest)
    counts = recorded(manifest, decided, ledger, tip[:12], datetime.date.today().isoformat())
    live = {c.id for spec in load_specs(root / "docs" / "specs") for c in spec.clauses if not c.withdrawn}
    for clause in [clause for clause in ledger if clause not in live]:
        del ledger[clause]
        counts["pruned"] += 1

    verb = "would record" if args.dry_run else "recorded"
    if not args.dry_run:
        reviewed.write_ledger(path, ledger)
    print(
        f"{verb} {counts['written']} verdicts at {tip[:12]} in {reviewed.LEDGER} "
        f"({counts['held']} held from an earlier review, {counts['written'] - counts['held']} new); "
        f"{counts['dropped']} dropped as undecided, {counts['pruned']} for clauses no longer there; "
        f"{len(ledger)} lines in all"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
