//! Verification twins of three fixes in this repository's history, after the method of
//! "From Bug Hunting to Bug Prevention with Rust and Verus": a contract is written for the
//! function, Verus checks the fixed code against it, and `make check-verus` requires it to refuse
//! the code as it stood before the fix, which each `before_<commit>` cfg restores.
//!
//! docs/development/verification.md says what is assumed about the standard library and how the
//! twins are kept in step with the code.

use vstd::prelude::*;

verus! {

// Text as its UTF-8 bytes. What the standard library asks of an offset into a `str` is stated
// here once, the way `str::is_char_boundary` defines it, and every slice and truncation below is
// checked against it.

/// `str::is_char_boundary`: 0 and the length are boundaries, and so is any offset whose byte does
/// not continue a character.
pub open spec fn boundary(s: Seq<u8>, i: int) -> bool {
    i == 0 || i == s.len() || (0 <= i < s.len() && (s[i] & 0xC0u8) != 0x80u8)
}

pub struct Text {
    pub bytes: Vec<u8>,
}

impl Text {
    pub open spec fn view_bytes(&self) -> Seq<u8> {
        self.bytes@
    }

    pub fn len(&self) -> (r: usize)
        ensures
            r == self.bytes@.len(),
    {
        self.bytes.len()
    }

    pub fn is_char_boundary(&self, i: usize) -> (r: bool)
        ensures
            r == boundary(self.bytes@, i as int),
    {
        if i == 0 || i == self.bytes.len() {
            true
        } else if i > self.bytes.len() {
            false
        } else {
            (self.bytes[i] & 0xC0u8) != 0x80u8
        }
    }

    /// `String::truncate`, which panics where `n` is inside the text and not a boundary.
    pub fn truncate(&mut self, n: usize)
        requires
            n >= old(self).bytes@.len() || boundary(old(self).bytes@, n as int),
        ensures
            final(self).bytes@ == if n >= old(self).bytes@.len() {
                old(self).bytes@
            } else {
                old(self).bytes@.subrange(0, n as int)
            },
    {
        if n < self.bytes.len() {
            self.bytes.truncate(n);
        }
    }

    /// The precondition `&s[i..]` and `&s[..i]` share: `i` is no further than the end, and is a
    /// boundary.
    pub open spec fn can_slice_at(&self, i: int) -> bool {
        0 <= i <= self.bytes@.len() && boundary(self.bytes@, i)
    }

    /// `s[at..].chars().next().map(char::len_utf8)`, assumed rather than proved: in valid UTF-8
    /// the next character ends on a boundary inside the text. That is the one thing taken on
    /// trust here, and it is a property of `str`, not of the code under test.
    #[verifier::external_body]
    pub fn next_char_len(&self, at: usize) -> (r: usize)
        requires
            self.can_slice_at(at as int),
        ensures
            at as int + r <= self.bytes@.len(),
            (at as int) < self.bytes@.len() ==> r > 0,
            (at as int) == self.bytes@.len() ==> r == 0,
            self.can_slice_at(at as int + r),
    {
        unimplemented!()
    }

    /// `s[..to].rfind('\n')`, assumed: an ASCII byte is always the start of a character, so what
    /// it finds is a boundary before `to`.
    #[verifier::external_body]
    pub fn rfind_newline_before(&self, to: usize) -> (r: Option<usize>)
        requires
            self.can_slice_at(to as int),
        ensures
            r matches Some(at) ==> at < to && self.can_slice_at(at as int + 1),
    {
        unimplemented!()
    }

    /// `s[from..].find('\n')`, assumed likewise, as an offset from `from`.
    #[verifier::external_body]
    pub fn find_newline_after(&self, from: usize) -> (r: Option<usize>)
        requires
            self.can_slice_at(from as int),
        ensures
            r matches Some(at) ==> from as int + at < self.bytes@.len() && self.can_slice_at(
                from as int + at,
            ),
    {
        unimplemented!()
    }
}

// 3facf14a: a search's matching line is cut to MAX_MATCH_LINE bytes. `text.truncate(500)` panics
// on a line whose 500th byte is inside a character, which ended the turn.

pub const MAX_MATCH_LINE: usize = 500;

/// `truncate_on_char_boundary`, crates/agent/src/workspace.rs. The text ends up no longer than
/// `limit`, as a prefix of what it was, cut at the last boundary at or before `limit`.
pub fn truncate_on_char_boundary(text: &mut Text, limit: usize)
    ensures
        final(text).bytes@.len() <= limit || final(text).bytes@ == old(text).bytes@,
        final(text).bytes@ == old(text).bytes@.subrange(0, final(text).bytes@.len() as int),
        boundary(old(text).bytes@, final(text).bytes@.len() as int),
        forall|j: int|
            final(text).bytes@.len() < j <= limit && j <= old(text).bytes@.len() ==> !boundary(
                old(text).bytes@,
                j,
            ),
{
    if text.len() <= limit {
        return;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end)
        invariant
            text.bytes@ == old(text).bytes@,
            end <= limit < text.bytes@.len(),
            forall|j: int| end < j <= limit ==> !boundary(text.bytes@, j),
        decreases end,
    {
        end = end - 1;
    }
    text.truncate(end);
}

/// The cut a search makes to one matching line.
pub fn cut_a_matching_line(text: &mut Text)
    ensures
        final(text).bytes@.len() <= MAX_MATCH_LINE || final(text).bytes@ == old(text).bytes@,
{
    #[cfg(before_3facf14a)]
    text.truncate(MAX_MATCH_LINE);
    #[cfg(not(before_3facf14a))]
    truncate_on_char_boundary(text, MAX_MATCH_LINE);
}

// e0cd1903: a VISUAL selection is an anchor and the caret, as byte offsets into the prompt. An
// edit that shortened the line left the anchor past its end, and the next draw sliced the line
// there and panicked with the terminal in raw mode.

/// `boundary_at_or_before`, crates/tui/src/wrap.rs.
pub fn boundary_at_or_before(text: &Text, offset: usize) -> (r: usize)
    ensures
        text.can_slice_at(r as int),
        r <= offset,
        forall|j: int| r < j <= offset && j <= text.bytes@.len() ==> !boundary(text.bytes@, j),
{
    let ghost asked = offset;
    let mut offset = if offset < text.len() {
        offset
    } else {
        text.len()
    };
    while !text.is_char_boundary(offset)
        invariant
            offset <= text.bytes@.len(),
            offset <= asked,
            forall|j: int| offset < j <= asked && j <= text.bytes@.len() ==> !boundary(
                text.bytes@,
                j,
            ),
        decreases offset,
    {
        offset = offset - 1;
    }
    offset
}

/// `past`, crates/tui/src/state/vi.rs, without the marker case, which reads a span the input
/// records and does not move the offsets checked here.
pub fn past(input: &Text, at: usize) -> (r: usize)
    requires
        input.can_slice_at(at as int),
    ensures
        input.can_slice_at(r as int),
        r >= at,
{
    // A Vec's length is a usize, which bounds the sum.
    let _ = input.len();
    at + input.next_char_len(at)
}

/// `marked_out`, crates/tui/src/state/vi.rs: the stretch a VISUAL selection covers, as the whole
/// rows it crosses where `rows` says so. Every offset it slices the input at is one it may slice
/// at, and so is each end of what it returns, whatever the anchor holds. The caret is held inside
/// the input by every edit, which the requires clause states.
pub fn marked_out(input: &Text, caret: usize, anchor: usize, rows: bool) -> (r: (usize, usize))
    requires
        input.can_slice_at(caret as int),
    ensures
        input.can_slice_at(r.0 as int),
        input.can_slice_at(r.1 as int),
        r.0 <= r.1,
{
    #[cfg(before_e0cd1903)]
    let anchor = anchor;
    #[cfg(not(before_e0cd1903))]
    let anchor = boundary_at_or_before(input, anchor);
    let (from, to) = if anchor < caret {
        (anchor, caret)
    } else {
        (caret, anchor)
    };
    if rows {
        let starts = match input.rfind_newline_before(from) {
            Some(at) => at + 1,
            None => 0,
        };
        let _ = input.len();
        let ends = match input.find_newline_after(to) {
            Some(at) => to + at,
            None => input.len(),
        };
        return (starts, ends);
    }
    (from, past(input, to))
}

// 20ce6086: a turn's cache figures are summed over its requests, and a gateway may state the
// largest number for either. `+=` overflows: a panic in a debug build, and a wrapped total near
// zero in a release build, which sets no overflow-checks.

pub struct Cached {
    pub read_tokens: u64,
    pub written_tokens: u64,
}

pub open spec fn saturated(a: u64, b: u64) -> u64 {
    if a + b > u64::MAX {
        u64::MAX
    } else {
        (a + b) as u64
    }
}

impl Cached {
    /// `Cached::add`, crates/aichat/src/protocol.rs. Each total is the sum, or the largest number
    /// where the sum is past it.
    pub fn add(&mut self, other: Cached)
        ensures
            final(self).read_tokens == saturated(old(self).read_tokens, other.read_tokens),
            final(self).written_tokens == saturated(
                old(self).written_tokens,
                other.written_tokens,
            ),
    {
        #[cfg(before_20ce6086)]
        {
            self.read_tokens += other.read_tokens;
            self.written_tokens += other.written_tokens;
        }
        #[cfg(not(before_20ce6086))]
        {
            self.read_tokens = self.read_tokens.saturating_add(other.read_tokens);
            self.written_tokens = self.written_tokens.saturating_add(other.written_tokens);
        }
    }
}

fn main() {
}

} // verus!
