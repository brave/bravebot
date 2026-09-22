# Overlapping delegate writes: Item 01 evidence

Implemented on `reconcile-delegate-writes`, starting at main
`25bc43ca3810abad88d55e315668e97d250d3910` on 2026-09-23. The prerequisite is the notebook
item 00 decision record at `7a6d16de75cc0dc53f29ea88f5a4bceb932680fc`, D1 and A1–A3/A12.
The combined branch at `d9fc42c617d5ab8ac870da3ef201d13f8754898a` was left untouched.
This document records this implementation's evidence, not the combined branch's results.

## Behavior

A parent and its delegates share one `FileAuthority`. Their capabilities, routing, quarantines,
conversations and prompt history remain separate. Exact command approvals still return as deltas.
File snapshots never overwrite the shared authority at collection.

Captures hold a short lock across label lookup and byte capture. Before entering a write, the
writer reserves its path and publishes distrust. Reads during the write receive an untrusted
label. Completion publishes the input's integrity only if no later decision superseded that
reservation. An error or dropped reservation leaves distrust. A second writer to the same path
is refused before it writes; other paths remain available. No lock spans a prompt, model call,
process wait or delegate join.

Foreground redirections enter this boundary before opening their destinations. Only branches
actually entered reserve paths. All stages must succeed before prior trust can be preserved;
commands never upgrade a previously untrusted destination. Early executor errors stop and reap
started stages before reservations are released. Cancellation uses the existing stop path.
An ordinary failed parent still joins its delegates, as before.

Command output is downgraded if file authority changed between its input proof and output capture.
The command's own redirection entries are accounted for separately. Background output checks the
same revision before explicit output delivery and automatic completion delivery. This is
conservative: even an unrelated file decision can quarantine output. Background redirections and
reference stdin remain refused.

Backups carry their capture integrity. Storage uses that field, never a pre-turn map, to decide
whether to save bytes. The stored format is unchanged. Imported checkpoint validity and safe
rewind are Item 02; ordinary interrupted caller retention and save/resume are Item 03.

## Behavior, faults and evidence

| Behavior | Test / fault | Evidence |
|---|---|---|
| A trusted sibling write followed by an untrusted overwrite cannot retain the grant | `overlapping_delegate_writes_follow_effect_order_in_both_collection_orders`: actual tool calls, bounded planner requests, both spawn/collection orders, both write orders, untouched third sibling, disk bytes, final map and next-planner requests | Demonstrated. Before the implementation, the initial successful-parent fixture failed because a sibling grant survived the later untrusted write. The final fixture passes all four order combinations. |
| Parent and sibling reads before collection cannot use stale grants | The same delegate fixture holds both writers in pending model requests while parent and sibling read | Demonstrated through production reads and captured planner requests. |
| Reads between effect and completion cannot use the old grant; an error cannot promote a replacement | `reads_before_write_publication_and_failed_replacements_remain_untrusted`: real Workspace write, test-only pause after disk replacement, two reader policies, attempted vouch, independent writes, and injected I/O error | Demonstrated. Removing only distrust at effect entry fails with `reader used the old grant during the effect`. Restored sources pass. The injected error is after replacement, not a simulated short OS write. |
| Independent paths and supported alternate names retain correct decisions | `shared_file_authority_preserves_aliases_scratch_added_paths_and_independent_writes`: relative/absolute spellings, canonical added-directory and scratch names, untouched snapshot, trusted replacement after untrusted replacement | Tests pass. No separate fault mutation for each spelling. |
| Foreground destinations stay untrusted while a process writes | `foreground_redirection_quarantines_live_reads_and_all_endings`: actual redirected process signals over a socket after writing, parent reads before release, command success/exit 7, parent failure and cancellation | Demonstrated through process and planner boundaries. Cancellation asserts `Ending::Stopped`; parent failure asserts `Unauthorized`; process EOF confirms cleanup. |
| Untaken branches do not change trust | Existing `a_branch_that_does_not_run_leaves_its_destination_as_it_was` | Passes in the affected suite; disk and trust assertions remain intact. |
| Background completion revalidates its proof | `an_ended_job_revalidates_its_file_proof_before_releasing_output`: real process output, unchanged control and intervening same-label effect | Tests pass. Explicit `job_output` revision checking and concurrent foreground input-proof invalidation are reasoned from code, not separately scheduled end-to-end race fixtures. Existing background and command-proof tests pass. |
| Stale pre-turn trust cannot authorize backup storage | `backup_capture_trust_overrides_a_stale_pre_turn_grant`: executor redirection through the real observer, Workspace backup capture, real session save/load, trusted backup control, raw/base64 sentinel exclusion | Demonstrated. Replacing capture integrity with the stale snapshot's grant saves the sentinel and fails the record assertion. Restored sources pass. The fixture wires the executor observer directly; the separate turn fixture exercises the tool's observer wiring. |
| Preview approval cannot trust or overwrite a newer same-run version | Per-path revision check around vouch and whole-file write; trusted capture and revision check for edits | Reasoned from the lock and revision checks. Existing approval/stale-edit tests pass, and the paused-write test rejects a vouch during an active effect. No separately scheduled prompt-versus-completed-write race test. |
| Exact command scope remains unchanged | Existing core adoption and agent command-approval tests | Pass. File authority sharing does not share capability or one-use endorsement state. |

Mutation experiments saved exact source contents, restored those contents, and reran the affected
checks. Compilation and fixture setup failures encountered while building the tests were excluded
from fault evidence. No mutation remains in the implementation.

## Consumer audit

- Direct text reads, paged reads and attachments capture bytes and labels under the authority lock.
  Preamble/context and workspace skills use these reads. Their later consumers hold immutable
  labelled values. The provenance treatment of the user's own `~/.bravebot` configuration remains
  the explicit TRUST-11 contract, outside workspace file trust.
- Listing and grep hold the capture boundary while enumerating and applying their existing label
  gates. Deferred file references capture under the same boundary through `Policy::materialise`.
  Already materialized slots keep the labels of their immutable bytes.
- Vouch previews carry a path revision; a later version cannot use the earlier preview approval.
  Whole-file approvals do the same. Edit comparison uses `read_trusted_content` before comparing.
  Credential attribution for interactive whole-file writes receives a pre-image only when its
  capture was trusted. Three stale-edit fixtures and one credential fixture now grant that trust
  explicitly; their original behavioral assertions remain.
- LSP prose remains untrusted regardless of file trust. Structural locations use the existing
  sanitized location path on this main revision; they do not turn filesystem prose into trusted
  content. No LSP trust upgrade was added.
- Foreground read proofs and both background output-delivery paths revalidate the authority
  revision after output capture. No trust decision hashes or examines untrusted bytes.
- Manifest file writes already use Workspace's effect path. Their redundant after-write
  reconciliation was removed so a caller cannot republish a stale completion. This does not add
  manifest recovery or Item 05 state retention.
- CLI, TUI and bridge continue consuming trust snapshots. The public `Policy::trust` accessor now
  returns an owned snapshot; delegates receive live authority separately. `Backup` constructors
  now supply capture integrity. TUI's session tests exercise the shared session storage boundary.
  No bridge protocol or desktop state shape changed. Ordinary failure/cancellation retention in
  these callers remains Item 03, and this change does not claim to fix it.

## Checks

With `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` for Cargo commands:

- `cargo test -p bravebot-core -p bravebot-agent -p bravebot-session -p bravebot-tui -p bravebot-ui-bridge -p bravebot-cli`: passed, including the session record regression, existing command controls and caller tests.
- Final focused core/agent library tests and the expanded delegate fixture: passed after the last active-vouch and untouched-sibling changes.
- `cargo check --workspace`: passed; covers CLI, TUI and bridge consumers.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `make check-spec`: passed with 0 errors and 34 existing unverified-clause warnings.
- `make check-security`: passed, 0 mechanical findings. This is the mechanical check plus the source audit above, not an independent security review.
- `npm ci --ignore-scripts --no-audit --no-fund` and `npm run typecheck` in `ui`: passed.
- `git diff --check`: passed.

Windows checking was selected because paths and process tests differ by platform. `make
check-windows` could not start: Docker is not installed. Linux and MSRV container checks were not
run for the same reason. No Windows runtime tests, desktop interaction smoke test or independent
review was run. `make check` is a before-push gate; no push was requested or performed.

## Limits and later work

This authority coordinates one run and its delegates. It does not coordinate separate sessions,
external editors, arbitrary side effects hidden inside programs, hostile symlink changes, or
crash recovery. TRUST-18's existing distinction between path names and physical-file identities
still applies; internal aliases are not made into a new physical-file identity system here.
Coordination is short-lived but global during capture, so a large search may delay other captures.

Item 02 owns restore coverage, imported checkpoints and grants after partial rewind. Item 03
owns ordinary failure/cancellation state returned to callers and saved for later turns. Item 04
owns panic recovery. Poisoned authority declines trust and cannot validate an output proof, but
that local precaution is not evidence of full panic recovery. Items 02–10 were not implemented.
