#!/usr/bin/env python3
"""Compare the token usage of coding agents on the same tasks, model and cache settings.

    bench.py list   [--config FILE]
    bench.py run    --config FILE [--only a,b] [--scenarios x,y] [--repeats N] [--out FILE]
    bench.py report RESULTS.jsonl [--plot DIR]
    bench.py --selftest

A contestant is one tool (bravebot, Claude Code, Codex, opencode, or the `reference` loop that talks to an
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
    # Delegates the turn has to start. Applied only where the tool reports how many it started.
    min_delegates: int = 0
    # A skill the turn has to load with `load_skill`. Applied only where the tool reports its calls.
    loads: str = ""


@dataclass
class Scenario:
    name: str
    category: str
    description: str
    setup: object  # (workdir: Path) -> None
    turns: list
    system_extra: str = ""
    # (home: Path, workdir: Path) -> None. A scenario with one runs with `BRAVEBOT_HOME` set to a
    # fresh directory it fills, so it loads skills and MCP servers a person would have configured.
    home: object = None
    # The tools the scenario can run on. Empty means every tool.
    tools: tuple = ()
    # Flags added to every bravebot command of the scenario, such as `--goal`. Other tools never run a
    # scenario that has them, since `tools` limits it to bravebot.
    bravebot_args: tuple = ()


@dataclass
class TurnResult:
    usage: Usage
    reply: str = ""
    tool_calls: int = 0
    cost_usd: float = 0.0
    # Delegates the turn started, or None where the tool does not say.
    delegates: object = None
    # The skills the turn loaded with `load_skill` and was not refused, or None where the tool does
    # not say.
    loaded: object = None
    # How many calls of each tool the turn made and was not refused, as a dict, and the targets of
    # its `read_file` calls in order. None where the tool does not say.
    by_tool: object = None
    reads: object = None


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
        body = "".join(
            f"def helper_{n:02d}_{k}(x):\n    return x + {n * 3 + i}\n\n" for i, k in enumerate("abc")
        )
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


def _part_key(n):
    return hashlib.md5(f"part-{n}".encode()).hexdigest()[:8]


def _setup_parts(workdir):
    """Four files of filler with one `key:` line each, so a delegate has real text to read."""
    for n in range(1, 5):
        lines = [f"part {n} filler line {i}: nothing to report\n" for i in range(150)]
        lines.insert(75, f"key: {_part_key(n)}\n")
        (workdir / f"part_{n}.txt").write_text("".join(lines))


def _delegate_one(n):
    return Turn(
        f"Start one delegate (a separate sub-agent) to read part_{n}.txt and report the value on "
        f"its line starting with 'key:'. Reply with that value only.",
        _says(_part_key(n)),
        min_delegates=1,
    )


def _style_guide(rules):
    return "Follow these house rules in every reply.\n" + "".join(
        f"Rule {n}: when a request touches area {n}, state the area number once and keep the "
        f"wording plain.\n"
        for n in range(1, rules + 1)
    )


def _digest(label):
    return hashlib.sha256(label.encode()).hexdigest()[:8]


TYPICAL_SKILLS = (
    ("commit-style", "How commit messages are written in this project. Use before writing one."),
    ("review-checklist", "What a code review checks, in order. Use when asked to review a change."),
    ("release-notes", "How release notes are drafted from merged changes. Use when preparing a release."),
    ("migration-plan", "How a schema migration is planned and rolled back. Use before changing a table."),
    ("incident-report", "The sections of an incident report. Use after an outage."),
    ("api-design", "Naming and versioning rules for public interfaces. Use when adding an endpoint."),
    ("test-triage", "How a failing test is classified as a bug, a flake or a stale fixture."),
    ("changelog-entry", "The one-line format of a changelog entry. Use when a change ships."),
)


def _write_skill(home, name, description, body):
    folder = home / "skills" / name
    folder.mkdir(parents=True)
    (folder / "SKILL.md").write_text(f"---\nname: {name}\ndescription: {description}\n---\n\n{body}")


def _filler_body(name, lines=40):
    return "".join(
        f"{i}. When working on {name}, state the area you are in and keep the wording plain.\n"
        for i in range(1, lines + 1)
    )


STUB_MCP_SERVER = HERE / "fixtures" / "stub_mcp_server.py"
# Seconds the selftest lets the stand-in server take for its whole exchange.
STUB_DEADLINE = 30


def _setup_configured_home(home):
    """A state directory with eight skills and one stdio MCP server that offers eight tools.

    The server is requested in the user settings, so a run without `--safe` starts it. The program
    is the interpreter running bench.py, by its real path, and the script is a file the server may
    read (mcp-servers.md SERVERS-10).
    """
    for name, description in TYPICAL_SKILLS:
        _write_skill(home, name, description, _filler_body(name))
    script = str(STUB_MCP_SERVER.resolve())
    (home / "mcp.json").write_text(json.dumps({"servers": {"bench": {
        "transport": "stdio", "argv": [os.path.realpath(sys.executable), script],
        "reads": [script]}}}))
    (home / "settings.json").write_text(json.dumps({"mcp": {"request": ["bench"]}}))


def _check_configured(workdir, reply):
    """The count the skills overview states, and the name of a tool only the stand-in server has."""
    return (re.search(r"\b%d\b" % len(TYPICAL_SKILLS), reply) is not None
            and "ledger_probe" in reply)


SKILL_NAME = "ledger-rounding"
SKILL_WORD = _digest("skill-word")
SKILL_STEP = "R-" + _digest("skill-step")


def _skill_body():
    """About five thousand characters of rules, with the two values the turns ask for in them."""
    lines = [f"{i}. Round amounts in the ledger as rule {i} says and note the rule number.\n"
             for i in range(1, 61)]
    lines.insert(30, f"Code word: {SKILL_WORD}\n")
    lines.append(f"Step 2 of the rounding procedure: apply table {SKILL_STEP}.\n")
    return "".join(lines)


def _setup_skill_home(home):
    """The target skill beside three that are not needed, so the overview names four."""
    _write_skill(home, SKILL_NAME, "How amounts are rounded in the ledger. Use when asked about "
                 "rounding.", _skill_body())
    for name, description in TYPICAL_SKILLS[:3]:
        _write_skill(home, name, description, _filler_body(name))


SKILL_LOAD_TURNS = [
    Turn(f"Load the skill named {SKILL_NAME}. Reply with its code word and nothing else.",
         _says(SKILL_WORD), loads=SKILL_NAME),
    Turn("What does step 2 of the rounding procedure say to apply? Reply with the table name "
         "and nothing else.", _says(SKILL_STEP)),
]
SKILL_INLINE_TURNS = [
    Turn(f"The code word for ledger rounding is {SKILL_WORD}. Reply with the code word and "
         f"nothing else.", _says(SKILL_WORD)),
    Turn(f"Step 2 of the rounding procedure says to apply table {SKILL_STEP}. Reply with the "
         f"table name and nothing else.", _says(SKILL_STEP)),
]


COPY_SOURCE = "".join(f"line {i}: the quick brown fox {_digest(f'copy-{i}')}\n" for i in range(1, 61))
COPY_PROMPT = ("Copy source.txt to copy.txt so that copy.txt has exactly the same content as "
               "source.txt. Do not change source.txt. Reply with the word: done")
COPY_GOAL = "copy.txt exists and holds exactly the content of source.txt"


def _setup_copy(workdir):
    (workdir / "source.txt").write_text(COPY_SOURCE)


def _check_copy(workdir, reply):
    """The copy is in the workspace and the source is as it was. The reply is not read."""
    source, copy = workdir / "source.txt", workdir / "copy.txt"
    return (source.is_file() and copy.is_file() and source.read_text() == COPY_SOURCE
            and copy.read_text().rstrip() == COPY_SOURCE.rstrip())


# What a goal run or a manifest run refuses beside the flag that makes it one: crates/cli/src/
# invocation.rs, `manifest_conflict` (agent, advisor, output schema, goal, tools, system prompts,
# resume) and `limit_not_with_a_goal` / `limit_not_with_a_manifest` (the two run limits). A
# contestant that passes one of them cannot run those scenarios.
REFUSED_BY_PLANNED_RUNS = ("--agent", "--advisor", "--output-schema", "--tools", "--system-prompt",
                           "--append-system-prompt", "--resume", "--continue", "--max-rounds",
                           "--max-tokens")


REFACTOR_OLD, REFACTOR_NEW = "compute_total", "order_total"
REFACTOR_FILES = {
    "pricing.py": f"def {REFACTOR_OLD}(items):\n    return sum(i['price'] * i['qty'] for i in items)\n",
    "cart.py": f"from pricing import {REFACTOR_OLD}\n\n\ndef cart_total(cart):\n"
               f"    return {REFACTOR_OLD}(cart['items'])\n",
    "invoice.py": f"import pricing\n\n\ndef invoice_line(order):\n"
                  f"    return f\"{{order['id']}}: {{pricing.{REFACTOR_OLD}(order['items'])}}\"\n",
    "report.py": f"from pricing import {REFACTOR_OLD}\n\n\ndef report(orders):\n"
                 f"    return sum({REFACTOR_OLD}(o['items']) for o in orders)\n",
    "NOTES.md": f"Price an order with `{REFACTOR_OLD}(items)`.\n",
}
REFACTOR_TEST = f"""from pricing import {REFACTOR_NEW}
from cart import cart_total
from invoice import invoice_line
from report import report

items = [{{'price': 3, 'qty': 2}}, {{'price': 5, 'qty': 1}}]
assert {REFACTOR_NEW}(items) == 11
assert cart_total({{'items': items}}) == 11
assert invoice_line({{'id': 'A1', 'items': items}}) == 'A1: 11'
assert report([{{'items': items}}, {{'items': items}}]) == 22
print('OK')
"""
REFACTOR_PROMPT = (f"Rename {REFACTOR_OLD} to {REFACTOR_NEW} everywhere in this project: the "
                   f"definition, every caller and the notes. test_shop.py already uses the new name "
                   f"and must not be edited. Run it to confirm.")


def _setup_refactor(workdir):
    for name, text in REFACTOR_FILES.items():
        (workdir / name).write_text(text)
    (workdir / "test_shop.py").write_text(REFACTOR_TEST)


def _check_refactor(workdir, reply):
    """The project's own script passes, it was not edited, and every file that named the old
    function now uses the new one, so an alias that keeps the old name does not pass."""
    if (workdir / "test_shop.py").read_text() != REFACTOR_TEST:
        return False
    for name in REFACTOR_FILES:
        text = (workdir / name).read_text()
        if REFACTOR_OLD in text or REFACTOR_NEW not in text:
            return False
    done = subprocess.run([sys.executable, "-B", "test_shop.py"], cwd=workdir,
                          capture_output=True, text=True, timeout=30)
    return done.returncode == 0 and "OK" in done.stdout


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
            "delegate_fanout", "delegates",
            "Four delegates started in one turn, each reading one file. Measures what the parent "
            "and the delegates spend together, and whether the delegates share a cached prefix.",
            _setup_parts,
            [Turn(
                "There are four files, part_1.txt to part_4.txt. Start one delegate (a separate "
                "sub-agent) per file, all four at the same time. Each reads its file and reports "
                "the value on its line starting with 'key:'. Reply with the four values in file "
                "order, joined by dashes, and nothing else.",
                _says("-".join(_part_key(n) for n in range(1, 5))),
                min_delegates=4,
            )],
        ),
        Scenario(
            "delegate_serial", "delegates",
            "Three turns over one conversation, each starting one delegate. Measures whether a "
            "later delegate and the parent's next request read the earlier ones from the cache.",
            _setup_parts,
            [_delegate_one(1), _delegate_one(2), _delegate_one(3)],
        ),
        Scenario(
            "configured_hello", "configured",
            "One trivial turn with eight skills and eight MCP tool schemas loaded, without --safe. "
            "The report gives the difference from the --safe baseline contestant's hello.",
            lambda workdir: None,
            [Turn("Skills and tools are listed in your context. How many skills of your own or "
                  "the project's are not listed above, and what is the name of the tool that "
                  "looks up one ledger entry? Reply with the number, a space and the tool name, "
                  "and nothing else.", _check_configured)],
            home=_setup_configured_home, tools=("bravebot",),
        ),
        Scenario(
            "skill_load", "skills",
            "Two turns. The first loads a skill of about five thousand characters, and the second "
            "asks about its text. Shows what the load adds and whether the next request reads the "
            "cached prefix.",
            lambda workdir: None, SKILL_LOAD_TURNS, home=_setup_skill_home, tools=("bravebot",),
        ),
        Scenario(
            "skill_inline", "skills",
            "The turns of skill_load with the values given in the prompt and no skill loaded. The "
            "difference from skill_load is what loading adds.",
            lambda workdir: None, SKILL_INLINE_TURNS, home=_setup_skill_home, tools=("bravebot",),
        ),
        Scenario(
            "plain_copy", "modes",
            "Copy one file to another in a plain turn. The baseline for goal_copy and manifest_copy.",
            _setup_copy, [Turn(COPY_PROMPT, _check_copy)],
        ),
        Scenario(
            "goal_copy", "modes",
            "The task of plain_copy as a bravebot --goal run, which checks the goal after each "
            "round. Compared with plain_copy.",
            _setup_copy, [Turn(COPY_PROMPT, _check_copy)], tools=("bravebot",),
            bravebot_args=("--goal", COPY_GOAL),
        ),
        Scenario(
            "manifest_copy", "modes",
            "The task of plain_copy as a bravebot --mode manifest run, which plans the whole run "
            "before reading anything. Compared with plain_copy.",
            _setup_copy, [Turn(COPY_PROMPT, _check_copy)], tools=("bravebot",),
            bravebot_args=("--mode", "manifest"),
        ),
        Scenario(
            "refactor_rename", "edits",
            "Rename a function across four modules and a notes file, then run the project's test "
            "script. Shows how edits are sent and whether files already read are read again.",
            _setup_refactor, [Turn(REFACTOR_PROMPT, _check_refactor)],
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
    """bravebot `--json`. `total` is prompt plus output, so the prompt is what is left of it.

    The totals already include what every delegate spent. `delegates` counts the `spawn_agent`
    calls the turn made that were not refused, `loaded` lists the `load_skill` targets of the
    calls that were not refused, `by_tool` counts the calls that were not refused for each tool,
    and `reads` lists the targets of the `read_file` calls in order. A goal run and a manifest run
    write the same object (main.rs passes `reporter.calls()` into the report for both, and
    progress.rs `tool_finished` is what records them, so finish.rs `what_ran` only receives them),
    so the same fields are read. A manifest run names no session, so `session` is then None.
    """
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
    calls = result.get("calls", [])
    delegates = sum(1 for c in calls if c.get("tool") == "spawn_agent" and not c.get("refused"))
    loaded = [c.get("target") for c in calls if c.get("tool") == "load_skill" and not c.get("refused")]
    by_tool = {}
    for c in calls:
        if not c.get("refused"):
            by_tool[c.get("tool")] = by_tool.get(c.get("tool"), 0) + 1
    reads = [c.get("target") for c in calls if c.get("tool") == "read_file" and not c.get("refused")]
    return (TurnResult(usage, result.get("reply") or "", len(calls), delegates=delegates,
                       loaded=loaded, by_tool=by_tool, reads=reads),
            result.get("session"))


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


OPENCODE_COUNTERS = ("input", "output", "cache_read", "cache_write", "messages")


def opencode_totals(export):
    """Sum the usage of every model request in an exported opencode session.

    The `--format json` stream leaves out the last request's usage, so the stored session is read
    instead. Each assistant message carries one request's `tokens`. Reasoning tokens are reported
    apart from `output` and are billed as output, so they are added back.
    """
    total = dict.fromkeys(OPENCODE_COUNTERS, 0)
    for message in export.get("messages", []):
        tokens = message.get("tokens")
        if not tokens:
            continue
        cache = tokens.get("cache") or {}
        total["input"] += tokens.get("input", 0)
        total["output"] += tokens.get("output", 0) + tokens.get("reasoning", 0)
        total["cache_read"] += cache.get("read", 0)
        total["cache_write"] += cache.get("write", 0)
        total["messages"] += 1
    return total


def parse_opencode(stdout, export, previous):
    """opencode `run --format json` events, with usage from the exported session.

    The export holds the whole session, so a later turn's cost is the difference from `previous`,
    the totals after the turn before. `input` is already net of the cache.
    """
    session = None
    reply = ""
    calls = 0
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        session = event.get("sessionID", session)
        kind = event.get("type")
        if kind == "error":
            raise BenchError("opencode reported an error: "
                             + str((event.get("error") or {}).get("message"))[:200])
        if kind == "text":
            reply = (event.get("part") or {}).get("text", reply)
        elif kind == "tool_use":
            calls += 1
    if session is None:
        raise BenchError("no session id in opencode's output")
    now = opencode_totals(export)
    delta = {k: now[k] - previous.get(k, 0) for k in OPENCODE_COUNTERS}
    if delta["messages"] <= 0:
        raise BenchError("the exported session holds no new model request")
    usage = Usage(delta["input"], delta["cache_read"], delta["cache_write"], delta["output"],
                  delta["messages"])
    return TurnResult(usage, reply, calls), session, now


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
        body = {"model": model_of(contestant), "tools": REF_TOOLS, "max_tokens": 4096,
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
    known = dict(os.environ, BENCH_DIR=str(HERE))
    return re.sub(r"\$\{(\w+)\}", lambda m: known.get(m.group(1), ""), value)


def model_of(contestant):
    """The model to pass, with ${VAR} expanded so a config can name a route without writing an ARN."""
    model = expand(contestant.get("model") or "")
    if contestant.get("model") and not model:
        raise BenchError(f"the model of {contestant['name']} names a variable that is not set")
    return model or None


def execute(cmd, workdir, contestant):
    env = dict(os.environ)
    # A tool that reads $PWD instead of asking the system for its directory would otherwise work in
    # the directory bench.py was started from.
    env["PWD"] = str(pathlib.Path(workdir).resolve())
    env["BENCH_DIR"] = str(HERE)
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


def run_turn(contestant, workdir, prompt, state, extra_system, single, scenario_args=()):
    tool = contestant["tool"]
    model = model_of(contestant)
    extra = [expand(a) for a in contestant.get("args", [])]
    if tool == "reference":
        return ref_turn(contestant, workdir, prompt, state, extra_system)
    if tool == "opencode":
        bin_ = contestant.get("bin", "opencode")
        cmd = [bin_, "run", "--standalone", "--format", "json", "--auto"]
        if model:
            cmd += ["-m", model]
        if state.get("session") and not single:
            cmd += ["-s", state["session"]]
        if extra_system:
            # opencode has no flag for it, so the text goes where it reads standing instructions.
            (workdir / "AGENTS.md").write_text(extra_system)
        stdout = execute(cmd + extra + [prompt], workdir, contestant)
        session = None
        for line in stdout.splitlines():
            if line.startswith("{") and '"sessionID"' in line:
                session = json.loads(line)["sessionID"]
                break
        if session is None:
            raise BenchError("no session id in opencode's output")
        state["session"] = session
        exported = execute([bin_, "session", "export", session], workdir, contestant)
        try:
            export = json.loads(exported)
        except json.JSONDecodeError as error:
            raise BenchError("opencode's session export is not JSON") from error
        result, _, now = parse_opencode(stdout, export, state.get("totals", {}))
        state["totals"] = now
        return result
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
        result, session = parse_bravebot(execute(cmd + extra + list(scenario_args), workdir,
                                                 contestant))
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


def turn_ok(turn, result, workdir):
    """Whether the reply is right and, where the tool reports its calls, the work was done.

    A turn that asks for delegates and does the work inline spends less, so a correct reply alone
    would let it win the scenario. The same holds for a turn that has to load a skill, and for a
    turn whose answer is in a skill the tool never loaded.
    """
    if result.delegates is not None and result.delegates < turn.min_delegates:
        return False
    if turn.loads and result.loaded is not None and turn.loads not in result.loaded:
        return False
    return bool(turn.check(workdir, result.reply))


def applies(contestant, scenario):
    """Whether a scenario can be run by a contestant.

    A scenario with a fixture home runs only for a `configured` contestant, and a `configured`
    contestant runs only those. Without `--safe` it would otherwise read the person's own state
    directory, and a `--safe` contestant would not load the fixture. A scenario limited to some
    tools is not run by the others, which have no skills or MCP servers of this kind to load. A
    scenario with `bravebot_args` is a goal run or a manifest run, which refuse some flags
    (`REFUSED_BY_PLANNED_RUNS`), so a contestant that passes one would fail every turn and is not
    run on it.
    """
    if scenario.bravebot_args and any(a in REFUSED_BY_PLANNED_RUNS
                                      for a in contestant.get("args", [])):
        return False
    if scenario.tools and contestant["tool"] not in scenario.tools:
        return False
    return bool(scenario.home) == bool(contestant.get("configured"))


def home_path(scenario):
    """The same path on every run, so a path that reaches the prompt does not change the prefix."""
    return pathlib.Path(tempfile.gettempdir()) / "tokbench-home" / scenario.name


def with_home(contestant, home):
    """The contestant with `BRAVEBOT_HOME` set to `home`, so the person's own directory is not read."""
    return dict(contestant, env=dict(contestant.get("env", {}), BRAVEBOT_HOME=str(home)))


def run_scenario(contestant, scenario, rep, sink):
    # The path is the same on every run: tools put the working directory in the system prompt, and
    # a path that changed would change the cached prefix and hide cross-run cache reuse.
    workdir = pathlib.Path(tempfile.gettempdir()) / "tokbench" / scenario.name
    shutil.rmtree(workdir, ignore_errors=True)
    workdir.mkdir(parents=True)
    state = {}
    single = len(scenario.turns) == 1
    weights = weights_of(contestant)
    home = None
    if scenario.home:
        # The fixture state directory replaces the person's own for every command of the run,
        # the session purge below included.
        home = home_path(scenario)
        shutil.rmtree(home, ignore_errors=True)
        home.mkdir(parents=True)
        contestant = with_home(contestant, home)
    try:
        if home:
            scenario.home(home)
        scenario.setup(workdir)
        for index, turn in enumerate(scenario.turns):
            started = time.monotonic()
            record = {"contestant": contestant["name"], "scenario": scenario.name,
                      "category": scenario.category, "rep": rep, "turn": index}
            if contestant.get("baseline"):
                record["baseline"] = contestant["baseline"]
            try:
                result = run_turn(contestant, workdir, turn.prompt, state,
                                  scenario.system_extra, single, scenario.bravebot_args)
                ok = turn_ok(turn, result, workdir)
                record.update(ok=ok, reply=result.reply[:200], tool_calls=result.tool_calls,
                              delegates=result.delegates, loaded=result.loaded,
                              by_tool=result.by_tool, reads=result.reads,
                              fresh=result.usage.fresh, cache_read=result.usage.cache_read,
                              cache_write=result.usage.cache_write, output=result.usage.output,
                              requests=result.usage.requests,
                              cost=result.usage.cost(weights), cost_usd=result.cost_usd)
            except BenchError as error:
                record.update(ok=False, error=str(error), tool_calls=0, delegates=None,
                              loaded=None, by_tool=None, reads=None, fresh=0,
                              cache_read=0,
                              cache_write=0, output=0, requests=0, cost=0.0, cost_usd=0.0)
            record["wall_s"] = round(time.monotonic() - started, 2)
            sink(record)
            if "error" in record:
                break
    finally:
        if contestant["tool"] == "opencode" and state.get("session"):
            subprocess.run([contestant.get("bin", "opencode"), "session", "delete", state["session"]],
                           cwd=workdir, capture_output=True, stdin=subprocess.DEVNULL)
        if contestant["tool"] == "bravebot" and not single:
            subprocess.run([contestant.get("bin", "bravebot"), "sessions", "purge", "-y"],
                           cwd=workdir, capture_output=True, stdin=subprocess.DEVNULL,
                           env=dict(os.environ, **{k: expand(v) for k, v in
                                                   contestant.get("env", {}).items()}))
        shutil.rmtree(workdir, ignore_errors=True)
        if home:
            shutil.rmtree(home, ignore_errors=True)


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
        total["delegates"] = sum(t.get("delegates") or 0 for t in turns)
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
            "delegates": med("delegates"),
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
                 "requests, delegates started (0 where the tool does not say), solved")
    for scenario in sorted({s for _, s in table}):
        lines.append(f"  {scenario}")
        here = sorted(((c, v) for (c, s), v in table.items() if s == scenario),
                      key=lambda item: -item[1]["score"])
        for name, v in here:
            lines.append(f"    {name:<22} score {v['score']:>5.1f}  cost {v['cost']:>9.0f}  "
                         f"prompt {v['prompt']:>8.0f}  hit {v['hit'] * 100:>3.0f}%  "
                         f"req {v['requests']:>3.0f}  delegates {v['delegates']:>2.0f}  "
                         f"solved {v['success'] * 100:>3.0f}%")
    return "\n".join(lines)


def render_extra(records):
    """The comparisons the ranking cannot show, from the scenarios that have one.

    `configured_hello` against the `--safe` contestant it names as `baseline`, and the two turns of
    `skill_load` against the same turns with the values given in the prompt. Empty where the
    records hold none of them.
    """
    lines = []
    table = summarize(records)
    baselines = {r["contestant"]: r["baseline"] for r in records if r.get("baseline")}
    for name in sorted(c for c, s in table if s == "configured_hello"):
        base = baselines.get(name)
        mine = table[(name, "configured_hello")]
        ref = table.get((base, "hello")) if base else None
        if ref is None:
            lines.append(f"  {name}: no hello run of its baseline {base or '(none named)'}")
            continue
        lines.append(f"  {name}: prompt {mine['prompt']:.0f} against {ref['prompt']:.0f} for "
                     f"{base} hello, so skills and MCP tools add {mine['prompt'] - ref['prompt']:+.0f} "
                     f"prompt tokens and {mine['cost'] - ref['cost']:+.0f} cost units "
                     f"(solved {mine['success'] * 100:.0f}%)"
                     + ("" if mine["prompt"] > ref["prompt"] else
                        "; not larger than the --safe run, so the configuration was not loaded"))
    if lines:
        lines.insert(0, "configured overhead: configured_hello minus the --safe baseline's hello "
                        "(includes the longer question)")

    turns = {}
    for r in records:
        if r["scenario"] in ("skill_load", "skill_inline") and "error" not in r:
            turns.setdefault((r["contestant"], r["scenario"], r["turn"]), []).append(r)
    med = lambda rs, key: statistics.median(r[key] for r in rs)
    skill_lines = []
    for name in sorted({c for c, _, _ in turns}):
        for scenario in ("skill_load", "skill_inline"):
            for turn in (0, 1):
                rs = turns.get((name, scenario, turn))
                if not rs:
                    continue
                prompt = med(rs, "fresh") + med(rs, "cache_read") + med(rs, "cache_write")
                skill_lines.append(
                    f"  {name:<22} {scenario:<13} turn {turn + 1}  prompt {prompt:>7.0f}  "
                    f"fresh {med(rs, 'fresh'):>7.0f}  read {med(rs, 'cache_read'):>7.0f}  "
                    f"write {med(rs, 'cache_write'):>7.0f}  "
                    f"hit {(med(rs, 'cache_read') / prompt * 100 if prompt else 0):>3.0f}%  "
                    f"req {med(rs, 'requests'):>3.0f}")
        a = turns.get((name, "skill_load", 0))
        b = turns.get((name, "skill_inline", 0))
        if a and b:
            total = lambda rs: med(rs, "fresh") + med(rs, "cache_read") + med(rs, "cache_write")
            skill_lines.append(f"  {name}: loading adds {total(a) - total(b):+.0f} prompt tokens to "
                               f"turn 1 over giving the values inline, and "
                               f"{med(a, 'requests') - med(b, 'requests'):+.0f} requests")
    if skill_lines:
        if lines:
            lines.append("")
        lines.append("skill loading, per turn (median): turn 2 read and write show whether the "
                     "load left the cached prefix as it was")
        lines += skill_lines
    more = render_modes_and_edits(records)
    if more:
        if lines:
            lines.append("")
        lines.append(more)
    return "\n".join(lines)


def render_modes_and_edits(records):
    """A goal run and a manifest run against the plain turn of the same task, and how the edits of
    `refactor_rename` were sent.

    `goal_copy` and `manifest_copy` set against `plain_copy` of the same contestant, which has the
    same fixture and prompt. For `refactor_rename`, the calls of each tool and the files read more
    than once, from the targets bravebot reports. A tool that does not report calls shows only its
    count. Empty where the records hold none of these scenarios.
    """
    table = summarize(records)
    lines = []
    for name in sorted({c for c, s in table if s in ("goal_copy", "manifest_copy")}):
        plain = table.get((name, "plain_copy"))
        for scenario in ("goal_copy", "manifest_copy"):
            mine = table.get((name, scenario))
            if mine is None:
                continue
            if plain is None:
                lines.append(f"  {name}: {scenario} has no plain_copy run to compare with")
                continue
            lines.append(
                f"  {name}: {scenario} prompt {mine['prompt']:.0f} against {plain['prompt']:.0f}, "
                f"cost {mine['cost']:.0f} against {plain['cost']:.0f} "
                f"({mine['cost'] - plain['cost']:+.0f}), requests {mine['requests']:.0f} against "
                f"{plain['requests']:.0f}, solved {mine['success'] * 100:.0f}% against "
                f"{plain['success'] * 100:.0f}% for plain_copy")
    if lines:
        lines.insert(0, "goal and manifest runs against the same task as a plain turn (plain_copy)")

    edits = {}
    for r in records:
        if r["scenario"] == "refactor_rename" and "error" not in r:
            edits.setdefault(r["contestant"], []).append(r)
    edit_lines = []
    for name in sorted(edits):
        rs = edits[name]
        median = lambda values: statistics.median(values)
        if any(r.get("by_tool") is None for r in rs):
            edit_lines.append(f"  {name:<22} calls {median(r['tool_calls'] for r in rs):.0f}, "
                              f"not reported per tool, so edits and re-reads are not shown")
            continue
        names = sorted({t for r in rs for t in r["by_tool"]})
        counts = ", ".join(f"{t} {median(r['by_tool'].get(t, 0) for r in rs):.0f}" for t in names)
        again = median(len(r["reads"]) - len(set(r["reads"])) for r in rs)
        edit_lines.append(f"  {name:<22} {counts or 'no calls'}; reads of a file already read "
                          f"{again:.0f}")
    if edit_lines:
        if lines:
            lines.append("")
        lines.append("refactor_rename: calls per tool (median over repeats), so edit_file against "
                     "write_file shows how the edits were sent, and read_file targets seen more "
                     "than once")
        lines += edit_lines
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
        if c.get("configured"):
            if c["tool"] != "bravebot":
                sys.exit(f"contestant {c['name']} is configured, which only bravebot can be")
            if "--safe" in c.get("args", []):
                sys.exit(f"contestant {c['name']} is configured and passes --safe")
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
        for c in contestants:
            first = next((s for s in scenarios() if applies(c, s)), None)
            if first:
                run_scenario(c, first, -1, lambda record: None)
    for rep in range(args.repeats):
        for scenario in chosen:
            for c in contestants:
                if applies(c, scenario):
                    run_scenario(c, scenario, rep, sink)
    handle.close()
    print(f"wrote {out}", file=sys.stderr)
    records = load_records(out)
    print(render_report(summarize(records)))
    extra = render_extra(records)
    if extra:
        print()
        print(extra)


def cmd_report(args):
    records = load_records(args.results)
    table = summarize(records)
    print(render_report(table))
    extra = render_extra(records)
    if extra:
        print()
        print(extra)
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
# Selftest. CLAUDE_SAMPLE, BRAVEBOT_SAMPLE, BRAVEBOT_DELEGATE_SAMPLE and the Codex and opencode events
# are output captured from the real tools, so a parser that reads the wrong field fails on numbers
# that were checked by hand against each tool's own totals. The bravebot `--json` samples for skills,
# refactor and manifest runs are written by hand to the shape of docs/specs/cli.md
# CLI-12 (the manifest one is the refactor sample with no session), so they check that a parser reads
# the fields that clause names.
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


BRAVEBOT_DELEGATE_SAMPLE = json.dumps({
    "schema": 1, "ok": True, "status": 0, "reply": "a-b", "session": "b-2", "steps": 2,
    "tokens": {"total": 90000, "output": 400, "context": 20000,
               "cache_read": 50000, "cache_written": 30000},
    "calls": [{"tool": "spawn_agent", "target": "reader", "refused": False},
              {"tool": "spawn_agent", "target": "reader", "refused": False},
              {"tool": "spawn_agent", "target": "worker", "refused": True},
              {"tool": "read_file", "target": "a.txt", "refused": False}]})


BRAVEBOT_SKILL_SAMPLE = json.dumps({
    "schema": 1, "ok": True, "status": 0, "reply": SKILL_WORD, "session": "b-3", "steps": 2,
    "tokens": {"total": 52000, "output": 90, "context": 26000,
               "cache_read": 25000, "cache_written": 26000},
    "calls": [{"tool": "load_skill", "target": SKILL_NAME, "refused": False},
              {"tool": "load_skill", "target": SKILL_NAME + "x", "refused": True},
              {"tool": "search_skills", "target": "rounding", "refused": False}]})


FAKE_BRAVEBOT = '''#!%(python)s
import json, os, pathlib, sys
args = sys.argv[1:]
if args[:1] == ["sessions"]:
    sys.exit(0)
home = pathlib.Path(os.environ["BRAVEBOT_HOME"])
prompt = args[args.index("-p") + 1]
skills = sorted(p.name for p in (home / "skills").iterdir())
loads = os.environ.get("FAKE_LOADS") == "1"
body = (home / "skills" / %(skill)r / "SKILL.md").read_text()
word = [l for l in body.splitlines() if l.startswith("Code word:")][0].split()[-1]
step = [l for l in body.splitlines() if l.startswith("Step 2")][0].split()[-1].rstrip(".")
reply = word if "code word" in prompt else step
calls = [{"tool": "load_skill", "target": %(skill)r, "refused": False}] if (
    loads and "Load the skill" in prompt) else []
print(json.dumps({"schema": 1, "ok": True, "status": 0, "reply": reply, "session": "s",
                  "steps": 1, "tokens": {"total": 1000, "output": 10, "context": 1,
                  "cache_read": 500, "cache_written": 0}, "calls": calls,
                  "seen": ["--safe" in args, skills]}))
'''


def selftest_configuration():
    """The scenarios that load skills and MCP servers: their fixtures, checks and report."""
    by_name = {s.name: s for s in scenarios()}
    configured, load, inline = (by_name[n] for n in ("configured_hello", "skill_load", "skill_inline"))

    # configured_hello: eight skills the overview counts and an MCP server that is requested,
    # declared in the format of mcp.json (SERVERS-1) and holding eight tools. A skill needs a name
    # and a description (SKILL-1).
    home = pathlib.Path(tempfile.mkdtemp())
    try:
        configured.home(home)
        names = sorted(p.name for p in (home / "skills").iterdir())
        assert names == sorted(n for n, _ in TYPICAL_SKILLS) and len(names) == 8, names
        for name, description in TYPICAL_SKILLS:
            lines = (home / "skills" / name / "SKILL.md").read_text().splitlines()
            assert lines[0] == "---" and lines[1] == f"name: {name}", lines[:2]
            assert lines[2] == f"description: {description}" and lines[3] == "---", lines[:4]
        declared = json.loads((home / "mcp.json").read_text())
        assert list(declared) == ["servers"], declared
        entry = declared["servers"]["bench"]
        assert entry["transport"] == "stdio" and os.path.isabs(entry["argv"][0]), entry
        assert pathlib.Path(entry["argv"][1]).is_file() and entry["reads"] == [entry["argv"][1]]
        assert set(entry) == {"transport", "argv", "reads"}, entry
        assert json.loads((home / "settings.json").read_text()) == {
            "mcp": {"request": ["bench"]}}
    finally:
        shutil.rmtree(home)

    # The stand-in server completes a handshake and lists tools with a name and an input schema.
    server = subprocess.Popen([sys.executable, str(STUB_MCP_SERVER)], stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True)
    # A server that stops answering ends the selftest: the timer kills it, `readline` then returns
    # an empty line and the JSON read fails.
    watchdog = threading.Timer(STUB_DEADLINE, server.kill)
    watchdog.start()
    try:
        def ask(method, params=None):
            message = {"jsonrpc": "2.0", "id": 1, "method": method}
            if params is not None:
                message["params"] = params
            server.stdin.write(json.dumps(message) + "\n")
            server.stdin.flush()
            return json.loads(server.stdout.readline())

        assert ask("initialize", {})["result"]["protocolVersion"]
        server.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"})
                           + "\n")
        tools = ask("tools/list")["result"]["tools"]
        assert len(tools) == 8 and tools[0]["name"] == "ledger_probe", tools
        assert all(t["inputSchema"]["type"] == "object" and t["description"] for t in tools)
        assert "result" in ask("tools/call", {"name": "ledger_probe", "arguments": {}})
        assert "error" in ask("no/such")
    finally:
        watchdog.cancel()
        server.kill()
        server.wait()

    # The check wants the skill count the overview states and a tool name only the server has. The
    # wrong count and a missing tool name are the two near misses.
    check = configured.turns[0].check
    assert check(None, "8 ledger_probe") and check(None, "There are 8: ledger_probe")
    assert not check(None, "4 ledger_probe") and not check(None, "8 ledger_search")
    assert not check(None, "ok")

    # skill_load: the values live only in the skill, so neither prompt gives them away, and the
    # inline control states the same values in its prompts.
    home = pathlib.Path(tempfile.mkdtemp())
    try:
        load.home(home)
        names = sorted(p.name for p in (home / "skills").iterdir())
        assert names == sorted([SKILL_NAME, *(n for n, _ in TYPICAL_SKILLS[:3])]), names
        body = (home / "skills" / SKILL_NAME / "SKILL.md").read_text()
        assert body.startswith(f"---\nname: {SKILL_NAME}\ndescription: ")
        assert body.count(SKILL_WORD) == 1 and body.count(SKILL_STEP) == 1
        assert 4000 < len(body) < 8000, len(body)
    finally:
        shutil.rmtree(home)
    assert SKILL_WORD != SKILL_STEP and SKILL_STEP.startswith("R-")
    for turn in load.turns:
        assert SKILL_WORD not in turn.prompt and SKILL_STEP not in turn.prompt
    for turn in inline.turns:
        assert SKILL_WORD in turn.prompt or SKILL_STEP in turn.prompt
    assert load.turns[0].loads == SKILL_NAME and not load.turns[1].loads
    assert not any(t.loads for t in inline.turns)

    # A turn that has to load a skill fails where the tool reports its calls and none was loaded,
    # or the load was refused, or another skill was loaded. A tool that does not report calls is
    # judged on the reply, which only the skill holds.
    first = load.turns[0]
    brave, _ = parse_bravebot(BRAVEBOT_SKILL_SAMPLE)
    assert brave.loaded == [SKILL_NAME], brave
    assert turn_ok(first, brave, None)
    assert not turn_ok(first, TurnResult(Usage(), SKILL_WORD, 0, loaded=[]), None)
    assert not turn_ok(first, TurnResult(Usage(), SKILL_WORD, 0, loaded=["commit-style"]), None)
    assert turn_ok(first, TurnResult(Usage(), SKILL_WORD, 0, loaded=None), None)
    assert not turn_ok(first, TurnResult(Usage(), "guess", 0, loaded=[SKILL_NAME]), None)
    assert parse_claude(CLAUDE_SAMPLE)[0].loaded is None
    assert parse_bravebot(BRAVEBOT_SAMPLE)[0].loaded == []
    assert turn_ok(load.turns[1], TurnResult(Usage(), SKILL_STEP, 0, loaded=[]), None)
    assert not turn_ok(load.turns[1], TurnResult(Usage(), "R-00000000", 0, loaded=[]), None)

    # Which contestant runs which scenario. A configured contestant reads no home but the fixture's,
    # so it runs the fixture scenarios and nothing else, and a `--safe` one never runs them.
    safe = {"name": "s", "tool": "bravebot", "args": ["--safe"]}
    real = {"name": "r", "tool": "bravebot", "configured": True, "baseline": "s"}
    other = {"name": "c", "tool": "claude", "args": ["--bare"]}
    assert [applies(real, s) for s in (configured, load, inline, by_name["hello"])] == [
        True, True, True, False]
    assert [applies(safe, s) for s in (configured, load, inline, by_name["hello"])] == [
        False, False, False, True]
    assert not applies(dict(other, configured=True), load) and not applies(other, load)
    assert with_home(real, "/h")["env"] == {"BRAVEBOT_HOME": "/h"} and "env" not in real
    assert expand("${BENCH_DIR}") == str(HERE)

    # A configured contestant that passes --safe, or is not bravebot, is refused when the file is
    # read, since a run of either would measure no configuration.
    scratch = pathlib.Path(tempfile.mkdtemp())
    try:
        for bad in (dict(real, args=["--safe"]), dict(real, tool="claude")):
            path = scratch / "c.json"
            path.write_text(json.dumps({"contestants": [bad]}))
            try:
                load_config(path)
                raise AssertionError("a configured contestant that cannot load configuration")
            except SystemExit:
                pass
        path.write_text(json.dumps({"contestants": [safe, real]}))
        assert [c["name"] for c in load_config(path)] == ["s", "r"]
    finally:
        shutil.rmtree(scratch)

    # A whole scenario run against a stand-in bravebot: the fixture home is the process's
    # BRAVEBOT_HOME for every turn, the skill is there to be loaded, the run records the load and
    # the second turn resumes, and the fixture home is gone afterwards.
    scratch = pathlib.Path(tempfile.mkdtemp())
    try:
        fake = scratch / "bravebot"
        fake.write_text(FAKE_BRAVEBOT % {"python": sys.executable, "skill": SKILL_NAME})
        fake.chmod(0o755)
        for loads, passes in ("1", True), ("0", False):
            contestant = dict(real, bin=str(fake), env={"FAKE_LOADS": loads})
            records = []
            run_scenario(contestant, load, 0, records.append)
            assert [r["turn"] for r in records] == [0, 1], records
            assert [r["ok"] for r in records] == [passes, True], records
            assert records[0]["loaded"] == ([SKILL_NAME] if passes else []), records
            assert records[0]["baseline"] == "s" and records[0]["cache_read"] == 500
            assert not home_path(load).exists()
    finally:
        shutil.rmtree(scratch)

    # The report gives the difference from the --safe hello, and says so when the configured run is
    # not larger than it. The skill report separates what the load adds from the inline control and
    # shows each turn's cache read and write.
    def row(name, scenario, turn, fresh, read, write, **extra):
        record = {"contestant": name, "scenario": scenario, "category": "x", "rep": 0, "turn": turn,
                  "ok": True, "fresh": fresh, "cache_read": read, "cache_write": write,
                  "output": 0, "requests": 2, "tool_calls": 0, "cost": float(fresh),
                  "cost_usd": 0.0, "wall_s": 1.0}
        return dict(record, **extra)

    records = [row("s", "hello", 0, 100, 3000, 0),
               row("r", "configured_hello", 0, 100, 5000, 0, baseline="s")]
    text = render_extra(records)
    assert "r: prompt 5100 against 3100 for s hello" in text and "+2000 prompt tokens" in text, text
    assert "not larger" not in text
    smaller = render_extra([row("s", "hello", 0, 100, 3000, 0),
                            row("r", "configured_hello", 0, 50, 2000, 0, baseline="s")])
    assert "was not loaded" in smaller, smaller
    assert "no hello run of its baseline s" in render_extra(records[1:])
    assert render_extra([row("s", "hello", 0, 100, 3000, 0)]) == ""
    records = [row("r", "skill_load", 0, 50, 1000, 1500), row("r", "skill_load", 1, 20, 2500, 0),
               row("r", "skill_inline", 0, 40, 1000, 0), row("r", "skill_inline", 1, 20, 1000, 0)]
    text = render_extra(records)
    assert "r: loading adds +1510 prompt tokens to turn 1" in text, text
    assert "skill_load    turn 2" in text and "read    2500" in text, text


FAKE_MODES_BRAVEBOT = '''#!%(python)s
import json, os, pathlib, sys
args = sys.argv[1:]
pathlib.Path(os.environ["FAKE_ARGS_OUT"]).write_text(json.dumps(args))
if os.environ.get("FAKE_WORK") == "1":
    pathlib.Path("copy.txt").write_text(pathlib.Path("source.txt").read_text())
print(json.dumps({"schema": 1, "ok": True, "status": 0, "reply": "done", "session": None,
                  "steps": 1, "tokens": {"total": 1000, "output": 10, "context": 1,
                  "cache_read": 500, "cache_written": 0}, "calls": []}))
'''


# A refactor run as `--json` reports it: one read of each file, a second read of one, an edit of
# each file that named the function, and the test run. Refused calls are not counted.
BRAVEBOT_REFACTOR_SAMPLE = json.dumps({
    "schema": 1, "ok": True, "status": 0, "reply": "done", "session": "b-4", "steps": 3,
    "tokens": {"total": 60000, "output": 700, "context": 20000,
               "cache_read": 30000, "cache_written": 20000},
    "calls": [{"tool": "read_file", "target": "pricing.py", "refused": False},
              {"tool": "read_file", "target": "cart.py", "refused": False},
              {"tool": "read_file", "target": "pricing.py", "refused": False},
              {"tool": "read_file", "target": "invoice.py", "refused": True},
              {"tool": "edit_file", "target": "pricing.py", "refused": False},
              {"tool": "edit_file", "target": "cart.py", "refused": False},
              {"tool": "write_file", "target": "NOTES.md", "refused": False},
              {"tool": "run", "target": "python3 test_shop.py", "refused": False}]})


def selftest_modes():
    """The goal, manifest and refactor scenarios: fixtures, checks, flags and report."""
    by_name = {s.name: s for s in scenarios()}
    plain, goal, manifest, refactor = (by_name[n] for n in (
        "plain_copy", "goal_copy", "manifest_copy", "refactor_rename"))

    # The goal and manifest scenarios have the plain turn's fixture and prompt, so the three differ
    # in the mode alone, and only bravebot runs the two that have a mode.
    assert goal.turns[0].prompt == plain.turns[0].prompt == manifest.turns[0].prompt
    assert goal.setup is plain.setup is manifest.setup
    assert goal.bravebot_args == ("--goal", COPY_GOAL) and manifest.bravebot_args == (
        "--mode", "manifest") and not plain.bravebot_args
    assert goal.tools == manifest.tools == ("bravebot",) and not plain.tools
    # Neither passes a flag the two modes refuse (invocation.rs manifest_conflict and the limits):
    # `--incognito` is not one of them, so a one-turn scenario keeps it.
    for scenario in (goal, manifest):
        assert not set(scenario.bravebot_args) & set(REFUSED_BY_PLANNED_RUNS)

    # A contestant that passes a flag the modes refuse is not run on them, and still runs the plain
    # turn. Other tools run the plain turn and nothing of the modes.
    safe = {"name": "s", "tool": "bravebot", "args": ["--safe"]}
    capped = {"name": "c", "tool": "bravebot", "args": ["--safe", "--max-rounds", "3"]}
    other = {"name": "o", "tool": "claude", "args": ["--bare"]}
    assert [applies(safe, s) for s in (plain, goal, manifest, refactor)] == [True] * 4
    assert [applies(capped, s) for s in (plain, goal, manifest, refactor)] == [
        True, False, False, True]
    assert [applies(other, s) for s in (plain, goal, manifest, refactor)] == [
        True, False, False, True]

    # copy: the check reads the workspace. Doing nothing, a reply that says done, a changed source
    # and a near-miss copy all fail, whatever the reply says.
    work = pathlib.Path(tempfile.mkdtemp())
    try:
        plain.setup(work)
        check = plain.turns[0].check
        assert not check(work, "done"), "no copy must not pass"
        (work / "copy.txt").write_text(COPY_SOURCE[:-20])
        assert not check(work, "done"), "a truncated copy must not pass"
        (work / "copy.txt").write_text(COPY_SOURCE.replace("line 7:", "line 8:"))
        assert not check(work, "done")
        (work / "copy.txt").write_text(COPY_SOURCE)
        assert check(work, "")
        (work / "source.txt").write_text("")
        assert not check(work, "done"), "a source that was changed must not pass"
    finally:
        shutil.rmtree(work)

    # refactor: the fixture fails as it is, passes when every file is renamed, and fails for an
    # edited test, an alias that keeps the old name, and a rename that missed a caller or the notes.
    def refactored(work):
        for name in REFACTOR_FILES:
            path = work / name
            path.write_text(path.read_text().replace(REFACTOR_OLD, REFACTOR_NEW))

    check = refactor.turns[0].check
    assert len(REFACTOR_FILES) >= 4 and len(refactor.turns) == 1
    for name, text in REFACTOR_FILES.items():
        assert REFACTOR_OLD in text and REFACTOR_NEW not in text, name
    work = pathlib.Path(tempfile.mkdtemp())
    try:
        refactor.setup(work)
        assert (work / "test_shop.py").read_text() == REFACTOR_TEST
        assert not check(work, "done"), "the unedited tree must not pass"
        refactored(work)
        assert check(work, "")
        for name in REFACTOR_FILES:
            refactor.setup(work)
            refactored(work)
            (work / name).write_text(REFACTOR_FILES[name])
            assert not check(work, "done"), f"{name} left unchanged must not pass"
        refactor.setup(work)
        refactored(work)
        (work / "pricing.py").write_text(
            (work / "pricing.py").read_text() + f"{REFACTOR_OLD} = {REFACTOR_NEW}\n")
        assert not check(work, "done"), "an alias for the old name must not pass"
        refactor.setup(work)
        refactored(work)
        (work / "pricing.py").write_text(
            f"def {REFACTOR_NEW}(items):\n    return 0\n")
        assert not check(work, "done"), "a rename that breaks the script must not pass"
        refactor.setup(work)
        refactored(work)
        (work / "test_shop.py").write_text("print('OK')\n")
        assert not check(work, "done"), "an edited test must not pass"
    finally:
        shutil.rmtree(work)

    # The parser keeps what `--json` says per tool and the read targets in order. A refused call is
    # in `tool_calls` and in neither. A tool that does not report calls has None for both.
    brave, session = parse_bravebot(BRAVEBOT_REFACTOR_SAMPLE)
    assert brave.by_tool == {"read_file": 3, "edit_file": 2, "write_file": 1, "run": 1}, brave
    assert brave.reads == ["pricing.py", "cart.py", "pricing.py"] and brave.tool_calls == 8
    claude = parse_claude(CLAUDE_SAMPLE)[0]
    assert claude.by_tool is None and claude.reads is None
    # A manifest run names no session and writes the same object, so it parses the same way.
    manifest_run = json.loads(BRAVEBOT_REFACTOR_SAMPLE)
    manifest_run.update(session=None, steps=1, calls=manifest_run["calls"][:2])
    brave, session = parse_bravebot(json.dumps(manifest_run))
    assert session is None and brave.by_tool == {"read_file": 2} and brave.usage.requests == 2

    # The flags reach the command, in a run of the whole scenario against a stand-in bravebot: the
    # goal run gets `--goal <condition>`, the manifest run `--mode manifest`, the plain turn neither,
    # and each keeps `--incognito` and `--dangerously-skip-permissions`. A run that does not copy
    # fails whatever the mode, which is what the check on the workspace is for.
    scratch = pathlib.Path(tempfile.mkdtemp())
    try:
        fake = scratch / "bravebot"
        fake.write_text(FAKE_MODES_BRAVEBOT % {"python": sys.executable})
        fake.chmod(0o755)
        seen = scratch / "args.json"
        for scenario, flags in ((plain, []), (goal, ["--goal", COPY_GOAL]),
                                (manifest, ["--mode", "manifest"])):
            for works in "1", "0":
                contestant = dict(safe, bin=str(fake), env={"FAKE_ARGS_OUT": str(seen),
                                                              "FAKE_WORK": works})
                records = []
                run_scenario(contestant, scenario, 0, records.append)
                args = json.loads(seen.read_text())
                assert args[-len(flags):] == flags if flags else "--goal" not in args, args
                assert "--mode" not in args or args[args.index("--mode") + 1] == "manifest"
                assert "--incognito" in args and "--dangerously-skip-permissions" in args
                assert [r["ok"] for r in records] == [works == "1"], (scenario.name, records)
                assert records[0]["by_tool"] == {} and records[0]["reads"] == []
    finally:
        shutil.rmtree(scratch)

    # The report sets each mode against the plain turn of the same contestant and says so when the
    # plain turn is missing. Refactor lines show the per-tool counts and the re-reads, and a tool
    # that does not report calls says so.
    def row(name, scenario, fresh, ok=True, **extra):
        record = {"contestant": name, "scenario": scenario, "category": "x", "rep": 0, "turn": 0,
                  "ok": ok, "fresh": fresh, "cache_read": 0, "cache_write": 0, "output": 0,
                  "requests": 2, "tool_calls": 3, "cost": float(fresh), "cost_usd": 0.0,
                  "wall_s": 1.0}
        return dict(record, **extra)

    records = [row("r", "plain_copy", 100), row("r", "goal_copy", 250),
               row("r", "manifest_copy", 60)]
    text = render_extra(records)
    assert "r: goal_copy prompt 250 against 100, cost 250 against 100 (+150)" in text, text
    assert "r: manifest_copy prompt 60 against 100, cost 60 against 100 (-40)" in text, text
    assert "r: goal_copy has no plain_copy run to compare with" in render_extra(records[1:2])
    assert render_extra([row("r", "plain_copy", 100)]) == ""
    records = [row("r", "refactor_rename", 100, by_tool={"read_file": 3, "edit_file": 2},
                   reads=["a.py", "b.py", "a.py"]),
               row("c", "refactor_rename", 100, by_tool=None, reads=None)]
    text = render_extra(records)
    assert "r                      edit_file 2, read_file 3; reads of a file already read 1" in text, text
    assert "c                      calls 3, not reported per tool" in text, text


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

    # A delegate counts when the call was not refused. The tokens are the run's totals, which
    # already hold what the delegates spent, so nothing is added for them here.
    brave, _ = parse_bravebot(BRAVEBOT_DELEGATE_SAMPLE)
    assert brave.delegates == 2 and brave.tool_calls == 4, brave
    assert brave.usage.prompt == 90000 - 400 and brave.usage.cache_read == 50000, brave
    assert parse_bravebot(BRAVEBOT_SAMPLE)[0].delegates == 0
    assert parse_claude(CLAUDE_SAMPLE)[0].delegates is None

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

    # Every scenario's setup must run and leave what its prompt refers to. A setup that raises
    # would stop a whole run partway through.
    for scenario in scenarios():
        scratch = pathlib.Path(tempfile.mkdtemp())
        try:
            scenario.setup(scratch)
            if scenario.name == "find_symbol":
                files = sorted((scratch / "pkg").glob("mod_*.py"))
                assert len(files) == 40, len(files)
                holders = [f.name for f in files if "reconcile_ledger_7f3a" in f.read_text()]
                assert holders == ["mod_27.py"], holders
            if scenario.name == "read_count":
                assert sorted(p.name for p in scratch.iterdir()) == ["a.txt", "b.txt", "c.txt"]
        finally:
            shutil.rmtree(scratch)

    # opencode: the stream omits the last request's usage, so usage comes from the exported session.
    # The sample is a real three-request session. A later turn costs the difference from the totals
    # before it, and reasoning tokens count as output.
    export = {"info": {}, "messages": [
        {"role": None, "tokens": None},
        {"tokens": {"input": 2, "output": 18, "reasoning": 0, "cache": {"read": 0, "write": 4198}}},
        {"tokens": {"input": 2, "output": 18, "reasoning": 0, "cache": {"read": 4198, "write": 46}}},
        {"tokens": {"input": 2, "output": 11, "reasoning": 0, "cache": {"read": 4244, "write": 42}}},
        {"tokens": None}]}
    stream = "\n".join(json.dumps(e) for e in [
        {"type": "step_start", "sessionID": "ses_1"},
        {"type": "tool_use", "sessionID": "ses_1", "part": {"tool": "read"}},
        {"type": "tool_use", "sessionID": "ses_1", "part": {"tool": "read"}},
        {"type": "text", "sessionID": "ses_1", "part": {"text": "There are 3 lines total."}}])
    first, session, totals = parse_opencode(stream, export, {})
    assert (first.usage.fresh, first.usage.cache_read, first.usage.cache_write,
            first.usage.output, first.usage.requests) == (6, 8442, 4286, 47, 3), first
    assert first.tool_calls == 2 and session == "ses_1" and first.reply.startswith("There are 3")
    export["messages"].append({"tokens": {"input": 3, "output": 4, "reasoning": 6,
                                          "cache": {"read": 4286, "write": 20}}})
    second, _, _ = parse_opencode(stream, export, totals)
    assert (second.usage.fresh, second.usage.cache_read, second.usage.cache_write,
            second.usage.output, second.usage.requests) == (3, 4286, 20, 10, 1), second
    try:
        parse_opencode(stream, export, {"input": 0, "output": 0, "cache_read": 0,
                                        "cache_write": 0, "messages": 4})
        raise AssertionError("a turn with no new request must be an error")
    except BenchError:
        pass
    try:
        parse_opencode(json.dumps({"type": "error", "sessionID": "s",
                                   "error": {"message": "Token is expired"}}), export, {})
        raise AssertionError("an error event must raise")
    except BenchError as error:
        assert "expired" in str(error)

    # A model written as ${VAR} comes from the environment, and an unset variable is an error rather
    # than an empty --model.
    os.environ["BENCH_SELFTEST_MODEL"] = "m-1"
    assert model_of({"name": "x", "model": "${BENCH_SELFTEST_MODEL}"}) == "m-1"
    assert model_of({"name": "x"}) is None
    del os.environ["BENCH_SELFTEST_MODEL"]
    try:
        model_of({"name": "x", "model": "${BENCH_SELFTEST_MODEL}"})
        raise AssertionError("an unset model variable must raise")
    except BenchError:
        pass

    # A started tool sees the scenario directory as $PWD, whatever bench.py inherited.
    scratch = pathlib.Path(tempfile.mkdtemp())
    try:
        seen = execute([sys.executable, "-c", "import os; print(os.environ['PWD'], end='')"],
                       scratch, {"name": "x"})
        assert seen == str(scratch.resolve()), seen
    finally:
        shutil.rmtree(scratch)

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

    # A turn that asks for delegates fails when the tool reports fewer, however right the reply is.
    # A tool that does not report them is judged on the reply alone.
    fanout = {s.name: s for s in scenarios()}["delegate_fanout"]
    answer = "-".join(_part_key(n) for n in range(1, 5))
    fan_turn = fanout.turns[0]
    assert fan_turn.min_delegates == 4
    assert turn_ok(fan_turn, TurnResult(Usage(), answer, 0, delegates=4), None)
    assert not turn_ok(fan_turn, TurnResult(Usage(), answer, 0, delegates=3), None)
    assert not turn_ok(fan_turn, TurnResult(Usage(), answer, 0, delegates=0), None)
    assert turn_ok(fan_turn, TurnResult(Usage(), answer, 0, delegates=None), None)
    assert not turn_ok(fan_turn, TurnResult(Usage(), "wrong", 0, delegates=4), None)
    assert len({_part_key(n) for n in range(1, 5)}) == 4
    scratch = pathlib.Path(tempfile.mkdtemp())
    try:
        fanout.setup(scratch)
        for n in range(1, 5):
            text = (scratch / f"part_{n}.txt").read_text()
            keys = [line for line in text.splitlines() if line.startswith("key:")]
            assert keys == [f"key: {_part_key(n)}"], keys
    finally:
        shutil.rmtree(scratch)

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
    selftest_configuration()
    selftest_modes()
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
