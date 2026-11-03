# Paths

<!-- applicability: always -->

On Linux a path, a program name, an argument and an environment value are arbitrary bytes. What a
reviewer checks here, none of which a compiler or a lint can decide, is whether a conversion to
text sits on a display or on a decision. Most of the `to_string_lossy` calls in the tree are
display, which is correct, so a lint cannot tell the two apart. The same reviewer checks that a fix
to a name that got past a rule accounts for the other names the file has. It also checks that a
program bravebot starts itself is tested for the names it starts, and that a fix to one such program
accounts for the others [SANDBOX-30](../specs/sandboxing.md#SANDBOX-30) lists. For a program
bravebot starts for itself with the person's environment, it checks that each variable the program
reads to choose code to run is removed unless its value is a safe file, directory or program name.

---

<a id="PATH-001"></a>

## Carry a path, program, argument or environment value as an OS string until it is shown

**Text conversion is for display.** `to_string_lossy`, `to_str`, `String::from_utf8_lossy` and a
`format!` of a path replace every byte that is not UTF-8 with U+FFFD or drop the value, so two
different values become one string. A reviewer looks for one of these whose result is compared,
hashed, used as a key or a map entry, matched against a rule, written to a persisted file, passed to
`Command::new`, `Sandbox::command` or `Sandbox::spawn`, or turned into a filesystem grant. In each
of those the value stays a `Path`, `PathBuf`, `OsStr` or `OsString`, or a `Vec<u8>` where it has to
be stored.

```rust
// Wrong: two files whose names differ only in invalid bytes get the same key.
let key = resolved.to_string_lossy().into_owned();
approved.insert(key);

// Right: the key is the bytes of the path.
use std::os::unix::ffi::OsStrExt;
approved.insert(resolved.as_os_str().as_bytes().to_vec());
```

**Why:** an approval, a grant or a started program is decided by the string, so a lossy rendering
lets a different file be approved, granted or run than the one a person was shown.

---

<a id="PATH-002"></a>

## Where a boundary can only carry text, refuse a value that is not text first

**A boundary that takes `&str` or `String` refuses an `OsStr` that is not valid UTF-8, and does so
before the approval is asked for or the grant is made.** A conversion that substitutes a
replacement character, or drops the value (`filter_map(|v| v.to_str())`, `to_str()?` in a loop),
changes what the approval or grant covers without saying so. A reviewer looks for a loop or a
`filter_map` over paths, arguments or environment values that skips an entry it cannot convert, and
for a lossy conversion between the value a person approved and the value the boundary receives.

```rust
// Wrong: an argument that is not UTF-8 is dropped, so the command that starts has fewer
// arguments than the one that was approved.
let args: Vec<&str> = argv.iter().filter_map(|a| a.to_str()).collect();

// Right: the whole command is refused, and nothing was approved or granted.
let args = argv
    .iter()
    .map(|a| a.to_str().ok_or(Error::NotText))
    .collect::<Result<Vec<_>, _>>()?;
```

**Why:** a refusal is visible and the person can act on it, while a substituted or dropped value
makes the approval cover something other than what runs.

---

<a id="PATH-003"></a>

## A test for path-handling code uses a name that is not UTF-8 and its lossy lookalike

**A test for code that handles a path, program, argument or environment value includes two names
that differ only where a lossy rendering would make them equal.** One is a name that is not UTF-8,
and the other is the name that its replacement-character rendering spells. A test that uses only
valid UTF-8 passes with a lossy implementation. Build the names in memory with
`OsStrExt::from_bytes` (`std::os::unix::ffi`) so the test runs on macOS, whose file system cannot
hold the name. A test that needs the file on disk is Linux-only and says so with
`#[cfg(target_os = "linux")]`.

```rust
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

// The two names a lossy rendering would merge.
let invalid = OsStr::from_bytes(b"tool-\xff");
let lookalike = OsStr::new("tool-\u{FFFD}");
assert_ne!(key_for(invalid), key_for(lookalike));
```

**Why:** without both names, an assertion that two values are different cannot fail on the
implementation that merges them.

---

<a id="PATH-004"></a>

## A fix for a spelling that got past a rule goes through every other name for the file

**A change that fixes a path spelling that bypassed a rule lists the other names the filesystem
treats as the same file, and says for each whether the change handles it.** The list is the table
of spellings in [TRUST-2](../specs/trust-map.md#TRUST-2). A reviewer looks for the pull request to
go down its rows, to add a row for a name the table lacks, and to show a test for each row it
handles and the issue for each it leaves. A diff that fixes only the spelling that was reported
leaves the next one to be found by another report.

**Why:** a rule is read under the name a call used, and a filesystem gives one file more names than
any one report finds. A fix confined to the reported name closes one of them and says nothing about
the rest.

---

<a id="PATH-005"></a>

## A program bravebot starts itself names the tests that show it never runs a file a stage wrote

**A change that starts a program bravebot runs for itself names, for that program, the test that
shows a file in a directory [SANDBOX-30](../specs/sandboxing.md#SANDBOX-30) leaves out is not found
as the program, the test that shows the same for a name the program itself starts, the `PATH` the
program is given, and whether `own_credentials()` is removed from its environment
([CRED-14](../specs/credential-protection.md#CRED-14)).** `clippy.toml` refuses every call the
tree starts a program with, and a start goes through `bravebot_sandbox::programs::start`, which takes
a `Resolved`, what the program inherits and the `PATH` it is given. A reviewer reads what neither
can decide: that the constructor in the diff (`own_executable`, `written_in_source`,
`named_by_person`, `found_earlier`, `to_confine`) names who chose the program truthfully, and what
the program itself starts by name (SANDBOX-30).

The second test puts a directory SANDBOX-30 leaves out first on the `PATH` bravebot inherits and
plants an executable file in it under a name the program itself starts, such as the interpreter an
`env` shebang names. The planted file records that it ran and then runs the real one, which is
installed later on that `PATH`. The test starts the program through the function production uses,
and asserts both that the planted file did not run and that the program ran to its end. The first
test passes for a program whose own file is found safely and whose interpreter is not, so it is no
substitute for the second. The program is started with `SearchPath::Reduced`, or the names it starts
are passed as absolute paths, as the `/caffeinate` inhibitor's `cat` is. A start that is given
`SearchPath::Inherited`, such as a language server that runs the project's own interpreter on
purpose, has its description cite the issue or Known costs entry that records it, as
[GAP-001](gaps.md#GAP-001) says.

**Why:** the lint sees every start and no reason for it. A reason that says a program was resolved
when it was found by a walk of the caller's own, and a program started by another program, each
reach the same unconfined lookup, and each shipped before being asked about. A program whose own
file is found safely still runs a file a stage wrote when a name it looks up resolves in a
directory SANDBOX-30 leaves out.

---

<a id="PATH-006"></a>

## A fix to one program on the SANDBOX-30 list goes through the others on it

**A change that fixes how one program in [SANDBOX-30](../specs/sandboxing.md#SANDBOX-30) is found or
started lists each entry in the clause and says for each whether the change handles it.**
A reviewer looks for the description to cover every entry, with a test for each one the change
handles. A diff that fixes only the reported program leaves the others unchecked. A case the change
leaves is recorded as [GAP-001](gaps.md#GAP-001) says. [PATH-004](#PATH-004) states the same rule
for the spellings of a path.

**Why:** the programs on the list are started from different code, so a fix at one start does not
reach the others unless it is made in code they share.

---

<a id="PATH-007"></a>

## A step that turns a name into a key says which different files it joins

**A change to a step that turns a name into a key (trimming, case folding, a lossy rendering, a
normalisation) states which different files the step joins and why, and carries a test in which
two names that must stay different do.** The test makes both names in a directory, asks the volume
whether they are one file, and asserts two keys when it holds them apart. A step that joins files on
purpose says so in the clause that governs it, as a distrust joining canonically equivalent forms
does in [TRUST-2](../specs/trust-map.md#TRUST-2). [PATH-004](#PATH-004) covers one file with many
names; this covers many files with one key. The names to build the pairs from are `held_names()` in
`crates/agent/test-support/aliases.rs`: `AWKWARD_NAMES` and `WINDOWS_NAMES`, less what a Windows
host does not hold.

**Why:** a table of the names one file has cannot show a step that merges two files, because every
row in it is a pair the volume already opens as one. Each such step was found by a report after it
shipped, and an extra rule about the wrong file decides what the person never agreed to.

---

<a id="PATH-008"></a>

## A program's code-selecting variables are removed unless they name a safe file

**A diff that adds or changes the start of a program bravebot starts for itself, with the
person's environment, has each variable the program's launcher reads to choose code to run removed,
unless the value passes a check.** The value passes when it is an absolute path that exists outside
every stage-writable directory, links followed, or, for a variable that names a program, a bare
program name, which the program finds on the reduced `PATH`. A variable that only carries a
credential, such as `GOOGLE_APPLICATION_CREDENTIALS`, stays. A variable that names a directory the
program reads configuration from counts as code when the program runs files from that directory,
even though the directory also holds credentials ([BACKEND-65](../specs/backends.md#BACKEND-65)).

A program bravebot starts for itself is one the person and the project did not ask to run. A `run`
stage, a `run` command in the background, a hook, a language server and a line typed at the `!`
prompt run commands the person or the project chose, so the rule does not apply to them. A start of
bravebot's own executable is not a start of another program, so the rule does not apply to it
either. A reviewer decides the rest from what the start is for, and the diff says which it is when
that is not plain. This entry does not say that a start the diff does not touch complies. The only
list whose values are kept or removed by `may_be_run` is `CODE_SELECTORS` in
`crates/agent/src/gcloud.rs`, so a diff that changes the AWS CLI, browser opener, scheduler or
push-identity start adds a list for it, or records the gap as [GAP-001](gaps.md#GAP-001) asks.

In a diff that adds or changes the start of a program bravebot starts for itself, a reviewer reads
the program's launcher and documentation for every variable that names an interpreter, a plugin or
module directory, a configuration directory it runs files from, or a wrapper to run first. Each one
is either listed in the program's selector list with tests, or the diff says why the program never
reads it. Such a start with no list, or with a variable the launcher reads missing from the list,
breaks the rule. A second list reuses the check by moving it to code the lists share
([PATH-006](#PATH-006)), not by copying it.

`make check-code-selectors` confirms only that every name in `CODE_SELECTORS` has one assertion that
removes a value and one that keeps a value. A reviewer confirms the rest of these, and that the list
is complete:

- A value inside a stage-writable directory is removed.
- A link from outside to inside such a directory is removed.
- A relative path and a path that does not exist are removed, except that a variable that names a
  program keeps a bare program name.
- An absolute path that exists outside every stage-writable directory is kept.
- A value that is not UTF-8 is judged as the bytes it is ([PATH-003](#PATH-003)).
- A variable the launcher does not read to choose code is left as set.

```rust
// Wrong: the variables are inherited as the person set them, so a value naming a file a stage
// wrote is run outside confinement.
bravebot_sandbox::programs::start(
    &Resolved::found_earlier(program),
    Inherits::Without(&own_credentials()),
    SearchPath::Reduced,
)

// Right: the same start, with each listed variable removed unless its value passes the check.
// This is the loop `gcloud.rs` has; a second program moves the check to shared code first.
let mut command = bravebot_sandbox::programs::start(
    &Resolved::found_earlier(program),
    Inherits::Without(&own_credentials()),
    SearchPath::Reduced,
);
for (name, names) in CODE_SELECTORS {
    if let Some(value) = held(name)
        && !may_be_run(names, Path::new(&value), &outside_stage_writable)
    {
        command.env_remove(name);
    }
}
command
```

**Why:** the sandbox confines what a stage runs. A program started outside it runs, with the
person's authority, whatever file a variable names as its interpreter or as a directory it sources
scripts from. The list of variables is read from the program, so a reviewer who does not read the
launcher cannot tell that one is missing.

