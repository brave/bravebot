//! Where a page of a bounded answer stops, from byte counts alone.
//!
//! A page of `read_git`'s log with messages holds at most a fixed number of bytes
//! ([GIT-12](../../../docs/specs/tools/read-git.md#GIT-12)). The counts added up here are the
//! lengths of lines read from a file, so the sum is worked out where Verus proves it never wraps
//! (docs/development/verification.md). Which lines are counted, and what is done with the answer,
//! is the caller's.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The bytes a page holds once `more` are added to the `used` it holds, where that is at most
/// `most`, and `None` where it would pass `most`. The sum is never worked out where it could pass
/// `most`, so it cannot wrap whatever the counts are.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r is Some <==> used + more <= most,
        r is Some ==> r->Some_0 == used + more,
))]
pub fn within(used: usize, more: usize, most: usize) -> Option<usize> {
    if used > most || more > most - used {
        return None;
    }
    Some(used + more)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page may be filled to its last byte, and not one past it.
    #[test]
    fn a_page_holds_up_to_its_bound_and_not_past_it() {
        assert_eq!(within(0, 0, 0), Some(0));
        assert_eq!(within(19_990, 10, 20_000), Some(20_000));
        assert_eq!(within(19_990, 11, 20_000), None);
        assert_eq!(within(20_001, 0, 20_000), None);
    }

    /// Counts whose sum is past what a `usize` holds are refused rather than wrapped to a small
    /// number that would read as fitting.
    #[test]
    fn counts_whose_sum_would_wrap_do_not_fit() {
        assert_eq!(within(usize::MAX, 1, usize::MAX), None);
        assert_eq!(within(1, usize::MAX, usize::MAX), None);
        assert_eq!(within(usize::MAX, 1, 20_000), None);
        assert_eq!(within(2, usize::MAX - 1, 20_000), None);
    }
}
