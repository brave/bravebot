#!/usr/bin/env python3
"""A failed security scanner must not be reported as a clean scan."""

import json
import os
from pathlib import Path
import shutil
# The fixture runs fixed local commands with argument lists, never shell input.
import subprocess  # nosemgrep: gitlab.bandit.B404
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("check-reviewdog.sh").resolve()


class ScanResults(unittest.TestCase):
    """Exercise the scan entry point without network access or real scanners."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.env = dict(os.environ, XDG_CACHE_HOME=str(self.root / "cache"))
        self.env.pop("GITHUB_BASE_REF", None)
        for args in (["init", "-q"], ["config", "user.email", "test@example.com"],
                     ["config", "user.name", "Test"], ["config", "commit.gpgsign", "false"]):
            self.git(*args)
        (self.repo / "sample.rs").write_text("// base\n")
        self.git("add", ".")
        self.git("commit", "-qm", "base")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        (self.repo / "sample.rs").write_text("// changed\n")
        self.git("commit", "-qam", "change")
        cache = self.root / "cache/bravebot-reviewdog"
        self.bin = cache / "bin"
        self.bin.mkdir(parents=True)
        assets = cache / "security-action/assets"
        (cache / "security-action/.git").mkdir(parents=True)
        (assets / "reviewdog").mkdir(parents=True)
        # Execute a real pipeline so a scanner failure masked by a formatter is covered.
        self.config = assets / "reviewdog/reviewdog.yml"
        self.config.write_text(
            'runner:\n  opengrep:\n    cmd: "opengrep | cat"\n'
        )
        self.executable("opengrep", '''#!/bin/sh
if [ "$1" = --version ]; then echo 1.11.5; exit; fi
case "$SCENARIO" in
  pipeline_failure|filtered_failure) exit 7 ;;
  findings) echo 'sample.rs:1: a finding without a bracket prefix' ;;
esac
''')
        self.executable("reviewdog", '''#!/usr/bin/env ruby
require 'yaml'
if ARGV.include?('-version'); puts '0.17.5'; exit; end
case ENV.fetch('SCENARIO')
when 'exit_failure'; exit 2
when 'stderr_failure'; File.write('reviewdog.opengrep.stderr.log', 'scanner failed'); exit
when 'reported_failure'; warn 'failed with zero findings: The command itself failed'; exit
end
config = ARGV.find { |arg| arg.start_with?('-conf=') }.split('=', 2)[1]
runners = YAML.load_file(config).fetch('runner')
names = ARGV.find { |arg| arg.start_with?('-runners=') }.split('=', 2)[1].split(',')
failed = false
names.each do |name|
  command = runners.fetch(name).fetch('cmd')
  if ENV['SCENARIO'] == 'filtered_failure'
    system(command, out: File::NULL)
  else
    failed = true unless system(command)
  end
end
exit(failed ? 1 : 0)
''')
        real_git = shutil.which("git")
        self.executable("git", f'''#!/bin/sh
if [ "$1" = -C ] && [ "$2" != "$PWD" ]; then exit 0; fi
exec "{real_git}" "$@"
''')
        self.env["PATH"] = str(self.bin) + os.pathsep + self.env["PATH"]

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.repo, env=self.env,
                       check=True, capture_output=True)

    def executable(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def scan(self, scenario, full, runners="opengrep"):
        return subprocess.run(
            ["bash", str(SCRIPT)] + (["--runners", runners] if runners else [])
            + (["--full"] if full else []),
            cwd=self.repo, env=dict(self.env, SCENARIO=scenario),
            capture_output=True, text=True, timeout=20,
        )

    def test_scan_identifies_the_branch_or_detached_commit(self):
        """The scan output must identify the checkout being checked."""
        self.git("checkout", "-qb", "scan-output-example")
        result = self.scan("clean", False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("scanning the `scan-output-example` branch against origin/main", result.stderr)
        self.git("checkout", "--detach")
        commit = subprocess.check_output(["git", "rev-parse", "--short", "HEAD"],
                                         cwd=self.repo, env=self.env, text=True).strip()
        result = self.scan("clean", False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"scanning detached HEAD ({commit}) against origin/main", result.stderr)

    def test_unchanged_or_deleted_script_files_do_not_start_the_script_scanner(self):
        """No script input should not trigger a walk through ignored build directories."""
        page = self.repo / "page.html"
        page.write_text("<script>example()</script>\n")
        self.git("add", "page.html")
        self.git("commit", "-qm", "page")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        (self.repo / "sample.rs").write_text("// new Rust change\n")
        self.config.write_text('runner:\n  opengrep:\n    cmd: "true"\n'
                               '  sveltegrep:\n    cmd: "echo script-scanner-ran"\n')
        for deleted in (False, True):
            with self.subTest(deleted=deleted):
                if deleted:
                    page.unlink()
                result = self.scan("clean", False, "")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertNotIn("script-scanner-ran", result.stdout)

    def test_script_changes_and_full_scans_keep_the_script_scanner(self):
        """Selecting by changed files must retain dirty, staged, committed and full coverage."""
        self.config.write_text('runner:\n  opengrep:\n    cmd: "true"\n'
                               '  sveltegrep:\n    cmd: "echo script-scanner-ran"\n')
        for suffix in ("html", "svelte"):
            page = self.repo / f"page.{suffix}"
            page.write_text("<script>example()</script>\n")
            self.git("add", str(page))
            for state in ("staged", "committed", "dirty", "full"):
                with self.subTest(suffix=suffix, state=state):
                    if state == "committed":
                        self.git("commit", "-qm", "page")
                    elif state == "dirty":
                        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
                        page.write_text("<script>changed()</script>\n")
                    elif state == "full":
                        self.git("checkout", "--", str(page))
                    result = self.scan("clean", state == "full", "")
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("script-scanner-ran", result.stdout)

    @unittest.skipUnless(os.environ.get("REVIEWDOG_TEST_BINARY"), "set REVIEWDOG_TEST_BINARY for real reviewdog")
    def test_real_reviewdog_cannot_hide_a_failed_runner_behind_filtered_findings(self):
        """Reviewdog can exit zero after filtering every finding from a failed runner."""
        shutil.copy2(os.environ["REVIEWDOG_TEST_BINARY"], self.bin / "reviewdog")
        for full in (False, True):
            for command, failed in (("true", False), ("exit 7", True),
                                    ("printf 'sample.rs:1: finding\\n'", True),
                                    ("printf 'other.rs:99: finding\\n'; exit 7", True)):
                with self.subTest(full=full, command=command):
                    self.config.write_text("runner:\n  opengrep:\n    cmd: " + json.dumps(command)
                                           + '\n    errorformat:\n      - "%f:%l: %m"\n')
                    result = self.scan("clean", full)
                    self.assertEqual(result.returncode != 0, failed, result.stdout + result.stderr)

    def test_full_scan_does_not_inherit_a_branch_baseline(self):
        """A CI environment must not silently narrow a requested full scan."""
        self.env["GITHUB_BASE_REF"] = "unrelated"
        self.executable("opengrep", '#!/bin/sh\nif [ "$1" = --version ]; then echo 1.11.5; exit; fi\n'
                        'if [ "${GITHUB_BASE_REF+set}" ]; then exit 7; fi\n')
        result = self.scan("clean", True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_failed_file_list_cannot_produce_a_clean_scan(self):
        """A missing input list can hide every file from a scanner."""
        real_git = shutil.which("git")
        self.executable("git", f'#!/bin/sh\nif [ "$1" = -C ]; then exit 0; fi\n'
                        'if [ "$1" = ls-files ] || [ "$1" = diff -a "$2" = --name-only ]; then exit 7; fi\n'
                        f'exec "{real_git}" "$@"\n')
        for full in (False, True):
            with self.subTest(full=full):
                result = self.scan("clean", full)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_later_success_cannot_clear_an_earlier_runner_failure(self):
        """Every selected runner must complete, even when the last one passes."""
        self.config.write_text('runner:\n  opengrep:\n    cmd: "exit 7"\n'
                               '  npm-audit:\n    cmd: "true"\n')
        for full in (False, True):
            with self.subTest(full=full):
                result = self.scan("clean", full, "opengrep,npm-audit")
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("security scan failed", result.stderr)
                self.assertFalse(list(self.repo.glob("reviewdog.*.stderr.log")))
                refs = subprocess.check_output(["git", "for-each-ref", "refs/remotes/origin/__check_reviewdog_base"],
                                               cwd=self.repo, env=self.env, text=True)
                self.assertEqual(refs, "")

    def test_an_unwritable_baseline_cannot_produce_a_clean_scan(self):
        """A stale baseline can filter out findings that belong to this branch."""
        real_git = shutil.which("git")
        self.executable("git", f'#!/bin/sh\nif [ "$1" = -C ]; then exit 0; fi\n'
                        'if [ "$1" = update-ref ] && [ "$2" != -d ]; then exit 7; fi\n'
                        f'exec "{real_git}" "$@"\n')
        result = self.scan("clean", False)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("cannot set scanner baseline", result.stderr)

    def test_committed_changes_keep_the_ci_baseline(self):
        """A clean branch should use the same baseline filtering as the PR scan."""
        self.executable("opengrep", '#!/bin/sh\nif [ "$1" = --version ]; then echo 1.11.5; exit; fi\n'
                        '[ "$GITHUB_BASE_REF" = __check_reviewdog_base ]\n')
        result = self.scan("clean", False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_dirty_files_do_not_use_the_commit_only_scanner_baseline(self):
        """Opengrep baseline targets omit uncommitted changes, even in tracked files."""
        self.executable("opengrep", '#!/bin/sh\nif [ "$1" = --version ]; then echo 1.11.5; exit; fi\n'
                        'if [ "${GITHUB_BASE_REF+set}" ]; then exit 7; fi\n')
        for staged in (False, True):
            with self.subTest(staged=staged):
                (self.repo / "sample.rs").write_text("// dirty file\n")
                if staged:
                    self.git("add", "sample.rs")
                result = self.scan("clean", False)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_uncommitted_changes_at_the_base_are_scanned(self):
        """A new branch can need scanning before its first commit."""
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        (self.repo / "sample.rs").write_text("// uncommitted change\n")
        result = self.scan("findings", False)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("a finding without a bracket prefix", result.stdout)

    def test_only_completed_empty_scans_pass(self):
        """Both entry points reject crashes, masked pipeline failures and findings."""
        for full in (False, True):
            for scenario in ("clean", "exit_failure", "stderr_failure", "reported_failure",
                             "pipeline_failure", "filtered_failure", "findings"):
                with self.subTest(full=full, scenario=scenario):
                    result = self.scan(scenario, full)
                    output = result.stdout + result.stderr
                    if scenario == "clean":
                        self.assertEqual(result.returncode, 0, output)
                        self.assertIn("no findings", output)
                    else:
                        self.assertNotEqual(result.returncode, 0, output)
                        self.assertNotIn("no findings", output)
                        if scenario == "findings":
                            self.assertIn("a finding without a bracket prefix", output)
                        else:
                            self.assertIn("security scan failed", output)


if __name__ == "__main__":
    unittest.main()
