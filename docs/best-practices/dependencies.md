# Dependencies

<!-- applicability: always -->

A dependency adds code to the build or shipped application. Automated checks enforce configured
advisory, licence, and source rules; reviewers decide whether the dependency and its transitive
cost are justified. Rust checks do not establish coverage for other package managers.

---

<a id="DEP-001"></a>

## No new dependency without a reason that survives scrutiny

**A diff that adds or changes a dependency says in the pull request what it buys and what using
existing code or implementing the needed behavior would cost.** This applies to Rust, npm, and
native-platform dependencies, including CocoaPods, Gradle, and Swift Package Manager, whether
declared in a manifest or a build file. Review transitive dependencies, build scripts, and native
code as part of that cost. Convenience alone does not justify the addition.

A diff that adds an entry to `[advisories] ignore` or `[bans] skip` in
`deny.toml` is read for what its `reason` says: what ships anyway, and why that
is acceptable. The next person to read it is whoever is deciding whether it
still holds.
