## Lane: what runs here that nobody in this repository wrote

Every other lane asks whether untrusted content can decide something. This one asks a different
question, and it is the only lane where the attacker is not the content: **what executes with this
tree checked out, or gets linked into the binary, and who decides what that is?**

The guarantee this repository makes says nothing about a build step that was replaced upstream. A
dependency that starts reading the environment, or a workflow step whose owner moves a tag, defeats
every gate in `crates/core` without touching one, because it runs before or beside them.

Most of this is already enforced. `make check-deps` runs the dependency policy in `deny.toml` over
advisories, licences, sources and duplicate versions. `make check-security` holds every workflow step
to a commit and every container image this tree runs to a digest, and faults a job that installs or
runs an npm dependency while holding `id-token: write` or a secret. Do not report what those already
fail on, and do not report a rule as a review habit when it could be a check: where you find one, the
finding is a request for the check.

### Where to start

Workflows:

{workflow_files}

The dependency policy and the manifests:

{dependency_files}

The one crate that opens a socket the egress gate does not own:

{second_client}

### What to ask

**A step that runs with the tree and a token.** For each workflow: what triggers it, what
`permissions` the job holds, and whether any step's input comes from a place a person outside the
repository can write. A trigger that runs against a fork's code with a token that can write is the
shape to look for, whatever it is called. A step that is pinned to a commit is fixed; a step that
resolves a version at run time is not, however it is spelled.

**A secret in an environment a step does not need.** A token granted at the job level is readable by
every step in that job, including one from a third party. Ask whether each secret could be scoped to
the step that uses it, and whether anything a step prints could carry one into a log.

**A dependency that is not what the lock file says.** `Cargo.lock` and `deny.toml` are the record.
Ask what is unused, what is unmaintained, and what pulls in a second copy of something already in the
tree. Where a dependency is fetched from anywhere other than the registry the policy names, that is a
finding regardless of what it contains.

**A second network client.** The egress gate is `bravebot-net`, and one crate does not use it. Ask
whether that client gets the same certificate roots, the same timeout, the same proxy handling and the
same host allowlist as the gate. A difference between the two is a finding even where neither is
wrong on its own, because the one a person read is not the one that runs.

**A downgrade on a path that started encrypted.** `crates/net/src/lib.rs` accepts both schemes and
resolves a redirect's `Location` against the URL it came from. Ask what happens when a response to an
`https://` request names an `http://` location: whether the scheme is checked again, whether the host
allowlist is re-applied to the resolved URL, and what is sent in the second request. A request a
person approved to one host, in plaintext to another, is a `high`.

**Python that reads a file somebody else wrote.** `contrib/` and `agents/skills/` run on a
maintainer's machine over paths from a work directory. A deserialiser that can construct arbitrary
objects is the shape to look for, and so is a path from a manifest joined onto a directory without
being checked to stay inside it.

### What is not this lane

An out of date dependency with no advisory against it. A licence question. A version bump. Anything
`make check-deps` already reports, which is red in CI on the branch that caused it. And any finding
whose whole content is that a third party could in principle be compromised: name the step, the
permission and what it reaches, or do not file it.
