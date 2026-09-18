#!/usr/bin/env python3
"""Merge the mechanical findings with the verified candidates and render one report.

Reads the work directory, applies each verifier's verdict to the candidate it was given, and prints
what survived. Exits non-zero when anything at severity error stands, so the skill and a shell get the
same answer from the same place.

    python3 agents/skills/security-audit/collect-findings.py --work-dir "$WORK_DIR"

A dropped candidate is not silently discarded. The count of what was dropped and why is the evidence
that the verification pass did anything, and a run where nothing was dropped is a run to be suspicious
of.
"""

import argparse
import json
import sys
from pathlib import Path

ERROR = "error"
WARNING = "warning"

CONFIRMED, DROPPED, UNCLEAR = "CONFIRMED", "DROPPED", "UNCLEAR"
IMPACTS = ("high", "medium", "low")

# A confirmed candidate's severity follows its impact, because impact is the thing a reader was asked
# to judge and a second scale judged by nobody would only disagree with it.
SEVERITY_FOR = {"high": ERROR, "medium": ERROR, "low": WARNING}


def merge(candidate, verdict):
    """The candidate as the verifier left it. Corrections win, since the verifier opened the files."""
    corrected = verdict.get("corrected") or {}
    impact = verdict.get("impact") or candidate.get("impact") or "medium"
    if impact not in IMPACTS:
        impact = "medium"
    evidence = corrected.get("evidence") or candidate.get("evidence") or []
    return {
        "severity": SEVERITY_FOR[impact],
        "impact": impact,
        "kind": candidate.get("kind") or "violation",
        "lane": candidate.get("lane"),
        "summary": corrected.get("summary") or candidate.get("summary") or "(no summary)",
        "place": corrected.get("place") or candidate.get("place"),
        "entry": candidate.get("entry"),
        "decision": candidate.get("decision"),
        "gain": corrected.get("gain") or candidate.get("gain"),
        "failure": corrected.get("failure"),
        "evidence": evidence if isinstance(evidence, list) else [str(evidence)],
        "clause": corrected.get("clause") or candidate.get("clause"),
        "area": corrected.get("area") or candidate.get("area"),
        "fix": corrected.get("fix") or candidate.get("fix"),
        "refutation": candidate.get("refutation_considered"),
        "verified_reason": verdict.get("reason"),
        "test_wanted": verdict.get("test_that_would_pin_it"),
        # What a reader could see, and what they could run. The verifier decides both, because it is
        # the pass that opened the files and knows whether the claim survives being run.
        "screen": verdict.get("screen") or candidate.get("screen"),
        "reproduce": verdict.get("reproduce") or candidate.get("reproduce"),
        "source": "audit",
        "verdict": CONFIRMED,
    }


def load_verdicts(manifest):
    """Apply each verdict, and report a candidate whose verifier left nothing."""
    findings, dropped, unclear, missing = [], [], [], []
    for entry in manifest.get("verifications", []):
        path = Path(entry["results_file"])
        candidate = entry["candidate"]
        if not path.exists():
            missing.append(entry)
            continue
        try:
            verdict = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as problem:
            missing.append({**entry, "problem": str(problem)})
            continue
        decided = (verdict.get("verdict") or "").upper()
        if decided == CONFIRMED:
            findings.append(merge(candidate, verdict))
        elif decided == UNCLEAR:
            unclear.append({**entry, "reason": verdict.get("reason")})
        else:
            dropped.append(
                {
                    **entry,
                    "reason": verdict.get("reason"),
                    "existing_issue": verdict.get("existing_issue"),
                }
            )
    return findings, dropped, unclear, missing


def render(findings, mechanical, dropped, unclear, missing, manifest):
    lines = []
    order = {ERROR: 0, WARNING: 1}
    rank = {"high": 0, "medium": 1, "low": 2}

    if mechanical:
        lines.append("")
        lines.append("mechanical")
        for item in sorted(mechanical, key=lambda f: order.get(f["severity"], 2)):
            mark = "error" if item["severity"] == ERROR else "warn "
            lines.append(f"  {mark}  {item['summary']}")
            for one in item.get("evidence") or []:
                lines.append(f"         at {one}")

    if findings:
        lines.append("")
        lines.append("confirmed")
        for item in sorted(findings, key=lambda f: rank.get(f["impact"], 3)):
            lines.append(f"  {item['impact']:<6} [{item['lane']}] {item['summary']}")
            if item.get("place"):
                lines.append(f"         at {item['place']}")
            if item.get("gain"):
                lines.append(f"         buys: {item['gain']}")

    if dropped:
        lines.append("")
        lines.append(f"dropped by verification ({len(dropped)})")
        for item in dropped:
            note = f" (#{item['existing_issue']})" if item.get("existing_issue") else ""
            lines.append(f"         [{item['lane']}] {item['summary']}{note}")
            if item.get("reason"):
                lines.append(f"           {item['reason']}")

    if unclear:
        lines.append("")
        lines.append(f"unsettled ({len(unclear)}), not filed")
        for item in unclear:
            lines.append(f"         [{item['lane']}] {item['summary']}")

    if missing:
        lines.append("")
        lines.append(f"unverified ({len(missing)}), not filed")
        for item in missing:
            lines.append(f"         [{item['lane']}] {item['summary']}")

    high = sum(1 for f in findings if f["impact"] == "high")
    lines.append("")
    lines.append(
        f"{len(manifest.get('lanes', []))} lanes, "
        f"{len(manifest.get('verifications', []))} candidates, "
        f"{len(findings)} confirmed ({high} high), {len(dropped)} dropped, "
        f"{len(mechanical)} mechanical"
    )
    if not findings and not mechanical:
        lines.append("nothing survived verification, and the mechanical pass is clean")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    manifest_path = Path(args.work_dir) / "manifest.json"
    if not manifest_path.exists():
        print(f"no manifest at {manifest_path}", file=sys.stderr)
        return 2
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    mechanical = list(manifest.get("mechanical_findings", []))
    findings, dropped, unclear, missing = load_verdicts(manifest)

    everything = mechanical + findings
    failed = any(f["severity"] == ERROR for f in everything)

    if args.json:
        print(json.dumps({"findings": everything, "dropped": dropped, "failed": failed}, indent=2))
    else:
        print(render(findings, mechanical, dropped, unclear, missing, manifest))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
