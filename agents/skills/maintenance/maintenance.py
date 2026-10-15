#!/usr/bin/env python3
"""Find code to clean up, have reviewers decide which of it is worth an issue, and draft those.

    python3 agents/skills/maintenance/maintenance.py [lane ...] [path ...] [--mechanical-only]
    python3 agents/skills/maintenance/maintenance.py collect --work-dir DIR
    python3 agents/skills/maintenance/maintenance.py draft --work-dir DIR

The scan reads every tracked source file and lists candidates in six lanes: public items nothing
names, items only tests name, attributes that silence the dead-code lint, blocks of lines repeated
in several places, string literals repeated across files, and files or functions past a length.
A candidate is something to look at, not a finding. The skill hands each lane to reviewers who
read the code and decide, and only what a reviewer calls `cleanup` becomes a draft.

Nothing here edits the tree or posts anything. `--mechanical-only` prints the candidates and
exits, which is what `make check-maintenance` runs. Standard library only, like the other checks.
"""

import argparse
import bisect
import datetime
import importlib.util
import json
import os
import re
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


specs = _load("check_spec_specs", HERE.parent / "check-spec" / "specs.py")
drafting = _load("check_spec_draft_issues", HERE.parent / "check-spec" / "draft-issues.py")

LANES = ("unreferenced", "test-only", "allowances", "duplicates", "literals", "size")
PREFIX = {
    "unreferenced": "U",
    "test-only": "T",
    "allowances": "A",
    "duplicates": "D",
    "literals": "L",
    "size": "S",
}
# Candidates per reviewer. A size candidate is a whole file, often thousands of lines, so a
# reviewer gets few of them; the other lanes point at one item each.
CHUNK = {"duplicates": 8, "size": 3}
DEFAULT_CHUNK = 15
LIMIT = 30
SIZE_LIMIT = 12

# A file past this many lines outside its tests, or a function past this many non-blank lines.
FILE_LINES = 2500
FUNCTION_LINES = 150
# A repeated block is at least this many non-blank lines, of which most are not braces or imports.
WINDOW = 8
WINDOW_SUBSTANTIAL = 6
WINDOW_CHARS = 200
# A repeated literal is at least this long and appears this often, in this many files.
LITERAL_LENGTH = 10
LITERAL_COPIES = 3
LITERAL_FILES = 2

AREAS = (
    "area/trust", "area/tools", "area/turns", "area/interface", "area/cli",
    "area/delegation", "area/backends", "area/skus", "area/i18n",
)
INFRASTRUCTURE = "infrastructure"
INFRASTRUCTURE_PATHS = ("contrib/", "agents/", ".github/", "Makefile", "scripts/")
KIND, BROKEN = "refactor", "bug"
ASSIGNEE = "netzenbot"

LANGUAGE = {
    ".rs": "rust", ".py": "python",
    ".ts": "js", ".tsx": "js", ".mts": "js", ".js": "js", ".mjs": "js", ".cjs": "js",
}
# Files that are not source but can still name a source item: a Makefile target running a script
# by function name, a workflow, a manifest. Read as plain text for references only.
REFERENCE_ONLY = re.compile(r"(?:^|/)(?:Makefile|[^/]+\.(?:toml|ya?ml|json|sh|html|css))$")
NEVER_READ = re.compile(r"(?:^|/)(?:Cargo\.lock|pnpm-lock\.yaml|package-lock\.json)$")
LARGEST_READ = 4_000_000

TEST_DIRECTORIES = {
    "tests", "test", "benches", "examples", "test-support", "test_support", "testing",
    "fixtures", "test-fixtures", "__tests__", "e2e",
}
TEST_NAME = {
    "rust": re.compile(r"^(?:tests?|.*_tests?)\.rs$"),
    "python": re.compile(r"^(?:test_.*|.*_test|.*selftest)\.py$"),
    "js": re.compile(r"\.(?:test|spec)\.[cm]?[jt]sx?$"),
}

IDENT = re.compile(r"[A-Za-z_$][\w$]*")

RUST_TOKEN = re.compile(
    r"//|/\*|(?P<raw>b?r(?P<hashes>#*)\")|(?P<str>b?\")"
    r"|(?P<char>b?'(?:[^'\\\n]|\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.))')"
)
PYTHON_TOKEN = re.compile(r"#|(?<![\w])(?P<prefix>[rRbBuUfF]{0,2})(?P<quote>'''|\"\"\"|'|\")")
JS_TOKEN = re.compile(r"//|/\*|['\"`]")

RUST_PUB = re.compile(
    r"^\s*pub\s+(?:(?:const|async|unsafe|default|extern(?:\s+\"[^\"]*\")?)\s+)*"
    r"(fn|struct|enum|trait|type|const|static|union)\s+(?:mut\s+)?(?:r#)?([A-Za-z_]\w*)"
)
PYTHON_DEF = re.compile(r"^(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)|^([A-Z][A-Z0-9_]*)\s*(?::[^=]*)?=")
JS_EXPORT = re.compile(
    r"^\s*export\s+(?:declare\s+)?(?:async\s+)?"
    r"(?:function\*?|const|let|var|class|abstract\s+class|interface|type|enum)\s+([A-Za-z_$][\w$]*)"
)
CFG = re.compile(r"#\[cfg\(([^\]]*)\)\]")
MODULE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;", re.MULTILINE)
ALLOWANCE = re.compile(r"#!?\[\s*(?:cfg_attr\s*\((.*?),\s*)?(?:allow|expect)\s*\(([^)\]]*)\)")
# A `cfg_attr` that silences the lint on some platforms only. The lint still runs on the others,
# so what it covers is used somewhere and the allowance hides nothing a build would catch.
PLATFORM_ONLY = re.compile(
    r"(?:not\s*\(\s*)?(?:unix|windows|target_(?:os|family|arch)\s*=\s*\"[^\"]*\")(?:\s*\))?"
)
RUST_FN = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
PYTHON_FN = re.compile(r"^(\s*)(?:async\s+)?def\s+([A-Za-z_]\w*)")
JS_FN = re.compile(
    r"\bfunction\s*\*?\s*([A-Za-z_$][\w$]*)\s*[<(]"
    r"|\b(?:const|let)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?"
    r"(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>\s*\{"
)
TRIVIAL = re.compile(
    r"^[\s{}()\[\];,.:<>|&]*$"
    r"|^(?:use|import|from|mod|pub mod|pub use|pub\(crate\) use|export \{|export \*)\b"
    r"|^#!?\["
)
NOT_A_LITERAL_LINE = re.compile(
    r"^\s*(?:#!?\[|use\b|import\b|from\b|export\s.*\bfrom\b|mod\b)|\brequire\s*\("
)
TITLE_LIMIT = drafting.TITLE_LIMIT


def blank(text):
    return "\n".join(" " * len(part) for part in text.split("\n"))


def line_starts(text):
    return [0] + [found.end() for found in re.finditer("\n", text)]


def line_of(starts, index):
    """The 1-based line holding `index`, from `line_starts` of the same text."""
    return bisect.bisect_right(starts, index)


class Source:
    """One tracked file, read once, in the three forms the lanes need.

    `code` has comments blanked and strings kept, since a name inside a string can still be a use
    of it (a serde attribute, a name looked up at run time). `bare` has strings blanked too, so a
    brace or an attribute written inside a string is not mistaken for one. Both keep every line
    where it was, so a line number means the same thing in either.
    """

    def __init__(self, path, language, text):
        self.path = path
        self.language = language
        self.text = text
        if language in TOKENIZE:
            code, bare, literals = TOKENIZE[language](text)
        else:
            code, bare, literals = text, text, []
        self.code = code.split("\n")
        self.bare = bare.split("\n")
        self.literals = literals
        self.whole_test = language in TEST_NAME and is_test_path(path, language)
        self.test = set(range(1, len(self.code) + 1)) if self.whole_test else set()
        self.modules, self.test_modules = [], []
        if language == "rust":
            lines, self.modules, self.test_modules = rust_modules(bare)
            if not self.whole_test:
                self.test = lines

    def make_test(self):
        self.whole_test = True
        self.test = set(range(1, len(self.code) + 1))

    def is_test(self, line):
        return line in self.test

    @property
    def lines(self):
        return len(self.code)


def is_test_path(path, language):
    parts = path.split("/")
    return bool(TEST_DIRECTORIES.intersection(parts[:-1])) or bool(TEST_NAME[language].search(parts[-1]))


def tokenize_rust(text):
    code, bare, literals = [], [], []
    starts = line_starts(text)
    pos = 0
    while True:
        found = RUST_TOKEN.search(text, pos)
        if not found:
            break
        start = found.start()
        code.append(text[pos:start])
        bare.append(text[pos:start])
        token = found.group(0)
        if token == "//":
            end = text.find("\n", start)
            end = len(text) if end < 0 else end
            code.append(blank(text[start:end]))
            bare.append(blank(text[start:end]))
        elif token == "/*":
            depth, i = 1, start + 2
            while depth and i < len(text):
                pair = text[i : i + 2]
                if pair == "/*":
                    depth, i = depth + 1, i + 2
                elif pair == "*/":
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            end = i
            code.append(blank(text[start:end]))
            bare.append(blank(text[start:end]))
        elif found.group("char"):
            end = found.end()
            code.append(token)
            bare.append(blank(token))
        else:
            if found.group("raw"):
                closing = '"' + found.group("hashes")
                close = text.find(closing, found.end())
                close = len(text) if close < 0 else close
            else:
                closing, close = '"', found.end()
                while close < len(text) and text[close] != '"':
                    close += 2 if text[close] == "\\" else 1
                close = min(close, len(text))
            end = min(len(text), close + len(closing))
            content = text[found.end() : close]
            literals.append((line_of(starts, start), content))
            code.append(text[start:end])
            bare.append(text[start : found.end()] + blank(content) + text[close:end])
        pos = end
    code.append(text[pos:])
    bare.append(text[pos:])
    return "".join(code), "".join(bare), literals


def tokenize_python(text):
    code, bare, literals = [], [], []
    starts = line_starts(text)
    pos = 0
    while True:
        found = PYTHON_TOKEN.search(text, pos)
        if not found:
            break
        start = found.start()
        code.append(text[pos:start])
        bare.append(text[pos:start])
        if found.group(0) == "#":
            end = text.find("\n", start)
            end = len(text) if end < 0 else end
            code.append(blank(text[start:end]))
            bare.append(blank(text[start:end]))
            pos = end
            continue
        quote = found.group("quote")
        close = found.end()
        while close < len(text):
            if text[close] == "\\":
                close += 2
                continue
            if text.startswith(quote, close) or (len(quote) == 1 and text[close] == "\n"):
                break
            close += 1
        close = min(close, len(text))
        end = min(len(text), close + (len(quote) if text.startswith(quote, close) else 0))
        content = text[found.end() : close]
        literals.append((line_of(starts, start), content))
        code.append(text[start:end])
        bare.append(text[start : found.end()] + blank(content) + text[close:end])
        pos = end
    code.append(text[pos:])
    bare.append(text[pos:])
    return "".join(code), "".join(bare), literals


def tokenize_js(text):
    code, bare, literals = [], [], []
    starts = line_starts(text)
    pos = 0
    while True:
        found = JS_TOKEN.search(text, pos)
        if not found:
            break
        start = found.start()
        code.append(text[pos:start])
        bare.append(text[pos:start])
        token = found.group(0)
        if token in ("//", "/*"):
            if token == "//":
                end = text.find("\n", start)
                end = len(text) if end < 0 else end
            else:
                end = text.find("*/", start + 2)
                end = len(text) if end < 0 else end + 2
            code.append(blank(text[start:end]))
            bare.append(blank(text[start:end]))
            pos = end
            continue
        close = start + 1
        while close < len(text):
            if text[close] == "\\":
                close += 2
                continue
            if text[close] == token or (token != "`" and text[close] == "\n"):
                break
            close += 1
        close = min(close, len(text))
        end = min(len(text), close + (1 if close < len(text) and text[close] == token else 0))
        content = text[start + 1 : close]
        literals.append((line_of(starts, start), content))
        code.append(text[start:end])
        bare.append(token + blank(content) + text[close:end])
        pos = end
    code.append(text[pos:])
    bare.append(text[pos:])
    return "".join(code), "".join(bare), literals


TOKENIZE = {"rust": tokenize_rust, "python": tokenize_python, "js": tokenize_js}


def matching_brace(text, opening):
    """The index just past the brace that closes the one at `opening`, in text with no strings."""
    depth = 0
    for i in range(opening, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i + 1
    return len(text)


def body_start(text, start):
    """The first `{` or `;` after `start` outside parentheses and brackets: where an item's body
    opens, or where an item with no body ends. A `;` inside `[u8; 4]` is neither."""
    depth = 0
    for i in range(start, len(text)):
        c = text[i]
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif depth <= 0 and c in "{;":
            return i
    return len(text)


ITEM = re.compile(
    r"(?:pub(?:\([^)]*\))?\s+)?(?:(?:unsafe|async|const|extern(?:\s+\"[^\"]*\")?)\s+)*"
    r"(?:fn|mod|use|impl|struct|enum|trait|type|static|const|union|macro_rules!)\b"
)
ATTRIBUTES = re.compile(r"(?:\s*#!?\[[^\]]*\])*\s*")
FIELD = re.compile(r"(?:pub(?:\([^)]*\))?\s+)?[A-Za-z_]\w*\s*(?::(?!:)|,)")


def field_end(text, start):
    """The index just past a field that begins at `start`: its `,` outside parentheses, brackets,
    braces and angle brackets, or the end of the text before the `}` closing the struct it is in."""
    depth = 0
    for i in range(start, len(text)):
        c = text[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                return i
            depth -= 1
        elif c == "<":
            depth += 1
        elif c == ">" and text[i - 1] not in "-=":
            depth = max(depth - 1, 0)
        elif c == "," and depth == 0:
            return i + 1
    return len(text)


def attributed_end(bare, after):
    """The index where the item or field attributed by an attribute ending at `after` ends, and
    whether it is a `{ }` block. A field of a struct, or of a struct literal, ends at its `,` and
    the next `{` belongs to something else; anything else runs to its body's end or its `;`."""
    first = ATTRIBUTES.match(bare, after).end()
    if FIELD.match(bare, first) and not ITEM.match(bare, first):
        return field_end(bare, first), False
    at = body_start(bare, after)
    if at < len(bare) and bare[at] == "{":
        return matching_brace(bare, at), True
    return at + 1, False


def rust_modules(bare):
    """The lines of items under `#[cfg(test)]`, or under a `cfg` that needs `test` to be set, and
    the names of the `mod name;` declarations in the file: every one, and those under such a `cfg`.
    A module declared that way lives in a file of its own, which is test code wherever it sits."""
    lines, test_modules = set(), []
    starts = line_starts(bare)
    for found in CFG.finditer(bare):
        condition = found.group(1)
        if not re.search(r"\btest\b", condition) or re.search(r"not\s*\(\s*test\b", condition):
            continue
        end, braced = attributed_end(bare, found.end())
        if not braced:
            declared = MODULE.search(bare, found.end(), end)
            if declared:
                test_modules.append(declared.group(1))
        lines.update(range(line_of(starts, found.start()), line_of(starts, end) + 1))
    return lines, [m.group(1) for m in MODULE.finditer(bare)], test_modules


def module_files(path, name):
    """Where `mod name;` in the file at `path` is read from."""
    here = Path(path)
    base = here.parent if here.name in ("lib.rs", "main.rs", "mod.rs") else here.parent / here.stem
    return (str(base / f"{name}.rs"), str(base / name / "mod.rs"))


def mark_test_modules(sources):
    """Every Rust file reached through a `#[cfg(test)] mod name;`, and every file those declare."""
    waiting = [
        file
        for source in sources.values()
        for name in source.test_modules
        for file in module_files(source.path, name)
    ]
    while waiting:
        source = sources.get(waiting.pop())
        if source is None or (source.whole_test and not source.modules):
            continue
        source.make_test()
        waiting += [file for name in source.modules for file in module_files(source.path, name)]
        source.modules = []


def tracked_files(root):
    listed = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"], capture_output=True, text=True, check=True
    )
    return sorted(name for name in listed.stdout.split("\0") if name)


def read_tree(root):
    """Every tracked file worth reading, as `Source`, keyed by path."""
    sources = {}
    for name in tracked_files(root):
        if NEVER_READ.search(name):
            continue
        language = LANGUAGE.get(Path(name).suffix)
        if language is None and not REFERENCE_ONLY.search(name):
            continue
        path = root / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > LARGEST_READ:
            continue
        raw = path.read_bytes()
        if b"\0" in raw[:8192]:
            continue
        sources[name] = Source(name, language, raw.decode("utf-8", errors="replace"))
    mark_test_modules(sources)
    return sources


def is_generated(source):
    head = "\n".join(source.text.splitlines()[:5]).lower()
    return "@generated" in head or "do not edit" in head or "generated by" in head


def references(sources):
    """How often each identifier appears outside comments, split into test and non-test code."""
    outside, inside = Counter(), Counter()
    for source in sources.values():
        for number, line in enumerate(source.code, start=1):
            names = IDENT.findall(line)
            if names:
                (inside if source.is_test(number) else outside).update(names)
    return outside, inside


def definitions(source):
    """(line, name, kind) for each item this file offers to the rest of the tree."""
    found = []
    for number, line in enumerate(source.bare, start=1):
        if source.is_test(number):
            continue
        if source.language == "rust":
            match = RUST_PUB.match(line)
            if match:
                found.append((number, match.group(2), match.group(1)))
        elif source.language == "python":
            match = PYTHON_DEF.match(line)
            if match:
                name = match.group(1) or match.group(2)
                if name != "main" and not name.startswith("__"):
                    found.append((number, name, "def" if match.group(1) else "constant"))
        elif source.language == "js":
            match = JS_EXPORT.match(line)
            if match:
                found.append((number, match.group(1), "export"))
    return found


def scan_references(sources):
    outside, inside = references(sources)
    defined = Counter()
    every = []
    for source in sources.values():
        if source.language is None or source.whole_test or is_generated(source):
            continue
        for number, name, kind in definitions(source):
            defined[name] += 1
            every.append((source.path, number, name, kind))
    unreferenced, test_only = [], []
    for path, number, name, kind in every:
        if outside[name] > defined[name]:
            continue
        item = {"path": path, "line": number, "symbol": name, "item": kind}
        if inside[name]:
            item["detail"] = f"`{kind} {name}`: {inside[name]} uses, every one of them in test code"
            test_only.append(item)
        else:
            item["detail"] = f"`{kind} {name}`: named nowhere but its definition"
            unreferenced.append(item)
    return {"unreferenced": unreferenced, "test-only": test_only}


def scan_allowances(sources):
    found = []
    for source in sources.values():
        if source.language != "rust":
            continue
        for number, line in enumerate(source.bare, start=1):
            if source.is_test(number):
                continue
            for match in ALLOWANCE.finditer(line):
                lints = match.group(2)
                if match.group(1) and PLATFORM_ONLY.fullmatch(match.group(1).strip()):
                    continue
                if "dead_code" in lints or "unused" in lints:
                    found.append(
                        {
                            "path": source.path,
                            "line": number,
                            "detail": f"`{line.strip()}`",
                        }
                    )
    return found


def segments(sources):
    """Runs of non-blank, non-test lines, normalised: what a repeated block is looked for in."""
    runs = []
    for source in sources.values():
        if source.language is None or source.whole_test or is_generated(source):
            continue
        current = []
        for number, line in enumerate(source.code, start=1):
            if source.is_test(number):
                if current:
                    runs.append((source.path, current))
                current = []
                continue
            normal = " ".join(line.split())
            if normal:
                current.append((number, normal, bool(TRIVIAL.match(normal))))
        if current:
            runs.append((source.path, current))
    return runs


def scan_duplicates(sources):
    runs = segments(sources)
    windows = {}
    for index, (_, lines) in enumerate(runs):
        for k in range(len(lines) - WINDOW + 1):
            chunk = lines[k : k + WINDOW]
            if sum(not trivial for _, _, trivial in chunk) < WINDOW_SUBSTANTIAL:
                continue
            if sum(len(normal) for _, normal, _ in chunk) < WINDOW_CHARS:
                continue
            key = hash(tuple(normal for _, normal, _ in chunk))
            windows.setdefault(key, []).append((index, k))

    repeated = {}
    for key, places in windows.items():
        kept = []
        for index, k in sorted(places):
            if kept and kept[-1][0] == index and k - kept[-1][1] < WINDOW:
                continue
            kept.append((index, k))
        if len(kept) >= 2:
            repeated[key] = kept
    owner = {place: key for key, places in repeated.items() for place in places}

    def shifted(places, by):
        keys = {owner.get((index, k + by)) for index, k in places}
        if len(keys) != 1 or None in keys:
            return None
        key = keys.pop()
        return key if len(repeated[key]) == len(places) else None

    groups = []
    for key, places in repeated.items():
        if shifted(places, -1) is not None:
            continue
        length = 1
        while shifted(places, length) is not None:
            length += 1
        copies = []
        for index, k in places:
            path, lines = runs[index]
            copies.append((path, lines[k][0], lines[k + length + WINDOW - 2][0]))
        groups.append((length + WINDOW - 1, copies))
    return [duplicate(cluster) for cluster in clusters(groups)]


def clusters(groups):
    """Groups of repeated lines whose copies overlap, joined into one.

    A block copied into nine places, with a line changed in two of them, is two runs of matching
    windows over the same lines: one across all nine copies, and a longer one across the seven
    that still agree. That is one thing to fix, so it is one candidate."""
    parent = list(range(len(groups)))

    def root(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    spans = sorted(
        (path, first, last, number)
        for number, (_, copies) in enumerate(groups)
        for path, first, last in copies
    )
    reach = None
    for path, first, last, number in spans:
        if reach and reach[0] == path and first <= reach[1]:
            parent[root(number)] = root(reach[2])
            reach = (path, max(reach[1], last), reach[2])
        else:
            reach = (path, last, number)
    joined = {}
    for number in range(len(groups)):
        joined.setdefault(root(number), []).append(groups[number])
    return list(joined.values())


def duplicate(cluster):
    spans = sorted((path, first, last) for _, copies in cluster for path, first, last in copies)
    merged = []
    for path, first, last in spans:
        if merged and merged[-1]["path"] == path and first <= merged[-1]["end"]:
            merged[-1]["end"] = max(merged[-1]["end"], last)
        else:
            merged.append({"path": path, "line": first, "end": last})
    longest = max(size for size, _ in cluster)
    shown = ", ".join(f"{c['path']}:{c['line']}-{c['end']}" for c in merged[:12])
    if len(merged) > 12:
        shown += f", and {len(merged) - 12} more"
    return {
        "path": merged[0]["path"],
        "line": merged[0]["line"],
        "copies": merged,
        "lines": longest,
        "rank": sum(size * (len(copies) - 1) for size, copies in cluster),
        "detail": f"up to {longest} matching lines, in {len(merged)} places: {shown}",
    }


def scan_literals(sources, blocks=()):
    """String literals spelled out in several files, ranked by copies times the directories they
    span, since a value two components must agree on matters more than one a family of scripts
    repeats. A word on its own, such as a JSON key, an argparse action or an environment
    variable's name, is left out, because a constant holding it says nothing the word does not.
    So is a literal whose every copy sits in a repeated block, since sharing the block shares it."""
    covered = {}
    for block in blocks:
        for copy in block["copies"]:
            covered.setdefault(copy["path"], []).append((copy["line"], copy["end"]))
    places = {}
    for source in sources.values():
        if source.language is None or source.whole_test or is_generated(source):
            continue
        for number, value in source.literals:
            if source.is_test(number) or "\n" in value:
                continue
            if len(value.strip()) < LITERAL_LENGTH or not re.search(r"[A-Za-z]", value):
                continue
            if re.fullmatch(r"[A-Za-z_][\w-]*", value):
                continue
            if NOT_A_LITERAL_LINE.search(source.code[number - 1]):
                continue
            places.setdefault((source.language, value), []).append((source.path, number))
    found = []
    for (language, value), where in places.items():
        files = sorted({path for path, _ in where})
        if len(where) < LITERAL_COPIES or len(files) < LITERAL_FILES:
            continue
        if all(
            any(first <= number <= last for first, last in covered.get(path, ()))
            for path, number in where
        ):
            continue
        shown = value if len(value) <= 80 else value[:77] + "..."
        found.append(
            {
                "path": where[0][0],
                "line": where[0][1],
                "copies": [{"path": path, "line": number} for path, number in where],
                "literal": value,
                "rank": len(where) * len({str(Path(path).parent) for path in files}),
                "detail": f"{json.dumps(shown)} {len(where)} times in {len(files)} files: "
                + ", ".join(f"{path}:{number}" for path, number in where[:12])
                + (f", and {len(where) - 12} more" if len(where) > 12 else ""),
            }
        )
    return found


def functions(source):
    """(name, first line, last line, non-blank lines) for each function outside test code."""
    found = []
    if source.language in ("rust", "js"):
        text = "\n".join(source.bare)
        starts = line_starts(text)
        pattern = RUST_FN if source.language == "rust" else JS_FN
        for match in pattern.finditer(text):
            name = next(group for group in match.groups() if group)
            if source.language == "rust":
                at = body_start(text, match.end())
                if at >= len(text) or text[at] != "{":
                    continue
            else:
                at = text.rfind("{", match.start(), match.end()) if text[match.end() - 1] == "{" else -1
                if at < 0:
                    at = body_start(text, match.end())
                    if at >= len(text) or text[at] != "{":
                        continue
            end = matching_brace(text, at)
            first = line_of(starts, match.start())
            last = line_of(starts, end)
            if source.is_test(first):
                continue
            counted = sum(1 for line in source.code[first - 1 : last] if line.strip())
            found.append((name, first, last, counted))
    elif source.language == "python":
        lines = source.code
        for index, line in enumerate(lines):
            match = PYTHON_FN.match(line)
            if not match:
                continue
            indent = len(match.group(1))
            depth, body = 0, index
            for body in range(index, len(lines)):
                depth += lines[body].count("(") + lines[body].count("[")
                depth -= lines[body].count(")") + lines[body].count("]")
                if depth <= 0 and lines[body].rstrip().endswith(":"):
                    break
            last = body
            for after in range(body + 1, len(lines)):
                text = lines[after]
                if not text.strip():
                    continue
                if len(text) - len(text.lstrip()) <= indent:
                    break
                last = after
            counted = sum(1 for text in lines[index : last + 1] if text.strip())
            found.append((match.group(2), index + 1, last + 1, counted))
    return found


def scan_size(sources):
    found = []
    for source in sources.values():
        if source.language is None or source.whole_test or is_generated(source):
            continue
        outside = sum(
            1
            for number, line in enumerate(source.code, start=1)
            if line.strip() and not source.is_test(number)
        )
        long = [one for one in functions(source) if one[3] > FUNCTION_LINES]
        if outside <= FILE_LINES and not long:
            continue
        parts = [f"{outside} non-blank lines outside tests"]
        if long:
            parts.append(
                "functions over "
                + str(FUNCTION_LINES)
                + " lines: "
                + ", ".join(f"`{name}` {first}-{last} ({count})" for name, first, last, count in long)
            )
        found.append(
            {
                "path": source.path,
                "line": 1,
                "lines": outside,
                "functions": [
                    {"name": name, "line": first, "end": last, "lines": count}
                    for name, first, last, count in long
                ],
                "rank": max(0, outside - FILE_LINES) + sum(c - FUNCTION_LINES for *_, c in long),
                "detail": "; ".join(parts),
            }
        )
    return found


def governed_by(root, loaded):
    """Each spec's governed paths, expanded: a glob to the files it matches, a directory whole."""
    table = []
    for spec in loaded:
        exact, prefixes = set(), []
        for pattern in spec.governs:
            if any(c in pattern for c in "*?["):
                exact.update(str(p.relative_to(root)) for p in root.glob(pattern))
            elif (root / pattern).is_dir():
                prefixes.append(pattern.rstrip("/") + "/")
            else:
                exact.add(pattern)
        bare_guards = {}
        for symbol in spec.guards:
            bare = re.split(r"::|\.", symbol.strip("`").rstrip("()"))[-1]
            bare_guards.setdefault(bare, []).append(symbol)
        table.append((rel_spec(root, spec), exact, prefixes, bare_guards))
    return table


def rel_spec(root, spec):
    try:
        return str(spec.path.resolve().relative_to(root))
    except ValueError:
        return spec.rel


def annotate(candidate, table, sources):
    """The specs that govern a candidate's files, and the guards it names. A fix touching either
    has a spec to keep true, and a guard's pinned sites to update when a use moves."""
    paths = [c["path"] for c in candidate.get("copies", [])] or [candidate["path"]]
    governing, guarded = set(), set()
    names = set()
    if candidate.get("symbol"):
        names.add(candidate["symbol"])
    for copy in candidate.get("copies", [])[:1]:
        source = sources.get(copy["path"])
        if source and copy.get("end"):
            for line in source.code[copy["line"] - 1 : copy["end"]]:
                names.update(IDENT.findall(line))
    for spec, exact, prefixes, guards in table:
        if any(p in exact or any(p.startswith(pre) for pre in prefixes) for p in paths):
            governing.add(spec)
        for name in names:
            for symbol in guards.get(name, []):
                guarded.add(f"{symbol} ({spec})")
    if governing:
        candidate["specs"] = sorted(governing)
    if guarded:
        candidate["guards"] = sorted(guarded)


def in_scope(candidate, prefixes):
    if not prefixes:
        return True
    paths = [c["path"] for c in candidate.get("copies", [])] or [candidate["path"]]
    return any(p == pre or p.startswith(pre.rstrip("/") + "/") for p in paths for pre in prefixes)


def scan(root, lanes=LANES, prefixes=(), limit=LIMIT, size_limit=SIZE_LIMIT):
    """Candidates per lane, ranked, capped, numbered, and annotated with the specs they touch.

    References are counted over the whole tree whatever `prefixes` says, because an item under
    one directory is used from another, and only the report is scoped."""
    sources = read_tree(root)
    found = {}
    if "unreferenced" in lanes or "test-only" in lanes:
        found.update(scan_references(sources))
    if "allowances" in lanes:
        found["allowances"] = scan_allowances(sources)
    if "duplicates" in lanes or "literals" in lanes:
        found["duplicates"] = scan_duplicates(sources)
    if "literals" in lanes:
        found["literals"] = scan_literals(sources, found["duplicates"])
    if "size" in lanes:
        found["size"] = scan_size(sources)

    table = governed_by(root, specs.load_specs(root / "docs" / "specs"))
    result = {}
    for lane in LANES:
        if lane not in lanes:
            continue
        every = [c for c in found.get(lane, []) if in_scope(c, prefixes)]
        every.sort(key=lambda c: (-c.get("rank", 0), c["path"], c["line"]))
        cap = size_limit if lane == "size" else limit
        kept = every[:cap]
        for number, candidate in enumerate(kept, start=1):
            candidate["id"] = f"{PREFIX[lane]}{number}"
            candidate["lane"] = lane
            annotate(candidate, table, sources)
        result[lane] = {"candidates": kept, "left_out": len(every) - len(kept)}
    return result


def report(result):
    lines = []
    total = 0
    for lane, entry in result.items():
        candidates = entry["candidates"]
        total += len(candidates)
        lines.append("")
        lines.append(f"{lane}: {len(candidates)}" + (f", {entry['left_out']} more past the cap" if entry["left_out"] else ""))
        for candidate in candidates:
            lines.append(f"  {candidate['id']:<4} {candidate['path']}:{candidate['line']}  {candidate['detail']}")
            for name in ("specs", "guards"):
                if candidate.get(name):
                    lines.append(f"       {name}: {', '.join(candidate[name])}")
    lines.append("")
    lines.append(f"{total} candidates in {len(result)} lanes. A candidate is not a finding until a reviewer reads it.")
    return "\n".join(lines)


PROMPT = """\
# Maintenance review: {lane}, candidates {first} to {last}

You are deciding which of these candidates is worth an issue. A scan found them by counting
names and lines; it did not read the code. You read it.

## The rule that outranks every cleanup

The driver and the planner never have untrusted content in their context. The driver is
`bravebot-core` and `bravebot-agent` both; the planner is the model. The driver may carry
untrusted content and hand it to an effect, but may not branch on it: no `if`, `match`,
comparison, or early return whose condition derives from untrusted bytes. Moving such a branch
from `bravebot-agent` into `bravebot-core` does not fix it, and relocating a decision is not
removing it.

A cleanup can break this without changing what any test sees. Merging two functions, moving one
into a shared crate, or hoisting a check into a helper can move a decision about labelled content
to a place nothing reviews, or give a value a better label than its inputs had. Never propose a
change that moves, merges or hides code that reads, labels or declassifies content. Where the
duplication or the length is in such code, the verdict is `keep`.

docs/specs/layering.md says which crate may depend on which. A shared version of two copies goes
in a crate both callers may already depend on, or it does not go anywhere.

## What the scan found

{question}

{candidates}

`specs` names the specs that govern a candidate's file: a fix there has to keep each one true,
so read the clauses it touches. `guards` names a symbol a spec pins: its uses are counted in the
spec's front matter, and a fix that moves or removes one updates that count in the same change.

## The job

For every candidate:

1. Read the code. Grep the name or the text yourself, across the whole tree, including strings,
   macros, build scripts, the Makefile, `.github/`, other languages, and docs. The scan counts
   identifiers, so it misses a use built from parts or reached by reflection, and it calls two
   unrelated items with one name a use of each other.
2. Look for the reason it is the way it is before calling it a mess. A comment, a spec clause, a
   platform `cfg`, or a public API another repository uses is a reason.
3. Decide one of:
   - `cleanup`: changing it makes the tree smaller or easier to work on, behaviour stays the
     same, and you can say exactly what the change is.
   - `keep`: it is the way it should be. Say why in a sentence.
   - `unclear`: you could not tell. Say what you would need.

Then write a finding for each change worth an issue, naming the candidates it covers. Related
candidates are one finding: three copies of one block, or an unused function and the constant
only it reads, are one change. A finding can name candidates from the list only, plus anything
else you found in the code you read that is hard to maintain, as a finding with `"candidates": []`.

A finding is the draft of an issue somebody will be assigned to fix without redoing your review,
so the fix says what to delete or move, where a shared version goes, and which callers change.
Every `file:line` you cite is one you read.

If what you find is not cleanup but a defect, such as a check that only tests call because the
call that should make it was removed, set `"bug": true` and say what breaks.

{lane_notes}

## Output

Write JSON to `{results_file}` and nothing else to stdout:

```json
{{
  "lane": "{lane}",
  "candidates": [
    {{"id": "{first}", "verdict": "cleanup | keep | unclear", "reason": "one sentence"}}
  ],
  "findings": [
    {{
      "candidates": ["{first}"],
      "summary": "one sentence stating what is wrong, which becomes the issue title",
      "detail": "what a reader needs beyond the summary, or leave it out",
      "evidence": ["crates/.../file.rs:123 what is there"],
      "fix": "what to change, concretely enough to do without redoing the review",
      "area": "one of {areas}, or leave it out",
      "bug": false
    }}
  ]
}}
```

One verdict for every candidate in your list. No prose outside the file. Do not edit any file.
"""

QUESTION = {
    "unreferenced": (
        "Each of these is a public item whose name appears nowhere in the tracked tree outside "
        "comments except where it is defined. rustc does not warn about a `pub` item a library "
        "crate exports, and nothing warns about an unused Python or TypeScript export, so these "
        "are invisible to every check."
    ),
    "test-only": (
        "Each of these is defined in code that ships and named only by tests. Either the tests "
        "are testing something nothing uses, or the item belongs in the test code that uses it."
    ),
    "allowances": (
        "Each of these silences the compiler's dead-code or unused warning. Remove it in your "
        "head and decide whether the warning it hides is real."
    ),
    "duplicates": (
        "Each of these is a block of lines, compared after comments and whitespace are removed, "
        "that appears in more than one place outside tests."
    ),
    "literals": (
        "Each of these is a string literal that appears in several files outside tests. Spelled "
        "out in each place, a change to one copy is a change somebody forgets to make to the rest."
    ),
    "size": (
        f"Each of these is a file over {FILE_LINES} non-blank lines outside its tests, or one with "
        f"functions over {FUNCTION_LINES} non-blank lines. Read the file, not only the parts the "
        "scan names."
    ),
}

LANE_NOTES = {
    "unreferenced": (
        "The usual cleanup is deletion, together with whatever only the deleted item used. "
        "Reasons to keep one: an attribute that exports it (`#[no_mangle]`, `#[wasm_bindgen]`, "
        "a serde or clap derive that names it), a public API a crate outside this repository "
        "uses, or a spec that names it."
    ),
    "test-only": (
        "The cleanup is either deleting it along with the tests that only test it, or moving it "
        "under `#[cfg(test)]` or into the test-support code that uses it. If it implements "
        "something a spec clause says the product does, that is a `bug`, not a cleanup."
    ),
    "allowances": (
        "An allowance under a platform `cfg_attr` covering code only another platform calls is a "
        "reason to keep. An allowance covering a field only `Debug` reads, a helper nothing calls, "
        "or code left from a removed feature is cleanup: the fix removes the attribute and what it "
        "was hiding."
    ),
    "duplicates": (
        "Keep copies that are short, in crates that may not depend on each other, or expected to "
        "change separately. A shared function that needs a flag to tell its callers apart is worse "
        "than two copies, so say so and keep them."
    ),
    "literals": (
        "The cleanup is one constant, in a module every user may depend on. Keep copies that mean "
        "different things and happen to be spelled alike, and text a person reads, such as an "
        "error message, unless it is matched on somewhere."
    ),
    "size": (
        "A cleanup proposes a specific split: which items move to which new module or function, "
        "and what stays. Keep a file that is long because it is a table, a match over every "
        "variant, or one cohesive state machine. Report anything else in the file that is hard to "
        "work on, such as a function doing several jobs or near-copies of one branch, as findings "
        "with `\"candidates\": []`."
    ),
}


def render_candidate(candidate):
    lines = [f"- **{candidate['id']}** `{candidate['path']}:{candidate['line']}` {candidate['detail']}"]
    for name in ("specs", "guards"):
        if candidate.get(name):
            lines.append(f"  - {name}: " + ", ".join(f"`{v}`" for v in candidate[name]))
    return "\n".join(lines)


def build_prompt(lane, chunk, results_file):
    return PROMPT.format(
        lane=lane,
        first=chunk[0]["id"],
        last=chunk[-1]["id"],
        question=QUESTION[lane],
        candidates="\n".join(render_candidate(c) for c in chunk),
        lane_notes=LANE_NOTES[lane],
        results_file=results_file,
        areas=", ".join(f"`{a}`" for a in (*AREAS, INFRASTRUCTURE)),
    )


def prepare(root, lanes, prefixes, work_dir, limit, size_limit):
    result = scan(root, lanes, prefixes, limit, size_limit)
    work_dir.mkdir(parents=True, exist_ok=True)
    manifest = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "root": str(root),
        "lanes": [],
        "candidates": {},
        "progress_lines": [],
    }
    for lane, entry in result.items():
        candidates = entry["candidates"]
        size = CHUNK.get(lane, DEFAULT_CHUNK)
        prompts = []
        for start in range(0, len(candidates), size):
            chunk = candidates[start : start + size]
            number = start // size + 1
            prompt_file = work_dir / f"{lane}-{number}_prompt.md"
            results_file = work_dir / f"{lane}-{number}_results.json"
            prompt_file.write_text(build_prompt(lane, chunk, results_file), encoding="utf-8")
            prompts.append(
                {
                    "prompt_file": str(prompt_file),
                    "results_file": str(results_file),
                    "candidates": [c["id"] for c in chunk],
                }
            )
        for candidate in candidates:
            manifest["candidates"][candidate["id"]] = candidate
        manifest["lanes"].append(
            {"lane": lane, "candidates": len(candidates), "left_out": entry["left_out"], "reviewer_prompts": prompts}
        )
        manifest["progress_lines"].append(
            f"{lane}: {len(candidates)} candidates for {len(prompts)} reviewers"
            + (f", {entry['left_out']} more past the cap" if entry["left_out"] else "")
        )
    path = work_dir / "manifest.json"
    path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return result, path


def incomplete(lane, ids, summary, evidence):
    return {"lane": lane, "kind": "review-incomplete", "candidates": ids, "summary": summary, "evidence": [evidence]}


def load_reviews(manifest):
    """Every finding the reviewers wrote, plus one per gap in what they were asked to cover."""
    findings, verdicts, problems = [], {}, []
    for entry in manifest["lanes"]:
        lane = entry["lane"]
        for prompt in entry["reviewer_prompts"]:
            path = Path(prompt["results_file"])
            asked = list(prompt["candidates"])
            if not path.exists():
                problems.append(incomplete(lane, asked, "no reviewer result: these candidates were never read", str(path)))
                continue
            try:
                result = json.loads(path.read_text(encoding="utf-8"))
                given = {c["id"]: c for c in result.get("candidates", []) if isinstance(c, dict) and "id" in c}
                written = [f for f in result.get("findings", []) if isinstance(f, dict)]
            except (json.JSONDecodeError, OSError, AttributeError, TypeError, KeyError) as problem:
                problems.append(
                    {"lane": lane, "kind": "review-unreadable", "candidates": asked,
                     "summary": f"reviewer result could not be read: {problem}", "evidence": [str(path)]}
                )
                continue
            missing = [i for i in asked if i not in given]
            if missing:
                problems.append(incomplete(lane, missing, "the reviewer returned no verdict for these candidates", str(path)))
            covered = set()
            for finding in written:
                named = [i for i in finding.get("candidates", []) if i in asked]
                covered.update(named)
                findings.append(dict(finding, lane=lane, candidates=named, kind="finding"))
            for one in asked:
                if one in given:
                    verdicts[one] = given[one]
            orphans = [i for i in asked if given.get(i, {}).get("verdict") == "cleanup" and i not in covered]
            if orphans:
                problems.append(
                    incomplete(lane, orphans, "called cleanup, but no finding says what the change is", str(path))
                )
    return findings, verdicts, problems


def render_review(manifest, findings, verdicts, problems):
    lines = []
    for entry in manifest["lanes"]:
        lane = entry["lane"]
        mine = [f for f in findings if f["lane"] == lane]
        counted = Counter(
            verdicts[i].get("verdict", "unclear")
            for p in entry["reviewer_prompts"]
            for i in p["candidates"]
            if i in verdicts
        )
        lines.append("")
        lines.append(
            f"{lane}: {entry['candidates']} candidates, "
            + ", ".join(f"{counted[v]} {v}" for v in ("cleanup", "keep", "unclear"))
            + f", {len(mine)} findings"
        )
        for finding in mine:
            mark = "bug  " if finding.get("bug") else "clean"
            named = ", ".join(finding["candidates"]) or "found while reading"
            lines.append(f"  {mark}  {finding.get('summary') or '(no summary)'} [{named}]")
            for item in finding.get("evidence", [])[:3]:
                lines.append(f"         at {item}")
            if finding.get("fix"):
                lines.append(f"         fix: {finding['fix']}")
        for one in (p for p in problems if p["lane"] == lane):
            lines.append(f"  error  {one['kind']}: {one['summary']} [{', '.join(one['candidates'])}]")
            lines.append(f"         at {one['evidence'][0]}")
        for p in entry["reviewer_prompts"]:
            for i in p["candidates"]:
                if verdicts.get(i, {}).get("verdict") == "unclear":
                    lines.append(f"  warn   {i} unclear: {verdicts[i].get('reason', '')}")
    lines.append("")
    lines.append(
        f"{sum(e['candidates'] for e in manifest['lanes'])} candidates, {len(findings)} findings, "
        f"{len(problems)} gaps in the review"
    )
    return "\n".join(lines)


def labels_for(finding, manifest):
    """`refactor`, or `bug` where the reviewer found a defect, and an area where one is clear."""
    labels = [BROKEN if finding.get("bug") else KIND]
    area = str(finding.get("area") or "").strip().strip("`")
    if area and not area.startswith("area/") and area != INFRASTRUCTURE:
        area = f"area/{area}"
    if area in AREAS or area == INFRASTRUCTURE:
        labels.append(area)
        return labels
    paths = [manifest["candidates"][i]["path"] for i in finding["candidates"] if i in manifest["candidates"]]
    paths += [item.split(":")[0] for item in drafting.evidence_items(finding.get("evidence"))]
    if paths and all(p.startswith(INFRASTRUCTURE_PATHS) for p in paths):
        labels.append(INFRASTRUCTURE)
    return labels


def title_for(finding):
    return (finding.get("summary") or "Code to clean up").strip().rstrip(".").replace("`", "")


def body_for(finding, manifest):
    items = drafting.evidence_items(finding.get("evidence"))
    candidates = [manifest["candidates"][i] for i in finding["candidates"] if i in manifest["candidates"]]
    path, line = drafting.first_site(items)
    if path is None and candidates:
        path, line = drafting.first_site([f"{c['path']}:{c['line']}" for c in candidates])

    lines = ["## The problem", "", drafting.sentence(finding.get("summary") or ""), ""]
    if finding.get("detail"):
        lines += [drafting.sentence(finding["detail"]), ""]
    if path is not None:
        lines += drafting.code_at(path, line)
    if items:
        lines += ["Where it is:", ""] + [drafting.as_bullet(item) for item in items] + [""]
    governing = sorted({s for c in candidates for s in c.get("specs", [])})
    guarded = sorted({g for c in candidates for g in c.get("guards", [])})
    if governing:
        lines += ["The change has to keep " + ", ".join(f"`{s}`" for s in governing) + " true.", ""]
    if guarded:
        lines += [
            "It touches guarded symbols, whose uses the spec counts, so the count changes in the same commit: "
            + ", ".join(f"`{g}`" for g in guarded) + ".",
            "",
        ]
    lines += ["## The fix", "", drafting.sentence(finding.get("fix") or "") or "Not proposed.", ""]
    if not finding.get("bug"):
        lines += ["Behaviour does not change, so the existing tests pass unchanged.", ""]
    return "\n".join(lines)


def slug_for(finding, taken):
    lead = finding["candidates"][0] if finding["candidates"] else "read"
    base = re.sub(r"[^A-Za-z0-9._-]+", "-", f"{finding['lane']}-{lead}").strip("-")
    slug, extra = base, 1
    while slug in taken:
        extra += 1
        slug = f"{base}-{extra}"
    taken.add(slug)
    return slug


def draft(manifest, findings, out):
    out.mkdir(parents=True, exist_ok=True)
    drafts, taken = [], set()
    for finding in findings:
        slug = slug_for(finding, taken)
        body_file = out / f"{slug}.md"
        body_file.write_text(body_for(finding, manifest), encoding="utf-8")
        title = title_for(finding)
        drafts.append(
            {
                "slug": slug,
                "title": title,
                "title_too_long": len(title) > TITLE_LIMIT,
                "lane": finding["lane"],
                "candidates": finding["candidates"],
                "labels": labels_for(finding, manifest),
                "assignee": ASSIGNEE,
                "body_file": str(body_file),
            }
        )
    return drafts


def render_drafts(drafts, out):
    lines = []
    for number, entry in enumerate(drafts, start=1):
        lines.append(f"  {number:>2}. {entry['title']}")
        lines.append(f"      {' '.join(entry['labels'])}, assignee {entry['assignee']}, body {entry['body_file']}")
        if entry["title_too_long"]:
            lines.append(
                f"      needs a shorter title: {len(entry['title'])} characters, and GitHub takes {TITLE_LIMIT}"
            )
    lines.append("")
    counted = f"{len(drafts)} draft" + ("" if len(drafts) == 1 else "s")
    lines.append(f"{counted} in {out}. Nothing has been posted.")
    return "\n".join(lines)


def read_manifest(work_dir):
    path = Path(work_dir) / "manifest.json"
    if not path.exists():
        print(f"no manifest at {path}", file=sys.stderr)
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    command = argv.pop(0) if argv and argv[0] in ("prepare", "collect", "draft") else "prepare"
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    if command == "prepare":
        parser.add_argument("scope", nargs="*", help="lane names and path prefixes; default every lane, whole tree")
        parser.add_argument("--mechanical-only", action="store_true", help="print the candidates and stop")
        parser.add_argument("--work-dir")
        parser.add_argument("--limit", type=int, default=LIMIT, help="candidates per lane")
        parser.add_argument("--size-limit", type=int, default=SIZE_LIMIT, help="candidates in the size lane")
        parser.add_argument("--root", default=str(ROOT))
    else:
        parser.add_argument("--work-dir", required=True)
        parser.add_argument("--out", help="default: WORK_DIR/issues")
        parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    if command == "prepare":
        root = Path(args.root).resolve()
        lanes = tuple(s for s in args.scope if s in LANES) or LANES
        prefixes = tuple(s for s in args.scope if s not in LANES)
        os.chdir(root)
        if args.mechanical_only:
            print(report(scan(root, lanes, prefixes, args.limit, args.size_limit)))
            return 0
        work_dir = Path(args.work_dir or tempfile.mkdtemp(prefix="maintenance-"))
        result, manifest = prepare(root, lanes, prefixes, work_dir, args.limit, args.size_limit)
        print(report(result), file=sys.stderr)
        print(json.dumps({"work_dir": str(work_dir), "manifest": str(manifest)}))
        return 0

    manifest = read_manifest(args.work_dir)
    if manifest is None:
        return 2
    os.chdir(manifest["root"])
    findings, verdicts, problems = load_reviews(manifest)
    if command == "collect":
        if args.json:
            print(json.dumps({"findings": findings, "verdicts": verdicts, "problems": problems}, indent=2))
        else:
            print(render_review(manifest, findings, verdicts, problems))
        return 1 if problems else 0
    out = Path(args.out) if args.out else Path(args.work_dir) / "issues"
    drafts = draft(manifest, findings, out)
    if args.json:
        print(json.dumps({"drafts": drafts, "out": str(out)}, indent=2))
    else:
        print(render_drafts(drafts, out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
