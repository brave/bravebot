//! The colour codes in output, which are counted instead of drawn.
//!
//! The removal is [`bravebot_core::colour_codes`], shared with the text a result puts in front of
//! the planner. This is the sentence the screen draws under text that had codes taken out
//! (VIEW-30).

use bravebot_i18n::t;

pub use bravebot_core::colour_codes::without;

/// The sentence drawn under text that had colour codes taken out, or nothing where none were.
pub fn note(dropped: usize) -> Option<String> {
    (dropped > 0).then(|| t!(colour_codes_not_drawn, count = dropped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_said_where_nothing_was_taken_out() {
        assert_eq!(note(0), None);
        assert!(note(1).expect("one").contains('1'));
        assert!(note(14).expect("many").contains("14"));
    }
}
