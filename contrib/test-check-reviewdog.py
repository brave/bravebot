#!/usr/bin/env python3
"""A failed security scanner must not be reported as a clean scan."""

import os
from pathlib import Path
import shutil
import subprocess
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
        (assets / "reviewdog/reviewdog.yml").write_text(
            'runner:\n  opengrep:\n    cmd: "opengrep | cat"\n'
        )
        self.executable("opengrep", '''#!/bin/sh
if [ "$1" = --version ]; then echo 1.11.5; exit; fi
case "$SCENARIO" in
  pipeline_failure) exit 7 ;;
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
command = YAML.load_file(config).fetch('runner').fetch('opengrep').fetch('cmd')
exit(system(command) ? 0 : 1)
''')
        real_git = shutil.which("git")
        self.executable("git", f'''#!/bin/sh
if [ "$1" = -C ]; then exit 0; fi
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

    def scan(self, scenario, full):
        return subprocess.run(
            ["bash", str(SCRIPT), "--runners", "opengrep"] + (["--full"] if full else []),
            cwd=self.repo, env=dict(self.env, SCENARIO=scenario),
            capture_output=True, text=True, timeout=20,
        )

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
                             "pipeline_failure", "findings"):
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
