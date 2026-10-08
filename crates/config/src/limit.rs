//! The figure a session spend limit is written as.
//!
//! One reader for the `limit` setting and the `/limit` command, so a number means the same thing
//! in a settings file and at the prompt.

/// Read a token count: whole digits, optionally followed by `k` (thousands) or `m` (millions).
///
/// `None` for anything else, and for zero: a limit of nothing would stop the first request, and
/// "no limit" has its own word at the command.
pub fn parse_tokens(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, scale) = match text.chars().last()? {
        'k' | 'K' => (&text[..text.len() - 1], 1_000),
        'm' | 'M' => (&text[..text.len() - 1], 1_000_000),
        _ => (text, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits
        .parse::<u64>()
        .ok()?
        .checked_mul(scale)
        .filter(|tokens| *tokens > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_whole_digits_with_an_optional_scale() {
        assert_eq!(parse_tokens("500000"), Some(500_000));
        assert_eq!(parse_tokens(" 500k "), Some(500_000));
        assert_eq!(parse_tokens("2M"), Some(2_000_000));
    }

    #[test]
    fn anything_else_is_not_a_count() {
        for text in [
            "", "k", "0", "0k", "-5", "1.5m", "1,000", "12x", "off", "1e6", "+5",
        ] {
            assert_eq!(parse_tokens(text), None, "{text:?}");
        }
        assert_eq!(parse_tokens("18446744073709551615m"), None);
    }
}
