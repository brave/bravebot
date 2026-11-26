//! The time a background session waits for a prompt before it ends (BG-13), as a test can shorten
//! it for one process.
//!
//! The wait is an hour, a constant of the host. A test that runs the real `__bg-host` process to its
//! idle end cannot wait an hour, so a build with the `test-hosts` feature reads one variable that
//! names a shorter wait in milliseconds. A build without the feature holds no code that reads it, and
//! a release build with the feature does not compile (the refusal at the top of this crate), so the
//! variable is a setting of no build that ships. The value is a number a test chose and is read by
//! nothing that decides from a byte of a file, a reply or a page.

use std::time::Duration;

/// The variable that names the wait, in milliseconds, in a build with the `test-hosts` feature.
#[cfg(all(feature = "test-hosts", debug_assertions))]
pub const IDLE_MILLIS: &str = "BRAVEBOT_TEST_BG_IDLE_MILLIS";

/// The wait [`IDLE_MILLIS`] names, or `None` where the build has no such seam, the variable is
/// unset, or its value is not a positive whole number of milliseconds.
pub fn shortened_for_a_test() -> Option<Duration> {
    #[cfg(all(feature = "test-hosts", debug_assertions))]
    {
        std::env::var(IDLE_MILLIS)
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .filter(|millis| *millis > 0)
            .map(Duration::from_millis)
    }
    #[cfg(not(all(feature = "test-hosts", debug_assertions)))]
    {
        None
    }
}
