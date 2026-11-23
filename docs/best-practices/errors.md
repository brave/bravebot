# Errors

<!-- applicability: always -->

What a reviewer checks about how code reports a value or a state it did not expect. `unwrap` is
denied by a lint, and `check-lint-increase` refuses a crate that gains `expect` or `panic` sites.
Neither can tell whether a value is one the program wrote or one from outside, or whether the
reason on an `#[expect]` is true of every input.

---

<a id="ERR-001"></a>

## A value from outside the process is checked and refused with an error

**A value read from a file, a socket, a model reply, the environment, a person's prompt or the UI
reaches an `expect`, `unreachable!`, `panic!` or `assert!` only after code has checked it and
returned an error for a value that failed.** `[profile.release]` sets `panic = "abort"`, so one
malformed input ends the session ([fuzzing.md](../development/fuzzing.md)). A `debug_assert!` that
is the only guard is skipped in a release build, so the malformed value is not checked. An index or
a slice on a length from outside is covered by
[the verification guide](../development/verification.md#when-to-write-one).

The rule applies to code that ships in the binaries. A test, an example and a build script may
unwrap, and fail at the line that did.

A condition the code itself established may be asserted, with a reason that says where it was
established, such as the key a function inserted a few lines earlier. A call that has no failing
input needs no check, only a reason that is true of every input: `Hmac::new_from_slice` accepts a
key of any length, and `crates/signing/src/webhook.rs` says so. A reviewer reads the reason on each
`#[expect(clippy::expect_used, reason = "...")]` and the message of each `unreachable!`, and
checks that it holds on every path to the call, including a file an earlier version wrote, a reply
from a different model and text a person edited.

```rust
// Wrong: the reply comes from outside, so a missing field ends the session.
#[expect(clippy::expect_used, reason = "replies always carry an id")]
let id = reply.get("id").expect("id");

// Right: the missing field is an error the caller handles.
let Some(id) = reply.get("id") else { return Err(Error::ReplyWithoutId) };
```

Content carried under an untrusted label is read by the driver only through the gates, and
[reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md) is the review of that. A
fix for a panic in a crate set up for Verus also follows [VER-001](verification.md#VER-001).

**Why:** a site that asserts on outside input can be made to crash by anyone who writes the input,
by mistake or on purpose.
