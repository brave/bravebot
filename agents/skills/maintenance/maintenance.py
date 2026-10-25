#!/usr/bin/env python3
"""Find code to clean up, have reviewers decide which of it is worth an issue, and draft those.

    python3 agents/skills/maintenance/maintenance.py [lane ...] [path ...] [--mechanical-only]
    python3 agents/skills/maintenance/maintenance.py status --work-dir DIR
    python3 agents/skills/maintenance/maintenance.py collect --work-dir DIR
    python3 agents/skills/maintenance/maintenance.py draft --work-dir DIR
    python3 agents/skills/maintenance/maintenance.py verify --work-dir DIR
    python3 agents/skills/maintenance/maintenance.py clean --work-dir DIR

The scan reads every tracked source file and lists candidates in fourteen lanes: public items
nothing names, items only tests name, attributes that silence a lint, blocks of lines repeated in
several places, string literals repeated across files, files or functions past a length, deeply
nested functions, modules that use many others, public functions no test names, shared mutable
state, commented-out code, string-typed errors, and what clippy reports in code that ships. A
candidate is something to look at, not a finding. The skill hands each lane to reviewers who read
the code and decide, and only what a reviewer calls `cleanup` becomes a draft.

`record` writes the verdicts reviewers gave to docs/maintenance-reviewed, and a later scan leaves
out each candidate recorded there while the files it names are unchanged, so a repeat run reaches
the candidates past each lane's cap instead of reading the first ones again.

Where the repository has a commit, `prepare` checks it out in the work directory and the reviewers
read that checkout, so the lines they cite are lines of one commit while the working tree moves.
`clean` removes it. `verify` lists the lines the drafts cite that have moved since that commit.

Nothing here edits source or posts anything. `--mechanical-only` prints the candidates and exits,
which is what `make check-maintenance` runs. Standard library only, like the other checks.
"""

import argparse
import bisect
import datetime
import hashlib
import importlib.util
import json
import os
import re
import shutil
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
workdir = _load("work_dir", HERE.parents[1] / "work_dir.py")

LANES = (
    "unreferenced", "test-only", "allowances", "duplicates", "literals", "size",
    "complexity", "coupling", "untested", "shared-state", "stale", "error-types", "clippy",
)
PREFIX = {
    "unreferenced": "U",
    "test-only": "T",
    "allowances": "A",
    "duplicates": "D",
    "literals": "L",
    "size": "S",
    "complexity": "X",
    "coupling": "C",
    "untested": "N",
    "shared-state": "H",
    "stale": "Z",
    "error-types": "E",
    "clippy": "K",
}
# Candidates per reviewer. A size candidate is a whole file, often thousands of lines, so a
# reviewer gets few of them; the other lanes point at one item each.
CHUNK = {"duplicates": 8, "size": 3}
DEFAULT_CHUNK = 15
LIMIT = 30
SIZE_LIMIT = 12
# The lanes added after the first six keep fewer candidates, so a full run asks one reviewer
# about each. `--limit` sets every lane but size to the same number.
LANE_LIMIT = {
    "complexity": 15, "coupling": 15, "untested": 15, "shared-state": 15, "stale": 15,
    "error-types": 15, "clippy": 15,
}
# Which verdicts `record` writes down, and the file it writes them to.
RECORDED = ("keep", "cleanup")
LEDGER = "docs/maintenance-reviewed"

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
# A function nested this many blocks deep inside its body, or with this many boolean parameters, or
# (outside Rust, where clippy refuses more than seven) this many parameters.
NESTING = 6
BOOL_PARAMETERS = 3
PARAMETERS = 8
# A file that uses this many other modules of its own crate, a file with this many marks of shared
# mutable state, this many public functions no test names, and this many functions that return a
# string or a boxed error.
COUPLING = 12
SHARED_STATE = 8
UNTESTED = 5
STRING_ERRORS = 3
# Consecutive lines of commented-out code that make a block, since one line is as often an example.
COMMENTED_OUT = 2

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
# A lint about a function or type that is too large or too complex. Allowing one is a record that
# the code is that way, so it is as much a candidate as an allowance of dead code.
SMELL_ALLOWANCE = re.compile(
    r"clippy::(?:too_many_arguments|too_many_lines|cognitive_complexity|type_complexity"
    r"|large_enum_variant|struct_excessive_bools|fn_params_excessive_bools|needless_pass_by_value)"
)
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


def scan_references(sources, counts):
    outside, inside = counts
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


def attributed_line(source, number):
    """The first line after `number` that is not blank and not an attribute: what one is on."""
    for line in source.bare[number:]:
        text = line.strip()
        if text and not text.startswith("#["):
            return " ".join(text.split())
    return ""


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
                if "dead_code" in lints or "unused" in lints or SMELL_ALLOWANCE.search(lints):
                    found.append(
                        {
                            "path": source.path,
                            "line": number,
                            "item": attributed_line(source, number),
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


def split_parameters(text, at, language, until=None):
    """The parameters of the function whose parameter list is the first `(` from `at` to `until`,
    split at the commas outside brackets, without `self` and `cls`. A Rust generic list is skipped
    first, since its bounds can hold parentheses."""
    if language == "rust":
        while at < len(text) and text[at].isspace():
            at += 1
        if text.startswith("<", at):
            depth = 0
            for at in range(at, len(text)):
                if text[at] == "<":
                    depth += 1
                elif text[at] == ">" and text[at - 1] != "-":
                    depth -= 1
                    if depth == 0:
                        at += 1
                        break
    opening = text.find("(", at, until) if until else text.find("(", at)
    if opening < 0:
        return []
    angles = language == "rust"
    depth, parts, current = 0, [], []
    for i in range(opening, len(text)):
        c = text[i]
        if c in "([{" or (angles and c == "<"):
            depth += 1
            if depth == 1:
                continue
        elif c in ")]}" or (angles and c == ">" and text[i - 1] != "-"):
            depth -= 1
            if depth == 0:
                break
        if c == "," and depth == 1:
            parts.append("".join(current))
            current = []
        else:
            current.append(c)
    parts.append("".join(current))
    parts = [part.strip() for part in parts if part.strip()]
    return [part for part in parts if not re.fullmatch(r"(?:&\s*(?:'\w+\s+)?)?(?:mut\s+)?(?:self|cls)\b.*", part)]


BOOL_PARAMETER = re.compile(r":\s*(?:bool|boolean)\b|=\s*(?:True|False)\s*$")


def function_parts(source):
    """One record for each function outside test code: its name, first and last line, non-blank
    lines, parameters, and how many blocks deep the deepest statement in its body sits."""
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
            depth = deepest = 0
            for c in text[at:end]:
                if c == "{":
                    depth += 1
                    deepest = max(deepest, depth)
                elif c == "}":
                    depth -= 1
            found.append(
                {
                    "name": name, "first": first, "last": last, "lines": counted,
                    "parameters": split_parameters(
                        text, *((match.end(), "rust") if source.language == "rust" else (match.start(), "js", match.end()))
                    ),
                    "depth": max(deepest - 1, 0),
                }
            )
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
            signature = "\n".join(source.bare[index : body + 1])
            found.append(
                {
                    "name": match.group(2), "first": index + 1, "last": last + 1, "lines": counted,
                    "parameters": split_parameters(signature, match.end(), "python"),
                    "depth": python_depth(source.bare[body + 1 : last + 1], indent),
                }
            )
    return found


def python_depth(lines, indent):
    """How many blocks deep the deepest line of a Python body is, with the body itself at 0. A line
    starts a block when the one before it ends in a colon; any other deeper line continues a
    statement."""
    stack, deepest, opens = [indent], 0, True
    for line in lines:
        if not line.strip():
            continue
        here = len(line) - len(line.lstrip())
        while len(stack) > 1 and here < stack[-1]:
            stack.pop()
        if here > stack[-1] and opens:
            stack.append(here)
        deepest = max(deepest, len(stack) - 2)
        opens = line.rstrip().endswith(":")
    return deepest


def functions(source):
    """(name, first line, last line, non-blank lines) for each function outside test code."""
    return [(f["name"], f["first"], f["last"], f["lines"]) for f in function_parts(source)]


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


def shippable(sources, languages=None):
    """The sources whose code ships: not a test file, not generated, in one of `languages`."""
    for source in sources.values():
        if source.language is None or source.whole_test or is_generated(source):
            continue
        if languages and source.language not in languages:
            continue
        yield source


def scan_complexity(sources):
    """Functions nested many blocks deep, with several boolean parameters, or (outside Rust, where
    clippy refuses more than seven) with many parameters."""
    found = []
    for source in shippable(sources):
        for part in function_parts(source):
            bools = sum(1 for p in part["parameters"] if BOOL_PARAMETER.search(p))
            many = len(part["parameters"]) if source.language != "rust" else 0
            reasons, rank = [], 0
            if part["depth"] >= NESTING:
                reasons.append(f"statements {part['depth']} blocks deep")
                rank += (part["depth"] - NESTING + 1) * 10
            if bools >= BOOL_PARAMETERS:
                reasons.append(f"{bools} boolean parameters")
                rank += (bools - BOOL_PARAMETERS + 1) * 10
            if many >= PARAMETERS:
                reasons.append(f"{many} parameters")
                rank += (many - PARAMETERS + 1) * 5
            if reasons:
                found.append(
                    {
                        "path": source.path,
                        "line": part["first"],
                        "symbol": part["name"],
                        "rank": rank,
                        "detail": f"`{part['name']}` ({part['lines']} lines): " + ", ".join(reasons),
                    }
                )
    return found


REEXPORT = re.compile(r"(?m)^[ \t]*pub(?:\([^)]*\))?[ \t]+use\b[^;]*;")
LOCAL_PATH = re.compile(
    r"(?<![\w:])(?:crate|super|self)::(?:\{((?:[^{}]|\{[^{}]*\})*)\}|([A-Za-z_]\w*))"
)
OTHER_CRATE = re.compile(r"\bbravebot_[a-z0-9_]+\b")


ROOT_ITEMS = "(items in the module above)"


def module_name(name):
    """A lower-case first segment is a module. A type or constant named straight from the crate
    root or the parent module is an item that module offers, and all of them are one dependency."""
    return name if name[0].islower() or name[0] == "_" else ROOT_ITEMS


def leading_names(group):
    """The modules a `use` group lists: `a::b, c::{d, e}, f, Type` is a, c, f and the items."""
    names, depth, current = set(), 0, []
    for c in group + ",":
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        if c == "," and depth == 0:
            found = IDENT.match("".join(current).strip())
            if found and found.group(0) not in ("self", "super", "crate"):
                names.add(module_name(found.group(0)))
            current = []
        else:
            current.append(c)
    return names


def used_modules(source):
    """The modules of its own crate, and the other bravebot crates, that a Rust file names outside
    its tests. A `pub use` is a re-export and is not counted, since a module that lists what it
    offers is not one that depends on all of it."""
    text = "\n".join("" if source.is_test(n) else line for n, line in enumerate(source.bare, start=1))
    text = REEXPORT.sub(lambda found: " " * len(found.group(0)), text)
    modules = set()
    for found in LOCAL_PATH.finditer(text):
        if found.group(1) is not None:
            modules |= leading_names(found.group(1))
        else:
            modules.add(module_name(found.group(2)))
    return modules, set(OTHER_CRATE.findall(text))


def scan_coupling(sources):
    found = []
    for source in shippable(sources, ("rust",)):
        modules, crates = used_modules(source)
        total = len(modules) + len(crates)
        if total < COUPLING:
            continue
        shown = ", ".join(sorted(modules)[:10]) + (", ..." if len(modules) > 10 else "")
        found.append(
            {
                "path": source.path,
                "line": 1,
                "rank": total,
                "detail": f"uses {len(modules)} other modules of its crate ({shown}) and {len(crates)} other crates",
            }
        )
    return found


def scan_untested(sources, counts):
    """Files with several public functions that code outside tests calls and no test names."""
    outside, inside = counts
    defined = Counter()
    every = []
    for source in shippable(sources, ("rust",)):
        for number, name, kind in definitions(source):
            defined[name] += 1
            every.append((source.path, number, name, kind))
    per_file = {}
    for path, number, name, kind in every:
        if kind == "fn" and inside[name] == 0 and outside[name] > defined[name]:
            per_file.setdefault(path, []).append((number, name))
    found = []
    for path, names in per_file.items():
        if len(names) < UNTESTED:
            continue
        shown = ", ".join(f"`{name}`" for _, name in names[:8]) + (", ..." if len(names) > 8 else "")
        found.append(
            {
                "path": path,
                "line": names[0][0],
                "rank": len(names),
                "detail": f"{len(names)} public functions that other code calls and no test names: {shown}",
            }
        )
    return found


SHARED_MARKS = (
    ("Mutex", re.compile(r"\bMutex\b")),
    ("RwLock", re.compile(r"\bRwLock\b")),
    ("RefCell", re.compile(r"\bRefCell\b")),
    ("static cell", re.compile(r"\b(?:OnceLock|LazyLock|thread_local!)")),
    ("atomic", re.compile(r"\bAtomic(?:Bool|[IU]\w+)\b")),
    ("static mut", re.compile(r"\bstatic\s+mut\b")),
    ("thread spawn", re.compile(r"\bthread::(?:spawn|Builder)\b")),
    ("channel", re.compile(r"\bmpsc::(?:sync_)?channel\b")),
)


def scan_shared_state(sources):
    found = []
    for source in shippable(sources, ("rust",)):
        counts = Counter()
        for number, line in enumerate(source.bare, start=1):
            if source.is_test(number) or USE_START.match(line):
                continue
            for name, pattern in SHARED_MARKS:
                counts[name] += len(pattern.findall(line))
        total = sum(counts.values())
        if total >= SHARED_STATE:
            found.append(
                {
                    "path": source.path,
                    "line": 1,
                    "rank": total,
                    "detail": f"{total} marks of shared state or threads: "
                    + ", ".join(f"{n} {name}" for name, n in counts.most_common() if n),
                }
            )
    return found


USE_START = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\b")
COMMENT_MARKER = {"rust": "//", "js": "//", "python": "#"}
COMMENTED_CODE = {
    "rust": re.compile(
        r"(?:let\s.*;|(?:pub(?:\([^)]*\))?\s+)?(?:fn|use|mod|struct|enum|impl|const|static)\s.*[;{]"
        r"|(?:if|for|while|match|loop)\b.*\{|else\s*\{|return\b.*;"
        r"|[\w:.]+(?:::<[^>]*>)?\(.*\)\??;|[\w.\[\]*]+\s*[-+*/|&^]?=\s*[^=\s].*;|[\w:]+!\(.*\);|\}\)?[;,]?)"
    ),
    "js": re.compile(
        r"(?:(?:const|let|var)\s.*;|(?:export\s+)?(?:function|class|import)\b.*[;{]"
        r"|(?:if|for|while|switch)\s*\(.*\{|else\s*\{|return\b.*;|[\w.]+\(.*\);|[\w.\[\]]+\s*[-+*/|&]?=\s*[^=\s].*;|\}\)?[;,]?)"
    ),
    "python": re.compile(
        r"(?:(?:def|class)\s.*:|(?:import|from)\s.*|(?:if|for|while|with|try|except|elif|else)\b.*:"
        r"|return\b.*|raise\b.*|[\w.]+\(.*\)|[\w.\[\]]+\s*[-+*/|&]?=\s*[^=\s].*)"
    ),
}
TODO_MARKER = re.compile(r"\b(?:TODO|FIXME|HACK|XXX)\b")
ISSUE_REFERENCE = re.compile(r"#\d+|issues/\d+|\bGH-\d+")


def comment_lines(source):
    """(line number, text after the marker) for each line that is only a comment. A doc comment
    (`///`, `//!`) or a shebang keeps a `/` or `!` in front of its text, which none of the
    patterns for code match, so an example in a doc comment is never commented-out code."""
    marker = COMMENT_MARKER[source.language]
    for number, (line, code) in enumerate(zip(source.text.split("\n"), source.code), start=1):
        stripped = line.strip()
        if code.strip() or not stripped.startswith(marker):
            continue
        if not source.is_test(number):
            yield number, stripped[len(marker):].strip()


def commented_out_blocks(source):
    """(first line, last line) of each run of consecutive comment lines that are all code."""
    pattern = COMMENTED_CODE[source.language]
    blocks, run, last = [], [], 0
    for number, text in comment_lines(source):
        is_code = bool(pattern.fullmatch(text))
        if is_code and (not run or number == last + 1):
            run.append(number)
        else:
            if len(run) >= COMMENTED_OUT:
                blocks.append((run[0], run[-1]))
            run = [number] if is_code else []
        last = number
    if len(run) >= COMMENTED_OUT:
        blocks.append((run[0], run[-1]))
    return blocks


def unissued_todos(source):
    """The lines holding a `TODO`, `FIXME`, `HACK` or `XXX` in a comment, with no issue named. A
    marker inside a string is not one: its place in `source.code`, where comments are blanked and
    strings are kept, is not blank."""
    found = []
    for number, line in enumerate(source.text.split("\n"), start=1):
        marked = TODO_MARKER.search(line)
        if not marked or source.is_test(number) or ISSUE_REFERENCE.search(line):
            continue
        if not source.code[number - 1][marked.start() : marked.end()].strip():
            found.append(number)
    return found


def scan_stale(sources):
    """Files holding commented-out code, in blocks of consecutive lines, or a TODO naming no issue."""
    found = []
    for source in shippable(sources, tuple(COMMENT_MARKER)):
        blocks, todos = commented_out_blocks(source), unissued_todos(source)
        if not blocks and not todos:
            continue
        parts = []
        if blocks:
            parts.append(
                "commented-out code at "
                + ", ".join(f"{first}-{last}" for first, last in blocks[:8])
                + (f", and {len(blocks) - 8} more" if len(blocks) > 8 else "")
            )
        if todos:
            parts.append("a TODO naming no issue at " + ", ".join(str(n) for n in todos[:8]))
        found.append(
            {
                "path": source.path,
                "line": blocks[0][0] if blocks else todos[0],
                "rank": sum(last - first + 1 for first, last in blocks) + 3 * len(todos),
                "detail": "; ".join(parts),
            }
        )
    return found


STRING_ERROR = re.compile(
    r"String|&\s*(?:'static\s+)?str|Box<\s*dyn\s[^>]*Error[^>]*>"
)
RESULT_ARROW = re.compile(r"->\s*Result\s*<")


def string_error_functions(source):
    """(function, line, error type) for each Rust function outside tests that returns a `Result`
    whose error is a string or a boxed `dyn Error`."""
    text = "\n".join(source.bare)
    starts = line_starts(text)
    found = []
    for arrow in RESULT_ARROW.finditer(text):
        depth, i, comma = 1, arrow.end(), None
        while i < len(text) and depth:
            c = text[i]
            if c == "<":
                depth += 1
            elif c == ">" and text[i - 1] != "-":
                depth -= 1
            elif c == "," and depth == 1:
                comma = i
            elif c in "{;":
                break
            i += 1
        if depth or comma is None:
            continue
        error = " ".join(text[comma + 1 : i - 1].split())
        line = line_of(starts, arrow.start())
        named = re.match(r"fn\s+([A-Za-z_]\w*)", text[text.rfind("fn ", 0, arrow.start()) :])
        if STRING_ERROR.fullmatch(error) and named and not source.is_test(line):
            found.append((named.group(1), line, error))
    return found


def scan_error_types(sources):
    found = []
    for source in shippable(sources, ("rust",)):
        returning = string_error_functions(source)
        if len(returning) < STRING_ERRORS:
            continue
        shown = ", ".join(f"`{name}` ({error})" for name, _, error in returning[:6])
        found.append(
            {
                "path": source.path,
                "line": returning[0][1],
                "rank": len(returning),
                "detail": f"{len(returning)} functions return a string or boxed error: {shown}"
                + (", ..." if len(returning) > 6 else ""),
            }
        )
    return found


# What clippy is asked about in code that ships, and how much a hit counts: 3 for what the
# checklist this lane came from rates High, 2 for Medium, 1 for Low. The first three are the
# strictest of them and are free to enforce while the tree has none.
CLIPPY_LINTS = {
    "let_underscore_must_use": 3,
    "panic": 3,
    "expect_used": 2,
    "unwrap_used": 3,
    "undocumented_unsafe_blocks": 3,
    "multiple_unsafe_ops_per_block": 2,
    "cast_possible_truncation": 3,
    "cast_sign_loss": 3,
    "string_slice": 2,
    "struct_excessive_bools": 3,
    "fn_params_excessive_bools": 2,
    "cognitive_complexity": 3,
    "too_many_lines": 2,
    "significant_drop_tightening": 3,
    "needless_pass_by_value": 2,
    "redundant_clone": 2,
    "implicit_clone": 2,
    "allow_attributes_without_reason": 2,
    "wildcard_imports": 1,
    "manual_let_else": 1,
    "items_after_statements": 1,
    "unused_self": 1,
    "needless_pass_by_ref_mut": 1,
}
CLIPPY_TARGET_DIR = "target/maintenance-clippy"
CLIPPY_SECONDS = 1800


def clippy_command():
    return [
        "cargo", "clippy", "--workspace", "--lib", "--bins", "--all-features", "--quiet",
        "--message-format=json", "--",
        *(f"-Wclippy::{lint}" for lint in CLIPPY_LINTS),
    ]


def run_clippy(root):
    """clippy's JSON diagnostics for the workspace's libraries and binaries, and a note saying why
    there are none when it could not be run. It has a target directory of its own, because these
    lints differ from the ones `make check` passes and sharing one would recompile every crate
    each time the two alternate."""
    if shutil.which("cargo") is None:
        return "", "cargo is not installed, so clippy was not run"
    env = dict(os.environ, CARGO_TARGET_DIR=str(root / CLIPPY_TARGET_DIR))
    try:
        # The program and arguments are fixed above; no shell reads them.
        done = subprocess.run(  # nosemgrep: gitlab.bandit.B603
            clippy_command(), cwd=root, env=env, capture_output=True, text=True,
            timeout=CLIPPY_SECONDS, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as problem:
        return "", f"clippy could not be run: {problem}"
    if '"reason":"compiler-message"' not in done.stdout and done.returncode != 0:
        return "", f"clippy exited {done.returncode} with no diagnostics: {done.stderr.strip()[-300:]}"
    return done.stdout, ""


def scan_clippy(sources, output):
    """One candidate for each lint and file, from the JSON lines clippy printed. A location in a
    file the tree does not track (build output, a dependency) or in test code is not one."""
    seen, grouped = set(), {}
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message.get("message") or {}
        lint = ((diagnostic.get("code") or {}).get("code") or "").removeprefix("clippy::")
        spans = [s for s in diagnostic.get("spans", []) if s.get("is_primary")]
        if lint not in CLIPPY_LINTS or not spans:
            continue
        span = spans[0]
        path = span["file_name"].removeprefix("./")
        source = sources.get(path)
        key = (lint, path, span["line_start"], span["column_start"])
        if source is None or source.is_test(span["line_start"]) or key in seen:
            continue
        seen.add(key)
        entry = grouped.setdefault((path, lint), {"lines": [], "message": diagnostic.get("message", "")})
        entry["lines"].append(span["line_start"])
    found = []
    for (path, lint), entry in grouped.items():
        lines = sorted(entry["lines"])
        found.append(
            {
                "path": path,
                "line": lines[0],
                "lint": lint,
                "rank": CLIPPY_LINTS[lint] * len(lines),
                "detail": f"`clippy::{lint}` {len(lines)} times, at "
                + ", ".join(str(n) for n in lines[:10])
                + (f", and {len(lines) - 10} more" if len(lines) > 10 else "")
                + f": {entry['message']}",
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


def digest_of(sources, paths, cache):
    """A short hash of the text of the files at `paths`: what a recorded verdict was given for."""
    parts = []
    for path in sorted(set(paths)):
        if path not in cache:
            source = sources.get(path)
            text = source.text if source else ""
            cache[path] = hashlib.sha256(text.encode("utf-8")).hexdigest()
        parts.append(f"{path}:{cache[path]}")
    return hashlib.sha256("|".join(parts).encode("utf-8")).hexdigest()[:16]


def short_hash(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:12]


def candidate_paths(candidate):
    return [c["path"] for c in candidate.get("copies", [])] or [candidate["path"]]


def ledger_field(text, also=""):
    """Text for one whitespace-separated ledger field, without loss: `%`, whitespace and the
    characters in `also` become `%XX` per UTF-8 byte, so two names that differ only there keep two keys."""
    return re.sub(
        r"[%\s" + re.escape(also) + r"]",
        lambda found: "".join("%%%02X" % b for b in found.group(0).encode("utf-8", "surrogateescape")),
        text,
    )


def ledger_value(field):
    return re.sub(
        r"(?:%[0-9A-F]{2})+",
        lambda found: bytes.fromhex(found.group(0).replace("%", "")).decode("utf-8", "surrogateescape"),
        field,
    )


def candidate_key(candidate, sources):
    """What names a candidate from one run to the next: the file and item, never a line number."""
    lane, path = candidate["lane"], candidate["path"]
    if lane in ("unreferenced", "test-only"):
        name = candidate["symbol"]
    elif lane == "complexity":
        name = candidate["symbol"]
    elif lane == "clippy":
        name = candidate["lint"]
    elif lane == "allowances":
        name = short_hash(candidate["detail"] + candidate["item"])
    elif lane == "literals":
        path, name = "text", short_hash(candidate["literal"])
    elif lane == "duplicates":
        first = candidate["copies"][0]
        lines = sources[first["path"]].code[first["line"] - 1 : first["end"]]
        path = "+".join(sorted(set(candidate_paths(candidate))))
        name = short_hash(" ".join(" ".join(lines).split()))
    else:
        name = ""
    return ledger_field(f"{lane}:{path}" + (f"::{name}" if name else ""))


def read_ledger(root):
    """The verdicts recorded in docs/maintenance-reviewed, by key. A line that does not parse is
    reported and skipped, which makes its candidate a new one."""
    path = Path(root) / LEDGER
    entries = {}
    if not path.exists():
        return entries
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split(None, 5)
        if len(fields) < 5 or fields[1] not in RECORDED:
            print(f"{LEDGER}:{number}: not a verdict line, so it is ignored", file=sys.stderr)
            continue
        key, verdict, date, digest, paths = fields[:5]
        entries[key] = {
            "verdict": verdict, "date": date, "digest": digest,
            "paths": [ledger_value(p) for p in paths.split(",")], "reason": fields[5] if len(fields) > 5 else "",
        }
    return entries


LEDGER_HEADER = """\
# Candidates the maintenance skill has read, one per line, sorted by key:
#
#   <key> <verdict> <date> <digest> <paths> <reason>
#
# keep      a reviewer read it and the code should stay as it is
# cleanup   a reviewer read it and a draft was written, which a person posted or declined
#
# <digest> hashes the text of every file in <paths>. A scan leaves a candidate out while its file
# is unchanged and offers it again after any edit to one of them, or when `--fresh` is given. A
# line is the verdict for that code and no other, so deleting it asks for the review again.
# In <key> and <paths>, `%`, whitespace and (in <paths>) `,` are written as `%XX`.
# The maintenance skill writes this file: agents/skills/maintenance/SKILL.md.
"""


def write_ledger(root, entries):
    lines = []
    for key in sorted(entries):
        entry = entries[key]
        reason = " ".join(str(entry.get("reason") or "").split())
        lines.append(
            f"{key} {entry['verdict']} {entry['date']} {entry['digest']} {','.join(ledger_field(p, ',') for p in entry['paths'])} {reason}".rstrip()
        )
    path = Path(root) / LEDGER
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(LEDGER_HEADER + "\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")
    return path


def scan(root, lanes=LANES, prefixes=(), limit=None, size_limit=SIZE_LIMIT, fresh=False, clippy=run_clippy):
    """Candidates per lane, ranked, capped, numbered, and annotated with the specs they touch.

    References are counted over the whole tree whatever `prefixes` says, because an item under
    one directory is used from another, and only the report is scoped. A candidate the ledger
    holds a verdict for, with its files unchanged since, is counted as carried and left out before
    the cap, so the cap reaches the next candidates instead of the same ones. `limit` is the cap
    for every lane but size; left unset, each lane has its own."""
    sources = read_tree(root)
    found, notes = {}, {}
    counts = None
    if {"unreferenced", "test-only", "untested"} & set(lanes):
        counts = references(sources)
    if "unreferenced" in lanes or "test-only" in lanes:
        found.update(scan_references(sources, counts))
    if "allowances" in lanes:
        found["allowances"] = scan_allowances(sources)
    if "duplicates" in lanes or "literals" in lanes:
        found["duplicates"] = scan_duplicates(sources)
    if "literals" in lanes:
        found["literals"] = scan_literals(sources, found["duplicates"])
    if "size" in lanes:
        found["size"] = scan_size(sources)
    if "complexity" in lanes:
        found["complexity"] = scan_complexity(sources)
    if "coupling" in lanes:
        found["coupling"] = scan_coupling(sources)
    if "untested" in lanes:
        found["untested"] = scan_untested(sources, counts)
    if "shared-state" in lanes:
        found["shared-state"] = scan_shared_state(sources)
    if "stale" in lanes:
        found["stale"] = scan_stale(sources)
    if "error-types" in lanes:
        found["error-types"] = scan_error_types(sources)
    if "clippy" in lanes:
        output, notes["clippy"] = clippy(root)
        found["clippy"] = scan_clippy(sources, output)

    table = governed_by(root, specs.load_specs(root / "docs" / "specs"))
    ledger = {} if fresh else read_ledger(root)
    hashes = {}
    result = {}
    for lane in LANES:
        if lane not in lanes:
            continue
        every = [c for c in found.get(lane, []) if in_scope(c, prefixes)]
        carried = 0
        remaining = []
        for candidate in every:
            candidate["lane"] = lane
            candidate["key"] = candidate_key(candidate, sources)
            candidate["digest"] = digest_of(sources, candidate_paths(candidate), hashes)
            entry = ledger.get(candidate["key"])
            if entry and entry["digest"] == candidate["digest"]:
                carried += 1
            else:
                remaining.append(candidate)
        remaining.sort(key=lambda c: (-c.get("rank", 0), c["path"], c["line"]))
        cap = size_limit if lane == "size" else (limit or LANE_LIMIT.get(lane, LIMIT))
        kept = remaining[:cap]
        for number, candidate in enumerate(kept, start=1):
            candidate["id"] = f"{PREFIX[lane]}{number}"
            annotate(candidate, table, sources)
        result[lane] = {"candidates": kept, "left_out": len(remaining) - len(kept), "carried": carried}
        if notes.get(lane):
            result[lane]["note"] = notes[lane]
    return result


def report(result):
    lines = []
    total = 0
    for lane, entry in result.items():
        candidates = entry["candidates"]
        total += len(candidates)
        lines.append("")
        lines.append(
            f"{lane}: {len(candidates)}"
            + (f", {entry['left_out']} more past the cap" if entry["left_out"] else "")
            + (f", {entry['carried']} already reviewed ({LEDGER})" if entry.get("carried") else "")
        )
        if entry.get("note"):
            lines.append(f"  note: {entry['note']}")
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

{reading}
## The job

For every candidate:

1. Read the code. Search the name or the text yourself, across all of it, including strings,
   macros, build scripts, the Makefile, `.github/`, other languages, and docs. The scan counts
   identifiers, so it misses a use built from parts or reached by reflection, and it calls two
   unrelated items with one name a use of each other.
2. Look for the reason it is the way it is before calling it a mess. A comment, a spec clause, a
   platform `cfg`, or a public API another repository uses is a reason. A candidate is a reason
   to look and not a defect: a long function can read well and a short one can hide a bad
   abstraction. Call it `cleanup` when a person changing this code is slowed or misled by it and
   you can name the change that fixes that.
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

{finish}"""

READING_PINNED = """\
## Where to read

The code is at `{tree}`, a checkout of commit `{commit}` made for this run. Read it there and cite
`file:line` from it, so each line number refers to that one commit however far `main` moves while
you work. It holds tracked files only, so `git grep -n NAME` searches all of it in a second and a
plain `grep -r` is as fast. Do not search `{root}`: it holds `target/` and `node_modules/`, and a
`grep -r` over them runs past the tool's time limit, which moves it to the background. History is in
`{root}`: `git -C {root} log -- PATH`.
"""

READING_LIVE = """\
## Where to read

The code is in `{root}`, your working directory. Search it with `git grep -n NAME`, which reads
tracked files only. A plain `grep -r .` also walks `target/` and `node_modules/`, runs past the
tool's time limit, and is moved to the background.
"""

FINISH = """\
## How to finish

- Start no background command. If a command ran past its time limit and the shell moved it to the
  background, stop it before you answer.
- Do not run `gh`, file or comment on an issue, or start another agent. The person running this
  skill reads your file and decides what is posted.
- When the results file is written and parses, answer with one line saying so and stop. Nothing
  reads your answer.
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
        "Each of these silences a lint about dead or unused code, or about a function or type "
        "that is too large or too complex. Remove it in your head and decide whether what it "
        "hides is real."
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
    "complexity": (
        f"Each of these is a function with statements {NESTING} or more blocks deep, "
        f"{BOOL_PARAMETERS} or more boolean parameters, or (outside Rust) {PARAMETERS} or more "
        "parameters. Clippy refuses more than seven parameters in Rust, so a Rust function with "
        "that many carries an allowance, which the `allowances` lane lists."
    ),
    "coupling": (
        f"Each of these is a Rust file that names {COUPLING} or more other modules of its own "
        "crate, or other crates of this workspace, outside its tests. A change to any of them can "
        "mean a change here. Types named straight from the crate root count as one."
    ),
    "untested": (
        f"Each of these is a Rust file with {UNTESTED} or more public functions that code outside "
        "tests calls and no test names, in a test file, a `tests` directory or a `#[cfg(test)]` "
        "module. The scan matches names, so a function tested through the behaviour it serves "
        "looks untested."
    ),
    "shared-state": (
        f"Each of these is a Rust file with {SHARED_STATE} or more marks of shared mutable state "
        "or threads outside its tests: `Mutex`, `RwLock`, `RefCell`, statics that initialise "
        "once, atomics, `static mut`, spawned threads and channels."
    ),
    "stale": (
        f"Each of these is a file with {COMMENTED_OUT} or more consecutive lines of commented-out "
        "code, or a `TODO`, `FIXME`, `HACK` or `XXX` that names no issue."
    ),
    "error-types": (
        f"Each of these is a Rust file with {STRING_ERRORS} or more functions that return a "
        "`Result` whose error is a `String`, a `&str` or a boxed `dyn Error`. A caller cannot "
        "match on one of these, only print it."
    ),
    "clippy": (
        "Each of these is one clippy lint in one file, from clippy run over the libraries and "
        "binaries and not the tests, counted over every site in the file. clippy sees types, so "
        "a site is what the lint says it is, but whether it matters is for you to read. Its "
        "`nursery` lints are wrong at times."
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
    "complexity": (
        "A cleanup names the change: a guard clause or `let ... else` that flattens a branch, a "
        "helper for the inner block, an enum or a small options struct in place of the booleans. "
        "Keep a parser or state machine whose nesting follows the structure of its input, a "
        "table, and flags a spec clause names. Never flatten a branch that decides something "
        "about labelled content by moving it into a helper elsewhere."
    ),
    "coupling": (
        "A cleanup says which jobs the file does, which module each moves to, and that each part "
        "then uses a handful of the modules. A new module goes in the crate the old one is in, "
        "or in one docs/specs/layering.md allows. Keep a file whose job is to join the others: "
        "the place a program starts, or the one that wires the application together."
    ),
    "untested": (
        "Read the tests and the spec clauses that name them before deciding a function is "
        "untested; most are exercised through what they serve. A cleanup, in this lane, is a "
        "test to write: name the function, the behaviour it decides, and the cases, refusals "
        "first, since a test of the allowed path alone is the one most likely to exist already. "
        "Keep accessors, constructors and one-line forwarders whose behaviour is their caller's."
    ),
    "shared-state": (
        "A cleanup says what one owner could hold instead and what passes messages to it, or "
        "which lock covers more than the data it guards. Keep state that is shared because "
        "several threads need it, a process-wide registry, and a lock around one value read and "
        "written briefly. A guard held across blocking I/O, a second lock taken while one is "
        "held, or a thread whose handle is dropped and whose failure nobody sees is a `bug` if "
        "you can say how it hangs or loses the failure."
    ),
    "stale": (
        "Commented-out code is deleted, since git keeps it. A `TODO` either gets the issue that "
        "tracks it, which you find by searching the tracker with `gh issue list --search`, or "
        "is deleted because it is done or will not be. Say which, for each."
    ),
    "error-types": (
        "A cleanup names the error type: an enum a caller can match, in the crate that returns "
        "it, with the variants the functions use. Keep a message that is the product, such as a "
        "refusal text a tool returns to the planner or a person reads, where a spec fixes the "
        "wording, and a function whose caller only prints the error. An error type that carries "
        "content from a file or a page carries untrusted bytes across a boundary: say what the "
        "type holds, and do not propose one that holds labelled content."
    ),
    "clippy": (
        "Read every site of a candidate. `cleanup` is where the lint is right at most sites and "
        "one change removes them: a helper, a `?`, a checked conversion, a named type. Keep a "
        "`let _ =` on a send to a channel whose receiver may be gone, a cast bounded by "
        "construction (say the bound), and a clone a later use needs. A lint that is right, in "
        "code that can panic or wrap on a value a person, a file or a server supplies "
        "(`string_slice` on text, a cast of a length, `expect` on parsed input), is a `bug`: say "
        "which input and what happens."
    ),
}


def render_candidate(candidate):
    lines = [f"- **{candidate['id']}** `{candidate['path']}:{candidate['line']}` {candidate['detail']}"]
    for name in ("specs", "guards"):
        if candidate.get(name):
            lines.append(f"  - {name}: " + ", ".join(f"`{v}`" for v in candidate[name]))
    return "\n".join(lines)


def build_prompt(lane, chunk, results_file, reading):
    return PROMPT.format(
        lane=lane,
        first=chunk[0]["id"],
        last=chunk[-1]["id"],
        question=QUESTION[lane],
        candidates="\n".join(render_candidate(c) for c in chunk),
        lane_notes=LANE_NOTES[lane],
        results_file=results_file,
        areas=", ".join(f"`{a}`" for a in (*AREAS, INFRASTRUCTURE)),
        reading=reading,
        finish=FINISH,
    )


def git(root, *args):
    # The program and arguments are fixed here or come from git itself; no shell reads them.
    return subprocess.run(  # nosemgrep: gitlab.bandit.B603
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=False
    )


def read_commit(root):
    """The commit `root` is on, or None where it has none."""
    done = git(root, "rev-parse", "--verify", "-q", "HEAD")
    return done.stdout.strip() if done.returncode == 0 else None


def has_uncommitted_changes(root):
    """Whether a tracked file differs from the commit it is on."""
    return bool(git(root, "status", "--porcelain", "--untracked-files=no").stdout.strip())


def make_snapshot(root, commit, tree):
    """Check `commit` out at `tree`, detached, and say whether that worked. Reviewers read it in
    place of the working tree, which other sessions move while a run is under way, so every line
    they cite is a line of one commit. Hooks are off because this repository's post-checkout hook
    copies `.envrc` and runs `direnv allow`, which a checkout made for reading has no use for."""
    if tree.exists():
        remove_snapshot(root, tree)
    done = git(root, "-c", "core.hooksPath=/dev/null", "worktree", "add", "--detach", str(tree), commit)
    return done.returncode == 0


def remove_snapshot(root, tree):
    """Remove the checkout `make_snapshot` made, and forget it in the repository."""
    git(root, "worktree", "remove", "--force", str(tree))
    git(root, "worktree", "prune")
    return not Path(tree).exists()


def reading_for(root, tree, commit):
    if tree is None:
        return READING_LIVE.format(root=root)
    return READING_PINNED.format(root=root, tree=tree, commit=commit)


def prepare(root, lanes, prefixes, work_dir, **options):
    """Scan, then write a prompt file for each group of candidates. `options` are the keywords
    `scan` takes after the prefixes: `limit`, `size_limit`, `fresh` and `clippy`.

    Where the repository has a commit, the reviewers read a checkout of it in `work_dir/tree`, and
    the manifest names the commit and the tree. Without one they read the working tree."""
    commit = read_commit(root)
    dirty = commit is not None and has_uncommitted_changes(root)
    result = scan(root, lanes, prefixes, **options)
    work_dir.mkdir(parents=True, exist_ok=True)
    tree = work_dir / "tree" if commit is not None and make_snapshot(root, commit, work_dir / "tree") else None
    reading = reading_for(root, tree, commit)
    manifest = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "root": str(root),
        "commit": commit,
        "tree": str(tree) if tree else None,
        "dirty": dirty,
        "lanes": [],
        "candidates": {},
        "progress_lines": [],
    }
    if tree:
        manifest["progress_lines"].append(f"reviewers read commit {commit[:12]} at {tree}")
        if dirty:
            manifest["progress_lines"].append(
                "tracked files have uncommitted changes: the candidates come from them, the reviewers read the commit"
            )
    else:
        manifest["progress_lines"].append("no commit to check out: reviewers read the working tree")
    for lane, entry in result.items():
        candidates = entry["candidates"]
        size = CHUNK.get(lane, DEFAULT_CHUNK)
        prompts = []
        for start in range(0, len(candidates), size):
            chunk = candidates[start : start + size]
            number = start // size + 1
            prompt_file = work_dir / f"{lane}-{number}_prompt.md"
            results_file = work_dir / f"{lane}-{number}_results.json"
            prompt_file.write_text(build_prompt(lane, chunk, results_file, reading), encoding="utf-8")
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
            + (f", {entry['carried']} already reviewed" if entry.get("carried") else "")
            + (f" ({entry['note']})" if entry.get("note") else "")
        )
    path = workdir.write_manifest(work_dir, manifest)
    return result, path


def incomplete(lane, ids, summary, evidence, waiting=False):
    """A gap in a review. `waiting` marks the gaps that mean the reviewer has not finished: no result,
    or one that does not give a verdict for every candidate it was asked about."""
    return {
        "lane": lane, "kind": "review-incomplete", "candidates": ids, "summary": summary,
        "evidence": [evidence], "waiting": waiting,
    }


def load_reviews(manifest):
    """Every finding the reviewers wrote, plus one per gap in what they were asked to cover."""
    findings, verdicts, problems = [], {}, []
    for entry in manifest["lanes"]:
        lane = entry["lane"]
        for prompt in entry["reviewer_prompts"]:
            path = Path(prompt["results_file"])
            asked = list(prompt["candidates"])
            if not path.exists():
                problems.append(
                    incomplete(lane, asked, "no reviewer result: these candidates were never read", str(path), True)
                )
                continue
            try:
                result = json.loads(path.read_text(encoding="utf-8"))
                given = {c["id"]: c for c in result.get("candidates", []) if isinstance(c, dict) and "id" in c}
                written = [f for f in result.get("findings", []) if isinstance(f, dict)]
            except (json.JSONDecodeError, OSError, AttributeError, TypeError, KeyError) as problem:
                problems.append(
                    {"lane": lane, "kind": "review-unreadable", "candidates": asked, "waiting": True,
                     "summary": f"reviewer result could not be read: {problem}", "evidence": [str(path)]}
                )
                continue
            missing = [i for i in asked if i not in given]
            if missing:
                problems.append(
                    incomplete(lane, missing, "the reviewer returned no verdict for these candidates", str(path), True)
                )
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
        commit = manifest.get("commit")
        heading = f"Where it is, in commit `{commit[:12]}`:" if commit else "Where it is:"
        lines += [heading, ""] + [drafting.as_bullet(item) for item in items] + [""]
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


CITE = re.compile(r"((?:[\w.@-]+/)+[\w.@-]+\.\w+):(\d+)(?:-(\d+))?")
HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+\d+(?:,(\d+))? @@", re.MULTILINE)


def hunk_count(text):
    return int(text) if text else 1


def changes_to(root, commit, path):
    """How `path` changed from `commit` to HEAD, as `(first old line, old lines, new lines)` for each
    hunk, or None where the file is gone. A file that did not change has no hunks."""
    if git(root, "cat-file", "-e", f"HEAD:{path}").returncode != 0:
        return None
    diff = git(root, "diff", "--no-renames", "-U0", commit, "HEAD", "--", path).stdout
    return [(int(old), hunk_count(count), hunk_count(new)) for old, count, new in HUNK.findall(diff)]


def line_after(hunks, line):
    """Where an old line is after `hunks`, or None where the line is one of those they changed."""
    offset = 0
    for old, count, new in hunks:
        first = old if count else old + 1
        if count and first <= line < first + count:
            return None
        if line < first:
            break
        offset += new - count
    return line + offset


def verify(manifest, out):
    """The places the drafts in `out` cite whose lines differ between the commit the reviewers read
    and HEAD: moved to a line that is known, in code that changed, or in a file that is gone. None
    where the manifest names no commit."""
    root, commit = manifest["root"], manifest.get("commit")
    if commit is None:
        return None
    known, drafts = {}, []
    for body in sorted(Path(out).glob("*.md")):
        cites, placed = [], set()
        for path, first, last in CITE.findall(body.read_text(encoding="utf-8")):
            place = f"{path}:{first}" + (f"-{last}" if last else "")
            if place in placed:
                continue
            placed.add(place)
            if path not in known:
                exists = git(root, "cat-file", "-e", f"{commit}:{path}").returncode == 0
                known[path] = changes_to(root, commit, path) if exists else False
            hunks = known[path]
            if hunks is False or hunks == []:
                continue
            if hunks is None:
                cites.append({"place": place, "state": "gone"})
                continue
            now = [line_after(hunks, int(n)) for n in (first, last or first)]
            if None in now:
                cites.append({"place": place, "state": "changed"})
            elif now != [int(first), int(last or first)]:
                cites.append({"place": place, "state": "moved", "now": f"{now[0]}" + (f"-{now[1]}" if last else "")})
        if cites:
            drafts.append({"draft": body.name, "cites": cites})
    return drafts


def render_verify(manifest, moved):
    commit = manifest.get("commit")
    if commit is None:
        return "the manifest names no commit, so there is nothing to compare the cited lines with"
    head = read_commit(manifest["root"])
    lines = [f"lines cited from commit {commit[:12]}, compared with {head[:12] if head else 'HEAD'}"]
    if not moved:
        lines.append("every cited line stands")
    for entry in moved:
        lines.append(entry["draft"])
        for cite in entry["cites"]:
            what = {"moved": f"moved to {cite.get('now')}", "changed": "is in changed code", "gone": "is gone"}
            lines.append(f"    {cite['place']} {what[cite['state']]}")
    lines.append("Line numbers written without a path are not checked.")
    return "\n".join(lines)


def record(manifest, verdicts, today=None):
    """Write the verdicts reviewers gave to the ledger, and drop the entries whose files have
    changed or gone, since those are reviewed again anyway. Returns the path and how many verdicts
    this run added."""
    root = Path(manifest["root"])
    today = today or datetime.date.today().isoformat()
    sources = read_tree(root)
    hashes = {}
    entries = {}
    for key, entry in read_ledger(root).items():
        if entry["digest"] == digest_of(sources, entry["paths"], hashes) and all(p in sources for p in entry["paths"]):
            entries[key] = entry
    added = 0
    for candidate_id, candidate in manifest["candidates"].items():
        verdict = verdicts.get(candidate_id, {})
        if verdict.get("verdict") not in RECORDED:
            continue
        entries[candidate["key"]] = {
            "verdict": verdict["verdict"],
            "date": today,
            "digest": candidate["digest"],
            "paths": sorted(set(candidate_paths(candidate))),
            "reason": verdict.get("reason", ""),
        }
        added += 1
    return write_ledger(root, entries), added


def reviewer_status(manifest, problems):
    """One `(reviewer, why it is not done or None)` row per reviewer, from the gaps `load_reviews`
    found. A reviewer is done when its result parses and gives a verdict for every candidate it was
    asked about. Whether the findings are complete is for `collect` to say."""
    waiting = {p["evidence"][0]: p["summary"] for p in problems if p.get("waiting")}
    return [
        (Path(prompt["results_file"]).name.removesuffix("_results.json"), waiting.get(prompt["results_file"]))
        for entry in manifest["lanes"]
        for prompt in entry["reviewer_prompts"]
    ]


def render_status(rows):
    done = sum(1 for _, why in rows if why is None)
    if done == len(rows):
        return f"every reviewer has written a readable result ({done} of {len(rows)})"
    lines = [f"{done} of {len(rows)} reviewers have written a readable result"]
    lines += [f"  waiting: {name} ({why})" for name, why in rows if why is not None]
    return "\n".join(lines)


def clean(manifest, work_dir):
    """Remove the checkout the reviewers read. Returns whether none is left. Only `work_dir/tree`
    is removed, whatever the manifest names."""
    tree = manifest.get("tree")
    if not tree:
        return True
    if Path(tree).resolve() != (Path(work_dir) / "tree").resolve():
        return False
    return remove_snapshot(manifest["root"], Path(tree))


COMMANDS = ("prepare", "collect", "draft", "verify", "record", "clean", "status")


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    command = argv.pop(0) if argv and argv[0] in COMMANDS else "prepare"
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    if command == "prepare":
        parser.add_argument("scope", nargs="*", help="lane names and path prefixes; default every lane, whole tree")
        parser.add_argument("--mechanical-only", action="store_true", help="print the candidates and stop")
        parser.add_argument("--work-dir")
        parser.add_argument("--limit", type=int, help=f"candidates per lane, default {LIMIT} or the lane's own")
        parser.add_argument("--size-limit", type=int, default=SIZE_LIMIT, help="candidates in the size lane")
        parser.add_argument("--fresh", action="store_true", help=f"review candidates {LEDGER} holds a verdict for")
        parser.add_argument("--no-clippy", action="store_true", help="leave out the clippy lane, which compiles")
        parser.add_argument("--root", default=str(ROOT))
    else:
        parser.add_argument("--work-dir", required=True)
        parser.add_argument("--out", help="default: WORK_DIR/issues")
        parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    if command == "prepare":
        root = Path(args.root).resolve()
        named = tuple(s for s in args.scope if s in LANES)
        lanes = named or tuple(lane for lane in LANES if not (args.no_clippy and lane == "clippy"))
        prefixes = tuple(s for s in args.scope if s not in LANES)
        os.chdir(root)
        if args.mechanical_only:
            print(report(scan(root, lanes, prefixes, args.limit, args.size_limit, args.fresh)))
            return 0
        work_dir = Path(args.work_dir or tempfile.mkdtemp(prefix="maintenance-"))
        result, manifest = prepare(
            root, lanes, prefixes, work_dir, limit=args.limit, size_limit=args.size_limit, fresh=args.fresh
        )
        print(report(result), file=sys.stderr)
        print(json.dumps({"work_dir": str(work_dir), "manifest": str(manifest)}))
        return 0

    manifest = workdir.read_manifest(args.work_dir)
    if manifest is None:
        return 2
    out = Path(args.out).resolve() if args.out else Path(args.work_dir).resolve() / "issues"
    os.chdir(manifest["root"])
    if command == "verify":
        moved = verify(manifest, out)
        print(json.dumps(moved, indent=2) if args.json else render_verify(manifest, moved))
        return 1 if any(c["state"] != "moved" for d in moved or [] for c in d["cites"]) else 0
    if command == "clean":
        removed = clean(manifest, args.work_dir)
        print("the reviewers' checkout is removed" if removed else f"did not remove {manifest['tree']}")
        return 0 if removed else 1
    findings, verdicts, problems = load_reviews(manifest)
    if command == "status":
        rows = reviewer_status(manifest, problems)
        print(render_status(rows))
        return 1 if any(why is not None for _, why in rows) else 0
    if command == "collect":
        if args.json:
            print(json.dumps({"findings": findings, "verdicts": verdicts, "problems": problems}, indent=2))
        else:
            print(render_review(manifest, findings, verdicts, problems))
        return 1 if problems else 0
    if command == "record":
        path, added = record(manifest, verdicts)
        print(f"{added} verdicts written to {path}. Nothing has been committed.")
        return 0
    # The reviewers cited lines of their checkout, so the code a body quotes is read from it too.
    if manifest.get("tree") and Path(manifest["tree"]).is_dir():
        os.chdir(manifest["tree"])
    drafts = draft(manifest, findings, out)
    if args.json:
        print(json.dumps({"drafts": drafts, "out": str(out)}, indent=2))
    else:
        print(render_drafts(drafts, out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
