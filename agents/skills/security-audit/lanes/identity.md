## Lane: a decision made on a name, and a use of the thing by name

Other lanes ask whether untrusted content can decide something. This one asks whether a decision made
on a spelling still holds when the thing is used. A path is judged safe, allowed, denied or granted as
a string, and the file, directory or socket is then opened, written or connected to by that string.
Whatever runs between the two can change what the string names: a stage the person approved, another
process of the same account, or a file that arrived in a clone. A directory replaced by a link, a file
given a second name or a parent renamed leaves a decision about one thing and a use of another.

The clauses are `TRUST-2` in `docs/specs/trust-map.md`, whose table lists the names one file has and
calls an uncovered cell a defect, `PERM-7` in `docs/specs/permissions.md`, which says a deny rule is
about the file and not the spelling used to ask for it, `SANDBOX-18` and `SANDBOX-24` in
`docs/specs/sandboxing.md`, and `PATH-007` in `docs/best-practices/paths.md`. `held_names()` in
`crates/agent/test-support/aliases.rs` is the tests' list of names to try. Do not report a spelling
the table-driven tests already fail on, or a gap the table records with its issue.

### Where to start

{identity_count} calls outside the tests that turn a name into a thing or ask what a name is
(`canonicalize`, `hard_links`, `read_link`, `symlink_metadata`, `PathFd::new`), by file with the
functions to open. `surface.json` lists every site:

{identity_files}

The lines in `crates/agent/src/confine.rs` that put a row into a stage's sandbox profile or name the
proxy's socket. The rows are the session's directories, the scratch directory, the proxy's socket
directory, the agent socket `SSH_AUTH_SOCK` names, and the paths a `request_path` grant names:

{confinement_rows}

### What to ask

For each site, find the decision and find the use.

**Where the name is decided on.** A rule matched against a path, a prefix test, a comparison with a
credential table, a canonicalised row compared with a refused one. Write down which spelling was
judged: the one the caller gave, a canonicalised one, one with a link resolved, or one with its last
component left alone.

**Where the thing is used.** The `open`, `read`, `write`, `connect`, `remove` or `rename` that
follows, and whether it takes the spelling that was judged or resolves the string again. A descriptor
opened once and used for the check and the use holds one thing. A check followed by a second open of
the same string holds two moments.

**What can run between them.** A confined stage that may write the directory the name sits in, a
program the person approved, a process of the same account, an archive extracted over the tree. Ask
what the stage may write, and whether that includes the name, its parent or a directory above it.
Where nothing can write in between, the site is sound and needs no more words.

**Which other names the thing has.** A symbolic link, a hard link, a case-folded or normalised
spelling, a path under another mount, a parent that is itself a link. A walk that judges each entry by
its spelled name misses a second name of a file inside the walk, and a hard link has a different name
and the same bytes.

**A row granted by name to something the stage can replace.** For each row in the list above: who
creates the path, who may write its parent, and whether anything compares the identity of what it
names after the first resolution. A row that resolves to a credential location after a relink, or a
socket path that an unconfined helper connects to by name on every connection, is a finding. Compare
how the session's other directories are held, since a check that covers every row but one names the
row to read.

### What is not this lane

A path comparison on text that is never opened afterwards. A name held only for display. A spelling
the trust-map table lists as a recorded gap with its issue. And a site whose only fault is that
check and use are separate calls, where nothing can change the name between them: name who can write
and what they gain, or do not file it.
