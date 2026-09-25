# Testing shared file authority

A parent and its delegates share current file decisions while their capabilities, routing,
quarantines and conversations remain separate. The governing contracts are
[TRUST-4](../specs/trust-map.md#TRUST-4) for file effects,
[DELEGATE-11](../specs/delegation.md#DELEGATE-11) for decisions shared with delegates,
[RUN-4](../specs/tools/run.md#RUN-4) for command output, and
[SESSION-2](../specs/sessions.md#SESSION-2) for saved backup bytes. Their `verified-by` entries
name the tests that pin each requirement.

This guide covers tests of overlapping effects and their readers. For checkpoint eligibility,
restoration and session handoffs, see [Testing undo coverage](undo-coverage.md).

## Regression cases

The [turn tests](../../crates/agent/tests/turn.rs) exercise real delegate tools, subprocesses and
captured planner requests. Collection order must not decide file trust: test both collection
orders and both write orders, with an untouched sibling to detect changes to unrelated paths.
Hold writers at observed events so parent and sibling reads happen before collection.

| Case | What the test must distinguish |
|---|---|
| Trusted write followed by an untrusted overwrite | Disk bytes, the current map and the next planner request must reflect the later effect. Collecting the earlier writer must not restore its grant. |
| Read while a write is in progress | Readers must see distrust from effect entry until completion. An error after replacement must leave distrust; a successful write must not overwrite a newer decision. |
| Two writers to the same path | The second is refused as contention before writing. An independent path remains writable. Staleness and contention must stay distinct because callers handle them differently. |
| Foreground redirection | Pause a real process after it writes and read the destination before releasing it. Check success, command failure, parent failure and observed cancellation, including process cleanup. An untaken branch must leave its destination unchanged. |
| Output based on file trust | An intervening file effect must invalidate the command's earlier proof, including a same-label effect. Check explicit and automatic background output delivery, with an unchanged-state control. |
| Approval after a preview | A write between preview and answer must prevent the approval from granting trust to the newer version. The same approval with no intervening write must still work. |
| Backup serialization | A stale pre-turn grant or a later grant must not authorize bytes captured as untrusted. Check trusted controls and exclusion of both raw and base64 sentinels from the saved record. |
| Credential attribution | Identical prior bytes with different capture trust must produce distinct decisions. Untrusted prior bytes cannot excuse a credential as already present. |

## Where to test

- [Workspace unit tests](../../crates/agent/src/workspace.rs) pause writes after replacement to
  exercise live readers, conflicting writes and injected failures.
  [Workspace integration tests](../../crates/agent/tests/workspace.rs) cover relative and absolute
  names, added directories, scratch paths and independent writes.
- [Policy tests](../../crates/core/src/policy.rs) check approvals against the version shown to the
  person. Keep both stale-preview and unchanged-preview controls.
- [Tool tests](../../crates/agent/src/tools.rs) exercise background output proof revalidation.
  The turn tests cover the wiring from approved commands to file effects and planner requests.
- [Manifest tests](../../crates/agent/tests/manifest.rs) cover credential attribution for file
  replacements. These are a separate caller of the write boundary and need their own tests.
- [Terminal session tests](../../crates/tui/tests/sessions.rs) exercise backup capture provenance
  through actual session save/load, including executor redirection.

## Running and assessing the tests

Focused entry points include:

```sh
cargo test -p bravebot-agent --test turn overlapping_delegate_writes
cargo test -p bravebot-agent --test turn foreground_redirection_quarantines
cargo test -p bravebot-agent --lib workspace::tests
cargo test -p bravebot-tui --test sessions backup_capture_trust
```

Use `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` when backend credentials are unavailable. Follow
[checks.md](checks.md) for checks required by the changed files and consumers, and
[testing-preflight](../../agents/skills/testing-preflight/SKILL.md) when choosing assertions.

Useful fault experiments include omitting distrust at effect entry, replacing capture integrity
with a stale snapshot's grant, and accepting a preview approval after the path changed. Record
observed failures, checks and gaps for the exact revision in the PR's testing summary. Separate
results demonstrated by a failing experiment from coverage inferred by reading the code.

## Limits

Shared authority coordinates a run and its delegates. It does not coordinate separate sessions,
external editors, arbitrary side effects inside programs, hostile symlink changes or crash
recovery. [TRUST-18](../specs/trust-map.md#TRUST-18) defines path naming; it does not turn names
into a physical-file identity system. Capture holds a shared lock, so a large search can delay
other captures.

These tests do not establish caller retention after failure or cancellation, manifest recovery,
or panic recovery. Test those paths at their caller and storage boundaries when changing them.
A poisoned authority declining trust is not proof that the whole caller recovered safely.
