#!/usr/bin/env python3
"""Collect the conformance reviewers' results and render the check-spec report.

Reads the work directory that check-spec.py prepared, merges what the reviewers wrote
with the mechanical findings already in the manifest, and prints one report. Exits
non-zero when anything at severity error survives, so the skill and a shell both get the
same answer from the same place.
"""

import argparse
import importlib.util
import json
import sys
from pathlib import Path

_shared = importlib.util.spec_from_file_location(
    "work_dir", Path(__file__).resolve().parents[2] / "work_dir.py"
)
workdir = importlib.util.module_from_spec(_shared)
_shared.loader.exec_module(workdir)

read_manifest = workdir.read_manifest

ERROR = "error"
WARNING = "warning"

VERDICT_SEVERITY = {"violation": ERROR, "untested": WARNING}


def carried(entry, clause, before):
    """A finding taken from the record: what an earlier run decided, with nothing the clause depends
    on changed since. It is reported, and it is not drafted again, because the run that decided it
    drafted it."""
    return {
        "spec": entry["path"],
        "clause": clause,
        "severity": before["severity"],
        "kind": before["verdict"],
        "summary": before["summary"] or "(no summary)",
        "evidence": None,
        "carried": True,
        "recorded": before["commit"],
        "source": "review",
    }


def load_reviews(manifest):
    """The findings the reviewers' results and the record make, how many clauses a reviewer read,
    and what each of them decided, keyed by clause, for `record-reviewed.py`."""
    findings = []
    reviewed = 0
    decided = {}
    for entry in manifest["specs"]:
        for held in entry.get("kept", []):
            if held["verdict"] != "conforms":
                findings.append(carried(entry, held["clause"], held))
        for prompt in entry["reviewer_prompts"]:
            path = Path(prompt["results_file"])
            if not path.exists():
                findings.append(
                    {
                        "spec": entry["path"],
                        "clause": ", ".join(prompt["clauses"]),
                        "severity": ERROR,
                        "kind": "review-incomplete",
                        "summary": "no reviewer result: these clauses were never checked",
                        "evidence": str(path),
                        "fix": "rerun the reviewer for this chunk before trusting the report",
                        "source": "review",
                    }
                )
                continue
            try:
                result = json.loads(path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, OSError) as problem:
                findings.append(
                    {
                        "spec": entry["path"],
                        "clause": ", ".join(prompt["clauses"]),
                        "severity": ERROR,
                        "kind": "review-unreadable",
                        "summary": f"reviewer result could not be read: {problem}",
                        "evidence": str(path),
                        "source": "review",
                    }
                )
                continue

            expected = set(prompt["clauses"])
            for clause in result.get("clauses", []):
                reviewed += 1
                expected.discard(clause.get("clause"))
                before = prompt.get("previous", {}).get(clause.get("clause"))
                holds = bool(before) and clause.get("holds") is True
                verdict = clause.get("verdict", before["verdict"] if holds else "unclear")
                # A reviewer that says the earlier verdict holds, and names no other, has kept
                # it. One that names another has not, whatever it says about `holds`.
                if holds and verdict == before["verdict"]:
                    decided[clause["clause"]] = dict(before, held=True)
                    if verdict != "conforms":
                        findings.append(carried(entry, clause["clause"], before))
                    continue
                decided[clause.get("clause")] = {
                    "verdict": verdict,
                    "severity": clause.get("severity") or VERDICT_SEVERITY.get(verdict, "-"),
                    "summary": clause.get("summary") or "",
                    "held": False,
                }
                if verdict == "conforms":
                    continue
                if verdict == "unclear":
                    findings.append(
                        {
                            "spec": entry["path"],
                            "clause": clause.get("clause"),
                            "severity": WARNING,
                            "kind": "unclear",
                            "summary": clause.get("summary")
                            or "the reviewer could not tell from the governed files",
                            "evidence": "; ".join(clause.get("evidence", [])) or None,
                            "fix": clause.get("fix"),
                            "source": "review",
                        }
                    )
                    continue
                findings.append(
                    {
                        "spec": entry["path"],
                        "clause": clause.get("clause"),
                        "severity": clause.get("severity")
                        or VERDICT_SEVERITY.get(verdict, ERROR),
                        "kind": verdict,
                        "summary": clause.get("summary") or "(no summary)",
                        "evidence": "; ".join(clause.get("evidence", [])) or None,
                        "failure": clause.get("failure"),
                        "fix": clause.get("fix"),
                        # Not rendered. Whether the code attempts the behaviour and gets it wrong,
                        # or does not attempt it at all, which decides the label the draft carries.
                        "absent": bool(clause.get("absent")),
                        # Not rendered. It is how to reach the screen this shows up on, for a
                        # bug report that shows the wrong screen rather than describing it.
                        "screen": clause.get("screen"),
                        "source": "review",
                    }
                )
            for missing in sorted(expected):
                findings.append(
                    {
                        "spec": entry["path"],
                        "clause": missing,
                        "severity": ERROR,
                        "kind": "review-incomplete",
                        "summary": "the reviewer returned no verdict for this clause",
                        "evidence": str(path),
                        "source": "review",
                    }
                )
    return findings, reviewed, decided


def render(findings, manifest, reviewed):
    lines = []
    by_spec = {}
    for item in findings:
        by_spec.setdefault(item["spec"], []).append(item)

    order = {ERROR: 0, WARNING: 1}
    for spec in sorted(by_spec):
        lines.append("")
        lines.append(spec)
        for item in sorted(by_spec[spec], key=lambda f: order.get(f["severity"], 2)):
            mark = "error" if item["severity"] == ERROR else "warn "
            clause = f"{item['clause']}: " if item.get("clause") else ""
            lines.append(f"  {mark}  {clause}{item['summary']}")
            if item.get("carried"):
                lines.append(f"         recorded at {item['recorded']} and still holds")
            if item.get("failure"):
                lines.append(f"         failure: {item['failure']}")
            if item.get("evidence"):
                lines.append(f"         at {item['evidence']}")
            if item.get("fix"):
                lines.append(f"         fix: {item['fix']}")

    errors = [f for f in findings if f["severity"] == ERROR]
    warnings = [f for f in findings if f["severity"] == WARNING]
    clauses = sum(entry["clauses"] for entry in manifest["specs"])
    kept = sum(len(entry.get("kept", [])) for entry in manifest["specs"])
    lines.append("")
    lines.append(
        f"{len(manifest['specs'])} specs, {clauses} clauses, {reviewed} reviewed"
        + (f", {kept} carried from the record" if kept else "")
        + f": {len(errors)} errors, {len(warnings)} warnings"
    )
    if not errors and not warnings:
        lines.append("the implementation matches the spec")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--strict", action="store_true", help="warnings fail too")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    manifest = read_manifest(args.work_dir)
    if manifest is None:
        return 2

    findings = list(manifest.get("mechanical_findings", []))
    reviews, reviewed, _ = load_reviews(manifest)
    findings.extend(reviews)

    strict = args.strict or manifest.get("strict", False)
    failed = any(f["severity"] == ERROR for f in findings) or (strict and bool(findings))

    if args.json:
        print(json.dumps({"findings": findings, "failed": failed}, indent=2))
    else:
        print(render(findings, manifest, reviewed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
