# Dependencies

<!-- applicability: always -->

A new crate widens the supply-chain surface of a binary people install.
`deny.toml` decides the half of that a tool can: an advisory out against it, the
licence it carries, a second version of something already in the tree, whether
it came from crates.io, a regex engine, and an exception written down without a
reason. Whether the trade is worth making is the half left, and it is this
document.

---

<a id="DEP-001"></a>

## No new dependency without a reason that survives scrutiny

**A diff that touches `Cargo.toml` or `package.json` says in the pull request
what the dependency buys and what writing it by hand would cost.** Convenience
is not an argument on its own. Depth counts: a crate that pulls in twenty others
is twenty decisions, not one.

A diff that adds an entry to `[advisories] ignore` or `[bans] skip` in
`deny.toml` is read for what its `reason` says: what ships anyway, and why that
is acceptable. The next person to read it is whoever is deciding whether it
still holds.
