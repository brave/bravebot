#!/usr/bin/env python3
"""Compare the token usage of coding agents on the same tasks, model and cache settings.

    bench.py list   [--config FILE]
    bench.py run    --config FILE [--only a,b] [--scenarios x,y] [--repeats N] [--out FILE]
    bench.py report RESULTS.jsonl [--plot DIR]
    bench.py --selftest

A contestant is one tool (bravebot, Claude Code, Codex, or the `reference` loop that talks to an
OpenAI-compatible endpoint such as OpenRouter) pointed at one model. Two contestants may be the
same tool on different models or routes, which is how Bedrock is compared with Brave's own
endpoint or with OpenRouter. See README.md.

Standard library only. matplotlib is imported by `report --plot` and nowhere else.
"""

import argparse
import hashlib
import http.server
import json
import math
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from dataclasses import dataclass, field

HERE = pathlib.Path(__file__).resolve().parent

# Price of each kind of token, in units of one uncached input token. A cached read costs a tenth
# of a fresh token on both Anthropic and OpenAI models. Anthropic charges a quarter more for a
# cache write and OpenAI charges nothing extra. Output is the usual list-price ratio. A contestant
# can name a profile or state its own numbers under `weights`.
WEIGHT_PROFILES = {
    "anthropic": {"write": 1.25, "read": 0.10, "output": 5.0},
    "openai": {"write": 1.0, "read": 0.10, "output": 8.0},
}


class BenchError(Exception):
    """A turn that could not be run or read. It is recorded as a failed turn, not raised further."""


@dataclass
class Usage:
    """What one turn cost, with every tool's accounting reduced to the same five numbers.

    `fresh` is prompt tokens that were neither read from nor written to a cache, so the prompt is
    the sum of the first three. `requests` counts model calls and is approximate for tools that
    do not report it.
    """

    fresh: int = 0
    cache_read: int = 0
    cache_write: int = 0
    output: int = 0
    requests: int = 0

    def __add__(self, other):
        return Usage(
            self.fresh + other.fresh,
            self.cache_read + other.cache_read,
            self.cache_write + other.cache_write,
            self.output + other.output,
            self.requests + other.requests,
        )

    @property
    def prompt(self):
        return self.fresh + self.cache_read + self.cache_write

    def cost(self, weights):
        return (
            self.fresh
            + weights["write"] * self.cache_write
            + weights["read"] * self.cache_read
            + weights["output"] * self.output
        )


@dataclass
class Turn:
    prompt: str
    check: object  # (workdir: Path, reply: str) -> bool


@dataclass
class Scenario:
    name: str
    category: str
    description: str
    setup: object  # (workdir: Path) -> None
    turns: list
    system_extra: str = ""


@dataclass
class TurnResult:
    usage: Usage
    reply: str = ""
    tool_calls: int = 0
    cost_usd: float = 0.0


# ---------------------------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------------------------


def _says(text):
    return lambda workdir, reply: text.lower() in reply.lower()


def _setup_read_count(workdir):
    for name, lines in (("a.txt", 7), ("b.txt", 11), ("c.txt", 13)):
        (workdir / name).write_text("".join(f"{name} line {i}\n" for i in range(lines)))


CALC_PY = """def median(values):
    s = sorted(values)
    n = len(s)
    mid = n // 2
    if n % 2 == 0:
        return s[mid]
    return s[mid]
"""

TEST_CALC_PY = """from calc import median

assert median([1, 3, 2]) == 2
assert median([4, 1, 3, 2]) == 2.5
assert median([5]) == 5
print("OK")
"""


def _setup_fix_bug(workdir):
    (workdir / "calc.py").write_text(CALC_PY)
    (workdir / "test_calc.py").write_text(TEST_CALC_PY)


def _check_fix_bug(workdir, reply):
    if (workdir / "test_calc.py").read_text() != TEST_CALC_PY:
        return False
    done = subprocess.run(
        [sys.executable, "test_calc.py"], cwd=workdir, capture_output=True, text=True, timeout=30
    )
    return done.returncode == 0 and "OK" in done.stdout


def _setup_find_symbol(workdir):
    package = workdir / "pkg"
    package.mkdir()
    for n in range(40):
        body = "".join(f"def helper_{n:02d}_{k}(x):\n    return x + {n * 3 + k}\n\n" for k in "abc")
        if n == 27:
            body += "def reconcile_ledger_7f3a(entries):\n    return sorted(entries)\n"
        (package / f"mod_{n:02d}.py").write_text(body)


def _note_value(n):
    return hashlib.md5(f"item-{n}".encode()).hexdigest()[:8]


def _setup_notes(workdir):
    (workdir / "notes.txt").write_text(
        "".join(f"item-{n:03d}: value={_note_value(n)}\n" for n in range(120))
    )


def _asks_value(n):
    return Turn(
        f"What is the value for item-{n:03d}? Reply with the value only.",
        _says(_note_value(n)),
    )


def _style_guide(rules):
    return "Follow these house rules in every reply.\n" + "".join(
        f"Rule {n}: when a request touches area {n}, state the area number once and keep the "
        f"wording plain.\n"
        for n in range(1, rules + 1)
    )


def scenarios():
    two_word_turns = [
        Turn("Reply with exactly the word: ok", _says("ok")),
        Turn("Reply with exactly the word: done", _says("done")),
    ]
    return [
        Scenario(
            "hello", "overhead",
            "One trivial turn. Measures the fixed cost of the system prompt and tool schemas.",
            lambda workdir: None,
            [Turn("Reply with exactly the word: ok", _says("ok"))],
        ),
        Scenario(
            "read_count", "tools",
            "Read three files and add up their lines. Two or three parallel or serial reads.",
            _setup_read_count,
            [Turn(
                "Read a.txt, b.txt and c.txt. Reply with only the total number of lines in the "
                "three files.",
                _says("31"),
            )],
        ),
        Scenario(
            "fix_bug", "tools",
            "Find a bug from a failing script, edit the file and run the script again.",
            _setup_fix_bug,
            [Turn(
                "test_calc.py fails. Fix the bug in calc.py without editing the test, then run "
                "the test to confirm.",
                _check_fix_bug,
            )],
        ),
        Scenario(
            "find_symbol", "tools",
            "Search forty small files for one definition. Rewards search over reading everything.",
            _setup_find_symbol,
            [Turn(
                "Which file defines reconcile_ledger_7f3a? Reply with only the file name.",
                _says("mod_27.py"),
            )],
        ),
        Scenario(
            "multi_turn", "cache",
            "Five turns over one growing conversation. Measures how much of each later request is "
            "read from the cache.",
            _setup_notes,
            [
                Turn("Read notes.txt and reply with the single word ok.", _says("ok")),
                _asks_value(17), _asks_value(88), _asks_value(3), _asks_value(111),
            ],
        ),
        Scenario(
            "sysprompt_short", "system_prompt",
            "Two turns with a ten-rule addition to the system prompt.",
            lambda workdir: None, two_word_turns, system_extra=_style_guide(10),
        ),
        Scenario(
            "sysprompt_long", "system_prompt",
            "Two turns with a two-thousand-token addition to the system prompt.",
            lambda workdir: None, two_word_turns, system_extra=_style_guide(150),
        ),
    ]


# ---------------------------------------------------------------------------------------------
# Reading each tool's output
# ---------------------------------------------------------------------------------------------


def last_json_line(text):
    for line in reversed(text.strip().splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    raise BenchError("no JSON object on stdout")


def parse_claude(stdout):
    """Claude Code `--output-format json`. Its usage sums every request the invocation made."""
    result = last_json_line(stdout)
    if result.get("is_error"):
        raise BenchError(f"claude reported an error: {str(result.get('result'))[:200]}")
    u = result["usage"]
    usage = Usage(
        fresh=u.get("input_tokens", 0),
        cache_read=u.get("cache_read_input_tokens", 0),
        cache_write=u.get("cache_creation_input_tokens", 0),
        output=u.get("output_tokens", 0),
        requests=result.get("num_turns", 1),
    )
    calls = max(result.get("num_turns", 1) - 1, 0)
    reply = result.get("result") or ""
    return TurnResult(usage, reply, calls, result.get("total_cost_usd") or 0.0), result["session_id"]


def parse_bravebot(stdout):
    """bravebot `--json`. `total` is prompt plus output, so the prompt is what is left of it."""
    result = last_json_line(stdout)
    if not result.get("ok"):
        raise BenchError(f"bravebot failed: {result.get('message')}")
    t = result["tokens"]
    fresh = t["total"] - t["output"] - t["cache_read"] - t["cache_written"]
    usage = Usage(
        fresh=max(fresh, 0),
        cache_read=t["cache_read"],
        cache_write=t["cache_written"],
        output=t["output"],
        requests=result.get("steps", 0) + 1,
    )
    return TurnResult(usage, result.get("reply") or "", len(result.get("calls", []))), result.get("session")


CODEX_COUNTERS = ("input_tokens", "cached_input_tokens", "cache_write_input_tokens", "output_tokens")


def parse_codex(stdout, previous):
    """Codex `exec --json` events.

    The usage in `turn.completed` is the total for the whole thread, so a resumed turn reports
    everything before it again. `previous` is the total after the earlier turn, and the turn's own
    cost is the difference. `input_tokens` includes the cached part.
    """
    thread = None
    reply = ""
    calls = 0
    total = None
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        kind = event.get("type")
        if kind == "thread.started":
            thread = event.get("thread_id")
        elif kind == "turn.completed":
            total = event.get("usage") or {}
        elif kind == "item.completed":
            item = event.get("item", {})
            if item.get("type") == "agent_message":
                reply = item.get("text", "")
            elif item.get("type") not in ("reasoning",):
                calls += 1
        elif kind in ("turn.failed", "error"):
            raise BenchError(f"codex reported {kind}: {json.dumps(event)[:200]}")
    if total is None:
        raise BenchError("no turn.completed event")
    now = {k: total.get(k, 0) for k in CODEX_COUNTERS}
    delta = {k: now[k] - previous.get(k, 0) for k in CODEX_COUNTERS}
    read = delta["cached_input_tokens"]
    write = delta["cache_write_input_tokens"]
    usage = Usage(
        fresh=max(delta["input_tokens"] - read - write, 0),
        cache_read=read,
        cache_write=write,
        output=delta["output_tokens"],
        requests=calls + 1,
    )
    return TurnResult(usage, reply, calls), thread, now


# ---------------------------------------------------------------------------------------------
# The reference loop: a minimal agent straight against an OpenAI-compatible endpoint
# ---------------------------------------------------------------------------------------------

REF_SYSTEM = (
    "You are a coding agent working in the current directory. Use the tools to read and change "
    "files and to run commands. Be brief."
)

REF_TOOLS = [
    {"type": "function", "function": {
        "name": "read_file", "description": "Read a text file.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}},
                       "required": ["path"]}}},
    {"type": "function", "function": {
        "name": "write_file", "description": "Write a text file, replacing it.",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string"}, "content": {"type": "string"}},
            "required": ["path", "content"]}}},
    {"type": "function", "function": {
        "name": "bash", "description": "Run a shell command and return its output.",
        "parameters": {"type": "object", "properties": {"command": {"type": "string"}},
                       "required": ["command"]}}},
]


def ref_tool(workdir, name, args):
    def inside(path):
        target = (workdir / path).resolve()
        if workdir.resolve() not in (target, *target.parents):
            raise BenchError("path outside the working directory")
        return target

    try:
        if name == "read_file":
            return inside(args["path"]).read_text()[:20000]
        if name == "write_file":
            inside(args["path"]).write_text(args["content"])
            return "written"
        if name == "bash":
            done = subprocess.run(args["command"], shell=True, cwd=workdir, capture_output=True,
                                  text=True, timeout=60)
            return (done.stdout + done.stderr)[:8000] or f"exit {done.returncode}"
    except (OSError, BenchError, subprocess.TimeoutExpired, KeyError) as error:
        return f"error: {error}"
    return f"error: no tool named {name}"


def ref_messages(system, history):
    """The request's messages, with a cache mark on the system prompt and on the last message."""

    def blocks(text, mark):
        block = {"type": "text", "text": text}
        if mark:
            block["cache_control"] = {"type": "ephemeral"}
        return [block]

    out = [{"role": "system", "content": blocks(system, True)}]
    for i, message in enumerate(history):
        message = dict(message)
        last = i == len(history) - 1
        if last and message["role"] in ("user", "tool") and isinstance(message["content"], str):
            message["content"] = blocks(message["content"], True)
        out.append(message)
    return out


def ref_turn(contestant, workdir, prompt, state, extra_system):
    key = os.environ.get(contestant.get("key_env", "OPENROUTER_API_KEY"), "")
    if not key:
        raise BenchError("the reference contestant needs its API key variable set")
    base = contestant.get("base_url", "https://openrouter.ai/api/v1").rstrip("/")
    history = state.setdefault("messages", [])
    history.append({"role": "user", "content": prompt})
    system = REF_SYSTEM + ("\n\n" + extra_system if extra_system else "")
    total, calls, reply = Usage(), 0, ""
    for _ in range(contestant.get("max_requests", 25)):
        body = {"model": contestant["model"], "tools": REF_TOOLS, "max_tokens": 4096,
                "messages": ref_messages(system, history), "usage": {"include": True}}
        request = urllib.request.Request(
            base + "/chat/completions", json.dumps(body).encode(),
            {"Authorization": "Bearer " + key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=180) as response:
                answer = json.load(response)
        except OSError as error:
            raise BenchError(f"request failed: {error}") from error
        u = answer.get("usage", {})
        details = u.get("prompt_tokens_details") or {}
        read = details.get("cached_tokens") or 0
        write = details.get("cache_write_tokens") or 0
        total += Usage(max(u.get("prompt_tokens", 0) - read - write, 0), read, write,
                       u.get("completion_tokens", 0), 1)
        message = answer["choices"][0]["message"]
        history.append({k: v for k, v in message.items() if k in ("role", "content", "tool_calls")}
                       | {"content": message.get("content") or ""})
        if not message.get("tool_calls"):
            reply = message.get("content") or ""
            break
        for call in message["tool_calls"]:
            calls += 1
            try:
                args = json.loads(call["function"]["arguments"] or "{}")
            except json.JSONDecodeError:
                args = {}
            history.append({"role": "tool", "tool_call_id": call["id"],
                            "content": ref_tool(workdir, call["function"]["name"], args)})
    else:
        raise BenchError("the reference loop hit its request limit")
    return TurnResult(total, reply, calls)


# ---------------------------------------------------------------------------------------------
# Running a turn
# ---------------------------------------------------------------------------------------------


def expand(value):
    return re.sub(r"\$\{(\w+)\}", lambda m: os.environ.get(m.group(1), ""), value)


def execute(cmd, workdir, contestant):
    env = dict(os.environ)
    env.update({k: expand(v) for k, v in contestant.get("env", {}).items()})
    try:
        done = subprocess.run(cmd, cwd=workdir, env=env, capture_output=True, text=True,
                              timeout=contestant.get("timeout", 600), stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired as error:
        raise BenchError("timed out") from error
    except OSError as error:
        raise BenchError(f"could not start {cmd[0]}: {error}") from error
    if done.returncode != 0 and not done.stdout.strip():
        raise BenchError(f"exit {done.returncode}: {done.stderr.strip()[-200:]}")
    return done.stdout


def run_turn(contestant, workdir, prompt, state, extra_system, single):
    tool = contestant["tool"]
    model = contestant.get("model")
    extra = list(contestant.get("args", []))
    if tool == "reference":
        return ref_turn(contestant, workdir, prompt, state, extra_system)
    if tool == "bravebot":
        cmd = [contestant.get("bin", "bravebot"), "-p", prompt, "--json", "--trust-workspace",
               "--dangerously-skip-permissions"]
        if single:
            cmd.append("--incognito")
        elif state.get("session"):
            cmd += ["--resume", state["session"]]
        if model:
            cmd += ["--model", model]
        if extra_system:
            cmd += ["--append-system-prompt", extra_system]
        result, session = parse_bravebot(execute(cmd + extra, workdir, contestant))
        state["session"] = session
        return result
    if tool == "claude":
        cmd = [contestant.get("bin", "claude"), "-p", prompt, "--output-format", "json",
               "--dangerously-skip-permissions"]
        if single:
            cmd.append("--no-session-persistence")
        elif state.get("session"):
            cmd += ["--resume", state["session"]]
        if model:
            cmd += ["--model", model]
        if extra_system:
            cmd += ["--append-system-prompt", extra_system]
        result, session = parse_claude(execute(cmd + extra, workdir, contestant))
        state["session"] = session
        return result
    if tool == "codex":
        flags = ["--json", "--skip-git-repo-check", "--dangerously-bypass-approvals-and-sandbox"]
        if model:
            flags += ["-m", model]
        if extra_system:
            flags += ["-c", "developer_instructions=" + json.dumps(extra_system)]
        flags += extra
        if state.get("session"):
            cmd = [contestant.get("bin", "codex"), "exec", "resume", *flags, state["session"], prompt]
        else:
            cmd = [contestant.get("bin", "codex"), "exec", *flags]
            if single:
                cmd.append("--ephemeral")
            cmd.append(prompt)
        result, thread, now = parse_codex(execute(cmd, workdir, contestant),
                                          state.get("totals", {}))
        if not single:
            state["session"], state["totals"] = thread, now
        return result
    raise BenchError(f"unknown tool {tool}")


def weights_of(contestant):
    chosen = contestant.get("weights")
    if isinstance(chosen, dict):
        return chosen
    default = "openai" if contestant["tool"] == "codex" else "anthropic"
    return WEIGHT_PROFILES[chosen or default]


def run_scenario(contestant, scenario, rep, sink):
    # The path is the same on every run: tools put the working directory in the system prompt, and
    # a path that changed would change the cached prefix and hide cross-run cache reuse.
    workdir = pathlib.Path(tempfile.gettempdir()) / "tokbench" / scenario.name
    shutil.rmtree(workdir, ignore_errors=True)
    workdir.mkdir(parents=True)
    state = {}
    single = len(scenario.turns) == 1
    weights = weights_of(contestant)
    try:
        scenario.setup(workdir)
        for index, turn in enumerate(scenario.turns):
            started = time.monotonic()
            record = {"contestant": contestant["name"], "scenario": scenario.name,
                      "category": scenario.category, "rep": rep, "turn": index}
            try:
                result = run_turn(contestant, workdir, turn.prompt, state,
                                  scenario.system_extra, single)
                ok = bool(turn.check(workdir, result.reply))
                record.update(ok=ok, reply=result.reply[:200], tool_calls=result.tool_calls,
                              fresh=result.usage.fresh, cache_read=result.usage.cache_read,
                              cache_write=result.usage.cache_write, output=result.usage.output,
                              requests=result.usage.requests,
                              cost=result.usage.cost(weights), cost_usd=result.cost_usd)
            except BenchError as error:
                record.update(ok=False, error=str(error), tool_calls=0, fresh=0, cache_read=0,
                              cache_write=0, output=0, requests=0, cost=0.0, cost_usd=0.0)
            record["wall_s"] = round(time.monotonic() - started, 2)
            sink(record)
            if "error" in record:
                break
    finally:
        if contestant["tool"] == "bravebot" and not single:
            subprocess.run([contestant.get("bin", "bravebot"), "sessions", "purge", "-y"],
                           cwd=workdir, capture_output=True, stdin=subprocess.DEVNULL)
        shutil.rmtree(workdir, ignore_errors=True)


# ---------------------------------------------------------------------------------------------
# Scoring and reporting
# ---------------------------------------------------------------------------------------------


def load_records(path):
    with open(path) as handle:
        return [json.loads(line) for line in handle if line.strip()]


def per_run(records, expected_turns):
    """One row per (contestant, scenario, rep), summed over its turns.

    A run that stopped early or has a failed turn counts as failed, whatever the turns before it
    cost.
    """
    runs = {}
    for r in records:
        runs.setdefault((r["contestant"], r["scenario"], r["rep"]), []).append(r)
    rows = []
    for (contestant, scenario, rep), turns in runs.items():
        total = {k: sum(t[k] for t in turns) for k in
                 ("fresh", "cache_read", "cache_write", "output", "requests", "tool_calls",
                  "cost", "cost_usd", "wall_s")}
        total["ok"] = len(turns) == expected_turns.get(scenario, len(turns)) and all(
            t["ok"] for t in turns)
        total.update(contestant=contestant, scenario=scenario, rep=rep,
                     category=turns[0]["category"])
        rows.append(total)
    return rows


def summarize(records):
    expected = {s.name: len(s.turns) for s in scenarios()}
    cells = {}
    for row in per_run(records, expected):
        cells.setdefault((row["contestant"], row["scenario"]), []).append(row)
    table = {}
    for key, rows in cells.items():
        med = lambda name: statistics.median(r[name] for r in rows)
        prompt = med("fresh") + med("cache_read") + med("cache_write")
        table[key] = {
            "category": rows[0]["category"], "runs": len(rows),
            "success": sum(r["ok"] for r in rows) / len(rows),
            "cost": med("cost"), "prompt": prompt, "fresh": med("fresh"),
            "cache_read": med("cache_read"), "cache_write": med("cache_write"),
            "output": med("output"), "requests": med("requests"),
            "tool_calls": med("tool_calls"), "wall_s": med("wall_s"),
            "hit": med("cache_read") / prompt if prompt else 0.0,
        }
    # Score each scenario against the cheapest contestant that solved it every time.
    for scenario in {s for _, s in table}:
        cells_here = {c: v for (c, s), v in table.items() if s == scenario}
        solved = [v["cost"] for v in cells_here.values() if v["success"] == 1 and v["cost"] > 0]
        best = min(solved) if solved else None
        for v in cells_here.values():
            v["score"] = 0.0 if not best or v["cost"] <= 0 else 100.0 * best / v["cost"] * v["success"]
    return table


def standings(table):
    names = sorted({c for c, _ in table})
    out = []
    for name in names:
        mine = {s: v for (c, s), v in table.items() if c == name}
        by_category = {}
        for v in mine.values():
            by_category.setdefault(v["category"], []).append(v["score"])
        prompt = sum(v["prompt"] for v in mine.values())
        out.append({
            "contestant": name,
            "score": statistics.mean(v["score"] for v in mine.values()),
            "categories": {k: statistics.mean(x) for k, x in by_category.items()},
            "scenarios": len(mine),
            "success": statistics.mean(v["success"] for v in mine.values()),
            "cost": sum(v["cost"] for v in mine.values()),
            "hit": sum(v["cache_read"] for v in mine.values()) / prompt if prompt else 0.0,
        })
    return sorted(out, key=lambda row: -row["score"])


def render_report(table):
    lines = []
    ranked = standings(table)
    categories = sorted({c for row in ranked for c in row["categories"]})
    header = ["rank", "contestant", "score", *categories, "solved", "cache hit", "cost units"]
    lines.append("  ".join(f"{h:>14}" if i > 1 else f"{h:<{4 if i == 0 else 22}}"
                           for i, h in enumerate(header)))
    for rank, row in enumerate(ranked, 1):
        cells = [f"{rank:<4}", f"{row['contestant']:<22}", f"{row['score']:>14.1f}"]
        cells += [f"{row['categories'].get(c, float('nan')):>14.1f}" for c in categories]
        cells += [f"{row['success'] * 100:>13.0f}%", f"{row['hit'] * 100:>13.0f}%",
                  f"{row['cost']:>14.0f}"]
        lines.append("  ".join(cells))
    lines.append("")
    lines.append("per scenario (median over repeats): cost units, prompt tokens, cache hit, "
                 "requests, solved")
    for scenario in sorted({s for _, s in table}):
        lines.append(f"  {scenario}")
        here = sorted(((c, v) for (c, s), v in table.items() if s == scenario),
                      key=lambda item: -item[1]["score"])
        for name, v in here:
            lines.append(f"    {name:<22} score {v['score']:>5.1f}  cost {v['cost']:>9.0f}  "
                         f"prompt {v['prompt']:>8.0f}  hit {v['hit'] * 100:>3.0f}%  "
                         f"req {v['requests']:>3.0f}  solved {v['success'] * 100:>3.0f}%")
    return "\n".join(lines)


def plot(records, table, out_dir):
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        sys.exit("plotting needs matplotlib: python3 -m pip install matplotlib")
    out_dir = pathlib.Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    ranked = standings(table)
    names = [row["contestant"] for row in ranked]
    categories = sorted({c for row in ranked for c in row["categories"]})

    fig, ax = plt.subplots(figsize=(max(7, 1.6 * len(names)), 5))
    width = 0.8 / (len(categories) + 1)
    for i, label in enumerate(["overall", *categories]):
        values = [row["score"] if label == "overall" else row["categories"].get(label, 0)
                  for row in ranked]
        ax.bar([x + i * width for x in range(len(names))], values, width, label=label)
    ax.set_xticks([x + 0.4 - width / 2 for x in range(len(names))], names, rotation=20, ha="right")
    ax.set_ylabel("score (100 = cheapest that solved it)")
    ax.set_title("Token cost score by contestant")
    ax.legend()
    fig.tight_layout()
    fig.savefig(out_dir / "scores.png", dpi=140)
    plt.close(fig)

    scenario_names = sorted({s for _, s in table})
    cols = min(4, len(scenario_names))
    rows = math.ceil(len(scenario_names) / cols)
    fig, axes = plt.subplots(rows, cols, figsize=(4.2 * cols, 3.6 * rows), squeeze=False)
    parts = [("fresh", "tab:red"), ("cache_write", "tab:orange"), ("cache_read", "tab:green"),
             ("output", "tab:blue")]
    for ax, scenario in zip(axes.flat, scenario_names):
        bottoms = [0] * len(names)
        for part, color in parts:
            heights = [table.get((n, scenario), {}).get(part, 0) for n in names]
            ax.bar(names, heights, bottom=bottoms, color=color, label=part)
            bottoms = [b + h for b, h in zip(bottoms, heights)]
        ax.set_title(scenario)
        ax.tick_params(axis="x", rotation=40)
    for ax in list(axes.flat)[len(scenario_names):]:
        ax.axis("off")
    axes.flat[0].legend(fontsize=7)
    fig.suptitle("Tokens per scenario, by kind (median)")
    fig.tight_layout()
    fig.savefig(out_dir / "tokens.png", dpi=140)
    plt.close(fig)

    by_turn = {}
    for r in records:
        if r["scenario"] == "multi_turn" and "error" not in r:
            by_turn.setdefault((r["contestant"], r["turn"]), []).append(r)
    if by_turn:
        fig, ax = plt.subplots(figsize=(7, 4.5))
        for name in names:
            points = sorted((t, statistics.median(
                x["cache_read"] / max(x["fresh"] + x["cache_read"] + x["cache_write"], 1)
                for x in rs) * 100) for (n, t), rs in by_turn.items() if n == name)
            if points:
                ax.plot([p[0] + 1 for p in points], [p[1] for p in points], marker="o", label=name)
        ax.set_xlabel("turn")
        ax.set_ylabel("prompt read from cache (%)")
        ax.set_ylim(0, 100)
        ax.set_title("Cache reuse across a five-turn conversation")
        ax.legend()
        fig.tight_layout()
        fig.savefig(out_dir / "cache_by_turn.png", dpi=140)
        plt.close(fig)
    return sorted(p.name for p in out_dir.glob("*.png"))


# ---------------------------------------------------------------------------------------------
# Command line
# ---------------------------------------------------------------------------------------------


def load_config(path):
    config = json.loads(pathlib.Path(path).read_text())
    seen = set()
    for c in config["contestants"]:
        if c["name"] in seen:
            sys.exit(f"contestant {c['name']} is listed twice")
        seen.add(c["name"])
    return config["contestants"]


def pick(items, names, label):
    if not names:
        return items
    wanted = names.split(",")
    keys = [i["name"] if isinstance(i, dict) else i.name for i in items]
    missing = [w for w in wanted if w not in keys]
    if missing:
        sys.exit(f"no {label} named {', '.join(missing)}; have {', '.join(keys)}")
    return [i for i, k in zip(items, keys) if k in wanted]


def cmd_run(args):
    contestants = pick(load_config(args.config), args.only, "contestant")
    chosen = pick(scenarios(), args.scenarios, "scenario")
    out = pathlib.Path(args.out or HERE / "results" / time.strftime("%Y%m%d-%H%M%S.jsonl"))
    out.parent.mkdir(parents=True, exist_ok=True)
    handle = open(out, "a")

    def sink(record):
        handle.write(json.dumps(record) + "\n")
        handle.flush()
        mark = "ok  " if record["ok"] else "FAIL"
        print(f"{mark} {record['contestant']:<22} {record['scenario']:<16} rep {record['rep']} "
              f"turn {record['turn']} cost {record['cost']:>8.0f} "
              f"{record.get('error', '')}", file=sys.stderr)

    if not args.no_warmup:
        hello = scenarios()[0]
        for c in contestants:
            run_scenario(c, hello, -1, lambda record: None)
    for rep in range(args.repeats):
        for scenario in chosen:
            for c in contestants:
                run_scenario(c, scenario, rep, sink)
    handle.close()
    print(f"wrote {out}", file=sys.stderr)
    table = summarize(load_records(out))
    print(render_report(table))


def cmd_report(args):
    records = load_records(args.results)
    table = summarize(records)
    print(render_report(table))
    if args.plot:
        for name in plot(records, table, args.plot):
            print(f"wrote {pathlib.Path(args.plot) / name}")


def cmd_list(args):
    for c in load_config(args.config):
        print(f"{c['name']:<24} {c['tool']:<10} {c.get('model') or '(tool default)'}")
    print()
    for s in scenarios():
        print(f"{s.name:<18} {s.category:<14} {len(s.turns)} turn(s)  {s.description}")


# ---------------------------------------------------------------------------------------------
# Selftest. The samples are output captured from the real tools, so a parser that reads the
# wrong field fails on numbers that were checked by hand against each tool's own totals.
# ---------------------------------------------------------------------------------------------

CLAUDE_SAMPLE = json.dumps({
    "type": "result", "is_error": False, "num_turns": 3, "session_id": "s-1",
    "total_cost_usd": 0.0312, "result": "The total is **3 lines**",
    "usage": {"input_tokens": 4, "cache_creation_input_tokens": 8907,
              "cache_read_input_tokens": 30287, "output_tokens": 591}})

BRAVEBOT_SAMPLE = json.dumps({
    "schema": 1, "ok": True, "status": 0, "reply": "The total is 3 lines.", "session": "b-1",
    "steps": 1, "tokens": {"total": 70020, "output": 257, "context": 35014,
                           "cache_read": 34745, "cache_written": 35012},
    "calls": [{"tool": "read_file", "target": "a.txt", "refused": False},
              {"tool": "read_file", "target": "b.txt", "refused": False}]})


def codex_events(thread, tool_items, input_tokens, cached, output):
    events = [{"type": "thread.started", "thread_id": thread}, {"type": "turn.started"}]
    events += [{"type": "item.completed", "item": {"id": f"i{n}", "type": "command_execution"}}
               for n in range(tool_items)]
    events += [{"type": "item.completed", "item": {"type": "agent_message", "text": "PLUM"}},
               {"type": "turn.completed", "usage": {
                   "input_tokens": input_tokens, "cached_input_tokens": cached,
                   "cache_write_input_tokens": 0, "output_tokens": output,
                   "reasoning_output_tokens": 0}}]
    return "\n".join(json.dumps(e) for e in events)


class FakeEndpoint(http.server.BaseHTTPRequestHandler):
    """Answers a read_file tool call first and text second, reporting cache figures."""

    seen = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        FakeEndpoint.seen.append(body)
        first = len(FakeEndpoint.seen) == 1
        if first:
            message = {"role": "assistant", "content": None, "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "read_file", "arguments": json.dumps({"path": "a.txt"})}}]}
            usage = {"prompt_tokens": 1000, "completion_tokens": 20,
                     "prompt_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 900}}
        else:
            message = {"role": "assistant", "content": "contents: hello"}
            usage = {"prompt_tokens": 1100, "completion_tokens": 5,
                     "prompt_tokens_details": {"cached_tokens": 900, "cache_write_tokens": 100}}
        payload = json.dumps({"choices": [{"message": message}], "usage": usage}).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):
        pass


def selftest():
    # Claude: the invocation's usage is already summed, and the prompt is the three parts added.
    claude, session = parse_claude(CLAUDE_SAMPLE)
    assert (claude.usage.fresh, claude.usage.cache_write, claude.usage.cache_read,
            claude.usage.output) == (4, 8907, 30287, 591), claude
    assert claude.usage.prompt == 39198 and claude.tool_calls == 2 and session == "s-1"

    # bravebot: total includes output, so fresh is the remainder and must not go negative.
    brave, session = parse_bravebot(BRAVEBOT_SAMPLE)
    assert (brave.usage.fresh, brave.usage.cache_read, brave.usage.cache_write,
            brave.usage.output) == (6, 34745, 35012, 257), brave
    assert brave.usage.prompt == 69763 and brave.tool_calls == 2 and brave.usage.requests == 2

    # codex: a resumed turn reports the thread's running total, so the second turn's cost is the
    # difference. Reading it raw would charge the second turn for the first as well.
    first, thread, totals = parse_codex(codex_events("t-1", 0, 17112, 13952, 5), {})
    assert (first.usage.fresh, first.usage.cache_read, first.usage.output) == (3160, 13952, 5)
    second, _, totals = parse_codex(codex_events("t-1", 0, 34249, 30976, 11), totals)
    assert (second.usage.fresh, second.usage.cache_read, second.usage.output) == (
        34249 - 17112 - (30976 - 13952), 30976 - 13952, 6), second
    assert second.usage.prompt == 34249 - 17112
    third, _, _ = parse_codex(codex_events("t-1", 1, 51404, 48000, 17), totals)
    assert third.usage.prompt == 51404 - 34249 and third.tool_calls == 1

    # Cost: one cached token must cost less than one fresh one and a write more, for anthropic.
    w = WEIGHT_PROFILES["anthropic"]
    assert Usage(fresh=100).cost(w) == 100
    assert Usage(cache_read=100).cost(w) == 10 and Usage(cache_write=100).cost(w) == 125
    assert Usage(output=10).cost(w) == 50

    # Scoring: the cheapest solver scores 100, a pricier one proportionally less, and a contestant
    # that failed scores 0 however cheap it was.
    def rec(name, cost, ok, scenario="hello", rep=0, turn=0):
        return {"contestant": name, "scenario": scenario, "category": "overhead", "rep": rep,
                "turn": turn, "ok": ok, "fresh": cost, "cache_read": 0, "cache_write": 0,
                "output": 0, "requests": 1, "tool_calls": 0, "cost": float(cost),
                "cost_usd": 0.0, "wall_s": 1.0}

    table = summarize([rec("a", 100, True), rec("b", 200, True), rec("cheat", 10, False)])
    assert table[("a", "hello")]["score"] == 100.0
    assert table[("b", "hello")]["score"] == 50.0
    assert table[("cheat", "hello")]["score"] == 0.0
    assert [row["contestant"] for row in standings(table)] == ["a", "b", "cheat"]

    # A multi-turn run missing its last turn is a failure, not a cheap success.
    partial = summarize([rec("a", 100, True, "multi_turn", turn=t) for t in range(4)])
    assert partial[("a", "multi_turn")]["success"] == 0.0

    # The median over repeats resists one cold run.
    repeats = summarize([rec("a", c, True, rep=r) for r, c in enumerate((900, 100, 100))])
    assert repeats[("a", "hello")]["cost"] == 100

    # Scenario checks accept the right answer and reject a near miss.
    by_name = {s.name: s for s in scenarios()}
    assert by_name["read_count"].turns[0].check(None, "the total is 31")
    assert not by_name["read_count"].turns[0].check(None, "the total is 13")
    assert by_name["find_symbol"].turns[0].check(None, "pkg/mod_27.py")
    assert not by_name["find_symbol"].turns[0].check(None, "pkg/mod_26.py")
    work = pathlib.Path(tempfile.mkdtemp())
    try:
        by_name["fix_bug"].setup(work)
        assert not by_name["fix_bug"].turns[0].check(work, "")
        (work / "calc.py").write_text(CALC_PY.replace(
            "return s[mid]\n    return", "return (s[mid - 1] + s[mid]) / 2\n    return"))
        assert by_name["fix_bug"].turns[0].check(work, "")
        (work / "test_calc.py").write_text("print('OK')\n")
        assert not by_name["fix_bug"].turns[0].check(work, ""), "an edited test must not pass"
    finally:
        shutil.rmtree(work)

    # The reference loop: runs the tool call, sums both requests' usage, splits cache figures out
    # of the prompt total, marks the last message for caching, and keeps the history for turn two.
    server = http.server.HTTPServer(("127.0.0.1", 0), FakeEndpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    work = pathlib.Path(tempfile.mkdtemp())
    os.environ["BENCH_SELFTEST_KEY"] = "k"
    try:
        (work / "a.txt").write_text("hello")
        contestant = {"name": "r", "tool": "reference", "model": "m", "key_env": "BENCH_SELFTEST_KEY",
                      "base_url": f"http://127.0.0.1:{server.server_port}"}
        state = {}
        result = ref_turn(contestant, work, "read a.txt", state, "")
        assert result.reply == "contents: hello" and result.tool_calls == 1
        assert (result.usage.fresh, result.usage.cache_write, result.usage.cache_read,
                result.usage.output, result.usage.requests) == (100 + 100, 1000, 900, 25, 2), result
        second = FakeEndpoint.seen[1]["messages"]
        assert second[-1]["role"] == "tool" and "hello" in json.dumps(second[-1])
        assert second[-1]["content"][0]["cache_control"] == {"type": "ephemeral"}
        assert second[0]["content"][0]["cache_control"] == {"type": "ephemeral"}
        assert len(state["messages"]) == 4
    finally:
        server.shutdown()
        shutil.rmtree(work)
    print("selftest ok")


def main():
    if "--selftest" in sys.argv:
        selftest()
        return
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    listing = sub.add_parser("list")
    listing.add_argument("--config", default=str(HERE / "contestants.example.json"))
    run = sub.add_parser("run")
    run.add_argument("--config", required=True)
    run.add_argument("--only", help="comma separated contestant names")
    run.add_argument("--scenarios", help="comma separated scenario names")
    run.add_argument("--repeats", type=int, default=3)
    run.add_argument("--out")
    run.add_argument("--no-warmup", action="store_true",
                     help="skip the unscored first call that puts the static prompt in the cache")
    report = sub.add_parser("report")
    report.add_argument("results")
    report.add_argument("--plot", metavar="DIR")
    args = parser.parse_args()
    {"list": cmd_list, "run": cmd_run, "report": cmd_report}[args.command](args)


if __name__ == "__main__":
    main()
