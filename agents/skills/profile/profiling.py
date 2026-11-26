#!/usr/bin/env python3
"""Measure bravebot, have reviewers decide which slow or large parts are worth an issue, and draft those.

    python3 agents/skills/profile/profiling.py [lane ...] [--measure-only]
    python3 agents/skills/profile/profiling.py status|collect|draft|verify|claims-prepare|claims-collect|record|clean --work-dir DIR

The scan runs the real programs in six lanes (tests, startup, build, size, scripts, functions), with
`HOME` pointed at a scratch directory, and ranks each measurement by how far it is over a stated
threshold or over the value `docs/profile-reviewed` recorded at an earlier commit. The functions lane
samples the tests and starts the other lanes found over their thresholds and ranks the functions in
them. A candidate is a number, not
a finding: reviewers read the code behind it, and only what a reviewer calls `optimize` becomes a
draft. The steps after the scan are the ones the maintenance skill uses, in `agents/review_pipeline.py`.

Nothing here edits source or posts anything. `--measure-only` prints the candidates and exits zero,
which is what `make check-profile` runs. Standard library only, like the other checks.
"""

import argparse
import datetime
import importlib.util
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE))


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pipeline = _load("review_pipeline", HERE.parents[1] / "review_pipeline.py")
workdir = _load("work_dir", HERE.parents[1] / "work_dir.py")
import ledger  # noqa: E402
import lanes as lane_rules  # noqa: E402
import measure  # noqa: E402
import prompts  # noqa: E402

drafting = pipeline.drafting
LANES = ("tests", "startup", "build", "size", "scripts", "functions")
PREFIX = {"tests": "T", "startup": "S", "build": "B", "size": "Z", "scripts": "C", "functions": "F"}
# The lanes whose candidates the functions lane samples, so a run of it measures them first.
SAMPLED_BY_FUNCTIONS = ("tests", "startup")
CHANGE = "optimize"
CHUNK = 8
LIMIT = 15
KIND, BROKEN = "enhancement", "bug"
TITLE_LIMIT = pipeline.TITLE_LIMIT
ASSIGNEE = pipeline.ASSIGNEE
AREAS = pipeline.AREAS
INFRASTRUCTURE = pipeline.INFRASTRUCTURE
COMMANDS = ("prepare", "collect", "draft", "verify", "claims-prepare", "claims-collect", "record", "clean", "status")


def scan(root, lanes=LANES, scratch=None, runners=None, previous=None, fresh=False, limit=None):
    """`(result, rows)`: the candidates per lane, ranked, capped and numbered, and every row measured.

    `previous` is the ledger. A row the ledger holds a verdict of `keep` or `optimize` for is carried,
    and left out before the cap, unless it has grown past the recorded value, so the cap reaches new
    candidates. `runners` is what measures each lane, `measure.RUNNERS` unless a test supplies its own.
    The functions lane runs last, on the candidates of the tests and startup lanes, which it adds to
    `lanes` where they were not asked for, and the functions it finds are named beside those."""
    runners = runners or measure.RUNNERS
    previous = {} if previous is None else previous
    scratch = Path(scratch or tempfile.mkdtemp(prefix="profile-scratch-"))
    context = measure.Context(root, scratch)
    lanes = set(lanes) | (set(SAMPLED_BY_FUNCTIONS) if "functions" in lanes else set())
    context.lanes = lanes
    result, every = {}, []
    for lane in LANES:
        if lane not in lanes:
            continue
        rows, note = runners[lane](context)
        every.extend(rows)
        candidates, carried = [], 0
        for entry in rows:
            before = previous.get(entry["key"])
            judged = lane_rules.judge(entry, before["value"] if before else None)
            if judged is None:
                continue
            if before and before["verdict"] in ("keep", "optimize") and not judged["regressed"] and not fresh:
                carried += 1
                continue
            candidates.append(judged)
        candidates.sort(key=lambda c: (-c["rank"], c["key"]))
        context.candidates.extend(candidates)
        kept = candidates[: limit or LIMIT]
        for number, candidate in enumerate(kept, start=1):
            candidate["id"] = f"{PREFIX[lane]}{number}"
        result[lane] = {"candidates": kept, "left_out": len(candidates) - len(kept), "carried": carried}
        if note:
            result[lane]["note"] = note
    for entry in result.values():
        for candidate in entry["candidates"]:
            if candidate["key"] in context.hot:
                candidate["hot"] = context.hot[candidate["key"]]
    return result, every


def report(result):
    lines, total = [], 0
    for lane, entry in result.items():
        candidates = entry["candidates"]
        total += len(candidates)
        lines.append("")
        lines.append(
            f"{lane}: {len(candidates)}"
            + (f", {entry['left_out']} more past the cap" if entry["left_out"] else "")
            + (f", {entry['carried']} already reviewed ({ledger.LEDGER})" if entry.get("carried") else "")
        )
        if entry.get("note"):
            lines.append(f"  note: {entry['note']}")
        for candidate in candidates:
            lines.append(f"  {candidate['id']:<4} {candidate['detail']}")
    lines.append("")
    lines.append(f"{total} candidates in {len(result)} lanes. A candidate is not a finding until a reviewer reads it.")
    return "\n".join(lines)


def render_candidate(candidate):
    text = f"- **{candidate['id']}** {candidate['detail']}"
    if candidate.get("worst") is not None:
        text += f" (worst run {lane_rules.fmt(candidate['worst'], candidate['unit'])})"
    if candidate.get("critical"):
        text += " (on the critical path)"
    if candidate["lane"] == "functions":
        text += f" ({candidate['samples']} samples, self {candidate['self_share'] * 100:.1f}%, inclusive {candidate['inclusive_share'] * 100:.1f}%)"
    if candidate.get("location"):
        text += f" at `{candidate['location']}`"
    for hot in candidate.get("hot") or []:
        place = f" at `{hot['location']}`" if hot.get("location") else ""
        text += f"\n  - hot: `{hot['name']}`{place}, self {hot['self'] * 100:.1f}%, inclusive {hot['inclusive'] * 100:.1f}%"
    for name, size in candidate.get("crates") or []:
        text += f"\n  - crate `{name}`: {size / (1024 * 1024):.2f} MiB"
    return text


def build_prompt(lane, chunk, results_file, reading, root):
    return prompts.PROMPT.format(
        lane=lane, first=chunk[0]["id"], last=chunk[-1]["id"], rule=prompts.RULE,
        question=prompts.QUESTION[lane], candidates="\n".join(render_candidate(c) for c in chunk),
        lane_notes=prompts.LANE_NOTES[lane], results_file=results_file, root=root, reading=reading,
        areas=", ".join(f"`{a}`" for a in (*AREAS, INFRASTRUCTURE)), finish=pipeline.FINISH,
    )


def prepare(root, lanes, work_dir, **options):
    """Measure, then write a prompt file for each group of candidates. Where the repository has a
    commit, the reviewers read a checkout of it in `work_dir/tree`."""
    commit = pipeline.read_commit(root)
    dirty = commit is not None and pipeline.has_uncommitted_changes(root)
    work_dir.mkdir(parents=True, exist_ok=True)
    result, rows = scan(root, lanes, scratch=work_dir / "scratch", **options)
    problem = pipeline.make_snapshot(root, commit, work_dir / "tree") if commit is not None else None
    tree = work_dir / "tree" if commit is not None and problem is None else None
    reading = pipeline.reading_for(root, tree, commit)
    manifest = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(), "root": str(root),
        "commit": commit, "tree": str(tree) if tree else None, "dirty": dirty, "lanes": [],
        "candidates": {}, "rows": rows, "progress_lines": [],
    }
    if tree:
        manifest["progress_lines"].append(f"reviewers read commit {commit[:12]} at {tree}")
        if dirty:
            manifest["progress_lines"].append(
                "tracked files have uncommitted changes: the measurements come from them, the reviewers read the commit"
            )
    elif commit is not None:
        manifest["progress_lines"].append(
            f"could not check commit {commit[:12]} out, so the reviewers read the working tree: " + " ".join(problem.split())
        )
    for lane, entry in result.items():
        candidates, prompt_list = entry["candidates"], []
        for start in range(0, len(candidates), CHUNK):
            chunk, number = candidates[start : start + CHUNK], start // CHUNK + 1
            prompt_file, results_file = work_dir / f"{lane}-{number}_prompt.md", work_dir / f"{lane}-{number}_results.json"
            prompt_file.write_text(build_prompt(lane, chunk, results_file, reading, root), encoding="utf-8")
            prompt_list.append({"prompt_file": str(prompt_file), "results_file": str(results_file),
                                "candidates": [c["id"] for c in chunk]})
        for candidate in candidates:
            manifest["candidates"][candidate["id"]] = candidate
        manifest["lanes"].append({"lane": lane, "candidates": len(candidates), "left_out": entry["left_out"],
                                  "reviewer_prompts": prompt_list})
        manifest["progress_lines"].append(
            f"{lane}: {len(candidates)} candidates for {len(prompt_list)} reviewers"
            + (f", {entry['carried']} already reviewed" if entry.get("carried") else "")
            + (f" ({entry['note']})" if entry.get("note") else "")
        )
    return result, workdir.write_manifest(work_dir, manifest)


def labels_for(finding, manifest):
    """`enhancement`, or `bug` where the reviewer found a defect, and an area where one is clear. A
    finding about the scripts lane alone is infrastructure."""
    labels = [BROKEN if finding.get("bug") else KIND]
    area = str(finding.get("area") or "").strip().strip("`")
    if area and not area.startswith("area/") and area != INFRASTRUCTURE:
        area = f"area/{area}"
    if area in AREAS or area == INFRASTRUCTURE:
        labels.append(area)
    elif finding["lane"] == "scripts":
        labels.append(INFRASTRUCTURE)
    return labels


def title_for(finding):
    return (finding.get("summary") or "Something measured slow or large").strip().rstrip(".").replace("`", "")


def body_for(finding, manifest):
    items = drafting.evidence_items(finding.get("evidence"))
    candidates = [manifest["candidates"][i] for i in finding["candidates"] if i in manifest["candidates"]]
    path, line = drafting.first_site(items)
    lines = ["## The problem", "", drafting.sentence(finding.get("summary") or ""), ""]
    if finding.get("detail"):
        lines += [drafting.sentence(finding["detail"]), ""]
    if candidates:
        lines += ["## What was measured", ""] + [render_candidate(c) for c in candidates]
        lines += ["", "One run on one machine, by `make check-profile`. Measure again before changing code.", ""]
    if path is not None:
        lines += drafting.code_at(path, line)
    if items:
        commit = manifest.get("commit")
        heading = f"Where it is, in commit `{commit[:12]}`:" if commit else "Where it is:"
        lines += [heading, ""] + [drafting.as_bullet(item) for item in items] + [""]
    lines += ["## The fix", "", drafting.sentence(finding.get("fix") or "") or "Not proposed.", ""]
    return "\n".join(lines)


def draft(manifest, findings, out):
    out.mkdir(parents=True, exist_ok=True)
    drafts, taken = [], set()
    for finding in findings:
        slug = pipeline.slug_for(finding, taken)
        body_file = out / f"{slug}.md"
        body_file.write_text(body_for(finding, manifest), encoding="utf-8")
        title = title_for(finding)
        drafts.append({
            "slug": slug, "title": title, "title_too_long": len(title) > TITLE_LIMIT, "lane": finding["lane"],
            "candidates": finding["candidates"], "labels": labels_for(finding, manifest),
            "assignee": ASSIGNEE, "body_file": str(body_file),
        })
    pipeline.mark_overlaps(drafts, findings)
    (out / pipeline.DRAFTS_FILE).write_text(json.dumps(drafts, indent=2), encoding="utf-8")
    return drafts


def record(manifest, verdicts, today=None):
    """Write every row this run measured to the ledger, with the verdict a reviewer gave it, and keep
    the entries of keys this run did not measure. Returns the path and how many rows were written."""
    root = Path(manifest["root"])
    today = today or datetime.date.today().isoformat()
    entries, _ = ledger.read(root)
    given = {c["key"]: verdicts.get(i, {}) for i, c in manifest["candidates"].items()}
    commit = (manifest.get("commit") or "none")[:12]
    for entry in manifest.get("rows", []):
        verdict = given.get(entry["key"], {})
        kind = verdict.get("verdict") if verdict.get("verdict") in ("keep", CHANGE) else "measured"
        entries[entry["key"]] = {"value": entry["value"], "date": today, "commit": commit,
                                 "verdict": kind, "reason": verdict.get("reason", "") if kind != "measured" else ""}
    return ledger.write(root, entries), len(manifest.get("rows", []))


def parse(argv):
    command = argv.pop(0) if argv and argv[0] in COMMANDS else "prepare"
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    if command == "prepare":
        parser.add_argument("scope", nargs="*", help="lane names; default every lane")
        parser.add_argument("--measure-only", action="store_true", help="print the candidates and stop")
        parser.add_argument("--work-dir")
        parser.add_argument("--limit", type=int, help=f"candidates per lane, default {LIMIT}")
        parser.add_argument("--fresh", action="store_true", help=f"review candidates {ledger.LEDGER} holds a verdict for")
        parser.add_argument("--root", default=str(ROOT))
    else:
        parser.add_argument("--work-dir", required=True)
        parser.add_argument("--out", help="default: WORK_DIR/issues")
        parser.add_argument("--json", action="store_true")
    return command, parser.parse_args(argv)


def main(argv=None):
    command, args = parse(list(sys.argv[1:] if argv is None else argv))
    if command == "prepare":
        root = Path(args.root).resolve()
        unknown = [s for s in args.scope if s not in LANES]
        if unknown:
            print(f"not a lane: {', '.join(unknown)}; the lanes are {', '.join(LANES)}", file=sys.stderr)
            return 2
        lanes = tuple(args.scope) or LANES
        os.chdir(root)
        previous, problems = ledger.read(root)
        for problem in problems:
            print(problem, file=sys.stderr)
        if args.measure_only:
            scratch = Path(tempfile.mkdtemp(prefix="profile-scratch-"))
            try:
                print(report(scan(root, lanes, scratch, previous=previous, fresh=args.fresh, limit=args.limit)[0]))
            finally:
                shutil.rmtree(scratch, ignore_errors=True)
            return 0
        work_dir = Path(args.work_dir or tempfile.mkdtemp(prefix="profile-"))
        result, manifest = prepare(root, lanes, work_dir, previous=previous, fresh=args.fresh, limit=args.limit)
        print(report(result), file=sys.stderr)
        print(json.dumps({"work_dir": str(work_dir), "manifest": str(manifest)}))
        return 0

    manifest = workdir.read_manifest(args.work_dir)
    if manifest is None:
        return 2
    out = Path(args.out).resolve() if args.out else Path(args.work_dir).resolve() / "issues"
    os.chdir(manifest["root"])
    if command == "verify":
        moved = pipeline.verify(manifest, out)
        print(json.dumps(moved, indent=2) if args.json else pipeline.render_verify(manifest, moved))
        return 1 if any(c["state"] != "moved" for d in moved or [] for c in d["cites"]) else 0
    if command == "claims-prepare":
        index = pipeline.claims_prepare(manifest, args.work_dir, out, prompts.READER_RULE)
        if index is None:
            print(f"no drafts in {out}: run draft first", file=sys.stderr)
            return 2
        print(json.dumps({"index": str(Path(args.work_dir).resolve() / pipeline.CLAIMS_DIR / "index.json"), **index}, indent=2))
        return 0
    if command == "claims-collect":
        drafts, read = pipeline.claim_drafts(out), pipeline.read_claims(args.work_dir)
        if drafts is None or read is None:
            print("run draft and claims-prepare first", file=sys.stderr)
            return 2
        verdicts, problems = read
        print(pipeline.render_drafts(drafts, out, verdicts))
        for problem in problems:
            print(f"claim check incomplete: {problem}", file=sys.stderr)
        return 1 if problems else 0
    if command == "clean":
        removed = pipeline.clean(manifest, args.work_dir)
        shutil.rmtree(Path(args.work_dir) / "scratch", ignore_errors=True)
        print("the reviewers' checkout is removed" if removed else f"did not remove {manifest['tree']}")
        return 0 if removed else 1
    findings, verdicts, problems = pipeline.load_reviews(manifest, CHANGE)
    if command == "status":
        rows = pipeline.reviewer_status(manifest, problems)
        print(pipeline.render_status(rows))
        return 1 if any(why is not None for _, why in rows) else 0
    if command == "collect":
        if args.json:
            print(json.dumps({"findings": findings, "verdicts": verdicts, "problems": problems}, indent=2))
        else:
            print(pipeline.render_review(manifest, findings, verdicts, problems, CHANGE))
        return 1 if problems else 0
    if command == "record":
        path, added = record(manifest, verdicts)
        print(f"{added} measurements written to {path}. Nothing has been committed.")
        return 0
    if manifest.get("tree") and Path(manifest["tree"]).is_dir():
        os.chdir(manifest["tree"])
    drafts = draft(manifest, findings, out)
    if args.json:
        print(json.dumps({"drafts": drafts, "out": str(out)}, indent=2))
    else:
        read = pipeline.read_claims(args.work_dir)
        print(pipeline.render_drafts(drafts, out, read[0] if read else None))
    return 0


if __name__ == "__main__":
    sys.exit(main())
