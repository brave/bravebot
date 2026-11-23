//! The window's one-time notice that crash reports are sent (CRASH-9).
//!
//! Whether reports are on was decided when this process started (`bravebot_backtrace::start`), so
//! nothing here reads a setting. The notice is recorded in the file the terminal's notice reads, so
//! a person told in one front end is not told again in the other.

use serde_json::{Value, json};

/// Whether the window should show the notice: this process is reporting and nobody has been told.
pub fn due() -> Value {
    json!({"due": bravebot_backtrace::notice_due()})
}

/// Record that the window showed the notice, and answer whether a record now exists. A process that
/// is not reporting records nothing and answers `false`.
pub fn mark_shown() -> Value {
    bravebot_backtrace::notice_given();
    json!({"shown": bravebot_backtrace::notice_recorded()})
}
