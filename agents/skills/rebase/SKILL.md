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

1. `python3 agents/skills/rebase/rebase.py start <pr>`
2. For each `path:start-end` it prints, Read only that range of the worktree's file and Edit it
   so both sides survive: the base's change and the pull request's. The commits it lists are the
   base's changes to those files; `git show` one only when a hunk alone is ambiguous. Don't
   `git add`. Where the two sides can't both be kept, stop and ask. A conflict in code touching a
   label: read [reviewing-for-the-rule.md](../../../docs/development/reviewing-for-the-rule.md)
   first.
3. `python3 agents/skills/rebase/rebase.py continue <pr>`, repeating 2 until it prints `next: push`.
4. If a conflict was resolved: `python3 agents/skills/rebase/rebase.py check <pr> [target ...]`
   with the targets covering the resolved files, per
   [checks.md](../../../docs/development/checks.md); it defaults to `check-all-local`. A clean
   rebase skips this and leaves the checks to CI.
5. `python3 agents/skills/rebase/rebase.py push <pr>`

Report the old and new head, one line per resolved conflict on how both sides were kept, and which
checks ran.
