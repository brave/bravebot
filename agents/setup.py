#!/usr/bin/env python3
"""Link the checked-in agent configuration into each tool's discovery directory.

The source of truth is `agents/`: versioned, reviewed, and named for no particular
vendor. No tool reads it. Claude Code discovers skills, slash commands and subagents
under `.claude/`; Codex discovers skills under `.agents/skills` and subagents under
`.codex/agents`; Cursor discovers skills and subagents under `.cursor/`; and bravebot
discovers skills under `.bravebot/skills`, `.claude/skills` and `.agents/skills`, and
subagents under `.bravebot/agents`. Codex, Cursor and bravebot read their instructions
from `AGENTS.md` at the workspace root. This script bridges them by creating one symlink
per entry, so a skill is written once and every tool sees it:

    .claude/skills/<name>      ->  agents/skills/<name>
    .agents/skills/<name>      ->  agents/skills/<name>
    .bravebot/skills/<name>    ->  agents/skills/<name>
    .cursor/skills/<name>      ->  agents/skills/<name>
    .claude/agents/<name>.md   ->  agents/agents/<name>.md
    .cursor/agents/<name>.md   ->  agents/agents/<name>.md
    .bravebot/agents/<name>.md ->  agents/agents/<name>.md
    .claude/CLAUDE.md          ->  agents/AGENTS.md
    AGENTS.md                  ->  agents/AGENTS.md
    .claude/settings.json      ->  agents/settings.json
    .bravebot/settings.json    ->  agents/settings.json

Codex reads a subagent as a TOML file and not as Markdown, so a link cannot serve it. Its
file is written instead, one per definition:

    .codex/agents/<name>.toml  <-  agents/agents/<name>.md

The generated links and files are gitignored and never committed, which is why this runs
from `make init` rather than being a one-time setup somebody has to remember.

It is idempotent: re-running only creates links that are missing or stale and rewrites a
generated file only when its source changed, and it never clobbers a real file or directory
somebody placed in a discovery dir by hand. A generated file is recognised by the first
line it carries, so one written by hand under the same name is left alone.

It does not touch the trust map. bravebot loads a workspace skill only from a path a
person vouched for (TRUST-1 in docs/specs/trust-map.md), and a script granting that on
the user's behalf is exactly the inference that clause forbids. Expect to be asked once,
at startup, about `.bravebot/skills`.

On Windows we COPY instead of symlink, because git's symlink support there is unreliable
and creating symlinks often needs elevated privileges.

Usage:
    python3 agents/setup.py link    # create the links (default)
    python3 agents/setup.py list    # show source vs. linked state
    python3 agents/setup.py unlink  # remove only the links we own
"""

import argparse
import logging
import os
import re
import shutil
import sys
from pathlib import Path

# .../<repo>/agents/setup.py -> parents[1] == <repo>
_ROOT = Path(__file__).resolve().parents[1]
_SRC = _ROOT / 'agents'
_IS_WINDOWS = os.name == 'nt'

# One source directory fans out to one link per child, rather than linking the directory
# itself, so a discovery dir can also hold entries this repo does not own.
#
# Codex reads skills only. Cursor and bravebot also read subagents, and bravebot ignores the
# frontmatter keys it has no use for, so one checked-in `agents/` serves both. Cursor dropped
# slash commands in favour of skills, so `commands/` links into `.claude/` alone.
#
# Cursor reads `.agents/skills` and `.claude/agents` as compatibility paths, so its own
# paths are listed too: they are what it documents, they win a name conflict against the
# compatibility ones, and support here should not rest on shims Cursor may retire.
_FANOUT = [
    ('skills', ['.claude/skills', '.agents/skills', '.bravebot/skills', '.cursor/skills']),
    ('commands', ['.claude/commands']),
    ('agents', ['.claude/agents', '.cursor/agents', '.bravebot/agents']),
]

# One file to one destination each, under the name the tool reading it looks for.
#
# The instructions: bravebot, Codex and Cursor read them from the workspace root, and Claude
# Code from `.claude/CLAUDE.md`.
#
# The settings: what a commit message and a pull request may carry, asked for in the
# instructions as well and stated here where no model is involved in honouring it. Claude Code
# and bravebot both read a `settings.json` in this shape, so one file serves both. Cursor has no
# equivalent file and has the instructions instead. Codex has none either, and no project-level
# file at all: its `commit_attribution_enabled` is an account setting read over the network, so
# turning it off is a step in `docs/development/agent-configuration.md` rather than anything
# `make init` can link.
_FILES = [
    ('AGENTS.md', ['.claude/CLAUDE.md', 'AGENTS.md']),
    ('settings.json', ['.claude/settings.json', '.bravebot/settings.json']),
]

# A child of a fanned-out source dir is only worth linking if it is a real entry rather
# than a stray file, so skills are recognised the way both tools recognise them.
_MARKERS = {'skills': ('SKILL.md', 'skill.md')}

# Codex reads a subagent from a TOML file, one per agent, and not from Markdown, so the
# definitions in `agents/agents/` are converted rather than linked.
#
# Codex requires a name, a description and the instructions. The rest of a definition's
# frontmatter has no Codex key and is dropped: `kind` and `tools` narrow what a bravebot
# delegate may do, and a Codex agent takes the permissions of the session that spawns it.
_CODEX_AGENTS = '.codex/agents'

# The first line of every file written there. It is how a later run knows a file is its own to
# replace or remove, and that a file somebody wrote by hand is not.
_GENERATED_PREFIX = '# Generated by agents/setup.py'
_GENERATED = _GENERATED_PREFIX + ' from {source}. Edit that file and run `make init`.'

_FRONTMATTER_MARKER = '---'


def _indent_of(line: str) -> int:
    return len(line) - len(line.lstrip())


def _unquoted(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] == "'":
        return value[1:-1].replace("''", "'")
    if len(value) >= 2 and value[0] == value[-1] == '"':
        return value[1:-1].replace('\\"', '"')
    return value


def _value_of(first: str, wrapped: list[str]) -> str:
    """One value, from what followed the colon and the lines indented under it."""
    first = first.strip()
    lines = [line.strip() for line in wrapped]
    if first in ('|', '|-', '|+'):
        # A literal block keeps its newlines, minus the blank lines around it.
        while lines and not lines[0]:
            lines.pop(0)
        while lines and not lines[-1]:
            lines.pop()
        return _unquoted('\n'.join(lines))
    if first in ('>', '>-', '>+'):
        first = ''
    return _unquoted(' '.join(part for part in [first, *lines] if part))


def _frontmatter(text: str) -> tuple[dict[str, str], str] | None:
    """The `key: value` declarations at the top of `text` and what follows them.

    This reads the dialect bravebot reads (`declarations` in crates/agent/src/skills.rs), so a
    definition means the same to Codex as it does to bravebot: a wrapped value is joined with
    spaces, a `|` block keeps its newlines, and a continuation line holding a colon is not a
    key. It is a second copy of that reading because this script has to run before anything
    is built. None means the text has no closed frontmatter block.
    """
    lines = text.replace('\r\n', '\n').split('\n')
    if lines[0].rstrip() != _FRONTMATTER_MARKER:
        return None
    for end in range(1, len(lines)):
        if lines[end].rstrip() == _FRONTMATTER_MARKER:
            break
    else:
        return None

    block = lines[1:end]
    declared: dict[str, str] = {}
    at = 0
    while at < len(block):
        line = block[at]
        at += 1
        wrapped = []
        while at < len(block) and (not block[at].strip() or _indent_of(block[at]) > _indent_of(line)):
            wrapped.append(block[at])
            at += 1
        key, colon, first = line.partition(':')
        if colon:
            declared[key.strip()] = _value_of(first, wrapped)
    return declared, '\n'.join(lines[end + 1:])


_LINK_TARGET = re.compile(r'(\]\()([^)\s]+)(\))')
_HAS_SCHEME = re.compile(r'^[A-Za-z][A-Za-z0-9+.-]*:')


def _repoint(body: str, src: Path) -> str:
    """Make each relative link in `body` relative to the repository root.

    A definition links with paths relative to its own directory, which a Codex session, running
    from the root, would not resolve. A link that leaves the repository, names a URL, or points
    inside the page is left as written.
    """

    def one(found: re.Match) -> str:
        path, hash_, fragment = found.group(2).partition('#')
        if not path or path.startswith('/') or _HAS_SCHEME.match(path):
            return found.group(0)
        rel = os.path.relpath(os.path.normpath(src.parent / path), _ROOT)
        if rel == '..' or rel.startswith('..' + os.sep):
            return found.group(0)
        return f'{found.group(1)}{Path(rel).as_posix()}{hash_}{fragment}{found.group(3)}'

    return _LINK_TARGET.sub(one, body)


def _toml_string(text: str, multiline: bool = False) -> str:
    """`text` as a TOML basic string, spread over lines when `multiline`."""
    out = []
    for ch in text:
        if ch == '\\':
            out.append('\\\\')
        elif ch == '"':
            out.append('\\"')
        elif ch == '\n' and multiline:
            out.append(ch)
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\t':
            out.append('\\t')
        elif ord(ch) < 0x20 or ord(ch) == 0x7f:
            out.append(f'\\u{ord(ch):04x}')
        else:
            out.append(ch)
    escaped = ''.join(out)
    return f'"""\n{escaped}"""' if multiline else f'"{escaped}"'


def render_codex_agent(src: Path) -> str:
    """The TOML Codex reads for the definition at `src`.

    Raises ValueError, naming the definition, when it lacks what Codex requires.
    """
    rel = src.relative_to(_ROOT).as_posix()
    parsed = _frontmatter(src.read_text(encoding='utf-8'))
    if parsed is None:
        raise ValueError(f'{rel} has no frontmatter block')
    declared, body = parsed
    instructions = _repoint(body, src).strip()
    for what, value in (('name', declared.get('name')), ('description', declared.get('description')),
                        ('instructions after the frontmatter', instructions)):
        if not value:
            raise ValueError(f'{rel} has no {what}')
    name = _toml_string(declared['name'])
    description = _toml_string(declared['description'])
    developer_instructions = _toml_string(instructions + '\n', multiline=True)
    return (_GENERATED.format(source=rel) + '\n'
            f'name = {name}\n'
            f'description = {description}\n'
            f'developer_instructions = {developer_instructions}\n')


def _log(msg, *args):
    logging.info(msg, *args)


def _is_entry(path: Path, kind: str) -> bool:
    markers = _MARKERS.get(kind)
    if markers is None:
        return path.is_dir() or path.suffix == '.md'
    return any((path / marker).exists() for marker in markers)


def planned_links() -> list[tuple[Path, Path]]:
    """Every (source, destination) pair this script owns, in link order.

    Sources that do not exist yet are skipped rather than reported, so adding a
    `commands/` directory later needs no change here.
    """
    links: list[tuple[Path, Path]] = []

    for kind, dests in _FANOUT:
        src_dir = _SRC / kind
        if not src_dir.is_dir():
            continue
        for entry in sorted(src_dir.iterdir()):
            if entry.name.startswith('.') or not _is_entry(entry, kind):
                continue
            for dest in dests:
                links.append((entry, _ROOT / dest / entry.name))

    for name, dests in _FILES:
        src = _SRC / name
        if not src.is_file():
            continue
        for dest in dests:
            links.append((src, _ROOT / dest))

    return links


def planned_codex_agents() -> list[tuple[Path, Path]]:
    """Every (definition, Codex file) pair this script writes, in name order."""
    src_dir = _SRC / 'agents'
    if not src_dir.is_dir():
        return []
    return [(entry, _ROOT / _CODEX_AGENTS / f'{entry.stem}.toml')
            for entry in sorted(src_dir.iterdir())
            if not entry.name.startswith('.') and entry.is_file() and entry.suffix == '.md']


def _is_generated(path: Path) -> bool:
    """Whether `path` is a file this script wrote, by the line it begins with."""
    if path.is_symlink() or not path.is_file():
        return False
    try:
        with path.open(encoding='utf-8') as f:
            return f.readline().startswith(_GENERATED_PREFIX)
    except (OSError, UnicodeDecodeError):
        return False


def _orphans() -> list[Path]:
    """Generated Codex files whose definition no longer exists, which Codex would still offer."""
    wanted = {dest for _, dest in planned_codex_agents()}
    found = _ROOT / _CODEX_AGENTS
    if not found.is_dir():
        return []
    return [path for path in sorted(found.glob('*.toml')) if path not in wanted and _is_generated(path)]


def _generate_one(src: Path, dest: Path) -> tuple[bool, bool]:
    """Write or refresh one Codex agent file. Returns (ok, changed) as `_link_one` does."""
    rel = dest.relative_to(_ROOT)
    try:
        content = render_codex_agent(src).encode('utf-8')
    except ValueError as e:
        logging.error('  ERROR generating %s: %s', rel, e)
        return False, False

    if (dest.is_symlink() or dest.exists()) and not _is_generated(dest):
        logging.warning('  SKIP %s: a real path is already there (left as-is)', rel)
        return True, False
    if dest.exists() and dest.read_bytes() == content:
        return True, False

    dest.parent.mkdir(parents=True, exist_ok=True)
    # Bytes, so Windows does not turn the newlines into carriage returns and a second run
    # finds the file unchanged.
    dest.write_bytes(content)
    logging.debug('  wrote   %s', rel)
    return True, True


def _link_one(src: Path, dest: Path) -> tuple[bool, bool]:
    """Create or refresh one link (or copy on Windows).

    Returns (ok, changed). `changed` is False when the link was already correct or a real
    path was left alone, so a re-run with nothing to do stays silent.
    """
    rel = dest.relative_to(_ROOT)

    if dest.is_symlink():
        try:
            if dest.resolve() == src.resolve():
                return True, False
        except OSError:
            pass  # broken link, replaced below
        _log('  refreshing stale link: %s', rel)
        dest.unlink()
    elif dest.exists():
        if _IS_WINDOWS:
            # Our own copy from a previous run. Refresh it.
            shutil.rmtree(dest) if dest.is_dir() else dest.unlink()
        else:
            # Somebody's real file or directory. Warning level so it survives -q.
            logging.warning('  SKIP %s: a real path is already there (left as-is)', rel)
            return True, False

    dest.parent.mkdir(parents=True, exist_ok=True)
    if _IS_WINDOWS:
        shutil.copytree(src, dest) if src.is_dir() else shutil.copy2(src, dest)
        logging.debug('  copied  %s', rel)
    else:
        # A relative target keeps the link valid if the checkout moves.
        os.symlink(os.path.relpath(src, dest.parent), dest,
                   target_is_directory=src.is_dir())
        logging.debug('  linked  %s', rel)
    return True, True


def link(_args) -> bool:
    links = planned_links()
    generated = planned_codex_agents()
    if not links and not generated:
        _log('Nothing to link: %s holds no skills, commands or AGENTS.md.', _SRC)
        return True

    ok = True
    changed = 0
    for src, dest in links:
        try:
            one_ok, one_changed = _link_one(src, dest)
            ok = one_ok and ok
            changed += one_changed
        except OSError as e:
            logging.error('  ERROR linking %s: %s', dest.relative_to(_ROOT), e)
            ok = False

    for src, dest in generated:
        try:
            one_ok, one_changed = _generate_one(src, dest)
            ok = one_ok and ok
            changed += one_changed
        except OSError as e:
            logging.error('  ERROR writing %s: %s', dest.relative_to(_ROOT), e)
            ok = False

    for path in _orphans():
        try:
            path.unlink()
            _log('  removed %s: its definition is gone', path.relative_to(_ROOT))
            changed += 1
        except OSError as e:
            logging.error('  ERROR removing %s: %s', path.relative_to(_ROOT), e)
            ok = False

    if changed:
        _log('Linked %d path(s) from agents/ into agent discovery paths.', changed)
    return ok


def unlink(_args) -> bool:
    """Remove only links pointing into agents/ and files this script wrote, never a real path."""
    for path in [dest for _, dest in planned_codex_agents()] + _orphans():
        if _is_generated(path):
            path.unlink()
            _log('  removed %s', path.relative_to(_ROOT))
    for src, dest in planned_links():
        if dest.is_symlink():
            try:
                if dest.resolve() != src.resolve():
                    continue
            except OSError:
                pass  # broken link into our own tree, safe to drop
            dest.unlink()
            _log('  unlinked %s', dest.relative_to(_ROOT))
        elif dest.exists() and _IS_WINDOWS:
            shutil.rmtree(dest) if dest.is_dir() else dest.unlink()
            _log('  removed copy %s', dest.relative_to(_ROOT))
    return True


def _generated_state(src: Path, dest: Path) -> str:
    if not (dest.is_symlink() or dest.exists()):
        return 'missing'
    if not _is_generated(dest):
        return 'real path (not ours)'
    try:
        current = dest.read_bytes() == render_codex_agent(src).encode('utf-8')
    except ValueError:
        return 'UNREADABLE definition'
    return 'generated' if current else 'STALE generated'


def show(_args) -> bool:
    links = planned_links()
    generated = planned_codex_agents()
    print(f'source: {_SRC}\n')
    if not links and not generated:
        print('(nothing to link)')
        return True

    rows = []
    for src, dest in links:
        if dest.is_symlink():
            state = 'link' if dest.exists() else 'BROKEN link'
        elif dest.exists():
            state = 'real path (not ours)'
        else:
            state = 'missing'
        rows.append((dest, state))
    rows += [(dest, _generated_state(src, dest)) for src, dest in generated]
    rows += [(path, 'orphan (removed by link)') for path in _orphans()]

    width = max(len(str(d.relative_to(_ROOT))) for d, _ in rows)
    print(f'{"DESTINATION".ljust(width)}  STATE')
    print(f'{"-" * width}  -----')
    for dest, state in rows:
        print(f'{str(dest.relative_to(_ROOT)).ljust(width)}  {state}')
    return True


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command',
                        choices=['link', 'unlink', 'list'],
                        nargs='?',
                        default='link')
    parser.add_argument('-q',
                        '--quiet',
                        action='store_true',
                        help='Only log warnings and errors.')
    args = parser.parse_args()

    logging.basicConfig(level=logging.WARNING if args.quiet else logging.INFO,
                        format='%(message)s')

    handler = {'link': link, 'unlink': unlink, 'list': show}[args.command]
    return 0 if handler(args) else 1


if __name__ == '__main__':
    sys.exit(main())
