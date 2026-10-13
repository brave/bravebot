#!/usr/bin/env python3
"""The decisions maintenance.py makes without a model: what is a candidate, and what a draft says."""

import contextlib
import importlib.util
import io
import json
import os
# These tests run git with argument lists against temporary repositories.
import subprocess  # nosemgrep: gitlab.bandit.B404
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("maintenance", HERE / "maintenance.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

os.environ.update(
    GIT_CONFIG_GLOBAL=os.devnull,
    GIT_CONFIG_NOSYSTEM="1",
    GIT_AUTHOR_NAME="selftest",
    GIT_AUTHOR_EMAIL="selftest@example.invalid",
    GIT_COMMITTER_NAME="selftest",
    GIT_COMMITTER_EMAIL="selftest@example.invalid",
)

EM_DASH = chr(0x2014)


@contextlib.contextmanager
def tree(files):
    """A git repository holding `files`, every one of them tracked, as the working directory."""
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp).resolve()
        for name, text in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(textwrap.dedent(text), encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "add", "-A"], check=True)
        before = os.getcwd()
        os.chdir(root)
        m.drafting._TRACKED.clear()
        try:
            yield root
        finally:
            os.chdir(before)


def found(files, lane, **options):
    with tree(files) as root:
        return m.scan(root, (lane,), **options)[lane]["candidates"]


def symbols(files, lane):
    return sorted(c["symbol"] for c in found(files, lane))


def quiet(function, *args):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()) as err:
        code = function(*args)
    return code, out.getvalue(), err.getvalue()


def block(name, lines=10):
    """Lines that are code, long enough to count, and differ from any other `name`."""
    return "".join(f"    let {name}_{i} = compute_the_value({i}, \"{name}\", other_input + {i});\n" for i in range(lines))


class References(unittest.TestCase):
    def test_a_public_item_named_in_another_file_is_not_a_candidate(self):
        files = {
            "crates/a/src/lib.rs": "pub fn shared_helper() {}\n",
            "crates/b/src/lib.rs": "fn caller() { a::shared_helper(); }\n",
        }
        self.assertEqual(symbols(files, "unreferenced"), [])
        self.assertEqual(symbols(files, "test-only"), [])

    def test_a_public_item_named_nowhere_else_is_a_candidate(self):
        files = {"crates/a/src/lib.rs": "pub fn forgotten() {}\npub struct Forgotten;\n"}
        self.assertEqual(symbols(files, "unreferenced"), ["Forgotten", "forgotten"])

    def test_a_name_in_a_comment_is_not_a_use(self):
        files = {
            "crates/a/src/lib.rs": "pub fn forgotten() {}\n",
            "crates/b/src/lib.rs": "// forgotten is still here\n/* outer /* forgotten */ forgotten */\nfn x() {}\n",
        }
        self.assertEqual(symbols(files, "unreferenced"), ["forgotten"])

    def test_a_name_in_a_string_is_a_use(self):
        files = {
            "crates/a/src/lib.rs": "pub fn encode_thing() {}\n",
            "crates/b/src/lib.rs": '#[serde(serialize_with = "encode_thing")]\nstruct S;\n',
        }
        self.assertEqual(symbols(files, "unreferenced"), [])

    def test_a_double_slash_in_a_raw_string_does_not_start_a_comment(self):
        files = {
            "crates/a/src/lib.rs": "pub fn after_the_url() {}\n",
            "crates/b/src/lib.rs": 'fn x() { let u = r#"http://x "q""#; after_the_url(); }\n',
        }
        self.assertEqual(symbols(files, "unreferenced"), [])

    def test_a_template_literal_does_not_start_a_comment(self):
        files = {
            "ui/src/a.ts": "export function afterTheUrl() {}\n",
            "ui/src/b.ts": "const u = `http://x/${id}`; afterTheUrl();\n",
        }
        self.assertEqual(symbols(files, "unreferenced"), [])

    def test_uses_only_from_test_code_make_it_test_only(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                pub fn checked_by_unit_tests() {}
                pub fn checked_by_integration_tests() {}
                pub fn checked_by_test_support() {}

                #[cfg(test)]
                mod tests {
                    fn t() { checked_by_unit_tests(); }
                }

                #[cfg(test)]
                mod support;
                """
            ),
            "crates/a/src/support.rs": "mod deeper;\nfn s() { checked_by_test_support(); }\n",
            "crates/a/src/support/deeper.rs": "pub fn only_reached_from_support() {}\n",
            "crates/b/src/lib.rs": "pub fn checked_by_test_helpers() {}\n#[cfg(test)]\nmod unit_test;\n",
            "crates/b/src/unit_test.rs": "mod shared;\n",
            "crates/b/src/unit_test/shared.rs": "fn h() { checked_by_test_helpers(); }\n",
            "crates/a/tests/it.rs": "fn t() { a::checked_by_integration_tests(); }\n",
        }
        self.assertEqual(
            symbols(files, "test-only"),
            [
                "checked_by_integration_tests",
                "checked_by_test_helpers",
                "checked_by_test_support",
                "checked_by_unit_tests",
            ],
        )
        self.assertEqual(symbols(files, "unreferenced"), [])

    def test_code_after_a_test_module_and_under_not_test_is_not_test_code(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                pub fn used_after_the_module() {}
                pub fn used_when_not_testing() {}
                fn f<'a>(x: &'a str) -> char { '"' }

                #[cfg(test)]
                mod tests {
                    fn t() { let close = "}"; }
                }

                fn real() { used_after_the_module(); }

                #[cfg(not(test))]
                fn other() { used_when_not_testing(); }
                """
            ),
        }
        self.assertEqual(symbols(files, "test-only"), [])
        self.assertEqual(symbols(files, "unreferenced"), [])

    def test_a_char_literal_quote_does_not_hide_a_test_module(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                pub fn only_tests_call_this() {}
                const QUOTE: char = '"';

                #[cfg(test)]
                mod tests {
                    fn t() { super::only_tests_call_this(); }
                }
                """
            ),
        }
        self.assertEqual(symbols(files, "test-only"), ["only_tests_call_this"])

    def test_python_and_typescript_definitions(self):
        files = {
            "contrib/tool.py": textwrap.dedent(
                '''\
                """
                def ghost_in_a_docstring():
                """
                UNUSED_LIMIT = 3


                def unused_function():
                    pass


                class Holder:
                    def method_is_not_module_level(self):
                        pass


                def main():
                    Holder()
                '''
            ),
            "ui/src/a.ts": "export const unusedExport = 1;\nexport default function () {}\n",
        }
        self.assertEqual(symbols(files, "unreferenced"), ["UNUSED_LIMIT", "unusedExport", "unused_function"])

    def test_a_generated_file_and_a_test_file_offer_no_candidates(self):
        files = {
            "crates/a/src/generated.rs": "// @generated by a tool\npub fn emitted() {}\n",
            "contrib/test_tool.py": "def helper_in_a_test():\n    pass\n",
        }
        self.assertEqual(symbols(files, "unreferenced"), [])


class Allowances(unittest.TestCase):
    def test_only_an_allowance_that_can_hide_a_real_warning_is_a_candidate(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                #[allow(dead_code)]
                fn plain() {}
                #[expect(unused_variables)]
                fn expected() {}
                #[cfg_attr(feature = "extra", allow(dead_code))]
                fn feature() {}
                #[cfg_attr(not(unix), allow(dead_code))]
                fn platform() {}
                #[cfg_attr(all(unix, feature = "extra"), allow(dead_code))]
                fn platform_and_feature() {}
                #[cfg_attr(target_os = "windows", allow(dead_code))]
                fn windows() {}
                #[allow(clippy::too_many_arguments)]
                fn other_lint() {}
                const TEXT: &str = "#[allow(dead_code)]";
                #[cfg(test)]
                mod tests {
                    #[allow(dead_code)]
                    fn in_a_test() {}
                }
                """
            ),
        }
        self.assertEqual([c["line"] for c in found(files, "allowances")], [1, 3, 5, 9])


class Duplicates(unittest.TestCase):
    def test_a_block_in_two_files_is_one_candidate_naming_both(self):
        files = {
            "crates/a/src/lib.rs": "fn a() {\n" + block("same") + "}\n",
            "crates/b/src/lib.rs": "fn b() {\n    let x = 1;\n" + block("same") + "}\n",
        }
        (candidate,) = found(files, "duplicates")
        self.assertEqual(
            [(c["path"], c["line"], c["end"]) for c in candidate["copies"]],
            [("crates/a/src/lib.rs", 2, 12), ("crates/b/src/lib.rs", 3, 13)],
        )

    def test_copies_that_drifted_in_a_line_are_still_one_candidate(self):
        files = {}
        for n in range(9):
            lines = block("shared", 14).split("\n")
            if n >= 7:
                lines[10] = f"    let changed_here = something_else_entirely({n}, extra_argument_value);"
            files[f"crates/c{n}/src/lib.rs"] = "fn f() {\n" + "\n".join(lines) + "}\n"
        (candidate,) = found(files, "duplicates")
        self.assertEqual(len(candidate["copies"]), 9)

    def test_tests_and_lines_of_braces_and_imports_are_not_duplicates(self):
        trivial = "".join(f"use crate::a_long_module_path::with_a_long_name_{i};\n}}\n" for i in range(8))
        files = {
            "crates/a/tests/one.rs": "fn a() {\n" + block("test") + "}\n",
            "crates/b/tests/two.rs": "fn b() {\n" + block("test") + "}\n",
            "crates/a/src/lib.rs": trivial + "#[cfg(test)]\nmod tests {\n" + block("unit") + "}\n",
            "crates/b/src/lib.rs": trivial + "#[cfg(test)]\nmod tests {\n" + block("unit") + "}\n",
        }
        self.assertEqual(found(files, "duplicates"), [])


class Literals(unittest.TestCase):
    def test_a_literal_spelled_out_across_files_is_a_candidate(self):
        files = {
            "crates/a/src/lib.rs": 'fn a() { f("refused by the policy"); g("refused by the policy"); }\n',
            "crates/b/src/lib.rs": 'fn b() { f("refused by the policy"); }\n',
        }
        (candidate,) = found(files, "literals")
        self.assertEqual(candidate["literal"], "refused by the policy")
        self.assertEqual(len(candidate["copies"]), 3)

    def test_what_is_not_a_repeated_literal(self):
        files = {
            "crates/a/src/lib.rs": 'fn a() { f("in one file only"); f("in one file only"); f("in one file only"); }\n'
            'fn b() { f("description_key"); f("description_key"); }\n'
            'use "imported/path/x";\n',
            "crates/b/src/lib.rs": 'fn b() { f("description_key"); }\nuse "imported/path/x";\n',
            "crates/a/tests/t.rs": 'fn t() { f("only in the tests"); f("only in the tests"); }\n',
            "crates/b/tests/t.rs": 'fn t() { f("only in the tests"); }\n',
            "crates/c/src/lib.rs": 'use "imported/path/x";\n#[cfg(test)]\nmod tests {\n    fn t() { f("in unit tests"); f("in unit tests"); }\n}\n',
            "crates/d/src/lib.rs": '#[cfg(test)]\nmod tests {\n    fn t() { f("in unit tests"); }\n}\n',
        }
        self.assertEqual(found(files, "literals"), [])

    def test_a_literal_inside_a_repeated_block_is_left_to_the_block(self):
        body = block("same") + '    f("the literal in the block");\n'
        files = {
            "crates/a/src/lib.rs": "fn a() {\n" + body + "}\n",
            "crates/b/src/lib.rs": "fn b() {\n" + body + "}\n",
            "crates/c/src/lib.rs": "fn c() {\n" + body + "}\n",
        }
        literals = [c["literal"] for c in found(files, "literals")]
        self.assertNotIn("the literal in the block", literals)
        self.assertNotIn("same", literals)

    def test_a_literal_across_directories_outranks_one_repeated_in_a_directory(self):
        files = {
            "ui/scripts/one.mjs": 'f("within one dir");\nf("across many dirs");\n',
            "ui/scripts/two.mjs": 'f("within one dir");\n',
            "ui/scripts/three.mjs": 'f("within one dir");\n',
            "ui/scripts/four.mjs": 'f("within one dir");\n',
            "ui/src/a.ts": 'f("across many dirs");\n',
            "packages/x/b.ts": 'f("across many dirs");\n',
        }
        self.assertEqual([c["literal"] for c in found(files, "literals")], ["across many dirs", "within one dir"])


class Size(unittest.TestCase):
    def test_a_long_file_is_a_candidate_unless_its_tests_are_what_make_it_long(self):
        long = "".join(f"const C{i}: u32 = {i};\n" for i in range(m.FILE_LINES + 1))
        files = {
            "crates/a/src/lib.rs": long,
            "crates/b/src/lib.rs": "fn real() {}\n#[cfg(test)]\nmod tests {\n" + long + "}\n",
        }
        self.assertEqual([c["path"] for c in found(files, "size")], ["crates/a/src/lib.rs"])

    def test_a_long_function_is_named_and_a_brace_in_a_string_does_not_end_it(self):
        body = '    let s = "}";\n' + "".join(f"    step({i});\n" for i in range(m.FUNCTION_LINES))
        python = "".join(f"        step({i})\n" for i in range(m.FUNCTION_LINES))
        arrow = "".join(f"  step({i});\n" for i in range(m.FUNCTION_LINES))
        files = {
            "crates/a/src/lib.rs": "fn long_one() {\n" + body + "}\nfn short_one() {\n    step(0);\n}\n",
            "contrib/tool.py": "def long_python(\n    first,\n    second,\n):\n    if first:\n" + python + "\n\ndef short():\n    pass\n",
            "ui/src/a.ts": "export const longArrow = async (x: number): Promise<void> => {\n" + arrow + "};\n",
        }
        named = {c["path"]: [f["name"] for f in c["functions"]] for c in found(files, "size")}
        self.assertEqual(
            named,
            {"crates/a/src/lib.rs": ["long_one"], "contrib/tool.py": ["long_python"], "ui/src/a.ts": ["longArrow"]},
        )


class Scope(unittest.TestCase):
    files = {
        "crates/a/src/lib.rs": "pub fn first_unused() {}\npub fn second_unused() {}\n",
        "crates/b/src/lib.rs": "pub fn third_unused() {}\n",
        "docs/specs/thing.md": "---\nid: THING\ngoverns:\n  - crates/a/src/lib.rs\nguards:\n  - symbol: Policy::first_unused\n---\n# Thing\n",
    }

    def test_naming_a_lane_scans_only_that_lane(self):
        with tree(self.files) as root:
            self.assertEqual(list(m.scan(root, ("size",))), ["size"])

    def test_a_path_prefix_limits_the_candidates_to_files_under_it(self):
        with tree(self.files) as root:
            scoped = m.scan(root, ("unreferenced",), ("crates/b",))["unreferenced"]
        self.assertEqual([c["symbol"] for c in scoped["candidates"]], ["third_unused"])

    def test_the_cap_keeps_the_first_candidates_and_counts_the_rest_as_left_out(self):
        with tree(self.files) as root:
            capped = m.scan(root, ("unreferenced",), limit=2)["unreferenced"]
        self.assertEqual([c["id"] for c in capped["candidates"]], ["U1", "U2"])
        self.assertEqual(capped["left_out"], 1)

    def test_a_candidate_names_the_spec_governing_it_and_the_guard_it_touches(self):
        with tree(self.files) as root:
            by_symbol = {c["symbol"]: c for c in m.scan(root, ("unreferenced",))["unreferenced"]["candidates"]}
        self.assertEqual(by_symbol["first_unused"]["specs"], ["docs/specs/thing.md"])
        self.assertEqual(by_symbol["first_unused"]["guards"], ["Policy::first_unused (docs/specs/thing.md)"])
        self.assertNotIn("guards", by_symbol["second_unused"])
        self.assertNotIn("specs", by_symbol["third_unused"])

    def test_mechanical_only_lists_candidates_and_exits_zero(self):
        with tree(self.files) as root:
            code, out, _ = quiet(m.main, ["--mechanical-only", "--root", str(root)])
        self.assertEqual(code, 0)
        self.assertIn("first_unused", out)
        self.assertIn("A candidate is not a finding until a reviewer reads it.", out)


def unused(count):
    return "".join(f"pub fn unused_number_{i}() {{}}\n" for i in range(count))


class Pipeline(unittest.TestCase):
    def prepared(self, root, count=3):
        work = root / ".work"
        code, out, err = quiet(m.main, ["prepare", "unreferenced", "--root", str(root), "--work-dir", str(work)])
        self.assertEqual(code, 0)
        pointer = json.loads(out)
        self.assertIn("unreferenced:", err)
        return json.loads(Path(pointer["manifest"]).read_text()), work

    def write(self, prompt, result):
        Path(prompt["results_file"]).write_text(json.dumps(result) if not isinstance(result, str) else result)

    def test_prepare_splits_a_lane_between_reviewers_and_states_the_rule(self):
        with tree({"crates/a/src/lib.rs": unused(17)}) as root:
            manifest, _ = self.prepared(root)
            (lane,) = manifest["lanes"]
            self.assertEqual([len(p["candidates"]) for p in lane["reviewer_prompts"]], [15, 2])
            prompt = Path(lane["reviewer_prompts"][1]["prompt_file"]).read_text()
        self.assertIn("never have untrusted content in their context", prompt)
        self.assertIn("**U16**", prompt)
        self.assertNotIn("**U15**", prompt)
        self.assertIn(lane["reviewer_prompts"][1]["results_file"], prompt)

    def test_collect_reports_every_gap_in_the_review(self):
        with tree({"crates/a/src/lib.rs": unused(17)}) as root:
            manifest, work = self.prepared(root)
            first, second = manifest["lanes"][0]["reviewer_prompts"]
            verdicts = [{"id": f"U{i}", "verdict": "keep", "reason": "exported"} for i in range(1, 15)]
            verdicts[0]["verdict"] = "cleanup"
            verdicts[1]["verdict"] = "cleanup"
            self.write(first, {
                "lane": "unreferenced",
                "candidates": verdicts,
                "findings": [{"candidates": ["U1", "U16"], "summary": "Delete unused_number_0", "fix": "Delete it."}],
            })
            code, _, _ = quiet(m.main, ["collect", "--work-dir", str(work)])
            findings, _, problems = m.load_reviews(manifest)

            self.write(second, "not json")
            _, _, unreadable = m.load_reviews(manifest)

        self.assertEqual(code, 1)
        self.assertEqual([f["candidates"] for f in findings], [["U1"]])
        gaps = sorted((p["kind"], p["summary"], tuple(p["candidates"])) for p in problems)
        self.assertEqual(
            gaps,
            [
                ("review-incomplete", "called cleanup, but no finding says what the change is", ("U2",)),
                ("review-incomplete", "no reviewer result: these candidates were never read", ("U16", "U17")),
                ("review-incomplete", "the reviewer returned no verdict for these candidates", ("U15",)),
            ],
        )
        self.assertIn("review-unreadable", [p["kind"] for p in unreadable])

    def test_collect_succeeds_when_every_candidate_has_a_verdict(self):
        with tree({"crates/a/src/lib.rs": unused(2)}) as root:
            manifest, work = self.prepared(root)
            (prompt,) = manifest["lanes"][0]["reviewer_prompts"]
            self.write(prompt, {"candidates": [{"id": "U1", "verdict": "keep"}, {"id": "U2", "verdict": "unclear"}]})
            code, out, _ = quiet(m.main, ["collect", "--work-dir", str(work)])
        self.assertEqual(code, 0)
        self.assertIn("0 findings", out)
        self.assertIn("U2 unclear", out)

    def test_drafts_carry_the_kind_the_area_and_the_assignee(self):
        files = {"crates/a/src/lib.rs": unused(1), "contrib/tool.py": "def unused_tool():\n    pass\n"}
        with tree(files) as root:
            manifest, work = self.prepared(root)
            (prompt,) = manifest["lanes"][0]["reviewer_prompts"]
            ids = {c["path"]: i for i, c in manifest["candidates"].items()}
            rust, tool = ids["crates/a/src/lib.rs"], ids["contrib/tool.py"]
            self.write(prompt, {
                "candidates": [{"id": rust, "verdict": "cleanup"}, {"id": tool, "verdict": "cleanup"}],
                "findings": [
                    {"candidates": [rust], "summary": "Delete `unused_number_0`", "area": "trust",
                     "evidence": ["crates/a/src/lib.rs:1 the function"], "fix": "Delete it"},
                    {"candidates": [tool], "summary": "Delete unused_tool", "fix": "Delete it."},
                    {"candidates": [rust], "summary": "A check nothing calls", "area": "made-up", "bug": True},
                    {"candidates": [rust, tool], "summary": "x" * (m.TITLE_LIMIT + 1)},
                ],
            })
            code, out, _ = quiet(m.main, ["draft", "--work-dir", str(work), "--json"])
            drafts = json.loads(out)["drafts"]
            bodies = [Path(d["body_file"]).read_text() for d in drafts]
            _, text, _ = quiet(m.main, ["draft", "--work-dir", str(work)])

        self.assertEqual(code, 0)
        self.assertEqual(
            [d["labels"] for d in drafts],
            [["maintenance", "area/trust"], ["maintenance", "infrastructure"], ["bug"], ["maintenance"]],
        )
        self.assertEqual({d["assignee"] for d in drafts}, {"netzenbot"})
        self.assertEqual(drafts[0]["title"], "Delete unused_number_0")
        self.assertEqual([d["title_too_long"] for d in drafts], [False, False, False, True])
        self.assertIn("1 | pub fn unused_number_0() {}", bodies[0])
        self.assertIn("## The fix\n\nDelete it.", bodies[0])
        self.assertIn("Behaviour does not change", bodies[0])
        self.assertNotIn("Behaviour does not change", bodies[2])
        self.assertFalse(any(EM_DASH in body for body in bodies))
        self.assertTrue(text.rstrip().endswith("Nothing has been posted."))
        self.assertIn("needs a shorter title", text)


if __name__ == "__main__":
    unittest.main()
