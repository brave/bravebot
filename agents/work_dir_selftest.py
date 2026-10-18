#!/usr/bin/env python3
"""Selftest for work_dir.py, which the skill scripts use to read and write their manifest.json."""

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("work_dir", HERE / "work_dir.py")
work_dir = importlib.util.module_from_spec(spec)
spec.loader.exec_module(work_dir)


class Manifest(unittest.TestCase):
    def test_a_written_manifest_is_read_back_and_lives_in_manifest_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = {"lanes": [], "nested": {"é": [1, 2]}}
            path = work_dir.write_manifest(tmp, manifest)
            self.assertEqual(path, Path(tmp) / "manifest.json")
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")), manifest)
            self.assertEqual(work_dir.read_manifest(tmp), manifest)

    def test_a_manifest_is_written_indented_by_two_without_a_trailing_newline(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = work_dir.write_manifest(Path(tmp), {"a": [1]})
            self.assertEqual(path.read_text(encoding="utf-8"), '{\n  "a": [\n    1\n  ]\n}')

    def test_a_missing_manifest_reads_as_none_and_names_the_path_on_stderr(self):
        with tempfile.TemporaryDirectory() as tmp:
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                found = work_dir.read_manifest(tmp)
            self.assertIsNone(found)
            self.assertEqual(err.getvalue(), f"no manifest at {Path(tmp) / 'manifest.json'}\n")

    def test_a_second_write_replaces_the_first(self):
        with tempfile.TemporaryDirectory() as tmp:
            work_dir.write_manifest(tmp, {"first": True, "extra": 1})
            work_dir.write_manifest(tmp, {"first": False})
            self.assertEqual(work_dir.read_manifest(tmp), {"first": False})

    def test_the_path_accepts_a_string_or_a_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(work_dir.manifest_path(tmp), work_dir.manifest_path(Path(tmp)))


if __name__ == "__main__":
    unittest.main()
