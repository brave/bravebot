//! The line on the startup screen that says crash reports are sent, said once (CRASH-9).
//!
//! What it says is a catalog message with no argument, so nothing a report or a response holds is
//! in it. Whether it is due is the answer of `bravebot-backtrace`, which says so only for a launch
//! that is reporting and has not said it before, so a person who switched reports off is told
//! nothing about them.

use bravebot_i18n::t;

/// The line to say now, or nothing. Saying it records that it was said, so no later launch does.
///
/// Recorded here, as the line is made, because the caller adds it to the screen at once. A launch
/// that ended before the first frame was drawn has still said it as far as this can tell, and the
/// alternative is a line shown twice.
pub fn at_startup() -> Option<String> {
    if !bravebot_backtrace::notice_due() {
        return None;
    }
    bravebot_backtrace::notice_given();
    Some(t!(crash_reports_notice).to_string())
}
