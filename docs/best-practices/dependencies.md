# Dependencies

<!-- applicability: always -->

A new crate widens the supply-chain surface of a binary people install.
`deny.toml` decides the half of that a tool can: an advisory out against it, the
licence it carries, a second version of something already in the tree, and
whether it came from crates.io. Whether the trade is worth making is the half
left, and it is this document.

---

<a id="DEP-001"></a>

## No new dependency without a reason that survives scrutiny

**A diff that touches `Cargo.toml` or `package.json` says in the pull request
what the dependency buys and what writing it by hand would cost.** Convenience
is not an argument on its own. Depth counts: a crate that pulls in twenty others
is twenty decisions, not one.

A diff that adds an entry to `[advisories] ignore` or `[bans] skip` in
`deny.toml` answers the same question in the other direction: what ships anyway,
and why that is acceptable. The `reason` field is where it goes, since the next
person to read it is whoever is deciding whether it still holds.

---

<a id="DEP-002"></a>

## Prefer literal matching to a regex engine

**Patterns that arrive through a turn are attack surface.** Prefer literal
matching and hand-written, non-backtracking matchers to a regex engine,
particularly anywhere a pattern could come from content rather than from this
repository's own source.
