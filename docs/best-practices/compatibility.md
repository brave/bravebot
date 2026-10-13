# Compatibility

<!-- applicability: always -->

What a reviewer checks when a change alters something that outlives the process that wrote it: a
file under the state directory, a settings or configuration file, a saved session, or a bot
definition. No tool decides whether an old value still reads correctly.

---

<a id="COMPAT-001"></a>

## A change to persisted data reads old data or migrates it

**Code that reads persisted data keeps reading what earlier versions wrote, or ships a migration
that converts it.** A person upgrades with files already on disk, so the new version meets old
data before it writes any new data.

A reviewer looks for these in a diff that touches a format:

- A field renamed or removed without keeping the old name readable.
- A new required field with no default for files that lack it.
- A value whose meaning changed while its spelling stayed the same.
- An enum variant removed or renamed.
- A stricter parse, such as rejecting unknown fields or tightening a range, that an old file would
  fail.

A diff that follows the rule does one of two things:

1. **Reads the old form.** The reader accepts the old spelling or a missing field and gives it a
   defined value.
2. **Migrates.** A migration runs on first read, converts the old form to the new one, runs once,
   loses nothing, and leaves the file unchanged when it cannot convert it. The migration is a named
   function, not a branch inside the reader.

Either way, a test reads a fixture written in the old format and checks the result. A test that
only round-trips data written by the new code does not cover this.

```rust
// Wrong: files written before `scope` existed no longer load.
#[derive(Deserialize)]
struct Entry {
    path: String,
    scope: Scope,
}

// Right: a missing `scope` loads as `Scope::default()`, which is the value the earlier version
// implied.
#[derive(Deserialize)]
struct Entry {
    path: String,
    #[serde(default)]
    scope: Scope,
}
```

A change that cannot keep compatibility says so in the pull request description, names the versions
it breaks, and gives the person's way forward. A silent break is not acceptable, and neither is a
reader that discards data it cannot parse.

**Why:** a person with existing data who upgrades and finds it unreadable loses sessions, settings
or bots, and tests written against the new format pass on that diff.
