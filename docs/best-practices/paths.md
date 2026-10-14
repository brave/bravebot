# Paths

<!-- applicability: always -->

On Linux a path, a program name, an argument and an environment value are arbitrary bytes. What a
reviewer checks here, none of which a compiler or a lint can decide, is whether a conversion to
text sits on a display or on a decision. Most of the `to_string_lossy` calls in the tree are
display, which is correct, so a lint cannot tell the two apart. The same reviewer checks that a fix
to a name that got past a rule accounts for the other names the file has.

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
