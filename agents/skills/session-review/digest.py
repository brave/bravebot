#!/usr/bin/env python3
"""A session's record and audit trail, counted by code and not read by a model.

`session-review.py pending` writes the result beside the record copy as `<id>.digest.json`, and both
prompts name it. The analyzer starts from these figures and opens the record for the turns they
point at. The verifier recomputes a figure it is asked to trust.

The digest holds numbers, tool names and fixed words. It holds no argument, prompt, path, result
text or refusal reason, so what a reader takes from it cannot carry the person's project into an
issue. A tool provided by an MCP server is counted as `mcp`, since a server's name for itself is
not this program's word.

What it counts, and from where:

- Turns are the ranges in `history[].start/end`, which index the conversation's archive followed by
  its live messages (crates/session/src/sessions.rs `StoredTurn`). A turn before a context reset has
  no messages in the record, so it has no counts from them.
- Tool calls are in `tool_calls` on assistant messages, not in the audit trail. The trail's
  `action_field` events cover a few file tools only.
- Permission mode, approvals and refusals are gates in the trail.
"""

import difflib
import json
import re
import statistics
from collections import Counter
from pathlib import Path

# Calls that read and change nothing. Two identical ones with no change between them returned the
# same bytes the second time.
READ_ONLY = frozenset({"read_file", "search", "list_files", "read_git", "read_output", "job_output", "repo_map"})
# Calls that change a file, after which an earlier read is no longer the same read.
CHANGES = frozenset({"edit_file", "write_file"})
# Rounds in a row of one read-only call each, from which a person could have batched them.
SERIAL_RUN = 3
BYPASS_ANSWER = "answered by bypass mode"
LEFT_TO_PERSON = "left to the confirmer"
# The `permission_mode` gate's detail: "the turn began in <mode> mode" (crates/core/src/policy.rs).
MODE = re.compile(r"\bbegan in (\S+) mode$")


def is_count(value):
    return isinstance(value, int) and not isinstance(value, bool)


def tool_name(call):
    """The word for a tool call: its name, or `mcp` for a tool a server provides."""
    function = call.get("function") if isinstance(call.get("function"), dict) else call
    name = function.get("name")
    if not isinstance(name, str) or not name:
        return "unknown"
    return "mcp" if name.startswith("mcp__") else name


def tool_arguments(call):
    function = call.get("function") if isinstance(call.get("function"), dict) else call
    arguments = function.get("arguments")
    return arguments if isinstance(arguments, str) else json.dumps(arguments, sort_keys=True)


def result_chars(message):
    content = message.get("content")
    if isinstance(content, str):
        return len(content)
    if isinstance(content, list):
        return sum(len(part.get("text", "")) for part in content if isinstance(part, dict) and isinstance(part.get("text"), str))
    return 0


def read_audit(path):
    """The events of an audit trail, skipping a line that is not one."""
    try:
        lines = Path(path).read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []
    events = []
    for line in lines:
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        if isinstance(entry, dict) and is_count(entry.get("turn")) and isinstance(entry.get("event"), dict):
            events.append(entry)
    return events


def turn_ranges(history, total):
    """The message range of each turn whose messages the record still holds.

    A turn that reset the context starts again at 0, and the turns before it belong to a context
    the record no longer has.
    """
    last_reset = max((h["number"] for h in history if h.get("reset_context") is True), default=0)
    ranges = {}
    for h in history:
        start, end = h.get("start"), h.get("end")
        if h["number"] < last_reset or not is_count(start) or not is_count(end) or end < start:
            continue
        ranges[h["number"]] = (min(start, total), min(end, total))
    return ranges


def calls_of(messages):
    """Every tool call in message order: its message index, tool word, arguments and result size."""
    sizes = {}
    for message in messages:
        if message.get("role") == "tool" and isinstance(message.get("tool_call_id"), str):
            sizes[message["tool_call_id"]] = result_chars(message)
    found = []
    for index, message in enumerate(messages):
        if message.get("role") != "assistant":
            continue
        for call in message.get("tool_calls") or []:
            if isinstance(call, dict):
                found.append(
                    {
                        "index": index,
                        "name": tool_name(call),
                        "arguments": tool_arguments(call),
                        "chars": sizes.get(call.get("id"), 0),
                    }
                )
    return found


def serial_read_rounds(rounds):
    """How many rounds sit in runs of at least SERIAL_RUN rounds that each made one read-only call."""
    total = run = 0
    for names in rounds + [None]:
        if names is not None and len(names) == 1 and names[0] in READ_ONLY:
            run += 1
            continue
        if run >= SERIAL_RUN:
            total += run
        run = 0
    return total


def audit_by_turn(events):
    """Mode, approvals and refusals per turn, and the trail's event counts."""
    turns = {}
    kinds = Counter()
    for entry in events:
        event = entry["event"]
        kind = event.get("kind")
        kinds[kind if isinstance(kind, str) else "unknown"] += 1
        row = turns.setdefault(entry["turn"], {"mode": None, "approvals": Counter(), "refusals": Counter(), "same": Counter(), "declassified": 0})
        gate, detail = event.get("gate"), event.get("detail")
        detail = detail if isinstance(detail, str) else ""
        if kind == "gate_passed" and gate == "permission_mode" and MODE.search(detail):
            row["mode"] = MODE.search(detail).group(1)
        elif kind == "gate_passed" and gate == "approval":
            row["approvals"]["bypass" if BYPASS_ANSWER in detail else "asked" if LEFT_TO_PERSON in detail else "other"] += 1
        elif kind == "gate_blocked":
            reason = event.get("reason")
            row["refusals"][gate if isinstance(gate, str) else "unknown"] += 1
            row["same"][(gate, reason if isinstance(reason, str) else "")] += 1
            if gate == "approval":
                row["approvals"]["refused"] += 1
        elif kind == "declassified":
            row["declassified"] += 1
    return turns, kinds


def similarity(before, after):
    """How alike two prompts are, from 0 to 1, as a number and nothing of their words."""
    if not isinstance(before, str) or not isinstance(after, str) or not before.strip() or not after.strip():
        return None
    return round(difflib.SequenceMatcher(None, before[:400].lower(), after[:400].lower()).ratio(), 2)


def build(record, events=None, from_turn=0):
    """The digest of a session record and its audit events (None where it has no trail)."""
    conversation = record.get("conversation") if isinstance(record.get("conversation"), dict) else {}
    archive = [m for m in conversation.get("archive") or [] if isinstance(m, dict)]
    messages = archive + [m for m in conversation.get("messages") or [] if isinstance(m, dict)]
    history = sorted(
        (h for h in record.get("history") or [] if isinstance(h, dict) and is_count(h.get("number"))),
        key=lambda h: h["number"],
    )
    ranges = turn_ranges(history, len(messages))
    owner = {}
    for number, (start, end) in ranges.items():
        for index in range(start, end):
            owner[index] = number

    spend = {int(k): v for k, v in (record.get("spend") or {}).items() if str(k).isdigit() and is_count(v)}
    timing = {int(k): v for k, v in (record.get("timing") or {}).items() if str(k).isdigit() and isinstance(v, dict)}
    todos = {int(k): v for k, v in (record.get("todos") or {}).items() if str(k).isdigit() and isinstance(v, list)}
    paid = [v for number, v in spend.items() if number > 0 and v > 0]
    median = statistics.median(paid) if paid else 0

    calls = calls_of(messages)
    by_turn = {number: Counter() for number in ranges}
    sizes = {number: [0, ("", 0)] for number in ranges}
    repeats = {number: Counter() for number in ranges}
    seen_reads, seen_runs = {}, {}
    for call in calls:
        number = owner.get(call["index"])
        name, key = call["name"], (call["name"], call["arguments"])
        if number is not None:
            by_turn[number][name] += 1
            sizes[number][0] += call["chars"]
            if call["chars"] > sizes[number][1][1]:
                sizes[number][1] = (name, call["chars"])
        if name in CHANGES:
            seen_reads.clear()
            seen_runs.clear()
        elif name == "run":
            if key in seen_runs and number is not None:
                repeats[number]["repeated_runs"] += 1
            seen_runs[key] = call["index"]
            seen_reads.clear()
        elif name in READ_ONLY:
            if key in seen_reads and number is not None:
                repeats[number]["repeated_reads"] += 1
                if seen_reads[key] < len(archive) <= call["index"]:
                    repeats[number]["reread_after_compaction"] += 1
            seen_reads[key] = call["index"]

    rounds = {number: [] for number in ranges}
    requests = Counter()
    for index, message in enumerate(messages):
        number = owner.get(index)
        if number is None or message.get("role") != "assistant":
            continue
        requests[number] += 1
        rounds[number].append([tool_name(c) for c in message.get("tool_calls") or [] if isinstance(c, dict)])

    trail, kinds = audit_by_turn(events) if events is not None else ({}, Counter())
    rows, previous_mode, changes, modes = [], None, [], Counter()
    for position, h in enumerate(history):
        number = h["number"]
        outcome = h.get("outcome") if isinstance(h.get("outcome"), dict) else {}
        clock = timing.get(number, {})
        audit = trail.get(number)
        row = {
            "turn": number,
            "new": number > from_turn,
            "outcome": outcome.get("kind") if isinstance(outcome.get("kind"), str) else "unknown",
            "context_reset": h.get("reset_context") is True,
            "tokens": spend.get(number),
            "times_median": round(spend[number] / median, 1) if median and number in spend else None,
            "wall_ms": clock.get("wall_ms"),
            "inference_ms": clock.get("inference_ms"),
            "tools_ms": clock.get("tools_ms"),
            "stalled_ms": clock.get("stalled_ms"),
            "file_wait_ms": clock.get("file_wait_ms"),
            "prompt_similarity_to_previous": similarity(history[position - 1].get("prompt"), h.get("prompt")) if position else None,
        }
        if number in ranges:
            row.update(
                {
                    "requests": requests[number],
                    "calls": dict(sorted(by_turn[number].items())),
                    "result_chars": sizes[number][0],
                    "largest_result": {"tool": sizes[number][1][0], "chars": sizes[number][1][1]},
                    "repeated_reads": repeats[number]["repeated_reads"],
                    "reread_after_compaction": repeats[number]["reread_after_compaction"],
                    "repeated_runs": repeats[number]["repeated_runs"],
                    "serial_read_rounds": serial_read_rounds(rounds[number]),
                }
            )
        tasks = Counter(t.get("status") for t in todos.get(number, []) if isinstance(t, dict))
        if tasks:
            row["tasks"] = dict(sorted((str(k), v) for k, v in tasks.items()))
        if audit is not None:
            mode = audit["mode"]
            row["mode"] = mode
            if mode is not None:
                modes[mode] += 1
                if previous_mode is not None and mode != previous_mode:
                    changes.append({"turn": number, "from": previous_mode, "to": mode})
                previous_mode = mode
            row["approvals"] = dict(sorted(audit["approvals"].items()))
            row["refusals"] = dict(sorted(audit["refusals"].items()))
            row["most_repeated_refusal"] = max(audit["same"].values(), default=0)
            row["declassified"] = audit["declassified"]
        rows.append(row)

    split = record.get("split") if isinstance(record.get("split"), dict) else {}
    cache = split.get("cache") if isinstance(split.get("cache"), dict) else None
    read = split.get("read") if is_count(split.get("read")) else None
    cache_read = cache.get("read_tokens") if cache and is_count(cache.get("read_tokens")) else 0
    outcomes = Counter(row["outcome"] for row in rows)
    last_tasks = Counter(t.get("status") for t in todos[max(todos)] if isinstance(t, dict)) if todos else Counter()
    absent = []
    if cache is None:
        absent.append("split.cache: the cached share was not recorded")
    if not timing:
        absent.append("timing")
    if events is None:
        absent.append("audit trail")
    if len(ranges) < len(history):
        absent.append("messages of the turns before a context reset")
    work = record.get("work") if isinstance(record.get("work"), dict) else None
    return {
        "session": record.get("id"),
        "turns": len(history),
        "from_turn": from_turn,
        "outcomes": dict(sorted(outcomes.items())),
        "tokens": record.get("tokens") if is_count(record.get("tokens")) else None,
        "read": read,
        "written": split.get("written") if is_count(split.get("written")) else None,
        "cached_share": round(cache_read / read, 2) if cache is not None and read else None,
        "median_turn_tokens": median,
        "archived_messages": len(archive),
        "live_messages": len(messages) - len(archive),
        "modes": dict(sorted(modes.items())),
        "mode_changes": changes,
        "tasks_in_last_list": dict(sorted((str(k), v) for k, v in last_tasks.items())),
        "work": {k: work[k] for k in ("writes", "commands", "ran_after_last_write", "last_command_failed") if k in work} if work else None,
        "audit_events": dict(sorted(kinds.items())),
        "absent": absent,
        "rows": rows,
    }


def write(record, audit_path, destination, from_turn=0):
    """Write the digest of `record` beside the copy of it, or raise ValueError if it cannot be built."""
    events = read_audit(audit_path) if audit_path is not None and Path(audit_path).is_file() else None
    try:
        digest = build(record, events, from_turn)
    except (KeyError, TypeError, AttributeError) as problem:
        raise ValueError(f"the record has a shape the digest does not read: {problem!r}") from problem
    Path(destination).write_text(json.dumps(digest, indent=1), encoding="utf-8")
    return digest
