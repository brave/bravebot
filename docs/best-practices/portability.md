# Portability

<!-- applicability: always -->

What a reviewer checks about whether a change makes sense for each person who runs bravebot, with
their own files, settings and programs. Tests run on a few fixed machines and cannot show it.

---

<a id="PORT-001"></a>

## A change is not fitted to its author's setup

**A fix, change or addition makes sense for different people on different computers.** It does not
depend on a file, skill, setting or program that only its author's setup has. It may rely on, or
add support for, a program people commonly have installed, such as an editor or a terminal.

A reviewer looks for these signs that a diff is fitted to one setup:

- a name, path or value that exists only on the author's machine, such as one person's skill,
  `AGENTS.md`, setting, account, home directory or host;
- a dependence on a program the author has installed and most people do not;
- a workaround that makes one failing case pass, such as a branch, exception or default for that
  case, where other cases fail for the same reason.

**Why:** a change fitted to one setup leaves the problem in place for everyone else. It also adds a
special case the next reader has to work around and cannot remove without the author's machine to
test on.
