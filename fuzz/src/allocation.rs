//! Counting what a function allocates, so a target can hold a decoder to allocating for the bytes
//! it was given.
//!
//! A target that wants the count registers [`Counting`] as its global allocator and checks with
//! [`counting`] that it did. The other targets do not register it, so they run on the allocator
//! they always did. The count is the calling thread's, which leaves out the fuzzer's own threads
//! and the other tests of a `cargo test` run.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // `const` and with no destructor, so reaching them allocates nothing and registers nothing,
    // which an allocator cannot do.
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn signed(bytes: usize) -> isize {
    isize::try_from(bytes).unwrap_or(isize::MAX)
}

fn grow(bytes: usize) {
    let live = LIVE.get().saturating_add(signed(bytes));
    LIVE.set(live);
    if live > PEAK.get() {
        PEAK.set(live);
    }
}

fn shrink(bytes: usize) {
    LIVE.set(LIVE.get().saturating_sub(signed(bytes)));
}

/// The system allocator, counting the bytes this thread holds.
pub struct Counting;

#[allow(
    unsafe_code,
    reason = "GlobalAlloc is an unsafe trait, and every method here hands its arguments unchanged to the system allocator, which is the contract's own caller"
)]
// SAFETY: each method passes its arguments to `System` exactly as it received them and returns what
// `System` returned, so `System`'s guarantees are this implementation's. The count it keeps
// touches no memory the caller owns.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller meets `GlobalAlloc::alloc`'s contract for `layout`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            grow(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller meets `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            grow(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller meets `GlobalAlloc::dealloc`'s contract for `pointer` and `layout`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { System.dealloc(pointer, layout) };
        shrink(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller meets `GlobalAlloc::realloc`'s contract for its arguments.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            if new_size >= layout.size() {
                grow(new_size - layout.size());
            } else {
                shrink(layout.size() - new_size);
            }
        }
        moved
    }
}

/// `f`'s result and the most bytes it held at once beyond what the thread held when it began. It
/// counts only where [`Counting`] is the global allocator, and a call inside `f` to this function
/// starts the count again.
pub fn peak_growth<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let start = LIVE.get();
    PEAK.set(start);
    let value = f();
    (value, usize::try_from(PEAK.get() - start).unwrap_or(0))
}

/// Whether [`Counting`] is the global allocator. Where it is not, [`peak_growth`] says zero, and a
/// bound on it holds for any input.
pub fn counting() -> bool {
    let ((), peak) = peak_growth(|| drop(std::hint::black_box(Vec::<u8>::with_capacity(4096))));
    peak >= 4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static COUNTED: Counting = Counting;

    const KIB: usize = 1 << 10;
    const MIB: usize = 1 << 20;

    /// A buffer the optimizer has to make, since one that is never read can be left out.
    fn held(capacity: usize) -> Vec<u8> {
        std::hint::black_box(Vec::with_capacity(capacity))
    }

    /// A target compares the peak with a bound, so a buffer has to be counted at its whole size,
    /// and still counted when it is gone by the time the function returns.
    #[test]
    fn a_buffer_is_counted_at_its_size() {
        let ((), peak) = peak_growth(|| drop(held(MIB)));
        assert!((MIB..MIB + 64 * KIB).contains(&peak), "{peak}");
    }

    /// A decoder that reads a claimed length into a zeroed buffer allocates it the way
    /// `vec![0; n]` does, which is a different call of the allocator from `with_capacity`.
    #[test]
    fn a_zeroed_buffer_is_counted_at_its_size() {
        let ((), peak) = peak_growth(|| drop(std::hint::black_box(vec![0u8; MIB])));
        assert!((MIB..MIB + 64 * KIB).contains(&peak), "{peak}");
    }

    /// Two buffers one after the other are one buffer's worth at once, and a count of everything
    /// ever allocated would read two.
    #[test]
    fn buffers_held_one_after_another_are_not_added() {
        let ((), peak) = peak_growth(|| {
            drop(held(MIB));
            drop(held(MIB));
        });
        assert!((MIB..2 * MIB).contains(&peak), "{peak}");
    }

    /// What a decoder already held when it was handed a stream is not what the stream made it
    /// allocate, and freeing it cannot make the count negative.
    #[test]
    fn what_was_held_before_is_left_out_and_freeing_it_counts_nothing() {
        let before = held(4 * MIB);
        let ((), peak) = peak_growth(|| drop(before));
        assert_eq!(peak, 0);
        let before = held(4 * MIB);
        let ((), peak) = peak_growth(|| {
            let small = held(100);
            drop(before);
            drop(small);
        });
        assert!((100..64 * KIB).contains(&peak), "{peak}");
    }

    /// A buffer that grows is how a decoder holds a stream that arrives in pieces. The growth is
    /// what is added to the buffer, so a count that adds the whole new size counts the old size
    /// twice.
    #[test]
    fn a_buffer_that_grows_is_counted_at_its_new_size() {
        let ((), peak) = peak_growth(|| {
            let mut buffer = held(256 * KIB);
            buffer.reserve_exact(MIB);
            drop(std::hint::black_box(buffer));
        });
        assert!((MIB..MIB + 64 * KIB).contains(&peak), "{peak}");
    }

    /// A buffer that shrinks gives back what it no longer holds, so the next one is counted from
    /// there. A count that ignores the shrink reads the two together.
    #[test]
    fn a_buffer_that_shrinks_gives_back_what_it_no_longer_holds() {
        let ((), peak) = peak_growth(|| {
            let mut first = held(MIB);
            first.push(1);
            first.shrink_to_fit();
            let second = held(MIB + MIB / 2);
            drop(std::hint::black_box((first, second)));
        });
        assert!((MIB + MIB / 2..2 * MIB).contains(&peak), "{peak}");
    }

    /// The fuzzer has threads of its own, and a `cargo test` run has the other tests. A count of
    /// all of them would move with what they do.
    #[test]
    fn another_thread_allocating_is_not_counted() {
        let (length, peak) = peak_growth(|| {
            std::thread::spawn(|| held(4 * MIB).capacity())
                .join()
                .expect("the thread finishes")
        });
        assert!(length >= 4 * MIB, "{length}");
        assert!(peak < MIB, "{peak}");
    }

    /// A target relies on this to know that a small peak means a small allocation.
    #[test]
    fn counting_says_so_where_the_allocator_is_registered() {
        assert!(counting());
    }
}
