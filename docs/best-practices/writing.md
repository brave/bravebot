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
