---
name: rebase
description:
  'Rebase a brave/bravebot pull request onto the latest base in a worktree of its own, resolve
  the conflicts, and push it back with a lease. Works from a direct clone or a fork clone.
  Triggers on: /rebase <pr>, rebase this PR, PR has bitrot, PR has conflicts.'
argument-hint: '<pr number or URL>'
allowed-tools: Bash(python3 agents/skills/rebase/rebase.py *), Bash(git *), Read, Edit
---

# Rebase a pull request

`rebase.py` does everything that needs no judgement: it finds whichever remote points at
brave/bravebot and the one holding the head, puts the branch in `../<checkout>-<pr>`, fetches,
rebases, and pushes over only the head it fetched. Spend tokens on the conflicts and nothing else.

Each `rebase.py` step runs `git` against the remote or signs the commits it rewrites from inside
the script, so its command line shows no operation for the sandbox to lend a credential for. When
the `run` tool takes a `scopes` argument, pass `scopes: ["remote", "signing"]` on `start`,
`continue`, `check` and `push`. The person is asked about the line each time.

1. `python3 agents/skills/rebase/rebase.py start <pr>`
2. For each `path:start-end` it prints, Read only that range of the worktree's file and Edit it
   so both sides survive: the base's change and the pull request's. The commits it lists are the
   base's changes to those files; `git show` one only when a hunk alone is ambiguous. Don't
   `git add`. Where the two sides can't both be kept, stop and ask. A conflict in code touching a
   label: read [reviewing-for-the-rule.md](../../../docs/development/reviewing-for-the-rule.md)
   first.
3. `python3 agents/skills/rebase/rebase.py continue <pr>`, repeating 2 until it prints `next: push`.
4. `python3 agents/skills/rebase/rebase.py check <pr>` runs only what the files resolved in step 2
   call for: for Rust, `cargo fmt --check`, clippy on the crates touched and any resolved test
   binary; `check-spec`, `check-locales`, `check-security` or `check-versions` where a spec, catalog,
   workflow or manifest was resolved. A clean rebase, or a resolved file no rule names (`ui/`,
   website, prose), runs nothing and leaves the checks to CI. Pass make targets to run those
   instead; do not widen it to `check-all-local`, which CI repeats.
5. `python3 agents/skills/rebase/rebase.py push <pr>`. When GitHub refuses the push because ssh
   offered a key without access, it retries once with each other key `ssh-add -L` lists and reports
   the one that worked. With no usable key it prints GitHub's refusal.

Report the old and new head, one line per resolved conflict on how both sides were kept, and which
checks ran.
