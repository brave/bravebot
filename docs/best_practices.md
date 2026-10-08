# Best practices

What a reviewer holds a pull request against. Every rule here is one a person has to read a diff to
decide.

Nothing here restates a rule a tool already enforces or could: formatting, lints, clause numbering,
the lockfiles, an em-dash, an attribution marker, a `declassify` outside the gates, a regex engine,
an exception in `deny.toml` with no reason, a first-person correction in a commit message. Those
belong in `make check`, `make check-spec`, `make check-npm`, `make check-deps`,
`make check-narration` and the security scan, and a rule that can be written as one of those
checks is a bug against this directory rather than an entry in it.

| Read | For |
|---|---|
| [best-practices/specs.md](best-practices/specs.md) | prose a clause is allowed to be, and a change agreeing with the specs that govern it |
| [best-practices/tests.md](best-practices/tests.md) | what a test is named, what it covers, and proving it fails first |
| [best-practices/writing.md](best-practices/writing.md) | what a comment is for |
| [best-practices/dependencies.md](best-practices/dependencies.md) | what a dependency has to be worth across package managers |
| [best-practices/shared-implementation.md](best-practices/shared-implementation.md) | where shared client behavior lives and how callers reuse it |
| [best-practices/paths.md](best-practices/paths.md) | carrying paths, programs and arguments as bytes so a lossy rendering never decides what is approved, granted or started |
| [best-practices/ui.md](best-practices/ui.md) | what a change to the desktop UI is held to before it lands, and where its logic lives |

The review pass over the rule this repository exists for is
[development/reviewing-for-the-rule.md](development/reviewing-for-the-rule.md): the four
shapes a violation takes in a diff, and the argument that mistakes a sound design for one.
[specs/](specs/README.md) is the source of truth for behaviour, and a spec wins where a spec
and a rule here disagree.
