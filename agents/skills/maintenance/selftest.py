#!/usr/bin/env python3
"""The decisions maintenance.py makes without a model: what is a candidate, and what a draft says."""

import contextlib
import importlib.util
import io
import json
import os
import re
# These tests run git with argument lists against temporary repositories.
import subprocess  # nosemgrep: gitlab.bandit.B404
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

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


def commit_all(root, message="change"):
    subprocess.run(["git", "-C", str(root), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(root), "commit", "-q", "-m", message], check=True)


def head(root):
    done = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"], check=True, capture_output=True, text=True)
    return done.stdout.strip()


@contextlib.contextmanager
def committed(files):
    """`tree`, with its files committed."""
    with tree(files) as root:
        commit_all(root, "fixture")
        yield root


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

    def test_a_cfg_test_field_does_not_cover_the_impl_after_it(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                pub struct Client<T> {
                    attempts: u32,
                    #[cfg(test)]
                    test_request: Option<std::collections::HashMap<String, T>>,
                    config: u32,
                }

                impl Client<u8> {
                    pub fn shipped() { after_the_field(); }
                }

                pub fn make() -> Client<u8> {
                    Client {
                        attempts: 0,
                        #[cfg(test)]
                        test_request: None,
                        config: 0,
                    }
                }

                fn more() {
                    after_the_literal();
                    let c = Client { attempts: 0, #[cfg(test)] test_request: None, config: 0 };
                }

                impl Client<u16> {
                    pub fn also_shipped() { after_the_inline_literal(); }
                }

                #[cfg(test)]
                mod tests {
                    fn t() { used_only_in_tests(); }
                }
                """
            ),
            "crates/a/src/other.rs": (
                "pub fn after_the_field() {}\npub fn after_the_literal() {}\n"
                "pub fn after_the_inline_literal() {}\npub fn used_only_in_tests() {}\n"
            ),
        }
        self.assertEqual(symbols(files, "test-only"), ["used_only_in_tests"])

    def test_a_cfg_test_statement_block_still_covers_its_block(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                pub fn f() {
                    #[cfg(test)]
                    if let Some(x) = hook() {
                        used_only_under_the_statement();
                    }
                    used_after_the_statement();
                }
                """
            ),
            "crates/a/src/other.rs": "pub fn used_only_under_the_statement() {}\npub fn used_after_the_statement() {}\n",
        }
        self.assertEqual(symbols(files, "test-only"), ["used_only_under_the_statement"])

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
                #[allow(clippy::disallowed_methods)]
                fn unrelated_lint() {}
                #[allow(clippy::too_many_arguments)]
                fn admits_many_arguments() {}
                #[expect(clippy::cognitive_complexity, reason = "one state machine")]
                fn admits_complexity() {}
                const TEXT: &str = "#[allow(dead_code)]";
                #[cfg(test)]
                mod tests {
                    #[allow(dead_code)]
                    fn in_a_test() {}
                }
                """
            ),
        }
        self.assertEqual([c["line"] for c in found(files, "allowances")], [1, 3, 5, 9, 15, 17])


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


class Labels(unittest.TestCase):
    def test_cleanup_uses_refactor_and_defects_use_bug_with_the_same_area_and_no_axes(self):
        """Cleanup and defect findings retain their area without acquiring triage labels."""
        manifest = {"candidates": {"U1": {"path": "agents/tool.py"}}}
        for defect, kind in ((False, "refactor"), (True, "bug")):
            for area, expected_area in (("trust", "area/trust"), ("", "infrastructure")):
                with self.subTest(bug=defect, area=area):
                    finding = {"candidates": ["U1"], "bug": defect, "area": area}
                    self.assertEqual(m.labels_for(finding, manifest), [kind, expected_area])


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

    def test_drafts_use_refactor_for_cleanup_bug_for_defects_and_keep_area_and_assignee(self):
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
        expected_labels = [["refactor", "area/trust"], ["refactor", "infrastructure"], ["bug"], ["refactor"]]
        self.assertEqual([d["labels"] for d in drafts], expected_labels)
        for entry, labels in zip(drafts, expected_labels):
            self.assertIn(
                f"      {' '.join(labels)}, assignee netzenbot, body {entry['body_file']}", text
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


def nested(depth):
    """A function whose deepest statement sits `depth` blocks inside its body."""
    body = "x();"
    for _ in range(depth):
        body = "if a {\n" + body + "\n}"
    return "fn deep() {\n" + body + "\n}\n"


def names(files, lane, key="symbol"):
    return sorted(c[key] for c in found(files, lane))


class Complexity(unittest.TestCase):
    def test_a_function_nested_six_blocks_deep_is_a_candidate_and_five_is_not(self):
        files = {
            "crates/a/src/lib.rs": nested(m.NESTING).replace("deep", "too_deep")
            + nested(m.NESTING - 1).replace("deep", "deep_enough"),
        }
        self.assertEqual(names(files, "complexity"), ["too_deep"])

    def test_a_brace_in_a_string_is_not_a_block(self):
        braces = 'fn braces() {\n    let s = "{{{{{{{{{{";\n    work(s);\n}\n'
        self.assertEqual(names({"crates/a/src/lib.rs": braces}, "complexity"), [])

    def test_boolean_parameters_count_from_three_and_self_and_generic_bounds_do_not_hide_them(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                fn three(a: bool, b: bool, c: bool) {}
                fn two(a: bool, b: bool, c: u8) {}
                struct S;
                impl S {
                    fn method(&self, a: bool, b: bool) {}
                    fn method_with_three(&mut self, a: bool, b: bool, c: bool) {}
                }
                fn bounded<F: Fn(bool) -> bool>(f: F, a: bool, b: bool, c: bool) {}
                """
            ),
        }
        self.assertEqual(names(files, "complexity"), ["bounded", "method_with_three", "three"])

    def test_many_parameters_are_a_candidate_outside_rust_where_clippy_refuses_them(self):
        many = ", ".join(f"p{i}" for i in range(m.PARAMETERS))
        fewer = ", ".join(f"p{i}" for i in range(m.PARAMETERS - 1))
        typed = ", ".join(f"p{i}: u8" for i in range(m.PARAMETERS))
        files = {
            "crates/a/src/lib.rs": f"fn rust_many({typed}) {{}}\n",
            "contrib/tool.py": f"def python_many({many}):\n    pass\n\n\nclass K:\n    def python_method(self, {fewer}):\n        pass\n",
            "ui/src/a.ts": f"export function tsMany({many}) {{\n  return 1;\n}}\n",
        }
        self.assertEqual(names(files, "complexity"), ["python_many", "tsMany"])

    def test_python_depth_counts_blocks_and_not_the_lines_of_one_statement(self):
        blocks = "".join("    " * (i + 1) + "if a:\n" for i in range(m.NESTING)) + "    " * (m.NESTING + 1) + "go()\n"
        staircase = "".join(f"{' ' * (8 + 4 * i)}argument_{i},\n" for i in range(m.NESTING + 2))
        wrapped = "def wrapped():\n    value = compute(\n" + staircase + "    )\n"
        files = {"contrib/tool.py": "def blocks():\n" + blocks + "\n\n" + wrapped}
        self.assertEqual(names(files, "complexity"), ["blocks"])

    def test_tests_are_not_candidates(self):
        files = {
            "crates/a/src/lib.rs": "#[cfg(test)]\nmod tests {\n" + nested(m.NESTING + 2) + "}\n",
            "crates/a/tests/it.rs": nested(m.NESTING + 2),
        }
        self.assertEqual(names(files, "complexity"), [])


def uses(count, prefix="crate::"):
    return "".join(f"use {prefix}module_{i};\n" for i in range(count))


class Coupling(unittest.TestCase):
    def test_a_file_using_twelve_other_modules_is_a_candidate_and_eleven_is_not(self):
        files = {
            "crates/a/src/wide.rs": uses(m.COUPLING),
            "crates/a/src/narrow.rs": uses(m.COUPLING - 1),
        }
        self.assertEqual([c["path"] for c in found(files, "coupling")], ["crates/a/src/wide.rs"])

    def test_grouped_imports_count_one_module_per_path_and_nested_groups_do_not_add_more(self):
        grouped = "use crate::{" + ", ".join(f"m{i}::Thing" for i in range(m.COUPLING - 2)) + ", last::{one, two}, extra};\n"
        files = {"crates/a/src/grouped.rs": grouped}
        (candidate,) = found(files, "coupling")
        self.assertEqual(candidate["rank"], m.COUPLING)

    def test_types_named_from_the_crate_root_are_one_dependency(self):
        items = "use crate::{" + ", ".join(f"Item{i}" for i in range(m.COUPLING * 2)) + "};\n"
        self.assertEqual(found({"crates/a/src/items.rs": items}, "coupling"), [])

    def test_a_re_export_a_test_and_a_string_are_not_uses(self):
        files = {
            "crates/a/src/lib.rs": uses(m.COUPLING - 1)
            + "pub use crate::exported::Thing;\npub(crate) use crate::also::Other;\n"
            + 'const S: &str = "crate::in_a_string";\n'
            + "#[cfg(test)]\nmod tests {\n    use crate::only_in_tests;\n}\n",
        }
        self.assertEqual(found(files, "coupling"), [])

    def test_other_bravebot_crates_count_with_the_modules(self):
        crates = "".join(f"use bravebot_crate_{i}::thing;\n" for i in range(4))
        files = {"crates/a/src/mixed.rs": uses(m.COUPLING - 4) + crates}
        (candidate,) = found(files, "coupling")
        self.assertIn("and 4 other crates", candidate["detail"])


def public_functions(count, prefix="f"):
    return "".join(f"pub fn {prefix}{i}() {{}}\n" for i in range(count))


def calls(count, prefix="f"):
    return "fn caller() {\n" + "".join(f"    a::{prefix}{i}();\n" for i in range(count)) + "}\n"


class Untested(unittest.TestCase):
    def files(self, **extra):
        return {
            "crates/a/src/lib.rs": public_functions(m.UNTESTED),
            "crates/b/src/lib.rs": calls(m.UNTESTED),
            **extra,
        }

    def test_public_functions_called_elsewhere_and_named_in_no_test_are_a_candidate(self):
        (candidate,) = found(self.files(), "untested")
        self.assertEqual((candidate["path"], candidate["rank"]), ("crates/a/src/lib.rs", m.UNTESTED))

    def test_a_function_a_test_names_is_not_counted_whichever_kind_of_test_names_it(self):
        in_tests_directory = self.files(**{"crates/a/tests/it.rs": "fn t() { a::f0(); }\n"})
        in_unit_test = self.files()
        in_unit_test["crates/a/src/lib.rs"] += "#[cfg(test)]\nmod tests {\n    fn t() { f0(); }\n}\n"
        self.assertEqual(found(in_tests_directory, "untested"), [])
        self.assertEqual(found(in_unit_test, "untested"), [])

    def test_a_function_nothing_calls_is_left_to_the_unreferenced_lane(self):
        files = {"crates/a/src/lib.rs": public_functions(m.UNTESTED)}
        self.assertEqual(found(files, "untested"), [])
        self.assertEqual(len(found(files, "unreferenced")), m.UNTESTED)

    def test_a_file_with_fewer_than_the_threshold_is_not_a_candidate(self):
        files = {"crates/a/src/lib.rs": public_functions(m.UNTESTED - 1), "crates/b/src/lib.rs": calls(m.UNTESTED - 1)}
        self.assertEqual(found(files, "untested"), [])


class SharedState(unittest.TestCase):
    marks = (
        "struct S { a: Mutex<u8>, b: Mutex<u8>, c: RwLock<u8>, d: RefCell<u8>, e: AtomicBool, f: AtomicU32 }\n"
        "fn go() { thread::spawn(|| {}); let (t, r) = mpsc::channel(); }\n"
    )

    def test_eight_marks_make_a_candidate_and_name_what_they_are(self):
        (candidate,) = found({"crates/a/src/lib.rs": self.marks}, "shared-state")
        self.assertEqual(candidate["rank"], m.SHARED_STATE)
        self.assertIn("2 Mutex", candidate["detail"])
        self.assertIn("1 thread spawn", candidate["detail"])

    def test_an_import_a_test_and_a_comment_are_not_marks(self):
        seven = self.marks.replace(" let (t, r) = mpsc::channel();", "")
        files = {
            "crates/a/src/lib.rs": "use std::sync::{Mutex, RwLock};\n// a Mutex here\n" + seven
            + "#[cfg(test)]\nmod tests {\n    static M: Mutex<u8> = Mutex::new(0);\n}\n",
        }
        self.assertEqual(found(files, "shared-state"), [])


class Stale(unittest.TestCase):
    def test_two_lines_of_commented_out_code_are_a_block_and_one_line_is_not(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                fn f() {
                    // let old = compute(1);
                    // old.apply();
                    // if old.ok() {
                    //     done();
                    // }
                    let x = 1;
                    // g();
                    let y = 2;
                    // the loop runs until x = 3
                    // so this is a sentence about it
                }
                """
            ),
        }
        (candidate,) = found(files, "stale")
        self.assertIn("commented-out code at 2-6", candidate["detail"])
        self.assertNotIn("8", candidate["detail"].split("at ")[1])

    def test_a_doc_comment_example_is_not_commented_out_code(self):
        files = {"crates/a/src/lib.rs": "/// let a = make();\n/// a.use_it();\nfn f() {}\n"}
        self.assertEqual(found(files, "stale"), [])

    def test_python_and_typescript_blocks(self):
        files = {
            "contrib/tool.py": "# import os\n# value = compute()\n# a sentence about it\nx = 1\n",
            "ui/src/a.ts": "// const a = 1;\n// run(a);\nexport const b = 2;\n",
        }
        self.assertEqual(sorted(c["path"] for c in found(files, "stale")), ["contrib/tool.py", "ui/src/a.ts"])

    def test_a_todo_with_no_issue_is_a_candidate_and_one_citing_an_issue_or_in_a_string_is_not(self):
        files = {
            "crates/a/src/lib.rs": "// TODO: handle the other case\nfn f() {}\n",
            "crates/b/src/lib.rs": "// TODO(#1234): handle the other case\nfn f() {}\n",
            "crates/c/src/lib.rs": 'const LIST: &str = "TODO list";\nfn f() {}\n',
            "crates/d/src/lib.rs": "// FIXME see issues/77\nfn f() {}\n",
        }
        self.assertEqual([c["path"] for c in found(files, "stale")], ["crates/a/src/lib.rs"])


def string_errors(count):
    return "".join(f"fn refuses_{i}() -> Result<(), String> {{ Ok(()) }}\n" for i in range(count))


class ErrorTypes(unittest.TestCase):
    def test_functions_returning_strings_or_boxed_errors_are_named_and_a_typed_error_is_not(self):
        files = {
            "crates/a/src/lib.rs": textwrap.dedent(
                """\
                fn a() -> Result<(), String> { Ok(()) }
                fn b(x: u8) -> Result<Vec<(u8, u8)>, String> { Ok(vec![]) }
                pub fn c() -> Result<u8, Box<dyn std::error::Error + Send + Sync>> { Ok(1) }
                fn d() -> Result<u8, MyError> { Ok(1) }
                fn e() -> Result<u8, &'static str> { Ok(1) }
                fn f() -> Result<String, MyError> { Ok(String::new()) }
                """
            ),
        }
        (candidate,) = found(files, "error-types")
        self.assertEqual(candidate["rank"], 4)
        for name in ("a", "b", "c", "e"):
            self.assertIn(f"`{name}`", candidate["detail"])
        for name in ("d", "f"):
            self.assertNotIn(f"`{name}`", candidate["detail"])

    def test_fewer_than_the_threshold_and_tests_are_not_candidates(self):
        files = {
            "crates/a/src/lib.rs": string_errors(m.STRING_ERRORS - 1),
            "crates/b/src/lib.rs": "fn real() {}\n#[cfg(test)]\nmod tests {\n" + string_errors(m.STRING_ERRORS) + "}\n",
        }
        self.assertEqual(found(files, "error-types"), [])


def diagnostic(lint, path, line, column=1, message="a message", primary=True):
    return json.dumps(
        {
            "reason": "compiler-message",
            "message": {
                "message": message,
                "code": {"code": f"clippy::{lint}"},
                "spans": [{"file_name": path, "line_start": line, "column_start": column, "is_primary": primary}],
            },
        }
    )


class Clippy(unittest.TestCase):
    files = {
        "crates/a/src/lib.rs": "fn a() {}\n" * 5 + "#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n",
        "crates/b/src/lib.rs": "fn b() {}\n" * 12,
    }

    def scanned(self, *lines, files=None):
        with tree(files or self.files) as root:
            result = m.scan(root, ("clippy",), clippy=lambda _: ("\n".join(lines), ""))
        return result["clippy"]["candidates"]

    def test_hits_are_grouped_by_lint_and_file_and_ranked_by_weight_times_count(self):
        candidates = self.scanned(
            diagnostic("let_underscore_must_use", "crates/a/src/lib.rs", 1),
            diagnostic("let_underscore_must_use", "crates/a/src/lib.rs", 3),
            *[diagnostic("wildcard_imports", "crates/b/src/lib.rs", n) for n in range(1, 6)],
            diagnostic("wildcard_imports", "crates/a/src/lib.rs", 2),
        )
        self.assertEqual(
            [(c["lint"], c["path"], c["rank"]) for c in candidates],
            [
                ("let_underscore_must_use", "crates/a/src/lib.rs", 6),
                ("wildcard_imports", "crates/b/src/lib.rs", 5),
                ("wildcard_imports", "crates/a/src/lib.rs", 1),
            ],
        )
        self.assertIn("2 times, at 1, 3", candidates[0]["detail"])

    def test_what_is_not_asked_for_or_not_code_that_ships_is_left_out(self):
        candidates = self.scanned(
            diagnostic("needless_return", "crates/a/src/lib.rs", 1),
            diagnostic("panic", "crates/a/src/lib.rs", 8),
            diagnostic("panic", "target/debug/build/x/out.rs", 1),
            diagnostic("panic", "crates/a/src/lib.rs", 2, primary=False),
            "not json at all",
            json.dumps({"reason": "compiler-artifact"}),
        )
        self.assertEqual(candidates, [])

    def test_a_diagnostic_reported_twice_counts_once(self):
        candidates = self.scanned(*[diagnostic("panic", "crates/b/src/lib.rs", 4, column=9)] * 3)
        self.assertEqual(candidates[0]["rank"], m.CLIPPY_LINTS["panic"])

    def test_a_note_says_why_clippy_was_not_run(self):
        with tree(self.files) as root:
            result = m.scan(root, ("clippy",), clippy=lambda _: ("", "cargo is not installed"))
            with mock.patch.object(m.shutil, "which", return_value=None):
                output, note = m.run_clippy(root)
        self.assertEqual(result["clippy"]["candidates"], [])
        self.assertEqual(result["clippy"]["note"], "cargo is not installed")
        self.assertEqual((output, "not installed" in note), ("", True))
        self.assertIn("note: cargo is not installed", m.report(result))

    def test_it_asks_about_code_that_ships_and_every_lint_it_ranks(self):
        command = m.clippy_command()
        self.assertIn("--lib", command)
        self.assertIn("--bins", command)
        self.assertNotIn("--all-targets", command)
        for lint in m.CLIPPY_LINTS:
            self.assertIn(f"-Wclippy::{lint}", command)

    def test_the_lane_is_left_out_when_asked_and_by_make_check_maintenance(self):
        with tree(self.files) as root, mock.patch.object(m, "run_clippy", side_effect=AssertionError("ran clippy")):
            code, out, _ = quiet(m.main, ["--mechanical-only", "--no-clippy", "--root", str(root)])
        self.assertEqual(code, 0)
        self.assertNotIn("clippy:", out)
        makefile = (m.ROOT / "Makefile").read_text()
        self.assertIn("maintenance.py --mechanical-only --no-clippy", makefile)


class Ledger(unittest.TestCase):
    files = {"crates/a/src/lib.rs": unused(3)}

    def prepared(self, root, **options):
        work = root / ".work"
        code, out, _ = quiet(
            m.main, ["prepare", "unreferenced", "--root", str(root), "--work-dir", str(work), *options.get("args", [])]
        )
        self.assertEqual(code, 0)
        return json.loads(Path(json.loads(out)["manifest"]).read_text()), work

    def judged(self, root, verdicts, **options):
        manifest, work = self.prepared(root, **options)
        (prompt,) = manifest["lanes"][0]["reviewer_prompts"]
        ids = {c["symbol"]: i for i, c in manifest["candidates"].items()}
        Path(prompt["results_file"]).write_text(
            json.dumps({"candidates": [{"id": ids[s], "verdict": v, "reason": f"{s} is {v}"} for s, v in verdicts.items()]})
        )
        code, out, _ = quiet(m.main, ["record", "--work-dir", str(work)])
        self.assertEqual(code, 0)
        return out

    def remaining(self, root, **options):
        return sorted(c["symbol"] for c in m.scan(root, ("unreferenced",), **options)["unreferenced"]["candidates"])

    def test_a_kept_or_drafted_candidate_is_left_out_while_its_file_is_unchanged(self):
        with tree(self.files) as root:
            out = self.judged(root, {"unused_number_0": "keep", "unused_number_1": "cleanup", "unused_number_2": "unclear"})
            result = m.scan(root, ("unreferenced",))["unreferenced"]
            ledger = (root / m.LEDGER).read_text()
        self.assertIn("2 verdicts written", out)
        self.assertEqual([c["symbol"] for c in result["candidates"]], ["unused_number_2"])
        self.assertEqual(result["carried"], 2)
        self.assertIn("unreferenced:crates/a/src/lib.rs::unused_number_0 keep ", ledger)
        self.assertIn("unreferenced:crates/a/src/lib.rs::unused_number_1 cleanup ", ledger)
        self.assertNotIn("unused_number_2", ledger)
        self.assertIn("unused_number_0 is keep", ledger)

    def test_an_edit_to_the_file_or_asking_afresh_offers_the_candidate_again(self):
        with tree(self.files) as root:
            self.judged(root, {f"unused_number_{i}": "keep" for i in range(3)})
            self.assertEqual(self.remaining(root), [])
            self.assertEqual(len(self.remaining(root, fresh=True)), 3)
            (root / "crates/a/src/lib.rs").write_text(unused(3) + "// edited\n")
            self.assertEqual(len(self.remaining(root)), 3)

    def test_a_verdict_is_for_the_item_and_survives_the_line_moving(self):
        with tree(self.files) as root:
            before = m.scan(root, ("unreferenced",))["unreferenced"]["candidates"][0]
            (root / "crates/a/src/lib.rs").write_text("// a line above\n" + unused(3))
            after = m.scan(root, ("unreferenced",))["unreferenced"]["candidates"][0]
        self.assertEqual(before["key"], after["key"])
        self.assertNotEqual(before["digest"], after["digest"])
        self.assertNotEqual(before["line"], after["line"])

    def test_the_cap_reaches_the_next_candidates_once_the_first_are_recorded(self):
        with tree(self.files) as root:
            first = [c["symbol"] for c in m.scan(root, ("unreferenced",), limit=1)["unreferenced"]["candidates"]]
            self.judged(root, {first[0]: "keep"})
            second = [c["symbol"] for c in m.scan(root, ("unreferenced",), limit=1)["unreferenced"]["candidates"]]
        self.assertEqual(len(first + second), 2)
        self.assertNotEqual(first, second)

    def test_recording_drops_entries_for_files_that_changed_and_keeps_the_rest(self):
        files = {"crates/a/src/lib.rs": unused(1), "crates/b/src/lib.rs": "pub fn b_unused() {}\n"}
        with tree(files) as root:
            self.judged(root, {"unused_number_0": "keep", "b_unused": "keep"})
            (root / "crates/b/src/lib.rs").write_text("pub fn b_unused() {}\n// changed\n")
            manifest = {"root": str(root), "candidates": {}}
            m.record(manifest, {})
            ledger = (root / m.LEDGER).read_text()
        self.assertIn("unused_number_0", ledger)
        self.assertNotIn("b_unused", ledger)

    def test_files_that_differ_only_in_whitespace_keep_two_keys_and_two_verdicts(self):
        spaced, joined = "crates/a/src/a b.rs", "crates/a/src/a_b.rs"
        with tree({spaced: "pub fn same() {}\n", joined: "pub fn same() {}\n"}) as root:
            self.assertTrue((root / spaced).exists() and (root / joined).exists())
            manifest, _ = self.prepared(root)
            by_path = {c["path"]: (i, c) for i, c in manifest["candidates"].items()}
            self.assertEqual(len({c["key"] for _, c in by_path.values()}), 2)
            verdicts = {by_path[spaced][0]: {"verdict": "keep"}, by_path[joined][0]: {"verdict": "cleanup"}}
            m.record(manifest, verdicts)
            m.record({"root": str(root), "candidates": {}}, {})
            entries = m.read_ledger(root)
            self.assertEqual(
                sorted((e["verdict"], e["paths"]) for e in entries.values()),
                [("cleanup", [joined]), ("keep", [spaced])],
            )
            self.assertEqual(m.scan(root, ("unreferenced",))["unreferenced"]["candidates"], [])

    def test_unicode_whitespace_and_its_hex_lookalike_keep_two_keys_and_two_verdicts(self):
        wide, lookalike = "crates/a/src/a\u2003b.rs", "crates/a/src/a 03b.rs"
        with tree({wide: "pub fn same() {}\n", lookalike: "pub fn same() {}\n"}) as root:
            manifest, _ = self.prepared(root)
            by_path = {c["path"]: (i, c) for i, c in manifest["candidates"].items()}
            self.assertEqual(len({c["key"] for _, c in by_path.values()}), 2)
            verdicts = {by_path[wide][0]: {"verdict": "keep"}, by_path[lookalike][0]: {"verdict": "cleanup"}}
            m.record(manifest, verdicts)
            m.record({"root": str(root), "candidates": {}}, {})
            entries = m.read_ledger(root)
            self.assertEqual(
                sorted((e["verdict"], e["paths"]) for e in entries.values()),
                [("cleanup", [lookalike]), ("keep", [wide])],
            )

    def test_a_line_that_is_not_a_verdict_is_reported_and_ignored(self):
        with tree(self.files) as root:
            (root / "docs").mkdir()
            (root / m.LEDGER).write_text("# header\n\nbroken line\nkey unclear 2026-01-01 abc crates/a/src/lib.rs\n")
            with contextlib.redirect_stderr(io.StringIO()) as err:
                entries = m.read_ledger(root)
        self.assertEqual(entries, {})
        self.assertEqual(err.getvalue().count("not a verdict line"), 2)

    def test_every_lane_keys_a_candidate_with_no_whitespace_and_no_two_alike(self):
        with tree(every_lane()) as root, mock.patch.object(m, "FILE_LINES", 10):
            result = m.scan(root, m.LANES, clippy=lambda _: (CLIPPY_HIT, ""))
            keys = [c["key"] for entry in result.values() for c in entry["candidates"]]
        self.assertEqual({lane for lane, entry in result.items() if entry["candidates"]}, set(m.LANES))
        self.assertEqual(len(keys), len(set(keys)))
        self.assertFalse([k for k in keys if re.search(r"\s", k)])
        self.assertEqual(sum(k.startswith("allowances:") for k in keys), 2)


CLIPPY_HIT = diagnostic("panic", "crates/a/src/lib.rs", 1)


def every_lane():
    """A tree with at least one candidate in every lane but the one that needs clippy's output."""
    many = ", ".join(f"p{i}" for i in range(m.PARAMETERS))
    literal = 'const A: &str = "a long enough literal here";\n'
    marks = (
        "struct S { a: Mutex<u8>, b: Mutex<u8>, c: RwLock<u8>, d: RefCell<u8>, e: AtomicBool, f: AtomicU32 }\n"
        "fn go() { thread::spawn(|| {}); let (t, r) = mpsc::channel(); }\n"
    )
    return {
        "crates/a/src/lib.rs": "#[allow(dead_code)]\nfn plain() {}\n#[allow(dead_code)]\nfn other() {}\npub fn unused_one() {}\n"
        + nested(m.NESTING) + uses(m.COUPLING) + string_errors(m.STRING_ERRORS) + "// TODO: later\n"
        + "pub fn only_tested() {}\n" + marks + public_functions(m.UNTESTED),
        "crates/a/tests/it.rs": "fn t() { a::only_tested(); }\n",
        "crates/b/src/lib.rs": block("one", 12) + calls(m.UNTESTED),
        "crates/c/src/lib.rs": block("one", 12),
        "crates/d/src/lib.rs": literal * 2 + "fn d() {}\n",
        "crates/e/src/lib.rs": literal + "fn e() {}\n",
        "contrib/tool.py": f"def many({many}):\n    pass\n",
    }


class Prompts(unittest.TestCase):
    def test_every_lane_has_a_prompt_that_states_the_rule_its_question_and_its_notes(self):
        with tree(every_lane()) as root, mock.patch.object(m, "FILE_LINES", 10):
            work = root / ".work"
            result, path = m.prepare(root, m.LANES, (), work, clippy=lambda _: (CLIPPY_HIT, ""))
            manifest = json.loads(Path(path).read_text())
            prompts = {
                entry["lane"]: Path(entry["reviewer_prompts"][0]["prompt_file"]).read_text()
                for entry in manifest["lanes"]
                if entry["reviewer_prompts"]
            }
        self.assertEqual(set(prompts), set(m.LANES))
        for lane, text in prompts.items():
            with self.subTest(lane=lane):
                self.assertIn("never have untrusted content in their context", text)
                self.assertIn(m.QUESTION[lane], text)
                self.assertIn(m.LANE_NOTES[lane], text)
                self.assertIn(f"**{m.PREFIX[lane]}1**", text)

    def test_a_lane_without_a_cap_of_its_own_keeps_thirty_and_the_new_lanes_fifteen(self):
        many = "".join(f"pub fn unused_{i}() {{}}\n" for i in range(40))
        files = {"crates/a/src/lib.rs": many, "crates/b/src/lib.rs": "".join(nested(m.NESTING).replace("deep", f"deep_{i}") for i in range(20))}
        with tree(files) as root:
            result = m.scan(root, ("unreferenced", "complexity"))
            capped = m.scan(root, ("unreferenced", "complexity"), limit=4)
        self.assertEqual((len(result["unreferenced"]["candidates"]), len(result["complexity"]["candidates"])), (30, 15))
        self.assertEqual((len(capped["unreferenced"]["candidates"]), len(capped["complexity"]["candidates"])), (4, 4))


class Reading(unittest.TestCase):
    """Where reviewers read, and what the prompt tells them to do while they do."""

    def prepared(self, root, work=None):
        _, path = m.prepare(root, ("unreferenced",), (), work or root / ".work")
        manifest = json.loads(Path(path).read_text())
        prompt = Path(manifest["lanes"][0]["reviewer_prompts"][0]["prompt_file"]).read_text()
        return manifest, prompt

    def test_reviewers_read_a_checkout_of_the_committed_files_and_not_the_working_tree(self):
        with committed({"crates/a/src/lib.rs": unused(2)}) as root:
            lib = root / "crates/a/src/lib.rs"
            committed_text = lib.read_text()
            lib.write_text(committed_text + "// edited after the commit\n")
            (root / "target").mkdir()
            (root / "target/build.log").write_text("not tracked\n")
            manifest, prompt = self.prepared(root)
            checkout = Path(manifest["tree"])
            read = (checkout / "crates/a/src/lib.rs").read_text()
            holds_untracked = (checkout / "target").exists()
            expected = head(root)
        self.assertEqual(manifest["commit"], expected)
        self.assertEqual(read, committed_text)
        self.assertFalse(holds_untracked)
        self.assertIn(f"`{checkout}`", prompt)
        self.assertIn(expected, prompt)
        self.assertIn(f"Do not search `{root}`", prompt)
        self.assertNotIn("your working directory", prompt)

    def test_a_repository_without_a_commit_is_read_in_place(self):
        with tree({"crates/a/src/lib.rs": unused(2)}) as root:
            manifest, prompt = self.prepared(root)
            checkout_made = (root / ".work/tree").exists()
        self.assertIsNone(manifest["commit"])
        self.assertIsNone(manifest["tree"])
        self.assertFalse(checkout_made)
        self.assertIn(f"The code is in `{root}`, your working directory", prompt)
        self.assertIn("no commit to check out", "\n".join(manifest["progress_lines"]))

    def test_uncommitted_changes_to_tracked_files_are_reported_and_a_clean_tree_is_not(self):
        with committed({"crates/a/src/lib.rs": unused(2)}) as root:
            clean, _ = self.prepared(root, root / ".clean")
            (root / "crates/a/src/lib.rs").write_text(unused(3))
            dirty, _ = self.prepared(root, root / ".dirty")
        self.assertFalse(clean["dirty"])
        self.assertTrue(dirty["dirty"])
        self.assertNotIn("uncommitted", "\n".join(clean["progress_lines"]))
        self.assertIn("uncommitted changes", "\n".join(dirty["progress_lines"]))

    def test_the_prompt_tells_a_reviewer_how_to_finish(self):
        with committed({"crates/a/src/lib.rs": unused(2)}) as root:
            _, prompt = self.prepared(root)
        for rule in (
            "Start no background command",
            "stop it before you answer",
            "Do not run `gh`",
            "answer with one line",
        ):
            with self.subTest(rule=rule):
                self.assertIn(rule, prompt)

    def test_preparing_again_into_the_same_work_directory_makes_a_new_checkout(self):
        with committed({"crates/a/src/lib.rs": unused(2), ".gitignore": ".work\n"}) as root:
            self.prepared(root)
            (root / "crates/a/src/lib.rs").write_text(unused(3))
            commit_all(root, "second")
            manifest, _ = self.prepared(root)
            text = (Path(manifest["tree"]) / "crates/a/src/lib.rs").read_text()
        self.assertEqual(text, unused(3))

    def test_clean_removes_the_checkout_and_the_repository_forgets_it(self):
        with committed({"crates/a/src/lib.rs": unused(2)}) as root:
            manifest, _ = self.prepared(root)
            checkout = Path(manifest["tree"])
            existed = checkout.exists()
            code, _, _ = quiet(m.main, ["clean", "--work-dir", str(root / ".work")])
            again, _, _ = quiet(m.main, ["clean", "--work-dir", str(root / ".work")])
            listed = subprocess.run(
                ["git", "-C", str(root), "worktree", "list"], check=True, capture_output=True, text=True
            ).stdout
        self.assertTrue(existed)
        self.assertEqual((code, again), (0, 0))
        self.assertFalse(checkout.exists())
        self.assertNotIn(str(checkout), listed)

    def test_clean_removes_only_the_checkout_in_its_own_work_directory(self):
        with committed({"crates/a/src/lib.rs": unused(2)}) as root:
            work = root / ".work"
            manifest, _ = self.prepared(root, work)
            elsewhere = root / "other-checkout"
            subprocess.run(
                ["git", "-C", str(root), "worktree", "add", "-q", "--detach", str(elsewhere), "HEAD"], check=True
            )
            (elsewhere / "note.txt").write_text("keep\n")
            manifest["tree"] = str(elsewhere)
            workdir_manifest = work / "manifest.json"
            workdir_manifest.write_text(json.dumps(manifest))
            code, out, _ = quiet(m.main, ["clean", "--work-dir", str(work)])
            kept = (elsewhere / "note.txt").exists()
        self.assertEqual(code, 1)
        self.assertIn("did not remove", out)
        self.assertTrue(kept)


if __name__ == "__main__":
    unittest.main()
