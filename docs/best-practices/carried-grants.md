# Carried grants

<!-- applicability: paths:crates/session/,crates/agent/,crates/core/,crates/tui/,crates/ui-bridge/ -->

What a reviewer checks when a change keeps a permission for later and reads it back: a trust
answer, a vouched command, a granted directory or a kept confirmation, held in a session record, a
rewind point or a state file. No tool decides what the reader compares the entry against.

---

<a id="KEPT-001"></a>

## A kept grant is read back only for what it was given for

**A grant that is written down and restored later is bound to the identity of the thing it was
given for.** Between the write and the read a person can remove a checkout and put another at the
same path, restore a backup, or re-clone a project. A grant compared by path string comes back for
whatever stands there now, so the restore compares something only the original has.

A reviewer looks for these in a diff that stores or restores a grant:

- A restore that compares a directory string or a name and nothing else.
- A grant added to what a resume or a rewind brings back, with no clause naming it.
- A default of "granted" for a record that lacks the field. A record from before the field existed
  asks again.

A diff that follows the rule does one of two things:

1. **Binds the grant to identity.** The entry records the identity the specs already use for the
   same answer ([TRUST-23](../specs/trust-map.md#TRUST-23) keys a remembered answer on the
   directory's identity), and the restore applies it only on a match. A move on one volume keeps
   the grant. A copy, a restore from backup or a re-clone asks again.
2. **Adds the grant to the Known costs entry.** The sessions spec already accepts that a resume
   restores the trust map and the vouched commands for whatever stands at the recorded path
   ([Known costs](../specs/sessions.md#known-costs)). A grant outside that list is added to the
   entry in the same pull request, and the description says so, so the review can decide whether
   the wider cost is acceptable.

```rust
// Wrong: the grant comes back for any directory whose path spells the same.
if saved.root == current_root {
    grants.allow(Grant::RunPlainLines);
}

// Right: the grant comes back only for the directory it was recorded in.
if saved.identity == identity_of(&current_root) {
    grants.allow(Grant::RunPlainLines);
}
```

A test writes a record, replaces the directory with another at the same path, resumes, and checks
that the grant did not come back. A test that resumes in the original directory covers only the
allowed case.

**Why:** a grant lets a command run unasked or a file be read as trusted, and the person who gave
it was shown one checkout. A restore that hands it to another checkout lets code the person never
looked at use it. Each grant added to a resume widens what that checkout can do before anyone is
asked.
