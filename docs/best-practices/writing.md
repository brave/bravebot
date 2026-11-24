# Writing

<!-- applicability: always -->

Comments. What a commit message, a pull request or a document may not narrate is checked by
`make check-narration`, and the rule is in [../development/commits.md](../development/commits.md).

---

<a id="WR-001"></a>

## Comments explain why, never what

**Prefer no comment to a restatement of the code.** A comment earns its place by saying something
the code cannot: the reason for a choice, the constraint that rules out the obvious alternative,
the cost that was accepted.

```rust
// Wrong: the line below already says this.
// Increment the counter.
count += 1;
```

---

<a id="WR-002"></a>

## A comment describes the code as it is

**A comment states what is true of the code in front of the reader. It does not mention code that
was replaced, an alternative that was dropped, or the review that led to the change.** "Used to",
"moved here from", "now uses", "removed because" and "per review feedback" make sense only to a
reader who saw the earlier code or the review. A comment that stays says what the code cannot
show: what it guards against, why the obvious alternative fails, and the cost accepted
([WR-001](#WR-001)). A comment that argues against an alternative only the earlier commits of the
change contained is deleted, and so is a justification of code that is clear without one.

A reviewer reads the comments a diff adds or changes and asks of each whether it makes sense to a
reader who has only the final file. "Used to" and "no longer" stay where they describe present
behaviour, as in "a file that no longer exists", and where the code reads data an earlier version
wrote ([COMPAT-001](compatibility.md#COMPAT-001)). A regression test's doc comment states the fault
it rejects as a behaviour an implementation could have, such as "a brace group is matched literally
and finds nothing", so it also reads correctly without the earlier code.

```rust
// Wrong: describes code the reader of this file never saw.
// The id used to be the time and the process id; it is random now.
fn new_id() -> String { random_id() }

// Right: says what holds and the constraint behind it.
// Random, so an id reveals neither when nor by which process it was made.
fn new_id() -> String { random_id() }
```

[SP-001](specs.md#SP-001) is this rule for specs. [commits.md](../development/commits.md) forbids
narrating the writer's own process in a comment, a commit message or a pull request, and
`make check-narration` rejects a fixed list of first-person phrases. No check reads the words
above, because "used to" and "no longer" also describe present behaviour.

**Why:** a later reader cannot find the earlier code that a comment refers to, and a comment such
as "now uses X" is false after the next change.
