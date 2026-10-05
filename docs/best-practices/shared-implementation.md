# Shared implementation

<!-- applicability: always -->

What a reviewer checks when a change adds a client, adapter, or behavior that multiple clients need.

---

<a id="SHARE-001"></a>

## Clients use one Rust implementation of shared behavior

**A client or adapter reuses or extends the Rust implementation that owns shared behavior.**
Session transitions, approval semantics, settings, validation, and reusable presentation transforms
must not acquire a second implementation in a client. When the needed behavior lives in one client,
extract the shared part into an appropriate Rust module or crate with the first additional caller.
Matching fixtures do not justify keeping duplicate rules.

A reviewer traces the affected callers to the owning implementation and checks that adapters only
translate the interface. Client-local state and interaction, such as drafts, focus, scrolling, and
layout, stay in the UI. Platform access stays behind narrow adapters.

Placement must follow the [layering spec](../specs/layering.md). Content-reading display transforms
belong in an allowed presentation layer; extracting them into Rust must not introduce content
inspection into the driver. Review such changes with [the rule review](../development/reviewing-for-the-rule.md).

The [shared implementation preflight](../../agents/skills/shared-implementation-preflight/SKILL.md)
describes how to check reuse before editing and against the final diff.
