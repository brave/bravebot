---
name: shared-implementation-preflight
description: Check where shared Bravebot behavior belongs before adding a client, adapter, or behavior that multiple clients need. Reuse Rust implementations, preserve content boundaries, and identify the caller integrations that need testing.
---

# Shared implementation preflight

Use this before choosing where shared behavior will live, then check the final diff. Keep it
proportional to the change. Layout, copy, and other client-local changes need no separate review
unless they introduce shared behavior.

## Before editing

Read the governing spec and find the existing implementation and callers. Use
[SHARE-001](../../../docs/best-practices/shared-implementation.md#SHARE-001) for shared behavior and the
[layering spec](../../../docs/specs/layering.md) for crate responsibilities. For mobile work,
also read the [shared view contract](../../../docs/design/mobile/client-contract.md#shared-screen-state).

Identify what the change will reuse, what needs to move into shared Rust code, and why anything
stays client-side. A few sentences in the working plan are enough; no separate report or approval
step is needed.

- Call or extend the Rust crate that owns the behavior. Check existing clients for suitable code
  before adding another implementation. Extract a shared module or crate only when the callers
  and layering rules require it; avoid unrelated migrations.
- Trace each affected caller to that implementation. For mobile, remote mode consumes the host's
  Rust view and embedded mode consumes the same implementation through its native binding.
  TypeScript applies view updates without rebuilding session state in a second domain reducer.
  Matching fixtures do not justify maintaining duplicate rules.
- Keep layout, focus, drafts, scrolling, and other client-local state in the UI. Keep platform
  access behind narrow adapters. Check that an adapter does not recreate policy, approval rules,
  or session transitions already owned by Rust.
- Check what the proposed shared code reads, not just its language. Content-reading display
  transforms need a presentation boundary allowed by the layering spec. Moving them into Rust
  does not make them safe in the driver. For label or content-boundary changes, follow
  [the rule review](../../../docs/development/reviewing-for-the-rule.md).

Pass the affected callers, bindings, and protocol changes to
[testing-preflight](../testing-preflight/SKILL.md). Shared Rust tests establish shared behavior;
caller integration evidence must establish that each materially different path uses it correctly.

## Check the final diff

Confirm the changed callers reach the intended Rust implementation and no adapter or UI has
acquired a duplicate rule. Check that content handling still follows the governing specs and
that any changed protocol preserves the required compatibility behavior. If placement changed
during implementation, update the short explanation and the affected integration checks.

In the normal completion summary, name the reused or extracted implementation, explain any
behavior kept client-side, and report integration gaps using testing-preflight's evidence rules.
