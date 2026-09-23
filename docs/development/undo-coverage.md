# Testing undo coverage

Undo must leave file trust consistent with the bytes left on disk. The governing contracts are
[SESSION-2](../specs/sessions.md#SESSION-2) for saved backup bytes,
[SESSION-19](../specs/sessions.md#SESSION-19) for checkpoint eligibility and restoration, and
[TRUST-19](../specs/trust-map.md#TRUST-19) for scratch files outside the backup journal. Their `verified-by` entries name
the tests that pin each requirement. This guide explains how to check the boundaries between
the engine, terminal UI and session store.

## Regression cases

The main integration tests are in [undo_tests.rs](../../crates/tui/src/undo_tests.rs). They use
real engine turns, file writes, session records and captured planner requests. Keep the fixtures'
file contents, prefix grants, child refusals, program arguments and turn costs distinct so that
restoring the wrong state fails an assertion.

| Case | What the test must distinguish |
|---|---|
| Original larger than the 32 MiB backup budget | The replacement remains on disk, undo reports incomplete restoration, grants are withdrawn, and the next planner request excludes the untrusted sentinel. |
| Failed or cancelled turn followed by undo | Live and resumed undo produce the same safe result. Cancellation must be observed after the write, not inferred from elapsed time. |
| Complete restore and deterministic restore failure | A directory blocking one restoration must not prevent another path from restoring. Check exact trust rules, programs, history and accounting in both cases. |
| Multiple checkpoints and repeated undo | Each path uses its earliest selected backup. Incomplete restoration removes older checkpoints so another undo cannot revive their grants. |
| Terminal to bridge to terminal | Save real terminal checkpoints, execute a bridge turn, save again, then resume in the terminal. Imported checkpoints must be absent even when the bridge write has no backup entry. |
| Full-record and mid-history forks | Both keep current file decisions and discard checkpoints without changing the source record or rewinding disk. |
| Programs and configured hooks | Existing checkpoints become invalid before untracked effects. Cover foreground redirection, a live background job, all three hook moments and a nonmatching-hook control. Check storage before UI pruning too. |

Additional tests cover these boundaries:

- [Session storage](../../crates/session/src/sessions.rs): missing, malformed or unknown coverage
  versions disable checkpoints while current state still loads. A round trip that loses the marker
  must not restore eligibility. A required path with no backup payload becomes `NotKept`, never
  an absent original. Capture provenance and pre-turn trust both gate serialized backup bytes.
- [Workspace writes](../../crates/agent/src/workspace.rs) and their
  [integration tests](../../crates/agent/tests/workspace.rs): first capture survives repeated and
  same-label writes and partial failures. Scratch writes and a failed backup lock invalidate
  coverage. The [turn tests](../../crates/agent/tests/turn.rs) check overlapping delegate writes
  in both collection orders.
- [Language servers](../../crates/agent/tests/lsp.rs): a real approved server invalidates old
  checkpoints and prevents new coverage across turns and after server shutdown. Build-tool
  children can outlive the server, so coverage stays disabled for that workspace. A declined
  launch starts no process and preserves coverage.

## Running and assessing the tests

Focused entry points are:

```sh
cargo test -p bravebot-tui --lib undo_tests
cargo test -p bravebot-agent --test lsp
cargo test -p bravebot-session
```

Use `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` when backend credentials are unavailable. Follow
[checks.md](checks.md) for the other checks required by the changed files and consumers.

Apply [testing-preflight](../../agents/skills/testing-preflight/SKILL.md) before changing behavior
or assertions. Useful fault experiments include restoring snapshot trust after a failed restore,
retaining imported checkpoints through bridge execution, and omitting invalidation before a hook
or language-server launch. Record the observed failure and checks for the exact revision in the
PR's testing summary. A passing test on its own does not show that it detects the intended fault.

## Limits

These tests exercise terminal handlers and storage boundaries, not a physical terminal or the
packaged desktop UI. They do not establish caller correctness for failure or cancellation without
undo, panic recovery, or crash recovery. Test those paths separately when changing them.

Coverage metadata assumes trusted local session records; it does not detect deliberate record
tampering. Records without a recognized marker lose undo, and older binaries do not enforce the
new coverage contract. The backup journal does not track external writers or entire process trees.
