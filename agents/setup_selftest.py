#!/usr/bin/env python3
"""Selftest for setup.py, which links agents/ into each tool's discovery directories and writes
the TOML files Codex reads its subagents from."""

import contextlib
import importlib.util
import io
import os
import re
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("agents_setup", HERE / "setup.py")
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)

ALPHA = """\
---
name: alpha
description:
  'Does the alpha work. It said: use it for
  every case, and it''s done.'
kind: worker
tools: read_file, search
---

You do the alpha work. See [the guide](../../docs/guide.md#steps), [the rules](../AGENTS.md),
[the site](https://example.com/a/../b) and [below](#report).

## Report

Say what you did.
"""

BETA = """\
---
name: beta
description: Does the beta work.
---

You do the beta work.
"""


@contextlib.contextmanager
def repo(definitions):
    """A checkout holding the given files in agents/agents/, with setup.py pointed at it."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        (root / "agents" / "agents").mkdir(parents=True)
        for filename, text in definitions.items():
            (root / "agents" / "agents" / filename).write_text(text, encoding="utf-8")
        with mock.patch.object(setup, "_ROOT", root), mock.patch.object(setup, "_SRC", root / "agents"):
            yield root


def codex(root, name):
    return tomllib.loads((root / ".codex" / "agents" / f"{name}.toml").read_text(encoding="utf-8"))


class Frontmatter(unittest.TestCase):
    def test_a_value_wrapped_over_lines_is_joined_with_spaces_and_unquoted(self):
        declared, _ = setup._frontmatter(ALPHA)
        self.assertEqual(declared["description"], "Does the alpha work. It said: use it for every case, and it's done.")

    def test_a_continuation_line_holding_a_colon_is_not_a_key(self):
        declared, _ = setup._frontmatter(ALPHA)
        self.assertEqual(sorted(declared), ["description", "kind", "name", "tools"])

    def test_a_folded_block_joins_its_lines_and_a_literal_block_keeps_them(self):
        text = "---\nname: n\ndescription: >\n  one\n  two\nnotes: |\n\n  first\n  second\n\n---\nbody\n"
        declared, body = setup._frontmatter(text)
        self.assertEqual(declared["description"], "one two")
        self.assertEqual(declared["notes"], "first\nsecond")
        self.assertEqual(body, "body\n")

    def test_text_without_a_closed_block_is_not_frontmatter(self):
        self.assertIsNone(setup._frontmatter("name: n\n---\nbody\n"))
        self.assertIsNone(setup._frontmatter("---\nname: n\nbody never closed\n"))


class Rendering(unittest.TestCase):
    def test_a_definition_becomes_the_three_keys_codex_requires(self):
        with repo({"alpha.md": ALPHA}) as root:
            self.assertTrue(setup.link(None))
            found = codex(root, "alpha")
        self.assertEqual(sorted(found), ["description", "developer_instructions", "name"])
        self.assertEqual(found["name"], "alpha")
        self.assertEqual(found["description"], "Does the alpha work. It said: use it for every case, and it's done.")
        self.assertTrue(found["developer_instructions"].startswith("You do the alpha work."))
        self.assertTrue(found["developer_instructions"].endswith("Say what you did.\n"))

    def test_two_definitions_keep_their_own_name_description_and_instructions(self):
        with repo({"alpha.md": ALPHA, "beta.md": BETA}) as root:
            setup.link(None)
            alpha, beta = codex(root, "alpha"), codex(root, "beta")
        self.assertEqual((beta["name"], beta["description"]), ("beta", "Does the beta work."))
        self.assertEqual(beta["developer_instructions"], "You do the beta work.\n")
        self.assertNotEqual(alpha["developer_instructions"], beta["developer_instructions"])

    def test_text_that_is_special_in_toml_survives_unchanged(self):
        description = 'Path a\\b with "quotes", a run """ of them and a tab\t.'
        awkward = 'A backslash \\ and \\n, a quote " and a run """ of them, a tab\there, \x01 a control, é.\n'
        text = f"---\nname: awkward\ndescription: '{description}'\n---\n\n{awkward}\n```\n'''\n```\n"
        with repo({"awkward.md": text}) as root:
            setup.link(None)
            found = codex(root, "awkward")
        self.assertEqual(found["description"], description)
        self.assertEqual(found["developer_instructions"], awkward + "\n```\n'''\n```\n")

    def test_a_relative_link_is_made_relative_to_the_repository_root(self):
        with repo({"alpha.md": ALPHA}) as root:
            setup.link(None)
            instructions = codex(root, "alpha")["developer_instructions"]
        self.assertIn("[the guide](docs/guide.md#steps)", instructions)
        self.assertIn("[the rules](agents/AGENTS.md)", instructions)

    def test_a_url_an_anchor_and_a_link_out_of_the_repository_are_left_as_written(self):
        text = ("---\nname: n\ndescription: d\n---\n"
                "[a](https://example.com/a/../b) [b](#report) [c](/abs/path.md) [d](../../../outside.md) "
                "[e](mailto:x@example.com)\n")
        with repo({"n.md": text}) as root:
            setup.link(None)
            instructions = codex(root, "n")["developer_instructions"]
        self.assertEqual(instructions, text.split("---\n", 2)[2])

    def test_a_definition_missing_what_codex_requires_is_reported_and_not_written(self):
        cases = {
            "noname.md": ("---\ndescription: d\n---\nbody\n", "has no name"),
            "nodescription.md": ("---\nname: n\n---\nbody\n", "has no description"),
            "nobody.md": ("---\nname: n\ndescription: d\n---\n\n", "has no instructions"),
            "nofrontmatter.md": ("just prose\n", "has no frontmatter block"),
        }
        with repo({"good.md": BETA, **{k: v[0] for k, v in cases.items()}}) as root:
            with self.assertLogs(level="ERROR") as logs:
                ok = setup.link(None)
            written = sorted(p.name for p in (root / ".codex" / "agents").iterdir())
        self.assertFalse(ok)
        self.assertEqual(written, ["good.toml"])
        for filename, (_, reason) in cases.items():
            self.assertTrue(
                any(f"agents/agents/{filename} {reason}" in line for line in logs.output),
                f"{filename}: {logs.output}",
            )


class Linking(unittest.TestCase):
    def test_a_second_run_with_nothing_to_do_says_nothing_and_changes_nothing(self):
        with repo({"alpha.md": ALPHA}) as root:
            setup.link(None)
            path = root / ".codex" / "agents" / "alpha.toml"
            first = path.stat().st_mtime_ns
            with self.assertNoLogs(level="INFO"):
                self.assertTrue(setup.link(None))
            self.assertEqual(path.stat().st_mtime_ns, first)

    def test_an_edited_definition_is_written_again(self):
        with repo({"alpha.md": ALPHA}) as root:
            setup.link(None)
            (root / "agents" / "agents" / "alpha.md").write_text(ALPHA.replace("alpha work.", "alpha chore."), encoding="utf-8")
            setup.link(None)
            self.assertIn("alpha chore.", codex(root, "alpha")["developer_instructions"])

    def test_a_file_written_by_hand_under_the_same_name_is_left_alone_with_a_warning(self):
        with repo({"alpha.md": ALPHA}) as root:
            mine = root / ".codex" / "agents" / "alpha.toml"
            mine.parent.mkdir(parents=True)
            mine.write_text('name = "mine"\n', encoding="utf-8")
            with self.assertLogs(level="WARNING") as logs:
                self.assertTrue(setup.link(None))
            self.assertEqual(mine.read_text(encoding="utf-8"), 'name = "mine"\n')
            self.assertTrue(any("alpha.toml" in line and "left as-is" in line for line in logs.output))

    @unittest.skipIf(os.name == "nt", "a symlink needs privileges on Windows")
    def test_a_symlink_where_the_file_belongs_is_not_written_through(self):
        with repo({"alpha.md": ALPHA}) as root:
            # The target begins with the marker, so only the link itself says it is not ours.
            untouched = setup._GENERATED_PREFIX + " from elsewhere.\nuntouched\n"
            elsewhere = root / "elsewhere.toml"
            elsewhere.write_text(untouched, encoding="utf-8")
            dest = root / ".codex" / "agents" / "alpha.toml"
            dest.parent.mkdir(parents=True)
            dest.symlink_to(elsewhere)
            with self.assertLogs(level="WARNING"):
                setup.link(None)
            self.assertEqual(elsewhere.read_text(encoding="utf-8"), untouched)
            self.assertTrue(dest.is_symlink())

    def test_a_generated_file_whose_definition_is_gone_is_removed_and_a_hand_written_one_is_not(self):
        with repo({"alpha.md": ALPHA, "beta.md": BETA}) as root:
            setup.link(None)
            mine = root / ".codex" / "agents" / "mine.toml"
            mine.write_text('name = "mine"\n', encoding="utf-8")
            (root / "agents" / "agents" / "beta.md").unlink()
            setup.link(None)
            left = sorted(p.name for p in (root / ".codex" / "agents").iterdir())
        self.assertEqual(left, ["alpha.toml", "mine.toml"])

    def test_unlink_removes_the_files_it_wrote_and_nothing_else(self):
        with repo({"alpha.md": ALPHA}) as root:
            setup.link(None)
            mine = root / ".codex" / "agents" / "mine.toml"
            mine.write_text('name = "mine"\n', encoding="utf-8")
            setup.unlink(None)
            left = sorted(p.name for p in (root / ".codex" / "agents").iterdir())
        self.assertEqual(left, ["mine.toml"])

    @unittest.skipIf(os.name == "nt", "Windows copies instead of linking")
    def test_the_other_tools_get_the_markdown_linked_and_codex_gets_no_copy_of_it(self):
        with repo({"alpha.md": ALPHA}) as root:
            setup.link(None)
            source = root / "agents" / "agents" / "alpha.md"
            for where in (".claude/agents", ".cursor/agents", ".bravebot/agents"):
                link = root / where / "alpha.md"
                self.assertTrue(link.is_symlink(), where)
                self.assertEqual(link.resolve(), source.resolve(), where)
            self.assertFalse((root / ".agents" / "agents").exists())
            self.assertFalse((root / ".codex" / "agents" / "alpha.md").exists())

    def test_list_says_what_each_codex_file_is(self):
        with repo({"alpha.md": ALPHA, "beta.md": BETA, "gamma.md": BETA.replace("beta", "gamma")}) as root:
            setup.link(None)
            agents = root / ".codex" / "agents"
            (root / "agents" / "agents" / "beta.md").write_text(BETA.replace("beta work", "beta chore"), encoding="utf-8")
            (agents / "gamma.toml").write_text('name = "mine"\n', encoding="utf-8")
            (root / "agents" / "agents" / "delta.md").write_text(BETA.replace("beta", "delta"), encoding="utf-8")
            orphan = agents / "old.toml"
            orphan.write_text(setup._GENERATED_PREFIX + " from agents/agents/old.md.\n", encoding="utf-8")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                setup.show(None)
        states = {}
        for line in out.getvalue().splitlines():
            found = re.match(r"\.codex/agents/(\w+)\.toml\s+(.*)", line)
            if found:
                states[found.group(1)] = found.group(2)
        self.assertEqual(states, {
            "alpha": "generated",
            "beta": "STALE generated",
            "delta": "missing",
            "gamma": "real path (not ours)",
            "old": "orphan (removed by link)",
        })


class CheckedIn(unittest.TestCase):
    def test_every_checked_in_definition_converts_to_toml_codex_accepts(self):
        pairs = setup.planned_codex_agents()
        self.assertTrue(pairs, "agents/agents/ holds no definition")
        for src, dest in pairs:
            found = tomllib.loads(setup.render_codex_agent(src))
            for key in ("name", "description", "developer_instructions"):
                self.assertTrue(found.get(key, "").strip(), f"{src.name}: {key}")
            self.assertEqual(dest.name, f"{src.stem}.toml")

    def test_the_issue_poster_keeps_its_prose_and_loses_its_yaml_quoting_and_relative_links(self):
        found = tomllib.loads(setup.render_codex_agent(setup._SRC / "agents" / "issue-poster.md"))
        self.assertEqual(found["name"], "issue-poster")
        self.assertTrue(found["description"].startswith("Files one issue on brave/bravebot"))
        self.assertTrue(found["description"].endswith("in place of a bare gh issue create."))
        self.assertTrue(found["developer_instructions"].startswith("You file one issue on `brave/bravebot`"))
        self.assertIn("(docs/development/labelling-issues.md)", found["developer_instructions"])
        self.assertIsNone(re.search(r"\]\(\.\./", found["developer_instructions"]))


if __name__ == "__main__":
    unittest.main()
