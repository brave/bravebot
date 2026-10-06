#!/usr/bin/env python3
"""Check that the aggregate reaches its gates and preserves their failures."""

import io
import os
from pathlib import Path
import shutil
import sys
import tarfile
# These tests run fixed local commands with argument lists.
import subprocess  # nosemgrep: gitlab.bandit.B404
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
GATES = (
    "check-scripts", "check", "check-spec", "check-security", "check-locales", "check-versions",
    "check-narration", "check-docs", "check-npm", "check-deps", "check-msrv", "check-windows",
    "check-linux", "check-ui", "check-agent-client", "check-reviewdog",
)


class CheckTargets(unittest.TestCase):
    """Replace expensive tools, but execute the repository's actual Make recipes."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        shutil.copy(ROOT / "Makefile", self.root)
        shutil.copy(ROOT / "Cargo.toml", self.root)
        (self.root / "ui/scripts").mkdir(parents=True)
        (self.root / "ui/scripts/fixture.test.mjs").touch()
        (self.root / "extension/tests").mkdir(parents=True)
        (self.root / "extension/tests/fixture.test.mjs").touch()
        (self.root / "packages/agent-client/test-fixtures/scenarios").mkdir(parents=True)
        (self.root / "packages/agent-client/test-fixtures/scenarios/fixture.json").touch()
        (self.root / "npm/tests").mkdir(parents=True)
        (self.root / "npm/tests/fixture.test.mjs").touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls"
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        CALL_LOG=str(self.log), FAIL_COMMAND="", TEST_PLATFORM="Darwin")
        for name in ("pnpm", "node", "xvfb-run", "python3", "cargo"):
            tool = self.bin / name
            tool.write_text('#!/bin/sh\ncommand="$(basename "$0") $*"\n'
                            'printf "%s\\n" "$command" >> "$CALL_LOG"\n'
                            'if [ "$command" = "pnpm --dir ui run build" ]; then\n'
                            '  [ "$BRAVEBOT_BUILD_UNCONFIGURED" = 1 ] || exit 9\nfi\n'
                            '[ "$command" != "$FAIL_COMMAND" ]\n')
            tool.chmod(0o755)
        platform = self.bin / "uname"
        platform.write_text('#!/bin/sh\nprintf "%s\\n" "$TEST_PLATFORM"\n')
        platform.chmod(0o755)

    def run_make(self, *args, **env):
        self.log.write_text("")
        return subprocess.run(["make", "--no-print-directory", *args], cwd=self.root,
                              env=dict(self.env, **env), capture_output=True, text=True, timeout=20)

    def test_check_profiles_run_only_their_promised_gates_and_preserve_failures(self):
        """The local suite must cover host checks without invoking Docker."""
        gates = set(GATES)
        local = gates - {"check-msrv", "check-windows", "check-linux"}
        overrides = self.root / "profiles.mk"
        # Empty these inherited prerequisites so only the selected gates are recorded.
        overrides.write_text("\n".join(
            f'{gate}:\n\t@echo {gate} >> "$$CALL_LOG"\n\t@test "$$FAIL_COMMAND" != {gate}\n'
            for gate in sorted(gates)
        ) + '\ncheck-ui-build check-extension check-all-selftest check-reviewdog-selftest'
          ' check-rebase-selftest'
          ' check-affected-selftest check-peer-advisories-selftest check-peer-features-selftest check-pr-fix-selftest:\n\t@true\n')
        for target, expected in (("check-all-local", local), ("check-all", gates)):
            for failing in ("", *sorted(expected)):
                with self.subTest(target=target, failing=failing):
                    result = self.run_make("-k", "-f", "Makefile", "-f", "profiles.mk", target,
                                           FAIL_COMMAND=failing)
                    self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                    self.assertCountEqual(self.log.read_text().splitlines(), expected)

    def test_script_checks_run_every_selftest_and_preserve_failures(self):
        """The CI entry point must cover every suite and fail when any fails."""
        commands = ["python3 contrib/check-all-selftest.py",
                    "python3 contrib/check-reviewdog-selftest.py",
                    "python3 agents/skills/rebase/selftest.py",
                    "python3 contrib/affected-checks.py --selftest",
                    "python3 agents/skills/peer-advisories/selftest.py",
                    "python3 agents/skills/peer-features/selftest.py",
                    "python3 agents/skills/pr-fix/selftest.py"]
        for failing in ("", *commands):
            with self.subTest(failing=failing):
                result = self.run_make("-k", "check-scripts", FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                self.assertCountEqual(self.log.read_text().splitlines(), commands)

    def affected_classifier(self):
        """A python3 that answers for the classifier with $AFFECTED and logs like the others."""
        python = self.bin / "python3"
        python.write_text('#!/bin/sh\ncommand="python3 $*"\n'
                          'printf "%s\\n" "$command" >> "$CALL_LOG"\n'
                          'case "$1 $2" in "contrib/affected-checks.py --containers") '
                          'printf "%s\\n" "$AFFECTED_CONTAINERS";;\n'
                          '  contrib/affected-checks.py*) printf "%s\\n" "$AFFECTED";; esac\n'
                          '[ "$command" != "$FAIL_COMMAND" ]\n')
        python.chmod(0o755)

    def test_affected_runs_only_what_the_classifier_names_and_fails_with_it(self):
        """A classifier that fails or names nothing must fail the gate, not run make's default
        goal, and a gate it names must fail the target when it fails, without stopping the rest."""
        self.affected_classifier()
        classify = "python3 contrib/affected-checks.py"
        locales = ["python3 contrib/check-locales.py --selftest", "python3 contrib/check-locales.py"]
        versions = ["python3 contrib/check-versions.py --selftest", "python3 contrib/check-versions.py"]
        for affected, failing, expected in (
            ("check-locales", "", [classify, *locales]),
            ("check-locales", locales[0], [classify, locales[0]]),
            ("check-locales check-versions", locales[0], [classify, locales[0], *versions]),
            ("check-locales", classify, [classify]),
            ("", "", [classify]),
        ):
            with self.subTest(affected=affected, failing=failing):
                result = self.run_make("check-affected", AFFECTED=affected, FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing) or not affected,
                                 result.stdout + result.stderr)
                self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_affected_containers_run_none_when_none_are_named(self):
        """No container gate needed is a pass, and a failed classifier is not."""
        self.affected_classifier()
        classify = "python3 contrib/affected-checks.py --containers"
        for failing in ("", classify):
            with self.subTest(failing=failing):
                result = self.run_make("check-affected-containers", AFFECTED_CONTAINERS="",
                                       FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                self.assertEqual(self.log.read_text().splitlines(), [classify])

    def test_scan_targets_run_only_the_scanner_selftest_and_stop_if_it_fails(self):
        """A scan needs its own selftest, and must not run after that selftest fails."""
        scanner = self.root / "contrib/check-reviewdog.sh"
        scanner.parent.mkdir()
        scanner.write_text('#!/bin/sh\nprintf "scan %s\\n" "$*" >> "$CALL_LOG"\n')
        scanner.chmod(0o755)
        command = "python3 contrib/check-reviewdog-selftest.py"
        for target, arguments, base in (("check-reviewdog", "", ""),
                                        ("check-reviewdog", "--base v1.0", "v1.0"),
                                        ("check-reviewdog-full", "--full", "")):
            for failing in ("", command):
                with self.subTest(target=target, base=base, failing=failing):
                    result = self.run_make(target, FAIL_COMMAND=failing, BASE=base)
                    self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                    expected = [command] if failing else [command, f"scan {arguments}"]
                    self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_ui_stops_at_each_failed_build_stage_and_reports_walkthrough_failures(self):
        """An app that cannot build or complete its walkthrough must fail the UI gate, and so must
        the extension's tests, which the UI gate runs first."""
        commands = ["node --test extension/tests/fixture.test.mjs",
                    "pnpm --dir ui install --frozen-lockfile", "pnpm --dir ui run typecheck",
                    "pnpm --dir ui run build", "node --test scripts/fixture.test.mjs",
                    "node scripts/drive-manual-walkthrough.mjs"]
        for failing in ("", *commands):
            with self.subTest(failing=failing):
                result = self.run_make("check-ui", FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                expected = commands[:commands.index(failing) + 1] if failing else commands
                self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_the_client_gate_builds_the_bridge_before_its_tests_and_preserves_each_failure(self):
        """The client's tests drive a built bravebot-rpc and never build it, so a bridge that
        fails to build must stop the gate before they run."""
        commands = ["cargo build -p bravebot-ui-bridge --bin bravebot-rpc",
                    "pnpm --dir packages/agent-client install --frozen-lockfile --ignore-scripts",
                    "pnpm --dir packages/agent-client run check"]
        for failing in ("", *commands):
            with self.subTest(failing=failing):
                result = self.run_make("check-agent-client", FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                expected = commands[:commands.index(failing) + 1] if failing else commands
                self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_client_scenarios_that_are_no_longer_there_fail_the_gate(self):
        """A scenario directory emptied or renamed must not leave the gate green on nothing."""
        (self.root / "packages/agent-client/test-fixtures/scenarios/fixture.json").unlink()
        result = self.run_make("check-agent-client")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.log.read_text().splitlines(), [])

    def test_extension_tests_that_are_no_longer_there_fail_the_gate(self):
        """`node --test` given a pattern matching nothing passes having run nothing."""
        (self.root / "extension/tests/fixture.test.mjs").unlink()
        result = self.run_make("check-extension")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.log.read_text().splitlines(), [])

    def test_linux_walkthrough_uses_a_virtual_display_and_preserves_its_failure(self):
        """The headless CI route must not hide an Electron failure."""
        command = "xvfb-run -a node scripts/drive-manual-walkthrough.mjs"
        for failing in ("", command):
            with self.subTest(failing=failing):
                result = self.run_make("check-ui-walkthrough", TEST_PLATFORM="Linux", FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                self.assertEqual(self.log.read_text().splitlines(), [command])

    def test_the_installer_test_precedes_the_install_and_each_failure_stops_the_gate(self):
        """The website and UI lockfile installs need the same gate as the wrapper's, and the
        installer test must run before anything installs a dependency for it to depend on."""
        commands = ["node --test npm/tests/fixture.test.mjs", "pnpm install --frozen-lockfile --ignore-scripts",
                    "pnpm --dir docs/website install --frozen-lockfile --ignore-scripts",
                    "pnpm --dir ui install --frozen-lockfile --ignore-scripts",
                    "pnpm --dir packages/agent-client install --frozen-lockfile --ignore-scripts"]
        for failing in ("", *commands):
            with self.subTest(failing=failing):
                result = self.run_make("check-npm", FAIL_COMMAND=failing)
                self.assertEqual(result.returncode != 0, bool(failing), result.stderr)
                expected = commands[:commands.index(failing) + 1] if failing else commands
                self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_an_installer_test_that_is_no_longer_there_fails_the_gate(self):
        """`node --test` given a pattern matching nothing exits 0 having run nothing, so a pin
        renamed out of npm/tests would leave the gate green with the clause unheld."""
        (self.root / "npm/tests/fixture.test.mjs").unlink()
        result = self.run_make("check-npm")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(self.log.read_text().splitlines(), [])

    def test_check_build_discards_baked_credentials_without_changing_development_builds(self):
        """The walkthrough's unconfigured-service case must not call a developer's account."""
        script = self.root / "ui/scripts/build-bridge.sh"
        shutil.copy(ROOT / "ui/scripts/build-bridge.sh", script)
        (self.root / ".envrc").touch()
        cargo = self.bin / "cargo"
        cargo.write_text('#!/bin/sh\n'
                         'if [ "$BRAVEBOT_BUILD_UNCONFIGURED" = 1 ]; then\n'
                         '  [ -z "$SERVICES_KEY_AICHAT$BRAVE_SERVICES_KEY_ID$BRAVE_AI_CHAT_ENDPOINT$BRAVE_AI_CHAT_PREMIUM_ENDPOINT$BRAVEBOT_DEFAULT_MODEL" ] || exit 7\n'
                         '  [ "$BRAVEBOT_ALLOW_UNCONFIGURED_BUILD" = 1 ]\n'
                         'else\n  [ "$SERVICES_KEY_AICHAT" = fixture-key ]\nfi\n')
        cargo.chmod(0o755)
        direnv = self.bin / "direnv"
        direnv.write_text('#!/bin/sh\nexit 8\n')
        direnv.chmod(0o755)
        for unconfigured in ("0", "1"):
            with self.subTest(unconfigured=unconfigured):
                result = subprocess.run(["bash", str(script)], cwd=self.root,
                    env=dict(self.env, BRAVEBOT_BUILD_UNCONFIGURED=unconfigured,
                             SERVICES_KEY_AICHAT="fixture-key", BRAVE_SERVICES_KEY_ID="fixture-id",
                             BRAVE_AI_CHAT_ENDPOINT="https://example.invalid",
                             BRAVE_AI_CHAT_PREMIUM_ENDPOINT="https://example.invalid",
                             BRAVEBOT_DEFAULT_MODEL="fixture-model"),
                    capture_output=True, text=True, timeout=20)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_container_source_includes_edits_but_excludes_ignored_local_state(self):
        """Containers need current source, not build output, credentials or other worktrees."""
        source = self.root / "source"
        source.mkdir()
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        (source / ".gitignore").write_text("target/\n.worktrees/\n.envrc\n")
        (source / "edited.rs").write_text("old\n")
        (source / "deleted.rs").touch()
        subprocess.run(["git", "-C", str(source), "add", "."], check=True)
        (source / "edited.rs").write_text("current\n")
        (source / "deleted.rs").unlink()
        (source / "new.rs").write_text("new\n")
        (source / "staged.rs").write_text("staged\n")
        subprocess.run(["git", "-C", str(source), "add", "staged.rs"], check=True)
        for name in ("target/output", ".worktrees/other/Makefile", ".envrc"):
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("excluded\n")
        result = subprocess.run([sys.executable, str(ROOT / "contrib/check-source.py")],
                                cwd=source, capture_output=True, check=True)
        with tarfile.open(fileobj=io.BytesIO(result.stdout)) as archive:
            self.assertEqual(set(archive.getnames()), {".gitignore", "edited.rs", "new.rs", "staged.rs"})
            self.assertEqual(archive.extractfile("edited.rs").read(), b"current\n")

    def test_container_checks_preserve_source_archive_failures(self):
        """A container must not pass when its source could not be copied."""
        docker = self.bin / "docker"
        docker.write_text('#!/bin/sh\ncat >/dev/null\nexit 0\n')
        docker.chmod(0o755)
        for target in ("check-msrv", "check-windows", "check-linux"):
            for failing in ("", "python3 contrib/check-source.py"):
                with self.subTest(target=target, failing=failing):
                    result = self.run_make(target, FAIL_COMMAND=failing)
                    self.assertEqual(result.returncode != 0, bool(failing), result.stderr)


if __name__ == "__main__":
    unittest.main()
